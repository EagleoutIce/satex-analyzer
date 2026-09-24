//! Phase and per-file timings.

use std::time::{Duration, Instant};

use crate::tex::FileId;

/// One stage of a run: discovering the TeX installation, reading the format,
/// reading the document, running end-of-document hooks. [`Timings::phases`]
/// records how long each took.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Phase {
    Discovery,
    Format,
    Document,
    Hooks,
}

impl Phase {
    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Discovery => "discovery",
            Phase::Format => "format",
            Phase::Document => "document",
            Phase::Hooks => "hooks",
        }
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

    pub fn count_token(&mut self) {
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

    pub fn phases(&self) -> &[(Phase, Duration)] {
        &self.phases
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
