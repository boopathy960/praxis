//! Intel e1000 (82540EM) NIC driver — the first real wire under the stack.
//!
//! The from-scratch net stack ([`crate::net`]) was built against a driver
//! boundary of three methods (`mac`/`transmit`/`receive`); this is the first
//! implementation of that boundary backed by actual hardware. The driver
//! core is portable and lives in the nucleus, in the same style as the PCI
//! enumerator: every hardware touch goes through the [`E1000Bus`] trait
//! (MMIO register reads/writes + physical-memory access for the DMA rings),
//! so the whole thing is unit-testable on the host against a software model
//! of the chip, and the bare-metal boot crate only supplies the thin real
//! bus (BAR0 MMIO through the physical-memory mapping).
//!
//! It drives the chip the honest way: software reset, MAC from RAL/RAH (or
//! the EEPROM via EERD when the receive-address registers are cold), legacy
//! 16-byte RX/TX descriptor rings in one physically-contiguous DMA arena,
//! and polled operation — interrupts stay masked, because the kernel's idle
//! loop already polls the stack and a poll-mode driver has no re-entrancy.

use alloc::vec::Vec;

use crate::net::{MacAddr, NetDevice};

// ── registers (offsets into BAR0 MMIO space) ────────────────────────────
const CTRL: u32 = 0x0000;
const EERD: u32 = 0x0014;
const ICR: u32 = 0x00C0;
const IMC: u32 = 0x00D8;
const RCTL: u32 = 0x0100;
const TCTL: u32 = 0x0400;
const TIPG: u32 = 0x0410;
const RDBAL: u32 = 0x2800;
const RDBAH: u32 = 0x2804;
const RDLEN: u32 = 0x2808;
const RDH: u32 = 0x2810;
const RDT: u32 = 0x2818;
const TDBAL: u32 = 0x3800;
const TDBAH: u32 = 0x3804;
const TDLEN: u32 = 0x3808;
const TDH: u32 = 0x3810;
const TDT: u32 = 0x3818;
const MTA: u32 = 0x5200;
const RAL0: u32 = 0x5400;
const RAH0: u32 = 0x5404;

const CTRL_ASDE: u32 = 1 << 5;
const CTRL_SLU: u32 = 1 << 6;
const CTRL_RST: u32 = 1 << 26;
const EERD_START: u32 = 1 << 0;
const EERD_DONE: u32 = 1 << 4;
const RAH_AV: u32 = 1 << 31;
const RCTL_EN: u32 = 1 << 1;
const RCTL_BAM: u32 = 1 << 15;
const RCTL_SECRC: u32 = 1 << 26;
const TCTL_EN: u32 = 1 << 1;
const TCTL_PSP: u32 = 1 << 3;

/// TX descriptor command: end-of-packet, insert FCS, report status (DD).
const TXD_CMD: u8 = 0x01 | 0x02 | 0x08;
/// Descriptor Done — the hardware finished with this descriptor.
const STAT_DD: u8 = 1 << 0;

// ── the DMA arena layout ─────────────────────────────────────────────────
// One physically-contiguous allocation carries everything the chip DMAs:
//   [0x0000]  RX ring   (32 × 16 B = 512 B; RDLEN must be a multiple of 128)
//   [0x0200]  TX ring   ( 8 × 16 B = 128 B)
//   [0x1000]  RX buffers (32 × 2048 B)
//   [0x11000] TX buffers ( 8 × 2048 B)
pub const RX_DESCS: usize = 32;
pub const TX_DESCS: usize = 8;
pub const BUF_SIZE: usize = 2048;
const RX_RING_OFF: u64 = 0x0000;
const TX_RING_OFF: u64 = 0x0200;
const RX_BUFS_OFF: u64 = 0x1000;
const TX_BUFS_OFF: u64 = 0x11000;
/// Total arena footprint: 0x15000 bytes = 84 KiB = 21 frames.
pub const ARENA_BYTES: u64 = TX_BUFS_OFF + (TX_DESCS * BUF_SIZE) as u64;
/// Arena size in 4 KiB physical frames, for `FrameAllocator::allocate_contiguous`.
pub const ARENA_FRAMES: u64 = ARENA_BYTES.div_ceil(crate::mem::FRAME_SIZE);

/// The hardware boundary of the driver core: MMIO registers and the physical
/// memory the chip DMAs into. Bare metal implements this over BAR0 + the
/// bootloader's physical-memory mapping; tests implement it over arrays.
pub trait E1000Bus {
    fn read32(&mut self, reg: u32) -> u32;
    fn write32(&mut self, reg: u32, value: u32);
    /// Mutable view of `len` bytes of physical memory at `phys`.
    fn mem(&mut self, phys: u64, len: usize) -> &mut [u8];
}

pub struct E1000<B: E1000Bus> {
    bus: B,
    arena: u64,
    mac: MacAddr,
    next_rx: usize,
    next_tx: usize,
    /// Which TX descriptors have been handed to the chip at least once —
    /// only those must be reclaimed (DD-polled) before reuse.
    tx_used: [bool; TX_DESCS],
}

impl<B: E1000Bus> E1000<B> {
    /// Bring the chip up: reset, learn the MAC, program both rings, enable
    /// RX + TX. `arena_phys` is the physically-contiguous DMA arena
    /// ([`ARENA_BYTES`] long) the rings and buffers live in.
    pub fn new(mut bus: B, arena_phys: u64) -> Result<Self, &'static str> {
        // Interrupts off — this is a polled driver — then software reset.
        bus.write32(IMC, u32::MAX);
        let _ = bus.read32(ICR);
        let ctrl = bus.read32(CTRL);
        bus.write32(CTRL, ctrl | CTRL_RST);
        let mut spins = 0;
        while bus.read32(CTRL) & CTRL_RST != 0 {
            spins += 1;
            if spins > 100_000 {
                return Err("e1000: reset never completed");
            }
        }
        bus.write32(IMC, u32::MAX);
        let _ = bus.read32(ICR);

        let mac = read_mac(&mut bus).ok_or("e1000: no MAC in RAL/RAH or EEPROM")?;

        // Program our unicast filter and clear the multicast table.
        bus.write32(RAL0, u32::from_le_bytes([mac[0], mac[1], mac[2], mac[3]]));
        bus.write32(RAH0, u32::from(mac[4]) | u32::from(mac[5]) << 8 | RAH_AV);
        for i in 0..128 {
            bus.write32(MTA + i * 4, 0);
        }

        // Link up (and let auto-speed detection do its thing).
        let ctrl = bus.read32(CTRL);
        bus.write32(CTRL, ctrl | CTRL_SLU | CTRL_ASDE);

        let mut driver = Self {
            bus,
            arena: arena_phys,
            mac,
            next_rx: 0,
            next_tx: 0,
            tx_used: [false; TX_DESCS],
        };

        // RX ring: every descriptor points at its buffer, hardware owns all
        // of them (RDT = last descriptor), and we consume from index 0.
        for i in 0..RX_DESCS {
            let buf = driver.arena + RX_BUFS_OFF + (i * BUF_SIZE) as u64;
            driver.write_desc(RX_RING_OFF, i, buf, 0, 0, 0);
        }
        let rx_ring = driver.arena + RX_RING_OFF;
        driver.bus.write32(RDBAL, rx_ring as u32);
        driver.bus.write32(RDBAH, (rx_ring >> 32) as u32);
        driver.bus.write32(RDLEN, (RX_DESCS * 16) as u32);
        driver.bus.write32(RDH, 0);
        driver.bus.write32(RDT, (RX_DESCS - 1) as u32);
        driver.bus.write32(RCTL, RCTL_EN | RCTL_BAM | RCTL_SECRC);

        // TX ring: empty (head == tail), descriptors filled per transmit.
        for i in 0..TX_DESCS {
            driver.write_desc(TX_RING_OFF, i, 0, 0, 0, 0);
        }
        let tx_ring = driver.arena + TX_RING_OFF;
        driver.bus.write32(TDBAL, tx_ring as u32);
        driver.bus.write32(TDBAH, (tx_ring >> 32) as u32);
        driver.bus.write32(TDLEN, (TX_DESCS * 16) as u32);
        driver.bus.write32(TDH, 0);
        driver.bus.write32(TDT, 0);
        driver
            .bus
            .write32(TCTL, TCTL_EN | TCTL_PSP | (0x0F << 4) | (0x3F << 12));
        driver.bus.write32(TIPG, 10 | 8 << 10 | 6 << 20);

        Ok(driver)
    }

    /// Direct access to the bus — the test's window into the device model.
    pub fn bus_mut(&mut self) -> &mut B {
        &mut self.bus
    }

    /// One legacy descriptor (16 bytes), packed explicitly — the layout is
    /// the chip's contract, not Rust's.
    fn write_desc(&mut self, ring_off: u64, i: usize, addr: u64, len: u16, cmd: u8, status: u8) {
        let desc = self.bus.mem(self.arena + ring_off + (i * 16) as u64, 16);
        desc[0..8].copy_from_slice(&addr.to_le_bytes());
        desc[8..10].copy_from_slice(&len.to_le_bytes());
        desc[10] = 0; // cso / checksum
        desc[11] = cmd;
        desc[12] = status;
        desc[13] = 0; // css / errors
        desc[14..16].copy_from_slice(&[0, 0]);
    }

    fn desc_status(&mut self, ring_off: u64, i: usize) -> u8 {
        self.bus.mem(self.arena + ring_off + (i * 16) as u64, 16)[12]
    }

    fn desc_len(&mut self, ring_off: u64, i: usize) -> u16 {
        let desc = self.bus.mem(self.arena + ring_off + (i * 16) as u64, 16);
        u16::from_le_bytes([desc[8], desc[9]])
    }
}

/// MAC discovery: the receive-address registers when the chip (or QEMU) has
/// them populated after reset, else three EEPROM words through EERD.
fn read_mac<B: E1000Bus>(bus: &mut B) -> Option<MacAddr> {
    let ral = bus.read32(RAL0);
    let rah = bus.read32(RAH0);
    if ral != 0 {
        let r = ral.to_le_bytes();
        let h = rah.to_le_bytes();
        return Some([r[0], r[1], r[2], r[3], h[0], h[1]]);
    }
    let mut mac = [0u8; 6];
    for word in 0..3u32 {
        bus.write32(EERD, EERD_START | word << 8);
        let mut spins = 0;
        loop {
            let v = bus.read32(EERD);
            if v & EERD_DONE != 0 {
                let data = (v >> 16) as u16;
                mac[word as usize * 2] = (data & 0xFF) as u8;
                mac[word as usize * 2 + 1] = (data >> 8) as u8;
                break;
            }
            spins += 1;
            if spins > 100_000 {
                return None;
            }
        }
    }
    if mac == [0; 6] {
        None
    } else {
        Some(mac)
    }
}

impl<B: E1000Bus> NetDevice for E1000<B> {
    fn mac(&self) -> MacAddr {
        self.mac
    }

    /// Hand one frame to the chip: copy into the descriptor's buffer, arm the
    /// descriptor (EOP | IFCS | RS), publish it by bumping the tail register.
    fn transmit(&mut self, frame: Vec<u8>) {
        let i = self.next_tx;
        // Reclaim: if the chip was ever given this descriptor, wait (bounded)
        // for its Descriptor-Done bit before overwriting.
        if self.tx_used[i] {
            let mut spins = 0;
            while self.desc_status(TX_RING_OFF, i) & STAT_DD == 0 {
                spins += 1;
                if spins > 1_000_000 {
                    return; // ring wedged; drop rather than hang the kernel
                }
            }
        }
        let len = frame.len().min(BUF_SIZE);
        let buf_phys = self.arena + TX_BUFS_OFF + (i * BUF_SIZE) as u64;
        self.bus.mem(buf_phys, len).copy_from_slice(&frame[..len]);
        self.write_desc(TX_RING_OFF, i, buf_phys, len as u16, TXD_CMD, 0);
        self.tx_used[i] = true;
        // The descriptor and payload writes must be visible before the tail
        // bump makes the chip look at them.
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        self.next_tx = (i + 1) % TX_DESCS;
        self.bus.write32(TDT, self.next_tx as u32);
    }

    /// Drain one received frame, if the chip has finished one: copy it out,
    /// then return the descriptor to the hardware by advancing the tail.
    fn receive(&mut self) -> Option<Vec<u8>> {
        let i = self.next_rx;
        if self.desc_status(RX_RING_OFF, i) & STAT_DD == 0 {
            return None;
        }
        let len = self.desc_len(RX_RING_OFF, i) as usize;
        let buf_phys = self.arena + RX_BUFS_OFF + (i * BUF_SIZE) as u64;
        let frame = self.bus.mem(buf_phys, len.min(BUF_SIZE)).to_vec();
        // Rearm the descriptor and give it back to the chip.
        self.write_desc(RX_RING_OFF, i, buf_phys, 0, 0, 0);
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);
        self.bus.write32(RDT, i as u32);
        self.next_rx = (i + 1) % RX_DESCS;
        Some(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ARENA: u64 = 0x10_0000; // pretend the arena sits at 1 MiB physical

    /// A software model of the 82540EM: a register file plus flat "physical"
    /// memory, with just enough behavior to catch a wrong driver — RST
    /// self-clears (and re-seeds RAL/RAH like QEMU's reset does), EERD
    /// completes with EEPROM data, and a TDT bump walks the TX ring exactly
    /// like the DMA engine (reads the descriptor, captures the payload, sets
    /// Descriptor-Done, advances the head).
    struct Model {
        regs: alloc::collections::BTreeMap<u32, u32>,
        ram: alloc::vec::Vec<u8>,
        mac: MacAddr,
        sent: alloc::vec::Vec<alloc::vec::Vec<u8>>,
    }

    impl Model {
        fn new(mac: MacAddr) -> Self {
            let mut model = Self {
                regs: alloc::collections::BTreeMap::new(),
                ram: alloc::vec![0; ARENA_BYTES as usize],
                mac,
                sent: alloc::vec::Vec::new(),
            };
            model.seed_ra();
            model
        }

        fn seed_ra(&mut self) {
            let m = self.mac;
            self.regs
                .insert(RAL0, u32::from_le_bytes([m[0], m[1], m[2], m[3]]));
            self.regs
                .insert(RAH0, u32::from(m[4]) | u32::from(m[5]) << 8);
        }

        fn desc(&self, ring_off: u64, i: usize) -> &[u8] {
            let at = (ring_off + (i * 16) as u64) as usize;
            &self.ram[at..at + 16]
        }

        fn desc_mut(&mut self, ring_off: u64, i: usize) -> &mut [u8] {
            let at = (ring_off + (i * 16) as u64) as usize;
            &mut self.ram[at..at + 16]
        }

        /// The DMA engine: consume TX descriptors from head to the new tail.
        fn run_tx(&mut self, new_tail: u32) {
            let mut head = *self.regs.get(&TDH).unwrap_or(&0) as usize;
            while head != new_tail as usize {
                let d = self.desc(TX_RING_OFF, head);
                let addr = u64::from_le_bytes(d[0..8].try_into().unwrap());
                let len = u16::from_le_bytes([d[8], d[9]]) as usize;
                assert_eq!(d[11], TXD_CMD, "driver must set EOP|IFCS|RS");
                let start = (addr - ARENA) as usize;
                self.sent.push(self.ram[start..start + len].to_vec());
                self.desc_mut(TX_RING_OFF, head)[12] |= STAT_DD;
                head = (head + 1) % TX_DESCS;
            }
            self.regs.insert(TDH, head as u32);
        }

        /// Deliver a frame into the RX ring the way the chip would: payload
        /// into the descriptor's buffer, length + DD into the descriptor.
        fn inject_rx(&mut self, i: usize, frame: &[u8]) {
            let d = self.desc(RX_RING_OFF, i);
            let addr = u64::from_le_bytes(d[0..8].try_into().unwrap());
            let start = (addr - ARENA) as usize;
            self.ram[start..start + frame.len()].copy_from_slice(frame);
            let len = (frame.len() as u16).to_le_bytes();
            let desc = self.desc_mut(RX_RING_OFF, i);
            desc[8..10].copy_from_slice(&len);
            desc[12] |= STAT_DD;
        }
    }

    impl E1000Bus for Model {
        fn read32(&mut self, reg: u32) -> u32 {
            *self.regs.get(&reg).unwrap_or(&0)
        }
        fn write32(&mut self, reg: u32, value: u32) {
            match reg {
                CTRL if value & CTRL_RST != 0 => {
                    // Reset self-clears and re-seeds the receive address,
                    // exactly like QEMU's e1000 reset path.
                    self.regs.insert(CTRL, value & !CTRL_RST);
                    self.seed_ra();
                }
                EERD if value & EERD_START != 0 => {
                    let word = (value >> 8 & 0xFF) as usize;
                    let m = self.mac;
                    let data = u32::from(m[word * 2]) | u32::from(m[word * 2 + 1]) << 8;
                    self.regs.insert(EERD, EERD_DONE | data << 16);
                }
                TDT => {
                    self.regs.insert(TDT, value);
                    self.run_tx(value);
                }
                _ => {
                    self.regs.insert(reg, value);
                }
            }
        }
        fn mem(&mut self, phys: u64, len: usize) -> &mut [u8] {
            let at = (phys - ARENA) as usize;
            &mut self.ram[at..at + len]
        }
    }

    const MAC: MacAddr = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];

    #[test]
    fn init_brings_the_chip_up_correctly() {
        let mut nic = E1000::new(Model::new(MAC), ARENA).unwrap();
        assert_eq!(nic.mac(), MAC);
        let bus = nic.bus_mut();
        assert_ne!(bus.read32(RCTL) & RCTL_EN, 0, "receiver enabled");
        assert_ne!(bus.read32(RCTL) & RCTL_BAM, 0, "broadcast accepted (ARP)");
        assert_ne!(bus.read32(TCTL) & TCTL_EN, 0, "transmitter enabled");
        assert_eq!(bus.read32(RDLEN), (RX_DESCS * 16) as u32);
        assert_eq!(bus.read32(TDLEN), (TX_DESCS * 16) as u32);
        assert_eq!(
            bus.read32(RDT),
            (RX_DESCS - 1) as u32,
            "whole RX ring owned by hardware"
        );
        assert_eq!(bus.read32(TDT), 0, "TX ring empty");
        assert_ne!(bus.read32(RAH0) & RAH_AV, 0, "unicast filter valid");
        assert_ne!(bus.read32(CTRL) & CTRL_SLU, 0, "link forced up");
    }

    #[test]
    fn mac_falls_back_to_eeprom_when_ra_is_cold() {
        let mut model = Model::new(MAC);
        model.regs.insert(RAL0, 0);
        model.regs.insert(RAH0, 0);
        // Suppress the reset re-seed by pre-clearing after construction: the
        // driver resets first, so make reset leave RA cold too.
        struct ColdRa(Model);
        impl E1000Bus for ColdRa {
            fn read32(&mut self, reg: u32) -> u32 {
                if reg == RAL0 || reg == RAH0 {
                    return 0;
                }
                self.0.read32(reg)
            }
            fn write32(&mut self, reg: u32, value: u32) {
                self.0.write32(reg, value);
            }
            fn mem(&mut self, phys: u64, len: usize) -> &mut [u8] {
                self.0.mem(phys, len)
            }
        }
        let nic = E1000::new(ColdRa(model), ARENA).unwrap();
        assert_eq!(nic.mac(), MAC, "EERD path must recover the MAC");
    }

    #[test]
    fn transmit_walks_the_ring_and_reclaims_descriptors() {
        let mut nic = E1000::new(Model::new(MAC), ARENA).unwrap();
        // More frames than TX descriptors: forces reclaim of DD'd slots.
        for n in 0..(TX_DESCS * 2 + 3) {
            let frame = alloc::vec![n as u8; 64 + n];
            nic.transmit(frame);
        }
        let sent = &nic.bus_mut().sent;
        assert_eq!(sent.len(), TX_DESCS * 2 + 3);
        for (n, frame) in sent.iter().enumerate() {
            assert_eq!(frame.len(), 64 + n);
            assert!(frame.iter().all(|&b| b == n as u8));
        }
    }

    #[test]
    fn receive_drains_dd_descriptors_and_returns_them_to_hw() {
        let mut nic = E1000::new(Model::new(MAC), ARENA).unwrap();
        assert_eq!(nic.receive(), None, "empty ring reads as no frame");
        nic.bus_mut().inject_rx(0, b"first-frame");
        nic.bus_mut().inject_rx(1, b"second");
        assert_eq!(nic.receive().as_deref(), Some(&b"first-frame"[..]));
        assert_eq!(nic.receive().as_deref(), Some(&b"second"[..]));
        assert_eq!(nic.receive(), None);
        // Both descriptors were handed back: RDT chased the consumer.
        assert_eq!(nic.bus_mut().read32(RDT), 1);
    }

    #[test]
    fn the_whole_net_stack_runs_over_the_driver() {
        // The point of the boundary: NetStack works over the e1000 exactly as
        // it does over loopback. Deliver an ARP request through the model's
        // RX ring (as the wire would); the stack must parse it off the driver,
        // learn the sender, and answer out the TX ring.
        use crate::net::NetStack;
        let mut nic = E1000::new(Model::new(MAC), ARENA).unwrap();
        // hand-built frame: broadcast ARP who-has 10.0.2.15? tell 10.0.2.2
        let gw_mac = [0x52, 0x55, 0x0A, 0x00, 0x02, 0x02];
        let mut frame = alloc::vec![0xFF; 6]; // broadcast dst
        frame.extend_from_slice(&gw_mac);
        frame.extend_from_slice(&[0x08, 0x06]); // ethertype ARP
        frame.extend_from_slice(&[
            0x00, 0x01, 0x08, 0x00, 6, 4, 0x00, 0x01, // htype/ptype/hlen/plen/op
        ]);
        frame.extend_from_slice(&gw_mac); // sender MAC (slirp gateway)
        frame.extend_from_slice(&[10, 0, 2, 2]); // sender IP
        frame.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // target MAC (unknown)
        frame.extend_from_slice(&[10, 0, 2, 15]); // target IP
        nic.bus_mut().inject_rx(0, &frame);

        let mut stack = NetStack::new([10, 0, 2, 15], alloc::boxed::Box::new(nic));
        assert_eq!(stack.mac, MAC);
        stack.poll_until_quiet();
        assert_eq!(stack.stats.rx_frames, 1, "frame came off the RX ring");
        assert_eq!(stack.stats.arp_replies, 1, "stack answered the ARP");
        assert_eq!(stack.stats.tx_frames, 1, "reply went out the TX ring");
        assert_eq!(stack.arp_len(), 1, "gateway learned into the ARP cache");
    }
}
