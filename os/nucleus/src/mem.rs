//! Physical memory management — a from-scratch bitmap frame allocator.
//!
//! On bare metal the bootloader hands the kernel a **memory map**: a list of
//! physical regions and whether each is usable. This module turns that map
//! into an allocator of 4 KiB physical frames, the unit paging hands out.
//! One bit per frame: 0 = free, 1 = used. Allocation is a linear scan from a
//! rotating cursor (so freshly-freed frames get reused promptly without an
//! O(n) rescan every time), and every usable frame is accounted for.
//!
//! The bitmap itself lives on the nucleus heap. On real hardware that would be
//! a bootstrap hazard (the heap needs frames); here the heap is a fixed BSS
//! arena that exists *before* paging, so a heap-backed bitmap is sound. When
//! this graduates to managing paging directly, the bitmap moves into a
//! reserved frame carved from the map before anything else is allocated.

use alloc::vec::Vec;

/// The architectural page/frame size.
pub const FRAME_SIZE: u64 = 4096;

/// A physical frame, identified by its number (`addr / FRAME_SIZE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Frame(pub u64);

impl Frame {
    pub fn start_addr(self) -> u64 {
        self.0 * FRAME_SIZE
    }
    pub fn containing(addr: u64) -> Self {
        Frame(addr / FRAME_SIZE)
    }
}

/// One region from the bootloader's memory map.
#[derive(Debug, Clone, Copy)]
pub struct MemoryRegion {
    pub start: u64,
    pub end: u64,
    pub usable: bool,
}

impl MemoryRegion {
    pub fn usable(start: u64, end: u64) -> Self {
        Self {
            start,
            end,
            usable: true,
        }
    }
    pub fn reserved(start: u64, end: u64) -> Self {
        Self {
            start,
            end,
            usable: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FrameStats {
    pub total_frames: u64,
    pub used_frames: u64,
    pub free_frames: u64,
    pub allocations: u64,
    pub deallocations: u64,
}

impl FrameStats {
    pub fn free_bytes(&self) -> u64 {
        self.free_frames * FRAME_SIZE
    }
    pub fn total_bytes(&self) -> u64 {
        self.total_frames * FRAME_SIZE
    }
}

/// The bitmap frame allocator.
pub struct FrameAllocator {
    /// One bit per frame; bit set = used.
    bitmap: Vec<u64>,
    total_frames: u64,
    used_frames: u64,
    cursor: u64,
    allocations: u64,
    deallocations: u64,
}

impl FrameAllocator {
    /// Build from a memory map. Every frame starts *used*; the usable regions
    /// are then released. Frames overlapping any reserved region stay used, so
    /// a usable region that brushes a reserved one never hands out a bad frame.
    pub fn from_map(regions: &[MemoryRegion]) -> Self {
        // Size the bitmap to actual RAM — the maximum end of a *usable* region.
        // Reserved regions above RAM (MMIO apertures, and the bootloader's
        // physical-memory-mapping window that can reach 0x100_0000_0000 / 1 TiB)
        // are not backed by frames we hand out, and must not balloon the bitmap.
        let max_addr = regions
            .iter()
            .filter(|r| r.usable)
            .map(|r| r.end)
            .max()
            .unwrap_or(0);
        let total_frames = max_addr.div_ceil(FRAME_SIZE);
        let words = (total_frames.div_ceil(64)) as usize;
        let mut allocator = Self {
            bitmap: alloc::vec![u64::MAX; words], // all used
            total_frames,
            used_frames: total_frames,
            cursor: 0,
            allocations: 0,
            deallocations: 0,
        };
        // Release usable frames.
        for region in regions.iter().filter(|r| r.usable) {
            let first = region.start.div_ceil(FRAME_SIZE);
            let last = region.end / FRAME_SIZE; // exclusive upper frame
            for frame in first..last {
                allocator.set_free(frame);
            }
        }
        // Re-mark any frame touched by a reserved region as used (overlap wins).
        for region in regions.iter().filter(|r| !r.usable) {
            let first = region.start / FRAME_SIZE;
            let last = region.end.div_ceil(FRAME_SIZE);
            for frame in first..last.min(total_frames) {
                allocator.set_used(frame);
            }
        }
        allocator
    }

    fn set_free(&mut self, frame: u64) {
        if frame >= self.total_frames {
            return;
        }
        let (word, bit) = (frame as usize / 64, frame % 64);
        if self.bitmap[word] & (1 << bit) != 0 {
            self.bitmap[word] &= !(1 << bit);
            self.used_frames -= 1;
        }
    }

    fn set_used(&mut self, frame: u64) {
        if frame >= self.total_frames {
            return;
        }
        let (word, bit) = (frame as usize / 64, frame % 64);
        if self.bitmap[word] & (1 << bit) == 0 {
            self.bitmap[word] |= 1 << bit;
            self.used_frames += 1;
        }
    }

    fn is_used(&self, frame: u64) -> bool {
        let (word, bit) = (frame as usize / 64, frame % 64);
        self.bitmap[word] & (1 << bit) != 0
    }

    /// Allocate one free frame, or `None` when memory is exhausted. Scans from
    /// a rotating cursor so recently-freed frames are found without rescanning
    /// from zero each time.
    pub fn allocate(&mut self) -> Option<Frame> {
        if self.used_frames >= self.total_frames {
            return None;
        }
        for offset in 0..self.total_frames {
            let frame = (self.cursor + offset) % self.total_frames;
            if !self.is_used(frame) {
                self.set_used(frame);
                self.cursor = (frame + 1) % self.total_frames;
                self.allocations += 1;
                return Some(Frame(frame));
            }
        }
        None
    }

    /// Allocate `n` *contiguous* free frames — what a DMA buffer or a large
    /// page needs. Returns the first frame, or `None` if no run is long enough.
    pub fn allocate_contiguous(&mut self, n: u64) -> Option<Frame> {
        if n == 0 || n > self.total_frames - self.used_frames {
            return None;
        }
        let mut run_start = 0u64;
        let mut run = 0u64;
        for frame in 0..self.total_frames {
            if self.is_used(frame) {
                run = 0;
                run_start = frame + 1;
            } else {
                run += 1;
                if run == n {
                    for f in run_start..run_start + n {
                        self.set_used(f);
                    }
                    self.allocations += n;
                    return Some(Frame(run_start));
                }
            }
        }
        None
    }

    /// Return a frame to the free pool.
    pub fn deallocate(&mut self, frame: Frame) {
        if frame.0 < self.total_frames && self.is_used(frame.0) {
            self.set_free(frame.0);
            self.deallocations += 1;
        }
    }

    pub fn stats(&self) -> FrameStats {
        FrameStats {
            total_frames: self.total_frames,
            used_frames: self.used_frames,
            free_frames: self.total_frames - self.used_frames,
            allocations: self.allocations,
            deallocations: self.deallocations,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allocator() -> FrameAllocator {
        // 1 MiB reserved (BIOS/kernel), then usable up to 16 MiB, with a
        // reserved MMIO hole at 8..8.5 MiB to prove overlap handling.
        FrameAllocator::from_map(&[
            MemoryRegion::reserved(0, 1024 * 1024),
            MemoryRegion::usable(1024 * 1024, 16 * 1024 * 1024),
            MemoryRegion::reserved(8 * 1024 * 1024, 8 * 1024 * 1024 + 512 * 1024),
        ])
    }

    #[test]
    fn allocation_accounting_is_exact() {
        let mut fa = allocator();
        let before = fa.stats();
        assert!(before.free_frames > 3000 && before.free_frames < 3840);
        let a = fa.allocate().unwrap();
        let b = fa.allocate().unwrap();
        assert_ne!(a, b);
        assert_eq!(fa.stats().free_frames, before.free_frames - 2);
        assert!(
            a.start_addr() >= 1024 * 1024,
            "never hands out reserved frames"
        );
        fa.deallocate(a);
        assert_eq!(fa.stats().free_frames, before.free_frames - 1);
        assert_eq!(fa.stats().deallocations, 1);
    }

    #[test]
    fn reserved_hole_is_never_allocated() {
        let mut fa = allocator();
        let hole_first = Frame::containing(8 * 1024 * 1024);
        let hole_last = Frame::containing(8 * 1024 * 1024 + 512 * 1024 - 1);
        // Drain everything; no allocation may land in the hole.
        while let Some(frame) = fa.allocate() {
            assert!(
                frame < hole_first || frame > hole_last,
                "allocated a reserved frame {frame:?}"
            );
        }
        assert_eq!(fa.stats().free_frames, 0);
        assert!(fa.allocate().is_none(), "exhaustion returns None, not UB");
    }

    #[test]
    fn contiguous_allocation_finds_a_run() {
        let mut fa = allocator();
        let run = fa.allocate_contiguous(64).unwrap();
        // 64 frames = 256 KiB, all consecutive.
        for i in 0..64 {
            assert!(fa.is_used(run.0 + i));
        }
        // Freeing the run makes the space reusable.
        for i in 0..64 {
            fa.deallocate(Frame(run.0 + i));
        }
        assert!(fa.allocate_contiguous(64).is_some());
    }
}
