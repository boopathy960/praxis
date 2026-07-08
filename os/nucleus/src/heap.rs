//! The nucleus heap: a from-scratch first-fit free-list allocator.
//!
//! Free blocks store their own bookkeeping (`ListNode`) in place, so the
//! allocator needs zero memory of its own. The bare-metal binary points it at
//! a static region and installs [`LockedHeap`] as the global allocator; the
//! hosted runner leaves it unused (std brings its own) but the shell can still
//! report its stats when present.
//!
//! The list is kept **address-ordered and coalescing**: freeing a block merges
//! it with adjacent free neighbors on both sides, so the arena's capacity for
//! large blocks survives any alloc/free pattern. The workload that demands
//! this is `Vec` growth — alloc 2N, copy, free N, repeat — which on a
//! non-coalescing allocator shreds the arena into never-reusable fragments
//! until a big allocation fails while plenty of memory is nominally free
//! (exactly how the first real `fetch` of a web page killed the kernel).

use core::alloc::{GlobalAlloc, Layout};
use core::mem;
use core::ptr;

use crate::sync::SpinLock;

struct ListNode {
    size: usize,
    next: Option<&'static mut ListNode>,
}

impl ListNode {
    const fn new(size: usize) -> Self {
        Self { size, next: None }
    }
    fn start_addr(&self) -> usize {
        self as *const Self as usize
    }
    fn end_addr(&self) -> usize {
        self.start_addr() + self.size
    }
}

pub struct FreeList {
    head: ListNode,
    total: usize,
    used: usize,
}

impl FreeList {
    pub const fn empty() -> Self {
        Self {
            head: ListNode::new(0),
            total: 0,
            used: 0,
        }
    }

    /// Point the allocator at its arena.
    ///
    /// # Safety
    /// `start..start+size` must be valid, writable, unused memory that outlives
    /// the allocator, and `init` must be called exactly once.
    pub unsafe fn init(&mut self, start: usize, size: usize) {
        self.total = size;
        self.add_free_region(start, size);
    }

    /// Insert a free region at its address-ordered position, merging with an
    /// adjacent successor and/or predecessor so free space re-forms into the
    /// largest possible blocks.
    unsafe fn add_free_region(&mut self, addr: usize, mut size: usize) {
        debug_assert_eq!(align_up(addr, mem::align_of::<ListNode>()), addr);
        debug_assert!(size >= mem::size_of::<ListNode>());

        // Walk to the last node that starts before `addr`. `at_head` tracks
        // whether we are still on the sentinel (which owns no memory and must
        // never be merged into).
        let mut at_head = true;
        let mut current: &mut ListNode = &mut self.head;
        while current
            .next
            .as_ref()
            .is_some_and(|next| next.start_addr() < addr)
        {
            current = current.next.as_mut().expect("checked above");
            at_head = false;
        }

        // Coalesce forward: absorb the successor if it starts exactly where
        // this region ends.
        if let Some(next) = current.next.take() {
            if addr + size == next.start_addr() {
                size += next.size;
                current.next = next.next.take();
            } else {
                current.next = Some(next);
            }
        }

        // Coalesce backward: extend the predecessor in place if it ends
        // exactly where this region starts.
        if !at_head && current.end_addr() == addr {
            current.size += size;
            return;
        }

        let mut node = ListNode::new(size);
        node.next = current.next.take();
        let node_ptr = addr as *mut ListNode;
        node_ptr.write(node);
        current.next = Some(&mut *node_ptr);
    }

    fn find_region(&mut self, size: usize, align: usize) -> Option<(&'static mut ListNode, usize)> {
        let mut current = &mut self.head;
        while let Some(ref mut region) = current.next {
            if let Ok(alloc_start) = Self::alloc_from_region(region, size, align) {
                let next = region.next.take();
                let taken = current.next.take().expect("region exists");
                current.next = next;
                return Some((taken, alloc_start));
            }
            current = current.next.as_mut().expect("checked above");
        }
        None
    }

    fn alloc_from_region(region: &ListNode, size: usize, align: usize) -> Result<usize, ()> {
        let alloc_start = align_up(region.start_addr(), align);
        let alloc_end = alloc_start.checked_add(size).ok_or(())?;
        if alloc_end > region.end_addr() {
            return Err(());
        }
        // Any leftover — in front (alignment gap) or behind — must be big
        // enough to carry its own bookkeeping node, or it can't be a region.
        let gap_front = alloc_start - region.start_addr();
        if gap_front > 0 && gap_front < mem::size_of::<ListNode>() {
            return Err(());
        }
        let excess = region.end_addr() - alloc_end;
        if excess > 0 && excess < mem::size_of::<ListNode>() {
            return Err(());
        }
        Ok(alloc_start)
    }

    /// Every allocation is padded so a freed block can always host a node.
    fn size_align(layout: Layout) -> (usize, usize) {
        let layout = layout
            .align_to(mem::align_of::<ListNode>())
            .expect("alignment adjustment failed")
            .pad_to_align();
        let size = layout.size().max(mem::size_of::<ListNode>());
        (size, layout.align())
    }

    pub fn allocate(&mut self, layout: Layout) -> *mut u8 {
        let (size, align) = Self::size_align(layout);
        match self.find_region(size, align) {
            Some((region, alloc_start)) => {
                let alloc_end = alloc_start.checked_add(size).expect("overflow checked");
                // Snapshot the region's bounds FIRST: inserting the front gap
                // writes a fresh node at the region's own start address,
                // clobbering the header `region` points at.
                let region_start = region.start_addr();
                let region_end = region.end_addr();
                // Return both leftovers to the list: the alignment gap in
                // front (previously leaked) and the excess behind.
                let gap_front = alloc_start - region_start;
                if gap_front > 0 {
                    unsafe { self.add_free_region(region_start, gap_front) };
                }
                let excess = region_end - alloc_end;
                if excess > 0 {
                    unsafe { self.add_free_region(alloc_end, excess) };
                }
                self.used += size;
                alloc_start as *mut u8
            }
            None => ptr::null_mut(),
        }
    }

    /// # Safety
    /// `ptr` must come from `allocate` with the same `layout`.
    pub unsafe fn deallocate(&mut self, ptr: *mut u8, layout: Layout) {
        let (size, _) = Self::size_align(layout);
        self.used = self.used.saturating_sub(size);
        self.add_free_region(ptr as usize, size);
    }

    pub fn used(&self) -> usize {
        self.used
    }
    pub fn total(&self) -> usize {
        self.total
    }
}

fn align_up(addr: usize, align: usize) -> usize {
    (addr + align - 1) & !(align - 1)
}

/// The lockable, installable form: `#[global_allocator] static HEAP: LockedHeap`.
pub struct LockedHeap {
    inner: SpinLock<FreeList>,
}

impl LockedHeap {
    pub const fn empty() -> Self {
        Self {
            inner: SpinLock::new(FreeList::empty()),
        }
    }

    /// # Safety
    /// Same contract as [`FreeList::init`].
    pub unsafe fn init(&self, start: usize, size: usize) {
        self.inner.lock().init(start, size);
    }

    pub fn used(&self) -> usize {
        self.inner.lock().used()
    }
    pub fn total(&self) -> usize {
        self.inner.lock().total()
    }
}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        self.inner.lock().allocate(layout)
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        self.inner.lock().deallocate(ptr, layout);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arena() -> (FreeList, Vec<u8>) {
        // Keep the backing memory alive for the test's duration.
        let mut backing = vec![0u8; 64 * 1024];
        let start = align_up(backing.as_mut_ptr() as usize, mem::align_of::<ListNode>());
        let size = 64 * 1024 - (start - backing.as_mut_ptr() as usize);
        let mut list = FreeList::empty();
        unsafe { list.init(start, size) };
        (list, backing)
    }

    #[test]
    fn alloc_free_realloc_cycles() {
        let (mut heap, _backing) = arena();
        let layout = Layout::from_size_align(256, 8).unwrap();
        let a = heap.allocate(layout);
        let b = heap.allocate(layout);
        assert!(!a.is_null() && !b.is_null() && a != b);
        assert!(heap.used() >= 512);
        unsafe {
            heap.deallocate(a, layout);
            heap.deallocate(b, layout);
        }
        assert_eq!(heap.used(), 0);
        // The freed space is reusable.
        let c = heap.allocate(Layout::from_size_align(1024, 16).unwrap());
        assert!(!c.is_null());
    }

    #[test]
    fn exhaustion_returns_null_not_ub() {
        let (mut heap, _backing) = arena();
        let too_big = Layout::from_size_align(1 << 20, 8).unwrap();
        assert!(heap.allocate(too_big).is_null());
    }

    #[test]
    fn freed_neighbors_coalesce_back_into_big_blocks() {
        let (mut heap, _backing) = arena();
        let piece = Layout::from_size_align(8 * 1024, 8).unwrap();
        // Carve the arena into pieces, then free them ALL, in an interleaved
        // order so every merge direction (forward, backward, both) happens.
        let ptrs: Vec<*mut u8> = (0..6).map(|_| heap.allocate(piece)).collect();
        assert!(ptrs.iter().all(|p| !p.is_null()));
        for &i in &[1, 3, 5, 0, 2, 4] {
            unsafe { heap.deallocate(ptrs[i], piece) };
        }
        assert_eq!(heap.used(), 0);
        // Only a coalescing allocator can now serve one near-arena-sized
        // block out of what was six scattered pieces.
        let big = Layout::from_size_align(56 * 1024, 8).unwrap();
        assert!(
            !heap.allocate(big).is_null(),
            "free space failed to re-form into a large block"
        );
    }

    #[test]
    fn vec_doubling_growth_never_fragments_the_arena_to_death() {
        // The workload that killed the first real `fetch`: a growing buffer
        // that repeatedly allocates 2N, copies, and frees N. On a
        // non-coalescing allocator the freed halves are never reusable for
        // the next doubling and the arena starves; after the fix, growth up
        // to half the arena must always succeed — repeatedly.
        let (mut heap, _backing) = arena();
        for _ in 0..8 {
            let mut size = 64usize;
            let mut current = heap.allocate(Layout::from_size_align(size, 8).unwrap());
            assert!(!current.is_null());
            while size < 16 * 1024 {
                let doubled = Layout::from_size_align(size * 2, 8).unwrap();
                let next = heap.allocate(doubled);
                assert!(!next.is_null(), "doubling to {} bytes failed", size * 2);
                unsafe {
                    heap.deallocate(current, Layout::from_size_align(size, 8).unwrap());
                }
                current = next;
                size *= 2;
            }
            unsafe {
                heap.deallocate(current, Layout::from_size_align(size, 8).unwrap());
            }
        }
        assert_eq!(heap.used(), 0);
    }

    #[test]
    fn alignment_gaps_are_reclaimed_not_leaked() {
        let (mut heap, _backing) = arena();
        // Force front gaps with an oversized alignment, then free everything;
        // the arena must still be able to serve one near-full-size block.
        let picky = Layout::from_size_align(1024, 4096).unwrap();
        let a = heap.allocate(picky);
        let b = heap.allocate(picky);
        assert!(!a.is_null() && !b.is_null());
        unsafe {
            heap.deallocate(a, picky);
            heap.deallocate(b, picky);
        }
        assert_eq!(heap.used(), 0);
        let big = Layout::from_size_align(56 * 1024, 8).unwrap();
        assert!(!heap.allocate(big).is_null(), "alignment gaps were leaked");
    }
}
