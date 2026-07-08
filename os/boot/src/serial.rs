//! A from-scratch 16550 UART driver (COM1) — the kernel's console.
//!
//! Port I/O via inline asm; no external crates. Under QEMU, `-serial stdio`
//! turns this into the interactive praxsh terminal.

use core::arch::asm;
use core::fmt;

const COM1: u16 = 0x3F8;

unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    value
}

pub struct SerialPort;

impl SerialPort {
    /// Initialize COM1: 38400 baud, 8N1, FIFOs on.
    pub fn init() -> Self {
        unsafe {
            outb(COM1 + 1, 0x00); // disable interrupts (we poll)
            outb(COM1 + 3, 0x80); // DLAB on
            outb(COM1, 0x03); // divisor lo (38400 baud)
            outb(COM1 + 1, 0x00); // divisor hi
            outb(COM1 + 3, 0x03); // 8 bits, no parity, one stop; DLAB off
            outb(COM1 + 2, 0xC7); // FIFO on, clear, 14-byte threshold
            outb(COM1 + 4, 0x0B); // RTS/DSR set
        }
        Self
    }

    fn transmit_ready(&self) -> bool {
        unsafe { inb(COM1 + 5) & 0x20 != 0 }
    }

    pub fn write_byte(&mut self, byte: u8) {
        while !self.transmit_ready() {
            core::hint::spin_loop();
        }
        unsafe { outb(COM1, byte) };
    }

    /// Non-blocking read: `Some(byte)` if the receive FIFO has data.
    pub fn try_read(&mut self) -> Option<u8> {
        let has_data = unsafe { inb(COM1 + 5) } & 0x01 != 0;
        if has_data {
            Some(unsafe { inb(COM1) })
        } else {
            None
        }
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.write_byte(b'\r');
            }
            self.write_byte(byte);
        }
        Ok(())
    }
}
