//! The `uma` allocator.  Regions are aligned to the 16 KiB guest page size
//! and carved out of a range that structurally excludes the kernel image, the
//! stack, and the display framebuffer.  Freed blocks go back onto a free list
//! with coalescing; there is no garbage collector, only explicit lifetimes.
//!
//! The same allocator backs both (a) explicit array-region allocation via the
//! kernel ops and (b) the small Rust heap used by the UIR stepper, through a
//! `GlobalAlloc` that stores a size header in each block.

use core::alloc::{GlobalAlloc, Layout};

pub const PAGE_SHIFT: usize = 14;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;
pub const CACHE_LINE: usize = 128;

pub struct Allocator {
    start: usize,
    end: usize,
    free: usize,
}

#[repr(C)]
struct FreeNode {
    size: usize,
    next: usize,
}

impl Allocator {
    pub fn new(start: usize, end: usize) -> Self {
        let start = (start + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        let end = end & !(PAGE_SIZE - 1);
        let mut a = Allocator { start, end, free: 0 };
        if end > start {
            a.push_free(start, end - start);
        }
        a
    }

    pub fn start(&self) -> usize {
        self.start
    }
    pub fn end(&self) -> usize {
        self.end
    }

    fn push_free(&mut self, addr: usize, size: usize) {
        unsafe {
            let node = addr as *mut FreeNode;
            (*node).size = size;
            (*node).next = self.free;
        }
        self.free = addr;
    }

    /// Remove a free-list node at `addr` (if present).
    fn remove_free(&mut self, addr: usize) {
        let mut prev: usize = 0;
        let mut cur = self.free;
        while cur != 0 {
            unsafe {
                let node = cur as *mut FreeNode;
                let next = (*node).next;
                if cur == addr {
                    if prev == 0 {
                        self.free = next;
                    } else {
                        (*(prev as *mut FreeNode)).next = next;
                    }
                    return;
                }
                prev = cur;
                cur = next;
            }
        }
    }

    /// Allocate at least `size` bytes, page-aligned.  Returns base or None.
    pub fn alloc(&mut self, size: usize) -> Option<usize> {
        let size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        if size == 0 {
            return None;
        }
        let mut prev: usize = 0;
        let mut cur = self.free;
        while cur != 0 {
            unsafe {
                let node = cur as *mut FreeNode;
                let nsize = (*node).size;
                let next = (*node).next;
                if nsize >= size {
                    if prev == 0 {
                        self.free = next;
                    } else {
                        (*(prev as *mut FreeNode)).next = next;
                    }
                    if nsize - size >= PAGE_SIZE {
                        self.push_free(cur + size, nsize - size);
                    }
                    return Some(cur);
                }
                prev = cur;
                cur = next;
            }
        }
        None
    }

    /// Free a previously allocated block.
    pub fn free(&mut self, addr: usize, size: usize) {
        let size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        if size == 0 {
            return;
        }
        let mut base = addr;
        let mut sz = size;

        // Coalesce with adjacent free blocks.
        let mut cur = self.free;
        let mut merged = false;
        while cur != 0 {
            unsafe {
                let node = cur as *mut FreeNode;
                let nsize = (*node).size;
                let next = (*node).next;
                if cur + nsize == base {
                    self.remove_free(cur);
                    base = cur;
                    sz += nsize;
                    cur = self.free; // restart scan (list changed)
                    merged = true;
                    continue;
                } else if base + sz == cur {
                    self.remove_free(cur);
                    sz += nsize;
                    cur = self.free;
                    merged = true;
                    continue;
                }
                cur = next;
            }
        }
        let _ = merged;
        self.push_free(base, sz);
    }
}

// ---------------------------------------------------------------------------
// Global state
// ---------------------------------------------------------------------------

static mut ALLOC: Allocator = Allocator { start: 0, end: 0, free: 0 };

/// Initialize the allocator over [start, end).  The range is chosen by the
/// caller to exclude the image, stack, and display.
pub fn init(start: usize, end: usize) {
    unsafe { ALLOC = Allocator::new(start, end) }
}

/// Allocate an `uma` region (page-aligned), for array payloads.
pub fn alloc_region(size: usize) -> Option<usize> {
    unsafe { ALLOC.alloc(size) }
}

/// Free a region previously returned by `alloc_region`.
pub fn free_region(addr: usize, size: usize) {
    unsafe { ALLOC.free(addr, size) }
}

/// Report how many bytes are still allocatable (for quota/diagnostics).
pub fn heap_start() -> usize {
    unsafe { ALLOC.start() }
}

// ---------------------------------------------------------------------------
// Rust heap (backed by the same allocator)
// ---------------------------------------------------------------------------

const HEAP_HEADER: usize = 16;

pub struct HeapAllocator;

unsafe impl GlobalAlloc for HeapAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size().max(1) + HEAP_HEADER;
        let rounded = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
        match ALLOC.alloc(rounded) {
            Some(base) => {
                let hdr = base as *mut usize;
                *hdr = rounded;
                (base + HEAP_HEADER) as *mut u8
            }
            None => core::ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, _layout: Layout) {
        if ptr.is_null() {
            return;
        }
        let base = (ptr as usize) - HEAP_HEADER;
        let rounded = *(base as *const usize);
        ALLOC.free(base, rounded);
    }
}

#[global_allocator]
static HEAP: HeapAllocator = HeapAllocator;
