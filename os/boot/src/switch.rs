//! Context switching and address-space switching — the arch-specific enactment
//! of what `nucleus::process` decides as policy.
//!
//! `switch_context` saves the current callee-saved registers + stack pointer
//! into one `Context` and restores them from another: the classic cooperative
//! kernel-thread switch. `load_address_space` writes a process's page-table
//! root into `CR3`, which is the instant one process's virtual memory replaces
//! another's — the hardware act of isolation the paging code set up.
//!
//! Both are naked/`asm!` routines for `x86_64-unknown-none`; they are exercised
//! by booting under QEMU, not by the host test suite (they touch real CPU
//! state). The field order here MUST match `nucleus::process::Context`.
//!
//! `switch_context`/`load_address_space` are the enactment primitives the
//! preemptive scheduler calls once each process's address space also maps the
//! kernel (a shared higher-half); until that bring-up step they are provided
//! and validated-to-compile but not yet invoked at boot, hence `allow(dead_code)`.

#![allow(dead_code)]

use core::arch::asm;

/// The saved-register layout, matching `nucleus::process::Context` field order:
/// rsp, rbp, rbx, r12, r13, r14, r15, rip.
#[repr(C)]
pub struct Context {
    pub rsp: u64,
    pub rbp: u64,
    pub rbx: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
}

/// Save the current execution context into `*prev` and resume `*next`. On the
/// next time `*prev` is restored, execution continues right after this call.
///
/// # Safety
/// `prev` and `next` must point to valid `Context`s; `next` must describe a
/// runnable stack/instruction pointer (freshly built or previously saved here).
#[unsafe(naked)]
pub unsafe extern "C" fn switch_context(prev: *mut Context, next: *const Context) {
    // SysV: rdi = prev, rsi = next.
    core::arch::naked_asm!(
        // Save callee-saved registers of the outgoing context.
        "mov [rdi + 0x00], rsp",
        "mov [rdi + 0x08], rbp",
        "mov [rdi + 0x10], rbx",
        "mov [rdi + 0x18], r12",
        "mov [rdi + 0x20], r13",
        "mov [rdi + 0x28], r14",
        "mov [rdi + 0x30], r15",
        // Save the return address as the outgoing rip.
        "mov rax, [rsp]",
        "mov [rdi + 0x38], rax",
        // Restore the incoming context.
        "mov rsp, [rsi + 0x00]",
        "mov rbp, [rsi + 0x08]",
        "mov rbx, [rsi + 0x10]",
        "mov r12, [rsi + 0x18]",
        "mov r13, [rsi + 0x20]",
        "mov r14, [rsi + 0x28]",
        "mov r15, [rsi + 0x30]",
        // Jump into the incoming rip (overwriting the return slot).
        "mov rax, [rsi + 0x38]",
        "mov [rsp], rax",
        "ret",
    );
}

/// Switch the active virtual address space by loading `cr3` (a page-table root
/// physical address). Every subsequent memory access is translated through the
/// new process's page tables — hardware-enforced isolation, one instruction.
///
/// # Safety
/// `cr3` must be the physical address of a valid, fully-populated PML4 that
/// maps at least the kernel's own code/stack, or the next fetch faults.
pub unsafe fn load_address_space(cr3: u64) {
    unsafe {
        asm!("mov cr3, {}", in(reg) cr3, options(nostack, preserves_flags));
    }
}

/// Read the current `CR3` (the active page-table root) — useful for bring-up
/// logging and to seed the kernel's own address space.
pub fn current_cr3() -> u64 {
    let cr3: u64;
    unsafe {
        asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
    }
    cr3
}
