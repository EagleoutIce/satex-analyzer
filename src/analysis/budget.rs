//! Counting what a run allocates, so `limits.memory` can stop it before the
//! operating system has to.
//!
//! Each run counts on its own ([`Run`]): runs side by side in one process
//! (parallel tests, the language server) never see each other's memory.
//! A run alone in the process answers for all of it ([`charged`]).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};

/// What the whole process holds, for allocations made outside any run.
static PROCESS: Counter = Counter::new();
/// Everything live in the process, every run's share included: a run
/// alone in the process answers for it ([`charged`]).
static TOTAL: AtomicI64 = AtomicI64::new(0);
/// `limits.memory`, 0 for none.
static LIMIT: AtomicUsize = AtomicUsize::new(0);
/// Runs going on now, nested ones not counted ([`Run::start`]).
static RUNS: AtomicUsize = AtomicUsize::new(0);
/// Whether a run alone answers for the whole process ([`whole_process`]).
static WHOLE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Live and peak bytes. Live may dip below zero where a run frees what was
/// allocated before it began; reads clamp at zero.
pub struct Counter {
    // 64 bits on wasm32 too: its memory reaches 4 GiB, past `isize`
    live: AtomicI64,
    peak: AtomicUsize,
    /// Set once the run holds more than `limits.memory`: it stops at its
    /// next step instead of its next periodic check.
    over: std::sync::atomic::AtomicBool,
    /// What the run held when it first passed `limits.memory` ([`Counter::admit`]).
    crossed: AtomicUsize,
}

impl Counter {
    const fn new() -> Counter {
        Counter {
            live: AtomicI64::new(0),
            peak: AtomicUsize::new(0),
            over: std::sync::atomic::AtomicBool::new(false),
            crossed: AtomicUsize::new(0),
        }
    }

    /// Whether the run may hold `now` bytes after an allocation of `last`
    /// under `limit`, noting when it first holds more than the limit.
    ///
    /// The interpreter stops itself at the limit between tokens. A refused
    /// allocation ends the process with Rust's "memory allocation failed"
    /// (Rust cannot unwind from one), so the block that takes a run past
    /// its limit is granted, up to the limit's own size (a table that
    /// doubles, one copy): the run stops at its next token ([`over`]).
    /// Refused, instead of the machine swapping, is what a run goes on to
    /// take inside that one step past half the limit more, and a block no
    /// run under the limit could hold.
    fn admit(&self, now: usize, last: usize, limit: usize) -> bool {
        if limit == 0 || now <= limit {
            return true;
        }
        if !self.over.swap(true, Ordering::Relaxed) {
            self.crossed.store(now, Ordering::Relaxed);
            return last <= limit;
        }
        now <= self.crossed.load(Ordering::Relaxed).saturating_add(limit / 2)
    }

    fn live(&self) -> usize {
        self.live.load(Ordering::Relaxed).max(0) as usize
    }
}

thread_local! {
    /// The run this thread allocates for; none outside one.
    static CURRENT: Cell<Option<&'static Counter>> = const { Cell::new(None) };
}

fn current() -> &'static Counter {
    CURRENT.try_with(Cell::get).ok().flatten().unwrap_or(&PROCESS)
}

/// One run's counter. Threads the run starts enter it too ([`Run::enter`]).
/// It lives as long as the process: a few bytes a run, never a dangling one.
#[derive(Clone, Copy)]
pub struct Run(&'static Counter);

impl Run {
    /// A fresh counter, entered on this thread until the guard drops.
    pub fn start() -> (Run, Entered) {
        let run = Run(Box::leak(Box::new(Counter::new())));
        let mut entered = run.enter();
        // A run inside another (a package cache, a probe) is part of it.
        entered.outermost = entered.outer.is_none();
        if entered.outermost {
            RUNS.fetch_add(1, Ordering::Relaxed);
        }
        (run, entered)
    }

    /// The run this thread counts for, to hand to a thread it starts.
    pub fn current() -> Option<Run> {
        CURRENT.with(Cell::get).map(Run)
    }

    /// Counts this thread's allocations for the run until the guard drops.
    pub fn enter(&self) -> Entered {
        settle();
        Entered { outer: CURRENT.with(|c| c.replace(Some(self.0))), outermost: false }
    }
}

/// Restores the thread's previous run when dropped.
pub struct Entered {
    outer: Option<&'static Counter>,
    /// Whether this started a run of its own, counted in [`RUNS`].
    outermost: bool,
}

impl Drop for Entered {
    fn drop(&mut self) {
        settle();
        CURRENT.with(|c| c.set(self.outer));
        if self.outermost {
            RUNS.fetch_sub(1, Ordering::Relaxed);
        }
    }
}

/// Makes a run alone in the process answer for all of it. The binary
/// does; a test process, whose memory is many tests' leftovers, does not.
pub fn whole_process(on: bool) {
    WHOLE.store(on, Ordering::Relaxed);
}

/// Sets `limits.memory`, which every run is held to.
pub fn set_limit(bytes: u64) {
    LIMIT.store(usize::try_from(bytes).unwrap_or(usize::MAX), Ordering::Relaxed);
}

/// The system allocator, counting what is live.  Only the binary installs it,
/// so a library user gets a count of zero and no memory limit.
pub struct Counted;

thread_local! {
    /// Bytes this thread allocated (or freed, negative) for its counter that
    /// the counter does not hold yet: an atomic update per allocation was a
    /// hot cost, so they go in batches of [`BATCH`].
    static PENDING: Cell<isize> = const { Cell::new(0) };
}

/// How far a thread's count may run ahead of its counter.
const BATCH: isize = 256 << 10;

/// Counts `bytes` more as live (negative: freed); false, counting nothing,
/// when the run may not hold them ([`Counter::admit`]).
fn count(bytes: isize) -> bool {
    match PENDING.try_with(|p| {
        let now = p.get() + bytes;
        p.set(now);
        now
    }) {
        Ok(now) if now.abs() < BATCH => true,
        Ok(now) => {
            _ = PENDING.try_with(|p| p.set(0));
            flush(now, bytes)
        }
        Err(_) => flush(bytes, bytes),
    }
}

/// The pending bytes of this thread to its counter now.
fn settle() {
    if let Ok(now) = PENDING.try_with(|p| p.replace(0))
        && now != 0
    {
        flush(now, 0);
    }
}

/// Adds `bytes` to the counter; where the run may not hold them
/// ([`Counter::admit`]) it takes back `last`, the allocation asked for, and says no.
fn flush(bytes: isize, last: isize) -> bool {
    let counter = current();
    let (bytes, last) = (bytes as i64, last as i64);
    let now = counter.live.fetch_add(bytes, Ordering::Relaxed) + bytes;
    TOTAL.fetch_add(bytes, Ordering::Relaxed);
    // The run's own bytes only: a run past the limit stops between
    // tokens ([`charged`]), never by failing another's allocation.
    let admitted = counter.admit(now.max(0) as usize, last.max(0) as usize, LIMIT.load(Ordering::Relaxed));
    if !admitted && last > 0 && !std::ptr::eq(counter, &PROCESS) {
        counter.live.fetch_sub(last, Ordering::Relaxed);
        TOTAL.fetch_sub(last, Ordering::Relaxed);
        return false;
    }
    if now > 0 && now as usize > counter.peak.load(Ordering::Relaxed) {
        counter.peak.fetch_max(now as usize, Ordering::Relaxed);
    }
    true
}

/// What an allocation of `size` bytes takes from the system allocator.
/// Assumption, after glibc's malloc and dlmalloc (wasm): a header word
/// before the block, 16-byte granules, and a smallest chunk of four words.
/// A run of small allocations (tokens, names, tree nodes) takes about
/// twice what it asks for, which counting `size` alone missed.
fn footprint(size: usize) -> usize {
    const WORD: usize = std::mem::size_of::<usize>();
    (size + WORD).max(4 * WORD).next_multiple_of(16)
}

fn take(bytes: usize) -> bool {
    count(bytes as isize)
}

fn give(bytes: usize) {
    count(-(bytes as isize));
}

unsafe impl GlobalAlloc for Counted {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        #[cfg(feature = "alloc-sites")]
        super::alloc_sites::note(layout.size());
        let bytes = footprint(layout.size());
        if !take(bytes) {
            return std::ptr::null_mut();
        }
        let pointer = unsafe { System.alloc(layout) };
        if pointer.is_null() {
            give(bytes);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) };
        give(footprint(layout.size()));
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let (old, new) = (footprint(layout.size()), footprint(new_size));
        if new > old && !take(new - old) {
            return std::ptr::null_mut();
        }
        let moved = unsafe { System.realloc(pointer, layout, new_size) };
        match (moved.is_null(), new > old) {
            (true, true) => give(new - old),
            (false, false) => give(old - new),
            _ => {}
        }
        moved
    }
}

/// Bytes this run (or, outside one, the process) holds right now.
pub fn in_use() -> usize {
    settle();
    current().live()
}

/// What `limits.memory` is checked against. Runs side by side each answer
/// for their own count. A run alone in the binary answers for all of it:
/// what the process allocated outside the run too (the format, caches
/// shared between runs), and, where the system says, what is resident
/// (allocator slack, mapped files), whichever is more.
pub fn charged() -> usize {
    let own = in_use();
    if !WHOLE.load(Ordering::Relaxed) || RUNS.load(Ordering::Relaxed) > 1 {
        return own;
    }
    own.max(TOTAL.load(Ordering::Relaxed).max(0) as usize).max(resident())
}

/// Resident bytes of the process, read again once [`RESIDENT_EVERY`] passed
/// or the count moved by a 64th of the limit since (a file read, too slow
/// for every check); 0 where unknown.
fn resident() -> usize {
    static CLOCK: std::sync::LazyLock<std::time::Instant> = std::sync::LazyLock::new(std::time::Instant::now);
    static READ_AT: AtomicUsize = AtomicUsize::new(0);
    static READ_WITH: AtomicUsize = AtomicUsize::new(0);
    static LAST: AtomicUsize = AtomicUsize::new(0);
    let now = CLOCK.elapsed().as_millis() as usize + RESIDENT_EVERY;
    let total = TOTAL.load(Ordering::Relaxed).max(0) as usize;
    let moved = total.abs_diff(READ_WITH.load(Ordering::Relaxed)) > LIMIT.load(Ordering::Relaxed) / 64;
    let at = READ_AT.load(Ordering::Relaxed);
    if at != 0 && now < at + RESIDENT_EVERY && !moved {
        return LAST.load(Ordering::Relaxed);
    }
    READ_AT.store(now, Ordering::Relaxed);
    READ_WITH.store(total, Ordering::Relaxed);
    let bytes = read_resident().unwrap_or(0);
    LAST.store(bytes, Ordering::Relaxed);
    bytes
}

/// Milliseconds between two reads of the resident size.
const RESIDENT_EVERY: usize = 50;

/// The resident set size the system reports: Linux's `/proc/self/statm`
/// (pages); none elsewhere, where the count alone is checked.
fn read_resident() -> Option<usize> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: usize = statm.split_whitespace().nth(1)?.parse().ok()?;
    // 4 KiB pages: the size every Linux target satex builds for uses.
    Some(pages << 12)
}

/// Whether this run has held more than `limits.memory`.
pub fn over() -> bool {
    current().over.load(Ordering::Relaxed)
}

/// The most this run has held at once.
pub fn peak() -> usize {
    settle();
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

    /// The block that takes a run past its limit is granted, so the run can
    /// stop at its next token; what it takes after that is bounded. Pins
    /// the aborts "memory allocation of 536870996 bytes failed": one table
    /// that doubled from under the limit to past one and a half times it.
    #[test]
    fn the_block_that_passes_the_limit_is_granted() {
        const LIMIT: usize = 1000;
        let run = Counter::new();
        assert!(run.admit(LIMIT, 10, LIMIT) && !run.over.load(Ordering::Relaxed));
        // One block from under the limit to twice it: granted.
        assert!(run.admit(2 * LIMIT, LIMIT, LIMIT));
        assert!(run.over.load(Ordering::Relaxed), "the run stops at its next token");
        // Until then it may take half the limit more, and no more.
        assert!(run.admit(2 * LIMIT + LIMIT / 2, 1, LIMIT));
        assert!(!run.admit(2 * LIMIT + LIMIT / 2 + 1, 1, LIMIT));
        // A run that passed the limit by one byte is held to half the limit more as well.
        let run = Counter::new();
        assert!(run.admit(LIMIT + 1, 1, LIMIT) && run.admit(LIMIT + 1 + LIMIT / 2, LIMIT / 2, LIMIT));
        assert!(!run.admit(LIMIT + 2 + LIMIT / 2, 1, LIMIT));
        // A block no run under the limit could hold is refused at once.
        assert!(!Counter::new().admit(LIMIT + 2, LIMIT + 1, LIMIT));
        // No limit, no refusal.
        assert!(Counter::new().admit(usize::MAX, usize::MAX, 0));
    }

    #[test]
    fn a_run_counts_apart_from_the_process() {
        let (run, _entered) = Run::start();
        assert_eq!(in_use(), 0);
        let handed = Run::current().expect("entered");
        assert!(std::ptr::eq(run.0, handed.0));
    }
}
