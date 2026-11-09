//! Call summaries: a call met again at one site, with unknown operands,
//! applies the join of what its first runs over unknown operands did
//! instead of running
//! (`limits.call_summary_after` of them; 0 never summarizes).
//! Forced convergence for parsers that walk unknown text a token at a time.
//!
//! A definition's runs also teach what any later call of it leaves
//! (`limits.call_learn_after` runs on different arguments): a run that keeps
//! forking (`limits.call_learn_forks`) ends there with their effect, each
//! value widened to its type (`analysis.assume_call_learned_effect`).

use std::rc::Rc;

use super::Machine;
use crate::analysis::facts::Severity;
use crate::engine::env::{Binding, NodeId};
use crate::engine::tex::{Meaning, Span, Sym, Token};

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
    mode: crate::engine::mode::Modes,
    /// The list being built: a run that added to it typesets, which a
    /// summary does not carry.
    natural: Option<crate::engine::mode::Natural>,
    material: u8,
    leaky: bool,
    /// Its arguments held unknown text.
    unknown: bool,
    args: Rc<[Vec<Token>]>,
    /// Where the list below the call's text stood: a run ends cleanly
    /// only on that list as the call left it.
    below: Option<FrameMark>,
    /// It records a run of this site, not only of the definition.
    site: bool,
    /// Open groups, many counted as one.
    depth: usize,
    /// [`Machine::learn_forks`] when the run began.
    forks_at: u64,
    /// What it runs: a definition's runs teach, by code, not by name.
    code: Rc<crate::engine::tex::MacroDef>,
}

/// One recorded run: the names it wrote, as it left them, and the names
/// it read.
#[derive(Clone)]
struct Run {
    writes: Vec<(Sym, Option<Binding>)>,
    reads: Vec<Sym>,
    /// What the counts it read but did not write hold, to tell a later call that reads otherwise.
    read_values: Vec<(Sym, Option<Binding>)>,
    args: Rc<[Vec<Token>]>,
    /// It closed the group around its call (`\\endgroup` of a `\\begingroup`
    /// its caller opened).
    closes: bool,
    /// It ended in `\\ignorespaces`.
    peeks: bool,
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
    /// The code a learned entry is keyed by the place of: held, so that
    /// place stays its own.
    code: Option<Rc<crate::engine::tex::MacroDef>>,
}

/// A conditional's meaning a flag switches between.
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
}

/// Where a definition's learned runs lie among the sites' runs.
fn learned_key(code: &Rc<crate::engine::tex::MacroDef>) -> CallKey {
    (NodeId::default(), Span::default(), Rc::as_ptr(code) as u64)
}

impl Machine<'_> {
    /// Forgets the visit counts of calls no run is recording now.
    pub(super) fn forget_settled_calls(&mut self) {
        self.call_runs.retain(|_, runs| runs.active > 0);
    }

    /// The key of a call of meaning `def` at `span` with `arguments`.
    pub(crate) fn call_key(&self, def: NodeId, span: Span, arguments: &[Vec<Token>]) -> CallKey {
        use std::hash::{BuildHasher, Hash, Hasher};
        let mut h = crate::engine::tex::NameHash.build_hasher();
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

    /// `limits.call_summary_after` for a call the main loop expands, 0 for
    /// one a scan expands: the scan reads its text, so a run of it ends
    /// with the scanning command, which a summary would skip.
    fn summary_after(&self) -> usize {
        if self.step_depth == 1 { usize::from(self.limits.call_summary_after) } else { 0 }
    }

    /// As a call's body is pushed: record the run while runs are missing,
    /// and every run of a definition that may yet teach or use its effect.
    pub(crate) fn record_call(
        &mut self,
        key: CallKey,
        code: &Rc<crate::engine::tex::MacroDef>,
        unknown: bool,
        args: &[Vec<Token>],
    ) {
        let needed = self.summary_after();
        let learn =
            self.step_depth == 1 && self.learns() && !self.call_runs.get(&learned_key(code)).is_some_and(|r| r.leaky);
        let runs = self.call_runs.entry(key).or_default();
        let site = !runs.leaky && runs.runs.len() + runs.active < needed;
        if site {
            runs.active += 1;
        }
        // Deeper calls learn nothing: each path shares this stack, so its length is the cost of a call.
        if (site || learn) && self.call_recs.len() < usize::from(self.limits.call_rec_depth) {
            if self.call_recs.is_empty() {
                self.env.log_reads(false);
                self.env.log_reads(true);
            } else if self.env.read_log_len() > crate::engine::env::READ_LOG {
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
                args: args.into(),
                below: self.input.last().map(frame_mark),
                site,
                depth: self.env.depth(),
                forks_at: self.learn_forks,
                code: code.clone(),
            };
            crate::push_shared(&mut self.call_recs, rec);
        }
    }

    /// A call met again with unknown operands: its recorded runs, joined,
    /// stand for it. `true` when they do.
    pub(crate) fn summarize_call(&mut self, key: CallKey, arguments: &[Vec<Token>]) -> bool {
        let needed = self.summary_after();
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
        if !unknown || runs.runs.iter().any(|r| self.reads_otherwise(r)) {
            return false;
        }
        self.apply_summary(key, key.1, arguments);
        true
    }

    /// The precise values among the count registers a run read and left
    /// alone: the page, a counter, what a decision compares.
    fn read_values(&self, reads: &[Sym], writes: &[(Sym, Option<Binding>)]) -> Vec<(Sym, Option<Binding>)> {
        let count = |sym: Sym| self.name(sym).strip_prefix(super::REGISTER).is_some_and(|n| n.starts_with("count"));
        let held = |&sym: &Sym| {
            Some((sym, self.env.slot_at(sym.0))).filter(|(_, b)| b.as_ref().is_some_and(|b| !self.imprecise(b)))
        };
        reads.iter().filter(|&&sym| count(sym) && !writes.iter().any(|w| w.0 == sym)).filter_map(held).collect()
    }

    /// A count the run read now holds another precise value: the call can
    /// decide otherwise than the run did, as a note column reading its
    /// page, so the run says nothing about it.
    fn reads_otherwise(&self, run: &Run) -> bool {
        let differs = |(sym, b): &(Sym, Option<Binding>)| {
            let now = self.env.slot_at(sym.0);
            now.as_ref().is_some_and(|n| !self.imprecise(n)) && !crate::engine::env::same_slot(&now, b)
        };
        run.read_values.iter().any(differs)
    }

    fn apply_summary(&mut self, key: CallKey, span: Span, arguments: &[Vec<Token>]) {
        let runs = std::mem::take(&mut self.call_runs.get_mut(&key).expect("recorded").runs);
        let mut syms: Vec<Sym> = runs.iter().flat_map(|r| r.writes.iter().map(|w| w.0)).collect();
        syms.sort_unstable();
        syms.dedup();
        let typed_syms = self.typed_syms();
        let typed = crate::engine::env::typed!(self, typed_syms);
        let mut writes = Vec::with_capacity(syms.len());
        // Each run's typed numbers by where its arguments hold them.
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
        for &sym in &syms {
            let now = self.env.slot_at(sym.0);
            let of = |r: &Run| r.writes.iter().find(|w| w.0 == sym).map_or_else(|| now.clone(), |w| w.1.clone());
            // Every run wrote the same text of its own arguments' numbers:
            // this call writes it of its own.
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
            let mut acc = of(&runs[0]);
            for r in &runs[1..] {
                let next = of(r);
                let joined = crate::engine::env::join_slot(acc.clone(), next, self.env.sets);
                acc = crate::engine::env::widen_slot(
                    &acc,
                    joined,
                    self.env.sets,
                    &typed,
                    crate::engine::env::Reach::Havoc,
                );
            }
            if let Some(mut b) = acc {
                b.certain = false;
                writes.push((sym, b));
            }
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
    }

    /// A read is about to take a token from frame `self.input.len() - 1`:
    /// every call recorded above it is over.
    pub(crate) fn end_call_recs(&mut self) {
        self.end_calls(false);
    }

    /// `\\ignorespaces` is about to read on: a call that ends in it is over
    /// and left the look-ahead to do again.
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
            // Nothing below the call's text was read or given back to: a
            // look-ahead past its end leaves its run unfinished.
            let intact = self.input.len() == rec.base && self.input.last().map(frame_mark) == rec.below;
            let whole = main
                && intact
                && !rec.leaky
                && self.env.recent_len() >= rec.recent
                && self.conds.len() == rec.conds
                && self.mode == rec.mode
                && self.natural == rec.natural
                && self.material == rec.material;
            let closes = self.env.group_level() + 1 == rec.groups;
            let balanced = self.env.group_level() == rec.groups;
            self.learn_run(&rec, whole && (balanced || closes), closes, peeks);
            if !rec.site {
                continue;
            }
            let clean = whole && balanced && !peeks;
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
            let read_values = self.read_values(&reads, &writes);
            let run = Run { writes, reads, read_values, args: rec.args, closes: false, peeks: false };
            self.call_runs.get_mut(&rec.key).expect("entry").runs.push(run);
        }
        if self.call_recs.is_empty() {
            self.env.log_reads(false);
        }
    }

    /// A run of `rec`'s definition ended: it teaches what the definition
    /// does, unless it forked (that run was not on values). A run that did
    /// more than a learned effect carries, or whose runs disagree, stops the
    /// definition's learning for good.
    fn learn_run(&mut self, rec: &CallRec, whole: bool, closes: bool, peeks: bool) {
        let key = learned_key(&rec.code);
        if !self.learns() {
            return;
        }
        let k = usize::from(self.limits.call_learn_after);
        let entry = self.call_runs.entry(key).or_default();
        entry.code.get_or_insert_with(|| rec.code.clone());
        if entry.leaky || entry.runs.len() >= k || self.learn_forks > rec.forks_at {
            return;
        }
        let same_args = |a: &[Vec<Token>], b: &[Vec<Token>]| {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.iter().map(|t| t.tok).eq(y.iter().map(|t| t.tok)))
        };
        if whole && entry.runs.iter().any(|r| same_args(&r.args, &rec.args)) {
            return;
        }
        let writes: Vec<_> =
            (self.env.recent_since(rec.recent).into_iter()).map(|sym| (sym, self.env.slot_at(sym.0))).collect();
        // A name it was handed: the next call writes another.
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
        });
        if entry.runs.len() == k {
            let runs = std::mem::take(&mut entry.runs);
            let names = |r: &Run| r.writes.iter().map(|w| w.0).collect::<Vec<_>>();
            let alike = runs
                .windows(2)
                .all(|w| names(&w[0]) == names(&w[1]) && (w[0].closes, w[0].peeks) == (w[1].closes, w[1].peeks))
                && self.typed_effect(&runs);
            let entry = self.call_runs.get_mut(&key).expect("entry");
            entry.leaky = !alike;
            if alike {
                entry.runs = runs;
            }
        }
    }

    /// Whether runs teach here: never while the kernel is built, whose
    /// cache holds no run.
    fn learns(&self) -> bool {
        self.limits.call_learn_after > 0 && self.cfg.analysis.assume_call_learned_effect && self.budget.is_none()
    }

    /// An undecided choice is about to be made: the outermost running call
    /// that forked `limits.call_learn_forks` times and whose definition has
    /// a learned effect ends with that effect. `true` when it will; the
    /// run unwinds to the main loop ([`Machine::end_learned_call`]).
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

    /// The main loop, after a [`Machine::learned_cut`]: the call's text goes,
    /// what it opened closes, and what its definition's runs left stands.
    /// `false` when a load forgot those runs meanwhile.
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

    /// The type all of `runs` left `sym` a value of.
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
            None
        }
    }

    /// Runs teach only what has a type to widen to: every name they wrote
    /// has one, or stayed the same, and one holds a number.
    fn typed_effect(&self, runs: &[Run]) -> bool {
        let types: Option<Vec<_>> = runs[0].writes.iter().map(|w| self.written_type(runs, w.0)).collect();
        types.is_some_and(|t| t.iter().any(|t| matches!(t, Written::Number | Written::Register)))
    }

    /// Each name the runs wrote gets its type: number text a printed number,
    /// a register any value, a flag the runs disagree on either meaning.
    /// What every run left the same stays as it is.
    fn apply_learned(&mut self, runs: &[Run], span: Span) {
        use crate::engine::builtins::{Cond, Primitive as P};
        let typed_syms = self.typed_syms();
        for w in &runs[0].writes {
            let Some(first) = w.1.clone() else { continue };
            let kind = self.written_type(runs, w.0);
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
                _ => first,
            };
            b.certain = false;
            let global = b.global;
            self.mint();
            self.env.set(w.0, b, global);
            if matches!(kind, Some(Written::Register)) {
                let typed = crate::engine::env::typed!(self, typed_syms);
                self.env.widen(&[w.0], None, &typed);
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

/// expl3's message formatting: the macro that wraps the text in the
/// token list into lines, inside the group `\iow_wrap:nnnN` opens.
const WRAP: (&str, &str) = ("__iow_wrap_do:", "l__iow_wrap_tl");

impl Machine<'_> {
    /// `analysis.assume_messages_output_only`: wrapping a message's
    /// unknown text into lines changes nothing but that text. The wrap
    /// runs in the group of `\iow_wrap:nnnN`, which drops all it sets
    /// besides the text it hands on. So the text becomes unknown string
    /// text, one path, where the real wrap splits on each unknown chunk.
    /// `true` when it did.
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

    /// Runs that left a name different in more than data: code, braces,
    /// a stack, a meaning. Their join would be text that may run as
    /// anything, or name anything, so they are not joined. Data is a number,
    /// or text of letters, others and spaces under one parameter text.
    fn folds_code(&self, a: &Run, b: &Run) -> bool {
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
        let of = |r: &Run, sym: Sym| {
            r.writes.iter().find(|w| w.0 == sym).map_or_else(|| self.env.slot_at(sym.0), |w| w.1.clone())
        };
        a.writes.iter().chain(&b.writes).any(|&(sym, _)| {
            let (x, y) = (of(a, sym), of(b, sym));
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
    a.writes.iter().zip(&b.writes).any(|(x, y)| a.reads.contains(&x.0) && !crate::engine::env::same_slot(&x.1, &y.1))
}

impl Machine<'_> {
    /// `text` with each run of slices that spells one typed number from its
    /// first place to its end as that number: it holds no more.
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

    /// `b` with its typed numbers whole and those of `map` renamed;
    /// `None` when a part of one of them is left, which has no rename.
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
