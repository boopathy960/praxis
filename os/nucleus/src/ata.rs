//! ATA PIO disk driver — real, durable storage under the filesystem.
//!
//! The last volatile piece of Praxis was the disk: the VFS could snapshot and
//! restore, but only to a RAM-backed block device that died with the power.
//! This driver puts a real IDE/ATA disk behind the same [`BlockDevice`]
//! boundary, so `sync` genuinely survives a power cycle.
//!
//! Same architecture as the e1000 driver: the protocol core is portable and
//! lives in the nucleus, every hardware touch goes through the [`AtaPorts`]
//! trait (8/16-bit port I/O on the legacy channel), and the whole driver is
//! unit-tested on the host against a software model of the drive. The
//! bare-metal boot crate supplies only the four real `in`/`out` instructions.
//!
//! It speaks classic ATA PIO over the primary channel (0x1F0–0x1F7 + the
//! 0x3F6 control port): IDENTIFY DEVICE to size the disk, READ SECTORS
//! (0x20) / WRITE SECTORS (0x30) with 28-bit LBA one sector at a time, and
//! CACHE FLUSH (0xE7) after every write — slow and honest, which is exactly
//! right for a filesystem snapshot a few hundred sectors long.

use core::cell::RefCell;

use crate::block::{BlockDevice, BlockError, BLOCK_SIZE};

// ── the primary-channel register file (offsets from the I/O base) ─────────
const REG_DATA: u16 = 0; // 16-bit data window
const REG_COUNT: u16 = 2; // sector count
const REG_LBA_LO: u16 = 3;
const REG_LBA_MID: u16 = 4;
const REG_LBA_HI: u16 = 5;
const REG_DRIVE: u16 = 6; // drive select + LBA[27:24]
const REG_STATUS: u16 = 7; // read: status, write: command

const STATUS_ERR: u8 = 1 << 0;
const STATUS_DRQ: u8 = 1 << 3;
const STATUS_BSY: u8 = 1 << 7;

const CMD_READ_SECTORS: u8 = 0x20;
const CMD_WRITE_SECTORS: u8 = 0x30;
const CMD_CACHE_FLUSH: u8 = 0xE7;
const CMD_IDENTIFY: u8 = 0xEC;

/// Give up on a stuck drive after this many status polls — a kernel must
/// never hang on hardware that stopped answering.
const MAX_POLLS: u32 = 1_000_000;

/// The hardware boundary: 8- and 16-bit port I/O plus the channel's control
/// port. Bare metal implements this with `in`/`out` instructions; tests
/// implement it over a `Vec<u8>` drive model.
pub trait AtaPorts {
    fn outb(&mut self, port: u16, value: u8);
    fn inb(&mut self, port: u16) -> u8;
    fn outw(&mut self, port: u16, value: u16);
    fn inw(&mut self, port: u16) -> u16;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtaError {
    /// No device answered the IDENTIFY on that position.
    NoDevice,
    /// The device answered but is not ATA (ATAPI/SATA signature).
    NotAta,
    /// The drive set ERR or stopped answering mid-command.
    DriveFault,
}

/// One ATA drive on the legacy primary channel, behind [`BlockDevice`].
///
/// `RefCell` because [`BlockDevice::read_block`] takes `&self` (a RAM disk
/// needs no mutation) while port I/O always mutates bus state — and the
/// kernel is single-threaded, so the runtime borrow can never conflict.
pub struct AtaDrive<P: AtaPorts> {
    ports: RefCell<P>,
    io_base: u16,
    /// 0 = master, 1 = slave.
    drive: u8,
    sectors: u64,
    pub model: [u8; 40],
}

impl<P: AtaPorts> AtaDrive<P> {
    /// Probe `drive` (0 master / 1 slave) on the channel at `io_base` /
    /// `ctrl_base` (0x1F0 / 0x3F6 for the primary). Returns a ready
    /// [`BlockDevice`] or why there isn't one.
    pub fn identify(ports: P, io_base: u16, ctrl_base: u16, drive: u8) -> Result<Self, AtaError> {
        let dev = Self {
            ports: RefCell::new(ports),
            io_base,
            drive,
            sectors: 0,
            model: [b' '; 40],
        };
        {
            let mut p = dev.ports.borrow_mut();
            // Select the drive, settle, zero the address registers.
            p.outb(io_base + REG_DRIVE, 0xA0 | (drive << 4));
            for _ in 0..4 {
                let _ = p.inb(ctrl_base); // ~400 ns settle
            }
            p.outb(io_base + REG_COUNT, 0);
            p.outb(io_base + REG_LBA_LO, 0);
            p.outb(io_base + REG_LBA_MID, 0);
            p.outb(io_base + REG_LBA_HI, 0);
            p.outb(io_base + REG_STATUS, CMD_IDENTIFY);

            if p.inb(io_base + REG_STATUS) == 0 {
                return Err(AtaError::NoDevice); // floating bus: nothing there
            }
            // Wait out BSY, then check the signature: ATAPI/SATA park nonzero
            // values in LBA mid/high.
            let mut polls = 0;
            while p.inb(io_base + REG_STATUS) & STATUS_BSY != 0 {
                polls += 1;
                if polls > MAX_POLLS {
                    return Err(AtaError::DriveFault);
                }
            }
            if p.inb(io_base + REG_LBA_MID) != 0 || p.inb(io_base + REG_LBA_HI) != 0 {
                return Err(AtaError::NotAta);
            }
            let mut polls = 0;
            loop {
                let status = p.inb(io_base + REG_STATUS);
                if status & STATUS_ERR != 0 {
                    return Err(AtaError::DriveFault);
                }
                if status & STATUS_DRQ != 0 {
                    break;
                }
                polls += 1;
                if polls > MAX_POLLS {
                    return Err(AtaError::DriveFault);
                }
            }
        }

        // The 256-word identify block: LBA28 capacity in words 60–61, the
        // model string (byte-swapped per word, per the spec) in words 27–46.
        let mut words = [0u16; 256];
        {
            let mut p = dev.ports.borrow_mut();
            for word in &mut words {
                *word = p.inw(io_base + REG_DATA);
            }
        }
        let sectors = u64::from(words[60]) | u64::from(words[61]) << 16;
        if sectors == 0 {
            return Err(AtaError::NotAta);
        }
        let mut model = [b' '; 40];
        for i in 0..20 {
            let w = words[27 + i];
            model[i * 2] = (w >> 8) as u8;
            model[i * 2 + 1] = (w & 0xFF) as u8;
        }
        Ok(Self {
            sectors,
            model,
            ..dev
        })
    }

    /// The drive's self-reported model string, trimmed.
    pub fn model_str(&self) -> &str {
        core::str::from_utf8(&self.model)
            .unwrap_or("?")
            .trim_ascii()
    }

    /// Select the drive and program a 28-bit LBA + sector count of 1.
    fn setup(&self, p: &mut P, lba: u64) {
        p.outb(
            self.io_base + REG_DRIVE,
            0xE0 | (self.drive << 4) | ((lba >> 24) & 0x0F) as u8,
        );
        p.outb(self.io_base + REG_COUNT, 1);
        p.outb(self.io_base + REG_LBA_LO, (lba & 0xFF) as u8);
        p.outb(self.io_base + REG_LBA_MID, (lba >> 8 & 0xFF) as u8);
        p.outb(self.io_base + REG_LBA_HI, (lba >> 16 & 0xFF) as u8);
    }

    /// Poll until BSY clears and DRQ raises (or the drive faults).
    fn wait_drq(&self, p: &mut P) -> Result<(), BlockError> {
        let mut polls = 0;
        loop {
            let status = p.inb(self.io_base + REG_STATUS);
            if status & STATUS_ERR != 0 {
                return Err(BlockError::OutOfRange); // the drive rejected the LBA
            }
            if status & STATUS_BSY == 0 && status & STATUS_DRQ != 0 {
                return Ok(());
            }
            polls += 1;
            if polls > MAX_POLLS {
                return Err(BlockError::OutOfRange);
            }
        }
    }

    fn wait_not_busy(&self, p: &mut P) {
        let mut polls = 0;
        while p.inb(self.io_base + REG_STATUS) & STATUS_BSY != 0 {
            polls += 1;
            if polls > MAX_POLLS {
                return;
            }
        }
    }
}

impl<P: AtaPorts> BlockDevice for AtaDrive<P> {
    fn num_blocks(&self) -> u64 {
        self.sectors
    }

    fn read_block(&self, index: u64, buf: &mut [u8]) -> Result<(), BlockError> {
        if buf.len() != BLOCK_SIZE {
            return Err(BlockError::BadLength);
        }
        if index >= self.sectors {
            return Err(BlockError::OutOfRange);
        }
        let mut p = self.ports.borrow_mut();
        self.setup(&mut p, index);
        p.outb(self.io_base + REG_STATUS, CMD_READ_SECTORS);
        self.wait_drq(&mut p)?;
        for chunk in buf.chunks_exact_mut(2) {
            let w = p.inw(self.io_base + REG_DATA);
            chunk[0] = (w & 0xFF) as u8;
            chunk[1] = (w >> 8) as u8;
        }
        Ok(())
    }

    fn write_block(&mut self, index: u64, buf: &[u8]) -> Result<(), BlockError> {
        if buf.len() != BLOCK_SIZE {
            return Err(BlockError::BadLength);
        }
        if index >= self.sectors {
            return Err(BlockError::OutOfRange);
        }
        let mut p = self.ports.borrow_mut();
        self.setup(&mut p, index);
        p.outb(self.io_base + REG_STATUS, CMD_WRITE_SECTORS);
        self.wait_drq(&mut p)?;
        for chunk in buf.chunks_exact(2) {
            p.outw(
                self.io_base + REG_DATA,
                u16::from(chunk[0]) | u16::from(chunk[1]) << 8,
            );
        }
        // Durability is the whole point: flush the drive cache every write.
        p.outb(self.io_base + REG_STATUS, CMD_CACHE_FLUSH);
        self.wait_not_busy(&mut p);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::BLOCK_SIZE;

    /// A software model of an ATA drive: a register file, a sector store, and
    /// the DRQ/BSY protocol — enough that a wrong driver fails loudly.
    struct Model {
        store: alloc::vec::Vec<u8>,
        sectors: u64,
        // register latches
        drive: u8,
        count: u8,
        lba: u64,
        // transfer engine
        mode: Mode,
        cursor: usize, // byte offset into the current transfer
        identify: [u16; 256],
        status: u8,
        present_drive: u8, // which drive number answers (master=0/slave=1)
    }

    #[derive(PartialEq)]
    enum Mode {
        Idle,
        Identify,
        Read,
        Write,
    }

    impl Model {
        fn new(sectors: u64, present_drive: u8) -> Self {
            let mut identify = [0u16; 256];
            identify[60] = (sectors & 0xFFFF) as u16;
            identify[61] = (sectors >> 16) as u16;
            let model_text = b"PRAXIS VIRTUAL DISK                     ";
            for i in 0..20 {
                identify[27 + i] =
                    u16::from(model_text[i * 2]) << 8 | u16::from(model_text[i * 2 + 1]);
            }
            Self {
                store: alloc::vec![0u8; (sectors as usize) * BLOCK_SIZE],
                sectors,
                drive: 0,
                count: 0,
                lba: 0,
                mode: Mode::Idle,
                cursor: 0,
                identify,
                status: 0x40, // RDY
                present_drive,
            }
        }

        fn selected_present(&self) -> bool {
            self.drive == self.present_drive
        }

        fn start(&mut self, cmd: u8) {
            if !self.selected_present() {
                self.status = 0;
                return;
            }
            match cmd {
                CMD_IDENTIFY => {
                    self.mode = Mode::Identify;
                    self.cursor = 0;
                    self.status = 0x40 | STATUS_DRQ;
                }
                CMD_READ_SECTORS => {
                    if self.lba >= self.sectors {
                        self.status = 0x40 | STATUS_ERR;
                        return;
                    }
                    self.mode = Mode::Read;
                    self.cursor = 0;
                    self.status = 0x40 | STATUS_DRQ;
                }
                CMD_WRITE_SECTORS => {
                    if self.lba >= self.sectors {
                        self.status = 0x40 | STATUS_ERR;
                        return;
                    }
                    self.mode = Mode::Write;
                    self.cursor = 0;
                    self.status = 0x40 | STATUS_DRQ;
                }
                CMD_CACHE_FLUSH => {
                    self.status = 0x40;
                }
                _ => self.status = 0x40 | STATUS_ERR,
            }
        }
    }

    impl AtaPorts for Model {
        fn outb(&mut self, port: u16, value: u8) {
            match port {
                0x1F2 => self.count = value,
                0x1F3 => self.lba = (self.lba & !0xFF) | u64::from(value),
                0x1F4 => self.lba = (self.lba & !0xFF00) | u64::from(value) << 8,
                0x1F5 => self.lba = (self.lba & !0xFF_0000) | u64::from(value) << 16,
                0x1F6 => {
                    self.drive = (value >> 4) & 1;
                    self.lba = (self.lba & 0xFF_FFFF) | u64::from(value & 0x0F) << 24;
                }
                0x1F7 => self.start(value),
                _ => {}
            }
        }
        fn inb(&mut self, port: u16) -> u8 {
            match port {
                0x1F7 => {
                    if self.selected_present() {
                        self.status
                    } else {
                        0
                    }
                }
                0x1F4 | 0x1F5 => 0, // ATA signature
                0x3F6 => self.status,
                _ => 0,
            }
        }
        fn outw(&mut self, port: u16, value: u16) {
            if port == 0x1F0 && self.mode == Mode::Write {
                let at = (self.lba as usize) * BLOCK_SIZE + self.cursor;
                self.store[at] = (value & 0xFF) as u8;
                self.store[at + 1] = (value >> 8) as u8;
                self.cursor += 2;
                if self.cursor >= BLOCK_SIZE {
                    self.mode = Mode::Idle;
                    self.status = 0x40; // DRQ drops when the sector is in
                }
            }
        }
        fn inw(&mut self, port: u16) -> u16 {
            if port != 0x1F0 {
                return 0;
            }
            match self.mode {
                Mode::Identify => {
                    let w = self.identify[self.cursor / 2];
                    self.cursor += 2;
                    if self.cursor >= 512 {
                        self.mode = Mode::Idle;
                        self.status = 0x40;
                    }
                    w
                }
                Mode::Read => {
                    let at = (self.lba as usize) * BLOCK_SIZE + self.cursor;
                    let w = u16::from(self.store[at]) | u16::from(self.store[at + 1]) << 8;
                    self.cursor += 2;
                    if self.cursor >= BLOCK_SIZE {
                        self.mode = Mode::Idle;
                        self.status = 0x40;
                    }
                    w
                }
                _ => 0,
            }
        }
    }

    #[test]
    fn identify_finds_the_drive_and_its_geometry() {
        let drive = AtaDrive::identify(Model::new(2048, 1), 0x1F0, 0x3F6, 1).unwrap();
        assert_eq!(drive.num_blocks(), 2048);
        assert_eq!(drive.capacity_bytes(), 1024 * 1024);
        assert_eq!(drive.model_str(), "PRAXIS VIRTUAL DISK");
    }

    #[test]
    fn absent_drive_reports_no_device_not_a_hang() {
        // Probing the slave position when only the master exists.
        assert_eq!(
            AtaDrive::identify(Model::new(2048, 0), 0x1F0, 0x3F6, 1).err(),
            Some(AtaError::NoDevice)
        );
    }

    #[test]
    fn sectors_round_trip_through_the_port_protocol() {
        let mut drive = AtaDrive::identify(Model::new(64, 0), 0x1F0, 0x3F6, 0).unwrap();
        let mut sector = [0u8; BLOCK_SIZE];
        for (i, b) in sector.iter_mut().enumerate() {
            *b = (i % 251) as u8;
        }
        drive.write_block(7, &sector).unwrap();
        let mut back = [0u8; BLOCK_SIZE];
        drive.read_block(7, &mut back).unwrap();
        assert_eq!(sector, back);
        // A different sector is still zero — no smearing.
        drive.read_block(8, &mut back).unwrap();
        assert!(back.iter().all(|&b| b == 0));
    }

    #[test]
    fn bounds_and_length_are_enforced() {
        let mut drive = AtaDrive::identify(Model::new(8, 0), 0x1F0, 0x3F6, 0).unwrap();
        let sector = [0u8; BLOCK_SIZE];
        assert_eq!(drive.write_block(8, &sector), Err(BlockError::OutOfRange));
        assert_eq!(drive.write_block(0, &[0u8; 10]), Err(BlockError::BadLength));
        let mut small = [0u8; 10];
        assert_eq!(drive.read_block(0, &mut small), Err(BlockError::BadLength));
    }

    #[test]
    fn the_filesystem_survives_a_power_cycle_on_ata() {
        // The point of the driver: snapshot the VFS to the (model) disk, drop
        // everything, restore from the same sectors — files intact.
        let mut drive = AtaDrive::identify(Model::new(2048, 1), 0x1F0, 0x3F6, 1).unwrap();
        let mut fs = crate::fs::Vfs::new();
        fs.mkdir("/boot").unwrap();
        fs.write_all("/boot/count", b"41").unwrap();
        fs.mkdir("/etc").unwrap();
        fs.write_all("/etc/motd", b"praxis persists").unwrap();
        fs.snapshot(&mut drive).unwrap();

        // "Power cycle": keep only the drive's sector store.
        let model = drive.ports.into_inner();
        let drive2 = AtaDrive::identify(model, 0x1F0, 0x3F6, 1).unwrap();
        let restored = crate::fs::Vfs::restore(&drive2).unwrap();
        assert_eq!(restored.read_all("/boot/count").unwrap(), b"41");
        assert_eq!(restored.read_all("/etc/motd").unwrap(), b"praxis persists");
    }
}
