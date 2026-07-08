//! The block layer — the persistence substrate under the filesystem.
//!
//! Every real OS separates *what* is stored (files, directories) from the
//! dumb fixed-size sectors it is stored on. The [`BlockDevice`] trait is that
//! boundary: read and write fixed-size blocks by index, nothing more. A real
//! NVMe/SATA/virtio driver implements the same trait the [`RamDisk`] here
//! does, so the filesystem above never knows or cares which it is talking to.
//!
//! On the SPU this maps cleanly onto the persistent (non-volatile) tier: a
//! `BlockDevice` backed by the NV substrate is exactly where a hibernated
//! context or a snapshotted filesystem survives power loss.

use alloc::vec::Vec;

/// The standard sector size the filesystem is built on.
pub const BLOCK_SIZE: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockError {
    OutOfRange,
    BadLength,
}

/// A fixed-geometry block device: `num_blocks` sectors of [`BLOCK_SIZE`] bytes.
pub trait BlockDevice {
    fn num_blocks(&self) -> u64;

    /// Read block `index` into `buf` (must be exactly [`BLOCK_SIZE`]).
    fn read_block(&self, index: u64, buf: &mut [u8]) -> Result<(), BlockError>;

    /// Write `buf` (must be exactly [`BLOCK_SIZE`]) to block `index`.
    fn write_block(&mut self, index: u64, buf: &[u8]) -> Result<(), BlockError>;

    fn capacity_bytes(&self) -> u64 {
        self.num_blocks() * BLOCK_SIZE as u64
    }
}

/// Boxed devices are devices: the nucleus stores its disk as
/// `Box<dyn BlockDevice>` so bare metal can swap a real ATA drive in.
impl<T: BlockDevice + ?Sized> BlockDevice for alloc::boxed::Box<T> {
    fn num_blocks(&self) -> u64 {
        (**self).num_blocks()
    }
    fn read_block(&self, index: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        (**self).read_block(index, buf)
    }
    fn write_block(&mut self, index: u64, buf: &[u8]) -> Result<(), BlockError> {
        (**self).write_block(index, buf)
    }
}

/// A RAM-backed block device — the test/boot disk, and the model of the SPU's
/// battery-backed-DRAM + NVMe persistent tier.
pub struct RamDisk {
    blocks: Vec<[u8; BLOCK_SIZE]>,
    reads: u64,
    writes: u64,
}

impl RamDisk {
    pub fn new(num_blocks: u64) -> Self {
        Self {
            blocks: alloc::vec![[0u8; BLOCK_SIZE]; num_blocks as usize],
            reads: 0,
            writes: 0,
        }
    }

    pub fn io_counts(&self) -> (u64, u64) {
        (self.reads, self.writes)
    }
}

impl BlockDevice for RamDisk {
    fn num_blocks(&self) -> u64 {
        self.blocks.len() as u64
    }

    fn read_block(&self, index: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        if buf.len() != BLOCK_SIZE {
            return Err(BlockError::BadLength);
        }
        let block = self
            .blocks
            .get(index as usize)
            .ok_or(BlockError::OutOfRange)?;
        buf.copy_from_slice(block);
        Ok(())
    }

    fn write_block(&mut self, index: u64, buf: &[u8]) -> Result<(), BlockError> {
        if buf.len() != BLOCK_SIZE {
            return Err(BlockError::BadLength);
        }
        let block = self
            .blocks
            .get_mut(index as usize)
            .ok_or(BlockError::OutOfRange)?;
        block.copy_from_slice(buf);
        Ok(())
    }
}

// RamDisk tracks IO through interior counters; the trait methods take &self /
// &mut self, so bump the counters via a small wrapper on the concrete type.
impl RamDisk {
    pub fn read_counted(&mut self, index: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        self.reads += 1;
        self.read_block(index, buf)
    }
    pub fn write_counted(&mut self, index: u64, buf: &[u8]) -> Result<(), BlockError> {
        self.writes += 1;
        self.write_block(index, buf)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_a_block() {
        let mut disk = RamDisk::new(64);
        assert_eq!(disk.capacity_bytes(), 64 * 512);
        let mut wr = [0u8; BLOCK_SIZE];
        wr[0] = 0xAB;
        wr[BLOCK_SIZE - 1] = 0xCD;
        disk.write_block(10, &wr).unwrap();
        let mut rd = [0u8; BLOCK_SIZE];
        disk.read_block(10, &mut rd).unwrap();
        assert_eq!(rd[0], 0xAB);
        assert_eq!(rd[BLOCK_SIZE - 1], 0xCD);
        // A different block is still zero.
        disk.read_block(11, &mut rd).unwrap();
        assert!(rd.iter().all(|&b| b == 0));
    }

    #[test]
    fn rejects_out_of_range_and_bad_length() {
        let mut disk = RamDisk::new(4);
        let buf = [0u8; BLOCK_SIZE];
        assert_eq!(disk.write_block(4, &buf), Err(BlockError::OutOfRange));
        let short = [0u8; 16];
        assert_eq!(disk.write_block(0, &short), Err(BlockError::BadLength));
    }
}
