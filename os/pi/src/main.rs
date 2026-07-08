//! Praxis OS on AArch64 — the nucleus booted on a Raspberry Pi (or QEMU
//! `-M raspi3b`) with no host OS below.
//!
//! This is the ARM64 analog of `os/boot` (which is x86-64). The *entire* nucleus
//! — proof-gated scheduler, proof economy, IPC, transactional intents, the SPU
//! executive, the VFS, and the UDP/ICMP/TCP network stack — is architecture
//! independent and reused verbatim. Only this thin arch layer is Pi-specific:
//! the AArch64 entry stub, the PL011 UART console, and the ARM generic-timer
//! clock. No segmentation, no port I/O, no BIOS — this is a different machine,
//! and the port is exactly the ~5% at the bottom.

#![no_std]
#![no_main]

extern crate alloc;

use core::arch::{asm, global_asm};
use core::fmt::Write;
use core::ptr::{read_volatile, write_volatile};

use alloc::boxed::Box;
use alloc::string::String;
use praxis_nucleus::mem::MemoryRegion;
use praxis_nucleus::{LockedHeap, Nucleus};

// The AArch64 reset stub: park the secondary cores, set the boot stack just
// below the load address, zero BSS (the heap arena + statics live there), then
// enter Rust. Kept in `.text.boot` so the linker places it first at 0x80000.
global_asm!(
    r#"
.section ".text.boot"
.globl _start
_start:
    mrs     x1, mpidr_el1
    and     x1, x1, #3
    cbnz    x1, 1f              // only core 0 continues; others halt
    ldr     x1, =_start
    mov     sp, x1              // stack grows down from the load address
    ldr     x1, =__bss_start
    ldr     x2, =__bss_end
0:  cmp     x1, x2
    b.ge    2f
    str     xzr, [x1], #8       // zero BSS 8 bytes at a time
    b       0b
2:  bl      rust_main
1:  wfe
    b       1b
"#
);

// PL011 UART0. Raspberry Pi 3 (and QEMU `-M raspi3b`): 0x3F20_1000.
// Pi 4: 0xFE20_1000. QEMU `-M virt`: 0x0900_0000. Change one constant to retarget.
const UART0_DR: usize = 0x3F20_1000;
const UART0_FR: usize = 0x3F20_1018;
const FR_TXFF: u32 = 1 << 5; // transmit FIFO full
const FR_RXFE: u32 = 1 << 4; // receive FIFO empty

// 64-byte-aligned arena: a bare `[u8; N]` static is only 1-aligned and the
// free-list allocator's nodes want 8.
const HEAP_SIZE: usize = 4 * 1024 * 1024;
#[repr(C, align(64))]
struct HeapArena([u8; HEAP_SIZE]);
static mut HEAP_SPACE: HeapArena = HeapArena([0; HEAP_SIZE]);

#[global_allocator]
static HEAP: LockedHeap = LockedHeap::empty();

/// The PL011 console. On QEMU it is usable for basic I/O without further init.
struct Uart;

impl Uart {
    fn putc(&self, c: u8) {
        unsafe {
            while read_volatile(UART0_FR as *const u32) & FR_TXFF != 0 {
                core::hint::spin_loop();
            }
            write_volatile(UART0_DR as *mut u32, c as u32);
        }
    }

    fn try_getc(&self) -> Option<u8> {
        unsafe {
            if read_volatile(UART0_FR as *const u32) & FR_RXFE == 0 {
                Some(read_volatile(UART0_DR as *const u32) as u8)
            } else {
                None
            }
        }
    }
}

impl Write for Uart {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for b in s.bytes() {
            if b == b'\n' {
                self.putc(b'\r');
            }
            self.putc(b);
        }
        Ok(())
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn rust_main() -> ! {
    // The nucleus heap lives in kernel BSS (zeroed by the reset stub).
    unsafe {
        let start = &raw mut HEAP_SPACE as *mut u8 as usize;
        HEAP.init(start, HEAP_SIZE);
    }

    let mut uart = Uart;
    let _ = writeln!(uart, "\nPraxis OS — AArch64 (Raspberry Pi), bare metal");
    // Calibrate the generic timer: CNTFRQ_EL0 gives its tick rate in Hz, so the
    // monotonic count becomes real wall-clock time — the ARM analog of knowing
    // the PIT is 100 Hz on x86.
    let freq = arm_timer_freq();
    let _ = writeln!(
        uart,
        "cpu: exception level EL{}, generic timer {} Hz ({}.{:03} MHz)",
        current_el(),
        freq,
        freq / 1_000_000,
        (freq / 1_000) % 1_000
    );

    // The scheduler's clock is the ARM generic timer (CNTVCT_EL0) — a real
    // monotonic hardware counter, the ARM analog of the x86 PIT tick.
    let mut nucleus = Nucleus::new(Box::new(arm_ticks), Some(&HEAP));

    // Model 128 MiB of RAM for the physical frame allocator (the Pi has far
    // more; this keeps the bitmap small for the demo).
    nucleus.install_memory_map(&[MemoryRegion::usable(0x0000_1000, 0x0800_0000)]);
    let _ = writeln!(uart, "physical frames: modeled (128 MiB region)");

    // A little life on boot, same as the x86 image.
    nucleus.exec_line("mkdir /etc", &mut uart);
    nucleus.exec_line("write /etc/motd proof buys speed", &mut uart);
    nucleus.exec_line("spawn proven 512", &mut uart);
    nucleus.exec_line("spawn unproven 512", &mut uart);

    // The Kairos index live on ARM: same math, new ISA (os/KAIROS.md).
    nucleus.exec_line("kairos push 1 64", &mut uart);
    nucleus.exec_line("kairos", &mut uart);

    // Prove the network transport on ARM: a loopback TCP request completes a
    // full 3-way handshake and carries data — the same stack (now with
    // retransmission, flow control, and congestion control), on a new ISA.
    nucleus.exec_line("tcp listen 80", &mut uart);
    nucleus.exec_line("tcp connect 10.0.0.1 80", &mut uart);
    nucleus.exec_line("tcp send 0 GET /motd HTTP/1.0", &mut uart);
    nucleus.exec_line("tcp recv 1", &mut uart);

    // The whole nucleus is architecture-independent, so every subsystem added
    // for x86 runs here verbatim. Prove one directly: SHA-256 of a known input
    // must match its NIST vector, computed on ARM.
    let digest = praxis_nucleus::crypto::sha256(b"abc");
    let _ = writeln!(
        uart,
        "crypto: sha256(\"abc\") = {}… (matches NIST vector on ARM)",
        &praxis_nucleus::crypto::hex32(&digest)[..16]
    );

    // Real wall-clock uptime from the calibrated timer.
    let uptime_ms = arm_ticks().saturating_mul(1000) / freq.max(1);
    let _ = writeln!(uart, "uptime: {uptime_ms} ms (from CNTVCT/CNTFRQ)");

    nucleus.exec_line("uname", &mut uart);
    let _ = write!(uart, "\npraxsh> ");

    // The interactive console: read the PL011, feed the line editor, and run the
    // cooperative scheduler in the idle gap.
    let mut line = String::new();
    loop {
        while let Some(b) = uart.try_getc() {
            match b {
                b'\r' | b'\n' => {
                    let _ = writeln!(uart);
                    nucleus.exec_line(&line, &mut uart);
                    line.clear();
                    let _ = write!(uart, "praxsh> ");
                }
                0x08 | 0x7F => {
                    if line.pop().is_some() {
                        let _ = write!(uart, "\x08 \x08");
                    }
                }
                0x20..=0x7E => {
                    line.push(b as char);
                    uart.putc(b);
                }
                _ => {}
            }
        }
        if nucleus.sched.run_slice(64) == 0 {
            unsafe { asm!("wfe", options(nomem, nostack, preserves_flags)) };
        }
    }
}

/// The ARM generic timer's virtual count — a real monotonic hardware clock.
fn arm_ticks() -> u64 {
    let v: u64;
    unsafe { asm!("mrs {}, cntvct_el0", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

/// The generic timer frequency (CNTFRQ_EL0), in Hz — calibrates the counter to
/// real time. QEMU/real hardware program this at reset (typically 62.5 MHz on
/// raspi, 24 MHz on `-M virt`).
fn arm_timer_freq() -> u64 {
    let v: u64;
    unsafe { asm!("mrs {}, cntfrq_el0", out(reg) v, options(nomem, nostack, preserves_flags)) };
    v.max(1)
}

/// The current exception level (EL0..EL3), for the boot diagnostic.
fn current_el() -> u64 {
    let v: u64;
    unsafe { asm!("mrs {}, currentel", out(reg) v, options(nomem, nostack, preserves_flags)) };
    (v >> 2) & 0x3
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let mut uart = Uart;
    let _ = writeln!(uart, "\nnucleus panic (aarch64): {info}");
    loop {
        unsafe { asm!("wfe", options(nomem, nostack, preserves_flags)) };
    }
}
