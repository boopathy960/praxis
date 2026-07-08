//! Interrupts, from scratch — the IDT, CPU exception handlers, the 8259 PIC,
//! the PIT timer tick, and the PS/2 keyboard. No `x86_64` crate; every
//! descriptor, port, and handler is built here.
//!
//! This is the piece that turns the cooperative nucleus into a real kernel: a
//! hardware timer now *interrupts* running code (the substrate of preemption),
//! and a keyboard IRQ delivers real console input. The proof-tier scheduler's
//! philosophy still holds — proven code is trusted to cooperate — but the
//! timer tick is the hardware that lets the kernel *enforce* a slice on code
//! that is not.
//!
//! Note: this module compiles for `x86_64-unknown-none` and is exercised by
//! booting under QEMU (see `os/README.md`); it is not part of the host test
//! suite because it manipulates real CPU state.

use core::arch::asm;
use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};

/// Monotonic tick count, incremented by the PIT on every timer IRQ. This is
/// the bare-metal clock the nucleus scheduler reads.
pub static TICKS: AtomicU64 = AtomicU64::new(0);

/// The last scancode seen by the keyboard IRQ (0 = none pending). A real
/// driver would ring-buffer these; the boot shell polls this cell.
pub static LAST_SCANCODE: AtomicU8 = AtomicU8::new(0);

// ── port I/O ─────────────────────────────────────────────────────────────

unsafe fn outb(port: u16, value: u8) {
    asm!("out dx, al", in("dx") port, in("al") value, options(nomem, nostack, preserves_flags));
}

unsafe fn inb(port: u16) -> u8 {
    let value: u8;
    asm!("in al, dx", in("dx") port, out("al") value, options(nomem, nostack, preserves_flags));
    value
}

/// A short delay by writing to an unused port (the classic PIC settle wait).
unsafe fn io_wait() {
    outb(0x80, 0);
}

// ── the Interrupt Descriptor Table ───────────────────────────────────────

#[repr(C, packed)]
#[derive(Clone, Copy)]
struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    zero: u32,
}

impl IdtEntry {
    const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            zero: 0,
        }
    }

    fn set_handler(&mut self, handler: u64) {
        self.offset_low = handler as u16;
        self.offset_mid = (handler >> 16) as u16;
        self.offset_high = (handler >> 32) as u32;
        self.selector = read_cs();
        self.ist = 0;
        // present | DPL 0 | 64-bit interrupt gate (0x8E).
        self.type_attr = 0x8E;
        self.zero = 0;
    }
}

#[repr(C, packed)]
struct IdtPointer {
    limit: u16,
    base: u64,
}

const IDT_LEN: usize = 256;
static mut IDT: [IdtEntry; IDT_LEN] = [IdtEntry::missing(); IDT_LEN];

fn read_cs() -> u16 {
    let cs: u16;
    unsafe { asm!("mov {0:x}, cs", out(reg) cs, options(nomem, nostack, preserves_flags)) };
    cs
}

/// The PIC maps IRQ 0/1 to vectors 32/33 after remap (below).
const TIMER_VECTOR: usize = 32;
const KEYBOARD_VECTOR: usize = 33;

/// Build the IDT and load it. Installs the CPU exception handlers plus the
/// timer and keyboard IRQ handlers.
pub fn init_idt() {
    unsafe {
        let idt = &mut *core::ptr::addr_of_mut!(IDT);
        idt[0].set_handler(divide_by_zero as *const () as u64);
        idt[6].set_handler(invalid_opcode as *const () as u64);
        idt[8].set_handler(double_fault as *const () as u64);
        idt[13].set_handler(general_protection as *const () as u64);
        idt[14].set_handler(page_fault as *const () as u64);
        idt[TIMER_VECTOR].set_handler(timer_interrupt as *const () as u64);
        idt[KEYBOARD_VECTOR].set_handler(keyboard_interrupt as *const () as u64);

        let pointer = IdtPointer {
            limit: (core::mem::size_of::<[IdtEntry; IDT_LEN]>() - 1) as u16,
            base: idt.as_ptr() as u64,
        };
        asm!("lidt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags));
    }
}

// ── the 8259 PIC: remap IRQs out of the CPU exception range ───────────────

const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;
const PIC_EOI: u8 = 0x20;

/// Remap the master/slave PICs to vectors 32..48 so hardware IRQs never
/// collide with CPU exceptions (0..31), then mask all but timer + keyboard.
pub fn init_pic() {
    unsafe {
        // Start init (ICW1), cascade mode.
        outb(PIC1_CMD, 0x11);
        io_wait();
        outb(PIC2_CMD, 0x11);
        io_wait();
        // ICW2: vector offsets 32 (master), 40 (slave).
        outb(PIC1_DATA, 32);
        io_wait();
        outb(PIC2_DATA, 40);
        io_wait();
        // ICW3: master/slave wiring on IRQ2.
        outb(PIC1_DATA, 4);
        io_wait();
        outb(PIC2_DATA, 2);
        io_wait();
        // ICW4: 8086 mode.
        outb(PIC1_DATA, 0x01);
        io_wait();
        outb(PIC2_DATA, 0x01);
        io_wait();
        // Mask everything except IRQ0 (timer) and IRQ1 (keyboard).
        outb(PIC1_DATA, 0b1111_1100);
        outb(PIC2_DATA, 0b1111_1111);
    }
}

/// Program the PIT (channel 0) to a periodic rate. `hz` interrupts/second.
pub fn init_timer(hz: u32) {
    let divisor = (1_193_182 / hz.max(1)) as u16;
    unsafe {
        outb(0x43, 0x36); // channel 0, lo/hi byte, mode 3 (square wave)
        outb(0x40, (divisor & 0xFF) as u8);
        outb(0x40, (divisor >> 8) as u8);
    }
}

/// Enable maskable interrupts (`sti`).
pub fn enable() {
    unsafe { asm!("sti", options(nomem, nostack, preserves_flags)) };
}

unsafe fn send_eoi(vector: usize) {
    if vector >= 40 {
        outb(PIC2_CMD, PIC_EOI);
    }
    outb(PIC1_CMD, PIC_EOI);
}

// ── handlers ─────────────────────────────────────────────────────────────
//
// The `x86-interrupt` ABI is still nightly-gated, so the ISRs are built from
// scratch as **naked functions** (stable since 1.88): each saves the
// caller-saved GP registers, 16-byte-aligns the stack, calls a plain Rust
// handler, restores, and returns with `iretq`. CPU exceptions here never
// return (they halt), so they only need to align-and-call.

/// A returning IRQ handler (timer, keyboard): save volatile state, 16-byte
/// align, call the Rust body, restore, `iretq`.
macro_rules! irq {
    ($name:ident, $body:path) => {
        #[unsafe(naked)]
        extern "C" fn $name() {
            core::arch::naked_asm!(
                "push rax", "push rcx", "push rdx", "push rsi", "push rdi",
                "push r8", "push r9", "push r10", "push r11",
                // 16-byte-align the stack for the SysV call, preserving old rsp.
                "mov rax, rsp",
                "and rsp, -16",
                "push rax",
                "push rax",
                "call {body}",
                "pop rax",
                "pop rsp",
                "pop r11", "pop r10", "pop r9", "pop r8",
                "pop rdi", "pop rsi", "pop rdx", "pop rcx", "pop rax",
                "iretq",
                body = sym $body,
            )
        }
    };
}

/// A fatal CPU exception ISR: align, announce, and halt forever. Never
/// returns, so it needs no register save/restore or error-code cleanup.
macro_rules! fault {
    ($name:ident, $report:ident, $msg:literal) => {
        extern "C" fn $report() {
            report_exception($msg);
        }
        #[unsafe(naked)]
        extern "C" fn $name() {
            core::arch::naked_asm!(
                "and rsp, -16",
                "call {body}",
                "2:",
                "hlt",
                "jmp 2b",
                body = sym $report,
            );
        }
    };
}

extern "C" fn on_timer() {
    TICKS.fetch_add(1, Ordering::Relaxed);
    unsafe { send_eoi(TIMER_VECTOR) };
}

extern "C" fn on_keyboard() {
    let scancode = unsafe { inb(0x60) };
    LAST_SCANCODE.store(scancode, Ordering::Relaxed);
    unsafe { send_eoi(KEYBOARD_VECTOR) };
}

irq!(timer_interrupt, on_timer);
irq!(keyboard_interrupt, on_keyboard);

fault!(divide_by_zero, report_divide, "divide by zero");
fault!(invalid_opcode, report_opcode, "invalid opcode");
fault!(general_protection, report_gp, "general protection fault");
fault!(page_fault, report_pf, "page fault");
fault!(double_fault, report_df, "double fault");

fn report_exception(what: &str) {
    // Announce on COM1 (we are in interrupt context; a fresh port avoids any
    // lock). The system halts after this returns.
    use core::fmt::Write;
    let mut port = crate::serial::SerialPort::init();
    let _ = writeln!(port, "\nCPU EXCEPTION: {what} — halting");
}

/// Translate a set-1 scancode to an ASCII byte for the common keys, or 0.
/// A real driver tracks shift/ctrl and the full keymap; this covers a US
/// layout well enough to drive the shell. Key-release codes (>= 0x80) map to 0.
pub fn scancode_to_ascii(scancode: u8) -> u8 {
    const MAP: &[u8] = b"\0\x1b1234567890-=\x08\tqwertyuiop[]\n\0asdfghjkl;'`\0\\zxcvbnm,./\0*\0 ";
    let index = scancode as usize;
    if index < MAP.len() {
        MAP[index]
    } else {
        0
    }
}
