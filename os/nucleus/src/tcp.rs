//! TCP — a real transport on top of the Praxis IP stack.
//!
//! The connection state machine is written as **pure logic**: every entry point
//! takes the local/remote addresses and returns the segments to emit, touching
//! no device. That makes the protocol unit-testable by handshaking two [`Tcp`]
//! engines directly (see the tests), and lets [`crate::net::NetStack`] own the
//! wire plumbing (ARP, IP framing, the NIC).
//!
//! It implements the parts a connection actually needs on a real, lossy wire:
//! the three-way handshake (SYN / SYN-ACK / ACK), in-order data transfer with
//! cumulative acknowledgements, the FIN close with a lingering **TIME-WAIT**,
//! **retransmission** with exponential backoff, **flow control** (a real
//! advertised receive window), and **congestion control** (Reno slow-start /
//! congestion-avoidance with multiplicative decrease on loss).

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::vec::Vec;

use crate::net::{checksum16, Ipv4Addr, IP_PROTO_TCP};

pub const FIN: u8 = 0x01;
pub const SYN: u8 = 0x02;
pub const RST: u8 = 0x04;
pub const PSH: u8 = 0x08;
pub const ACK: u8 = 0x10;

/// The receive buffer per connection, in bytes — also the largest window we
/// ever advertise. A real socket buffer, bounding memory per connection and
/// giving the peer a true flow-control signal instead of a fixed lie.
const RCV_BUF_MAX: u16 = 0xffff;

/// The TCP connection states this transport moves through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    SynSent,
    SynRcvd,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    LastAck,
    /// After an active close completes, linger here to absorb a peer's
    /// retransmitted FIN before the tuple is reused (2·MSL in real TCP).
    TimeWait,
    Closed,
}

impl TcpState {
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            TcpState::SynSent => "SYN-SENT",
            TcpState::SynRcvd => "SYN-RCVD",
            TcpState::Established => "ESTABLISHED",
            TcpState::FinWait1 => "FIN-WAIT-1",
            TcpState::FinWait2 => "FIN-WAIT-2",
            TcpState::CloseWait => "CLOSE-WAIT",
            TcpState::LastAck => "LAST-ACK",
            TcpState::TimeWait => "TIME-WAIT",
            TcpState::Closed => "CLOSED",
        }
    }
}

/// Retransmission timing: first retry after [`RTO_BASE_TICKS`], doubling per
/// retry (50, 100, 200, … ticks — 0.5 s, 1 s, 2 s… on the 100 Hz PIT).
const RTO_BASE_TICKS: u64 = 50;
/// A segment retried this many times without an ACK abandons the connection.
const MAX_RETRIES: u8 = 6;
/// How long TIME-WAIT lingers before the connection is reaped (2·MSL, scaled
/// to the tick clock).
const TIME_WAIT_TICKS: u64 = 200;

/// The maximum segment size we chunk into — also the congestion-window unit.
const MSS: u32 = 1200;
/// Initial congestion window (RFC 6928 initial window of ~10 segments) — large
/// enough that a small response never waits, small enough to still ramp.
const INIT_CWND: u32 = 10 * MSS;

/// One sent-but-unacknowledged segment, kept verbatim for retransmission.
struct Unacked {
    /// The sequence number just past this segment (`seq + len`, counting SYN
    /// and FIN as one each) — the segment is acked once `ack ≥ end_seq`.
    end_seq: u32,
    /// The exact bytes that went on the wire.
    bytes: Vec<u8>,
    sent_at: u64,
    retries: u8,
}

/// A transmission control block — one connection's bookkeeping.
struct Tcb {
    state: TcpState,
    local_port: u16,
    remote_ip: Ipv4Addr,
    remote_port: u16,
    /// Next sequence number we will send.
    snd_nxt: u32,
    /// Oldest unacknowledged sequence number.
    snd_una: u32,
    /// Next sequence number we expect to receive.
    rcv_nxt: u32,
    /// Delivered, not-yet-read payload (bounded by [`RCV_BUF_MAX`]).
    rx: VecDeque<u8>,
    /// The peer's most recently advertised receive window — the sender-side
    /// flow-control limit: we never put more than this many unacknowledged
    /// bytes on the wire.
    snd_wnd: u16,
    /// Congestion window (bytes) — the *network's* limit on in-flight data,
    /// grown by slow-start / congestion-avoidance and cut on loss. The sender
    /// is bounded by `min(snd_wnd, cwnd)`: flow control protects the receiver,
    /// congestion control protects the path.
    cwnd: u32,
    /// Slow-start threshold: below it cwnd grows by a full MSS per ACK
    /// (exponential); at/above it, by ~MSS per RTT (linear).
    ssthresh: u32,
    /// The tick TIME-WAIT began, for reaping.
    time_wait_since: u64,
    /// Sent, not-yet-acknowledged segments — the retransmission queue.
    send_q: Vec<Unacked>,
}

impl Tcb {
    /// The window we advertise right now: free space left in the receive
    /// buffer. Shrinks as unread data accumulates, reaches 0 when full (the
    /// peer must then stop), and reopens as the app drains it.
    fn advertised_window(&self) -> u16 {
        (RCV_BUF_MAX as usize).saturating_sub(self.rx.len()) as u16
    }

    /// Bytes we may still put in flight right now: the smaller of the peer's
    /// receive window and the congestion window, minus what is already unacked.
    fn usable_send(&self) -> u32 {
        let in_flight = self.snd_nxt.wrapping_sub(self.snd_una);
        let limit = u32::from(self.snd_wnd).min(self.cwnd);
        limit.saturating_sub(in_flight)
    }

    /// An ACK advanced `snd_una` — grow the congestion window: exponentially in
    /// slow start (cwnd < ssthresh), linearly in congestion avoidance.
    fn on_ack_progress(&mut self) {
        if self.cwnd < self.ssthresh {
            self.cwnd = self.cwnd.saturating_add(MSS); // slow start
        } else {
            // Congestion avoidance: +MSS per window of data acked.
            self.cwnd = self.cwnd.saturating_add(MSS * MSS / self.cwnd.max(1));
        }
    }

    /// A retransmission fired — treat it as congestion: halve the threshold
    /// and collapse the window back to slow-start (Reno's multiplicative
    /// decrease).
    fn on_loss(&mut self) {
        self.ssthresh = (self.cwnd / 2).max(2 * MSS);
        self.cwnd = INIT_CWND.min(self.ssthresh).max(MSS);
    }
}

/// Is `a` strictly after `b` in wrapping sequence space?
fn seq_after(a: u32, b: u32) -> bool {
    (a.wrapping_sub(b) as i32) > 0
}

/// Segments to emit toward a peer: `(destination IP, TCP segment bytes)`.
#[derive(Debug, Default)]
pub struct TcpOut(pub Vec<(Ipv4Addr, Vec<u8>)>);

/// The TCP engine: the connection table, the passive-open listeners, and the
/// accept queue. Pure — it never touches a device.
#[derive(Default)]
pub struct Tcp {
    conns: BTreeMap<usize, Tcb>,
    listeners: BTreeSet<u16>,
    accept_q: BTreeMap<u16, VecDeque<usize>>,
    next_id: usize,
    isn: u32,
    ephemeral: u16,
    /// Last clock value seen by [`Tcp::on_tick`]; stamps outgoing segments.
    now: u64,
}

impl Tcp {
    #[must_use]
    pub fn new() -> Self {
        Self {
            isn: 0x1000,
            ephemeral: 49152,
            ..Default::default()
        }
    }

    fn next_isn(&mut self) -> u32 {
        self.isn = self.isn.wrapping_add(64_000).wrapping_add(1);
        self.isn
    }

    fn next_ephemeral(&mut self) -> u16 {
        let p = self.ephemeral;
        self.ephemeral = if self.ephemeral >= 60000 {
            49152
        } else {
            self.ephemeral + 1
        };
        p
    }

    fn find(&self, local_port: u16, remote_ip: Ipv4Addr, remote_port: u16) -> Option<usize> {
        self.conns.iter().find_map(|(id, t)| {
            (t.local_port == local_port && t.remote_ip == remote_ip && t.remote_port == remote_port)
                .then_some(*id)
        })
    }

    /// Passively open `port`: accept inbound connections to it.
    pub fn listen(&mut self, port: u16) {
        self.listeners.insert(port);
        self.accept_q.entry(port).or_default();
    }

    #[must_use]
    pub fn is_listening(&self, port: u16) -> bool {
        self.listeners.contains(&port)
    }

    #[must_use]
    pub fn conn_count(&self) -> usize {
        self.conns.len()
    }

    #[must_use]
    pub fn listener_count(&self) -> usize {
        self.listeners.len()
    }

    /// Actively open a connection to `remote_ip:remote_port`. Returns the new
    /// connection id and the SYN to emit.
    pub fn connect(
        &mut self,
        my_ip: Ipv4Addr,
        remote_ip: Ipv4Addr,
        remote_port: u16,
    ) -> (usize, TcpOut) {
        let id = self.next_id;
        self.next_id += 1;
        let iss = self.next_isn();
        let local_port = self.next_ephemeral();
        let seg = build_segment(
            local_port,
            remote_port,
            iss,
            0,
            SYN,
            RCV_BUF_MAX,
            my_ip,
            remote_ip,
            &[],
        );
        self.conns.insert(
            id,
            Tcb {
                state: TcpState::SynSent,
                local_port,
                remote_ip,
                remote_port,
                snd_nxt: iss.wrapping_add(1),
                snd_una: iss,
                rcv_nxt: 0,
                rx: VecDeque::new(),
                snd_wnd: 0, // unknown until the SYN-ACK advertises one
                cwnd: INIT_CWND,
                ssthresh: RCV_BUF_MAX as u32,
                time_wait_since: 0,
                send_q: alloc::vec![Unacked {
                    end_seq: iss.wrapping_add(1), // SYN occupies one seq
                    bytes: seg.clone(),
                    sent_at: self.now,
                    retries: 0,
                }],
            },
        );
        (id, TcpOut(alloc::vec![(remote_ip, seg)]))
    }

    /// Send `data` on an established connection. `CloseWait` is also sendable:
    /// the peer half-closed (it will send no more) but is still reading — an
    /// HTTP client that shuts down its write side after the request still
    /// expects the response.
    ///
    /// **Flow- and congestion-controlled**: the send is refused (`Err`) when it
    /// would push more than `min(peer window, congestion window)` bytes into
    /// flight — a zero peer window (the receiver said stop) or a collapsed
    /// congestion window (the path is loss-signalling) both block it. The
    /// initial congestion window is ~10 segments, so a small response never
    /// waits; a bulk transfer ramps through slow start. The caller retries
    /// after polling (as httpd and fetch already do).
    pub fn send(&mut self, my_ip: Ipv4Addr, id: usize, data: &[u8]) -> Result<TcpOut, ()> {
        let tcb = self.conns.get_mut(&id).ok_or(())?;
        if tcb.state != TcpState::Established && tcb.state != TcpState::CloseWait {
            return Err(());
        }
        // Never exceed the smaller of the receiver's window and the path's
        // congestion window.
        if !data.is_empty() && data.len() as u32 > tcb.usable_send() {
            return Err(());
        }
        let seq = tcb.snd_nxt;
        let ack = tcb.rcv_nxt;
        tcb.snd_nxt = tcb.snd_nxt.wrapping_add(data.len() as u32);
        let window = tcb.advertised_window();
        let seg = build_segment(
            tcb.local_port,
            tcb.remote_port,
            seq,
            ack,
            PSH | ACK,
            window,
            my_ip,
            tcb.remote_ip,
            data,
        );
        tcb.send_q.push(Unacked {
            end_seq: seq.wrapping_add(data.len() as u32),
            bytes: seg.clone(),
            sent_at: self.now,
            retries: 0,
        });
        Ok(TcpOut(alloc::vec![(tcb.remote_ip, seg)]))
    }

    /// Read (and consume) received data on a connection.
    pub fn recv(&mut self, id: usize) -> Vec<u8> {
        match self.conns.get_mut(&id) {
            Some(tcb) => tcb.rx.drain(..).collect(),
            None => Vec::new(),
        }
    }

    /// A window-update ACK for `id`, if draining its receive buffer has
    /// reopened a window that was (near) closed. [`NetStack`] emits this after
    /// [`Tcp::recv`] so a peer stopped by a full window learns it may resume.
    /// Returns nothing when there is no established peer or the window was
    /// already open.
    pub fn window_update(&mut self, my_ip: Ipv4Addr, id: usize) -> TcpOut {
        let Some(tcb) = self.conns.get(&id) else {
            return TcpOut::default();
        };
        if !matches!(tcb.state, TcpState::Established | TcpState::CloseWait) {
            return TcpOut::default();
        }
        TcpOut(alloc::vec![(tcb.remote_ip, ack_of(tcb, my_ip))])
    }

    /// Begin an active close (send FIN).
    pub fn close(&mut self, my_ip: Ipv4Addr, id: usize) -> TcpOut {
        let Some(tcb) = self.conns.get_mut(&id) else {
            return TcpOut::default();
        };
        let next = match tcb.state {
            TcpState::Established => TcpState::FinWait1,
            TcpState::CloseWait => TcpState::LastAck,
            _ => return TcpOut::default(),
        };
        let seq = tcb.snd_nxt;
        let ack = tcb.rcv_nxt;
        tcb.snd_nxt = tcb.snd_nxt.wrapping_add(1);
        tcb.state = next;
        let window = tcb.advertised_window();
        let seg = build_segment(
            tcb.local_port,
            tcb.remote_port,
            seq,
            ack,
            FIN | ACK,
            window,
            my_ip,
            tcb.remote_ip,
            &[],
        );
        tcb.send_q.push(Unacked {
            end_seq: seq.wrapping_add(1), // FIN occupies one seq
            bytes: seg.clone(),
            sent_at: self.now,
            retries: 0,
        });
        TcpOut(alloc::vec![(tcb.remote_ip, seg)])
    }

    /// Drive retransmission from the platform clock: any queued segment whose
    /// retransmission timeout has elapsed is re-emitted verbatim, with the
    /// timeout doubling per retry. A segment that exhausts [`MAX_RETRIES`]
    /// abandons its connection (state → `Closed`) — the wire is gone; better
    /// an honest dead connection than an immortal zombie. Call this from the
    /// idle loop; returns the segments to put back on the wire.
    pub fn on_tick(&mut self, now: u64) -> TcpOut {
        self.now = now;
        let mut out = TcpOut::default();
        for tcb in self.conns.values_mut() {
            // Reap a lingering TIME-WAIT connection once 2·MSL has elapsed.
            if tcb.state == TcpState::TimeWait {
                if now.saturating_sub(tcb.time_wait_since) >= TIME_WAIT_TICKS {
                    tcb.state = TcpState::Closed;
                }
                continue;
            }
            if tcb.state == TcpState::Closed {
                tcb.send_q.clear();
                continue;
            }
            let mut give_up = false;
            let mut retransmitted = false;
            for unacked in &mut tcb.send_q {
                let rto = RTO_BASE_TICKS << unacked.retries;
                if now.saturating_sub(unacked.sent_at) < rto {
                    continue;
                }
                if unacked.retries >= MAX_RETRIES {
                    give_up = true;
                    break;
                }
                unacked.retries += 1;
                unacked.sent_at = now;
                retransmitted = true;
                out.0.push((tcb.remote_ip, unacked.bytes.clone()));
            }
            // A timeout is the classic congestion signal: back the window off.
            if retransmitted {
                tcb.on_loss();
            }
            if give_up {
                tcb.state = TcpState::Closed;
                tcb.send_q.clear();
            }
        }
        out
    }

    #[must_use]
    pub fn state(&self, id: usize) -> Option<TcpState> {
        self.conns.get(&id).map(|t| t.state)
    }

    /// Pop the next accepted connection on a listening port.
    pub fn accept(&mut self, port: u16) -> Option<usize> {
        self.accept_q.get_mut(&port).and_then(|q| q.pop_front())
    }

    /// List `(id, state, remote_ip, remote_port)` for the shell.
    #[must_use]
    pub fn table(&self) -> Vec<(usize, TcpState, Ipv4Addr, u16)> {
        self.conns
            .iter()
            .map(|(id, t)| (*id, t.state, t.remote_ip, t.remote_port))
            .collect()
    }

    /// Feed one received TCP segment (already stripped of IP). Returns any
    /// segments to emit in response. This is the state machine.
    pub fn on_segment(&mut self, my_ip: Ipv4Addr, remote_ip: Ipv4Addr, bytes: &[u8]) -> TcpOut {
        let Some(seg) = Segment::parse(bytes) else {
            return TcpOut::default();
        };
        let mut out = TcpOut::default();
        let local_port = seg.dst_port;
        let now = self.now; // captured before borrowing self.conns

        if let Some(id) = self.find(local_port, remote_ip, seg.src_port) {
            let mut newly_accepted = None;
            {
                let tcb = self.conns.get_mut(&id).expect("conn exists");
                // Track the peer's advertised receive window on every segment —
                // this is the sender-side flow-control limit.
                tcb.snd_wnd = seg.window;
                // Any ACK retires the segments it covers from the
                // retransmission queue — fully-acked means `ack ≥ end_seq`.
                if seg.flags & ACK != 0 {
                    let before = tcb.send_q.len();
                    tcb.send_q
                        .retain(|unacked| seq_after(unacked.end_seq, seg.ack));
                    // If this ACK cleared at least one segment, it made real
                    // forward progress — open the congestion window.
                    if tcb.send_q.len() < before {
                        tcb.on_ack_progress();
                    }
                }
                match tcb.state {
                    TcpState::SynSent => {
                        if seg.flags & (SYN | ACK) == (SYN | ACK) && seg.ack == tcb.snd_nxt {
                            tcb.rcv_nxt = seg.seq.wrapping_add(1);
                            tcb.snd_una = seg.ack;
                            tcb.state = TcpState::Established;
                            out.0.push((remote_ip, ack_of(tcb, my_ip)));
                        }
                    }
                    TcpState::SynRcvd => {
                        if seg.flags & ACK != 0 && seg.ack == tcb.snd_nxt {
                            tcb.snd_una = seg.ack;
                            tcb.state = TcpState::Established;
                            newly_accepted = Some(local_port);
                        }
                    }
                    TcpState::Established => {
                        deliver(tcb, &seg, &mut out, remote_ip, my_ip);
                        if seg.flags & FIN != 0 && seg.seq == tcb.rcv_nxt {
                            tcb.rcv_nxt = tcb.rcv_nxt.wrapping_add(1);
                            tcb.state = TcpState::CloseWait;
                            out.0.push((remote_ip, ack_of(tcb, my_ip)));
                        } else if seg.flags & SYN != 0 {
                            // The peer is retransmitting its SYN-ACK — our
                            // final handshake ACK was lost. Re-ACK to resync.
                            out.0.push((remote_ip, ack_of(tcb, my_ip)));
                        }
                    }
                    TcpState::FinWait1 => {
                        if seg.flags & ACK != 0 {
                            tcb.snd_una = seg.ack;
                            tcb.state = TcpState::FinWait2;
                        }
                        if seg.flags & FIN != 0 {
                            // Simultaneous/last FIN: ACK it and linger in
                            // TIME-WAIT to absorb any retransmission.
                            tcb.rcv_nxt = tcb.rcv_nxt.wrapping_add(1);
                            tcb.state = TcpState::TimeWait;
                            tcb.time_wait_since = now;
                            out.0.push((remote_ip, ack_of(tcb, my_ip)));
                        }
                    }
                    TcpState::FinWait2 => {
                        if seg.flags & FIN != 0 {
                            tcb.rcv_nxt = tcb.rcv_nxt.wrapping_add(1);
                            tcb.state = TcpState::TimeWait;
                            tcb.time_wait_since = now;
                            out.0.push((remote_ip, ack_of(tcb, my_ip)));
                        }
                    }
                    TcpState::TimeWait => {
                        if seg.flags & FIN != 0 {
                            // The peer retransmitted its FIN (our ACK was lost).
                            // Re-ACK; TIME-WAIT is exactly for absorbing this.
                            out.0.push((remote_ip, ack_of(tcb, my_ip)));
                        }
                    }
                    TcpState::LastAck => {
                        if seg.flags & ACK != 0 {
                            tcb.state = TcpState::Closed;
                        }
                    }
                    TcpState::CloseWait => {
                        if seg.flags & FIN != 0 {
                            // The peer is retransmitting its FIN — our ACK of
                            // it was lost. Re-ACK so it can finish closing.
                            out.0.push((remote_ip, ack_of(tcb, my_ip)));
                        }
                    }
                    TcpState::Closed => {}
                }
            }
            if let Some(port) = newly_accepted {
                self.accept_q.entry(port).or_default().push_back(id);
            }
        } else if seg.flags & SYN != 0
            && seg.flags & ACK == 0
            && self.listeners.contains(&local_port)
        {
            // Passive open: a SYN to a listening port.
            let id = self.next_id;
            self.next_id += 1;
            let iss = self.next_isn();
            let rcv_nxt = seg.seq.wrapping_add(1);
            let synack = build_segment(
                local_port,
                seg.src_port,
                iss,
                rcv_nxt,
                SYN | ACK,
                RCV_BUF_MAX,
                my_ip,
                remote_ip,
                &[],
            );
            self.conns.insert(
                id,
                Tcb {
                    state: TcpState::SynRcvd,
                    local_port,
                    remote_ip,
                    remote_port: seg.src_port,
                    snd_nxt: iss.wrapping_add(1),
                    snd_una: iss,
                    rcv_nxt,
                    rx: VecDeque::new(),
                    snd_wnd: seg.window, // the peer told us its window in the SYN
                    cwnd: INIT_CWND,
                    ssthresh: RCV_BUF_MAX as u32,
                    time_wait_since: 0,
                    send_q: alloc::vec![Unacked {
                        end_seq: iss.wrapping_add(1), // SYN-ACK's SYN takes one seq
                        bytes: synack.clone(),
                        sent_at: self.now,
                        retries: 0,
                    }],
                },
            );
            out.0.push((remote_ip, synack));
        }
        out
    }
}

/// Append in-order payload and acknowledge it; also track the peer's ACK.
fn deliver(tcb: &mut Tcb, seg: &Segment, out: &mut TcpOut, remote_ip: Ipv4Addr, my_ip: Ipv4Addr) {
    if seg.flags & ACK != 0 {
        tcb.snd_una = seg.ack;
    }
    if !seg.payload.is_empty() {
        if seg.seq == tcb.rcv_nxt {
            // Accept only what fits the receive buffer; the ACK we send back
            // advertises the now-smaller window, so the peer won't overrun us.
            let space = (RCV_BUF_MAX as usize).saturating_sub(tcb.rx.len());
            let take = seg.payload.len().min(space);
            tcb.rx.extend(seg.payload[..take].iter().copied());
            tcb.rcv_nxt = tcb.rcv_nxt.wrapping_add(take as u32);
            out.0.push((remote_ip, ack_of(tcb, my_ip)));
        } else {
            // Out-of-window data — almost always the peer retransmitting
            // because OUR ack got lost. Re-ACK to resync, or the peer keeps
            // retransmitting until it abandons a perfectly good connection.
            out.0.push((remote_ip, ack_of(tcb, my_ip)));
        }
    }
}

/// A bare ACK carrying our current sequence + acknowledgement numbers, and the
/// window we can currently accept (flow control rides on every ACK).
fn ack_of(tcb: &Tcb, my_ip: Ipv4Addr) -> Vec<u8> {
    build_segment(
        tcb.local_port,
        tcb.remote_port,
        tcb.snd_nxt,
        tcb.rcv_nxt,
        ACK,
        tcb.advertised_window(),
        my_ip,
        tcb.remote_ip,
        &[],
    )
}

// ── segment codec ────────────────────────────────────────────────────────────

struct Segment {
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    window: u16,
    payload: Vec<u8>,
}

impl Segment {
    fn parse(b: &[u8]) -> Option<Self> {
        if b.len() < 20 {
            return None;
        }
        let data_off = (b[12] >> 4) as usize * 4;
        if data_off < 20 || b.len() < data_off {
            return None;
        }
        Some(Self {
            src_port: u16::from_be_bytes([b[0], b[1]]),
            dst_port: u16::from_be_bytes([b[2], b[3]]),
            seq: u32::from_be_bytes([b[4], b[5], b[6], b[7]]),
            ack: u32::from_be_bytes([b[8], b[9], b[10], b[11]]),
            flags: b[13],
            window: u16::from_be_bytes([b[14], b[15]]),
            payload: b[data_off..].to_vec(),
        })
    }
}

#[allow(clippy::too_many_arguments)]
fn build_segment(
    src_port: u16,
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    window: u16,
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    payload: &[u8],
) -> Vec<u8> {
    let mut h = Vec::with_capacity(20 + payload.len());
    h.extend_from_slice(&src_port.to_be_bytes());
    h.extend_from_slice(&dst_port.to_be_bytes());
    h.extend_from_slice(&seq.to_be_bytes());
    h.extend_from_slice(&ack.to_be_bytes());
    h.push(5 << 4); // data offset: 5 32-bit words, no options
    h.push(flags);
    h.extend_from_slice(&window.to_be_bytes());
    h.extend_from_slice(&[0, 0]); // checksum placeholder
    h.extend_from_slice(&[0, 0]); // urgent pointer
    h.extend_from_slice(payload);
    let len = h.len();
    let pseudo = [
        src_ip[0],
        src_ip[1],
        src_ip[2],
        src_ip[3],
        dst_ip[0],
        dst_ip[1],
        dst_ip[2],
        dst_ip[3],
        0,
        IP_PROTO_TCP,
        (len >> 8) as u8,
        (len & 0xff) as u8,
    ];
    let csum = checksum16(&[&pseudo, &h]);
    h[16..18].copy_from_slice(&csum.to_be_bytes());
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: Ipv4Addr = [10, 0, 0, 1];
    const B: Ipv4Addr = [10, 0, 0, 2];

    /// Deliver every segment `from` produced to `to`, collecting `to`'s replies.
    fn pump(to: &mut Tcp, to_ip: Ipv4Addr, from_ip: Ipv4Addr, out: TcpOut) -> TcpOut {
        let mut replies = TcpOut::default();
        for (_dst, seg) in out.0 {
            let r = to.on_segment(to_ip, from_ip, &seg);
            replies.0.extend(r.0);
        }
        replies
    }

    #[test]
    fn three_way_handshake_and_data_and_close() {
        let mut client = Tcp::new();
        let mut server = Tcp::new();
        server.listen(80);

        // SYN → SYN-ACK → ACK.
        let (cid, syn) = client.connect(A, B, 80);
        let synack = pump(&mut server, B, A, syn);
        let ack = pump(&mut client, A, B, synack);
        let _ = pump(&mut server, B, A, ack);

        assert_eq!(client.state(cid), Some(TcpState::Established));
        let sid = server.accept(80).expect("server accepted");
        assert_eq!(server.state(sid), Some(TcpState::Established));

        // Client → server data (server ACKs).
        let data = client.send(A, cid, b"GET / HTTP/1.0").unwrap();
        let _ack = pump(&mut server, B, A, data);
        assert_eq!(server.recv(sid), b"GET / HTTP/1.0".to_vec());

        // Server → client data.
        let resp = server.send(B, sid, b"200 OK").unwrap();
        let _ack = pump(&mut client, A, B, resp);
        assert_eq!(client.recv(cid), b"200 OK".to_vec());

        // Active close from the client.
        let fin = client.close(A, cid);
        let server_reacts = pump(&mut server, B, A, fin); // ACK (+ maybe FIN)
        assert_eq!(server.state(sid), Some(TcpState::CloseWait));
        let _ = pump(&mut client, A, B, server_reacts);
        let sfin = server.close(B, sid);
        let client_ack = pump(&mut client, A, B, sfin);
        let _ = pump(&mut server, B, A, client_ack);
        // The passive closer reaches CLOSED on our ACK; the active closer
        // lingers in TIME-WAIT to absorb a retransmitted FIN…
        assert_eq!(server.state(sid), Some(TcpState::Closed));
        assert_eq!(client.state(cid), Some(TcpState::TimeWait));
        // …and is reaped to CLOSED once 2·MSL elapses on the clock.
        client.on_tick(TIME_WAIT_TICKS + 1);
        assert_eq!(client.state(cid), Some(TcpState::Closed));
    }

    #[test]
    fn unknown_segment_to_no_listener_is_ignored() {
        let mut t = Tcp::new();
        let stray = build_segment(1234, 80, 5, 0, SYN, RCV_BUF_MAX, A, B, &[]);
        assert!(t.on_segment(B, A, &stray).0.is_empty());
        assert_eq!(t.conn_count(), 0);
    }

    #[test]
    fn send_on_unopened_connection_errs() {
        let mut t = Tcp::new();
        assert!(t.send(A, 999, b"x").is_err());
    }

    // ── reliability: retransmission, backoff, resync ────────────────────

    /// Establish a connection between two fresh stacks; returns (client, cid,
    /// server, sid).
    fn established() -> (Tcp, usize, Tcp, usize) {
        let mut client = Tcp::new();
        let mut server = Tcp::new();
        server.listen(80);
        let (cid, syn) = client.connect(A, B, 80);
        let synack = pump(&mut server, B, A, syn);
        let ack = pump(&mut client, A, B, synack);
        let _ = pump(&mut server, B, A, ack);
        let sid = server.accept(80).expect("accepted");
        (client, cid, server, sid)
    }

    #[test]
    fn lost_data_is_retransmitted_and_arrives() {
        let (mut client, cid, mut server, sid) = established();

        // The wire eats the data segment: we simply never deliver it.
        let lost = client.send(A, cid, b"important payload").unwrap();
        drop(lost);
        assert_eq!(server.recv(sid), Vec::<u8>::new());

        // Before the RTO nothing is resent…
        assert!(client.on_tick(RTO_BASE_TICKS - 1).0.is_empty());
        // …at the RTO the exact segment reappears and completes delivery.
        let retrans = client.on_tick(RTO_BASE_TICKS);
        assert_eq!(retrans.0.len(), 1);
        let acks = pump(&mut server, B, A, retrans);
        assert_eq!(server.recv(sid), b"important payload".to_vec());

        // The ACK retires the queue: no further retransmissions, ever.
        let _ = pump(&mut client, A, B, acks);
        assert!(client.on_tick(1_000_000).0.is_empty());
    }

    #[test]
    fn lost_syn_is_retransmitted_verbatim() {
        let mut client = Tcp::new();
        let (cid, syn) = client.connect(A, B, 80);
        let original = syn.0[0].1.clone();
        drop(syn); // the wire ate the SYN

        let retrans = client.on_tick(RTO_BASE_TICKS);
        assert_eq!(retrans.0.len(), 1);
        assert_eq!(retrans.0[0].1, original, "SYN must resend byte-identical");
        assert_eq!(client.state(cid), Some(TcpState::SynSent));
    }

    #[test]
    fn backoff_doubles_and_exhaustion_closes_the_connection() {
        let mut client = Tcp::new();
        let (cid, _syn) = client.connect(A, B, 80); // never answered

        // Walk the doubling schedule: retries at 50, 50+100, 150+200, …
        let mut now = 0;
        let mut resends = 0;
        for retry in 0..=MAX_RETRIES {
            now += RTO_BASE_TICKS << retry;
            let out = client.on_tick(now);
            if client.state(cid) == Some(TcpState::Closed) {
                break;
            }
            assert_eq!(out.0.len(), 1, "retry {retry} due at tick {now}");
            resends += 1;
        }
        assert_eq!(resends, MAX_RETRIES as usize);
        // One more overdue tick exhausts the budget: honest death, no zombie.
        now += RTO_BASE_TICKS << MAX_RETRIES;
        let _ = client.on_tick(now);
        assert_eq!(client.state(cid), Some(TcpState::Closed));
        assert!(client.on_tick(now + 1_000_000).0.is_empty());
    }

    #[test]
    fn duplicate_data_gets_a_resync_ack_not_silence() {
        let (mut client, cid, mut server, _sid) = established();

        // Deliver the same data segment twice — as if our first ACK was lost
        // and the peer retransmitted.
        let data = client.send(A, cid, b"hello").unwrap();
        let seg = data.0[0].1.clone();
        let first = server.on_segment(B, A, &seg);
        assert_eq!(first.0.len(), 1, "fresh data is ACKed");
        let second = server.on_segment(B, A, &seg);
        assert_eq!(
            second.0.len(),
            1,
            "duplicate data must be re-ACKed so the peer stops retransmitting"
        );
        // And the payload was not delivered twice.
        let sid = server.table()[0].0;
        assert_eq!(server.recv(sid), b"hello".to_vec());
    }

    #[test]
    fn retransmitted_synack_gets_reacked_by_established_peer() {
        let mut client = Tcp::new();
        let mut server = Tcp::new();
        server.listen(80);
        let (cid, syn) = client.connect(A, B, 80);
        let synack = pump(&mut server, B, A, syn);
        let synack_bytes = synack.0[0].1.clone();
        let ack = pump(&mut client, A, B, synack);
        drop(ack); // the wire ate the final handshake ACK
        assert_eq!(client.state(cid), Some(TcpState::Established));

        // The server times out and retransmits its SYN-ACK; the established
        // client must answer with a fresh ACK so the server can complete.
        let reack = client.on_segment(A, B, &synack_bytes);
        assert_eq!(reack.0.len(), 1);
        let _ = pump(&mut server, B, A, reack);
        let sid = server.accept(80).expect("handshake completed on retry");
        assert_eq!(server.state(sid), Some(TcpState::Established));
    }

    #[test]
    fn server_synack_is_queued_for_retransmission() {
        let mut client = Tcp::new();
        let mut server = Tcp::new();
        server.listen(80);
        let (_cid, syn) = client.connect(A, B, 80);
        let synack = pump(&mut server, B, A, syn);
        drop(synack); // the wire ate the SYN-ACK

        let retrans = server.on_tick(RTO_BASE_TICKS);
        assert_eq!(retrans.0.len(), 1, "SYN-ACK must retransmit too");
    }

    // ── flow control: the advertised receive window ─────────────────────

    /// Read the 16-bit window field out of a built segment.
    fn window_of(seg: &[u8]) -> u16 {
        u16::from_be_bytes([seg[14], seg[15]])
    }

    #[test]
    fn advertised_window_shrinks_as_data_buffers_and_reopens_on_drain() {
        let (mut client, cid, mut server, sid) = established();
        // Server sends data; the client buffers it unread, so the ACK the
        // client returns must advertise a window reduced by that many bytes.
        let payload = alloc::vec![0xAB_u8; 1000];
        let data = server.send(B, sid, &payload).unwrap();
        let ack = pump(&mut client, A, B, data);
        assert_eq!(ack.0.len(), 1);
        assert_eq!(
            window_of(&ack.0[0].1),
            RCV_BUF_MAX - 1000,
            "window must reflect buffered-but-unread bytes"
        );
        // Draining the buffer reopens the window (a full window-update ACK).
        assert_eq!(client.recv(cid).len(), 1000);
        let update = client.window_update(A, cid);
        assert_eq!(window_of(&update.0[0].1), RCV_BUF_MAX);
    }

    #[test]
    fn congestion_window_starts_open_and_grows_on_acked_data() {
        let (mut client, cid, mut server, sid) = established();
        // cwnd starts around the initial window (the handshake's SYN-ACK, which
        // acks our SYN, already counts as one unit of forward progress).
        assert!(client.conns[&cid].cwnd >= INIT_CWND);
        let cwnd0 = client.conns[&cid].cwnd;
        // Send data over the real path; the server delivers + ACKs it, and the
        // ACK's forward progress opens the sender's congestion window further.
        let payload = alloc::vec![b'x'; 1000];
        let data = client.send(A, cid, &payload).unwrap();
        let server_ack = pump(&mut server, B, A, data);
        assert_eq!(server.recv(sid).len(), 1000);
        pump(&mut client, A, B, server_ack);
        assert_eq!(
            client.conns[&cid].cwnd,
            cwnd0 + MSS,
            "a clean ACK grows cwnd by one MSS in slow start"
        );
    }

    #[test]
    fn a_retransmit_collapses_the_congestion_window() {
        let (mut client, cid, _server, _sid) = established();
        let grown = 20 * MSS;
        client.conns.get_mut(&cid).unwrap().cwnd = grown;
        client.conns.get_mut(&cid).unwrap().ssthresh = grown;
        // Send data, let it go unacked, and let the RTO fire.
        let _ = client.send(A, cid, b"lost data").unwrap();
        client.on_tick(RTO_BASE_TICKS);
        let after = client.conns[&cid].cwnd;
        assert!(after < grown, "loss must cut cwnd ({after} vs {grown})");
        assert_eq!(client.conns[&cid].ssthresh, grown / 2, "ssthresh halved");
    }

    #[test]
    fn sender_tracks_peer_window_and_stops_on_zero() {
        let (mut client, cid, _server, _sid) = established();
        // A nonzero peer window admits the send.
        client.conns.get_mut(&cid).unwrap().snd_wnd = 8;
        assert!(client.send(A, cid, b"hello").is_ok());
        // A zero peer window stops the sender until an ACK reopens it.
        client.conns.get_mut(&cid).unwrap().snd_wnd = 0;
        assert!(client.send(A, cid, b"x").is_err(), "zero window blocks all");
        // Reopening the window lets data flow again.
        client.conns.get_mut(&cid).unwrap().snd_wnd = 100;
        assert!(client.send(A, cid, b"resumed").is_ok());
    }

    #[test]
    fn receive_buffer_never_overflows_its_bound() {
        // A misbehaving peer ignores our advertised window and floods us with
        // one oversized segment. The receiver must clamp to its buffer bound
        // and advertise a zero window — never allocate unboundedly.
        let (mut client, cid, _server, _sid) = established();
        let seq = client.conns[&cid].rcv_nxt;
        let cport = client.conns[&cid].local_port;
        let flood = alloc::vec![0x5A_u8; RCV_BUF_MAX as usize + 5000];
        // Hand-built in-order data segment from B:80 → A:cport, bypassing the
        // sender-side flow control entirely.
        let seg = build_segment(80, cport, seq, 0, ACK, RCV_BUF_MAX, B, A, &flood);
        let out = client.on_segment(A, B, &seg);
        assert_eq!(window_of(&out.0[0].1), 0, "buffer full → zero window");
        assert_eq!(
            client.recv(cid).len(),
            RCV_BUF_MAX as usize,
            "never buffers beyond the bound"
        );
    }

    #[test]
    fn window_flows_end_to_end_over_a_link() {
        // The whole loop: a connect advertises the initial window, so the
        // sender learns it and can send immediately.
        let mut client = Tcp::new();
        let mut server = Tcp::new();
        server.listen(80);
        let (cid, syn) = client.connect(A, B, 80);
        assert_eq!(window_of(&syn.0[0].1), RCV_BUF_MAX, "SYN advertises window");
        let synack = pump(&mut server, B, A, syn);
        assert_eq!(window_of(&synack.0[0].1), RCV_BUF_MAX);
        let ack = pump(&mut client, A, B, synack);
        let _ = pump(&mut server, B, A, ack);
        // The client learned the server's window from the SYN-ACK.
        assert_eq!(client.conns[&cid].snd_wnd, RCV_BUF_MAX);
        assert!(client.send(A, cid, b"data flows").is_ok());
    }
}
