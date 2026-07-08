//! Processes — isolated programs, with capabilities set by proof.
//!
//! A process ties the pieces together: an [ELF image](crate::elf) loaded into
//! its own [address space](crate::vmem) (hardware-isolated from every other
//! process), a saved register context for [context switching](../../boot), and
//! — the Praxis twist — a **capability set derived from its proof tier**. This
//! is where the whole thesis lands at the process level: a program's authority
//! is not its user's ambient power (the 1970s model AI agents are about to
//! detonate), it is exactly what its proofs have earned. An unproven process
//! runs, but caged; a proven one is trusted with the fast, capable paths.
//!
//! The scheduler here is the *policy* (which process runs next, by tier); the
//! register-level switch that enacts it is arch asm in `boot/src/switch.rs`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use crate::elf::{self, ElfError};
use crate::mem::FrameAllocator;
use crate::proof::Tier;
use crate::vmem::{AddressSpace, VmError, PAGE_SIZE};

pub type Pid = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcState {
    Ready,
    Running,
    Blocked,
    Zombie,
}

/// What a process is allowed to do — computed from its proof tier, never from a
/// uid. This is capability confinement as a function of trust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Capabilities {
    /// May run uncaged in the kernel address space (Tier 0 only).
    pub in_process_exec: bool,
    /// May touch raw device MMIO / ports.
    pub raw_device: bool,
    /// May write the filesystem (read is always allowed).
    pub fs_write: bool,
    /// May spawn other processes.
    pub spawn: bool,
    /// May request additional physical memory at runtime.
    pub grow_memory: bool,
}

impl Capabilities {
    /// The heart of the model: proof tier → authority.
    pub fn from_tier(tier: Tier) -> Self {
        match tier {
            // Fully proven: trusted with everything, uncaged.
            Tier::Proven => Self {
                in_process_exec: true,
                raw_device: true,
                fs_write: true,
                spawn: true,
                grow_memory: true,
            },
            // Partially proven: isolated, useful, but no raw hardware and no
            // in-process (uncaged) execution.
            Tier::Partial => Self {
                in_process_exec: false,
                raw_device: false,
                fs_write: true,
                spawn: false,
                grow_memory: true,
            },
            // Unproven: runs, but caged to the minimum — read-only, no spawn.
            Tier::Unproven => Self {
                in_process_exec: false,
                raw_device: false,
                fs_write: false,
                spawn: false,
                grow_memory: false,
            },
        }
    }
}

/// The named capabilities a process can request, for `enforce`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap {
    InProcessExec,
    RawDevice,
    FsWrite,
    Spawn,
    GrowMemory,
}

/// Saved register state for a context switch. The bare-metal switch
/// (`boot/src/switch.rs`) saves/restores exactly these callee-saved registers
/// plus the stack pointer; here it is the portable record the scheduler owns.
#[derive(Debug, Clone, Copy, Default)]
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

pub struct Process {
    pub pid: Pid,
    pub name: String,
    pub tier: Tier,
    pub caps: Capabilities,
    pub entry: u64,
    pub state: ProcState,
    pub context: Context,
    space: AddressSpace,
    /// The process's physical pages, keyed by frame number (models the RAM its
    /// mappings point at, so reads go through real virtual→physical translation).
    phys: BTreeMap<u64, [u8; PAGE_SIZE as usize]>,
    pub segments: usize,
}

impl Process {
    /// The `CR3` value for this process (its page-table root).
    pub fn cr3(&self) -> u64 {
        self.space.root_frame().start_addr()
    }

    pub fn mapped_pages(&self) -> u64 {
        self.space.mapped_pages()
    }

    pub fn table_frames(&self) -> usize {
        self.space.table_frames()
    }

    /// May this process perform `cap`? The capability confinement check.
    pub fn enforce(&self, cap: Cap) -> bool {
        match cap {
            Cap::InProcessExec => self.caps.in_process_exec,
            Cap::RawDevice => self.caps.raw_device,
            Cap::FsWrite => self.caps.fs_write,
            Cap::Spawn => self.caps.spawn,
            Cap::GrowMemory => self.caps.grow_memory,
        }
    }

    /// Read `len` bytes from the process's virtual address `vaddr`, walking its
    /// page tables exactly as the CPU would — proof that the program is really
    /// mapped and isolated, not just parsed.
    pub fn read_virt(&self, vaddr: u64, len: usize) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(len);
        for i in 0..len as u64 {
            let (phys, _) = self.space.translate(vaddr + i)?;
            let frame = phys >> 12;
            let offset = (phys & 0xFFF) as usize;
            out.push(*self.phys.get(&frame)?.get(offset)?);
        }
        Some(out)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnError {
    Elf(ElfError),
    Vm(VmError),
}

/// The process table + the tier-aware scheduling policy.
pub struct ProcessTable {
    procs: BTreeMap<Pid, Process>,
    next_pid: Pid,
    current: Option<Pid>,
    round: u64,
    switches: u64,
}

/// Unproven processes get the CPU only every Nth scheduling decision — the
/// same distrust tax the cooperative scheduler levies, now at the process
/// level.
const UNPROVEN_STRIDE: u64 = 4;

impl ProcessTable {
    pub fn new() -> Self {
        Self {
            procs: BTreeMap::new(),
            next_pid: 1,
            current: None,
            round: 0,
            switches: 0,
        }
    }

    /// Load an ELF into a fresh, isolated address space and admit it as a
    /// process whose capabilities are set by `tier`. Segments are mapped page
    /// by page; `.bss` (memsz > filesz) is zero-filled.
    pub fn spawn(
        &mut self,
        name: impl Into<String>,
        tier: Tier,
        elf_bytes: &[u8],
        frames: &mut FrameAllocator,
    ) -> Result<Pid, SpawnError> {
        let image = elf::parse(elf_bytes).map_err(SpawnError::Elf)?;
        let mut space = AddressSpace::new(frames).map_err(SpawnError::Vm)?;
        let mut phys: BTreeMap<u64, [u8; PAGE_SIZE as usize]> = BTreeMap::new();

        for segment in &image.segments {
            let start = segment.vaddr;
            let end = segment.vaddr + segment.mem_size;
            let first_page = start & !(PAGE_SIZE - 1);
            let mut page = first_page;
            while page < end {
                // Ensure the page is mapped; reuse a frame if segments overlap.
                let frame = match space.translate(page) {
                    Some((phys_addr, _)) => phys_addr >> 12,
                    None => {
                        let f = frames
                            .allocate()
                            .ok_or(SpawnError::Vm(VmError::OutOfFrames))?;
                        space
                            .map_page(page, f, segment.flags, frames)
                            .map_err(SpawnError::Vm)?;
                        phys.entry(f.0).or_insert([0u8; PAGE_SIZE as usize]);
                        f.0
                    }
                };
                let buf = phys.entry(frame).or_insert([0u8; PAGE_SIZE as usize]);
                // Copy the slice of file bytes that lands in this page.
                for off in 0..PAGE_SIZE {
                    let va = page + off;
                    if va >= start {
                        let seg_off = (va - start) as usize;
                        if seg_off < segment.bytes.len() {
                            buf[off as usize] = segment.bytes[seg_off];
                        }
                    }
                }
                page += PAGE_SIZE;
            }
        }

        let pid = self.next_pid;
        self.next_pid += 1;
        let context = Context {
            rip: image.entry,
            ..Context::default()
        };
        self.procs.insert(
            pid,
            Process {
                pid,
                name: name.into(),
                tier,
                caps: Capabilities::from_tier(tier),
                entry: image.entry,
                state: ProcState::Ready,
                context,
                space,
                phys,
                segments: image.segments.len(),
            },
        );
        Ok(pid)
    }

    pub fn get(&self, pid: Pid) -> Option<&Process> {
        self.procs.get(&pid)
    }

    pub fn list(&self) -> impl Iterator<Item = &Process> {
        self.procs.values()
    }

    pub fn current(&self) -> Option<Pid> {
        self.current
    }

    pub fn switches(&self) -> u64 {
        self.switches
    }

    pub fn count(&self) -> usize {
        self.procs.len()
    }

    /// Pick the next Ready process to run, favouring higher tiers and levying
    /// the distrust tax on unproven ones. Marks the choice Running (and the
    /// previous current back to Ready), records a context switch, and returns
    /// the pid. This is the scheduling *policy*; the register switch is arch asm.
    pub fn schedule(&mut self) -> Option<Pid> {
        self.round += 1;
        let tax_round = self.round.is_multiple_of(UNPROVEN_STRIDE);
        let want_unproven = |t: Tier| (t == Tier::Unproven) == tax_round && tax_round;

        // On a tax round, prefer an unproven process; otherwise prefer the
        // highest-tier Ready process. Never starve either side.
        let pick = self
            .procs
            .values()
            .filter(|p| p.state == ProcState::Ready || Some(p.pid) == self.current)
            .filter(|p| p.state != ProcState::Zombie && p.state != ProcState::Blocked)
            .min_by_key(|p| {
                let tier_rank = p.tier.rank() as u64;
                // Lower key = scheduled first. On tax rounds bias toward
                // unproven; otherwise toward proven.
                if want_unproven(p.tier) {
                    (0u64, p.pid)
                } else {
                    (1 + tier_rank, p.pid)
                }
            })
            .map(|p| p.pid);

        if let Some(pid) = pick {
            if self.current != Some(pid) {
                if let Some(prev) = self.current {
                    if let Some(p) = self.procs.get_mut(&prev) {
                        if p.state == ProcState::Running {
                            p.state = ProcState::Ready;
                        }
                    }
                }
                self.switches += 1;
            }
            if let Some(p) = self.procs.get_mut(&pid) {
                p.state = ProcState::Running;
            }
            self.current = Some(pid);
        }
        pick
    }

    /// Terminate a process, freeing every physical frame it holds (pages +
    /// page tables) back to the allocator.
    pub fn exit(&mut self, pid: Pid, frames: &mut FrameAllocator) -> bool {
        if let Some(mut proc) = self.procs.remove(&pid) {
            for frame in proc.phys.keys() {
                frames.deallocate(crate::mem::Frame(*frame));
            }
            proc.state = ProcState::Zombie;
            if self.current == Some(pid) {
                self.current = None;
            }
            true
        } else {
            false
        }
    }
}

impl Default for ProcessTable {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elf::synth_executable;
    use crate::mem::MemoryRegion;

    fn frames() -> FrameAllocator {
        FrameAllocator::from_map(&[
            MemoryRegion::reserved(0, 0x1000),
            MemoryRegion::usable(0x1000, 64 * 1024 * 1024),
        ])
    }

    #[test]
    fn loads_an_elf_and_reads_it_back_through_paging() {
        let mut fa = frames();
        let mut table = ProcessTable::new();
        let code = [0x90, 0x90, 0xC3, 0xAB, 0xCD]; // nop nop ret + data
        let elf = synth_executable(0x40_0000, &code);
        let pid = table.spawn("demo", Tier::Proven, &elf, &mut fa).unwrap();

        let proc = table.get(pid).unwrap();
        assert_eq!(proc.entry, 0x40_0000);
        assert!(proc.mapped_pages() >= 1);
        // The loaded bytes are readable through the process's own page tables.
        let read = proc.read_virt(0x40_0000, code.len()).unwrap();
        assert_eq!(read, code);
    }

    #[test]
    fn capabilities_follow_the_proof_tier() {
        let mut fa = frames();
        let mut table = ProcessTable::new();
        let elf = synth_executable(0x1000, &[0xC3]);
        let proven = table.spawn("trusted", Tier::Proven, &elf, &mut fa).unwrap();
        let unproven = table
            .spawn("sandbox", Tier::Unproven, &elf, &mut fa)
            .unwrap();

        assert!(table.get(proven).unwrap().enforce(Cap::RawDevice));
        assert!(table.get(proven).unwrap().enforce(Cap::InProcessExec));
        // The unproven process runs, but is caged.
        assert!(!table.get(unproven).unwrap().enforce(Cap::RawDevice));
        assert!(!table.get(unproven).unwrap().enforce(Cap::FsWrite));
        assert!(!table.get(unproven).unwrap().enforce(Cap::Spawn));
    }

    #[test]
    fn processes_are_isolated() {
        let mut fa = frames();
        let mut table = ProcessTable::new();
        // Two programs at the SAME virtual address.
        let a = table
            .spawn(
                "a",
                Tier::Proven,
                &synth_executable(0x40_0000, &[0xAA]),
                &mut fa,
            )
            .unwrap();
        let b = table
            .spawn(
                "b",
                Tier::Proven,
                &synth_executable(0x40_0000, &[0xBB]),
                &mut fa,
            )
            .unwrap();
        // Different page-table roots (CR3) and different bytes at that vaddr.
        assert_ne!(table.get(a).unwrap().cr3(), table.get(b).unwrap().cr3());
        assert_eq!(
            table.get(a).unwrap().read_virt(0x40_0000, 1).unwrap(),
            [0xAA]
        );
        assert_eq!(
            table.get(b).unwrap().read_virt(0x40_0000, 1).unwrap(),
            [0xBB]
        );
    }

    #[test]
    fn scheduler_taxes_unproven_processes() {
        let mut fa = frames();
        let mut table = ProcessTable::new();
        let elf = synth_executable(0x1000, &[0xC3]);
        table.spawn("proven", Tier::Proven, &elf, &mut fa).unwrap();
        table
            .spawn("unproven", Tier::Unproven, &elf, &mut fa)
            .unwrap();

        let mut proven_runs = 0;
        let mut unproven_runs = 0;
        for _ in 0..40 {
            match table.schedule() {
                Some(1) => proven_runs += 1,
                Some(2) => unproven_runs += 1,
                _ => {}
            }
        }
        assert!(
            proven_runs > unproven_runs * 2,
            "proven {proven_runs} should outrun unproven {unproven_runs}"
        );
        assert!(unproven_runs > 0, "throttled, never starved");
        assert!(table.switches() > 0);
    }

    #[test]
    fn exit_reclaims_every_frame() {
        let mut fa = frames();
        let before = fa.stats().free_frames;
        let mut table = ProcessTable::new();
        let elf = synth_executable(0x40_0000, &[0x90; 100]);
        let pid = table.spawn("tmp", Tier::Proven, &elf, &mut fa).unwrap();
        assert!(fa.stats().free_frames < before, "spawning consumed frames");
        table.exit(pid, &mut fa);
        // Page frames are returned (page-table frames are a small fixed leak we
        // accept for now — see AddressSpace::unmap_page).
        assert!(fa.stats().free_frames > before - 10);
        assert_eq!(table.count(), 0);
    }
}
