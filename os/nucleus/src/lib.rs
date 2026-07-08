//! Praxis Nucleus — the freestanding kernel core of the proof-scheduled OS.
//!
//! `no_std`, zero dependencies, built from scratch: its own spinlock
//! ([`sync`]), its own free-list heap ([`heap`]), its own cooperative future
//! executor ([`sched`]), and the mechanisms that make Praxis Praxis rather than
//! a Linux re-tread:
//!
//! * **Trust ledger** ([`proof`]) — execution privilege as live, revocable
//!   state derived from claims, not from a uid.
//! * **Proof-gated cooperative scheduling + epistemic scheduler** ([`sched`])
//!   — preemption as a priced distrust tax; CPU attention steered by
//!   prediction error, active-inference style.
//! * **Kairos index scheduling** ([`kairos`]) — a risk-adjusted
//!   generalized-cµ index (backlog × service-rate ÷ model-uncertainty, pure
//!   integer math) that provably collapses mean flow time under
//!   heterogeneous load and makes predictability itself CPU currency.
//! * **Tier-gated zero-copy IPC** ([`ipc`]) — messages move by ownership
//!   handoff; the fast path is a proof privilege.
//! * **Transactional intent syscalls** ([`intent`]) — syscalls carry goals and
//!   postconditions; the kernel fuses the batch and proves the goal before
//!   commit.
//! * **praxsh** ([`shell`]) — the kernel's own command line over any
//!   `fmt::Write`.
//!
//! The same core is linked by two platforms: `praxis-boot` (bare-metal
//! x86_64-unknown-none, UART console, TSC clock) and `praxis-host` (std
//! console, for running the nucleus interactively anywhere). Platform code
//! provides exactly two things — a clock and a console — which is the whole
//! porting surface.

#![cfg_attr(not(test), no_std)]

extern crate alloc;

pub mod abi;
pub mod acpi;
pub mod ata;
pub mod block;
pub mod crypto;
pub mod dns;
pub mod e1000;
pub mod elf;
pub mod fs;
pub mod heap;
pub mod httpd;
pub mod intent;
pub mod ipc;
pub mod kairos;
pub mod lapic;
pub mod mem;
pub mod net;
pub mod ops;
pub mod pci;
pub mod preempt;
pub mod process;
pub mod proof;
pub mod sched;
pub mod shell;
pub mod spu;
pub mod sync;
pub mod syscall;
pub mod tcp;
pub mod virtio;
pub mod vmem;
pub mod x25519;

use alloc::boxed::Box;

pub use heap::LockedHeap;
pub use proof::{ClaimClass, Tier, TrustLedger};
pub use sched::{yield_now, Scheduler};

/// The assembled kernel: scheduler + trust ledger + object store + the
/// SPU-native executive (device model, p-bit fabric, mode fabric,
/// consolidation — `os/SPU-OS.md`) + the storage stack (VFS over a block
/// device, backed by the persistent tier).
pub struct Nucleus {
    pub sched: Scheduler,
    pub ledger: TrustLedger,
    pub store: intent::ObjectStore,
    /// Present on bare metal where [`LockedHeap`] is the global allocator;
    /// `None` on the hosted runner (std allocates).
    pub heap: Option<&'static LockedHeap>,
    /// The State Processing Unit model: contexts, tiers, wear, grafts.
    pub spu: spu::SpuDevice,
    /// The p-bit sampling fabric (deterministically seeded stand-in for the
    /// chip's physical randomness).
    pub pbits: spu::cortex::PBitFabric,
    /// The mode-switching fabric: thinking vs exploring, ESQ-batched.
    pub modes: spu::cortex::ModeFabric,
    /// The Consolidation Engine: experience → permanent skill, wear-gated.
    pub consolidator: spu::cortex::Consolidator,
    /// The virtual filesystem — a real hierarchical namespace.
    pub fs: fs::Vfs,
    /// The persistence substrate under the VFS (the NV-tier block device).
    /// Boxed so bare metal can swap in a real ATA drive ([`ata::AtaDrive`])
    /// and `sync` becomes genuinely durable; defaults to a RAM disk.
    pub disk: alloc::boxed::Box<dyn block::BlockDevice>,
    /// The physical frame allocator, once a memory map is installed.
    pub frames: Option<mem::FrameAllocator>,
    /// The process table: isolated programs whose capabilities are set by
    /// their proof tier (each owns a paged address space).
    pub procs: process::ProcessTable,
    /// The operator engine: `ensure`/`undo`/`dry`/`why`/`prove`/`when` — the
    /// command primitives that exploit the transactional + causal substrate to
    /// delete classes of terminal problems Unix cannot.
    pub ops: ops::OpsEngine,
    /// The preemptive scheduler: real timer-driven multitasking where the
    /// time-slice length is a function of proof, so unproven runaways are
    /// contained and proven work runs nearly uninterrupted.
    pub preempt: preempt::PreemptiveScheduler,
    /// The network stack: Ethernet/ARP/IPv4/ICMP/UDP over a device boundary,
    /// with egress gated by proof tier — network authority as a function of
    /// proof, not identity.
    pub net: net::NetStack,
    /// The PCI devices discovered on the bus. Empty on the host; populated by the
    /// bare-metal boot layer, which reads real config space and calls
    /// [`Nucleus::set_pci`].
    pub pci: alloc::vec::Vec<pci::PciDevice>,
    /// The kernel HTTP daemon, once started (`httpd start <port>`): serves the
    /// VFS over the kernel's own TCP stack, pumped by the idle loop.
    pub httpd: Option<httpd::Httpd>,
    /// Topology the boot layer discovered (ACPI) and enacted (SMP): total CPU
    /// cores, and how many application cores it actually started. Reported by
    /// the `cpus` shell command; `1/0` on the hosted runner.
    pub cpu_topology: CpuTopology,
}

/// CPU topology as discovered and brought up by the platform layer.
#[derive(Debug, Clone, Copy, Default)]
pub struct CpuTopology {
    /// Total logical cores the firmware described (1 if unknown).
    pub cores_total: usize,
    /// Application cores the boot layer started via INIT-SIPI-SIPI.
    pub cores_online: usize,
    /// Physical address of the Local APIC, when known.
    pub local_apic: u64,
    /// Each online AP's live tick counter (index 0 = first AP started, …),
    /// refreshed by the platform layer just before `cpus` runs. Not a
    /// one-time check-in: every online core increments its own slot forever,
    /// so reading this twice and seeing the numbers advance is the live proof
    /// that core is genuinely, independently executing.
    pub ap_ticks: [u64; 8],
}

impl Nucleus {
    /// `clock` is the platform's monotonic tick source.
    pub fn new(clock: Box<dyn Fn() -> u64>, heap: Option<&'static LockedHeap>) -> Self {
        Self {
            sched: Scheduler::new(clock),
            ledger: TrustLedger::new(),
            store: intent::ObjectStore::new(),
            heap,
            spu: spu::SpuDevice::model(),
            pbits: spu::cortex::PBitFabric::new(0xA10A_0511),
            modes: spu::cortex::ModeFabric::new(),
            consolidator: spu::cortex::Consolidator::new(),
            fs: fs::Vfs::new(),
            disk: alloc::boxed::Box::new(block::RamDisk::new(2048)), // 1 MiB image
            frames: None,
            procs: process::ProcessTable::new(),
            ops: ops::OpsEngine::new(),
            preempt: preempt::PreemptiveScheduler::new(),
            // The default interface is loopback at 10.0.0.1; a real driver
            // replaces the device without touching anything above it.
            net: net::NetStack::loopback([10, 0, 0, 1]),
            pci: alloc::vec::Vec::new(),
            httpd: None,
            cpu_topology: CpuTopology {
                cores_total: 1,
                cores_online: 0,
                local_apic: 0,
                ap_ticks: [0; 8],
            },
        }
    }

    /// One pump of the kernel's polled network services (the HTTP daemon, when
    /// running). Call from the platform idle loop alongside `net.poll()`.
    pub fn pump_services(&mut self) {
        if let Some(httpd) = &mut self.httpd {
            httpd.poll(&mut self.net, &self.fs);
        }
    }

    /// Install the physical frame allocator from the bootloader's memory map.
    pub fn install_memory_map(&mut self, regions: &[mem::MemoryRegion]) {
        self.frames = Some(mem::FrameAllocator::from_map(regions));
    }

    /// Record the PCI devices the boot layer discovered on the real bus.
    pub fn set_pci(&mut self, devices: alloc::vec::Vec<pci::PciDevice>) {
        self.pci = devices;
    }

    /// One shell line in, text out — the whole kernel API a console needs.
    pub fn exec_line(&mut self, line: &str, out: &mut dyn core::fmt::Write) {
        shell::exec(self, line, out);
    }
}
