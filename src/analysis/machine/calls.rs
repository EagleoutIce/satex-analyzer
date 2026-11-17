//! Call summaries. When a call site with unknown operands is met again, it
//! applies the join of its first runs' effects instead of running again
//! (after `limits.call_summary_after` runs; 0 turns this off). This forces
//! parsers that walk unknown text one token at a time to converge.
//!
//! A definition can also learn its effect from `limits.call_learn_after`
//! runs on different arguments. A later run that keeps forking
//! (`limits.call_learn_forks`) stops and applies that effect instead, with
//! each value widened to its type (`analysis.assume_call_learned_effect`).

use std::rc::Rc;

use super::Machine;
use crate::analysis::facts::Severity;
use crate::engine::env::{Binding, NodeId};
use crate::engine::tex::{Meaning, Span, Sym, Token};

/// Identifies a call: the macro's definition, the call site, and a hash of
/// the control sequences in its arguments (a call may write the names it is given).
pub(crate) type CallKey = (NodeId, Span, u64);

/// A call being recorded. It ends when the main loop reads below input depth `base`.
#[derive(Debug, Clone)]
pub(crate) struct CallRec {
    key: CallKey,
    base: usize,
    recent: usize,
    reads: usize,
    groups: usize,
    conds: usize,
    mode: crate::engine::mode::Modes,
    /// The list being built. A run that adds to it typesets something, which a summary cannot replay.
    natural: Option<crate::engine::mode::Natural>,
    material: u8,
    /// [`Machine::file_writes`] at the start: a run that wrote a file did
    /// what a summary of its name writes cannot replay.
    side: u64,
    leaky: bool,
    /// A loop summary cut code inside the run, below its own text: the run
    /// over-approximates, so it may teach the definition, but no site summary.
    soft: bool,
    /// Whether its arguments hold unknown text.
    unknown: bool,
    args: Rc<[Vec<Token>]>,
    /// Position of the input frame below the call's text. A run ends cleanly
    /// only if that frame is still where the call left it.
    below: Option<FrameMark>,
    /// Whether it records a run for this call site, not only for the definition.
    site: bool,
    /// Group depth at the start (a group standing for many counts once).
    depth: usize,
    /// [`Machine::learn_forks`] at the start of the run.
    forks_at: u64,
    /// The definition it runs. Learning is keyed by the code, not the name.
    code: Rc<crate::engine::tex::MacroDef>,
    /// Where its effect may be kept from ([`Memo`]).
    memo: Option<Rc<MemoStart>>,
}

/// What a kept effect needs to stand as it stood: lists, `\\aftergroup`
/// tokens, conditionals, categories, pending prefixes. A run leaves all as
/// it met them, so a replay changes none.
#[derive(Debug, Clone, PartialEq)]
struct Stand {
    mode: (crate::engine::mode::Modes, crate::engine::mode::Modes, u8),
    page: (bool, bool),
    natural: Option<crate::engine::mode::Natural>,
    tail: (crate::engine::mode::Last, crate::engine::mode::Dim),
    after: usize,
    conds: (usize, Option<Span>),
    catcodes: crate::engine::tex::CatcodeTable,
    pending: (Option<crate::engine::tex::Tok>, [bool; 5], i64),
    /// Whether what it defines is certain ([`Machine::exact`], no choice open).
    certain: (bool, bool),
}

/// What a run may have done besides assignments: reports, files, outputs,
/// choices, slot changes no assignment replays, reads of the clock or the
/// input line. Equal before and after means it did none.
#[derive(Debug, Clone, PartialEq)]
struct Counts {
    said: [usize; 9],
    side: u64,
    odd: u64,
    forks: (u64, u64, u64, u64),
}

#[derive(Debug)]
pub(crate) struct MemoStart {
    mark: crate::engine::env::MemoMark,
    assignments: u64,
    steps: u64,
    counts: Counts,
    stand: Stand,
    facts: [usize; 5],
    /// The kept effect this run checks (`limits.call_memo_verify`).
    verify: Option<(Rc<Memo>, Sym)>,
    /// [`Machine::memo_said`]'s length at the start.
    said: usize,
}

/// What decides where a run's occurrences stand ([`Machine::raised_at`]).
type Raise = (Option<(Sym, Span)>, Option<Span>, Option<Span>);

/// Occurrences a run said again, each with its kind, text and place.
type Said = Rc<[(crate::engine::builtins::OccKind, String, Span)]>;

/// A call's kept effect: called again on the same arguments while every
/// name its run read holds what it held then, the call makes the run's
/// assignments, each local or global as the run left it. Exact.
#[derive(Debug)]
pub(crate) struct Memo {
    args: Rc<[Vec<Token>]>,
    /// Whether an argument holds a token of the user's own files.
    own: bool,
    stand: Stand,
    /// Each name read, whether more than its meaning, and what it held.
    reads: Vec<(u32, bool, Option<Binding>)>,
    /// The names the run gave the read log of enclosing records
    /// ([`crate::engine::env::Env::reads_since`]), a subset of `reads`: a replay logs these, no more.
    /// `None` when that log ran over: it replays only while the log is off.
    logged: Option<Vec<Sym>>,
    /// Occurrences the run said again, each said before it, and where it
    /// raised them: a replay holds only where each is still said before.
    said: Option<(Raise, Said)>,
    writes: Vec<(Sym, Binding, bool)>,
    /// A run that closed its caller's group ends there: its `\\endgroup`, and
    /// the tokens its text had left to read. A replay closes the group and reads them.
    rest: Option<(Token, Rc<[Token]>)>,
    steps: u64,
    approximated: bool,
}

impl CallRec {
    pub(crate) fn memo_mark(&self) -> Option<crate::engine::env::MemoMark> {
        self.memo.as_ref().map(|m| m.mark)
    }
}

/// Whether a definition's calls are still recorded for kept effects.
#[derive(Clone)]
pub(crate) struct MemoCode {
    _code: Rc<crate::engine::tex::MacroDef>,
    misses: u16,
    kept: u32,
}

/// Whether `b` holds a token at one of `spans`.
fn carries(b: &Binding, spans: &[Span]) -> bool {
    fn value(v: &crate::engine::value::Value, spans: &[Span]) -> bool {
        use crate::engine::value::Value;
        match v {
            Value::Toks(t) => t.iter().any(|t| spans.contains(&t.span)),
            Value::OneOf(vs) => vs.iter().any(|v| value(v, spans)),
            _ => false,
        }
    }
    let text = |m: &Meaning| m.as_macro().is_some_and(|d| d.replacement_text.iter().any(|t| spans.contains(&t.span)));
    text(&b.meaning) || b.may.iter().flat_map(|m| m.iter()).any(text) || value(&b.value, spans)
}

fn memo_key(code: &Rc<crate::engine::tex::MacroDef>, args: &[Vec<Token>]) -> u64 {
    use std::hash::{BuildHasher, Hash, Hasher};
    let mut h = crate::engine::tex::NameHash.build_hasher();
    (Rc::as_ptr(code) as usize).hash(&mut h);
    for a in args {
        a.len().hash(&mut h);
        a.iter().for_each(|t| t.tok.hash(&mut h));
    }
    h.finish()
}

/// One recorded run: the names it wrote with their final values, and the names it read.
#[derive(Debug, Clone)]
struct Run {
    writes: Vec<(Sym, Option<Binding>)>,
    reads: Vec<Sym>,
    /// Values of count registers it read but did not write. A later call that sees other values may behave differently.
    read_values: Vec<(Sym, Option<Binding>)>,
    args: Rc<[Vec<Token>]>,
    /// Whether it closed the group around its call (an `\\endgroup` for a
    /// `\\begingroup` its caller opened).
    closes: bool,
    /// Whether it ended in `\\ignorespaces`.
    peeks: bool,
    /// List bits a soft run may have added (the unknown text after a cut is
    /// typeset): a learned effect adds them and forgets the list's size.
    typeset: Option<u8>,
    /// It forked or a summary cut code inside it: an approximation may have
    /// written more names, so a lesson takes the union and widens what disagrees.
    loose: bool,
}

/// The position of an input frame: a token list and offset (the `Rc` is held
/// so its address is not reused), tokens put back, or a place in a file.
#[derive(Debug, Clone)]
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
    /// Never summarized: a run read past its own text, changed catcodes, or
    /// left groups, conditionals or the mode changed.
    leaky: bool,
    reported: bool,
    /// Number of runs when the shape checks last passed. The checks compare
    /// whole texts, so they are not repeated for the same runs.
    shaped: Option<usize>,
    /// Number of runs being recorded right now.
    active: usize,
    /// The code whose address keys a learned entry. Held so the address is not reused.
    code: Option<Rc<crate::engine::tex::MacroDef>>,
}

/// Whether a meaning is `\iftrue` or `\iffalse`, the two values of a flag.
fn flag(m: &Meaning) -> bool {
    use crate::engine::builtins::{Cond, Primitive as P};
    matches!(m, Meaning::Primitive(P::If(Cond::True | Cond::False)))
}

/// What a learned effect leaves in a name.
enum Written {
    Number,
    Register,
    Flag,
    Same,
    /// Runs that approximated disagree: anything of its kind.
    Any,
}

/// The key under which a definition's learned runs are stored.
fn learned_key(code: &Rc<crate::engine::tex::MacroDef>) -> CallKey {
    (NodeId::default(), Span::default(), Rc::as_ptr(code) as u64)
}

impl Machine<'_> {
    /// Drops the records of calls that are not being recorded right now.
    pub(super) fn forget_settled_calls(&mut self) {
        self.call_runs.retain(|_, runs| runs.active > 0);
    }

    /// The key of a call of `code` (defined at `def`) at `span` with
    /// `arguments`. The body counts too: a name redefined at one place
    /// (`\use:x`'s scratch macro) runs other code each time.
    pub(crate) fn call_key(
        &self,
        def: NodeId,
        code: &crate::engine::tex::MacroDef,
        span: Span,
        arguments: &[Vec<Token>],
    ) -> CallKey {
        use std::hash::{BuildHasher, Hash, Hasher};
        let mut h = crate::engine::tex::NameHash.build_hasher();
        // Unknown values count as one: a summary joins over them.
        let mut add = |t: &Token| match t.tok {
            crate::engine::tex::Tok::Cs(sym) if self.is_unknown_marker(sym) => {}
            tok => tok.hash(&mut h),
        };
        code.replacement_text.iter().for_each(&mut add);
        for a in arguments {
            for t in a {
                if let crate::engine::tex::Tok::Cs(sym) = t.tok
                    && !self.is_unknown_marker(sym)
                {
                    sym.hash(&mut h);
                }
            }
            0u32.hash(&mut h);
        }
        (def, span, h.finish())
    }

    /// `limits.call_summary_after` for a call the main loop expands. 0 for a
    /// call a scan expands: the scan reads its text, so its run ends inside the
    /// scanning command, which a summary would skip.
    fn summary_after(&self) -> usize {
        if self.step_depth == 1 { usize::from(self.limits.call_summary_after) } else { 0 }
    }

    /// Called when a call's body is pushed. Records the run while the site has
    /// too few runs, and every run of a definition that may still learn or use its effect.
    pub(crate) fn record_call(
        &mut self,
        key: CallKey,
        code: &Rc<crate::engine::tex::MacroDef>,
        unknown: bool,
        args: &[Vec<Token>],
        head: bool,
    ) {
        let needed = self.summary_after();
        let learn =
            self.step_depth == 1 && self.learns() && !self.call_runs.get(&learned_key(code)).is_some_and(|r| r.leaky);
        let memo = head && self.memo_records(code) && self.memo_seen(code, args);
        if !memo {
            self.memo_verify = None;
        }
        let runs = self.call_runs.entry(key).or_default();
        let site = !runs.leaky && runs.runs.len() + runs.active < needed;
        if site {
            runs.active += 1;
        }
        // Nested recordings are capped: every call pays for the length of this stack.
        if (site || learn || memo) && self.call_recs.len() < usize::from(self.limits.call_rec_depth) {
            if self.call_recs.is_empty() {
                self.env.log_reads(false);
                self.env.log_reads(true);
            } else if self.env.read_log_len() > crate::engine::env::READ_LOG {
                self.leak_call_recs();
                self.env.log_reads(false);
            }
            // A log that ran over holds for no open record: they keep
            // nothing, and it starts again.
            if memo && self.env.memo_full() {
                for rec in Rc::make_mut(&mut self.call_recs) {
                    Rc::make_mut(rec).memo = None;
                }
                self.env.memo_floor(None);
            }
            // A run past `limits.call_memo_longest` keeps nothing: what
            // it said and read grows with it, and its log costs every read.
            let (now, longest) = (self.out.steps, self.limits.call_memo_longest);
            if memo && self.call_recs.iter().any(|r| r.memo.as_ref().is_some_and(|m| now - m.steps > longest)) {
                for rec in Rc::make_mut(&mut self.call_recs) {
                    if rec.memo.as_ref().is_some_and(|m| now - m.steps > longest) {
                        Rc::make_mut(rec).memo = None;
                    }
                }
                let floor = self.call_recs.iter().rev().find_map(|r| r.memo_mark());
                self.env.memo_floor(floor);
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
                side: self.file_writes,
                leaky: false,
                soft: false,
                unknown,
                args: args.into(),
                below: self.input.last().map(frame_mark),
                site,
                depth: self.env.depth(),
                forks_at: self.learn_forks,
                code: code.clone(),
                memo: memo.then(|| {
                    Rc::new(MemoStart {
                        mark: self.env.memo_open(),
                        assignments: self.env.assignments,
                        steps: self.out.steps,
                        counts: self.memo_counts(),
                        stand: self.stand(),
                        facts: self.fact_lens(),
                        verify: self.memo_verify.take(),
                        said: self.memo_said.len(),
                    })
                }),
            };
            crate::push_shared(&mut self.call_recs, Rc::new(rec));
        }
    }

    /// For a repeated call with unknown operands, applies the join of its
    /// recorded runs instead of running it. Returns `true` when it did.
    pub(crate) fn summarize_call(&mut self, key: CallKey, arguments: &[Vec<Token>]) -> bool {
        let needed = self.summary_after();
        let Some(runs) = self.call_runs.get(&key) else { return false };
        if needed == 0 || runs.leaky || runs.runs.len() < needed {
            return false;
        }
        // Runs that wrote different names chose them from data, so a third
        // run may write yet another name. A call refused here, or by the shape
        // checks, is never summarized.
        let names = |r: &Run| r.writes.iter().map(|w| w.0).collect::<Vec<_>>();
        let checked = runs.shaped == Some(runs.runs.len());
        let same = checked || runs.runs.windows(2).all(|w| names(&w[0]) == names(&w[1]));
        if !same
            || !checked
                && runs
                    .runs
                    .windows(2)
                    .any(|w| accumulates(&w[0], &w[1]) || updates(&w[0], &w[1]) || self.folds_code(&w[0], &w[1]))
        {
            let runs = self.call_runs.get_mut(&key).expect("recorded");
            runs.leaky = true;
            runs.runs.clear();
            return false;
        }
        let len = runs.runs.len();
        self.call_runs.get_mut(&key).expect("recorded").shaped = Some(len);
        let runs = self.call_runs.get(&key).expect("recorded");
        // Summarize only if an operand is unknown: an argument, or a name the runs read.
        let unknown = arguments.iter().any(|a| self.has_unknown(a))
            || runs
                .runs
                .iter()
                .flat_map(|r| &r.reads)
                .any(|&sym| self.env.slot(sym).is_some_and(|b| self.imprecise(b)));
        if !unknown || runs.runs.iter().any(|r| self.reads_otherwise(r)) {
            return false;
        }
        self.apply_summary(key, key.1, arguments)
    }

    /// The count registers a run read but did not write, with their precise
    /// values (the page, a counter, a compared value).
    fn read_values(&self, reads: &[Sym], writes: &[(Sym, Option<Binding>)]) -> Vec<(Sym, Option<Binding>)> {
        let count = |sym: Sym| self.name(sym).strip_prefix(super::REGISTER).is_some_and(|n| n.starts_with("count"));
        let held = |&sym: &Sym| {
            Some((sym, self.env.slot_at(sym.0))).filter(|(_, b)| b.as_ref().is_some_and(|b| !self.imprecise(b)))
        };
        reads.iter().filter(|&&sym| count(sym) && !writes.iter().any(|w| w.0 == sym)).filter_map(held).collect()
    }

    /// Whether a count the run read now holds a different precise value. The
    /// call may then decide differently (like a note column reading the page),
    /// so the run does not describe it.
    fn reads_otherwise(&self, run: &Run) -> bool {
        let differs = |(sym, b): &(Sym, Option<Binding>)| {
            let now = self.env.slot_at(sym.0);
            now.as_ref().is_some_and(|n| !self.imprecise(n)) && !crate::engine::env::same_slot(&now, b)
        };
        run.read_values.iter().any(differs)
    }

    /// Applies the joined effect of the recorded runs. Returns `false` when a run
    /// wrote a typed number from its own arguments that this call's arguments
    /// do not hold, since that number does not belong to this call.
    fn apply_summary(&mut self, key: CallKey, span: Span, arguments: &[Vec<Token>]) -> bool {
        let runs = std::mem::take(&mut self.call_runs.get_mut(&key).expect("recorded").runs);
        let mut syms: Vec<Sym> = runs.iter().flat_map(|r| r.writes.iter().map(|w| w.0)).collect();
        syms.sort_unstable();
        syms.dedup();
        let typed_syms = self.typed_syms();
        let typed = crate::engine::env::typed!(self, typed_syms);
        let mut writes = Vec::with_capacity(syms.len());
        // The typed numbers in a call's arguments, in order.
        let numbers = |args: &[Vec<Token>]| -> Vec<Option<Sym>> {
            (args.iter().flat_map(|a| self.whole_numbers(a)))
                .map(|t| t.cs().filter(|&s| self.typed_digits_of(s).is_some()))
                .collect()
        };
        let here = numbers(arguments);
        let to = |r: &Run| -> Option<Vec<(Sym, Sym)>> {
            let theirs = numbers(&r.args);
            let alike = theirs.len() == here.len() && theirs.iter().zip(&here).all(|(a, b)| a.is_some() == b.is_some());
            alike.then(|| theirs.iter().zip(&here).filter_map(|(a, b)| Some(((*a)?, (*b)?))).collect())
        };
        let maps: Option<Vec<Vec<(Sym, Sym)>>> = runs.iter().map(to).collect();
        // Whether a write mentions a typed number from its run's arguments that this call's arguments do not have.
        let foreign = |r: &Run, b: &Option<Binding>| {
            let theirs = numbers(&r.args);
            let Some(b) = b else { return false };
            let toks = match &b.value {
                crate::engine::value::Value::Toks(t) => &t[..],
                _ => &[],
            };
            let macros = std::iter::once(&b.meaning).chain(b.may.iter().flat_map(|m| m.iter()));
            (macros.filter_map(|m| m.as_macro()).flat_map(|d| d.replacement_text.iter()).chain(toks))
                .filter_map(|t| t.cs())
                .map(|s| self.typed_slice(s).map_or(s, |c| c.0))
                .any(|s| theirs.contains(&Some(s)) && !here.contains(&Some(s)))
        };
        let mut refused = false;
        let index: Vec<std::collections::HashMap<Sym, &Option<Binding>>> =
            runs.iter().map(|r| r.writes.iter().map(|(s, b)| (*s, b)).collect()).collect();
        let at = |r: &Run| runs.iter().position(|q| std::ptr::eq(q, r)).expect("a run of these");
        for &sym in &syms {
            let now = self.env.slot_at(sym.0);
            let of = |r: &Run| index[at(r)].get(&sym).map_or_else(|| now.clone(), |&b| b.clone());
            // Every run wrote the same text in terms of its own arguments'
            // numbers, so this call writes that text with its own numbers.
            if let Some(maps) = &maps
                && maps.iter().all(|m| m.iter().any(|(a, b)| a != b))
            {
                let mine: Option<Vec<_>> = runs.iter().zip(maps).map(|(r, m)| self.relate(&of(r), m)).collect();
                if let Some(mine) = mine
                    && mine.windows(2).all(|w| crate::engine::env::same_slot(&w[0], &w[1]))
                    && !crate::engine::env::same_slot(&mine[0], &of(&runs[0]))
                {
                    if let Some(mut b) = mine.into_iter().next().flatten() {
                        b.certain = false;
                        writes.push((sym, b));
                    }
                    continue;
                }
            }
            if runs.iter().any(|r| foreign(r, &of(r))) {
                refused = true;
                break;
            }
            if let Some(mut b) = self.join_runs(runs.iter().map(of), &typed) {
                b.certain = false;
                writes.push((sym, b));
            }
        }
        if refused {
            self.call_runs.get_mut(&key).expect("recorded").runs = runs;
            return false;
        }
        self.mint();
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
            crate::util::progress::note!("satex: call at {file}:{}:{} summarized: {names:?}", span.line, span.col);
        }
        if !reported {
            self.diagnose(
                Severity::Imprecision,
                "call-summarized",
                span,
                "a call with unknown operands met again here: what it does is the join of its first runs".to_string(),
            );
        }
        true
    }

    /// Called before a read from frame `self.input.len() - 1`. Ends every
    /// call recording that started above that frame.
    pub(crate) fn end_call_recs(&mut self) {
        self.end_calls(false);
    }

    /// Called before `\\ignorespaces` reads on. A call that ends in it is
    /// over, and the lookahead must be repeated after a summary.
    pub(crate) fn peek_ends_calls(&mut self) {
        if !self.call_recs.is_empty() {
            self.end_used_up_lists();
            self.end_calls(true);
        }
    }

    fn end_calls(&mut self, peeks: bool) {
        let needed = usize::from(self.limits.call_summary_after);
        while let Some(rec) = self.call_recs.last()
            && self.input.len() <= rec.base
        {
            let rec = Rc::make_mut(&mut self.call_recs).pop().expect("one");
            let main = (peeks || self.reading_command) && self.step_depth == 1;
            // The run is complete only if nothing below the call's text was
            // read or put back. A lookahead past its end leaves it unfinished.
            let intact = self.input.len() == rec.base && self.input.last().map(frame_mark) == rec.below;
            let ended = main
                && intact
                && !rec.leaky
                && self.env.recent_len() >= rec.recent
                && self.conds.len() == rec.conds
                && self.mode == rec.mode
                && self.file_writes == rec.side;
            let listed = self.natural == rec.natural && self.material == rec.material;
            let whole = ended && listed;
            // After a cut the unknown rest is typeset: a lesson keeps that it may have been.
            let typeset = (rec.soft && !listed).then_some(self.material & !rec.material);
            let closes = self.env.group_level() + 1 == rec.groups;
            let balanced = self.env.group_level() == rec.groups;
            if let Some(start) = &rec.memo {
                let clean = main
                    && intact
                    && !peeks
                    && balanced
                    && self.env.recent_len() >= rec.recent
                    && self.learn_forks == rec.forks_at;
                self.end_memo_run(&rec, start, clean);
                let floor = self.call_recs.iter().rev().find_map(|r| r.memo_mark());
                if floor.is_none() {
                    self.memo_said.clear();
                }
                self.env.memo_floor(floor);
            }
            self.learn_run(&rec, ended && (listed || rec.soft) && (balanced || closes), closes, peeks, typeset);
            if !rec.site {
                continue;
            }
            let clean = whole && balanced && !peeks && !rec.soft;
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
            // Only runs over unknown operands are recorded. What a run on
            // known values wrote holds only for those values.
            if !rec.unknown && !reads.iter().any(|&sym| self.env.slot(sym).is_some_and(|b| self.imprecise(b))) {
                continue;
            }
            let writes: Vec<_> =
                self.env.recent_since(rec.recent).into_iter().map(|sym| (sym, self.env.slot_at(sym.0))).collect();
            // A run that wrote nothing acted only through the tokens it left
            // or consumed, which a summary cannot replay.
            if writes.is_empty() {
                let entry = self.call_runs.get_mut(&rec.key).expect("entry");
                entry.leaky = true;
                entry.runs.clear();
                continue;
            }
            let read_values = self.read_values(&reads, &writes);
            let run = Run {
                writes,
                reads,
                read_values,
                args: rec.args.clone(),
                closes: false,
                peeks: false,
                typeset: None,
                loose: false,
            };
            self.call_runs.get_mut(&rec.key).expect("entry").runs.push(run);
        }
        if self.call_recs.is_empty() {
            self.env.log_reads(false);
        }
    }

    /// A run of `rec`'s definition ended. Records it as a lesson about the
    /// definition unless it forked. A run that did more than a learned effect
    /// can express, or runs that disagree, stop learning for the definition.
    fn learn_run(&mut self, rec: &CallRec, whole: bool, closes: bool, peeks: bool, typeset: Option<u8>) {
        let key = learned_key(&rec.code);
        if !self.learns() {
            return;
        }
        let k = usize::from(self.limits.call_learn_after);
        let entry = self.call_runs.entry(key).or_default();
        entry.code.get_or_insert_with(|| rec.code.clone());
        let forked = self.learn_forks > rec.forks_at;
        if entry.leaky || entry.runs.len() >= k {
            return;
        }
        let same_args = |a: &[Vec<Token>], b: &[Vec<Token>]| {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.iter().map(|t| t.tok).eq(y.iter().map(|t| t.tok)))
        };
        if whole && !forked && entry.runs.iter().any(|r| same_args(&r.args, &rec.args)) {
            return;
        }
        // A partial run teaches nothing: decided before its writes are collected,
        // which costs as much as the run wrote.
        let writes: Vec<_> = match whole {
            true => (self.env.recent_within(rec.recent, self.env.depth() as u32).into_iter())
                .map(|sym| (sym, self.env.slot_at(sym.0)))
                .collect(),
            false => Vec::new(),
        };
        // It wrote a name from its arguments, so the next call writes another name.
        let handed = writes.iter().any(|w| rec.args.iter().flatten().any(|t| t.cs() == Some(w.0)));
        let entry = self.call_runs.get_mut(&key).expect("entry");
        if !whole || handed || writes.is_empty() {
            entry.leaky = true;
            entry.runs.clear();
            return;
        }
        entry.runs.push(Run {
            writes,
            reads: Vec::new(),
            read_values: Vec::new(),
            args: rec.args.clone(),
            closes,
            peeks,
            typeset,
            loose: forked || rec.soft,
        });
        if entry.runs.len() == k {
            let runs = std::mem::take(&mut entry.runs);
            let names = |r: &Run| r.writes.iter().map(|w| w.0).collect::<Vec<_>>();
            let loose = runs.iter().any(|r| r.loose);
            let alike = runs.windows(2).all(|w| {
                (loose || names(&w[0]) == names(&w[1])) && (w[0].closes, w[0].peeks) == (w[1].closes, w[1].peeks)
            }) && self.typed_effect(&runs);
            let entry = self.call_runs.get_mut(&key).expect("entry");
            entry.leaky = !alike;
            if alike {
                entry.runs = runs;
                entry.shaped = None;
            }
        }
    }

    /// Whether runs are learned from. Never while the kernel is built, since its cache keeps no runs.
    fn learns(&self) -> bool {
        self.limits.call_learn_after > 0 && self.cfg.analysis.assume_call_learned_effect && self.budget.is_none()
    }

    /// Called before an undecided choice. Finds the outermost running call
    /// that forked `limits.call_learn_forks` times and whose definition has a
    /// learned effect. Returns `true` if there is one: the run then unwinds to
    /// the main loop, which applies the effect ([`Machine::end_learned_call`]).
    pub(crate) fn learned_cut(&mut self) -> bool {
        if self.call_recs.is_empty() || !self.learns() {
            return false;
        }
        self.learn_forks += 1;
        let k = usize::from(self.limits.call_learn_after);
        let after = self.limits.call_learn_forks;
        let at = self.call_recs.iter().position(|rec| {
            let learned = self.call_runs.get(&learned_key(&rec.code)).filter(|r| !r.leaky && r.runs.len() >= k);
            !rec.leaky
                && self.learn_forks - rec.forks_at >= u64::from(after)
                && learned.is_some()
                && !self.input.iter().skip(rec.base).any(|f| matches!(f, super::Frame::Boundary))
        });
        self.guard_seen.call_cut = at;
        self.guard_seen.cut |= at.is_some();
        at.is_some()
    }

    /// Called by the main loop after [`Machine::learned_cut`]. Drops the
    /// call's text, closes what it opened, and applies the learned effect.
    /// Returns `false` when a load dropped the learned runs in the meantime.
    pub(super) fn end_learned_call(&mut self, at: usize) -> bool {
        let Some(rec) = self.call_recs.get(at).cloned() else { return false };
        let key = learned_key(&rec.code);
        if !self.call_runs.get(&key).is_some_and(|r| !r.runs.is_empty()) {
            return false;
        }
        self.guard_seen.cut = false;
        let span = self.position_span();
        Rc::make_mut(&mut self.call_recs).truncate(at);
        while self.input.len() > rec.base {
            self.pop_frame();
        }
        self.mark_cut_scans();
        let runs = std::mem::take(&mut self.call_runs.get_mut(&key).expect("checked").runs);
        let keep = rec.depth.saturating_sub(usize::from(runs[0].closes));
        self.close_since((rec.conds, self.env.group_id_at(keep), u64::MAX), false);
        (self.mode, self.natural, self.material) = (rec.mode, rec.natural, rec.material);
        let peeks = runs[0].peeks;
        // The callers' runs now hold that approximation: lessons only.
        if let Some(bits) = runs.iter().filter_map(|r| r.typeset).reduce(|a, b| a | b) {
            self.material |= bits;
            self.natural = None;
            self.soften_call_recs();
        }
        self.apply_learned(&runs, span);
        let entry = self.call_runs.get_mut(&key).expect("checked");
        entry.runs = runs;
        let reported = std::mem::replace(&mut entry.reported, true);
        self.approximated = true;
        if !reported {
            self.diagnose(
                Severity::Assumption,
                "call-learned-effect",
                span,
                "a call whose run forks on unknown values is assumed to leave what its runs on other values left: each value of its type (`analysis.assume_call_learned_effect`)".to_string(),
            );
        }
        if peeks {
            self.ignore_spaces();
        }
        true
    }

    /// The type that every run in `runs` left `sym` with.
    fn written_type(&self, runs: &[Run], sym: Sym) -> Option<Written> {
        fn of(r: &Run, sym: Sym) -> Option<&Binding> {
            r.writes.iter().find(|w| w.0 == sym).and_then(|w| w.1.as_ref())
        }
        let all = |test: &dyn Fn(&Binding) -> bool| {
            runs.iter().all(|r| of(r, sym).is_some_and(|b| b.may.is_none() && test(b)))
        };
        if all(
            &|b| matches!(&b.meaning, Meaning::Macro(d) if d.parameter_text.arity == 0 && self.spells_number(&d.replacement_text)),
        ) {
            Some(Written::Number)
        } else if all(&|b| matches!(b.value, crate::engine::value::Value::Num { .. })) {
            Some(Written::Register)
        } else if runs
            .windows(2)
            .all(|w| crate::engine::env::same_slot(&of(&w[0], sym).cloned(), &of(&w[1], sym).cloned()))
        {
            Some(Written::Same)
        } else if all(&|b| flag(&b.meaning)) {
            Some(Written::Flag)
        } else {
            // Only where an approximation already made it vague: precise
            // texts that differ were chosen from data, as a third run may do.
            let vague = runs.iter().any(|r| of(r, sym).is_some_and(|b| !b.certain || self.imprecise(b)));
            (vague && runs.iter().any(|r| r.loose)).then_some(Written::Any)
        }
    }

    /// The names any of `runs` wrote, each once.
    fn learned_names(runs: &[Run]) -> Vec<Sym> {
        let mut syms: Vec<Sym> = runs.iter().flat_map(|r| r.writes.iter().map(|w| w.0)).collect();
        syms.sort_unstable();
        syms.dedup();
        syms
    }

    /// Runs can be learned only if every name they wrote has a type to widen
    /// to (or is the same in all runs), and at least one holds a number.
    fn typed_effect(&self, runs: &[Run]) -> bool {
        let types: Option<Vec<_>> = Self::learned_names(runs).into_iter().map(|s| self.written_type(runs, s)).collect();
        types.is_some_and(|t| t.iter().any(|t| matches!(t, Written::Number | Written::Register)))
    }

    /// Sets each name the runs wrote to its type: number text becomes an
    /// unknown printed number, a register any value, a flag either meaning.
    /// Names that every run left the same keep that value.
    /// The join of what runs left in a name, widened past the set limits.
    fn join_runs(
        &self,
        mut slots: impl Iterator<Item = Option<Binding>>,
        typed: &crate::engine::env::Typed,
    ) -> Option<Binding> {
        let mut acc = slots.next()?;
        for next in slots {
            let joined = crate::engine::env::join_slot(acc.clone(), next, self.env.sets);
            acc = crate::engine::env::widen_slot(&acc, joined, self.env.sets, typed, crate::engine::env::Reach::Havoc);
        }
        acc
    }

    fn apply_learned(&mut self, runs: &[Run], span: Span) {
        use crate::engine::builtins::{Cond, Primitive as P};
        let typed_syms = self.typed_syms();
        for sym in Self::learned_names(runs) {
            let first = runs.iter().flat_map(|r| &r.writes).find(|w| w.0 == sym && w.1.is_some());
            let Some(first) = first.and_then(|w| w.1.clone()) else { continue };
            let kind = self.written_type(runs, sym);
            let mut b = match kind {
                Some(Written::Number) => {
                    let text = self.printed_dimen(None, span);
                    let Meaning::Macro(d) = &first.meaning else { continue };
                    let d = crate::engine::tex::MacroDef { replacement_text: Rc::from(text), ..(**d).clone() };
                    Binding { meaning: Meaning::Macro(Rc::new(d)), ..first.clone() }
                }
                Some(Written::Flag) => {
                    let [t, f] = [Cond::True, Cond::False]
                        .map(|c| Binding { meaning: Meaning::Primitive(P::If(c)), ..first.clone() });
                    let Some(b) = crate::engine::env::join_slot(Some(t), Some(f), self.env.sets) else { continue };
                    b
                }
                // A run that did not write the name left it as it is now.
                Some(Written::Any) => {
                    let typed = crate::engine::env::typed!(self, typed_syms);
                    let now = self.env.slot_at(sym.0);
                    let of =
                        |r: &Run| r.writes.iter().find(|w| w.0 == sym).map_or_else(|| now.clone(), |w| w.1.clone());
                    let Some(b) = self.join_runs(runs.iter().map(of), &typed) else { continue };
                    b
                }
                _ => first,
            };
            b.certain = false;
            let global = b.global;
            self.mint();
            self.env.set(sym, b, global);
            if matches!(kind, Some(Written::Register)) {
                let typed = crate::engine::env::typed!(self, typed_syms);
                self.env.widen(&[sym], None, &typed);
            }
        }
        self.mint();
    }

    /// Text that spells a number: digits, a sign, a point, typed digits.
    fn spells_number(&self, text: &[Token]) -> bool {
        !text.is_empty()
            && text.iter().all(|t| match t.tok {
                crate::engine::tex::Tok::Chr(c, _) => c.is_ascii_digit() || c == '.' || c == '-',
                crate::engine::tex::Tok::Cs(s) => {
                    self.is_unknown_digits(s)
                        || self
                            .typed_text_of(*t)
                            .is_some_and(|(class, _)| class == crate::engine::tex::TextClass::DIGIT)
                }
                crate::engine::tex::Tok::Param(_) => false,
            })
    }

    /// Whether a binding holds unknown text that a parser would walk token by
    /// token. An inexact number does not count: it is computed with, not walked.
    fn imprecise(&self, b: &Binding) -> bool {
        b.may.is_some() || matches!(&b.meaning, Meaning::Macro(d) if self.has_unknown(&d.replacement_text))
    }
    /// Drops every open call recording at the start or end of a cacheable
    /// load. A replayed load runs none of these calls, so a read load must
    /// keep nothing from them either, or cold and warm runs differ.
    pub(crate) fn drop_call_recs(&mut self) {
        self.env.memo_floor(None);
        self.memo_said.clear();
        for rec in std::mem::take(Rc::make_mut(&mut self.call_recs)) {
            if rec.site
                && let Some(entry) = self.call_runs.get_mut(&rec.key)
            {
                entry.active = entry.active.saturating_sub(1);
            }
        }
        self.env.log_reads(false);
    }

    /// Marks all running call recordings as unusable: they read text that is not
    /// theirs, or changed state a summary cannot replay.
    /// Paths join: a record keeps a lesson or a kept effect only as far as
    /// it does on both, and one the other path lacks is unusable.
    pub(super) fn join_call_recs(x: &mut Rc<Vec<Rc<CallRec>>>, y: &Rc<Vec<Rc<CallRec>>>) {
        if Rc::ptr_eq(x, y) {
            return;
        }
        let same = |a: &CallRec, b: &CallRec| a.key == b.key && a.base == b.base;
        let differ = |(i, a): (usize, &Rc<CallRec>)| {
            y.get(i).is_none_or(|b| {
                !Rc::ptr_eq(a, b)
                    && (!same(a, b) || b.leaky && !a.leaky || b.soft && !a.soft || b.memo.is_none() && a.memo.is_some())
            })
        };
        if !x.iter().enumerate().any(differ) {
            return;
        }
        for (i, rec) in Rc::make_mut(x).iter_mut().enumerate() {
            let rec = Rc::make_mut(rec);
            match y.get(i).filter(|b| same(rec, b)) {
                Some(b) => {
                    rec.leaky |= b.leaky;
                    rec.soft |= b.soft;
                    if b.memo.is_none() {
                        rec.memo = None;
                    }
                }
                None => {
                    rec.leaky = true;
                    rec.memo = None;
                }
            }
        }
    }

    /// A loop summary cut code: records whose text the cut dropped are
    /// unusable. Records still running over-approximate their run, so they
    /// stay lessons for their definitions ([`CallRec::soft`]).
    pub(crate) fn soften_call_recs(&mut self) {
        let len = self.input.len();
        if self.call_recs.iter().any(|rec| !rec.leaky && (!rec.soft || rec.base >= len) || rec.memo.is_some()) {
            for rec in Rc::make_mut(&mut self.call_recs) {
                let rec = Rc::make_mut(rec);
                rec.soft = true;
                rec.leaky |= rec.base >= len;
                rec.memo = None;
            }
        }
    }

    pub(crate) fn leak_call_recs(&mut self) {
        if self.call_recs.iter().any(|rec| !rec.leaky || rec.memo.is_some()) {
            for rec in Rc::make_mut(&mut self.call_recs) {
                let rec = Rc::make_mut(rec);
                rec.leaky = true;
                rec.memo = None;
            }
        }
    }

    /// Running recordings read where they stand (the group level): their
    /// summaries would not hold for other calls. Kept effects compare that
    /// state themselves.
    pub(crate) fn leak_call_summaries(&mut self) {
        if self.call_recs.iter().any(|rec| !rec.leaky) {
            for rec in Rc::make_mut(&mut self.call_recs) {
                Rc::make_mut(rec).leaky = true;
            }
        }
    }

    /// A category changes. Records keep their effect only where a group of
    /// their own restores it before they end.
    pub(crate) fn leak_catcode_recs(&mut self) {
        let depth = self.env.group_level();
        if self.call_recs.iter().any(|rec| !rec.leaky || rec.memo.is_some() && rec.groups >= depth) {
            for rec in Rc::make_mut(&mut self.call_recs) {
                let rec = Rc::make_mut(rec);
                rec.leaky = true;
                if rec.groups >= depth {
                    rec.memo = None;
                }
            }
        }
    }

    /// `\\endgroup` closed the group open at `level`: a recorded run that
    /// began inside it ends its caller's group and ends there. It keeps its
    /// effect when nothing below its text was read, the group held no
    /// `\\aftergroup` tokens or box, and what its text has left to read is
    /// tokens. Only its global writes outlive the group.
    pub(crate) fn note_group_closed(&mut self, level: usize, token: Token, plain: bool) {
        let Some(at) = self.call_recs.iter().position(|r| r.groups == level && r.memo.is_some()) else { return };
        let rec = self.call_recs[at].clone();
        let start = rec.memo.clone().expect("memo");
        let below = rec.base.checked_sub(1).and_then(|i| self.input.get(i)).map(frame_mark);
        let mut rest: Option<Vec<Token>> = (plain
            && self.step_depth == 1
            && below == rec.below
            && self.env.recent_len() >= rec.recent
            && self.learn_forks == rec.forks_at
            && start.verify.is_none())
        .then(Vec::new);
        for frame in self.input.iter().skip(rec.base).rev() {
            let Some(out) = rest.as_mut() else { break };
            match frame {
                super::Frame::Tokens { toks, pos, .. } => out.extend_from_slice(toks.get(*pos..).unwrap_or_default()),
                super::Frame::Returned(tokens) => out.extend(tokens.iter().rev()),
                _ => rest = None,
            }
        }
        let steps = self.out.steps - start.steps;
        let kept = rest.is_some_and(|rest| {
            steps >= u64::from(self.limits.call_memo_steps)
                && self.keep_memo(&rec, &start, steps, Some((token, rest.into())))
        });
        let entry = self.memo_codes.entry(Rc::as_ptr(&rec.code) as usize).or_insert_with(|| MemoCode {
            _code: rec.code.clone(),
            misses: 0,
            kept: 0,
        });
        match kept {
            true => entry.kept += 1,
            false => entry.misses = entry.misses.saturating_add(1),
        }
        // The run's effect stops here: its later part is the caller's.
        for rec in Rc::make_mut(&mut self.call_recs) {
            if rec.groups == level && rec.memo.is_some() {
                Rc::make_mut(rec).memo = None;
            }
        }
        let floor = self.call_recs.iter().rev().find_map(|r| r.memo_mark());
        if floor.is_none() {
            self.memo_said.clear();
        }
        self.env.memo_floor(floor);
    }

    /// Whether a call of `code` is recorded for a kept effect: from the
    /// main loop in the document's body (LaTeX's own test, so plain TeX is
    /// all body), and while its runs were not all short or unclean. The
    /// preamble's facts go into package caches, which a replay must not tell
    /// apart from a read.
    fn memo_records(&mut self, code: &Rc<crate::engine::tex::MacroDef>) -> bool {
        let probes = self.limits.call_memo_probes;
        self.step_depth == 1
            && self.package_capture.is_none()
            && !self.in_preamble()
            && self.limits.call_memo_steps > 0
            && self.budget.is_none()
            && self.cacheable_load.is_none()
            && !self.cfg.trace
            && self
                .memo_codes
                .get(&(Rc::as_ptr(code) as usize))
                .is_none_or(|c| probes == 0 || c.kept > 0 || c.misses < probes)
    }

    /// Whether a call of `code` on `args` was met before: only a repeated
    /// call is recorded, so a call met once costs a hash, not a record.
    fn memo_seen(&mut self, code: &Rc<crate::engine::tex::MacroDef>, args: &[Vec<Token>]) -> bool {
        let key = memo_key(code, args);
        if self.memos.contains_key(&key) {
            return true;
        }
        if self.memo_seen.len() >= self.limits.call_memo_entries as usize {
            self.memo_seen.clear();
        }
        !self.memo_seen.insert(key)
    }

    fn stand(&self) -> Stand {
        let p = &self.prefixes;
        Stand {
            mode: (self.mode, self.list, self.material),
            page: (self.page_touched, self.page_grew),
            natural: self.natural,
            tail: (self.main_tail, self.main_depth),
            after: self.env.after_len(),
            conds: (self.conds.len(), self.conds.last().map(|c| c.at)),
            catcodes: self.catcodes.clone(),
            certain: (self.exact(), self.cds.is_empty()),
            pending: (
                self.after_assignment.map(|t| t.tok),
                [p.global, p.long, p.outer, p.protected, p.immediate],
                self.align_state,
            ),
        }
    }

    fn memo_counts(&self) -> Counts {
        let f = &self.out.facts;
        Counts {
            said: [
                f.diagnostics.len(),
                f.occurrences.len(),
                f.loads.len(),
                f.written_files.len() + f.absent.len() + f.found.len(),
                self.out.written.len(),
                self.written_lines.len(),
                self.out.package_caches.len(),
                f.text.at.len(),
                self.gaps.len(),
            ],
            side: self.side_effects,
            odd: self.env.memo_odds(),
            forks: (self.out.forks, self.out.output_forks, self.learn_forks, self.env.switches),
        }
    }

    fn fact_lens(&self) -> [usize; 5] {
        let f = &self.out.facts;
        [f.defs.len(), f.expansions.len(), f.vars.len(), f.literals.len(), f.letters.len()]
    }

    /// Whether a fact since `from` lies in the user's files: a kept effect
    /// does not say it again at another call's place.
    fn facts_mine(&mut self, from: [usize; 5]) -> bool {
        let f = &self.out.facts;
        let mut files: Vec<_> = (f.defs[from[0]..].iter().map(|d| d.span.file))
            .chain(f.expansions[from[1]..].iter().map(|e| e.span.file))
            .chain(f.vars[from[2]..].iter().map(|v| v.span.file))
            .chain(f.literals[from[3]..].iter().map(|l| l.at.file))
            .chain(f.letters[from[4]..].iter().map(|l| l.0.file))
            .collect();
        files.sort_unstable();
        files.dedup();
        files.into_iter().any(|file| self.own_file(file))
    }

    /// A run of `rec` ended; `clean` when it ended whole and balanced. Its
    /// effect is kept when it ran `limits.call_memo_steps`, did nothing but
    /// assignments, and left everything else as it found it.
    fn end_memo_run(&mut self, rec: &CallRec, start: &MemoStart, clean: bool) {
        let steps = self.out.steps - start.steps;
        if let Some(want) = &start.verify {
            let got = if clean { self.memo_effect(rec, start, steps, false, None) } else { Err("unclean") };
            self.verify_memo(rec, want, got);
            return;
        }
        let kept = clean && steps >= u64::from(self.limits.call_memo_steps) && self.keep_memo(rec, start, steps, None);
        let entry = self.memo_codes.entry(Rc::as_ptr(&rec.code) as usize).or_insert_with(|| MemoCode {
            _code: rec.code.clone(),
            misses: 0,
            kept: 0,
        });
        match kept {
            true => entry.kept += 1,
            false => entry.misses = entry.misses.saturating_add(1),
        }
    }

    fn keep_memo(&mut self, rec: &CallRec, start: &MemoStart, steps: u64, rest: Option<(Token, Rc<[Token]>)>) -> bool {
        if self.memo_count >= self.limits.call_memo_entries as usize {
            return false;
        }
        let Ok(memo) = self.memo_effect(rec, start, steps, true, rest) else { return false };
        let variants = usize::from(self.limits.call_memo_variants);
        let kept = self.memos.entry(memo_key(&rec.code, &rec.args)).or_default();
        // The oldest state gives way: loops meet the newest again.
        if kept.len() >= variants {
            kept.remove(0);
            self.memo_count -= 1;
        }
        kept.push(Rc::new(memo));
        self.memo_count += 1;
        self.out.call_memo[0] += 1;
        true
    }

    /// Whether a run that read `b` met something it could not know.
    fn vague(&self, sym: Sym, value: bool, b: &Binding) -> bool {
        use crate::engine::value::Value;
        let register = || self.name(sym).starts_with(['\u{2}', '\u{4}']);
        self.imprecise(b)
            || value
                && match &b.value {
                    Value::Num { num, .. } => num.lo != num.hi,
                    Value::OneOf(_) => true,
                    Value::Unknown => register(),
                    _ => false,
                }
    }

    fn raise(&self) -> Raise {
        (self.file_call, self.project_site, self.typed_at)
    }

    /// `limits.call_memo_verify`: reports where a run differs from the kept effect it replaces.
    fn verify_memo(&mut self, rec: &CallRec, (want, sym): &(Rc<Memo>, Sym), got: Result<Memo, &'static str>) {
        let mut diff = Vec::new();
        match &got {
            Err(why) => diff.push(why.to_string()),
            Ok(got) => {
                let same = |(s, b, g): &(Sym, Binding, bool), (t, c, h): &(Sym, Binding, bool)| {
                    s == t && g == h && crate::engine::env::same_part(Some(b), Some(c), true)
                };
                let writes = |m: &Memo, o: &Memo| -> Vec<String> {
                    m.writes
                        .iter()
                        .filter(|w| !o.writes.iter().any(|x| same(w, x)))
                        .map(|w| self.name(w.0).to_string())
                        .collect()
                };
                // A run with no read log (it ran over) logs nothing to compare.
                let logged = |m: &Memo, o: &Memo| -> Vec<String> {
                    let (Some(m), Some(o)) = (&m.logged, &o.logged) else { return Vec::new() };
                    m.iter().filter(|s| !o.contains(s)).map(|s| self.name(*s).to_string()).collect()
                };
                let (w, l) = ((writes(got, want), writes(want, got)), (logged(got, want), logged(want, got)));
                if !w.0.is_empty() || !w.1.is_empty() {
                    diff.push(format!("writes: run only {:?}, kept only {:?}", w.0, w.1));
                }
                if !l.0.is_empty() || !l.1.is_empty() {
                    diff.push(format!("read log: run only {:?}, kept only {:?}", l.0, l.1));
                }
                if got.steps != want.steps {
                    diff.push(format!("steps: run {}, kept {}", got.steps, want.steps));
                }
            }
        }
        self.out.call_memo[1] += 1;
        if !diff.is_empty() {
            let at = format!("{}:{}", self.out.file_name(rec.key.1.file), rec.key.1.line);
            crate::util::progress::note!(
                "satex: kept effect of \\{} differs from its run at {at}: {}",
                self.name(*sym),
                diff.join(". ")
            );
        }
    }

    /// The effect a run of `rec` keeps: its assignments, what it read, and the state it stood in.
    /// `strict`: a run that read what it could not know keeps nothing.
    fn memo_effect(
        &mut self,
        rec: &CallRec,
        start: &MemoStart,
        steps: u64,
        strict: bool,
        rest: Option<(Token, Rc<[Token]>)>,
    ) -> Result<Memo, &'static str> {
        let now = self.memo_counts();
        if now != start.counts {
            let c = &start.counts;
            return Err(match () {
                _ if now.said != c.said => {
                    const SAID: [&str; 9] =
                        ["diagnostics", "occurrences", "loads", "files", "written", "lines", "caches", "text", "gaps"];
                    SAID[(0..9).find(|&i| now.said[i] != c.said[i]).unwrap_or(0)]
                }
                _ if now.side != c.side => "side effects",
                _ if now.odd != c.odd => "odd",
                _ if now.forks.0 != c.forks.0 => "forks",
                _ if now.forks.1 != c.forks.1 => "output forks",
                _ if now.forks.2 != c.forks.2 => "learned cuts",
                _ => "path switches",
            });
        }
        // Closing the caller's group put back its categories and `\\aftergroup` list, as a replay's close does.
        let now = self.stand();
        let want = match rest {
            Some(_) => Stand { catcodes: now.catcodes.clone(), after: now.after, ..start.stand.clone() },
            None => start.stand.clone(),
        };
        if now != want {
            return Err("stand");
        }
        if self.env.written_since(start.assignments).is_none() || self.facts_mine(start.facts) {
            return Err("written/facts");
        }
        let Some((reads, pre, [globals, locals])) = self.env.memo_since(start.mark) else { return Err("full") };
        let logged = self.env.reads_since(rec.reads);
        let mut writes = Vec::new();
        for sym in self.env.recent_since(rec.recent) {
            // Unbound again, or written both globally and locally: no one
            // assignment says that at another group level.
            let Some(now) = self.env.peek(sym.0).cloned() else { return Err("unbound") };
            let global = globals.binary_search(&sym.0).is_ok();
            if global == locals.binary_search(&sym.0).is_ok() {
                return Err("global and local");
            }
            writes.push((sym, now, global));
        }
        let held = |sym: u32| pre.get(&sym).cloned().unwrap_or_else(|| self.env.peek(sym).cloned());
        let reads: Vec<_> = reads.into_iter().map(|(sym, value)| (sym, value, held(sym))).collect();
        if strict && reads.iter().any(|(sym, value, b)| b.as_ref().is_some_and(|b| self.vague(Sym(*sym), *value, b))) {
            return Err("vague read");
        }
        // The user's tokens of its arguments that what it wrote carries:
        // such a call is the same only at the same place.
        let mut spans: Vec<Span> = Vec::new();
        for t in rec.args.iter().flatten() {
            if self.own_file(t.span.file) {
                spans.push(t.span);
            }
        }
        let own = !spans.is_empty()
            && (writes.iter().any(|(_, b, _)| carries(b, &spans))
                || rest.as_ref().is_some_and(|r| r.1.iter().any(|t| spans.contains(&t.span))));
        let said = self.memo_said.get(start.said..).filter(|s| !s.is_empty()).map(|s| (self.raise(), s.into()));
        Ok(Memo {
            said,
            logged,
            own,
            args: rec.args.clone(),
            stand: start.stand.clone(),
            reads,
            writes,
            rest,
            steps,
            approximated: self.approximated,
        })
    }

    /// A call of `code` on `args` that a kept effect stands for: makes its
    /// assignments instead of running it. `true` when it did.
    pub(crate) fn replay_memo(
        &mut self,
        sym: Sym,
        code: &Rc<crate::engine::tex::MacroDef>,
        args: &[Vec<Token>],
    ) -> bool {
        self.memo_verify = None;
        if self.memos.is_empty() || !self.memo_records(code) {
            return false;
        }
        let key = memo_key(code, args);
        if !self.memos.contains_key(&key) {
            return false;
        }
        let stand = self.stand();
        let memos = &self.memos[&key];
        let env = &self.env;
        let holds = |m: &Memo| {
            // A run that wrote the user's tokens of its arguments holds
            // only at the same place: a replay writes them as they stood.
            let same = |a: &Token, b: &Token| a.tok == b.tok && (!m.own || a.span == b.span);
            m.args.len() == args.len()
                && m.args.iter().zip(args).all(|(x, y)| x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b)))
                && m.stand == stand
                && m.reads
                    .iter()
                    .all(|(sym, value, b)| crate::engine::env::same_part(env.peek(*sym), b.as_ref(), *value))
                && (m.logged.is_some() || !env.logging())
                && (m.rest.is_none()
                    || env.group_kind() == Some(crate::engine::env::GroupKind::SemiSimple) && env.nest().is_none())
                && m.said.as_ref().is_none_or(|(raise, said)| {
                    *raise == self.raise() && said.iter().all(|(k, key, at)| self.said_before(*k, key, *at).is_some())
                })
        };
        let Some(memo) = memos.iter().find(|m| holds(m)).cloned() else { return false };
        if self.limits.call_memo_verify > 0 {
            self.memo_verify = Some((memo, sym));
            return false;
        }
        // Records around this call read what its run read.
        let reads: Vec<(u32, bool)> = memo.reads.iter().map(|(sym, value, _)| (*sym, *value)).collect();
        self.env.replay_reads(&reads, memo.logged.as_deref().unwrap_or_default());
        if let Some((_, said)) = &memo.said {
            for (kind, key, at) in said.iter() {
                self.occurrence(*kind, key.clone(), None, *at);
            }
        }
        for (sym, b, global) in &memo.writes {
            self.env.set(*sym, b.clone(), *global);
        }
        if let Some((token, rest)) = &memo.rest {
            self.close_group(crate::engine::env::GroupKind::SemiSimple, *token);
            self.push_tokens(rest.clone());
        }
        self.approximated |= memo.approximated;
        self.sync_end_line();
        self.out.call_memo[1] += 1;
        self.out.call_memo[2] += memo.steps;
        true
    }
}

/// expl3's message wrapping: the macro that breaks the token list into lines,
/// run inside the group `\iow_wrap:nnnN` opens.
const WRAP: (&str, &str) = ("__iow_wrap_do:", "l__iow_wrap_tl");

impl Machine<'_> {
    /// `analysis.assume_messages_output_only`: wrapping a message with unknown
    /// text changes nothing but that text. The wrap runs in the group of
    /// `\iow_wrap:nnnN`, which drops everything it sets except the text. So
    /// the text becomes unknown string text on one path, instead of forking on
    /// each unknown chunk. Returns `true` when it applied.
    pub(crate) fn message_output_only(&mut self, sym: Sym, m: &crate::engine::tex::MacroDef, span: Span) -> bool {
        if !self.cfg.analysis.assume_messages_output_only
            || m.parameter_text.arity != 0
            || self.name(sym) != WRAP.0
            || self.env.group_level() == 0
        {
            return false;
        }
        let tl = self.intern(WRAP.1);
        let Some(b) = self.env.slot(tl).cloned() else { return false };
        let Meaning::Macro(d) = &b.meaning else { return false };
        if b.may.is_none() && !self.has_unknown(&d.replacement_text) {
            return false;
        }
        let text = self.unknown_text(super::Unk::Class(crate::engine::tex::TextClass::WIDE, false, None, &[]), span);
        let d = crate::engine::tex::MacroDef { replacement_text: Rc::from([text]), ..(**d).clone() };
        let b = Binding { meaning: Meaning::Macro(Rc::new(d)), may: None, ..b };
        self.env.set(tl, b, false);
        self.approximated = true;
        self.diagnose(
            Severity::Assumption,
            "message-output-only",
            span,
            "a message over unknown text is assumed to change nothing but the text it writes: its wrapped text is unknown"
                .to_string(),
        );
        true
    }

    /// Whether two runs left a name different in more than data (code, braces,
    /// a stack, a meaning). Joining those would give text that may run as
    /// anything, so they are not joined. Data is a number, or text of letters,
    /// other characters and spaces with the same parameter text.
    fn folds_code<'a>(&self, a: &'a Run, b: &'a Run) -> bool {
        let data = |t: &[Token]| {
            t.iter().all(|t| match t.tok {
                crate::engine::tex::Tok::Chr(_, c) => {
                    matches!(
                        c,
                        crate::engine::tex::Catcode::Letter
                            | crate::engine::tex::Catcode::Other
                            | crate::engine::tex::Catcode::Space
                    )
                }
                crate::engine::tex::Tok::Cs(s) => self.is_unknown_marker(s),
                crate::engine::tex::Tok::Param(_) => false,
            })
        };
        let numeric = |v: &crate::engine::value::Value| {
            !matches!(v, crate::engine::value::Value::Toks(_) | crate::engine::value::Value::Unknown)
        };
        let index = |r: &'a Run| r.writes.iter().map(|(s, b)| (*s, b)).collect::<std::collections::HashMap<_, _>>();
        let (ia, ib) = (index(a), index(b));
        let of = |i: &std::collections::HashMap<Sym, &Option<Binding>>, sym: Sym| {
            i.get(&sym).map_or_else(|| self.env.slot_at(sym.0), |&b| b.clone())
        };
        a.writes.iter().chain(&b.writes).any(|&(sym, _)| {
            let (x, y) = (of(&ia, sym), of(&ib, sym));
            if crate::engine::env::same_slot(&x, &y) {
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
                    (crate::engine::value::Value::Toks(s), crate::engine::value::Value::Toks(t)) => {
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
        Some((_, crate::engine::value::Value::Toks(t))) => Some(t.clone()),
        _ => None,
    }
}

/// Whether the second run left a name holding the first run's text plus more.
/// That is an accumulator, and joining would lose its entries.
fn accumulates(a: &Run, b: &Run) -> bool {
    a.writes.iter().zip(&b.writes).any(|(x, y)| match (text(&x.1), text(&y.1)) {
        (Some(x), Some(y)) if x.len() < y.len() => {
            let same = |p: &[Token], q: &[Token]| p.iter().zip(q).all(|(s, t)| s.tok == t.tok);
            same(&x, &y) || same(&x, &y[y.len() - x.len()..])
        }
        _ => false,
    })
}

/// Whether a name the runs read was left different by them. Each run computed
/// its write from what it read (a stack push, for example), so the join does
/// not describe a later run.
fn updates(a: &Run, b: &Run) -> bool {
    a.writes.iter().zip(&b.writes).any(|(x, y)| a.reads.contains(&x.0) && !crate::engine::env::same_slot(&x.1, &y.1))
}

impl Machine<'_> {
    /// `text` with each complete sequence of slices of one typed number
    /// replaced by that number.
    fn whole_numbers(&self, text: &[Token]) -> Vec<Token> {
        let mut out: Vec<Token> = Vec::with_capacity(text.len());
        let mut n = 0;
        while n < text.len() {
            let cut = |t: &Token| t.cs().and_then(|s| self.typed_slice(s));
            if let Some((whole, mut k, 0, mut j)) = cut(&text[n]) {
                use crate::engine::scan::{complete, continues};
                let mut m = n + 1;
                while !complete(k, j)
                    && let Some((w, l, i, e)) = text.get(m).and_then(cut)
                    && w == whole
                    && continues((k, j), (l, i))
                {
                    (k, j, m) = (k.max(l), e, m + 1);
                }
                if complete(k, j) {
                    out.push(Token::new(crate::engine::tex::Tok::Cs(whole), text[n].span));
                    n = m;
                    continue;
                }
            }
            out.push(text[n]);
            n += 1;
        }
        out
    }

    /// `b` with its typed numbers made whole and renamed by `map`. `None` when
    /// a partial slice of a renamed number remains, since it cannot be renamed.
    fn relate(&self, b: &Option<Binding>, map: &[(Sym, Sym)]) -> Option<Option<Binding>> {
        let stale = std::cell::Cell::new(false);
        let swap = |t: &[Token]| -> Rc<[Token]> {
            (self.whole_numbers(t).into_iter())
                .map(|t| match t.cs().and_then(|s| map.iter().find(|m| m.0 == s)) {
                    Some(&(_, to)) => Token::new(crate::engine::tex::Tok::Cs(to), t.span),
                    None => {
                        let cut = t.cs().and_then(|s| self.typed_slice(s));
                        stale.set(stale.get() || cut.is_some_and(|c| map.iter().any(|m| m.0 == c.0)));
                        t
                    }
                })
                .collect()
        };
        let meaning = |m: &Meaning| match m {
            Meaning::Macro(d) => Meaning::Macro(Rc::new(crate::engine::tex::MacroDef {
                replacement_text: swap(&d.replacement_text),
                ..(**d).clone()
            })),
            m => m.clone(),
        };
        let Some(b) = b else { return Some(None) };
        let b = Binding {
            meaning: meaning(&b.meaning),
            value: match &b.value {
                crate::engine::value::Value::Toks(t) => crate::engine::value::Value::Toks(swap(t)),
                v => v.clone(),
            },
            may: b.may.as_ref().map(|may| may.iter().map(meaning).collect()),
            ..b.clone()
        };
        (!stale.get()).then_some(Some(b))
    }
}
