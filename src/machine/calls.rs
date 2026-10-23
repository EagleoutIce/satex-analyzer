//! Call summaries: a call met again at one site, with unknown operands,
//! applies the join of what its first runs over unknown operands did
//! instead of running
//! (`limits.call_summary_after` of them; 0 never summarizes).
//! Forced convergence for parsers that walk unknown text a token at a time.

use std::rc::Rc;

use super::Machine;
use crate::env::{Binding, NodeId};
use crate::facts::Severity;
use crate::tex::{Meaning, Span, Sym, Token};

/// A macro meaning, the site it is called at, and the control sequences
/// its arguments name: a call writes the names it is handed.
pub(crate) type CallKey = (NodeId, Span, u64);

/// One call being recorded: open until a main-loop read goes below `base`.
#[derive(Clone)]
pub(crate) struct CallRec {
    key: CallKey,
    base: usize,
    recent: usize,
    reads: usize,
    groups: usize,
    conds: usize,
    mode: crate::mode::Modes,
    /// The list being built: a run that added to it typesets, which a
    /// summary does not carry.
    natural: Option<crate::mode::Natural>,
    material: bool,
    leaky: bool,
    /// Its arguments held unknown text.
    unknown: bool,
    /// Where the list below the call's text stood: a run ends cleanly
    /// only on that list as the call left it.
    below: Option<FrameMark>,
}

/// One recorded run: the names it wrote, as it left them, and the names
/// it read.
#[derive(Clone)]
struct Run {
    writes: Vec<(Sym, Option<Binding>)>,
    reads: Vec<Sym>,
}

/// Where a list stands: its text (held, so its place is never another
/// list's) and position, the tokens given back, or a file's place.
#[derive(Clone)]
enum FrameMark {
    Tokens(Rc<[Token]>, usize),
    Returned(Vec<Token>),
    At(Span),
    Other,
}

impl PartialEq for FrameMark {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Tokens(a, p), Self::Tokens(b, q)) => Rc::ptr_eq(a, b) && p == q,
            (Self::Returned(a), Self::Returned(b)) => {
                a.iter().map(|t| (t.tok, t.span)).eq(b.iter().map(|t| (t.tok, t.span)))
            }
            (Self::At(a), Self::At(b)) => a == b,
            (Self::Other, Self::Other) => true,
            _ => false,
        }
    }
}

fn frame_mark(frame: &super::Frame) -> FrameMark {
    use super::Frame;
    match frame {
        Frame::Tokens { toks, pos, .. } => FrameMark::Tokens(toks.clone(), *pos),
        Frame::Returned(tokens) => FrameMark::Returned(tokens.clone()),
        Frame::File { mouth, .. } => FrameMark::At(mouth.here()),
        Frame::Pseudo { pos, at, .. } => FrameMark::At(Span::new(at.file, at.line, *pos as u32)),
        Frame::Boundary => FrameMark::Other,
    }
}

/// What the recorded runs of one call did.
#[derive(Clone, Default)]
pub(crate) struct CallRuns {
    runs: Vec<Run>,
    /// A run read past its own text, changed catcodes, or left groups,
    /// conditionals or the mode changed: never summarized.
    leaky: bool,
    reported: bool,
    /// Runs of it being recorded now.
    active: usize,
}

impl Machine<'_> {
    /// Forgets the visit counts of calls no run is recording now.
    pub(super) fn forget_settled_calls(&mut self) {
        self.call_runs.retain(|_, runs| runs.active > 0);
    }

    /// The key of a call of meaning `def` at `span` with `arguments`.
    pub(crate) fn call_key(&self, def: NodeId, span: Span, arguments: &[Vec<Token>]) -> CallKey {
        use std::hash::{BuildHasher, Hash, Hasher};
        let mut h = crate::tex::NameHash.build_hasher();
        for a in arguments {
            for t in a {
                if let crate::tex::Tok::Cs(sym) = t.tok
                    && !self.is_unknown_marker(sym)
                {
                    sym.hash(&mut h);
                }
            }
            0u32.hash(&mut h);
        }
        (def, span, h.finish())
    }

    /// As a call's body is pushed: record the run while runs are missing.
    pub(crate) fn record_call(&mut self, key: CallKey, unknown: bool) {
        let needed = usize::from(self.limits.call_summary_after);
        let runs = self.call_runs.entry(key).or_default();
        if !runs.leaky && runs.runs.len() + runs.active < needed {
            runs.active += 1;
            if self.call_recs.is_empty() {
                self.env.log_reads(false);
                self.env.log_reads(true);
            } else if self.env.read_log_len() > crate::env::READ_LOG {
                self.leak_call_recs();
                self.env.log_reads(false);
            }
            let rec = CallRec {
                key,
                base: self.input.len(),
                recent: self.env.recent_len(),
                reads: self.env.read_log_len(),
                groups: self.env.group_level(),
                conds: self.conds.len(),
                mode: self.mode,
                natural: self.natural,
                material: self.material,
                leaky: false,
                unknown,
                below: self.input.last().map(frame_mark),
            };
            crate::push_shared(&mut self.call_recs, rec);
        }
    }

    /// A call met again with unknown operands: its recorded runs, joined,
    /// stand for it. `true` when they do.
    pub(crate) fn summarize_call(&mut self, key: CallKey, arguments: &[Vec<Token>]) -> bool {
        let needed = usize::from(self.limits.call_summary_after);
        let Some(runs) = self.call_runs.get(&key) else { return false };
        if needed == 0 || runs.leaky || runs.runs.len() < needed {
            return false;
        }
        // Runs that wrote different names chose them by data: a third run
        // may write yet another. That, and the shape checks, hold for good:
        // a call refused once is never looked at again.
        let names = |r: &Run| r.writes.iter().map(|w| w.0).collect::<Vec<_>>();
        let same = runs.runs.windows(2).all(|w| names(&w[0]) == names(&w[1]));
        if !same
            || runs
                .runs
                .windows(2)
                .any(|w| accumulates(&w[0], &w[1]) || updates(&w[0], &w[1]) || self.folds_code(&w[0], &w[1]))
        {
            let runs = self.call_runs.get_mut(&key).expect("recorded");
            runs.leaky = true;
            runs.runs.clear();
            return false;
        }
        // An operand is unknown: an argument, or a name the runs read.
        let unknown = arguments.iter().any(|a| self.has_unknown(a))
            || runs
                .runs
                .iter()
                .flat_map(|r| &r.reads)
                .any(|&sym| self.env.slot(sym).is_some_and(|b| self.imprecise(b)));
        if !unknown {
            return false;
        }
        self.apply_summary(key, key.1);
        true
    }

    fn apply_summary(&mut self, key: CallKey, span: Span) {
        let runs = std::mem::take(&mut self.call_runs.get_mut(&key).expect("recorded").runs);
        let mut syms: Vec<Sym> = runs.iter().flat_map(|r| r.writes.iter().map(|w| w.0)).collect();
        syms.sort_unstable();
        syms.dedup();
        let typed_syms = self.typed_syms();
        let typed = crate::env::typed!(self, typed_syms);
        let mut writes = Vec::with_capacity(syms.len());
        for &sym in &syms {
            let now = self.env.slot_at(sym.0);
            let of = |r: &Run| r.writes.iter().find(|w| w.0 == sym).map_or_else(|| now.clone(), |w| w.1.clone());
            let mut acc = of(&runs[0]);
            for r in &runs[1..] {
                let next = of(r);
                let joined = crate::env::join_slot(acc.clone(), next, self.env.sets);
                acc = crate::env::widen_slot(&acc, joined, self.env.sets, &typed);
            }
            if let Some(mut b) = acc {
                b.certain = false;
                writes.push((sym, b));
            }
        }
        for (sym, b) in writes {
            let global = b.global;
            self.env.set(sym, b, global);
        }
        let reported = std::mem::replace(&mut self.call_runs.get_mut(&key).expect("recorded").reported, true);
        self.call_runs.get_mut(&key).expect("recorded").runs = runs;
        self.approximated = true;
        if self.cfg.verbose >= 4 {
            let names: Vec<&str> = syms.iter().map(|s| self.name(*s)).collect();
            let file = self.out.short_name(span.file);
            crate::progress::note!("satex: call at {file}:{}:{} summarized: {names:?}", span.line, span.col);
        }
        if !reported {
            self.diagnose(
                Severity::Imprecision,
                "call-summarized",
                span,
                "a call with unknown operands met again here: what it does is the join of its first runs".to_string(),
            );
        }
    }

    /// A read is about to take a token from frame `self.input.len() - 1`:
    /// every call recorded above it is over.
    pub(crate) fn end_call_recs(&mut self) {
        let needed = usize::from(self.limits.call_summary_after);
        while let Some(rec) = self.call_recs.last()
            && self.input.len() <= rec.base
        {
            let rec = Rc::make_mut(&mut self.call_recs).pop().expect("one");
            let main = self.reading_command && self.step_depth == 1;
            // Nothing below the call's text was read or given back to: a
            // look-ahead past its end leaves its run unfinished.
            let intact = self.input.len() == rec.base && self.input.last().map(frame_mark) == rec.below;
            let clean = main
                && intact
                && !rec.leaky
                && self.env.recent_len() >= rec.recent
                && self.env.group_level() == rec.groups
                && self.conds.len() == rec.conds
                && self.mode == rec.mode
                && self.natural == rec.natural
                && self.material == rec.material;
            let entry = self.call_runs.entry(rec.key).or_default();
            entry.active = entry.active.saturating_sub(1);
            if !clean {
                entry.leaky = true;
                entry.runs.clear();
                continue;
            }
            if entry.runs.len() >= needed {
                continue;
            }
            let Some(reads) = self.env.reads_since(rec.reads) else {
                self.call_runs.get_mut(&rec.key).expect("entry").leaky = true;
                continue;
            };
            // Only a run over unknown operands stands for one: what a run
            // on known values wrote holds for those values alone.
            if !rec.unknown && !reads.iter().any(|&sym| self.env.slot(sym).is_some_and(|b| self.imprecise(b))) {
                continue;
            }
            let writes: Vec<_> =
                self.env.recent_since(rec.recent).into_iter().map(|sym| (sym, self.env.slot_at(sym.0))).collect();
            // A run that wrote nothing did what it did through the tokens
            // it left or took, which a summary does not carry.
            if writes.is_empty() {
                let entry = self.call_runs.get_mut(&rec.key).expect("entry");
                entry.leaky = true;
                entry.runs.clear();
                continue;
            }
            self.call_runs.get_mut(&rec.key).expect("entry").runs.push(Run { writes, reads });
        }
        if self.call_recs.is_empty() {
            self.env.log_reads(false);
        }
    }

    /// A binding that holds unknown text, which a parser walks a token at
    /// a time. A number not known exactly is computed with, not walked.
    fn imprecise(&self, b: &Binding) -> bool {
        b.may.is_some() || matches!(&b.meaning, Meaning::Macro(d) if self.has_unknown(&d.replacement_text))
    }
    /// The running calls read text that is not theirs, or changed state a
    /// summary does not carry.
    pub(crate) fn leak_call_recs(&mut self) {
        if self.call_recs.iter().any(|rec| !rec.leaky) {
            for rec in Rc::make_mut(&mut self.call_recs) {
                rec.leaky = true;
            }
        }
    }
}

impl Machine<'_> {
    /// Runs that left a name different in more than data: code, braces,
    /// a stack, a meaning. Their join would be text that may run as
    /// anything, or name anything, so they are not joined. Data is a number,
    /// or text of letters, others and spaces under one parameter text.
    fn folds_code(&self, a: &Run, b: &Run) -> bool {
        let data = |t: &[Token]| {
            t.iter().all(|t| match t.tok {
                crate::tex::Tok::Chr(_, c) => {
                    matches!(c, crate::tex::Catcode::Letter | crate::tex::Catcode::Other | crate::tex::Catcode::Space)
                }
                crate::tex::Tok::Cs(s) => self.is_unknown_marker(s),
                crate::tex::Tok::Param(_) => false,
            })
        };
        let numeric =
            |v: &crate::value::Value| !matches!(v, crate::value::Value::Toks(_) | crate::value::Value::Unknown);
        let of = |r: &Run, sym: Sym| {
            r.writes.iter().find(|w| w.0 == sym).map_or_else(|| self.env.slot_at(sym.0), |w| w.1.clone())
        };
        a.writes.iter().chain(&b.writes).any(|&(sym, _)| {
            let (x, y) = (of(a, sym), of(b, sym));
            if crate::env::same_slot(&x, &y) {
                return false;
            }
            let (Some(p), Some(q)) = (&x, &y) else { return true };
            if p.may.is_some() || q.may.is_some() {
                return true;
            }
            match (&p.meaning, &q.meaning) {
                (Meaning::Macro(d), Meaning::Macro(e)) => {
                    d.parameter_text != e.parameter_text || !data(&d.replacement_text) || !data(&e.replacement_text)
                }
                _ => match (&p.value, &q.value) {
                    (crate::value::Value::Toks(s), crate::value::Value::Toks(t)) => {
                        p.meaning != q.meaning || !data(s) || !data(t)
                    }
                    (v, w) => p.meaning != q.meaning || !numeric(v) || !numeric(w),
                },
            }
        })
    }
}

/// The text a macro or token list holds.
fn text(b: &Option<Binding>) -> Option<std::rc::Rc<[Token]>> {
    match b.as_ref().map(|b| (&b.meaning, &b.value)) {
        Some((Meaning::Macro(d), _)) => Some(d.replacement_text.clone()),
        Some((_, crate::value::Value::Toks(t))) => Some(t.clone()),
        _ => None,
    }
}

/// A name the second run left holding the first run's text and more: an
/// accumulator, whose join would lose every entry.
fn accumulates(a: &Run, b: &Run) -> bool {
    a.writes.iter().zip(&b.writes).any(|(x, y)| match (text(&x.1), text(&y.1)) {
        (Some(x), Some(y)) if x.len() < y.len() => {
            let same = |p: &[Token], q: &[Token]| p.iter().zip(q).all(|(s, t)| s.tok == t.tok);
            same(&x, &y) || same(&x, &y[y.len() - x.len()..])
        }
        _ => false,
    })
}

/// A name the runs read and left different: what each wrote came from
/// what it read, a stack push say, which the next call holds anew. Their
/// join stands for no later run.
fn updates(a: &Run, b: &Run) -> bool {
    a.writes.iter().zip(&b.writes).any(|(x, y)| a.reads.contains(&x.0) && !crate::env::same_slot(&x.1, &y.1))
}
