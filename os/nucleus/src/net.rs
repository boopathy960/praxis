//! The Praxis network stack — Ethernet / ARP / IPv4 / ICMP / UDP, from scratch.
//!
//! A real OS talks to the world. This is a freestanding, zero-dependency stack:
//! a [`NetDevice`] boundary (the line a real virtio-net / e1000 driver would sit
//! behind), a loopback device, and the protocol machinery above it — frame
//! parsing, an ARP resolver + cache, IPv4 with the ones-complement checksum,
//! ICMP echo (ping), and UDP datagrams demultiplexed to bound sockets.
//!
//! The Praxis twist — the thing Linux does not do — is that **egress is a
//! function of proof, not identity**. Every send carries the caller's [`Tier`];
//! an `Unproven` principal may talk to loopback but is denied the wire. Network
//! authority is granted by what has been proven about the code, exactly like the
//! process-capability and syscall layers.

use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;

use crate::proof::Tier;
use crate::tcp::{Tcp, TcpOut, TcpState};

pub type MacAddr = [u8; 6];
pub type Ipv4Addr = [u8; 4];

pub const BROADCAST_MAC: MacAddr = [0xff; 6];
pub const ETHERTYPE_IPV4: u16 = 0x0800;
pub const ETHERTYPE_ARP: u16 = 0x0806;
pub const IP_PROTO_ICMP: u8 = 1;
pub const IP_PROTO_TCP: u8 = 6;
pub const IP_PROTO_UDP: u8 = 17;
const ARP_REQUEST: u16 = 1;
const ARP_REPLY: u16 = 2;
const ICMP_ECHO_REQUEST: u8 = 8;
const ICMP_ECHO_REPLY: u8 = 0;

/// What can go wrong on a send.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetError {
    /// The destination MAC is not yet known; an ARP request was emitted. Poll
    /// the stack, then retry once the reply lands.
    ArpPending,
    /// The caller's proof tier is not permitted to egress to this destination.
    Denied,
    /// No socket is bound to that port.
    PortClosed,
    /// No such (established) connection.
    NotConnected,
}

/// The hardware boundary. A real NIC driver implements this; the kernel above it
/// never changes.
pub trait NetDevice {
    fn mac(&self) -> MacAddr;
    fn transmit(&mut self, frame: Vec<u8>);
    fn receive(&mut self) -> Option<Vec<u8>>;
}

/// A loopback NIC: whatever is transmitted is received back. Enough to exercise
/// the entire stack (a host pinging its own address) with no wire.
pub struct Loopback {
    mac: MacAddr,
    queue: VecDeque<Vec<u8>>,
}

impl Loopback {
    #[must_use]
    pub fn new(mac: MacAddr) -> Self {
        Self {
            mac,
            queue: VecDeque::new(),
        }
    }
}

impl NetDevice for Loopback {
    fn mac(&self) -> MacAddr {
        self.mac
    }
    fn transmit(&mut self, frame: Vec<u8>) {
        self.queue.push_back(frame);
    }
    fn receive(&mut self) -> Option<Vec<u8>> {
        self.queue.pop_front()
    }
}

#[derive(Debug, Default, Clone)]
pub struct NetStats {
    pub tx_frames: u64,
    pub rx_frames: u64,
    pub tx_udp: u64,
    pub rx_udp: u64,
    pub tx_icmp: u64,
    pub rx_icmp: u64,
    pub arp_replies: u64,
    pub egress_denied: u64,
}

/// The assembled stack: our identity, an ARP cache, bound UDP sockets, the
/// device, and the proof-gated egress policy.
pub struct NetStack {
    pub mac: MacAddr,
    pub ip: Ipv4Addr,
    /// Which destinations are on-link. Off-subnet traffic is L2-addressed to
    /// the gateway (the routing decision every real stack makes per packet).
    pub netmask: Ipv4Addr,
    /// The default gateway's IP, when this interface has a way off-subnet.
    pub gateway: Option<Ipv4Addr>,
    pub arp: BTreeMap<Ipv4Addr, MacAddr>,
    sockets: BTreeMap<u16, VecDeque<Datagram>>,
    pings: VecDeque<(Ipv4Addr, u16)>,
    dev: Box<dyn NetDevice>,
    pub stats: NetStats,
    /// When set, `Unproven` callers may only reach loopback / our own address.
    pub egress_requires_proof: bool,
    ident: u16,
    /// The TCP transport (pure state machine; this stack does its wire plumbing).
    pub tcp: Tcp,
    /// TCP segments awaiting ARP resolution, flushed each poll.
    tcp_pending: Vec<(Ipv4Addr, Vec<u8>)>,
}

/// A delivered UDP datagram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Datagram {
    pub src_ip: Ipv4Addr,
    pub src_port: u16,
    pub payload: Vec<u8>,
}

impl NetStack {
    /// A stack over an arbitrary device.
    #[must_use]
    pub fn new(ip: Ipv4Addr, dev: Box<dyn NetDevice>) -> Self {
        Self {
            mac: dev.mac(),
            ip,
            netmask: [255, 255, 255, 0],
            gateway: None,
            arp: BTreeMap::new(),
            sockets: BTreeMap::new(),
            pings: VecDeque::new(),
            dev,
            stats: NetStats::default(),
            egress_requires_proof: true,
            ident: 0,
            tcp: Tcp::new(),
            tcp_pending: Vec::new(),
        }
    }

    /// A stack over a loopback NIC — the kernel's default interface.
    #[must_use]
    pub fn loopback(ip: Ipv4Addr) -> Self {
        let mac = [0x02, 0xA1, 0x0A, ip[1], ip[2], ip[3]];
        Self::new(ip, Box::new(Loopback::new(mac)))
    }

    /// Open a UDP socket on `port`.
    pub fn bind(&mut self, port: u16) {
        self.sockets.entry(port).or_default();
    }

    /// True once this port has a bound socket.
    #[must_use]
    pub fn is_bound(&self, port: u16) -> bool {
        self.sockets.contains_key(&port)
    }

    #[must_use]
    pub fn arp_len(&self) -> usize {
        self.arp.len()
    }

    #[must_use]
    pub fn socket_count(&self) -> usize {
        self.sockets.len()
    }

    /// Does `tier` have authority to egress to `dst`? Loopback / our own address
    /// is always allowed; the wire requires a non-Unproven tier when the policy
    /// is on. This is the whole thesis, at the network layer.
    #[must_use]
    pub fn egress_permitted(&self, tier: Tier, dst: Ipv4Addr) -> bool {
        let local = dst == self.ip || dst[0] == 127;
        local || !self.egress_requires_proof || tier != Tier::Unproven
    }

    /// Send a UDP datagram. `tier` is the caller's proof tier; egress to the wire
    /// is denied to `Unproven` callers under the default policy.
    pub fn send_udp(
        &mut self,
        dst_ip: Ipv4Addr,
        dst_port: u16,
        src_port: u16,
        payload: &[u8],
        tier: Tier,
    ) -> Result<(), NetError> {
        if !self.egress_permitted(tier, dst_ip) {
            self.stats.egress_denied += 1;
            return Err(NetError::Denied);
        }
        let dst_mac = self.resolve(dst_ip).ok_or(NetError::ArpPending)?;
        let udp = build_udp(src_port, dst_port, self.ip, dst_ip, payload);
        let ip = build_ipv4(self.ip, dst_ip, IP_PROTO_UDP, &udp, self.next_ident());
        self.tx(dst_mac, ETHERTYPE_IPV4, &ip);
        self.stats.tx_udp += 1;
        Ok(())
    }

    /// Read the next datagram delivered to `port`, if any.
    pub fn recv_udp(&mut self, port: u16) -> Result<Option<Datagram>, NetError> {
        match self.sockets.get_mut(&port) {
            Some(inbox) => Ok(inbox.pop_front()),
            None => Err(NetError::PortClosed),
        }
    }

    /// Send an ICMP echo request (ping).
    pub fn ping(&mut self, dst_ip: Ipv4Addr, seq: u16, tier: Tier) -> Result<(), NetError> {
        if !self.egress_permitted(tier, dst_ip) {
            self.stats.egress_denied += 1;
            return Err(NetError::Denied);
        }
        let dst_mac = self.resolve(dst_ip).ok_or(NetError::ArpPending)?;
        let echo = build_icmp(ICMP_ECHO_REQUEST, 0x1D, seq, b"praxis-ping");
        let ip = build_ipv4(self.ip, dst_ip, IP_PROTO_ICMP, &echo, self.next_ident());
        self.tx(dst_mac, ETHERTYPE_IPV4, &ip);
        self.stats.tx_icmp += 1;
        Ok(())
    }

    /// Pop the next received echo reply `(from, seq)`.
    pub fn take_ping_reply(&mut self) -> Option<(Ipv4Addr, u16)> {
        self.pings.pop_front()
    }

    // ── TCP ────────────────────────────────────────────────────────────────

    /// Passively open `port` for inbound connections.
    pub fn tcp_listen(&mut self, port: u16) {
        self.tcp.listen(port);
    }

    /// Actively open a connection. Egress to the wire is proof-gated exactly like
    /// UDP: an `Unproven` caller may connect to loopback but not off-host.
    pub fn tcp_connect(
        &mut self,
        remote_ip: Ipv4Addr,
        remote_port: u16,
        tier: Tier,
    ) -> Result<usize, NetError> {
        if !self.egress_permitted(tier, remote_ip) {
            self.stats.egress_denied += 1;
            return Err(NetError::Denied);
        }
        let (id, out) = self.tcp.connect(self.ip, remote_ip, remote_port);
        self.emit_tcp_out(out);
        Ok(id)
    }

    /// Send bytes on an established connection.
    pub fn tcp_send(&mut self, id: usize, data: &[u8]) -> Result<(), NetError> {
        match self.tcp.send(self.ip, id, data) {
            Ok(out) => {
                self.emit_tcp_out(out);
                Ok(())
            }
            Err(()) => Err(NetError::NotConnected),
        }
    }

    /// Read received bytes on a connection. Draining the buffer reopens the
    /// receive window, so we send a window-update ACK to let a peer that was
    /// stopped by a full window resume sending.
    pub fn tcp_recv(&mut self, id: usize) -> Vec<u8> {
        let data = self.tcp.recv(id);
        if !data.is_empty() {
            let out = self.tcp.window_update(self.ip, id);
            self.emit_tcp_out(out);
        }
        data
    }

    /// Begin an active close (FIN).
    pub fn tcp_close(&mut self, id: usize) {
        let out = self.tcp.close(self.ip, id);
        self.emit_tcp_out(out);
    }

    /// Accept the next completed inbound connection on a listening port.
    pub fn tcp_accept(&mut self, port: u16) -> Option<usize> {
        self.tcp.accept(port)
    }

    #[must_use]
    pub fn tcp_state(&self, id: usize) -> Option<TcpState> {
        self.tcp.state(id)
    }

    #[must_use]
    pub fn tcp_table(&self) -> Vec<(usize, TcpState, Ipv4Addr, u16)> {
        self.tcp.table()
    }

    /// Drain the device and process every received frame. Returns how many frames
    /// were handled. Run it in a loop until it returns 0 to quiesce the stack.
    pub fn poll(&mut self) -> usize {
        let mut n = 0;
        while let Some(frame) = self.dev.receive() {
            n += 1;
            self.stats.rx_frames += 1;
            if let Some(eth) = EthFrame::parse(&frame) {
                match eth.ethertype {
                    ETHERTYPE_ARP => self.on_arp(&eth.payload),
                    ETHERTYPE_IPV4 => self.on_ipv4(&eth.payload),
                    _ => {}
                }
            }
        }
        // Retry any TCP segments that were blocked on ARP; a reply may have just
        // populated the cache during this poll.
        if !self.tcp_pending.is_empty() {
            let pending = core::mem::take(&mut self.tcp_pending);
            for (dst, bytes) in pending {
                self.emit_tcp(dst, &bytes);
            }
        }
        n
    }

    /// Poll until the stack is quiet (bounded), so request→reply cascades settle.
    pub fn poll_until_quiet(&mut self) {
        for _ in 0..16 {
            if self.poll() == 0 {
                break;
            }
        }
    }

    /// Drive TCP retransmission from the platform clock: overdue unacked
    /// segments go back on the wire with exponential backoff. Call from the
    /// idle loop alongside [`NetStack::poll`]. Returns how many segments were
    /// retransmitted.
    pub fn tick(&mut self, now: u64) -> usize {
        let out = self.tcp.on_tick(now);
        let n = out.0.len();
        if n > 0 {
            self.emit_tcp_out(out);
        }
        n
    }

    // ── internals ──────────────────────────────────────────────────────────

    fn next_ident(&mut self) -> u16 {
        self.ident = self.ident.wrapping_add(1);
        self.ident
    }

    fn tx(&mut self, dst: MacAddr, ethertype: u16, payload: &[u8]) {
        let frame = build_eth(dst, self.mac, ethertype, payload);
        self.dev.transmit(frame);
        self.stats.tx_frames += 1;
    }

    /// Wrap and send the segments a TCP step produced (or buffer on ARP miss).
    fn emit_tcp_out(&mut self, out: TcpOut) {
        for (dst, bytes) in out.0 {
            self.emit_tcp(dst, &bytes);
        }
    }

    fn emit_tcp(&mut self, dst_ip: Ipv4Addr, seg: &[u8]) {
        match self.resolve(dst_ip) {
            Some(mac) => {
                let ip = build_ipv4(self.ip, dst_ip, IP_PROTO_TCP, seg, self.next_ident());
                self.tx(mac, ETHERTYPE_IPV4, &ip);
            }
            None => self.tcp_pending.push((dst_ip, seg.to_vec())),
        }
    }

    /// Is `ip` on this interface's subnet (same network under the netmask)?
    fn on_link(&self, ip: Ipv4Addr) -> bool {
        (0..4).all(|i| ip[i] & self.netmask[i] == self.ip[i] & self.netmask[i])
    }

    /// Resolve `ip` to the MAC of its **next hop**. Our own address (or
    /// loopback) is us; on-link destinations resolve directly; off-subnet
    /// destinations resolve to the default gateway — the routing decision.
    /// A cache miss emits an ARP request and reports pending.
    fn resolve(&mut self, ip: Ipv4Addr) -> Option<MacAddr> {
        if ip == self.ip || ip[0] == 127 {
            return Some(self.mac);
        }
        let next_hop = if self.on_link(ip) {
            ip
        } else {
            match self.gateway {
                Some(gw) => gw,
                None => ip, // no route off-subnet: last-ditch direct ARP
            }
        };
        if let Some(mac) = self.arp.get(&next_hop) {
            return Some(*mac);
        }
        let req = build_arp(ARP_REQUEST, self.mac, self.ip, [0; 6], next_hop);
        self.tx(BROADCAST_MAC, ETHERTYPE_ARP, &req);
        None
    }

    fn on_arp(&mut self, payload: &[u8]) {
        let Some(arp) = ArpPacket::parse(payload) else {
            return;
        };
        // Learn the sender either way.
        if arp.spa != [0u8; 4] {
            self.arp.insert(arp.spa, arp.sha);
        }
        if arp.op == ARP_REQUEST && arp.tpa == self.ip {
            let reply = build_arp(ARP_REPLY, self.mac, self.ip, arp.sha, arp.spa);
            self.tx(arp.sha, ETHERTYPE_ARP, &reply);
            self.stats.arp_replies += 1;
        }
    }

    fn on_ipv4(&mut self, payload: &[u8]) {
        let Some(ip) = Ipv4Packet::parse(payload) else {
            return;
        };
        if ip.dst != self.ip && ip.dst[0] != 127 {
            return; // not for us
        }
        match ip.proto {
            IP_PROTO_ICMP => self.on_icmp(ip.src, &ip.payload),
            IP_PROTO_UDP => self.on_udp(ip.src, &ip.payload),
            IP_PROTO_TCP => {
                let out = self.tcp.on_segment(self.ip, ip.src, &ip.payload);
                self.emit_tcp_out(out);
            }
            _ => {}
        }
    }

    fn on_icmp(&mut self, src: Ipv4Addr, payload: &[u8]) {
        let Some(echo) = IcmpEcho::parse(payload) else {
            return;
        };
        self.stats.rx_icmp += 1;
        match echo.typ {
            ICMP_ECHO_REQUEST => {
                // Reply to the sender. It is in the ARP cache (or is us).
                if let Some(dst_mac) = self.resolve(src) {
                    let reply = build_icmp(ICMP_ECHO_REPLY, echo.id, echo.seq, &echo.data);
                    let ip = build_ipv4(self.ip, src, IP_PROTO_ICMP, &reply, self.next_ident());
                    self.tx(dst_mac, ETHERTYPE_IPV4, &ip);
                    self.stats.tx_icmp += 1;
                }
            }
            ICMP_ECHO_REPLY => self.pings.push_back((src, echo.seq)),
            _ => {}
        }
    }

    fn on_udp(&mut self, src_ip: Ipv4Addr, payload: &[u8]) {
        let Some(udp) = UdpDatagram::parse(payload) else {
            return;
        };
        if let Some(inbox) = self.sockets.get_mut(&udp.dst_port) {
            inbox.push_back(Datagram {
                src_ip,
                src_port: udp.src_port,
                payload: udp.payload,
            });
            self.stats.rx_udp += 1;
        }
    }
}

// ── the ones-complement Internet checksum (RFC 1071) ─────────────────────────

pub(crate) fn checksum16(parts: &[&[u8]]) -> u16 {
    let mut sum = 0u32;
    let mut carry_byte: Option<u8> = None;
    for part in parts {
        let mut bytes = *part;
        // Stitch an odd trailing byte from the previous part with the first here.
        if let Some(hi) = carry_byte.take() {
            if let Some((lo, rest)) = bytes.split_first() {
                sum += u16::from_be_bytes([hi, *lo]) as u32;
                bytes = rest;
            } else {
                sum += (hi as u32) << 8;
            }
        }
        let mut i = 0;
        while i + 1 < bytes.len() {
            sum += u16::from_be_bytes([bytes[i], bytes[i + 1]]) as u32;
            i += 2;
        }
        if i < bytes.len() {
            carry_byte = Some(bytes[i]);
        }
    }
    if let Some(hi) = carry_byte {
        sum += (hi as u32) << 8;
    }
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

// ── Ethernet ─────────────────────────────────────────────────────────────────

struct EthFrame {
    ethertype: u16,
    payload: Vec<u8>,
}

impl EthFrame {
    fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 14 {
            return None;
        }
        Some(Self {
            ethertype: u16::from_be_bytes([bytes[12], bytes[13]]),
            payload: bytes[14..].to_vec(),
        })
    }
}

fn build_eth(dst: MacAddr, src: MacAddr, ethertype: u16, payload: &[u8]) -> Vec<u8> {
    let mut f = Vec::with_capacity(14 + payload.len());
    f.extend_from_slice(&dst);
    f.extend_from_slice(&src);
    f.extend_from_slice(&ethertype.to_be_bytes());
    f.extend_from_slice(payload);
    f
}

// ── ARP ──────────────────────────────────────────────────────────────────────

struct ArpPacket {
    op: u16,
    sha: MacAddr,
    spa: Ipv4Addr,
    #[allow(dead_code)]
    tha: MacAddr,
    tpa: Ipv4Addr,
}

impl ArpPacket {
    fn parse(b: &[u8]) -> Option<Self> {
        if b.len() < 28 {
            return None;
        }
        // htype=1 ethernet, ptype=0x0800 ipv4, hlen=6, plen=4 expected.
        let op = u16::from_be_bytes([b[6], b[7]]);
        let mut sha = [0u8; 6];
        sha.copy_from_slice(&b[8..14]);
        let mut spa = [0u8; 4];
        spa.copy_from_slice(&b[14..18]);
        let mut tha = [0u8; 6];
        tha.copy_from_slice(&b[18..24]);
        let mut tpa = [0u8; 4];
        tpa.copy_from_slice(&b[24..28]);
        Some(Self {
            op,
            sha,
            spa,
            tha,
            tpa,
        })
    }
}

fn build_arp(op: u16, sha: MacAddr, spa: Ipv4Addr, tha: MacAddr, tpa: Ipv4Addr) -> Vec<u8> {
    let mut p = Vec::with_capacity(28);
    p.extend_from_slice(&1u16.to_be_bytes()); // htype: ethernet
    p.extend_from_slice(&ETHERTYPE_IPV4.to_be_bytes()); // ptype: ipv4
    p.push(6); // hlen
    p.push(4); // plen
    p.extend_from_slice(&op.to_be_bytes());
    p.extend_from_slice(&sha);
    p.extend_from_slice(&spa);
    p.extend_from_slice(&tha);
    p.extend_from_slice(&tpa);
    p
}

// ── IPv4 ─────────────────────────────────────────────────────────────────────

struct Ipv4Packet {
    src: Ipv4Addr,
    dst: Ipv4Addr,
    proto: u8,
    payload: Vec<u8>,
}

impl Ipv4Packet {
    fn parse(b: &[u8]) -> Option<Self> {
        if b.len() < 20 || (b[0] >> 4) != 4 {
            return None;
        }
        let ihl = (b[0] & 0x0f) as usize * 4;
        if ihl < 20 || b.len() < ihl {
            return None;
        }
        let total = u16::from_be_bytes([b[2], b[3]]) as usize;
        let end = total.min(b.len());
        if end < ihl {
            return None;
        }
        let mut src = [0u8; 4];
        src.copy_from_slice(&b[12..16]);
        let mut dst = [0u8; 4];
        dst.copy_from_slice(&b[16..20]);
        Some(Self {
            src,
            dst,
            proto: b[9],
            payload: b[ihl..end].to_vec(),
        })
    }
}

fn build_ipv4(src: Ipv4Addr, dst: Ipv4Addr, proto: u8, payload: &[u8], ident: u16) -> Vec<u8> {
    let total = 20 + payload.len();
    let mut h = Vec::with_capacity(total);
    h.push(0x45); // version 4, IHL 5
    h.push(0); // DSCP/ECN
    h.extend_from_slice(&(total as u16).to_be_bytes());
    h.extend_from_slice(&ident.to_be_bytes());
    h.extend_from_slice(&0x4000u16.to_be_bytes()); // flags: don't fragment
    h.push(64); // TTL
    h.push(proto);
    h.extend_from_slice(&[0, 0]); // checksum placeholder
    h.extend_from_slice(&src);
    h.extend_from_slice(&dst);
    let csum = checksum16(&[&h]);
    h[10..12].copy_from_slice(&csum.to_be_bytes());
    h.extend_from_slice(payload);
    h
}

// ── ICMP echo ────────────────────────────────────────────────────────────────

struct IcmpEcho {
    typ: u8,
    id: u16,
    seq: u16,
    data: Vec<u8>,
}

impl IcmpEcho {
    fn parse(b: &[u8]) -> Option<Self> {
        if b.len() < 8 {
            return None;
        }
        Some(Self {
            typ: b[0],
            id: u16::from_be_bytes([b[4], b[5]]),
            seq: u16::from_be_bytes([b[6], b[7]]),
            data: b[8..].to_vec(),
        })
    }
}

fn build_icmp(typ: u8, id: u16, seq: u16, data: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(8 + data.len());
    p.push(typ);
    p.push(0); // code
    p.extend_from_slice(&[0, 0]); // checksum placeholder
    p.extend_from_slice(&id.to_be_bytes());
    p.extend_from_slice(&seq.to_be_bytes());
    p.extend_from_slice(data);
    let csum = checksum16(&[&p]);
    p[2..4].copy_from_slice(&csum.to_be_bytes());
    p
}

// ── UDP ──────────────────────────────────────────────────────────────────────

struct UdpDatagram {
    src_port: u16,
    dst_port: u16,
    payload: Vec<u8>,
}

impl UdpDatagram {
    fn parse(b: &[u8]) -> Option<Self> {
        if b.len() < 8 {
            return None;
        }
        let len = u16::from_be_bytes([b[4], b[5]]) as usize;
        let end = len.max(8).min(b.len());
        Some(Self {
            src_port: u16::from_be_bytes([b[0], b[1]]),
            dst_port: u16::from_be_bytes([b[2], b[3]]),
            payload: b[8..end].to_vec(),
        })
    }
}

fn build_udp(
    src_port: u16,
    dst_port: u16,
    src_ip: Ipv4Addr,
    dst_ip: Ipv4Addr,
    payload: &[u8],
) -> Vec<u8> {
    let len = 8 + payload.len();
    let mut d = Vec::with_capacity(len);
    d.extend_from_slice(&src_port.to_be_bytes());
    d.extend_from_slice(&dst_port.to_be_bytes());
    d.extend_from_slice(&(len as u16).to_be_bytes());
    d.extend_from_slice(&[0, 0]); // checksum placeholder
    d.extend_from_slice(payload);
    // UDP checksum covers a pseudo-header (src, dst, zero, proto, udp length).
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
        IP_PROTO_UDP,
        (len >> 8) as u8,
        (len & 0xff) as u8,
    ];
    let csum = checksum16(&[&pseudo, &d]);
    // A computed 0 is transmitted as 0xffff (0 means "no checksum" in UDP).
    let csum = if csum == 0 { 0xffff } else { csum };
    d[6..8].copy_from_slice(&csum.to_be_bytes());
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cell::RefCell;
    use std::rc::Rc;

    /// A NIC one end of a crossed link: its TX is the peer's RX.
    struct LinkNic {
        mac: MacAddr,
        tx: Rc<RefCell<VecDeque<Vec<u8>>>>,
        rx: Rc<RefCell<VecDeque<Vec<u8>>>>,
    }
    impl NetDevice for LinkNic {
        fn mac(&self) -> MacAddr {
            self.mac
        }
        fn transmit(&mut self, frame: Vec<u8>) {
            self.tx.borrow_mut().push_back(frame);
        }
        fn receive(&mut self) -> Option<Vec<u8>> {
            self.rx.borrow_mut().pop_front()
        }
    }

    /// Two stacks on a crossed link.
    fn link(ip_a: Ipv4Addr, ip_b: Ipv4Addr) -> (NetStack, NetStack) {
        let a2b = Rc::new(RefCell::new(VecDeque::new()));
        let b2a = Rc::new(RefCell::new(VecDeque::new()));
        let a = NetStack::new(
            ip_a,
            Box::new(LinkNic {
                mac: [0x02, 0, 0, 0, 0, 0xAA],
                tx: a2b.clone(),
                rx: b2a.clone(),
            }),
        );
        let b = NetStack::new(
            ip_b,
            Box::new(LinkNic {
                mac: [0x02, 0, 0, 0, 0, 0xBB],
                tx: b2a,
                rx: a2b,
            }),
        );
        (a, b)
    }

    #[test]
    fn checksum_matches_known_ipv4_header() {
        // A textbook IPv4 header with a known checksum of 0xb861.
        let hdr = [
            0x45u8, 0x00, 0x00, 0x73, 0x00, 0x00, 0x40, 0x00, 0x40, 0x11, 0x00, 0x00, 0xc0, 0xa8,
            0x00, 0x01, 0xc0, 0xa8, 0x00, 0xc7,
        ];
        assert_eq!(checksum16(&[&hdr]), 0xb861);
    }

    #[test]
    fn off_subnet_traffic_routes_through_the_gateway() {
        // A is a host with a default gateway; B plays the gateway. A sends a
        // datagram to an internet address: the ARP must target the GATEWAY's
        // IP, and the emitted frame must be L2-addressed to the gateway's MAC
        // while the IP header still names the real destination.
        let a2b = Rc::new(RefCell::new(VecDeque::new()));
        let b2a = Rc::new(RefCell::new(VecDeque::new()));
        let mut a = NetStack::new(
            [10, 0, 2, 15],
            Box::new(LinkNic {
                mac: [0x02, 0, 0, 0, 0, 0xAA],
                tx: a2b.clone(),
                rx: b2a.clone(),
            }),
        );
        a.gateway = Some([10, 0, 2, 2]);
        let gw_mac = [0x02, 0, 0, 0, 0, 0xBB];
        let mut gw = NetStack::new(
            [10, 0, 2, 2],
            Box::new(LinkNic {
                mac: gw_mac,
                tx: b2a,
                rx: a2b.clone(),
            }),
        );

        let internet: Ipv4Addr = [93, 184, 216, 34];
        assert_eq!(
            a.send_udp(internet, 80, 5000, b"GET", Tier::Proven),
            Err(NetError::ArpPending)
        );
        gw.poll(); // the gateway answers the ARP (it was asked for ITS ip)
        a.poll(); // a learns the gateway's MAC
        assert_eq!(a.arp.get(&[10, 0, 2, 2]), Some(&gw_mac), "ARPed the gw");
        a.send_udp(internet, 80, 5000, b"GET", Tier::Proven)
            .unwrap();

        // Inspect the wire: eth dst = gateway MAC, IP dst = the internet.
        let frame = a2b.borrow_mut().pop_front().expect("frame on the wire");
        assert_eq!(&frame[0..6], &gw_mac, "L2-addressed to the gateway");
        let eth = EthFrame::parse(&frame).unwrap();
        assert_eq!(eth.ethertype, ETHERTYPE_IPV4);
        let ip = Ipv4Packet::parse(&eth.payload).unwrap();
        assert_eq!(ip.dst, internet, "L3 still names the real destination");
    }

    #[test]
    fn tcp_survives_frame_loss_via_tick_retransmission() {
        // Full-stack loss recovery: a data FRAME vanishes off the wire (we
        // hold the link queue and clear it before delivery); the clock-driven
        // tick puts the segment back and the transfer still completes. This is
        // the property that keeps a real lossy wire honest.
        let a2b = Rc::new(RefCell::new(VecDeque::new()));
        let b2a = Rc::new(RefCell::new(VecDeque::new()));
        let mut a = NetStack::new(
            [10, 0, 0, 1],
            Box::new(LinkNic {
                mac: [0x02, 0, 0, 0, 0, 0xAA],
                tx: a2b.clone(),
                rx: b2a.clone(),
            }),
        );
        let mut b = NetStack::new(
            [10, 0, 0, 2],
            Box::new(LinkNic {
                mac: [0x02, 0, 0, 0, 0, 0xBB],
                tx: b2a,
                rx: a2b.clone(),
            }),
        );
        b.tcp_listen(80);
        let cid = a
            .tcp_connect([10, 0, 0, 2], 80, crate::proof::Tier::Proven)
            .unwrap();
        for _ in 0..4 {
            b.poll();
            a.poll();
        }
        let sid = b.tcp_accept(80).expect("established");

        // The wire eats the data frame: clear the queue before b sees it.
        a.tcp_send(cid, b"lossy hello").unwrap();
        a2b.borrow_mut().clear();
        b.poll();
        assert_eq!(b.tcp_recv(sid), Vec::<u8>::new(), "frame was lost");

        // Clock-driven recovery: tick past the RTO, deliver, done.
        let retransmitted = a.tick(10_000);
        assert_eq!(retransmitted, 1, "the unacked segment goes back out");
        for _ in 0..4 {
            b.poll();
            a.poll();
        }
        assert_eq!(b.tcp_recv(sid), b"lossy hello".to_vec());
    }

    #[test]
    fn loopback_ping_round_trips() {
        let mut net = NetStack::loopback([10, 0, 0, 1]);
        net.ping([10, 0, 0, 1], 7, Tier::Proven).unwrap();
        net.poll_until_quiet();
        assert_eq!(net.take_ping_reply(), Some(([10, 0, 0, 1], 7)));
        assert!(net.stats.tx_icmp >= 2 && net.stats.rx_icmp >= 2);
    }

    #[test]
    fn udp_delivers_to_bound_socket_over_a_link() {
        let (mut a, mut b) = link([10, 0, 0, 1], [10, 0, 0, 2]);
        b.bind(4242);
        // First send resolves ARP (pending), so drive the handshake.
        assert_eq!(
            a.send_udp([10, 0, 0, 2], 4242, 5000, b"hello", Tier::Proven),
            Err(NetError::ArpPending)
        );
        b.poll(); // b answers the ARP request
        a.poll(); // a learns b's MAC
        a.send_udp([10, 0, 0, 2], 4242, 5000, b"hello", Tier::Proven)
            .unwrap();
        b.poll();
        let dg = b.recv_udp(4242).unwrap().expect("datagram delivered");
        assert_eq!(dg.src_ip, [10, 0, 0, 1]);
        assert_eq!(dg.src_port, 5000);
        assert_eq!(dg.payload, b"hello");
    }

    #[test]
    fn arp_handshake_populates_both_caches() {
        let (mut a, mut b) = link([10, 0, 0, 1], [10, 0, 0, 2]);
        assert_eq!(a.resolve([10, 0, 0, 2]), None); // emits request
        b.poll(); // reply
        a.poll(); // learn
        assert_eq!(a.arp.get(&[10, 0, 0, 2]), Some(&[0x02, 0, 0, 0, 0, 0xBB]));
        assert_eq!(b.arp.get(&[10, 0, 0, 1]), Some(&[0x02, 0, 0, 0, 0, 0xAA]));
    }

    #[test]
    fn unproven_egress_to_the_wire_is_denied_but_loopback_is_allowed() {
        let mut net = NetStack::loopback([10, 0, 0, 1]);
        // The wire: denied for an unproven caller.
        assert_eq!(
            net.send_udp([8, 8, 8, 8], 53, 1000, b"q", Tier::Unproven),
            Err(NetError::Denied)
        );
        assert_eq!(net.stats.egress_denied, 1);
        // Loopback to self: allowed even unproven.
        net.bind(9);
        assert!(net
            .send_udp([10, 0, 0, 1], 9, 1000, b"local", Tier::Unproven)
            .is_ok());
        // Proven caller reaches the wire (ARP pending, but not denied).
        assert_eq!(
            net.send_udp([8, 8, 8, 8], 53, 1000, b"q", Tier::Proven),
            Err(NetError::ArpPending)
        );
    }

    #[test]
    fn tcp_connects_and_transfers_over_a_link() {
        let (mut a, mut b) = link([10, 0, 0, 1], [10, 0, 0, 2]);
        b.tcp_listen(80);
        // No pre-resolved ARP: the SYN is buffered until the handshake lands,
        // then flushed — the whole path (ARP → IP → TCP) drives itself.
        let cid = a.tcp_connect([10, 0, 0, 2], 80, Tier::Proven).unwrap();
        for _ in 0..8 {
            b.poll();
            a.poll();
        }
        assert_eq!(a.tcp_state(cid), Some(TcpState::Established));
        let sid = b.tcp_accept(80).expect("server accepted a connection");

        a.tcp_send(cid, b"hello tcp").unwrap();
        for _ in 0..4 {
            b.poll();
            a.poll();
        }
        assert_eq!(b.tcp_recv(sid), b"hello tcp".to_vec());

        // And the reverse direction.
        b.tcp_send(sid, b"ack from server").unwrap();
        for _ in 0..4 {
            a.poll();
            b.poll();
        }
        assert_eq!(a.tcp_recv(cid), b"ack from server".to_vec());
    }

    #[test]
    fn tcp_egress_to_the_wire_is_proof_gated() {
        let mut net = NetStack::loopback([10, 0, 0, 1]);
        assert_eq!(
            net.tcp_connect([93, 184, 216, 34], 80, Tier::Unproven),
            Err(NetError::Denied)
        );
    }

    #[test]
    fn udp_checksum_is_never_zero_on_the_wire() {
        // A payload that would zero the checksum must be transmitted as 0xffff.
        let d = build_udp(1, 1, [0, 0, 0, 0], [0, 0, 0, 0], &[]);
        assert_ne!(&d[6..8], &[0, 0]);
    }
}
