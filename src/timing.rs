//! Phase and per-file timings.

use std::time::{Duration, Instant};

use crate::tex::{FileId, Sym};

named_enum! {
    /// One stage of a run: discovering the TeX installation, reading the format,
    /// reading the document, running end-of-document hooks. [`Timings::phases`]
    /// records how long each took.
    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Phase {
        Discovery => "discovery",
        Format => "format",
        Document => "document",
        Hooks => "hooks",
        /// Running the lint rules over the finished analysis.
        Lint => "lint",
        /// Applying quick-fixes and re-analyzing.
        Fix => "fix",
    }
}

/// Where the run's time went: per-[`Phase`] totals and per-file token and
/// time counts, collected only when the run asked for timings.
#[derive(Default)]
pub struct Timings {
    enabled: bool,
    phases: Vec<(Phase, Duration)>,
    open: Option<(Phase, Instant)>,
    files: Vec<(Duration, u64)>,
    stack: Vec<(FileId, Instant, Duration)>,
    /// How often each macro was expanded: where a slow run loops.
    calls: crate::fixed::HashMap<Sym, u64>,
    /// Tokens read since a project file's line was last read, per line:
    /// what each line of the document costs, packages' work included.
    site: Option<(FileId, u32)>,
    sites: crate::fixed::HashMap<(FileId, u32), u64>,
    /// What running each written line where it is read back cost, per
    /// writing line: its tokens, how many lines, and the first line's text.
    written: crate::fixed::HashMap<(FileId, u32), (u64, u32, String)>,
    /// Phases timed after the analysis is finished, so behind a lock: the
    /// command that reads the analysis only has it by shared reference.
    after: std::sync::Mutex<Vec<(Phase, Duration)>>,
}

impl Timings {
    pub fn new(enabled: bool) -> Self {
        Self { enabled, ..Default::default() }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn start(&mut self, phase: Phase) {
        if !self.enabled {
            return;
        }
        self.finish();
        self.open = Some((phase, Instant::now()));
    }

    pub fn finish(&mut self) {
        if let Some((phase, since)) = self.open.take() {
            self.phases.push((phase, since.elapsed()));
        }
    }

    pub fn enter_file(&mut self, file: FileId) {
        if self.enabled {
            self.stack.push((file, Instant::now(), Duration::ZERO));
        }
    }

    pub fn leave_file(&mut self) {
        if !self.enabled {
            return;
        }
        let Some((file, since, children)) = self.stack.pop() else { return };
        let total = since.elapsed();
        if let Some(parent) = self.stack.last_mut() {
            parent.2 += total;
        }
        let slot = file as usize;
        if self.files.len() <= slot {
            self.files.resize(slot + 1, (Duration::ZERO, 0));
        }
        self.files[slot].0 += total.saturating_sub(children);
    }

    pub fn count_call(&mut self, sym: Sym) {
        *self.calls.entry(sym).or_default() += 1;
    }

    /// The `n` macros expanded most often, most first.
    pub fn top_calls(&self, n: usize) -> Vec<(Sym, u64)> {
        let mut out: Vec<(Sym, u64)> = self.calls.iter().map(|(&s, &c)| (s, c)).collect();
        out.sort_unstable_by_key(|&(s, c)| (std::cmp::Reverse(c), s));
        out.truncate(n);
        out
    }

    /// The project file's line read last: the tokens read next are its cost.
    pub fn at_site(&mut self, file: FileId, line: u32) {
        self.site = Some((file, line));
    }

    /// The `n` costliest project lines, most first.
    pub fn top_sites(&self, n: usize) -> Vec<((FileId, u32), u64)> {
        let mut out: Vec<((FileId, u32), u64)> = self.sites.iter().map(|(&s, &c)| (s, c)).collect();
        out.sort_unstable_by_key(|&(s, c)| (std::cmp::Reverse(c), s));
        out.truncate(n);
        out
    }

    /// A line `site` wrote, run where it is read back, took `steps`.
    pub fn written_line(&mut self, site: (FileId, u32), steps: u64, text: &str) {
        let entry = self.written.entry(site).or_insert_with(|| (0, 0, text.chars().take(100).collect()));
        entry.0 += steps;
        entry.1 += 1;
    }

    /// The `n` writing lines whose written lines cost most, most first.
    pub fn top_written(&self, n: usize) -> Vec<((FileId, u32), (u64, u32, String))> {
        let mut out: Vec<_> = self.written.iter().map(|(&s, w)| (s, w.clone())).collect();
        out.sort_unstable_by_key(|(s, w)| (std::cmp::Reverse(w.0), *s));
        out.truncate(n);
        out
    }

    pub fn count_token(&mut self) {
        if let Some(site) = self.site {
            *self.sites.entry(site).or_default() += 1;
        }
        if let Some((file, _, _)) = self.stack.last() {
            let slot = *file as usize;
            if self.files.len() <= slot {
                self.files.resize(slot + 1, (Duration::ZERO, 0));
            }
            self.files[slot].1 += 1;
        }
    }

    /// Credits a file with tokens a replayed package-cache segment stands
    /// for: the segment did not read them token by token this run, so
    /// [`Self::count_token`] never saw them.
    pub fn credit_tokens(&mut self, file: FileId, tokens: u64) {
        if !self.enabled || tokens == 0 {
            return;
        }
        let slot = file as usize;
        if self.files.len() <= slot {
            self.files.resize(slot + 1, (Duration::ZERO, 0));
        }
        self.files[slot].1 += tokens;
    }

    /// Time a phase that runs after the analysis: the linter, the fixes.
    pub fn record(&self, phase: Phase, elapsed: Duration) {
        if self.enabled
            && let Ok(mut after) = self.after.lock()
        {
            after.push((phase, elapsed));
        }
    }

    /// Run `work`, timing it as `phase` when the run asked for timings.
    pub fn around<T>(&self, phase: Phase, work: impl FnOnce() -> T) -> T {
        if !self.enabled {
            return work();
        }
        let since = Instant::now();
        let out = work();
        self.record(phase, since.elapsed());
        out
    }

    pub fn phases(&self) -> Vec<(Phase, Duration)> {
        let mut out = self.phases.clone();
        if let Ok(after) = self.after.lock() {
            out.extend(after.iter().copied());
        }
        out
    }

    pub fn by_file(&self) -> Vec<(FileId, Duration, u64)> {
        let mut out: Vec<(FileId, Duration, u64)> = self
            .files
            .iter()
            .enumerate()
            .filter(|(_, (time, tokens))| !time.is_zero() || *tokens > 0)
            .map(|(id, (time, tokens))| (id as FileId, *time, *tokens))
            .collect();
        out.sort_by_key(|(_, time, _)| std::cmp::Reverse(*time));
        out
    }
}
