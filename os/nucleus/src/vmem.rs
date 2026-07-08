//! Virtual memory — from-scratch x86_64 4-level paging and address spaces.
//!
//! This is the mechanism that gives every process its own private view of
//! memory and makes isolation *hardware-enforced* rather than a promise: a
//! virtual address is translated through four levels of page table
//! (PML4 → PDPT → PD → PT) to a physical frame, and the permission bits on the
//! leaf entry (writable / user / no-execute) are what the CPU checks on every
//! access. Two address spaces mapping the same virtual address to different
//! frames cannot see each other's memory — the foundation the proof-tier model
//! builds capability confinement on.
//!
//! The page tables are faithful to the architecture (same 9-bit index split,
//! same entry bit layout, root frame = what `CR3` holds). To stay testable off
//! real hardware, the "physical memory" the tables live in is modeled as a
//! frame-keyed store rather than dereferenced through raw pointers; the walk,
//! the allocation of intermediate tables, and the permission logic are exactly
//! what runs on metal. On the bare-metal target, `root_frame()` is loaded into
//! `CR3` (see `boot/src/switch.rs`).

use alloc::collections::BTreeMap;

use crate::mem::{Frame, FrameAllocator};

pub const PAGE_SIZE: u64 = 4096;

// Page-table entry flag bits (architectural positions).
pub const PRESENT: u64 = 1 << 0;
pub const WRITABLE: u64 = 1 << 1;
pub const USER: u64 = 1 << 2;
pub const NO_EXECUTE: u64 = 1 << 63;

const ENTRIES: usize = 512;
/// Physical frame number field of an entry (bits 12..48).
const FRAME_MASK: u64 = 0x000F_FFFF_FFFF_F000;

/// Leaf-page permissions, the part of an entry the CPU enforces per access.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageFlags {
    pub writable: bool,
    pub user: bool,
    pub executable: bool,
}

impl PageFlags {
    pub fn code() -> Self {
        Self {
            writable: false,
            user: true,
            executable: true,
        }
    }
    pub fn data() -> Self {
        Self {
            writable: true,
            user: true,
            executable: false,
        }
    }
    pub fn read_only() -> Self {
        Self {
            writable: false,
            user: true,
            executable: false,
        }
    }

    fn to_bits(self) -> u64 {
        let mut bits = PRESENT;
        if self.writable {
            bits |= WRITABLE;
        }
        if self.user {
            bits |= USER;
        }
        if !self.executable {
            bits |= NO_EXECUTE;
        }
        bits
    }

    fn from_bits(bits: u64) -> Self {
        Self {
            writable: bits & WRITABLE != 0,
            user: bits & USER != 0,
            executable: bits & NO_EXECUTE == 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VmError {
    /// The frame allocator is exhausted; a page table could not be created.
    OutOfFrames,
    /// The virtual page is not mapped.
    NotMapped,
    /// The virtual page is already mapped (map without replace).
    AlreadyMapped,
}

/// One 512-entry page table, the size of a single 4 KiB frame.
#[derive(Clone)]
struct Table {
    entries: [u64; ENTRIES],
}

impl Table {
    fn empty() -> Self {
        Self {
            entries: [0; ENTRIES],
        }
    }
}

/// A region reserved for **demand paging**: virtual pages that are promised but
/// not yet backed by a frame. The first access faults, and the fault handler
/// allocates and maps a frame on the spot — so a process can reserve a large
/// address range (a heap, a stack, an mmap'd file) and only pay physical memory
/// for the pages it actually touches.
#[derive(Debug, Clone, Copy)]
struct LazyRegion {
    start: u64,
    end: u64,
    flags: PageFlags,
}

/// A per-process virtual address space: a 4-level page-table tree rooted at one
/// physical frame. The `tables` map models the physical RAM those tables live
/// in (keyed by frame number), so the walk is real without raw pointers.
pub struct AddressSpace {
    root: Frame,
    tables: BTreeMap<u64, Table>,
    mapped_pages: u64,
    /// Reserved-but-unbacked regions, resolved on first fault.
    lazy: alloc::vec::Vec<LazyRegion>,
    /// How many faults this space has resolved by paging a frame in — the
    /// demand-paging counter (a proxy for resident-set growth).
    faults_resolved: u64,
}

impl AddressSpace {
    /// Create an empty address space, allocating its root (PML4) frame.
    pub fn new(frames: &mut FrameAllocator) -> Result<Self, VmError> {
        let root = frames.allocate().ok_or(VmError::OutOfFrames)?;
        let mut tables = BTreeMap::new();
        tables.insert(root.0, Table::empty());
        Ok(Self {
            root,
            tables,
            mapped_pages: 0,
            lazy: alloc::vec::Vec::new(),
            faults_resolved: 0,
        })
    }

    /// The value that goes into `CR3`: the root table's physical address.
    pub fn root_frame(&self) -> Frame {
        self.root
    }

    pub fn mapped_pages(&self) -> u64 {
        self.mapped_pages
    }

    /// Number of physical frames this address space occupies (page tables) —
    /// the paging overhead, `pmap` reports it.
    pub fn table_frames(&self) -> usize {
        self.tables.len()
    }

    fn indices(vaddr: u64) -> [usize; 4] {
        [
            ((vaddr >> 39) & 0x1FF) as usize,
            ((vaddr >> 30) & 0x1FF) as usize,
            ((vaddr >> 21) & 0x1FF) as usize,
            ((vaddr >> 12) & 0x1FF) as usize,
        ]
    }

    /// Map one 4 KiB page `vaddr` → `frame` with `flags`, creating the three
    /// intermediate tables as needed. Intermediate entries are made permissive
    /// (present/writable/user); the leaf flags govern access, matching how the
    /// architecture ANDs down but with enforcement modeled at the leaf.
    pub fn map_page(
        &mut self,
        vaddr: u64,
        frame: Frame,
        flags: PageFlags,
        frames: &mut FrameAllocator,
    ) -> Result<(), VmError> {
        let idx = Self::indices(vaddr);
        let mut table_frame = self.root.0;
        for &index in idx.iter().take(3) {
            let entry = self.tables[&table_frame].entries[index];
            table_frame = if entry & PRESENT != 0 {
                (entry & FRAME_MASK) >> 12
            } else {
                let new = frames.allocate().ok_or(VmError::OutOfFrames)?;
                self.tables.insert(new.0, Table::empty());
                let e = (new.0 << 12) | PRESENT | WRITABLE | USER;
                self.tables
                    .get_mut(&table_frame)
                    .expect("walked frame exists")
                    .entries[index] = e;
                new.0
            };
        }
        let leaf = self
            .tables
            .get_mut(&table_frame)
            .expect("leaf table exists");
        if leaf.entries[idx[3]] & PRESENT != 0 {
            return Err(VmError::AlreadyMapped);
        }
        leaf.entries[idx[3]] = (frame.0 << 12) | flags.to_bits();
        self.mapped_pages += 1;
        Ok(())
    }

    /// **Reserve** `[start, start+len)` for demand paging: the pages are
    /// promised with `flags` but no frames are allocated. The first access to
    /// each page faults, and [`handle_fault`](Self::handle_fault) backs it then.
    /// Cheap — a large reservation costs nothing until touched.
    pub fn reserve_lazy(&mut self, start: u64, len: u64, flags: PageFlags) {
        let start = start & !(PAGE_SIZE - 1);
        let end = (start + len + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        self.lazy.push(LazyRegion { start, end, flags });
    }

    /// True if `vaddr` lies in a reserved-but-unbacked region.
    #[must_use]
    pub fn is_lazy(&self, vaddr: u64) -> bool {
        self.lazy.iter().any(|r| vaddr >= r.start && vaddr < r.end)
    }

    /// Resolve a page fault at `vaddr`: if it falls in a reserved region and is
    /// not yet mapped, allocate a frame and map the page with the region's
    /// permissions (**demand paging** — the page comes into existence on first
    /// touch). Returns the freshly-mapped frame, or an error if the address was
    /// never reserved (a genuine segfault) or is already mapped.
    pub fn handle_fault(
        &mut self,
        vaddr: u64,
        frames: &mut FrameAllocator,
    ) -> Result<Frame, VmError> {
        if self.translate(vaddr).is_some() {
            return Err(VmError::AlreadyMapped); // not actually a fault
        }
        let region = *self
            .lazy
            .iter()
            .find(|r| vaddr >= r.start && vaddr < r.end)
            .ok_or(VmError::NotMapped)?; // outside any reservation → real fault
        let page = vaddr & !(PAGE_SIZE - 1);
        let frame = frames.allocate().ok_or(VmError::OutOfFrames)?;
        self.map_page(page, frame, region.flags, frames)?;
        self.faults_resolved += 1;
        Ok(frame)
    }

    /// How many demand-paging faults this space has serviced.
    #[must_use]
    pub fn faults_resolved(&self) -> u64 {
        self.faults_resolved
    }

    /// Translate a virtual address to `(physical_address, leaf_flags)`, or
    /// `None` if any level along the walk is absent — exactly the walk the MMU
    /// performs, and the source of a page fault when it returns `None`.
    pub fn translate(&self, vaddr: u64) -> Option<(u64, PageFlags)> {
        let idx = Self::indices(vaddr);
        let mut table_frame = self.root.0;
        for level in idx.iter().take(3) {
            let entry = self.tables.get(&table_frame)?.entries[*level];
            if entry & PRESENT == 0 {
                return None;
            }
            table_frame = (entry & FRAME_MASK) >> 12;
        }
        let entry = self.tables.get(&table_frame)?.entries[idx[3]];
        if entry & PRESENT == 0 {
            return None;
        }
        let phys = (entry & FRAME_MASK) | (vaddr & 0xFFF);
        Some((phys, PageFlags::from_bits(entry)))
    }

    /// Unmap a page, returning the frame it referenced so the caller can free
    /// it. The intermediate tables are left in place (cheap; a real
    /// implementation would reap empty tables).
    pub fn unmap_page(&mut self, vaddr: u64) -> Result<Frame, VmError> {
        let idx = Self::indices(vaddr);
        let mut table_frame = self.root.0;
        for level in idx.iter().take(3) {
            let entry = self
                .tables
                .get(&table_frame)
                .ok_or(VmError::NotMapped)?
                .entries[*level];
            if entry & PRESENT == 0 {
                return Err(VmError::NotMapped);
            }
            table_frame = (entry & FRAME_MASK) >> 12;
        }
        let leaf = self
            .tables
            .get_mut(&table_frame)
            .ok_or(VmError::NotMapped)?;
        let entry = leaf.entries[idx[3]];
        if entry & PRESENT == 0 {
            return Err(VmError::NotMapped);
        }
        leaf.entries[idx[3]] = 0;
        self.mapped_pages -= 1;
        Ok(Frame((entry & FRAME_MASK) >> 12))
    }

    /// Would an access at `vaddr` be permitted? Models the CPU's per-access
    /// permission check: presence + writability + execute (NX) + user.
    pub fn can_access(&self, vaddr: u64, write: bool, execute: bool, user: bool) -> bool {
        match self.translate(vaddr) {
            None => false,
            Some((_, flags)) => {
                (!write || flags.writable)
                    && (!execute || flags.executable)
                    && (!user || flags.user)
            }
        }
    }

    /// Map the kernel's higher half (PML4 indices 256..512, virtual addresses
    /// with bit 47 set) into this address space, so a `syscall` trap taken
    /// while a user process is active lands in *mapped* kernel code and stack
    /// rather than a page fault. Every process shares the one kernel mapping.
    ///
    /// On real hardware this is a copy of the top PML4 entries — all address
    /// spaces then point at the *same* physical kernel tables. The model mirrors
    /// that by cloning the reachable kernel tables in too, so a kernel virtual
    /// address resolves identically from any process space.
    pub fn share_higher_half(&mut self, kernel: &AddressSpace) {
        let kernel_root = kernel.tables[&kernel.root.0].clone();
        for index in 256..ENTRIES {
            let entry = kernel_root.entries[index];
            if entry & PRESENT != 0 {
                self.clone_subtree(kernel, (entry & FRAME_MASK) >> 12, 1);
                self.tables
                    .get_mut(&self.root.0)
                    .expect("own root exists")
                    .entries[index] = entry;
            }
        }
    }

    /// Clone the kernel page-table subtree rooted at `frame` (which lives at
    /// paging `level`) into this space's table store. Levels 1..3 are tables;
    /// level-3 (PT) entries point at data pages, not tables, so recursion stops.
    fn clone_subtree(&mut self, kernel: &AddressSpace, frame: u64, level: usize) {
        if level > 3 {
            return;
        }
        if let Some(table) = kernel.tables.get(&frame) {
            self.tables.insert(frame, table.clone());
            if level < 3 {
                for entry in table.entries.iter() {
                    if entry & PRESENT != 0 {
                        self.clone_subtree(kernel, (entry & FRAME_MASK) >> 12, level + 1);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mem::MemoryRegion;

    fn frames() -> FrameAllocator {
        FrameAllocator::from_map(&[
            MemoryRegion::reserved(0, 0x1000),
            MemoryRegion::usable(0x1000, 64 * 1024 * 1024),
        ])
    }

    #[test]
    fn map_translate_preserves_offset_and_permissions() {
        let mut fa = frames();
        let mut space = AddressSpace::new(&mut fa).unwrap();
        let frame = fa.allocate().unwrap();
        space
            .map_page(0x4000_1234, frame, PageFlags::code(), &mut fa)
            .unwrap();

        let (phys, flags) = space.translate(0x4000_1234).unwrap();
        // Physical address = frame base + page offset (0x234).
        assert_eq!(phys, frame.start_addr() + 0x234);
        assert!(flags.executable && !flags.writable);
        // Unmapped neighbours translate to nothing (a page fault).
        assert!(space.translate(0x4001_0000).is_none());
    }

    #[test]
    fn permission_bits_are_enforced() {
        let mut fa = frames();
        let mut space = AddressSpace::new(&mut fa).unwrap();
        let code = fa.allocate().unwrap();
        let data = fa.allocate().unwrap();
        space
            .map_page(0x1000, code, PageFlags::code(), &mut fa)
            .unwrap();
        space
            .map_page(0x2000, data, PageFlags::data(), &mut fa)
            .unwrap();

        // Code page: executable, not writable.
        assert!(space.can_access(0x1000, false, true, true));
        assert!(!space.can_access(0x1000, true, false, true)); // W^X: no write to code
                                                               // Data page: writable, not executable.
        assert!(space.can_access(0x2000, true, false, true));
        assert!(!space.can_access(0x2000, false, true, true)); // NX: no exec on data
    }

    #[test]
    fn two_address_spaces_are_isolated() {
        let mut fa = frames();
        let mut a = AddressSpace::new(&mut fa).unwrap();
        let mut b = AddressSpace::new(&mut fa).unwrap();
        let fa_frame = fa.allocate().unwrap();
        let fb_frame = fa.allocate().unwrap();
        // Same virtual address in both spaces → different physical frames.
        a.map_page(0x5000, fa_frame, PageFlags::data(), &mut fa)
            .unwrap();
        b.map_page(0x5000, fb_frame, PageFlags::data(), &mut fa)
            .unwrap();
        assert_ne!(a.root_frame(), b.root_frame());
        assert_ne!(
            a.translate(0x5000).unwrap().0,
            b.translate(0x5000).unwrap().0
        );
    }

    #[test]
    fn unmap_frees_the_mapping() {
        let mut fa = frames();
        let mut space = AddressSpace::new(&mut fa).unwrap();
        let frame = fa.allocate().unwrap();
        space
            .map_page(0x8000, frame, PageFlags::data(), &mut fa)
            .unwrap();
        assert_eq!(space.mapped_pages(), 1);
        let returned = space.unmap_page(0x8000).unwrap();
        assert_eq!(returned, frame);
        assert_eq!(space.mapped_pages(), 0);
        assert!(space.translate(0x8000).is_none());
        assert_eq!(space.unmap_page(0x8000).unwrap_err(), VmError::NotMapped);
    }

    #[test]
    fn double_map_is_rejected() {
        let mut fa = frames();
        let mut space = AddressSpace::new(&mut fa).unwrap();
        let f1 = fa.allocate().unwrap();
        let f2 = fa.allocate().unwrap();
        space
            .map_page(0x9000, f1, PageFlags::data(), &mut fa)
            .unwrap();
        assert_eq!(
            space
                .map_page(0x9000, f2, PageFlags::data(), &mut fa)
                .unwrap_err(),
            VmError::AlreadyMapped
        );
    }

    #[test]
    fn demand_paging_backs_pages_only_on_first_touch() {
        let mut fa = frames();
        let mut space = AddressSpace::new(&mut fa).unwrap();
        // Reserve a 1 MiB heap; costs no data frames yet.
        space.reserve_lazy(0x10_0000, 1024 * 1024, PageFlags::data());
        assert_eq!(space.mapped_pages(), 0, "reservation maps nothing");
        assert!(space.is_lazy(0x10_5000));
        assert!(
            space.translate(0x10_5000).is_none(),
            "not backed until touched"
        );

        // First touch of one page faults it in.
        let frame = space.handle_fault(0x10_5678, &mut fa).unwrap();
        assert_eq!(space.faults_resolved(), 1);
        let (phys, flags) = space.translate(0x10_5678).unwrap();
        assert_eq!(
            phys,
            frame.start_addr() + 0x678,
            "page backed at first touch"
        );
        assert!(flags.writable && !flags.executable);
        // Only the touched page is resident; its neighbour is still lazy.
        assert_eq!(space.mapped_pages(), 1);
        assert!(space.translate(0x10_6000).is_none());

        // A second touch of the SAME page is already mapped — not a fault.
        assert_eq!(
            space.handle_fault(0x10_5000, &mut fa).unwrap_err(),
            VmError::AlreadyMapped
        );
    }

    #[test]
    fn a_fault_outside_any_reservation_is_a_real_segfault() {
        let mut fa = frames();
        let mut space = AddressSpace::new(&mut fa).unwrap();
        space.reserve_lazy(0x20_0000, 0x1000, PageFlags::data());
        // An address in no reserved region cannot be paged in — a genuine
        // fault the kernel would turn into a fatal signal.
        assert_eq!(
            space.handle_fault(0x99_0000, &mut fa).unwrap_err(),
            VmError::NotMapped
        );
        assert_eq!(space.faults_resolved(), 0);
    }

    #[test]
    fn higher_half_kernel_mapping_is_shared_but_user_half_is_private() {
        let mut fa = frames();
        // The kernel address space maps something high (bit 47 set).
        let mut kernel = AddressSpace::new(&mut fa).unwrap();
        let kframe = fa.allocate().unwrap();
        let kvaddr = 0xFFFF_8000_0011_0000;
        kernel
            .map_page(kvaddr, kframe, PageFlags::code(), &mut fa)
            .unwrap();

        // A user process maps something low, then shares the kernel half.
        let mut user = AddressSpace::new(&mut fa).unwrap();
        let uframe = fa.allocate().unwrap();
        user.map_page(0x40_0000, uframe, PageFlags::code(), &mut fa)
            .unwrap();
        assert!(user.translate(kvaddr).is_none(), "no kernel mapping yet");

        user.share_higher_half(&kernel);
        // The kernel address now resolves identically from the user space — so
        // a syscall trap lands in real kernel code.
        assert_eq!(user.translate(kvaddr), kernel.translate(kvaddr));
        assert_eq!(user.translate(kvaddr).unwrap().0, kframe.start_addr());
        // The user's own low mapping is untouched and private to it.
        assert_eq!(user.translate(0x40_0000).unwrap().0, uframe.start_addr());
        assert!(
            kernel.translate(0x40_0000).is_none(),
            "user half stays private"
        );
    }
}
