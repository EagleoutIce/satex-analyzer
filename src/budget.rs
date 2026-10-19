//! Counting what a run allocates, so `limits.memory` can stop it before the
//! operating system has to.
//!
//! Each run counts on its own ([`Run`]): runs side by side in one process
//! (parallel tests, the language server) never see each other's memory.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, AtomicUsize, Ordering};

/// What the whole process holds, for allocations made outside any run.
static PROCESS: Counter = Counter::new();
/// Everything live in the process, every run's share included: runs side
/// by side (parallel tests) are held to `limits.memory` together.
static TOTAL: AtomicIsize = AtomicIsize::new(0);
/// `limits.memory`, 0 for none.
static LIMIT: AtomicUsize = AtomicUsize::new(0);

/// Live and peak bytes. Live may dip below zero where a run frees what was
/// allocated before it began; reads clamp at zero.
pub struct Counter {
    live: AtomicIsize,
    peak: AtomicUsize,
}

impl Counter {
    const fn new() -> Counter {
        Counter { live: AtomicIsize::new(0), peak: AtomicUsize::new(0) }
    }

    fn live(&self) -> usize {
        self.live.load(Ordering::Relaxed).max(0) as usize
    }
}

thread_local! {
    /// The run this thread allocates for; null outside one.
    static CURRENT: Cell<*const Counter> = const { Cell::new(std::ptr::null()) };
}

fn current() -> &'static Counter {
    // SAFETY: a non-null pointer is a `Run`'s counter, kept alive by the
    // `Entered` guard that set it on this thread.
    let ptr = CURRENT.try_with(Cell::get).unwrap_or(std::ptr::null());
    if ptr.is_null() { &PROCESS } else { unsafe { &*ptr } }
}

/// One run's counter. Threads the run starts enter it too ([`Run::enter`]).
#[derive(Clone)]
pub struct Run(Arc<Counter>);

impl Run {
    /// A fresh counter, entered on this thread until the guard drops.
    pub fn start() -> (Run, Entered) {
        let run = Run(Arc::new(Counter::new()));
        let entered = run.enter();
        (run, entered)
    }

    /// The run this thread counts for, to hand to a thread it starts.
    pub fn current() -> Option<Run> {
        let ptr = CURRENT.with(Cell::get);
        // SAFETY: as in `current`; the Arc is alive while entered.
        (!ptr.is_null()).then(|| unsafe {
            Arc::increment_strong_count(ptr);
            Run(Arc::from_raw(ptr))
        })
    }

    /// Counts this thread's allocations for the run until the guard drops.
    pub fn enter(&self) -> Entered {
        let outer = CURRENT.with(|c| c.replace(Arc::as_ptr(&self.0)));
        Entered { _run: self.clone(), outer }
    }
}

/// Restores the thread's previous run when dropped.
pub struct Entered {
    _run: Run,
    outer: *const Counter,
}

impl Drop for Entered {
    fn drop(&mut self) {
        CURRENT.with(|c| c.set(self.outer));
    }
}

/// Sets `limits.memory`, which every run is held to.
pub fn set_limit(bytes: u64) {
    LIMIT.store(usize::try_from(bytes).unwrap_or(usize::MAX), Ordering::Relaxed);
}

/// One and a half times `limits.memory`, 0 for none. The interpreter stops
/// itself at the limit between tokens; this catches whatever grows past it
/// inside one step: the allocation fails and the process aborts with Rust's
/// "memory allocation failed" message instead of the machine swapping.
fn ceiling() -> usize {
    let limit = LIMIT.load(Ordering::Relaxed);
    limit.saturating_add(limit / 2)
}

/// The system allocator, counting what is live.  Only the binary installs it,
/// so a library user gets a count of zero and no memory limit.
pub struct Counted;

/// Counts `bytes` more as live; false, counting nothing, when that would pass
/// the ceiling.
fn take(bytes: usize) -> bool {
    let counter = current();
    let now = counter.live.fetch_add(bytes as isize, Ordering::Relaxed) + bytes as isize;
    let total = TOTAL.fetch_add(bytes as isize, Ordering::Relaxed) + bytes as isize;
    let ceiling = ceiling();
    if ceiling > 0
        && !std::ptr::eq(counter, &PROCESS)
        // A ceiling past `isize::MAX` (on wasm32) is no ceiling, not a negative one.
        && now.max(total) > isize::try_from(ceiling).unwrap_or(isize::MAX)
    {
        counter.live.fetch_sub(bytes as isize, Ordering::Relaxed);
        TOTAL.fetch_sub(bytes as isize, Ordering::Relaxed);
        return false;
    }
    if now as usize > counter.peak.load(Ordering::Relaxed) {
        counter.peak.fetch_max(now as usize, Ordering::Relaxed);
    }
    true
}

fn give(bytes: usize) {
    current().live.fetch_sub(bytes as isize, Ordering::Relaxed);
    TOTAL.fetch_sub(bytes as isize, Ordering::Relaxed);
}

unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if !take(layout.size()) {
            return std::ptr::null_mut();
        }
        let pointer = unsafe { System.alloc(layout) };
        if pointer.is_null() {
            give(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        give(layout.size());
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let old = layout.size();
        if new_size > old && !take(new_size - old) {
            return std::ptr::null_mut();
        }
        let moved = unsafe { System.realloc(pointer, layout, new_size) };
        match (moved.is_null(), new_size > old) {
            (true, true) => give(new_size - old),
            (false, false) => give(old - new_size),
            _ => {}
        }
        moved
    }
}

/// Bytes this run (or, outside one, the process) holds right now.
pub fn in_use() -> usize {
    current().live()
}

/// What `limits.memory` is checked against: this run's bytes, or the
/// process's where runs side by side hold more together.
pub fn held() -> usize {
    in_use().max(TOTAL.load(Ordering::Relaxed).max(0) as usize)
}

/// The most this run has held at once.
pub fn peak() -> usize {
    current().peak.load(Ordering::Relaxed)
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

    #[test]
    fn a_run_counts_apart_from_the_process() {
        let (run, _entered) = Run::start();
        assert_eq!(in_use(), 0);
        let handed = Run::current().expect("entered");
        assert!(Arc::ptr_eq(&run.0, &handed.0));
    }
}
