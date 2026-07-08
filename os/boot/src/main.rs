//! Praxis OS, bare metal: the nucleus booted on x86_64 with no host OS below.
//!
//! The bootloader hands over the machine; from that instant everything running
//! is ours — the free-list heap in kernel BSS, the physical frame allocator
//! over the bootloader memory map, the IDT + PIC + PIT timer + PS/2 keyboard
//! we install ourselves, the UART console, and the nucleus (trust ledger,
//! epistemic scheduler, zero-copy IPC, transactional intents, the SPU
//! executive, and the VFS) behind an interactive praxsh prompt. The timer IRQ
//! is the real hardware clock; the keyboard IRQ is real console input. Run
//! under QEMU with `-serial stdio` for a terminal.

#![no_std]
#![no_main]

mod acpi;
mod disk;
mod gdt;
mod interrupts;
mod nic;
mod serial;
mod smp;
mod switch;
mod usermode;

use core::arch::asm;
use core::fmt::Write;
use core::sync::atomic::Ordering;

use bootloader_api::config::{BootloaderConfig, Mapping};
use bootloader_api::info::MemoryRegionKind;
use bootloader_api::{entry_point, BootInfo};
use praxis_nucleus::block::BlockDevice;
use praxis_nucleus::mem::MemoryRegion;
use praxis_nucleus::{LockedHeap, Nucleus};

/// The nucleus heap lives in kernel BSS — no paging gymnastics required to
/// get an arena, and the allocator itself is ours. 8 MiB: fetching real web
/// pages holds the TCP receive buffer and the growing body at once. The
/// wrapper type pins the arena to a 64-byte alignment (a bare `[u8; N]`
/// static is only 1-aligned, and the free-list's nodes want 8).
const HEAP_SIZE: usize = 8 * 1024 * 1024;
#[repr(C, align(64))]
struct HeapArena([u8; HEAP_SIZE]);
static mut HEAP_SPACE: HeapArena = HeapArena([0; HEAP_SIZE]);

#[global_allocator]
static HEAP: LockedHeap = LockedHeap::empty();

/// Ask the bootloader to map ALL physical memory into our address space —
/// that mapping is how the NIC driver reaches its BAR0 MMIO registers and
/// the DMA rings the chip scribbles into.
static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    let phys_offset: Option<u64> = boot_info.physical_memory_offset.into_option();
    let mut console = serial::SerialPort::init();
    // Safety: HEAP_SPACE is a static arena used exactly once, here.
    unsafe {
        let start = &raw mut HEAP_SPACE as *mut u8 as usize;
        HEAP.init(start, HEAP_SIZE);
    }

    let _ = writeln!(
        console,
        "\nPraxis OS — proof-scheduled kernel, bare metal x86_64"
    );

    // Bring up the privilege boundary first: the GDT (kernel + user segments)
    // and TSS, then the syscall/sysret fast path.
    gdt::init();
    gdt::init_syscalls();
    let _ = writeln!(
        console,
        "gdt/tss + syscall fast path: online (ring 0/3 ready)"
    );

    // Bring up interrupts: the IDT with our exception + IRQ handlers, the PIC
    // remapped clear of the exception range, and the PIT ticking at 100 Hz.
    interrupts::init_idt();
    interrupts::init_pic();
    interrupts::init_timer(100);
    interrupts::enable();
    let _ = writeln!(console, "idt/pic/timer/keyboard: online (PIT @ 100 Hz)");

    // The scheduler's clock is now the hardware timer tick, not the TSC — a
    // real periodic interrupt the kernel installed.
    let mut nucleus = Nucleus::new(alloc::boxed::Box::new(timer_ticks), Some(&HEAP));

    // Hand the bootloader's memory map to the physical frame allocator.
    let regions = collect_memory_map(boot_info);
    nucleus.install_memory_map(&regions);
    let _ = writeln!(
        console,
        "physical frames: mapped from bootloader memory map"
    );

    let _ = writeln!(
        console,
        "kernel address space: CR3={:#x}",
        switch::current_cr3()
    );

    // Discover the real hardware on the PCI bus by reading config space through
    // the 0xCF8/0xCFC ports — the kernel seeing the actual machine beneath it.
    let pci_devices = praxis_nucleus::pci::enumerate(pci_read_config);
    let _ = writeln!(
        console,
        "pci bus: {} device(s) discovered",
        pci_devices.len()
    );
    nucleus.set_pci(pci_devices);

    // Discover every CPU core the firmware describes (ACPI MADT), then START
    // the application cores: INIT-SIPI-SIPI each one to a trampoline that walks
    // it into long mode on the kernel's page tables and reports in. This is
    // real SMP — a second core executing our code — done best-effort so a core
    // that never answers can never hang the boot.
    if let (Some(rsdp), Some(offset)) = (boot_info.rsdp_addr.into_option(), phys_offset) {
        match acpi::discover_cpus(rsdp, offset) {
            Ok(info) => {
                let _ = writeln!(
                    console,
                    "acpi: {} cpu core(s) discovered ({} enabled), local apic @ {:#x}",
                    info.cpus.len(),
                    info.enabled_count(),
                    info.local_apic_addr
                );
                // Record the topology so the `cpus` shell command can report it.
                nucleus.cpu_topology = praxis_nucleus::CpuTopology {
                    cores_total: info.enabled_count(),
                    cores_online: 0,
                    local_apic: info.local_apic_addr,
                    ap_ticks: [0; 8],
                };
                let aps = info.application_processor_ids();
                if aps.is_empty() {
                    let _ = writeln!(
                        console,
                        "acpi: uniprocessor — no application cores to start"
                    );
                } else if let Some(frames) = nucleus.frames.as_mut() {
                    let lapic_virt = offset + info.local_apic_addr;
                    let cr3 = switch::current_cr3();
                    let online = unsafe {
                        smp::bring_up(&aps, lapic_virt, offset, cr3, frames, || {
                            for _ in 0..3_000 {
                                core::hint::spin_loop();
                            }
                        })
                    };
                    nucleus.cpu_topology.cores_online = online as usize;
                    let _ = writeln!(
                        console,
                        "smp: {}/{} application core(s) online (started via INIT-SIPI-SIPI)",
                        online,
                        aps.len()
                    );
                }
            }
            Err(e) => {
                let _ = writeln!(console, "acpi: core discovery failed: {e:?}");
            }
        }
    }

    // Durable storage: probe the ATA data disk (primary slave). If it holds a
    // filesystem snapshot, restore it — the files written on the LAST boot
    // come back — and make the real disk the target of every future `sync`.
    let mut restored = false;
    match disk::probe() {
        Ok(drive) => {
            let _ = writeln!(
                console,
                "ata: {} — {} sectors ({} KiB), persistent tier online",
                drive.model_str(),
                drive.num_blocks(),
                drive.capacity_bytes() / 1024
            );
            if let Ok(fs) = praxis_nucleus::fs::Vfs::restore_journaled(&drive) {
                let nodes = fs.node_count();
                nucleus.fs = fs;
                restored = true;
                let _ = writeln!(
                    console,
                    "fs: restored from disk (journaled) — {nodes} nodes survived the power cycle"
                );
            } else {
                let _ = writeln!(console, "fs: disk is blank — starting fresh");
            }
            nucleus.disk = alloc::boxed::Box::new(drive);
        }
        Err(e) => {
            let _ = writeln!(console, "ata: no data disk ({e:?}) — RAM-backed tier only");
        }
    }

    // Seed the filesystem so `ls` shows something real on first boot — but
    // never clobber a restored one.
    if !restored {
        nucleus.exec_line("mkdir /etc", &mut console);
        nucleus.exec_line("write /etc/motd proof buys speed", &mut console);
    }

    // The persistence proof: a boot counter that lives on the disk. Every
    // boot reads it, increments it, writes it back, and syncs — so the number
    // on the serial console is the number of times this machine has EVER
    // booted this disk, across full power cycles.
    let boots = nucleus
        .fs
        .read_all("/boot/count")
        .ok()
        .and_then(|bytes| {
            core::str::from_utf8(&bytes)
                .ok()
                .map(str::trim)?
                .parse::<u64>()
                .ok()
        })
        .unwrap_or(0)
        + 1;
    let _ = nucleus.fs.mkdir("/boot");
    let _ = nucleus
        .fs
        .write_all("/boot/count", alloc::format!("{boots}").as_bytes());
    match nucleus.fs.snapshot_journaled(&mut nucleus.disk) {
        Ok(bytes) => {
            let _ = writeln!(
                console,
                "persistence: boot #{boots} recorded ({bytes} bytes, crash-safe journal)"
            );
        }
        Err(e) => {
            let _ = writeln!(console, "persistence: sync failed: {e:?}");
        }
    }

    // Load a demo program into its own isolated address space (paging + ELF).
    nucleus.exec_line("exec proven init", &mut console);

    // Drop to ring 3 for real: build a user process, run its code at CPL 3,
    // and service the `syscall` traps it makes — the hardware privilege
    // boundary, live. Needs the bootloader's physical-memory mapping (to edit
    // the page tables) and the frame allocator (for user + table frames).
    if let (Some(offset), Some(frames)) = (phys_offset, nucleus.frames.as_mut()) {
        unsafe { usermode::launch(&mut console, offset, frames, &mut nucleus.fs) };
    }

    // A little life on boot: one proven and one unproven task, so `ps` shows
    // the distrust tax immediately.
    nucleus.exec_line("spawn proven 512", &mut console);
    nucleus.exec_line("spawn unproven 512", &mut console);

    // The Kairos index live on bare metal: declare arrivals for the proven
    // task and show the decomposed scheduling math (os/KAIROS.md).
    nucleus.exec_line("kairos push 1 64", &mut console);
    nucleus.exec_line("kairos", &mut console);

    // List the real PCI hardware the kernel just discovered.
    nucleus.exec_line("pci", &mut console);

    // Prove the network transport on real hardware: a loopback TCP request
    // completes a full 3-way handshake and carries data, on bare metal.
    nucleus.exec_line("tcp listen 80", &mut console);
    nucleus.exec_line("tcp connect 10.0.0.1 80", &mut console);
    nucleus.exec_line("tcp send 0 GET /motd HTTP/1.0", &mut console);
    nucleus.exec_line("tcp recv 1", &mut console);

    // Leave loopback for a real wire: bring up the e1000 through its BAR0
    // MMIO + DMA rings and make it the kernel's interface, then prove it by
    // ARPing and pinging the gateway across actual (virtualized) hardware.
    match phys_offset {
        Some(offset) => match nic::init(&mut nucleus, offset, pci_read_config, pci_write_config) {
            Ok(mac) => {
                let _ = writeln!(
                    console,
                    "e1000: online, mac {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x} — \
                     interface now 10.0.2.15 (gw 10.0.2.2)",
                    mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
                );
                nucleus.exec_line("ping 10.0.2.2 3", &mut console);
                nucleus.exec_line("arp", &mut console);
                // The whole stack against the actual internet: resolve a
                // real name over UDP/53, then TCP-connect and HTTP-GET it.
                nucleus.exec_line("dns example.com", &mut console);
                nucleus.exec_line("fetch example.com /", &mut console);
                // And the other direction: serve the VFS to the world. With
                // `hostfwd=tcp::18080-:80` on the QEMU netdev, a host-side
                // `curl http://127.0.0.1:18080/etc/motd` is answered by THIS
                // kernel — its own TCP listener, its own files.
                nucleus.exec_line("httpd start 80", &mut console);
            }
            Err(e) => {
                let _ = writeln!(console, "e1000: {e} — staying on loopback");
            }
        },
        None => {
            let _ = writeln!(
                console,
                "e1000: no physical-memory mapping — staying on loopback"
            );
        }
    }

    nucleus.exec_line("uname", &mut console);
    let _ = write!(console, "\npraxsh> ");

    let mut line = alloc::string::String::new();
    let mut last_scancode = 0u8;
    let mut last_timer = 0u64;
    loop {
        // Console input arrives two ways: the serial port (QEMU `-serial
        // stdio`) and the real PS/2 keyboard IRQ. Poll both.
        while let Some(byte) = console.try_read() {
            feed(&mut nucleus, &mut console, &mut line, byte);
        }
        let scancode = interrupts::LAST_SCANCODE.load(Ordering::Relaxed);
        if scancode != 0 && scancode != last_scancode {
            last_scancode = scancode;
            let ascii = interrupts::scancode_to_ascii(scancode);
            if ascii != 0 {
                feed(&mut nucleus, &mut console, &mut line, ascii);
            }
        }

        // Drive the preemptive scheduler from the real hardware timer: each PIT
        // tick since the last poll is charged to the running thread, preempting
        // it when its proof-sized quantum runs out. On a full ring-3 launch the
        // returned `Preempted { from, to }` is where `switch::switch_context`
        // fires; here the policy runs live and preemptions are observable via
        // `top`. This is what stops an unproven runaway from hanging the box.
        let now = interrupts::TICKS.load(Ordering::Relaxed);
        while last_timer < now {
            last_timer += 1;
            let _ = nucleus.preempt.on_tick();
        }

        // Frames arrive on the wire whenever they like; drain the NIC every
        // trip around the idle loop so the stack sees them promptly, then pump
        // the polled kernel services (the HTTP daemon) that feed off it, and
        // let TCP retransmit anything the wire dropped.
        nucleus.net.poll();
        nucleus.net.tick(now);
        nucleus.pump_services();

        // The kernel's idle loop IS the cooperative scheduler.
        if nucleus.sched.run_slice(64) == 0 {
            halt_until_interrupt();
        }
    }
}

/// Handle one input byte for the line editor.
fn feed(
    nucleus: &mut Nucleus,
    console: &mut serial::SerialPort,
    line: &mut alloc::string::String,
    byte: u8,
) {
    match byte {
        b'\r' | b'\n' => {
            let _ = writeln!(console);
            // Refresh the live per-AP tick counters just before dispatch, so
            // `cpus` always reports this instant's snapshot rather than
            // whatever was true at boot.
            if line.trim() == "cpus" {
                let ticks = smp::ap_tick_snapshot();
                let mut padded = [0u64; 8];
                let n = ticks.len().min(8);
                padded[..n].copy_from_slice(&ticks[..n]);
                nucleus.cpu_topology.ap_ticks = padded;
            }
            nucleus.exec_line(line, console);
            line.clear();
            let _ = write!(console, "praxsh> ");
        }
        0x08 | 0x7F => {
            if line.pop().is_some() {
                let _ = write!(console, "\x08 \x08");
            }
        }
        b if (0x20..0x7F).contains(&b) => {
            line.push(b as char);
            console.write_byte(b);
        }
        _ => {}
    }
}

/// Translate the bootloader memory map into the nucleus's region model.
fn collect_memory_map(boot_info: &BootInfo) -> alloc::vec::Vec<MemoryRegion> {
    boot_info
        .memory_regions
        .iter()
        .map(|r| {
            if matches!(r.kind, MemoryRegionKind::Usable) {
                MemoryRegion::usable(r.start, r.end)
            } else {
                MemoryRegion::reserved(r.start, r.end)
            }
        })
        .collect()
}

extern crate alloc;

/// The scheduler's monotonic clock: the PIT timer tick count. A real periodic
/// hardware interrupt, installed and serviced by this kernel.
fn timer_ticks() -> u64 {
    interrupts::TICKS.load(Ordering::Relaxed)
}

fn halt_until_interrupt() {
    unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
}

/// 32-bit port output, for PCI configuration space (mechanism #1).
unsafe fn outl(port: u16, value: u32) {
    asm!("out dx, eax", in("dx") port, in("eax") value, options(nomem, nostack, preserves_flags));
}

/// 32-bit port input.
unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    asm!("in eax, dx", in("dx") port, out("eax") value, options(nomem, nostack, preserves_flags));
    value
}

/// Read one PCI config dword via the 0xCF8 address / 0xCFC data ports — the real
/// hardware access the nucleus's portable enumerator drives.
fn pci_read_config(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let addr = praxis_nucleus::pci::config_address(bus, slot, func, offset);
    unsafe {
        outl(0xCF8, addr);
        inl(0xCFC)
    }
}

/// Write one PCI config dword — how the NIC gets its command-register bits
/// (memory-space decoding + bus mastering) turned on.
fn pci_write_config(bus: u8, slot: u8, func: u8, offset: u8, value: u32) {
    let addr = praxis_nucleus::pci::config_address(bus, slot, func, offset);
    unsafe {
        outl(0xCF8, addr);
        outl(0xCFC, value);
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    let mut console = serial::SerialPort::init();
    let _ = writeln!(console, "\nnucleus panic: {info}");
    loop {
        halt_until_interrupt();
    }
}
