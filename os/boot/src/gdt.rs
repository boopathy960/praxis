//! Ring 3 — the GDT, the TSS, and the `syscall`/`sysret` fast path.
//!
//! Isolation needs two hardware facts the earlier phases did not set up: a
//! **privilege boundary** (user code runs at ring 3, unable to touch kernel
//! memory or execute privileged instructions) and a **controlled door** back
//! into the kernel (the `syscall` instruction, which jumps to a fixed kernel
//! entry point at ring 0). This module builds both from scratch:
//!
//!   * a **GDT** with kernel code/data (ring 0) and user code/data (ring 3)
//!     segments plus a **TSS** (so a `syscall`/interrupt from ring 3 switches
//!     to a known-good kernel stack, `RSP0`);
//!   * the **`syscall`/`sysret` MSRs** — `IA32_STAR` (the segment selectors),
//!     `IA32_LSTAR` (the entry point), `IA32_FMASK` (flags cleared on entry),
//!     and `IA32_EFER.SCE` (which enables the instruction at all);
//!   * a naked **entry stub** that lands the trap, calls the Rust dispatcher,
//!     and `sysret`s back to ring 3.
//!
//! This is `x86_64-unknown-none` code exercised under QEMU. The pieces that a
//! full user-process launch needs (`enter_user_mode`) are provided and
//! compile-checked; wiring the first ring-3 process is the QEMU bring-up step,
//! hence `allow(dead_code)` on the not-yet-called paths.

#![allow(dead_code)]

use core::arch::{asm, naked_asm};

// Segment selectors (index << 3 | RPL). Order in the GDT below matters:
// syscall/sysret derive the user selectors from IA32_STAR by fixed offsets.
pub const KERNEL_CODE: u16 = 0x08; // index 1, ring 0
pub const KERNEL_DATA: u16 = 0x10; // index 2, ring 0
pub const USER_CODE: u16 = 0x1B; // index 3, ring 3 (RPL 3)
pub const USER_DATA: u16 = 0x23; // index 4, ring 3

/// The 64-bit TSS. Only the ring-0 stack pointer (`rsp0`) matters for our use:
/// it is the stack the CPU switches to when entering the kernel from ring 3.
#[repr(C, packed)]
struct Tss {
    reserved0: u32,
    rsp0: u64,
    rsp1: u64,
    rsp2: u64,
    reserved1: u64,
    ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    iomap_base: u16,
}

impl Tss {
    const fn empty() -> Self {
        Self {
            reserved0: 0,
            rsp0: 0,
            rsp1: 0,
            rsp2: 0,
            reserved1: 0,
            ist: [0; 7],
            reserved2: 0,
            reserved3: 0,
            iomap_base: core::mem::size_of::<Tss>() as u16,
        }
    }
}

static mut TSS: Tss = Tss::empty();

/// The GDT: null, kernel code, kernel data, user code, user data, then a
/// 16-byte TSS descriptor (two slots). Entries are the classic 64-bit
/// long-mode descriptors.
static mut GDT: [u64; 7] = [0; 7];

const KERNEL_STACK_SIZE: usize = 64 * 1024;
static mut KERNEL_STACK: [u8; KERNEL_STACK_SIZE] = [0; KERNEL_STACK_SIZE];

#[repr(C, packed)]
struct DescriptorPointer {
    limit: u16,
    base: u64,
}

/// Build and load the GDT + TSS, and set the ring-0 stack the CPU uses on a
/// privilege change.
pub fn init() {
    unsafe {
        let gdt = &mut *core::ptr::addr_of_mut!(GDT);
        // Code/data segment descriptors (long mode: base/limit ignored).
        gdt[1] = segment(true, false); // kernel code (ring 0)
        gdt[2] = segment(false, false); // kernel data (ring 0)
        gdt[3] = segment(true, true); // user code (ring 3)
        gdt[4] = segment(false, true); // user data (ring 3)

        // Point the TSS at a fresh kernel stack (top of the array).
        let stack_top = core::ptr::addr_of!(KERNEL_STACK) as u64 + KERNEL_STACK_SIZE as u64;
        (*core::ptr::addr_of_mut!(TSS)).rsp0 = stack_top;

        // The 16-byte TSS descriptor spans gdt[5] and gdt[6].
        let tss_base = core::ptr::addr_of!(TSS) as u64;
        let (low, high) = tss_descriptor(tss_base, core::mem::size_of::<Tss>() as u32 - 1);
        gdt[5] = low;
        gdt[6] = high;

        let pointer = DescriptorPointer {
            limit: (core::mem::size_of::<[u64; 7]>() - 1) as u16,
            base: gdt.as_ptr() as u64,
        };
        asm!("lgdt [{}]", in(reg) &pointer, options(readonly, nostack, preserves_flags));

        // Load the TSS selector (index 5).
        asm!("ltr {0:x}", in(reg) 5u16 << 3, options(nostack, preserves_flags));

        reload_segments();
    }
}

/// A long-mode code/data segment descriptor.
fn segment(code: bool, user: bool) -> u64 {
    // Present | S (code/data) | type. Long-mode flag (L) set for code.
    let mut access: u64 = 0b1001_0010; // present, S, writable data
    if code {
        access = 0b1001_1010; // present, S, executable, readable
    }
    if user {
        access |= 0b0110_0000; // DPL = 3
    }
    let mut descriptor = access << 40;
    if code {
        descriptor |= 1 << 53; // L: 64-bit code segment
    }
    descriptor
}

/// The two 8-byte halves of a 64-bit TSS system descriptor.
fn tss_descriptor(base: u64, limit: u32) -> (u64, u64) {
    let mut low: u64 = 0;
    low |= (limit as u64) & 0xFFFF;
    low |= (base & 0xFF_FFFF) << 16;
    low |= 0b1000_1001u64 << 40; // present, type = 64-bit available TSS
    low |= ((limit as u64 >> 16) & 0xF) << 48;
    low |= ((base >> 24) & 0xFF) << 56;
    let high = (base >> 32) & 0xFFFF_FFFF;
    (low, high)
}

/// Reload CS via a far return and the data segments directly.
unsafe fn reload_segments() {
    unsafe {
        asm!(
            "push {sel}",
            "lea {tmp}, [rip + 2f]",
            "push {tmp}",
            "retfq",
            "2:",
            sel = in(reg) KERNEL_CODE as u64,
            tmp = lateout(reg) _,
            options(preserves_flags),
        );
        asm!(
            "mov ds, {0:x}",
            "mov es, {0:x}",
            "mov ss, {0:x}",
            in(reg) KERNEL_DATA,
            options(nostack, preserves_flags),
        );
    }
}

// ── syscall/sysret MSRs ─────────────────────────────────────────────────

const IA32_EFER: u32 = 0xC000_0080;
const IA32_STAR: u32 = 0xC000_0081;
const IA32_LSTAR: u32 = 0xC000_0082;
const IA32_FMASK: u32 = 0xC000_0084;

unsafe fn wrmsr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    unsafe {
        asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high, options(nostack, preserves_flags));
    }
}

unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    unsafe {
        asm!("rdmsr", in("ecx") msr, out("eax") low, out("edx") high, options(nostack, preserves_flags));
    }
    (u64::from(high) << 32) | u64::from(low)
}

/// Enable the `syscall`/`sysret` fast path and point it at [`syscall_entry`].
pub fn init_syscalls() {
    unsafe {
        // EFER.SCE (bit 0) enables the syscall instruction.
        let efer = rdmsr(IA32_EFER);
        wrmsr(IA32_EFER, efer | 1);

        // STAR: [63:48] user base selectors, [47:32] kernel base selectors.
        // On syscall the CPU loads CS = kernel_code, SS = kernel_code+8; on
        // sysret CS = user_base+16, SS = user_base+8 — which is why the GDT
        // order is kernel-code, kernel-data, user-*.
        let star: u64 = ((KERNEL_CODE as u64) << 32) | ((USER_CODE as u64 - 16) << 48);
        wrmsr(IA32_STAR, star);

        // LSTAR: the entry point the syscall instruction jumps to.
        wrmsr(IA32_LSTAR, syscall_entry as *const () as u64);

        // FMASK: clear IF (and DF/TF) on entry so the kernel runs with
        // interrupts off until it chooses otherwise.
        wrmsr(IA32_FMASK, 0x0000_0300);
    }
}

/// A dedicated ring-0 stack for the `syscall` trap. `syscall` (unlike an
/// interrupt) does NOT switch RSP, so on entry we are still on the *user*
/// stack; the stub swaps to the top of this before touching memory, so the
/// kernel never trusts a user-controlled stack pointer.
const SYSCALL_STACK_SIZE: usize = 32 * 1024;
#[repr(C, align(16))]
struct SyscallStack([u8; SYSCALL_STACK_SIZE]);
static mut SYSCALL_STACK: SyscallStack = SyscallStack([0; SYSCALL_STACK_SIZE]);
/// Where the stub parks the user RSP while the kernel handler runs (single
/// syscall in flight — the kernel is not re-entrant here).
static mut USER_RSP_SAVE: u64 = 0;

/// The `syscall` landing pad. On entry `rcx` = user return rip, `r11` = user
/// rflags (the CPU stashes them there), and RSP is still the *user* stack.
///
/// We switch to the kernel syscall stack, call the Rust dispatcher with the
/// SysV argument registers (number in rdi, the user's rsi/rdx as args 2/3),
/// then return to ring 3 via **`iretq`** rather than `sysretq`: iretq reads the
/// GDT and restores CS/SS/RSP/RFLAGS/RIP with proper privilege checks against
/// the correctly-ordered user descriptors, sidestepping sysret's STAR-layout
/// constraints entirely.
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_entry() {
    naked_asm!(
        // Park the user stack, switch to the kernel syscall stack top.
        "mov [rip + {user_rsp}], rsp",
        "lea rsp, [rip + {kstack} + {ksize}]",
        // Preserve the values iretq will need (user rip + rflags).
        "push rcx", // user return rip
        "push r11", // user rflags
        // Dispatch: rax = syscall number → rdi (arg1); rsi/rdx pass through.
        "mov rdi, rax",
        "call {handler}",
        // rax now holds the handler's return value → hand back to the user in
        // rax. Rebuild the iretq frame: SS, RSP, RFLAGS, CS, RIP.
        "pop r11",  // user rflags
        "pop rcx",  // user rip
        "push {user_ss}",
        "mov rdx, [rip + {user_rsp}]",
        "push rdx", // user rsp
        "push r11", // user rflags
        "push {user_cs}",
        "push rcx", // user rip
        "iretq",
        user_rsp = sym USER_RSP_SAVE,
        kstack = sym SYSCALL_STACK,
        ksize = const SYSCALL_STACK_SIZE,
        handler = sym syscall_handler_c,
        user_ss = const USER_DATA as u64,
        user_cs = const USER_CODE as u64,
    );
}

/// The Rust side of the trap: `number` = rax, `arg0` = the user's rsi, `arg1`
/// = the user's rdx. Dispatches to the boot crate's userland handler (which
/// owns the console and the resume point). An `EXIT` never returns here — it
/// longjmps back into the kernel — so any return value only matters for calls
/// that resume the user program.
extern "C" fn syscall_handler_c(number: u64, arg0: u64, arg1: u64) -> u64 {
    crate::usermode::handle_syscall(number, arg0, arg1)
}

/// Drop to ring 3: `iretq` into user code with a user stack. Provided for the
/// QEMU bring-up of the first user process.
///
/// # Safety
/// `entry` and `stack` must be valid user-mapped addresses in the active
/// address space (which must also map the kernel higher half).
pub unsafe fn enter_user_mode(entry: u64, stack: u64) -> ! {
    unsafe {
        asm!(
            "push {ss}",       // user SS
            "push {rsp}",      // user RSP
            "push 0x202",      // rflags with IF set
            "push {cs}",       // user CS
            "push {rip}",      // user RIP
            "iretq",
            ss = in(reg) USER_DATA as u64,
            rsp = in(reg) stack,
            cs = in(reg) USER_CODE as u64,
            rip = in(reg) entry,
            options(noreturn),
        );
    }
}
