//! virtio-blk — a from-scratch paravirtual block driver.
//!
//! ATA speaks to real (emulated) IDE hardware; virtio is the *modern*
//! path — the paravirtual interface every hypervisor (QEMU, KVM, Firecracker,
//! cloud) offers, where the driver and the device cooperate through a shared
//! **split virtqueue** in memory instead of port I/O. This implements that
//! ring protocol faithfully and puts a block device behind it, so Praxis can
//! talk to the storage a modern VM actually exposes.
//!
//! The split virtqueue is three shared structures: a **descriptor table** (each
//! entry an address/length/flags/next), an **available ring** the driver
//! appends request-head indices to, and a **used ring** the device appends
//! completions to. A virtio-blk request is a 3-descriptor chain: a header
//! (type + sector, device reads it), a data buffer (device reads on write /
//! writes on read), and a 1-byte status (device writes it).
//!
//! As with the other drivers the ring logic is portable and the single
//! hardware touch — "notify the device that the queue advanced" — goes through
//! the [`VirtioTransport`] trait, so the whole protocol is unit-tested on the
//! host against a software device model that drains the queue and serves the
//! backing store.

use crate::block::{BlockDevice, BlockError, BLOCK_SIZE};

/// Number of descriptors in the queue (a power of two, per the spec).
pub const QUEUE_SIZE: usize = 8;

// Descriptor flags.
const VIRTQ_DESC_F_NEXT: u16 = 1; // chained: `next` is valid
const VIRTQ_DESC_F_WRITE: u16 = 2; // device writes into this buffer

// virtio-blk request types.
const VIRTIO_BLK_T_IN: u32 = 0; // read from device
const VIRTIO_BLK_T_OUT: u32 = 1; // write to device
const VIRTIO_BLK_S_OK: u8 = 0;

/// The DMA layout offsets within the shared arena (all within one page-ish
/// region; addresses handed to the device are `arena + offset`).
const DESC_OFF: usize = 0; // 16 bytes * QUEUE_SIZE
const AVAIL_OFF: usize = 0x100; // flags,idx,ring[QUEUE_SIZE]
/// The used-ring offset. The driver learns completions via
/// [`VirtioTransport::used_idx`] rather than reading the ring directly, so this
/// is referenced by the device side (and the test model), not the driver.
#[allow(dead_code)]
const USED_OFF: usize = 0x200; // flags,idx,ring[QUEUE_SIZE]*8
const HEADER_OFF: usize = 0x300; // 16-byte request header
const DATA_OFF: usize = 0x400; // one sector
const STATUS_OFF: usize = 0x800; // 1 byte
pub const ARENA_LEN: usize = 0x1000;

/// The hardware boundary: the shared DMA arena (both sides read/write it) and
/// the doorbell that tells the device the available ring advanced.
pub trait VirtioTransport {
    /// The shared arena as bytes (driver and device see the same memory).
    fn arena(&mut self) -> &mut [u8];
    /// Notify the device that queue `q`'s available ring has new entries.
    fn notify(&mut self, q: u16);
    /// The device's current `used.idx` — advances when a request completes.
    fn used_idx(&self) -> u16;
}

/// A virtio-blk device behind [`BlockDevice`], driving one split virtqueue.
pub struct VirtioBlk<T: VirtioTransport> {
    transport: T,
    capacity_sectors: u64,
    avail_idx: u16,
    last_used: u16,
}

impl<T: VirtioTransport> VirtioBlk<T> {
    /// Wrap a transport whose device reports `capacity_sectors` 512-byte
    /// sectors of backing storage.
    pub fn new(transport: T, capacity_sectors: u64) -> Self {
        Self {
            transport,
            capacity_sectors,
            avail_idx: 0,
            last_used: 0,
        }
    }

    /// Direct transport access — the test's window on the device model.
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    /// Run one block request (read or write) through the virtqueue and return
    /// the device's status byte.
    fn request(&mut self, write: bool, sector: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        if buf.len() != BLOCK_SIZE {
            return Err(BlockError::BadLength);
        }
        if sector >= self.capacity_sectors {
            return Err(BlockError::OutOfRange);
        }
        let arena = self.transport.arena();

        // Request header: type, reserved, sector.
        let rtype = if write {
            VIRTIO_BLK_T_OUT
        } else {
            VIRTIO_BLK_T_IN
        };
        arena[HEADER_OFF..HEADER_OFF + 4].copy_from_slice(&rtype.to_le_bytes());
        arena[HEADER_OFF + 4..HEADER_OFF + 8].copy_from_slice(&0u32.to_le_bytes());
        arena[HEADER_OFF + 8..HEADER_OFF + 16].copy_from_slice(&sector.to_le_bytes());
        // For a write, stage the data now; for a read the device fills it.
        if write {
            arena[DATA_OFF..DATA_OFF + BLOCK_SIZE].copy_from_slice(buf);
        }
        arena[STATUS_OFF] = 0xFF; // sentinel: device overwrites with status

        // Three-descriptor chain: header (r) → data (r on write / w on read)
        // → status (w).
        write_desc(
            arena,
            0,
            DESC_OFF,
            HEADER_OFF as u64,
            16,
            VIRTQ_DESC_F_NEXT,
            1,
        );
        let data_flags = VIRTQ_DESC_F_NEXT | if write { 0 } else { VIRTQ_DESC_F_WRITE };
        write_desc(
            arena,
            1,
            DESC_OFF,
            DATA_OFF as u64,
            BLOCK_SIZE as u32,
            data_flags,
            2,
        );
        write_desc(
            arena,
            2,
            DESC_OFF,
            STATUS_OFF as u64,
            1,
            VIRTQ_DESC_F_WRITE,
            0,
        );

        // Publish the head (descriptor 0) into the available ring.
        let ring_slot = (self.avail_idx as usize) % QUEUE_SIZE;
        let slot_off = AVAIL_OFF + 4 + ring_slot * 2;
        arena[slot_off..slot_off + 2].copy_from_slice(&0u16.to_le_bytes()); // head = desc 0
        self.avail_idx = self.avail_idx.wrapping_add(1);
        arena[AVAIL_OFF + 2..AVAIL_OFF + 4].copy_from_slice(&self.avail_idx.to_le_bytes());

        // Doorbell, then wait (bounded) for the used ring to advance.
        self.transport.notify(0);
        let mut spins = 0;
        while self.transport.used_idx() == self.last_used {
            spins += 1;
            if spins > 1_000_000 {
                return Err(BlockError::OutOfRange); // device wedged
            }
        }
        self.last_used = self.transport.used_idx();

        let arena = self.transport.arena();
        if arena[STATUS_OFF] != VIRTIO_BLK_S_OK {
            return Err(BlockError::OutOfRange); // device reported an error
        }
        if !write {
            buf.copy_from_slice(&arena[DATA_OFF..DATA_OFF + BLOCK_SIZE]);
        }
        Ok(())
    }
}

/// Write one 16-byte split-virtqueue descriptor at index `i`.
fn write_desc(arena: &mut [u8], i: usize, base: usize, addr: u64, len: u32, flags: u16, next: u16) {
    let off = base + i * 16;
    arena[off..off + 8].copy_from_slice(&addr.to_le_bytes());
    arena[off + 8..off + 12].copy_from_slice(&len.to_le_bytes());
    arena[off + 12..off + 14].copy_from_slice(&flags.to_le_bytes());
    arena[off + 14..off + 16].copy_from_slice(&next.to_le_bytes());
}

impl<T: VirtioTransport> BlockDevice for VirtioBlk<T> {
    fn num_blocks(&self) -> u64 {
        self.capacity_sectors
    }
    fn read_block(&self, _index: u64, _buf: &mut [u8]) -> Result<(), BlockError> {
        // A virtio read mutates shared ring state, so it needs `&mut self`;
        // the BlockDevice read is `&self`. Callers that own the driver use
        // `read`/`write` below; the trait read is intentionally unsupported to
        // keep the &self contract honest rather than fake interior mutability.
        Err(BlockError::OutOfRange)
    }
    fn write_block(&mut self, index: u64, buf: &[u8]) -> Result<(), BlockError> {
        let mut tmp = [0u8; BLOCK_SIZE];
        tmp.copy_from_slice(buf);
        self.request(true, index, &mut tmp)
    }
}

impl<T: VirtioTransport> VirtioBlk<T> {
    /// Read sector `index` (the `&mut` counterpart to the trait's `&self`
    /// read, which the virtqueue can't satisfy).
    pub fn read(&mut self, index: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        self.request(false, index, buf)
    }
    /// Write sector `index`.
    pub fn write(&mut self, index: u64, buf: &[u8]) -> Result<(), BlockError> {
        let mut tmp = [0u8; BLOCK_SIZE];
        if buf.len() != BLOCK_SIZE {
            return Err(BlockError::BadLength);
        }
        tmp.copy_from_slice(buf);
        self.request(true, index, &mut tmp)
    }
}

/// A software virtio-blk device: a backing store plus the virtqueue engine a
/// hypervisor runs — on `notify` it walks the descriptor chain the driver
/// published, serves the read/write, writes the status, and bumps the used
/// ring. It's the reference "other end" of the protocol, used both by the
/// tests and by the live [`shell_selftest`], so the driver is exercised the
/// same way in the running kernel as under test.
pub struct ModelDevice {
    arena: alloc::vec::Vec<u8>,
    store: alloc::vec::Vec<u8>,
    used: u16,
    seen_avail: u16,
}

impl ModelDevice {
    #[must_use]
    pub fn new(sectors: u64) -> Self {
        Self {
            arena: alloc::vec![0u8; ARENA_LEN],
            store: alloc::vec![0u8; sectors as usize * BLOCK_SIZE],
            used: 0,
            seen_avail: 0,
        }
    }

    fn read_desc(&self, i: usize) -> (u64, u32, u16, u16) {
        let off = DESC_OFF + i * 16;
        let a = &self.arena;
        (
            u64::from_le_bytes(a[off..off + 8].try_into().unwrap()),
            u32::from_le_bytes(a[off + 8..off + 12].try_into().unwrap()),
            u16::from_le_bytes(a[off + 12..off + 14].try_into().unwrap()),
            u16::from_le_bytes(a[off + 14..off + 16].try_into().unwrap()),
        )
    }

    fn run(&mut self) {
        let avail_idx =
            u16::from_le_bytes(self.arena[AVAIL_OFF + 2..AVAIL_OFF + 4].try_into().unwrap());
        while self.seen_avail != avail_idx {
            let slot = (self.seen_avail as usize) % QUEUE_SIZE;
            let soff = AVAIL_OFF + 4 + slot * 2;
            let head = u16::from_le_bytes(self.arena[soff..soff + 2].try_into().unwrap()) as usize;
            self.serve(head);
            self.seen_avail = self.seen_avail.wrapping_add(1);
            self.used = self.used.wrapping_add(1);
            self.arena[USED_OFF + 2..USED_OFF + 4].copy_from_slice(&self.used.to_le_bytes());
        }
    }

    fn serve(&mut self, head: usize) {
        let (haddr, _, _, hnext) = self.read_desc(head);
        let h = haddr as usize;
        let rtype = u32::from_le_bytes(self.arena[h..h + 4].try_into().unwrap());
        let sector = u64::from_le_bytes(self.arena[h + 8..h + 16].try_into().unwrap()) as usize;
        let (daddr, dlen, _, dnext) = self.read_desc(hnext as usize);
        let d = daddr as usize;
        let dlen = dlen as usize;
        let base = sector * BLOCK_SIZE;
        match rtype {
            VIRTIO_BLK_T_IN => {
                self.arena[d..d + dlen].copy_from_slice(&self.store[base..base + dlen]);
            }
            VIRTIO_BLK_T_OUT => {
                let data = self.arena[d..d + dlen].to_vec();
                self.store[base..base + dlen].copy_from_slice(&data);
            }
            _ => {}
        }
        let (saddr, _, _, _) = self.read_desc(dnext as usize);
        self.arena[saddr as usize] = VIRTIO_BLK_S_OK;
    }
}

impl VirtioTransport for ModelDevice {
    fn arena(&mut self) -> &mut [u8] {
        &mut self.arena
    }
    fn notify(&mut self, _q: u16) {
        self.run(); // a real device processes asynchronously; we do it now
    }
    fn used_idx(&self) -> u16 {
        self.used
    }
}

/// Live self-test: round-trip one sector through the split virtqueue against a
/// [`ModelDevice`]. Wired to the `virtio selftest` shell command so the driver
/// runs in the kernel, not only in the host test suite.
pub fn shell_selftest() -> Result<(), BlockError> {
    let mut blk = VirtioBlk::new(ModelDevice::new(64), 64);
    let mut sector = [0u8; BLOCK_SIZE];
    for (i, b) in sector.iter_mut().enumerate() {
        *b = (i * 7 % 251) as u8;
    }
    blk.write(5, &sector)?;
    let mut back = [0u8; BLOCK_SIZE];
    blk.read(5, &mut back)?;
    if back == sector {
        Ok(())
    } else {
        Err(BlockError::BadLength)
    }
}

#[cfg(test)]
mod tests {
    use super::ModelDevice as Device;
    use super::*;

    #[test]
    fn reports_capacity() {
        let dev = VirtioBlk::new(Device::new(2048), 2048);
        assert_eq!(dev.num_blocks(), 2048);
        assert_eq!(dev.capacity_bytes(), 1024 * 1024);
    }

    #[test]
    fn a_sector_written_through_the_virtqueue_reads_back() {
        let mut dev = VirtioBlk::new(Device::new(64), 64);
        let mut sector = [0u8; BLOCK_SIZE];
        for (i, b) in sector.iter_mut().enumerate() {
            *b = (i * 3 % 251) as u8;
        }
        dev.write(9, &sector).unwrap();
        let mut back = [0u8; BLOCK_SIZE];
        dev.read(9, &mut back).unwrap();
        assert_eq!(sector, back, "round-trip through the split virtqueue");
        // A different, untouched sector is still zero.
        dev.read(10, &mut back).unwrap();
        assert!(back.iter().all(|&b| b == 0));
    }

    #[test]
    fn many_requests_advance_the_rings_correctly() {
        // More requests than QUEUE_SIZE: the available/used indices must wrap
        // and stay in lockstep.
        let mut dev = VirtioBlk::new(Device::new(64), 64);
        for s in 0..20u64 {
            let sector = [(s as u8).wrapping_mul(7); BLOCK_SIZE];
            dev.write(s % 64, &sector).unwrap();
            let mut back = [0u8; BLOCK_SIZE];
            dev.read(s % 64, &mut back).unwrap();
            assert_eq!(back, sector, "request {s} round-tripped");
        }
    }

    #[test]
    fn bounds_and_length_are_enforced() {
        let mut dev = VirtioBlk::new(Device::new(4), 4);
        let sector = [0u8; BLOCK_SIZE];
        assert_eq!(dev.write(4, &sector), Err(BlockError::OutOfRange));
        assert_eq!(dev.write(0, &[0u8; 10]), Err(BlockError::BadLength));
        let mut small = [0u8; 10];
        assert_eq!(dev.read(0, &mut small), Err(BlockError::BadLength));
    }
}
