//! SMP bring-up — actually starting the application processor cores.
//!
//! ACPI found the cores and [`praxis_nucleus::lapic`] knows how to poke them;
//! this is the arch-specific glue that makes a second core execute our code.
//! Each application processor (AP) is started with INIT-SIPI-SIPI, wakes in
//! 16-bit real mode at a **trampoline** page, walks itself up through protected
//! mode into 64-bit long mode using the *same* page tables the boot core uses
//! (so the kernel is already mapped), switches to a private stack, and jumps
//! into [`ap_entry`], where it records itself as online and then spins
//! incrementing its own tick counter forever — the live, ongoing proof that
//! it is a genuinely independent instruction stream, not a one-time check-in.
//!
//! The trampoline is position-fixed at physical `0x8000` (SIPI vector `0x08`):
//! every absolute address inside it is written as `label − start + 0x8000`, so
//! copying the blob to `0x8000` needs no code patching — only the three data
//! words (CR3, stack, entry) the boot core fills in per core.
//!
//! Bring-up is **best-effort and bounded**: each core is given a deadline to
//! check in; whether it does or not, the boot core continues. A core that
//! never reports simply isn't counted — the kernel is never left waiting on
//! hardware that didn't answer.

use core::sync::atomic::{AtomicU64, Ordering};

use praxis_nucleus::lapic::{Lapic, LapicMmio};
use praxis_nucleus::mem::FrameAllocator;

const FRAME_MASK: u64 = 0x000F_FFFF_FFFF_F000;

/// Identity-map one page (`vaddr → vaddr`) into the *live* kernel page tables,
/// present + writable, **kernel-only** (no USER bit). The AP needs this: the
/// instant it enables paging it is still executing at linear `0x8000`, which
/// the bootloader otherwise maps only at the high physical-memory offset — an
/// identity mapping keeps the trampoline (code, GDT, data) reachable across the
/// paging switch, until it jumps to the high kernel entry.
///
/// # Safety
/// `phys_offset` must window physical memory; single-threaded boot context.
unsafe fn map_identity(
    phys_offset: u64,
    cr3: u64,
    frames: &mut FrameAllocator,
    vaddr: u64,
) -> Result<(), &'static str> {
    let idx = [
        ((vaddr >> 39) & 0x1FF) as u64,
        ((vaddr >> 30) & 0x1FF) as u64,
        ((vaddr >> 21) & 0x1FF) as u64,
        ((vaddr >> 12) & 0x1FF) as u64,
    ];
    let mut table = cr3 & FRAME_MASK;
    for &index in idx.iter().take(3) {
        let entry_ptr = (phys_offset + table + index * 8) as *mut u64;
        let entry = unsafe { core::ptr::read_volatile(entry_ptr) };
        if entry & 1 != 0 {
            table = entry & FRAME_MASK;
        } else {
            let new = frames.allocate().ok_or("out of frames")?.start_addr();
            unsafe {
                core::ptr::write_bytes((phys_offset + new) as *mut u8, 0, 4096);
                core::ptr::write_volatile(entry_ptr, new | 0b11); // present+writable
            }
            table = new;
        }
    }
    let leaf = (phys_offset + table + idx[3] * 8) as *mut u64;
    unsafe { core::ptr::write_volatile(leaf, (vaddr & FRAME_MASK) | 0b11) };
    Ok(())
}

/// Physical address of the trampoline page. Must be page-aligned and below
/// 1 MiB (the SIPI vector is a page number: `0x8000 >> 12 == 0x08`).
const TRAMPOLINE_PHYS: u64 = 0x8000;
const TRAMPOLINE_VECTOR: u8 = 0x08;

/// How many cores have finished bring-up and reported in. Doubles as each
/// AP's claimed index (see [`ap_entry`]): bring-up starts cores strictly one
/// at a time and waits for a check-in before starting the next, so the
/// pre-increment value this fetch_add returns is stable per AP.
static AP_ONLINE: AtomicU64 = AtomicU64::new(0);

/// Per-AP tick counters: **not** a one-time check-in flag but a work counter
/// each AP increments forever after coming online. This is the difference
/// between "a core answered once" and "a core is genuinely running its own
/// independent instruction stream" — reading this twice, moments apart, and
/// seeing every online core's count advance is the live proof of real SMP.
static AP_TICKS: [AtomicU64; MAX_APS] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

/// A snapshot of every AP's tick counter, for the `cpus` shell command.
#[must_use]
pub fn ap_tick_snapshot() -> [u64; MAX_APS] {
    core::array::from_fn(|i| AP_TICKS[i].load(Ordering::Relaxed))
}

/// Per-AP stacks (bring-up is serialized, so one reusable region per core, up
/// to a small fixed fleet). 16 KiB each.
const AP_STACK_SIZE: usize = 16 * 1024;
const MAX_APS: usize = 7;
#[repr(C, align(16))]
struct ApStacks([[u8; AP_STACK_SIZE]; MAX_APS]);
static mut AP_STACKS: ApStacks = ApStacks([[0; AP_STACK_SIZE]; MAX_APS]);

// The trampoline blob. `.code16` → `.code32` → `.code64`, position-fixed at
// 0x8000 via the `- ap_trampoline_start + 0x8000` offsets. The three data
// words at the end are filled in by the boot core before each SIPI.
core::arch::global_asm!(
    r#"
.section .rodata.aptramp, "a"
.align 4096
.globl ap_trampoline_start
.globl ap_trampoline_end
.globl ap_tramp_cr3
.globl ap_tramp_stack
.globl ap_tramp_entry
.code16
ap_trampoline_start:
    cli
    cld
    xorw %ax, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    lgdtl off_gdt_ptr + 0x8000
    movl %cr0, %eax
    orl $1, %eax
    movl %eax, %cr0
    ljmp $0x08, $(off_prot32 + 0x8000)

.code32
ap_prot32:
    movw $0x10, %ax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movl %cr4, %eax
    orl $0x20, %eax                  /* PAE */
    movl %eax, %cr4
    movl off_cr3 + 0x8000, %eax      /* the kernel's PML4 */
    movl %eax, %cr3
    movl $0xC0000080, %ecx           /* EFER */
    rdmsr
    orl $0x900, %eax                 /* LME (bit 8) + NXE (bit 11) */
    wrmsr
    movl %cr0, %eax
    orl $0x80000000, %eax            /* PG */
    movl %eax, %cr0
    ljmp $0x18, $(off_long64 + 0x8000)

.code64
ap_long64:
    movl $0x10, %eax
    movw %ax, %ds
    movw %ax, %es
    movw %ax, %ss
    movq off_stack + 0x8000, %rsp
    movq off_entry + 0x8000, %rax
    jmp *%rax

.align 8
ap_gdt:
    .quad 0x0000000000000000          /* null */
    .quad 0x00CF9A000000FFFF          /* 0x08: 32-bit code */
    .quad 0x00CF92000000FFFF          /* 0x10: 32-bit data */
    .quad 0x00AF9A000000FFFF          /* 0x18: 64-bit code */
ap_gdt_ptr:
    .word ap_gdt_ptr - ap_gdt - 1
    .long off_gdt + 0x8000
.align 8
ap_tramp_cr3:   .quad 0
ap_tramp_stack: .quad 0
ap_tramp_entry: .quad 0
ap_trampoline_end:

/* Offsets within the blob (label differences are link-time constants), used
   as single-symbol memory/immediate operands so the copied-to-0x8000 code
   addresses correctly. */
.set off_prot32,   ap_prot32      - ap_trampoline_start
.set off_long64,   ap_long64      - ap_trampoline_start
.set off_gdt,      ap_gdt         - ap_trampoline_start
.set off_gdt_ptr,  ap_gdt_ptr     - ap_trampoline_start
.set off_cr3,      ap_tramp_cr3   - ap_trampoline_start
.set off_stack,    ap_tramp_stack - ap_trampoline_start
.set off_entry,    ap_tramp_entry - ap_trampoline_start
"#,
    options(att_syntax)
);

extern "C" {
    static ap_trampoline_start: u8;
    static ap_trampoline_end: u8;
    static ap_tramp_cr3: u8;
    static ap_tramp_stack: u8;
    static ap_tramp_entry: u8;
}

/// The 64-bit entry every AP reaches. `fetch_add` both reports this core
/// online (bring-up polls the same counter) *and* hands back a stable index
/// (0, 1, 2, …) this AP claims for life — safe because bring-up starts cores
/// strictly one at a time and waits for a check-in before sending the next
/// SIPI. From there the AP never parks: it spins incrementing its own tick
/// counter forever, so `cpus` can read [`AP_TICKS`] twice, moments apart, and
/// show every online core's count independently advancing — a second
/// instruction stream genuinely executing, not just a one-time check-in.
/// Uses only atomics and its own stack, so it is safe against the boot core
/// with no locks. Interrupts stay off (no IDT is loaded for the AP); a full
/// SMP scheduler would install one and take over here.
#[unsafe(no_mangle)]
extern "C" fn ap_entry() -> ! {
    let idx = AP_ONLINE.fetch_add(1, Ordering::SeqCst) as usize;
    loop {
        if idx < MAX_APS {
            AP_TICKS[idx].fetch_add(1, Ordering::Relaxed);
        }
        core::hint::spin_loop();
    }
}

/// The bare-metal LAPIC: memory-mapped registers via the physical-memory
/// window.
struct MmioLapic {
    base: u64,
}
impl LapicMmio for MmioLapic {
    fn read(&self, reg: u32) -> u32 {
        unsafe { core::ptr::read_volatile((self.base + u64::from(reg)) as *const u32) }
    }
    fn write(&mut self, reg: u32, value: u32) {
        unsafe { core::ptr::write_volatile((self.base + u64::from(reg)) as *mut u32, value) };
    }
}

/// Start the application cores. `apic_ids` come from ACPI; `lapic_phys` is the
/// Local APIC base; `phys_offset`/`kernel_cr3` let the trampoline reach memory
/// and adopt the kernel's page tables. `wait` polls the tick clock to bound
/// each core's check-in. Returns how many cores came online.
///
/// # Safety
/// Bare-metal, single-BSP boot context; the physical-memory mapping must be
/// live and `TRAMPOLINE_PHYS` must be a free low page.
pub unsafe fn bring_up(
    apic_ids: &[u8],
    lapic_phys: u64,
    phys_offset: u64,
    kernel_cr3: u64,
    frames: &mut FrameAllocator,
    mut wait: impl FnMut(),
) -> u64 {
    // 0. Identity-map the trampoline page so the AP survives enabling paging,
    //    then flush the TLB so the new mapping is live.
    if unsafe { map_identity(phys_offset, kernel_cr3, frames, TRAMPOLINE_PHYS) }.is_err() {
        return 0;
    }
    unsafe {
        core::arch::asm!("mov cr3, {}", in(reg) kernel_cr3, options(nostack, preserves_flags));
    }

    // 1. Copy the trampoline blob to the fixed low page.
    let start = core::ptr::addr_of!(ap_trampoline_start) as u64;
    let end = core::ptr::addr_of!(ap_trampoline_end) as u64;
    let len = (end - start) as usize;
    unsafe {
        core::ptr::copy_nonoverlapping(
            start as *const u8,
            (phys_offset + TRAMPOLINE_PHYS) as *mut u8,
            len,
        );
    }

    // Offsets of the three data words within the blob, so we can fill them in
    // the *copied* page.
    let cr3_off = core::ptr::addr_of!(ap_tramp_cr3) as u64 - start;
    let stack_off = core::ptr::addr_of!(ap_tramp_stack) as u64 - start;
    let entry_off = core::ptr::addr_of!(ap_tramp_entry) as u64 - start;
    let write_word = |off: u64, val: u64| unsafe {
        core::ptr::write_volatile((phys_offset + TRAMPOLINE_PHYS + off) as *mut u64, val);
    };
    write_word(cr3_off, kernel_cr3);
    write_word(entry_off, ap_entry as *const () as u64);

    let mut lapic = Lapic::new(MmioLapic { base: lapic_phys });
    lapic.enable();

    for (i, &apic_id) in apic_ids.iter().take(MAX_APS).enumerate() {
        let before = AP_ONLINE.load(Ordering::SeqCst);
        // Hand this core a fresh stack top.
        let stack_top = core::ptr::addr_of!(AP_STACKS.0[i]) as u64 + AP_STACK_SIZE as u64;
        write_word(stack_off, stack_top);

        // INIT, then two STARTUP IPIs at the trampoline page.
        lapic.start_core(apic_id, TRAMPOLINE_VECTOR);

        // Wait, bounded, for this core to check in before starting the next.
        // A working trampoline reports in almost immediately, so a small budget
        // catches success while keeping a no-show cheap.
        for _ in 0..400 {
            if AP_ONLINE.load(Ordering::SeqCst) > before {
                break;
            }
            wait();
        }
    }
    AP_ONLINE.load(Ordering::SeqCst)
}
