//! Where allocations come from (`--features alloc-sites`, a developer build):
//! with `SATEX_ALLOC_SITES=<file>` every `SATEX_ALLOC_EVERY`th allocation
//! (default 100003) appends its call stack to the file, frames from the
//! allocator inward cut off. Samplers that walk frame pointers lose the
//! allocating function, as glibc's `malloc` keeps none.

use std::cell::Cell;
use std::io::Write;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

thread_local! {
    /// Set while a sample is being written: its own allocations are not sampled.
    static INSIDE: Cell<bool> = const { Cell::new(false) };
}

static SEEN: AtomicU64 = AtomicU64::new(0);

fn every() -> u64 {
    static EVERY: OnceLock<u64> = OnceLock::new();
    *EVERY.get_or_init(|| std::env::var("SATEX_ALLOC_EVERY").ok().and_then(|v| v.parse().ok()).unwrap_or(100_003))
}

/// Called by the global allocator for each allocation of `size` bytes.
pub fn note(size: usize) {
    // A thread being torn down has no flag left: its allocations go unsampled.
    if INSIDE.try_with(|i| i.replace(true)).unwrap_or(true) {
        return;
    }
    if SEEN.fetch_add(1, Ordering::Relaxed).is_multiple_of(every())
        && let Some(path) = std::env::var_os("SATEX_ALLOC_SITES")
    {
        let trace = std::backtrace::Backtrace::force_capture().to_string();
        // The frames above the allocator's entry, function and place, one line.
        // The crate's own frames, innermost first: function, then file and line.
        let frames: Vec<&str> = trace.lines().map(str::trim).collect();
        let mut line = format!("{size}");
        let own = |at: &str| at.starts_with("at ") && !at.contains("/library/") && !at.contains("/.cargo/");
        let skip = |at: &str| at.contains("alloc_sites.rs") || at.contains("budget.rs");
        for w in frames.windows(2).filter(|w| own(w[1]) && !skip(w[1])).take(12) {
            line.push_str(" | ");
            line.push_str(w[0]);
            line.push_str(" | ");
            line.push_str(w[1]);
        }
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = writeln!(f, "{line}");
        }
    }
    let _ = INSIDE.try_with(|i| i.set(false));
}
