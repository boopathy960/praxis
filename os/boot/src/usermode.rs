//! Ring 3 — launching a real user process, for real, on the metal.
//!
//! Everything the privilege boundary needed has existed and compiled for a
//! while (the GDT's ring-3 segments, the TSS, the `syscall`/`sysret` MSRs,
//! `enter_user_mode`); what was missing was the *launch* — the code that
//! actually builds a user address region, drops the CPU to CPL 3, runs
//! instructions there, and services the `syscall` trap they make. This module
//! is that final wiring, and it closes the single biggest gap between Praxis
//! and a general-purpose OS.
//!
//! The approach is the pragmatic, robust one: rather than load a modelled
//! [`AddressSpace`](praxis_nucleus::vmem) (whose page tables live in a
//! `BTreeMap`, not at the physical frames they name), we map the user code and
//! stack straight into the **live bootloader page tables** — walking the real
//! four-level hierarchy through the bootloader's physical-memory window and
//! setting the `USER` bit at *every* level (the classic gotcha: one non-user
//! parent entry and the CPU denies ring-3 access to everything below it).
//!
//! The user program is a hand-assembled sequence that makes two `WRITE`
//! syscalls and one `EXIT`. `WRITE` proves ring-3 code executed and trapped
//! cleanly into the kernel and back; `EXIT` `longjmp`s out of the syscall
//! handler all the way back to [`launch`]'s `setjmp`, so the boot sequence
//! resumes at the interactive shell. The syscall return path is `iretq`
//! (see `gdt::syscall_entry`), which restores ring-3 state through the
//! correctly-ordered GDT descriptors.

use core::fmt::Write;

use praxis_nucleus::abi::{Syscall, SYS_WRITE};
use praxis_nucleus::fs::Vfs;
use praxis_nucleus::mem::{Frame, FrameAllocator};
use praxis_nucleus::{abi, elf};

use crate::gdt;
use crate::serial::SerialPort;

// Page-table entry flags (architectural bit positions).
const PRESENT: u64 = 1 << 0;
const WRITABLE: u64 = 1 << 1;
const USER: u64 = 1 << 2;
const FRAME_MASK: u64 = 0x000F_FFFF_FFFF_F000;

// The user address layout: code at 4 MiB, a one-page stack just below 8 MiB.
const USER_CODE_VADDR: u64 = 0x0040_0000;
const USER_STACK_PAGE: u64 = 0x007F_F000;
const USER_STACK_TOP: u64 = 0x0080_0000;
// Messages live in the code page, past the code, so one frame carries both.
const MSG1_OFF: u64 = 0x200;
const MSG2_OFF: u64 = 0x280;
/// Where the user program's ELF image is stored in the VFS before it is loaded.
const USER_ELF_PATH: &str = "/bin/hello";

/// A `setjmp`/`longjmp` buffer: callee-saved registers + stack + resume rip.
#[repr(C)]
struct JmpBuf {
    rbx: u64,
    rbp: u64,
    r12: u64,
    r13: u64,
    r14: u64,
    r15: u64,
    rsp: u64,
    rip: u64,
}

static mut KERNEL_RESUME: JmpBuf = JmpBuf {
    rbx: 0,
    rbp: 0,
    r12: 0,
    r13: 0,
    r14: 0,
    r15: 0,
    rsp: 0,
    rip: 0,
};

/// Save the current callee-saved state + return point into `buf` and return 0.
/// After a later [`longjmp`] to the same `buf`, execution resumes *here* with
/// the value `longjmp` was given.
#[unsafe(naked)]
unsafe extern "C" fn setjmp(buf: *mut JmpBuf) -> u64 {
    core::arch::naked_asm!(
        "mov [rdi + 0x00], rbx",
        "mov [rdi + 0x08], rbp",
        "mov [rdi + 0x10], r12",
        "mov [rdi + 0x18], r13",
        "mov [rdi + 0x20], r14",
        "mov [rdi + 0x28], r15",
        "lea rax, [rsp + 8]", // caller's rsp (after the pending ret)
        "mov [rdi + 0x30], rax",
        "mov rax, [rsp]", // return address
        "mov [rdi + 0x38], rax",
        "xor eax, eax",
        "ret",
    );
}

/// Restore the state saved in `buf` and resume just after its [`setjmp`],
/// which returns `val` there. Never returns to the caller.
#[unsafe(naked)]
unsafe extern "C" fn longjmp(buf: *const JmpBuf, val: u64) -> ! {
    core::arch::naked_asm!(
        "mov rbx, [rdi + 0x00]",
        "mov rbp, [rdi + 0x08]",
        "mov r12, [rdi + 0x10]",
        "mov r13, [rdi + 0x18]",
        "mov r14, [rdi + 0x20]",
        "mov r15, [rdi + 0x28]",
        "mov rsp, [rdi + 0x30]",
        "mov rax, rsi", // return value for the setjmp site
        "jmp [rdi + 0x38]",
    );
}

/// Map one 4 KiB page `vaddr` → the frame at `phys`, into the *live* page
/// tables rooted at the current CR3. Intermediate tables are created as needed
/// and every entry along the path is made user- and (for the leaf) optionally
/// writable-accessible.
///
/// # Safety
/// Must run with the bootloader's full physical-memory mapping active at
/// `phys_offset`, single-threaded, before the mapped region is used.
unsafe fn map_user(
    phys_offset: u64,
    frames: &mut FrameAllocator,
    vaddr: u64,
    phys: u64,
    writable: bool,
) -> Result<(), &'static str> {
    let idx = [
        ((vaddr >> 39) & 0x1FF) as usize,
        ((vaddr >> 30) & 0x1FF) as usize,
        ((vaddr >> 21) & 0x1FF) as usize,
        ((vaddr >> 12) & 0x1FF) as usize,
    ];
    let cr3 = crate::switch::current_cr3();
    let mut table_phys = cr3 & FRAME_MASK;

    // Walk / build the three upper levels (PML4, PDPT, PD).
    for &index in idx.iter().take(3) {
        let entry_ptr = (phys_offset + table_phys + (index as u64) * 8) as *mut u64;
        let entry = unsafe { core::ptr::read_volatile(entry_ptr) };
        if entry & PRESENT != 0 {
            // Ensure the path permits ring-3 access (OR the bits in, never
            // clearing kernel mappings, whose leaves still lack USER).
            unsafe { core::ptr::write_volatile(entry_ptr, entry | PRESENT | WRITABLE | USER) };
            table_phys = entry & FRAME_MASK;
        } else {
            let new = frames.allocate().ok_or("out of frames for page table")?;
            let new_phys = new.start_addr();
            // Zero the fresh table before linking it in.
            unsafe {
                core::ptr::write_bytes((phys_offset + new_phys) as *mut u8, 0, 4096);
                core::ptr::write_volatile(entry_ptr, new_phys | PRESENT | WRITABLE | USER);
            }
            table_phys = new_phys;
        }
    }

    // The leaf (PT) entry.
    let leaf_ptr = (phys_offset + table_phys + (idx[3] as u64) * 8) as *mut u64;
    let mut flags = PRESENT | USER;
    if writable {
        flags |= WRITABLE;
    }
    unsafe { core::ptr::write_volatile(leaf_ptr, (phys & FRAME_MASK) | flags) };
    Ok(())
}

/// Map one ELF `PT_LOAD` segment into user pages: a fresh frame per 4 KiB of
/// its memory span, the file bytes copied in and the `.bss` tail (mem_size >
/// file bytes) zero-filled, each page mapped user-accessible with the
/// segment's writability.
///
/// # Safety
/// Same contract as [`map_user`]; `phys_offset` must window physical memory.
unsafe fn load_segment(
    console: &mut SerialPort,
    phys_offset: u64,
    frames: &mut FrameAllocator,
    seg: &elf::Segment,
) -> Result<(), &'static str> {
    let page_base = seg.vaddr & !0xFFF;
    let end = seg.vaddr + seg.mem_size;
    let mut vaddr = page_base;
    while vaddr < end {
        let frame = frames
            .allocate()
            .ok_or("out of frames for a segment page")?;
        let dst = (phys_offset + frame.start_addr()) as *mut u8;
        unsafe { core::ptr::write_bytes(dst, 0, 4096) };
        // Copy whatever of this segment's file bytes fall inside this page.
        for i in 0..4096u64 {
            let addr = vaddr + i;
            if addr < seg.vaddr {
                continue; // page starts before the segment (alignment slack)
            }
            let file_off = (addr - seg.vaddr) as usize;
            if file_off < seg.bytes.len() {
                unsafe { *dst.add(i as usize) = seg.bytes[file_off] };
            }
        }
        unsafe {
            map_user(
                phys_offset,
                frames,
                vaddr,
                frame.start_addr(),
                seg.flags.writable,
            )?
        };
        vaddr += 4096;
    }
    let _ = console;
    Ok(())
}

/// Reload CR3 to flush the TLB after editing the active page tables.
unsafe fn flush_tlb() {
    let cr3 = crate::switch::current_cr3();
    unsafe {
        core::arch::asm!("mov cr3, {}", in(reg) cr3, options(nostack, preserves_flags));
    }
}

/// Assemble the user program into `code` (a 4 KiB buffer that will back the
/// user code page), returning the entry offset (0).
fn build_user_program(code: &mut [u8]) {
    const MSG1: &[u8] = b"[ring3] hello from userland: this code runs at CPL 3\n";
    const MSG2: &[u8] = b"[ring3] second syscall returned - iretq round-trip works\n";

    let msg1_vaddr = USER_CODE_VADDR + MSG1_OFF;
    let msg2_vaddr = USER_CODE_VADDR + MSG2_OFF;

    let mut p = 0usize;
    let mut emit = |bytes: &[u8], p: &mut usize| {
        code[*p..*p + bytes.len()].copy_from_slice(bytes);
        *p += bytes.len();
    };

    // mov rax, SYS_WRITE ; movabs rsi, msg1 ; mov rdx, len1 ; syscall
    emit(&[0x48, 0xC7, 0xC0], &mut p);
    emit(&(SYS_WRITE as u32).to_le_bytes(), &mut p);
    emit(&[0x48, 0xBE], &mut p);
    emit(&msg1_vaddr.to_le_bytes(), &mut p);
    emit(&[0x48, 0xC7, 0xC2], &mut p);
    emit(&(MSG1.len() as u32).to_le_bytes(), &mut p);
    emit(&[0x0F, 0x05], &mut p);

    // mov rax, SYS_WRITE ; movabs rsi, msg2 ; mov rdx, len2 ; syscall
    emit(&[0x48, 0xC7, 0xC0], &mut p);
    emit(&(SYS_WRITE as u32).to_le_bytes(), &mut p);
    emit(&[0x48, 0xBE], &mut p);
    emit(&msg2_vaddr.to_le_bytes(), &mut p);
    emit(&[0x48, 0xC7, 0xC2], &mut p);
    emit(&(MSG2.len() as u32).to_le_bytes(), &mut p);
    emit(&[0x0F, 0x05], &mut p);

    // xor rax, rax (SYS_EXIT) ; syscall ; jmp $  (safety, must never run)
    emit(&[0x48, 0x31, 0xC0], &mut p);
    emit(&[0x0F, 0x05], &mut p);
    emit(&[0xEB, 0xFE], &mut p);

    // Place the two messages.
    code[MSG1_OFF as usize..MSG1_OFF as usize + MSG1.len()].copy_from_slice(MSG1);
    code[MSG2_OFF as usize..MSG2_OFF as usize + MSG2.len()].copy_from_slice(MSG2);
}

/// The ring-3 syscall handler (called from `gdt::syscall_entry`). `number` =
/// user rax, `arg0` = user rsi, `arg1` = user rdx.
///
/// `WRITE` copies the user's buffer to the console; `EXIT` `longjmp`s back into
/// [`launch`], never returning. Everything is single-threaded and trusted here
/// (one hand-built program), but user pointers are still range-checked.
pub extern "C" fn handle_syscall(number: u64, arg0: u64, arg1: u64) -> u64 {
    match Syscall::from_number(number) {
        Syscall::Write => {
            let ptr = arg0;
            let len = arg1.min(4096) as usize;
            // The buffer is user-mapped and present in the active address
            // space; we are in ring 0, so we may read it directly.
            if ptr >= USER_CODE_VADDR && ptr + len as u64 <= USER_CODE_VADDR + 4096 {
                let bytes = unsafe { core::slice::from_raw_parts(ptr as *const u8, len) };
                let mut console = SerialPort::init();
                for &b in bytes {
                    if b == b'\n' {
                        console.write_byte(b'\r');
                    }
                    console.write_byte(b);
                }
                len as u64
            } else {
                abi::ENOSYS // rejected: pointer outside the user region
            }
        }
        Syscall::Yield => 0,  // cooperative: the kernel just notes it
        Syscall::GetPid => 1, // the one ring-3 process we launch
        Syscall::Exit => {
            unsafe { longjmp(core::ptr::addr_of!(KERNEL_RESUME), 1) };
        }
        Syscall::Unknown(_) => abi::ENOSYS,
    }
}

/// Build a user process and run it in ring 3, returning once it `EXIT`s.
///
/// The program is not mapped directly: it is wrapped in a real ELF64 image,
/// **written to the VFS**, then **loaded back and parsed** ([`elf::parse`]) and
/// its `PT_LOAD` segments mapped into user pages — the genuine
/// file-on-disk → isolated-process path, exercising the VFS, the ELF loader,
/// and per-segment permissions rather than a hand-placed blob.
///
/// # Safety
/// Requires the bootloader's physical-memory mapping (`phys_offset`) and the
/// GDT/TSS/syscall MSRs to be initialised; single-threaded boot context.
pub unsafe fn launch(
    console: &mut SerialPort,
    phys_offset: u64,
    frames: &mut FrameAllocator,
    fs: &mut Vfs,
) {
    // 1. Assemble the program and wrap it in a real ELF64 executable.
    let mut code = [0u8; 4096];
    build_user_program(&mut code);
    let image_bytes = elf::synth_executable(USER_CODE_VADDR, &code);

    // 2. Store it in the filesystem, then read it back — a program loaded from
    //    a file, not baked into the kernel.
    let _ = fs.mkdir("/bin");
    if fs.write_all(USER_ELF_PATH, &image_bytes).is_err() {
        let _ = writeln!(console, "ring3: could not write {USER_ELF_PATH} to the vfs");
        return;
    }
    let Ok(raw) = fs.read_all(USER_ELF_PATH) else {
        let _ = writeln!(console, "ring3: could not read {USER_ELF_PATH} back");
        return;
    };

    // 3. Parse the ELF and map each loadable segment into user pages.
    let image = match elf::parse(&raw) {
        Ok(image) => image,
        Err(e) => {
            let _ = writeln!(console, "ring3: {USER_ELF_PATH} is not a valid elf: {e:?}");
            return;
        }
    };
    let _ = writeln!(
        console,
        "ring3: loaded {USER_ELF_PATH} — elf entry {:#x}, {} segment(s)",
        image.entry,
        image.segments.len()
    );
    for seg in &image.segments {
        if let Err(e) = unsafe { load_segment(console, phys_offset, frames, seg) } {
            let _ = writeln!(console, "ring3: mapping segment failed: {e}");
            return;
        }
    }

    // 4. A one-page user stack.
    let Some(stack_frame): Option<Frame> = frames.allocate() else {
        let _ = writeln!(console, "ring3: out of frames for user stack");
        return;
    };
    if let Err(e) = unsafe {
        map_user(
            phys_offset,
            frames,
            USER_STACK_PAGE,
            stack_frame.start_addr(),
            true,
        )
    } {
        let _ = writeln!(console, "ring3: mapping user stack failed: {e}");
        return;
    }
    unsafe { flush_tlb() };

    let _ = writeln!(
        console,
        "ring3: entering user mode at {:#x} (stack {USER_STACK_TOP:#x})",
        image.entry
    );

    // setjmp: 0 on the initial pass (enter ring 3); the EXIT syscall longjmps
    // back here with 1, and the boot sequence continues.
    let resumed = unsafe { setjmp(core::ptr::addr_of_mut!(KERNEL_RESUME)) };
    if resumed == 0 {
        unsafe { gdt::enter_user_mode(image.entry, USER_STACK_TOP) };
    }

    // Critical: the `syscall` that carried EXIT cleared the interrupt flag
    // (IA32_FMASK), and `longjmp` returned here without restoring rflags — so
    // interrupts are currently OFF. Turn them back on, or the idle loop's
    // `hlt` would sleep forever and every timer/keyboard/network service that
    // the rest of boot depends on would silently stall.
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
    }
    let _ = writeln!(
        console,
        "ring3: user process exited cleanly — back in ring 0, kernel continues"
    );
}
