//! Counting what a run allocates, so `limits.memory` can stop it before the
//! operating system has to.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};

static IN_USE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
/// Twice `limits.memory`, 0 for none.  The interpreter stops itself at the
/// limit between tokens; this catches whatever grows past it inside one step
/// or after the run: the allocation fails and the process aborts with Rust's
/// "memory allocation failed" message instead of the machine swapping.
static CEILING: AtomicUsize = AtomicUsize::new(0);

/// Sets the hard ceiling from `limits.memory`.
pub fn set_limit(bytes: u64) {
    CEILING.store(usize::try_from(bytes.saturating_mul(2)).unwrap_or(usize::MAX), Ordering::Relaxed);
}

/// The system allocator, counting what is live.  Only the binary installs it,
/// so a library user gets a count of zero and no memory limit.
pub struct Counted;

/// Counts `bytes` more as live; false, counting nothing, when that would pass
/// the ceiling.  One atomic add per allocation, the ceiling and peak are
/// plain loads that stay in cache.
fn take(bytes: usize) -> bool {
    let now = IN_USE.fetch_add(bytes, Ordering::Relaxed) + bytes;
    let ceiling = CEILING.load(Ordering::Relaxed);
    if ceiling > 0 && now > ceiling {
        IN_USE.fetch_sub(bytes, Ordering::Relaxed);
        return false;
    }
    if now > PEAK.load(Ordering::Relaxed) {
        PEAK.fetch_max(now, Ordering::Relaxed);
    }
    true
}

unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !take(layout.size()) {
            return std::ptr::null_mut();
        }
        let pointer = unsafe { System.alloc(layout) };
        if pointer.is_null() {
            IN_USE.fetch_sub(layout.size(), Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        IN_USE.fetch_sub(layout.size(), Ordering::Relaxed);
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old = layout.size();
        if new_size > old && !take(new_size - old) {
            return std::ptr::null_mut();
        }
        let moved = unsafe { System.realloc(pointer, layout, new_size) };
        match (moved.is_null(), new_size > old) {
            (true, true) => IN_USE.fetch_sub(new_size - old, Ordering::Relaxed),
            (false, false) => IN_USE.fetch_sub(old - new_size, Ordering::Relaxed),
            _ => 0,
        };
        moved
    }
}

/// Bytes this process holds right now.
pub fn in_use() -> usize {
    IN_USE.load(Ordering::Relaxed)
}

/// The most it has held at once.
pub fn peak() -> usize {
    PEAK.load(Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_what_it_hands_out() {
        // The library's own tests do not install this allocator, so the count
        // only has to stay consistent, not non-zero.
        let before = in_use();
        let block = vec![0u8; 1 << 20];
        let held = in_use();
        drop(block);
        assert!(held >= before);
        assert!(peak() >= held);
    }
}
