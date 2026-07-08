//! Bare-metal wiring for the ATA driver: real port I/O under the portable
//! core in `nucleus/src/ata.rs` — four instructions, nothing more.
//!
//! The data disk is the **primary-channel slave** (QEMU:
//! `-drive format=raw,file=praxis-data.img,if=ide,index=1`); the master is
//! the boot image the BIOS loaded us from, which we never touch.

use core::arch::asm;

use praxis_nucleus::ata::{AtaDrive, AtaError, AtaPorts};

pub struct RealPorts;

impl AtaPorts for RealPorts {
    fn outb(&mut self, port: u16, value: u8) {
        unsafe {
            asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
        }
    }
    fn inb(&mut self, port: u16) -> u8 {
        let value: u8;
        unsafe {
            asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
        }
        value
    }
    fn outw(&mut self, port: u16, value: u16) {
        unsafe {
            asm!("out dx, ax", in("dx") port, in("ax") value, options(nomem, nostack, preserves_flags));
        }
    }
    fn inw(&mut self, port: u16) -> u16 {
        let value: u16;
        unsafe {
            asm!("in ax, dx", in("dx") port, out("ax") value, options(nomem, nostack, preserves_flags));
        }
        value
    }
}

/// Probe the primary-slave position for the persistent data disk.
pub fn probe() -> Result<AtaDrive<RealPorts>, AtaError> {
    AtaDrive::identify(RealPorts, 0x1F0, 0x3F6, 1)
}
