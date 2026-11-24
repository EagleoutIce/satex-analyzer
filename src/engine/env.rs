//! The save stack: dense slot table plus undo journal (The TeXbook § 275).
//! Drives conditional analysis: run branch, capture, rollback, join.

use crate::fixed::{HashMap, LookupMap, LookupSet};
use std::rc::Rc;

use crate::engine::mode::Nest;
use crate::engine::tex::{Catcode, CatcodeTable, Interner, Meaning, Span, Sym, TextClass, Tok, Token};
use crate::engine::value::Value;

mod changed;
use changed::Changed;
pub use changed::Walked;
/// No trail entry.
const NONE: u32 = u32::MAX;

pub type NodeId = u32;

/// A copy of a name's definition sites, as a macro call or a reference reads
/// them: nearly always one or two, so the copy stays off the heap.
pub type Defs = smallvec::SmallVec<[NodeId; 2]>;

/// What a control sequence is bound to: its [`Meaning`], [`Value`], and the
/// [`NodeId`]s that defined it. [`Env`] stores one per active [`Sym`].
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Binding {
    pub meaning: Meaning,
    pub value: Value,
    pub defs: Vec<NodeId>,
    pub certain: bool,
    /// Set by `\global`, `\gdef`, `\xdef`, `\glet`.
    pub global: bool,
    /// The meanings a join left the name with, when they differ and are few.
    /// `meaning` is then unknown, which is what code that ignores this set sees.
    pub may: Option<std::rc::Rc<[Meaning]>>,
}

impl Binding {
    pub fn new(meaning: Meaning, def: NodeId) -> Self {
        Self { meaning, value: Value::Unknown, defs: vec![def], certain: true, global: false, may: None }
    }
    pub fn builtin(meaning: Meaning) -> Self {
        Self { meaning, value: Value::Unknown, defs: Vec::new(), certain: true, global: true, may: None }
    }
}

/// Save-stack entry (catcodes too: `\catcode` is local like other assignments).
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
enum Entry {
    Meaning {
        sym: u32,
        old: Option<Binding>,
        /// The level `old` was made at.
        level: u32,
        /// The primitive `old` was a copy of ([`Env::aliases`]).
        alias: Option<Sym>,
        /// When `old` was written ([`Env::stamp`]).
        stamp: u64,
    },
    Catcode {
        character: char,
        old: Catcode,
        /// The categories it may have had, as [`CatcodeTable::may`] gives them.
        old_may: u16,
    },
}

/// Kind of group (tex.web § 269): `{`/`}`, `\begingroup`/`\endgroup`, or `$...$`.
/// A closer closes only its opener's kind ([`GroupKind::closes`], enforced by
/// [`Env::pop_group`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum GroupKind {
    Simple,
    SemiSimple,
    Math {
        display: bool,
    },
    /// `\left`…`\right` (tex.web § 1191 `math_left_group`).
    MathLeft,
}

impl GroupKind {
    pub fn closes(self, group: GroupKind) -> bool {
        matches!(
            (self, group),
            (GroupKind::Simple, GroupKind::Simple)
                | (GroupKind::SemiSimple, GroupKind::SemiSimple)
                | (GroupKind::Math { .. }, GroupKind::Math { .. })
                | (GroupKind::MathLeft, GroupKind::MathLeft)
        )
    }
}

/// The save stack.  A path through an undecided conditional copies it, so
/// what lies below the last copy is frozen in shared chunks: a copy costs
/// what was pushed since, not the whole stack.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
struct Journal {
    /// The first `n` entries of a chunk, and below them its parent's.
    frozen: Option<(std::rc::Rc<Chunk>, usize)>,
    frozen_len: usize,
    top: Vec<Entry>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Chunk {
    entries: Vec<Entry>,
    parent: Option<(std::rc::Rc<Chunk>, usize)>,
}

impl Drop for Chunk {
    // A long chain dropped link by link, not by recursion: the stack would
    // overflow.
    fn drop(&mut self) {
        let mut next = self.parent.take();
        while let Some((rc, _)) = next {
            next = match std::rc::Rc::try_unwrap(rc) {
                Ok(mut chunk) => chunk.parent.take(),
                Err(_) => None,
            };
        }
    }
}

impl Journal {
    fn len(&self) -> usize {
        self.frozen_len + self.top.len()
    }

    fn push(&mut self, entry: Entry) {
        self.top.push(entry);
    }

    fn pop(&mut self) -> Option<Entry> {
        if let Some(entry) = self.top.pop() {
            return Some(entry);
        }
        let (chunk, n) = self.frozen.take()?;
        let entry = chunk.entries[n - 1].clone();
        self.frozen_len -= 1;
        self.frozen = if n > 1 { Some((chunk, n - 1)) } else { chunk.parent.clone() };
        Some(entry)
    }

    /// A copy that shares everything pushed so far with this one.
    fn snapshot(&mut self) -> Journal {
        if !self.top.is_empty() {
            let entries = std::mem::take(&mut self.top);
            let n = entries.len();
            self.frozen = Some((std::rc::Rc::new(Chunk { entries, parent: self.frozen.take() }), n));
            self.frozen_len += n;
        }
        self.clone()
    }
}

/// A logged write: the name, the group depth of a local write (0 for a
/// global one: a local one is undone once fewer groups are open), and
/// [`Env::assignments`] when it was made (0 when unknown).
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
struct Write {
    sym: u32,
    depth: u32,
    at: u64,
}

/// Open group: journal start, opener kind, saved `\aftergroup` tokens.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
struct Mark {
    journal: usize,
    kind: GroupKind,
    after: Vec<Token>,
    /// Where the group was opened, so an unbalanced file can name it.
    opened: Span,
    /// What the group saved of the semantic nest (tex.web § 216).
    nest: Option<Nest>,
    /// Which group this is: ids grow with every opening.
    id: u64,
    /// Whether it stands for any number of such groups, including none (a widened stack).
    many: bool,
}

/// The meaning table and the save stack: [`Binding`]s are pushed and undone
/// per group, which is what `\begingroup` and `}` do.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Env {
    pub slots: crate::fixed::Chunked<Option<Binding>>,
    /// The group level each slot's binding was made at, 0 for level one
    /// (tex.web § 221 `eq_level`). A local assignment saves the old binding
    /// only once per level (§ 277). A `\global` one is made at level one, so
    /// it survives the saving group's end (§ 283).
    level: crate::fixed::Chunked<u32>,
    journal: Journal,
    /// Shared with the forks and paths that saw it. A fork only copies a
    /// pointer; opening or closing a group copies the stack.
    marks: Rc<Vec<Mark>>,
    /// The last group id given out.
    group_ids: u64,
    /// While a path through an undecided conditional runs, records every slot
    /// it changes with the slot's previous binding, so the next path can start
    /// from the same bindings. One entry per slot and fork: the first change
    /// under a fork saves what rewinding restores, and links to the slot's
    /// previous entry, which rewinding puts back.
    /// Chunked: a checkpoint taken while a fork is open shares it.
    #[serde(with = "crate::fixed::flat")]
    trail: crate::fixed::Chunked<(u32, Option<Binding>, u32, u32, u64)>,
    /// Per slot, its latest trail entry, `NONE` for none.
    trailed: crate::fixed::Chunked<u32>,
    /// Per open fork, the fewest groups the running path stood in since.
    lows: Vec<usize>,
    /// Per open fork, innermost last: where its trail begins, and the current
    /// bindings of the slots changed since. Most forks see one path only, so
    /// the map is built from the trail when a path first asks for it.
    floors: Vec<usize>,
    changed: Vec<Option<Changed>>,
    forks: usize,
    /// Forks that several paths continued from. A package load's record of
    /// what it wrote does not follow them.
    paths: usize,
    /// Writes since [`Env::clear_recent`], for widening a loop that does
    /// not end to every state it could have left. Read from a mark on
    /// ([`Env::assignments`] then); [`Env::fold_recent`] bounds it.
    recent: Vec<Write>,
    /// [`Env::assignments`] when the log was last cleared, set or folded:
    /// it answers exactly for marks from then on.
    recent_from: u64,
    /// Entries the last fold kept, 0 since a clear.
    #[serde(skip)]
    recent_kept: usize,
    /// Per slot, [`Env::assignments`] at the write its binding came from.
    /// A group end puts back the old one, so a name whose write a group
    /// end undid is not widened.
    stamp: crate::fixed::Stamps,
    /// What the join the running path came from kept of its arms
    /// ([`Corr`]), less the names written since.
    corr: Option<Rc<Corr>>,
    /// `\let\tex_escapechar:D\escapechar`: the engine primitive a name was
    /// made equal to, for primitives satex models alike (tex.web keeps the
    /// primitive's `chr` code in the copy). A parameter's value lives with
    /// that primitive. `\meaning` and `\ifx` tell such copies apart.
    pub aliases: HashMap<Sym, Sym>,
    /// Assignments made so far: a loop that assigns is changing state, which
    /// a loop that only re-expands itself is not.
    pub assignments: u64,
    /// Count of writes that changed a name or register, or other state a later
    /// read may see (catcodes, `\\read`). Equal counts mean unchanged state.
    pub effective: u64,
    /// The part of `effective` made by slot writes (each logged in `recent`).
    /// The rest is state that no slot holds.
    pub slot_changes: u64,
    /// How large a set of meanings or values a join keeps.
    pub sets: SetLimits,
    /// While a package cache is being captured: every slot read, as a bit
    /// set, which is what the package's effect may depend on.
    tracking: std::cell::Cell<bool>,
    reads: std::cell::RefCell<Reads>,
    /// While calls are recorded: every name read, in order, with the stamp
    /// of the binding it read.
    logging: std::cell::Cell<bool>,
    #[serde(with = "crate::fixed::flat::cell")]
    read_log: std::cell::RefCell<crate::fixed::Chunked<(u32, u64)>>,
    /// Whether the log passed [`READ_LOG`] and stopped, so it is incomplete.
    log_full: std::cell::Cell<bool>,
    /// When the log was cleared, how long it was then, and how long when last asked
    /// ([`Env::read_log_peak`]).
    #[serde(skip)]
    read_spans: std::cell::RefCell<Vec<(u64, usize, usize)>>,
    #[serde(skip)]
    read_asked: std::cell::Cell<usize>,
    /// While a call's effect may be kept ([`MemoLog`]).
    /// Path switches (rewinds, resumes): a run another path's steps
    /// interleaved keeps no effect.
    #[serde(skip)]
    pub switches: u64,
    #[serde(skip)]
    memo_on: std::cell::Cell<bool>,
    #[serde(skip)]
    memo: std::cell::RefCell<MemoLog>,
    /// Map entries walked, for `Machine::charge`.
    #[serde(skip)]
    pub walked: Walked,
    /// While a run keeps checkpoints: when each slot was last read or written.
    #[serde(skip)]
    pub touched: LastUse,
    #[serde(skip)]
    distinct: Distinct,
}

/// Takes each name of a read log once, in one pass over it: per name, the pass that last met
/// it and where. Sorting the log instead cost a record's end `n log n` for `n` reads, most of
/// them repeats. A copy starts empty: it is scratch.
#[derive(Default)]
struct Distinct {
    pass: std::cell::Cell<u32>,
    met: std::cell::RefCell<Vec<(u32, u32)>>,
}

impl Clone for Distinct {
    fn clone(&self) -> Self {
        Distinct::default()
    }
}

impl Distinct {
    /// The names of `reads` (`sym | value << 31`), each once, sorted, with the value bit
    /// set where any of its reads had it.
    fn of(&self, reads: impl Iterator<Item = u32>) -> Vec<u32> {
        let pass = self.pass.get().wrapping_add(1);
        let mut met = self.met.borrow_mut();
        if pass == 0 {
            met.clear();
        }
        self.pass.set(pass.max(1));
        let pass = pass.max(1);
        let mut out: Vec<u32> = Vec::new();
        for r in reads {
            let sym = (r & !(1 << 31)) as usize;
            if sym >= met.len() {
                met.resize(sym + 1, (0, 0));
            }
            match met[sym] {
                (p, at) if p == pass => out[at as usize] |= r & 1 << 31,
                _ => {
                    met[sym] = (pass, out.len() as u32);
                    out.push(r);
                }
            }
        }
        out.sort_unstable_by_key(|r| r & !(1 << 31));
        out
    }
}

/// When each entry of a table (a slot, a recorded list's node) was last used, as the run's
/// step count at its current top-level command: what the run from any point on depended on
/// is every entry with a later time. After an edit the last run's rest is reused only where
/// it used nothing the new run holds differently. A copy of the table's owner (a checkpoint,
/// a probe) keeps the time and starts without the times.
#[derive(Default)]
pub struct LastUse {
    on: std::cell::Cell<bool>,
    now: std::cell::Cell<u64>,
    last: std::cell::RefCell<Vec<u64>>,
}

impl Clone for LastUse {
    fn clone(&self) -> Self {
        LastUse { now: self.now.clone(), ..LastUse::default() }
    }
}

impl LastUse {
    /// Notes entry `i` as used now.
    #[inline]
    pub fn note(&self, i: usize) {
        if self.on.get() {
            let mut last = self.last.borrow_mut();
            if i >= last.len() {
                last.resize(i + 1, 0);
            }
            last[i] = self.now.get();
        }
    }

    /// The time [`LastUse::note`] notes from now on.
    #[inline]
    pub fn tick(&self, now: u64) {
        self.now.set(now);
    }

    /// Notes every use from now on, on top of `last` (an earlier run's [`LastUse::times`],
    /// which this one resumes at `now`: what that run used later, this one has not).
    pub fn track(&self, mut last: Vec<u64>, now: u64) {
        last.iter_mut().for_each(|t| *t = (*t).min(now));
        self.on.set(true);
        self.now.set(now);
        *self.last.borrow_mut() = last;
    }

    /// Per entry, when it was last used, 0 for never.
    pub fn times(&self) -> Vec<u64> {
        self.last.borrow().clone()
    }

    /// `mine` (a run's times up to step `now`, where it met the last run at that run's step
    /// `was`) with what the last run's `rest` used from there on, as many steps later.
    pub fn joined(mut mine: Vec<u64>, rest: &[u64], was: u64, now: u64) -> Vec<u64> {
        mine.resize(mine.len().max(rest.len()), 0);
        for (mine, &rest) in mine.iter_mut().zip(rest) {
            if rest >= was {
                *mine = (*mine).max(rest - was + now);
            }
        }
        mine
    }
}

/// What running calls whose effect may be kept did to the slots: every
/// name read (raw slot reads too), what a slot held before its first write
/// since `floor`, the names written `\global`, and how often a slot changed
/// in a way no assignment replays (`odd`).
#[derive(Clone, Default)]
struct MemoLog {
    reads: Vec<u32>,
    /// Parts of `reads` an ended record took each name of once, by start:
    /// their end and those names, so a record around it reads no part twice.
    parts: std::collections::BTreeMap<usize, (usize, Rc<[u32]>)>,
    pre: Vec<(u32, Option<Binding>)>,
    at: LookupMap<u32, usize>,
    floor: usize,
    globals: Vec<u32>,
    locals: Vec<u32>,
    odd: u64,
    full: bool,
}

/// Where a record's part of the [`MemoLog`] begins.
#[derive(Debug, Clone, Copy)]
pub struct MemoMark {
    reads: usize,
    pre: usize,
    globals: usize,
    locals: usize,
}

/// How many reads the log holds. A call that reads more cannot be summarized.
pub const READ_LOG: usize = 1 << 22;

/// Which part of a binding a read looked at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ReadKind {
    /// The meaning, with the flags and definition sites that go with it.
    Meaning,
    /// The value a register or parameter holds.
    Value,
    /// What assigning a value keeps: the binding without its value (an empty
    /// slot counts as a new register slot).
    Kept,
    /// Only whether the name has a meaning.
    Defined,
    /// Only whether the meaning opens, divides or closes a conditional:
    /// what skipping a conditional's text looks at (tex.web § 494).
    Skip,
    /// Only whether the name has definition sites. The dependency graph links
    /// to them by this. A package cache links the actual sites again on replay.
    Defs,
    /// Only whether the meaning is a text copied through: a parameterless
    /// macro whose all-character replacement text an `\edef` of the same
    /// name copies without looking at it. A replay remakes that definition
    /// from the text it finds.
    Copy,
    /// Only what `\ifx` compares a macro with anything else by: that it is
    /// a macro, or else the whole meaning (tex.web § 507).
    Shape,
}

impl ReadKind {
    fn bit(self) -> u8 {
        1 << self as u8
    }
}

/// An integer a load computed from values it found (its *leaves*: slots it has not written),
/// by TeX's own arithmetic. A package cache stores the expression where the value only flows
/// into what the load leaves, and fills it in from the state it is served into.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Expr {
    Leaf(Sym),
    Const(i64),
    Op(crate::engine::builtins::Arith, Rc<Expr>, Rc<Expr>),
    Neg(Rc<Expr>),
}

impl Expr {
    pub fn leaves(&self, out: &mut Vec<Sym>) {
        match self {
            Expr::Leaf(sym) => out.push(*sym),
            Expr::Const(_) => {}
            Expr::Op(_, a, b) => {
                a.leaves(out);
                b.leaves(out);
            }
            Expr::Neg(a) => a.leaves(out),
        }
    }

    /// Its value where `leaf` gives each leaf's. `None` where a leaf is no known integer or
    /// TeX reports "Arithmetic overflow" (tex.web §§ 1238-1240).
    pub fn eval(&self, leaf: &dyn Fn(Sym) -> Option<i64>) -> Option<i64> {
        match self {
            Expr::Leaf(sym) => leaf(*sym),
            Expr::Const(n) => Some(*n),
            Expr::Op(op, a, b) => crate::engine::exec::int_arith(a.eval(leaf)?, b.eval(leaf)?, *op, false),
            Expr::Neg(a) => Some(crate::engine::value::wrap(-a.eval(leaf)?)),
        }
    }
}

/// Where a hole's tokens come from: the decimal print of an integer, or the whole text of a
/// macro the load found ([`copied_text`]).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Src {
    Dec(Rc<Expr>),
    TextOf(Sym),
}

/// Tokens `at..at + len` of a replacement text stand for `src`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Hole {
    pub at: u32,
    pub len: u32,
    pub src: Src,
}

/// What a slot the load wrote holds, as a function of the load's leaves: its value, or
/// parts of its replacement text. The slot itself holds the concrete content.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Amb {
    Int(Rc<Expr>),
    Text(Vec<Hole>),
}

impl Amb {
    /// The slots it depends on, each with the part read of it.
    pub fn sources(&self, out: &mut Vec<(Sym, ReadKind)>) {
        let mut leaves = Vec::new();
        match self {
            Amb::Int(e) => e.leaves(&mut leaves),
            Amb::Text(holes) => {
                for hole in holes {
                    match &hole.src {
                        Src::Dec(e) => e.leaves(&mut leaves),
                        Src::TextOf(sym) => out.push((*sym, ReadKind::Meaning)),
                    }
                }
            }
        }
        out.extend(leaves.into_iter().map(|sym| (sym, ReadKind::Value)));
    }
}

/// A test a load decided on its leaves: the relation between two expressions and how it came out.
pub type Guard = (Rc<Expr>, char, Rc<Expr>, bool);

/// A number scan that read exactly one integer quantity: what it is as an [`Expr`], the
/// value read, and the exact reads the scan logged for it (to take back when the consumer
/// keeps the expression instead, [`Env::retract`]).
#[derive(Clone, Debug)]
pub struct Scanned {
    pub expr: Rc<Expr>,
    pub value: i64,
    undo: Vec<(u32, u8)>,
    serial: u64,
}

/// Reads recorded while a package cache is captured. Per slot: which parts
/// were read while they still held their value from before the load, and
/// which parts the load has overwritten since (a later read of those does
/// not depend on the state before the load). A group end restores both.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
struct Reads {
    read: LookupMap<u32, u8>,
    written: LookupMap<u32, u8>,
    /// Slots assigned since the last [`Env::take_dirty`], in order.
    dirty: Vec<u32>,
    dirty_seen: LookupSet<u32>,
    /// Journal position, slot, and what was written of it when the journal
    /// saved it.
    saved: Vec<(usize, u32, u8)>,
    /// Slots into which the load copied text from before the load. A read of
    /// them depends on that text, however the load wrote them since.
    carrying: LookupSet<u32>,
    /// The slot whose meaning is being copied through. A read of its meaning
    /// counts as that copy.
    copying: Option<u32>,
    /// Slots assigned since the segment began, and of those, the ones whose
    /// text holds what the slot held then, where and how long.
    touched: LookupSet<u32>,
    splices: LookupMap<u32, (usize, usize)>,
    /// Read parts added by a `\csname` that become irrelevant if the whole
    /// meaning is overwritten before any other read (`\cs_gset:cpn`).
    moot: LookupMap<u32, u8>,
    /// Slots the load wrote whose content is a function of its leaves ([`Amb`]), and every
    /// slot such a function names. A plain read of the slot, or a write of a leaf, turns
    /// the function into exact reads of its leaves ([`Reads::settle`]).
    ambient: LookupMap<u32, Amb>,
    sources: LookupSet<u32>,
    guards: Vec<Guard>,
    /// Expressions whose exact reads were taken back: each read found a known integer,
    /// which the segment still depends on.
    kept: Vec<Rc<Expr>>,
    /// The slots the last expression scan read exactly ([`Env::scanned`]), the scan's
    /// number, and whether anything read one of them since: then the reads are no longer
    /// the scan's alone to take back.
    #[serde(skip)]
    pending: Vec<u32>,
    #[serde(skip)]
    serial: u64,
    #[serde(skip)]
    shared: bool,
    /// The reader accounts for the slot's [`Amb`] itself.
    #[serde(skip)]
    quiet: bool,
}

impl Reads {
    /// The content `amb` describes was looked at, or may change apart from it: from here on
    /// the segment depends on the exact leaves.
    fn settle(&mut self, amb: &Amb) {
        let mut sources = Vec::new();
        amb.sources(&mut sources);
        for (sym, kind) in sources {
            self.shared |= self.pending.contains(&sym.0);
            *self.read.entry(sym.0).or_insert(0) |= kind.bit();
        }
    }
}

/// The save stack at the point where paths split. Groups can open and close
/// on one path only, so the stack is copied rather than trailed.
#[derive(Clone)]
pub struct Fork {
    trail: usize,
    /// Its index in [`Env::floors`].
    level: usize,
    /// Whether several paths continued from it ([`Env::part`]).
    parted: bool,
    journal: Journal,
    marks: Rc<Vec<Mark>>,
    corr: Option<Rc<Corr>>,
}

impl Fork {
    /// The groups open where the paths split.
    pub fn depth(&self) -> usize {
        self.marks.len()
    }
}

/// What one path did to the bindings, relative to its [`Fork`].
#[derive(Clone)]
pub struct PathEnv {
    pub slots: Changed,
    journal: Journal,
    marks: Rc<Vec<Mark>>,
    corr: Option<Rc<Corr>>,
    /// The fewest groups the path was in since the fork. The groups up to
    /// there, and what they saved before the fork, belong to the fork.
    low: usize,
}

impl PathEnv {
    /// The names this path and `other` (from the same fork) bind differently:
    /// what one changed that the other does not hold.
    pub fn differing(&self, other: &PathEnv, walked: &Walked) -> Vec<Sym> {
        self.differing_over(other, |_| None, walked)
    }

    /// Like [`PathEnv::differing`], but a name only one path changed is
    /// compared with its binding at the fork (`base`, when known). If it was
    /// changed back to the same binding, it does not differ.
    pub fn differing_over(
        &self,
        other: &PathEnv,
        base: impl Fn(u32) -> Option<Option<Binding>>,
        walked: &Walked,
    ) -> Vec<Sym> {
        let differs = |k: u32, b: &Option<Binding>| base(k).is_none_or(|old| !same_slot(&old, b));
        // A subtree both maps share binds its names alike on both paths.
        (self.slots.pairs(&other.slots, |s| Changed::settled(s, u32::MAX), walked).into_iter())
            .filter(|&(k, mine, theirs)| match (mine, theirs) {
                (Some((m, _)), Some((t, _))) => !same_slot(m, t),
                (Some((b, _)), None) | (None, Some((b, _))) => differs(k, b),
                (None, None) => false,
            })
            .map(|(k, ..)| Sym(k))
            .collect()
    }

    /// Whether this path and `other` bind `sym` alike: neither changed it, or
    /// both changed it to the same binding.
    pub fn binds_alike(&self, other: &PathEnv, sym: Sym) -> bool {
        match (self.slots.get(&sym.0), other.slots.get(&sym.0)) {
            (Some((x, _)), Some((y, _))) => same_slot(x, y),
            (x, y) => x.is_none() && y.is_none(),
        }
    }

    /// How many slots the path changed since its fork.
    pub fn len(&self) -> usize {
        self.slots.size()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// The binding the path gave `sym` since its fork, if it changed it.
    pub fn changed(&self, sym: Sym) -> Option<&Binding> {
        self.slots.get(&sym.0).and_then(|(binding, _)| binding.as_ref())
    }

    /// The names the path changed since its fork.
    pub fn syms(&self, walked: &Walked) -> Vec<Sym> {
        self.slots.keys(walked).map(|&s| Sym(s)).collect()
    }
}

/// `names` sorted, each once. A loop writes the same few names over and
/// over: a bitset drops the repeats before the sort.
fn distinct_sorted(names: &[u32]) -> Vec<u32> {
    let mut out = if names.len() <= 64 {
        names.to_vec()
    } else {
        let mut seen = vec![0u64; names.iter().max().map_or(0, |&m| m as usize / 64 + 1)];
        names
            .iter()
            .copied()
            .filter(|&n| {
                let (word, bit) = (n as usize / 64, 1u64 << (n % 64));
                let new = seen[word] & bit == 0;
                seen[word] |= bit;
                new
            })
            .collect()
    };
    out.sort_unstable();
    out.dedup();
    out
}

impl Env {
    pub fn probe(&self) -> Vec<(&'static str, usize)> {
        crate::probe_sz!("e.slots" => self.slots, "e.level" => self.level, "e.journal" => self.journal, "e.marks" => self.marks, "e.trail" => self.trail, "e.trailed" => self.trailed, "e.lows" => self.lows, "e.floors" => self.floors, "e.changed" => self.changed, "e.recent" => self.recent, "e.stamp" => self.stamp, "e.aliases" => self.aliases, "e.read_log" => self.read_log, "e.reads" => self.reads)
    }
}

impl Env {
    pub fn with_capacity(n: usize) -> Self {
        Self {
            slots: crate::fixed::Chunked::filled(n, None),
            level: crate::fixed::Chunked::filled(n, 0),
            trailed: crate::fixed::Chunked::filled(n, NONE),
            stamp: crate::fixed::Stamps::filled(n),
            ..Default::default()
        }
    }

    #[inline]
    fn grow(&mut self, sym: Sym) {
        let n = sym.0 as usize + 1;
        if self.slots.len() < n {
            self.slots.resize(n, None);
            self.level.resize(n, 0);
            self.trailed.resize(n, NONE);
            self.stamp.resize(n);
        }
    }

    /// The primitive `sym` is a copy of, or `sym` itself. A copy that has
    /// since been given a non-primitive meaning counts as itself again.
    pub fn identity(&self, sym: Sym) -> Sym {
        match self.aliases.get(&sym) {
            Some(&original) if matches!(self.get(sym).map(|b| &b.meaning), Some(Meaning::Primitive(_))) => original,
            _ => sym,
        }
    }

    /// The open groups, outermost first: kind, where opened, and whether it is widened.
    pub fn group_stack(&self) -> impl DoubleEndedIterator<Item = (GroupKind, crate::engine::tex::Span, bool)> + '_ {
        self.marks.iter().map(|m| (m.kind, m.opened, m.many))
    }

    /// The names a join left paired ([`Corr`]), and how many arm rows pair them.
    pub fn paired(&self) -> (Vec<Sym>, usize) {
        self.corr.as_ref().map_or((Vec::new(), 0), |c| (c.syms.iter().map(|&s| Sym(s)).collect(), c.arms.len()))
    }

    /// The whole binding: its meaning and its value.
    #[inline]
    /// The binding an assignment keeps all but the value of: a read of its
    /// meaning, not of what it holds.
    pub fn kept(&self, sym: Sym) -> Option<&Binding> {
        self.log_meaning_read(sym);
        self.peek(sym.0)
    }

    pub fn get(&self, sym: Sym) -> Option<&Binding> {
        self.log_read(sym);
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Meaning);
            self.note_read(sym, ReadKind::Value);
        }
        self.slot(sym)
    }

    /// The binding, without recording a read.
    #[inline]
    /// The bindings of the first `n` names, sharing their chunks, read as
    /// [`Env::get`] reads each.
    pub fn bindings(&self, n: usize) -> crate::fixed::Chunked<Option<Binding>> {
        if self.tracking.get() || self.logging.get() {
            (0..n as u32).for_each(|i| _ = self.get(Sym(i)));
        }
        self.slots.clone()
    }

    pub fn slot(&self, sym: Sym) -> Option<&Binding> {
        self.memo_read(sym.0, true);
        self.peek(sym.0)
    }

    /// The slot `sym` holds, not logged as a read.
    pub fn peek(&self, sym: u32) -> Option<&Binding> {
        self.slots.get(sym as usize).and_then(|s| s.as_ref())
    }

    /// Every name that currently has a binding. Used to sweep a family of
    /// names, such as all registers when a register number is unknown.
    pub fn bound(&self) -> Vec<Sym> {
        self.memo_odd();
        self.slots.iter().enumerate().filter(|(_, s)| s.is_some()).map(|(i, _)| Sym(i as u32)).collect()
    }

    /// A register's value as a number, without recording a read.
    pub fn slot_value(&self, sym: Sym) -> Option<i64> {
        self.slot(sym).and_then(|b| b.value.as_int())
    }

    /// Record that `kind` of `sym` was read, unless the load being captured
    /// has overwritten that part already.
    #[cold]
    pub fn note_read(&self, sym: Sym, kind: ReadKind) {
        if !self.tracking.get() {
            return;
        }
        let mut reads = self.reads.borrow_mut();
        reads.moot.remove(&sym.0);
        reads.shared |= kind == ReadKind::Value && reads.pending.contains(&sym.0);
        if !reads.quiet && !reads.ambient.is_empty() {
            let looked = match reads.ambient.get(&sym.0) {
                Some(Amb::Int(_)) => kind == ReadKind::Value,
                Some(Amb::Text(_)) => matches!(kind, ReadKind::Meaning | ReadKind::Kept | ReadKind::Copy),
                None => false,
            };
            if looked && let Some(amb) = reads.ambient.remove(&sym.0) {
                reads.settle(&amb);
            }
        }
        let kind = match kind {
            ReadKind::Meaning if reads.copying == Some(sym.0) => ReadKind::Copy,
            kind => kind,
        };
        let written = reads.written.get(&sym.0).copied().unwrap_or(0);
        let covered = match kind {
            ReadKind::Kept | ReadKind::Defined | ReadKind::Skip | ReadKind::Defs | ReadKind::Shape => {
                written & ReadKind::Meaning.bit() != 0
            }
            ReadKind::Meaning | ReadKind::Copy => {
                written & ReadKind::Meaning.bit() != 0 && !reads.carrying.contains(&sym.0)
            }
            kind => written & kind.bit() != 0,
        };
        if !covered {
            *reads.read.entry(sym.0).or_insert(0) |= kind.bit();
        }
    }

    fn drop_moot(&self, sym: u32) {
        if !self.tracking.get() {
            return;
        }
        let mut reads = self.reads.borrow_mut();
        let Some(bits) = reads.moot.remove(&sym) else { return };
        if let Some(read) = reads.read.get_mut(&sym) {
            *read &= !bits;
            if *read == 0 {
                reads.read.remove(&sym);
            }
        }
    }

    /// The parts of `sym` read so far, for [`Env::moot_reads`].
    pub fn read_parts(&self, sym: Sym) -> u8 {
        if !self.tracking.get() {
            return 0;
        }
        self.reads.borrow().read.get(&sym.0).copied().unwrap_or(0)
    }

    /// The parts of `sym` read since `before` ([`Env::read_parts`]) are
    /// dropped if the whole meaning is overwritten for good (globally, or
    /// outside every group) before anything reads `sym` again.
    pub fn moot_reads(&self, sym: Sym, before: u8) {
        if !self.tracking.get() || self.paths > 0 {
            return;
        }
        let mut reads = self.reads.borrow_mut();
        let added = reads.read.get(&sym.0).copied().unwrap_or(0) & !before;
        if added != 0 {
            reads.moot.insert(sym.0, added);
        }
    }

    fn note_write(&self, sym: u32, bits: u8) {
        if !self.tracking.get() {
            return;
        }
        let mut reads = self.reads.borrow_mut();
        if reads.dirty_seen.insert(sym) {
            reads.dirty.push(sym);
        }
        // A path through an undecided conditional is rewound before the next,
        // so its writes are no reason to stop recording reads.
        if self.paths == 0 {
            *reads.written.entry(sym).or_insert(0) |= bits;
        }
    }

    /// The reads recorded since the last call. Starts a new record of them.
    /// What has been written stays known.
    pub fn take_read_marks(&self) -> Vec<(Sym, u8)> {
        let mut reads = self.reads.borrow_mut();
        reads.moot.clear();
        let read = std::mem::take(&mut reads.read);
        drop(reads);
        let mut out: Vec<(Sym, u8)> = read.into_iter().map(|(sym, bits)| (Sym(sym), bits)).collect();
        out.sort_by_key(|(sym, _)| *sym);
        out
    }

    /// The slots assigned since the last call, in order, with the parts of
    /// each written since recording began.
    pub fn take_dirty(&self) -> Vec<(Sym, u8)> {
        let mut reads = self.reads.borrow_mut();
        reads.dirty_seen.clear();
        let dirty = std::mem::take(&mut reads.dirty);
        dirty.into_iter().map(|sym| (Sym(sym), reads.written.get(&sym).copied().unwrap_or(0))).collect()
    }

    /// Whether the load being recorded has assigned the meaning of `sym`,
    /// as far as a replay can tell: not on a path through an undecided
    /// conditional, whose writes are not recorded.
    pub fn meaning_written(&self, sym: Sym) -> bool {
        self.paths > 0
            || self.reads.borrow().written.get(&sym.0).is_some_and(|bits| bits & ReadKind::Meaning.bit() != 0)
    }

    /// Record that the parts `bits` of `sym` were written, as a replayed
    /// load did.
    pub fn mark_written(&self, sym: Sym, bits: u8) {
        *self.reads.borrow_mut().written.entry(sym.0).or_insert(0) |= bits;
    }

    /// While `sym`'s meaning is copied through, a read of it is a
    /// [`ReadKind::Copy`].
    pub fn set_copying(&self, sym: Option<Sym>) {
        self.reads.borrow_mut().copying = sym.map(|s| s.0);
    }

    /// Whether reading `sym`'s meaning depends on the state before the load:
    /// it was not written since, or it holds text copied from that state.
    pub fn meaning_fresh(&self, sym: Sym) -> bool {
        let reads = self.reads.borrow();
        !self.meaning_written(sym) || reads.carrying.contains(&sym.0)
    }

    /// Records that `sym` now holds text from before the load.
    pub fn note_carrying(&self, sym: Sym) {
        if self.tracking.get() {
            self.reads.borrow_mut().carrying.insert(sym.0);
        }
    }

    pub fn carries(&self, sym: Sym) -> bool {
        self.reads.borrow().carrying.contains(&sym.0)
    }

    pub fn carrying(&self) -> Vec<Sym> {
        let mut out: Vec<Sym> = self.reads.borrow().carrying.iter().map(|&s| Sym(s)).collect();
        out.sort();
        out
    }

    /// Where the text `sym` held at the segment's start sits in its current
    /// text, if it does: `sym` was not assigned since (at 0, the whole `len`),
    /// or only by copying it.
    pub fn splice_of(&self, sym: Sym, len: usize) -> Option<(usize, usize)> {
        let reads = self.reads.borrow();
        match reads.splices.get(&sym.0) {
            Some(&splice) => Some(splice),
            None if !reads.touched.contains(&sym.0) => Some((0, len)),
            None => None,
        }
    }

    /// Where a copy put that text, if one did and nothing was assigned since.
    pub fn splice(&self, sym: Sym) -> Option<(usize, usize)> {
        self.reads.borrow().splices.get(&sym.0).copied()
    }

    pub fn set_splice(&self, sym: Sym, at: usize, len: usize) {
        if self.tracking.get() {
            self.reads.borrow_mut().splices.insert(sym.0, (at, len));
        }
    }

    /// Whether a load is recorded on one path: only there a value is kept as a function of
    /// the load's leaves, as only there its writes are recorded ([`Env::note_write`]).
    pub fn symbolic(&self) -> bool {
        self.tracking.get() && self.paths == 0
    }

    /// What the content of `sym` is as a function of the load's leaves, if the load made it one.
    pub fn amb(&self, sym: Sym) -> Option<Amb> {
        self.reads.borrow().ambient.get(&sym.0).cloned()
    }

    /// The load left `sym` holding `amb`, if `holds` (said after the write, which retires
    /// what the slot held). Else, or where a rewind may undo the write, or where the load
    /// has written a slot `amb` names since it read it, the content is not kept as a
    /// function: everything `amb` names is read exactly.
    pub fn set_amb(&self, sym: Sym, amb: Amb, holds: bool) {
        if !self.tracking.get() {
            return;
        }
        let symbolic = self.symbolic();
        let mut reads = self.reads.borrow_mut();
        let mut sources = Vec::new();
        amb.sources(&mut sources);
        let written = |reads: &Reads, (s, kind): &(Sym, ReadKind)| {
            reads.written.get(&s.0).copied().unwrap_or(0) & kind.bit() != 0
        };
        if !holds || !symbolic || sources.iter().any(|source| written(&reads, source)) {
            return reads.settle(&amb);
        }
        reads.sources.extend(sources.into_iter().map(|(s, _)| s.0));
        reads.ambient.insert(sym.0, amb);
    }

    /// The meaning read of `sym` since its parts read were `before` was a copy of its text
    /// into a hole: it counts as [`ReadKind::Copy`], as a text copied through does.
    pub fn copied_not_read(&self, sym: Sym, before: u8) {
        if !self.tracking.get() {
            return;
        }
        let mut reads = self.reads.borrow_mut();
        let read = reads.read.entry(sym.0).or_insert(0);
        *read = *read & (before | !ReadKind::Meaning.bit()) | ReadKind::Copy.bit();
    }

    /// A served segment left `sym` holding `amb`: the load recorded from here on holds it so
    /// too, unless a later served segment wrote a slot it names (which that one keyed on).
    pub fn restore_amb(&self, sym: Sym, amb: Amb) {
        let mut reads = self.reads.borrow_mut();
        let mut sources = Vec::new();
        amb.sources(&mut sources);
        if sources.iter().all(|(s, kind)| reads.written.get(&s.0).copied().unwrap_or(0) & kind.bit() == 0) {
            reads.sources.extend(sources.into_iter().map(|(s, _)| s.0));
            reads.ambient.insert(sym.0, amb);
        }
    }

    /// What `amb` names is read exactly: its consumer could not keep it as a function.
    pub fn settle_amb(&self, amb: &Amb) {
        if self.tracking.get() {
            self.reads.borrow_mut().settle(amb);
        }
    }

    /// Reads of slots with an [`Amb`] while `on` are the reader's to account for.
    pub fn quiet(&self, on: bool) {
        self.reads.borrow_mut().quiet = on;
    }

    /// Before a read of the integer `sym` holds: what it is as an expression. The load's own
    /// function of its leaves, or the slot as a leaf while the load has not written its
    /// value. `None` where no load is recorded or a rewind may undo the read. The read
    /// itself is logged as any read, then [`Env::scanned`] completes this.
    pub fn scanning(&self, sym: Sym) -> Option<Scanned> {
        if !self.symbolic() {
            return None;
        }
        let reads = self.reads.borrow();
        let bit = ReadKind::Value.bit();
        let expr = match reads.ambient.get(&sym.0) {
            Some(Amb::Int(expr)) => expr.clone(),
            Some(Amb::Text(_)) => return None,
            None if reads.written.get(&sym.0).copied().unwrap_or(0) & bit == 0 => Rc::new(Expr::Leaf(sym)),
            None => return None,
        };
        let mut leaves = Vec::new();
        expr.leaves(&mut leaves);
        // The exact reads the read is about to add: those are its own to take back.
        let undo = leaves.iter().filter(|s| reads.read.get(&s.0).copied().unwrap_or(0) & bit == 0);
        let undo = undo.map(|s| (s.0, bit)).collect();
        Some(Scanned { expr, value: 0, undo, serial: 0 })
    }

    /// After the read [`Env::scanning`] prepared gave `value`: the plain read settled the
    /// slot's function, which stands again for the consumer to keep or not.
    pub fn scanned(&self, mut scanned: Scanned, sym: Sym, value: i64) -> Scanned {
        let mut reads = self.reads.borrow_mut();
        if !matches!(*scanned.expr, Expr::Leaf(_)) {
            reads.ambient.insert(sym.0, Amb::Int(scanned.expr.clone()));
        }
        // From here on a read of the slot or of a leaf is someone else's.
        reads.serial += 1;
        reads.pending = scanned.undo.iter().map(|(s, _)| *s).chain([sym.0]).collect();
        reads.shared = false;
        (scanned.value, scanned.serial) = (value, reads.serial);
        scanned
    }

    /// The consumer of `scanned` keeps its expression: the exact reads its scan logged are
    /// taken back. Only when no other scan came and nothing read those slots since, else
    /// they stay and `false` says the expression is not worth keeping.
    pub fn retract(&self, scanned: &Scanned) -> bool {
        let mut reads = self.reads.borrow_mut();
        if !self.symbolic() || reads.serial != scanned.serial || reads.shared {
            return false;
        }
        reads.pending.clear();
        reads.kept.push(scanned.expr.clone());
        for (sym, bits) in &scanned.undo {
            if let Some(read) = reads.read.get_mut(sym) {
                *read &= !bits;
                if *read == 0 {
                    reads.read.remove(sym);
                }
            }
        }
        true
    }

    /// A test the load decided on its leaves.
    pub fn guard(&self, guard: Guard) {
        self.reads.borrow_mut().guards.push(guard);
    }

    /// The tests decided since the last call, and the expressions kept in place of exact
    /// reads since then.
    pub fn take_guards(&self) -> (Vec<Guard>, Vec<Rc<Expr>>) {
        let mut reads = self.reads.borrow_mut();
        (std::mem::take(&mut reads.guards), std::mem::take(&mut reads.kept))
    }

    /// A segment begins: nothing is assigned since.
    pub fn begin_segment(&self) {
        let mut reads = self.reads.borrow_mut();
        reads.touched.clear();
        reads.splices.clear();
    }

    /// Stop recording and forget what was recorded.
    pub fn end_tracking(&self) {
        self.tracking.set(false);
        *self.reads.borrow_mut() = Reads::default();
    }

    /// Start or stop recording which slots are read.
    /// Returns whether it was recording before.
    #[inline]
    fn log_read(&self, sym: Sym) {
        self.memo_read(sym.0, true);
        if self.logging.get() {
            self.log_name(sym);
        }
    }

    /// Log every name read from now on (`on`), or stop and forget the log.
    pub fn log_reads(&self, on: bool) {
        self.logging.set(on);
        self.log_full.set(false);
        if !on {
            self.read_span(self.read_log.borrow().len());
            self.read_log.borrow_mut().clear();
        }
    }

    /// Logs a read of `sym`'s meaning, or of all of it (`value`).
    #[inline]
    fn memo_read(&self, sym: u32, value: bool) {
        self.touch(sym);
        if self.memo_on.get() {
            let mut memo = self.memo.borrow_mut();
            if memo.reads.len() >= READ_LOG {
                memo.full = true;
                memo.reads.clear();
                memo.parts.clear();
            } else if !memo.full {
                memo.reads.push(sym | u32::from(value) << 31);
            }
        }
    }

    /// Notes `sym` as read or written now. Every read goes through [`Env::memo_read`] and
    /// every write through [`Env::log`], a group end's restores too.
    #[inline]
    fn touch(&self, sym: u32) {
        self.touched.note(sym as usize);
    }

    /// Whether reads go to the read log of running records.
    pub fn logging(&self) -> bool {
        self.logging.get()
    }

    /// A kept call effect is applied: the logs take the reads its run made,
    /// each log what that run gave it, so records around it see the same.
    pub fn replay_reads(&self, memo: &[(u32, bool)], logged: &[Sym]) {
        for &(sym, value) in memo {
            self.memo_read(sym, value);
            if self.tracking.get() {
                self.note_read(Sym(sym), ReadKind::Meaning);
                if value {
                    self.note_read(Sym(sym), ReadKind::Value);
                }
            }
        }
        if self.logging.get() {
            logged.iter().for_each(|&sym| self.log_name(sym));
        }
    }

    /// Logs a read of `sym` with the assignment its binding came from.
    fn log_name(&self, sym: Sym) {
        let mut log = self.read_log.borrow_mut();
        if log.len() >= READ_LOG {
            self.read_span(READ_LOG);
            log.clear();
            self.logging.set(false);
            self.log_full.set(true);
        } else {
            log.push((sym.0, self.stamp_of(sym)));
        }
    }

    /// A change no assignment replays: a running record keeps no effect.
    pub fn memo_odd(&self) {
        if self.memo_on.get() {
            self.memo.borrow_mut().odd += 1;
        }
    }

    /// Whether the log ran over: no open record keeps its effect.
    pub fn memo_full(&self) -> bool {
        self.memo.borrow().full
    }

    pub fn memo_odds(&self) -> u64 {
        self.memo.borrow().odd
    }

    /// A record whose effect may be kept begins: the log runs, and what
    /// slots hold before their next write is logged again.
    pub fn memo_open(&mut self) -> MemoMark {
        self.memo_on.set(true);
        let memo = self.memo.get_mut();
        memo.floor = memo.pre.len();
        MemoMark {
            reads: memo.reads.len(),
            pre: memo.pre.len(),
            globals: memo.globals.len(),
            locals: memo.locals.len(),
        }
    }

    /// The innermost open record begins at `floor` again; `None` stops the log.
    /// Whether a call is being recorded for a kept effect.
    pub fn memo_recording(&self) -> bool {
        self.memo_on.get()
    }

    pub fn memo_floor(&mut self, floor: Option<MemoMark>) {
        match floor {
            Some(m) => self.memo.get_mut().floor = m.pre,
            None => {
                self.memo_on.set(false);
                let odd = self.memo.get_mut().odd;
                *self.memo.get_mut() = MemoLog { odd, ..MemoLog::default() };
            }
        }
    }

    /// Since `from`: the names read, each once (`None` when the log ran
    /// over), what each written slot held before, and the names written
    /// globally and locally.
    #[allow(clippy::type_complexity)]
    pub fn memo_since(
        &self,
        from: MemoMark,
    ) -> Option<(Vec<(u32, bool)>, LookupMap<u32, Option<Binding>>, [Vec<u32>; 2])> {
        let mut memo = self.memo.borrow_mut();
        if memo.full {
            return None;
        }
        // Each name once: read whole if any read took more than its meaning.
        let end = memo.reads.len();
        memo.reads.get(from.reads..)?;
        // The log's own reads and the parts ended records took, a part that
        // overlaps the last one skipped: the log has its reads.
        let mut pieces: Vec<&[u32]> = Vec::new();
        let mut at = from.reads;
        for (&start, (stop, names)) in memo.parts.range(from.reads..) {
            if start < at {
                continue;
            }
            pieces.extend([&memo.reads[at..start], &names[..]]);
            at = *stop;
        }
        pieces.push(&memo.reads[at..end]);
        let logged = self.distinct.of(pieces.into_iter().flatten().copied());
        let reads: Vec<(u32, bool)> = logged.iter().map(|r| (r & !(1 << 31), r >> 31 == 1)).collect();
        // The parts inside this one are in it.
        drop(memo.parts.split_off(&from.reads));
        memo.parts.insert(from.reads, (end, logged.into()));
        let mut pre = LookupMap::default();
        for (sym, b) in memo.pre.get(from.pre..).unwrap_or_default() {
            pre.entry(*sym).or_insert_with(|| b.clone());
        }
        let names = |v: &[u32]| {
            let mut v = v.to_vec();
            v.sort_unstable();
            v.dedup();
            v
        };
        let globals = names(memo.globals.get(from.globals..).unwrap_or_default());
        Some((reads, pre, [globals, names(memo.locals.get(from.locals..).unwrap_or_default())]))
    }

    /// The group level a slot's binding was made at (0: global).
    pub fn level_of(&self, sym: u32) -> u32 {
        self.level.get(sym as usize).copied().unwrap_or(0)
    }

    /// `\aftergroup` tokens the innermost group holds.
    pub fn after_len(&self) -> usize {
        self.marks.last().map_or(0, |m| m.after.len())
    }

    /// A read of `sym`'s meaning only, for the logs.
    #[inline]
    fn log_meaning_read(&self, sym: Sym) {
        self.memo_read(sym.0, false);
        if self.logging.get() {
            self.log_name(sym);
        }
    }

    /// Notes that the read log, `len` long, is cleared now.
    fn read_span(&self, len: usize) {
        if len > 0 {
            self.read_spans.borrow_mut().push((self.touched.now.get(), len, self.read_asked.replace(0)));
        }
    }

    /// Whether a run whose read log is `longer` at step `at` than this run's was is told the
    /// same by it from there on. It is until one of them runs over ([`READ_LOG`]), and
    /// after both have. The longer one runs over that much sooner: alike if this run's log
    /// never came that near the bound, or was last asked that long before it ran over.
    pub fn read_log_alike(&self, at: u64, longer: usize) -> bool {
        let spans = self.read_spans.borrow();
        let (len, asked) = spans.iter().find(|s| s.0 >= at).map_or((self.read_log.borrow().len(), 0), |s| (s.1, s.2));
        len + longer < READ_LOG || (len >= READ_LOG && asked + longer < READ_LOG)
    }

    pub fn read_log_len(&self) -> usize {
        self.read_log.borrow().len()
    }

    /// The names read since the log was `from` long, each once, sorted;
    /// `None` when the log does not reach back that far.
    pub fn reads_since(&self, from: usize) -> Option<Vec<Sym>> {
        if self.log_full.get() {
            return None;
        }
        let log = self.read_log.borrow();
        self.read_asked.set(log.len());
        let syms = self.distinct.of((from <= log.len()).then(|| log.iter_from(from))?.map(|r| r.0));
        Some(syms.into_iter().map(Sym).collect())
    }

    /// Of `syms` (sorted), those read since the log was `from` long while
    /// they held a binding from assignment `at` or before: the run read them
    /// as it met them. `None` when the log does not reach back that far.
    pub fn read_before(&self, from: usize, at: u64, syms: &[Sym]) -> Option<Vec<Sym>> {
        if syms.is_empty() || self.log_full.get() {
            return syms.is_empty().then(Vec::new);
        }
        let log = self.read_log.borrow();
        self.read_asked.set(log.len());
        let mut met: Vec<Sym> = ((from <= log.len()).then(|| log.iter_from(from))?)
            .filter(|&&(sym, stamp)| stamp <= at && syms.binary_search(&Sym(sym)).is_ok())
            .map(|&(sym, _)| Sym(sym))
            .collect();
        met.sort_unstable();
        met.dedup();
        Some(met)
    }

    pub fn track_reads(&self, on: bool) -> bool {
        self.tracking.replace(on)
    }

    #[inline]
    pub fn meaning(&self, sym: Sym) -> Meaning {
        self.log_meaning_read(sym);
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Meaning);
        }
        match self.peek(sym.0) {
            Some(b) => b.meaning.clone(),
            None => Meaning::Undefined,
        }
    }

    /// The meaning, read by the analysis watching the run, not by TeX code:
    /// kept call effects depend on it, records of the code around it do not.
    pub fn observed_meaning(&self, sym: Sym) -> Meaning {
        self.memo_read(sym.0, false);
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Meaning);
        }
        self.peek(sym.0).map_or(Meaning::Undefined, |b| b.meaning.clone())
    }

    /// The meaning, for a reader that only distinguishes macros from other
    /// meanings. A macro's text does not count as read.
    pub fn shape(&self, sym: Sym) -> Meaning {
        self.log_meaning_read(sym);
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Shape);
        }
        self.peek(sym.0).map_or(Meaning::Undefined, |b| b.meaning.clone())
    }

    /// Whether `sym` means an `\outer` macro. tex.web § 336 checks this for
    /// every token read — engine bookkeeping, not a read of the meaning, so
    /// nothing is recorded or cloned.
    pub fn is_outer(&self, sym: Sym) -> bool {
        self.memo_read(sym.0, false);
        self.peek(sym.0).is_some_and(|b| matches!(&b.meaning, Meaning::Macro(m) if m.outer))
    }

    /// The primitive, as text being skipped sees it: only whether it is a
    /// conditional, `\else`, `\or` or `\fi` counts as read.
    pub fn skip_prim(&self, sym: Sym) -> Option<crate::engine::builtins::Primitive> {
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Skip);
        }
        self.slot(sym).and_then(|b| b.meaning.prim())
    }

    /// The name's definition sites, to link a dependency-graph edge to.
    /// Whatever they are, a package cache links the edge again on replay,
    /// so this reads nothing.
    pub fn link_defs(&self, sym: Sym) -> Defs {
        self.slot(sym).map(|b| Defs::from_slice(&b.defs)).unwrap_or_default()
    }

    /// The name's definition sites, when what is done depends on whether
    /// there are any.
    pub fn defs(&self, sym: Sym) -> Defs {
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Defs);
        }
        self.slot(sym).map(|b| Defs::from_slice(&b.defs)).unwrap_or_default()
    }

    pub fn is_defined(&self, sym: Sym) -> bool {
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Defined);
        }
        !matches!(self.slot(sym).map(|b| &b.meaning), None | Some(Meaning::Undefined))
    }

    /// Whether `sym` has a meaning on every path. If a join left it undefined
    /// on one path, or its meaning is beyond every set, it is only maybe defined.
    pub fn surely_defined(&self, sym: Sym) -> bool {
        self.is_defined(sym)
            && self.slot(sym).is_none_or(|b| match b.may.as_deref() {
                Some(m) => !m.contains(&Meaning::Undefined),
                None => !matches!(b.meaning, Meaning::Unknown),
            })
    }

    /// Number of slots that hold a meaning, which is what a format cache
    /// stores, so a rebuild and a cache hit report the same number.
    pub fn meaning_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| slot.as_ref().is_some_and(|binding| !matches!(binding.meaning, Meaning::Undefined)))
            .count()
    }

    /// Whether a path through an undecided conditional is running.
    pub fn forking(&self) -> bool {
        self.paths > 0
    }

    pub fn journal_len(&self) -> usize {
        self.journal.len()
    }

    /// Trail, journal, recent names and open forks, for the memory report.
    pub fn sizes(&self) -> [usize; 4] {
        [self.trail.len(), self.journal.len(), self.recent.len(), self.floors.len()]
    }

    /// Group depth.  0 is the outer, "global" level.
    pub fn depth(&self) -> usize {
        self.marks.len()
    }

    /// The lowest group level TeX could report, and whether it may be higher
    /// by any number because a group stands for many.
    pub fn group_level(&self) -> usize {
        self.marks.iter().filter(|m| !m.many).count()
    }

    /// Whether `other` stands in the same groups, which saved alike since both had made `since`
    /// (assignments, saves): before, the runs were the same. With `slots`, also whether it
    /// binds every name alike.
    pub fn alike(&self, other: &Env, since: (u64, usize), slots: bool) -> bool {
        self.marks.len() == other.marks.len()
            && self.journal_len() == other.journal_len()
            && (!slots || self.slots.alike(&other.slots, same_slot))
            && self.saves(since.1) == other.saves(since.1)
    }

    /// Binds `slots` as `from` does, as if assigned now: the names a run's rest never
    /// touched keep what the run before it left them ([`Env::differing`]).
    pub fn carry(&mut self, from: &Env, slots: &[u32]) {
        for &sym in slots {
            let i = sym as usize;
            self.grow(Sym(sym));
            self.assignments += 1;
            self.stamp.set(i, self.assignments);
            self.level[i] = from.level_of(sym);
            self.slots[i] = from.peek(sym).cloned();
            match from.aliases.get(&Sym(sym)) {
                Some(&alias) => self.aliases.insert(Sym(sym), alias),
                None => self.aliases.remove(&Sym(sym)),
            };
        }
    }

    /// The groups and the saves made since the first `from`, as text with every token's place.
    fn saves(&self, from: usize) -> String {
        let chain: Vec<_> = std::iter::successors(self.journal.frozen.as_ref(), |(c, _)| c.parent.as_ref()).collect();
        let entries = chain.iter().rev().flat_map(|(c, n)| &c.entries[..*n]).chain(&self.journal.top);
        format!("{:?}{:?}", self.marks, entries.skip(from).collect::<Vec<_>>())
    }

    /// The slots `other` binds differently, in order.
    pub fn differing(&self, other: &Env) -> Vec<u32> {
        let same = |a: Option<&Option<Binding>>, b: Option<&Option<Binding>>| match (a, b) {
            (Some(a), Some(b)) => same_slot(a, b),
            (a, b) => a.is_none_or(Option::is_none) && b.is_none_or(Option::is_none),
        };
        self.slots.differing(&other.slots, same).into_iter().map(|i| i as u32).collect()
    }

    /// [`Env::saves`] and the bindings written since `since`, less the names `but` (sorted):
    /// what a later reader of this state may meet that the runs did not share.
    pub fn trace(&self, since: (u64, usize), but: &[u32]) -> String {
        let kept = |i: &usize| but.binary_search(&(*i as u32)).is_err();
        let written: Vec<_> = self.stamp.after(since.0).filter(kept).map(|i| &self.slots[i]).collect();
        self.saves(since.1) + &format!("{written:?}")
    }

    /// Whether some open group stands for any number of groups.
    pub fn many_groups(&self) -> bool {
        self.marks.iter().any(|m| m.many)
    }

    /// Whether the innermost group stands for any number of groups.
    pub fn top_many(&self) -> bool {
        self.marks.last().is_some_and(|m| m.many)
    }

    /// A closer meets an innermost group that stands for any number of groups.
    /// With `more`, one of them closes and the entry stays, and what they saved
    /// becomes unknown, since the closing one may have saved it. Otherwise
    /// there are none: the entry goes, and its saves move to the group below.
    pub fn unfold(&mut self, more: bool, catcodes: &mut CatcodeTable, typed: &Typed) {
        let Some(mark) = self.marks.last().cloned() else { return };
        self.memo_odd();
        self.effective += 1;
        let mut journal = self.journal.clone();
        let mut saved = Vec::new();
        while journal.len() > mark.journal {
            saved.push(journal.pop().expect("length checked"));
        }
        if !more {
            Rc::make_mut(&mut self.marks).pop();
            self.lower();
            self.clamp_levels(&saved);
            return;
        }
        for entry in saved {
            match entry {
                Entry::Meaning { sym, .. } => self.widen_in_place(Sym(sym), typed),
                Entry::Catcode { character, .. } => catcodes.set_unknown(character),
            }
        }
        self.group_ids += 1;
        let one = Mark { journal: self.journal.len(), many: false, id: self.group_ids, ..mark };
        crate::push_shared(&mut self.marks, one);
    }

    /// Groups `top..=to` (1-based depths) became one at `top` without ending
    /// (a widened stack): a write logged in them is undone only when that
    /// one ends, and one above moves down with its group. Applied to every
    /// path's writes, which only keeps more of them.
    pub fn merge_recent(&mut self, top: usize, to: usize) {
        let (top, to) = (top as u32, to as u32);
        if to > top {
            // Monotone in the depth, so a folded log stays one ([`Env::fold_recent`]).
            for w in &mut self.recent {
                w.depth = match w.depth {
                    d if d <= top => d,
                    d if d <= to => top,
                    d => d - (to - top),
                };
            }
        }
    }

    /// The slots in `saved`, written deeper than the groups now open, count as
    /// written in the innermost group, so a group opened later saves them.
    fn clamp_levels(&mut self, saved: &[Entry]) {
        let level = self.marks.len() as u32;
        for entry in saved {
            if let Entry::Meaning { sym, .. } = *entry
                && self.level[sym as usize] > level
            {
                let i = sym as usize;
                self.log(i);
                self.level[i] = level;
                self.changed(i);
            }
        }
    }

    /// Collapses the groups from `from` up into one that stands for any number
    /// of them, if all are plain and alike. Returns whether it did.
    pub fn many_since(&mut self, from: usize) -> bool {
        let Some(first) = self.marks.get(from).cloned() else { return true };
        let open = self.marks.len();
        let alike = |m: &Mark| {
            m.kind == first.kind
                && m.nest.is_none()
                && m.after.iter().map(|t| t.tok).eq(first.after.iter().map(|t| t.tok))
        };
        if !self.marks[from..].iter().all(alike) {
            return false;
        }
        self.memo_odd();
        self.effective += 1;
        let mut journal = self.journal.clone();
        let mut saved = Vec::new();
        while journal.len() > first.journal {
            saved.push(journal.pop().expect("length checked"));
        }
        let marks = Rc::make_mut(&mut self.marks);
        marks[from].many = true;
        marks.truncate(from + 1);
        self.lower();
        self.clamp_levels(&saved);
        self.merge_recent(from + 1, open);
        true
    }

    /// How many open groups were opened up to group `id`.
    pub fn depth_upto(&self, id: u64) -> usize {
        self.marks.iter().take_while(|m| m.id <= id).count()
    }

    /// The innermost group stands for any number of its kind.
    pub fn set_top_many(&mut self) {
        if let Some(mark) = Rc::make_mut(&mut self.marks).last_mut() {
            mark.many = true;
        }
    }

    /// The newest group id given out, and the innermost open group's.
    pub fn group_ids(&self) -> (u64, Option<u64>) {
        (self.group_ids, self.marks.last().map(|m| m.id))
    }

    /// A fresh group id, for a group made by a widening.
    pub fn next_group_id(&mut self) -> u64 {
        self.group_ids += 1;
        self.group_ids
    }

    /// Local or global assignment.  A global one survives every enclosing
    /// group, exactly as TeX's does.
    pub fn set(&mut self, sym: Sym, binding: Binding, global: bool) {
        self.grow(sym);
        self.set_tracked(sym, binding, global, ReadKind::Meaning.bit() | ReadKind::Value.bit());
    }

    fn set_tracked(&mut self, sym: Sym, mut binding: Binding, global: bool, writes: u8) {
        let i = sym.0 as usize;
        self.log(i);
        self.assignments += 1;
        if !self.slots.get(i).and_then(Option::as_ref).is_some_and(|b| {
            b.meaning == binding.meaning && same_value(&b.value, &binding.value) && b.may == binding.may
        }) {
            self.effective += 1;
            self.slot_changes += 1;
        }
        self.log_write(sym.0, if global { 0 } else { self.marks.len() as u32 });
        let stamp = self.stamp.set(i, self.assignments);
        if (global || self.marks.is_empty()) && writes & ReadKind::Meaning.bit() != 0 && self.paths == 0 {
            self.drop_moot(sym.0);
        }
        if global {
            if self.memo_on.get() {
                self.memo.get_mut().globals.push(sym.0);
            }
            self.note_write(sym.0, writes);
            binding.global = true;
            self.level[i] = 0;
            self.slots[i] = Some(binding);
            self.changed(i);
            return;
        }
        if self.memo_on.get() {
            self.memo.get_mut().locals.push(sym.0);
        }
        let current = self.marks.len() as u32;
        if current > 0 && self.level[i] != current {
            let old = self.slots[i].clone();
            if self.tracking.get() {
                let mut reads = self.reads.borrow_mut();
                let written = reads.written.get(&sym.0).copied().unwrap_or(0);
                reads.saved.push((self.journal.len(), sym.0, written));
            }
            let alias = self.aliases.get(&sym).copied();
            self.journal.push(Entry::Meaning { sym: sym.0, old, level: self.level[i], alias, stamp });
        }
        self.note_write(sym.0, writes);
        self.level[i] = current;
        self.slots[i] = Some(binding);
        self.changed(i);
    }

    pub fn value(&self, sym: Sym) -> Value {
        self.log_read(sym);
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Value);
        }
        match self.slot(sym).map(|b| &b.value) {
            None | Some(Value::Num { .. } | Value::OneOf(_)) => Value::Unknown,
            Some(value) => value.clone(),
        }
    }

    /// The abstract number a register holds (whether it is a dimension,
    /// and its interval): exact when known, `None` when not a number.
    pub fn num(&self, sym: Sym) -> Option<(crate::engine::value::Quantity, crate::engine::value::Num)> {
        self.log_read(sym);
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Value);
        }
        self.slot(sym)?.value.num()
    }

    /// Records what a test taught a path about a register: its value is
    /// `value`. Not an assignment, so the save stack is not involved. `keep`
    /// says which arms of [`Env::arms`] may hold it.
    pub fn refine_value(&mut self, sym: Sym, value: Value, keep: &[bool]) {
        self.memo_odd();
        self.effective += 1;
        let i = sym.0 as usize;
        let Some(Some(binding)) = self.slots.get(i) else { return };
        let binding = Binding { value, ..binding.clone() };
        self.correlate(i, keep);
        self.log(i);
        self.slots[i] = Some(binding);
        self.changed(i);
    }

    /// Assigns a value and keeps the rest of the binding. It reads only the
    /// kept parts, not the old value (an empty slot gets a new register slot's parts).
    pub fn set_value(&mut self, sym: Sym, value: Value, global: bool) {
        self.effective += 1;
        self.slot_changes += 1;
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Kept);
        }
        self.memo_read(sym.0, false);
        let mut binding = match self.peek(sym.0) {
            Some(b) => b.clone(),
            None => Binding::builtin(Meaning::Unknown),
        };
        binding.value = value;
        self.grow(sym);
        self.set_tracked(sym, binding, global, ReadKind::Value.bit());
    }

    /// Changes a value in place without saving it: TeX's `box(n):=null`
    /// (tex.web § 1079). A group that saved the register undoes it at its end;
    /// otherwise it stays.
    pub fn set_value_in_place(&mut self, sym: Sym, value: Value) {
        self.effective += 1;
        self.slot_changes += 1;
        self.grow(sym);
        let i = sym.0 as usize;
        self.log(i);
        self.wrote_in_place(i);
        self.note_write(sym.0, ReadKind::Value.bit());
        match &mut self.slots[i] {
            Some(binding) => binding.value = value,
            slot => {
                let mut binding = Binding::builtin(Meaning::Unknown);
                binding.value = value;
                *slot = Some(binding);
            }
        }
        self.changed(i);
    }

    pub fn push_group(&mut self, kind: GroupKind, opened: Span) {
        self.group_ids += 1;
        crate::push_shared(
            &mut self.marks,
            Mark {
                journal: self.journal.len(),
                kind,
                after: Vec::new(),
                opened,
                nest: None,
                id: self.group_ids,
                many: false,
            },
        );
    }

    /// The innermost open group's id, 0 outside every group.
    pub fn group_id(&self) -> u64 {
        self.marks.last().map_or(0, |m| m.id)
    }

    /// The id of the group `depth` groups deep, or of the innermost one
    /// when fewer are open.
    pub fn group_id_at(&self, depth: usize) -> u64 {
        match depth {
            0 => 0,
            d => self.marks.get(d - 1).map_or(self.group_id(), |m| m.id),
        }
    }

    /// Closes the innermost group if it was opened after group `id`, as
    /// `\endgroup` would (tex.web §§ 281-283). Not a math group, and with
    /// `keep_many` not one that stands for many (it may stay open). A value it
    /// restores that was itself written after assignment `since` becomes
    /// unknown, since the dropped text would have written it again.
    /// Returns whether it closed.
    pub fn close_since(
        &mut self,
        id: u64,
        since: u64,
        keep_many: bool,
        catcodes: &mut CatcodeTable,
        typed: &Typed,
    ) -> bool {
        let n = self.marks.last().is_some_and(|mark| {
            mark.id > id && !(keep_many && mark.many) && matches!(mark.kind, GroupKind::Simple | GroupKind::SemiSimple)
        });
        if n {
            let mut restored = Vec::new();
            self.memo_odd();
            self.effective += 1;
            let mark = Rc::make_mut(&mut self.marks).pop().expect("checked");
            self.lower();
            self.unwind_into(mark.journal, Some(&mut *catcodes), Some(&mut restored));
            for (sym, stamp) in restored {
                if stamp >= since {
                    self.widen_in_place(Sym(sym), typed);
                }
            }
        }
        n
    }

    /// Record what the innermost group saved of the semantic nest.
    pub fn set_nest(&mut self, nest: Nest) {
        if let Some(mark) = Rc::make_mut(&mut self.marks).last_mut() {
            mark.nest = Some(nest);
        }
    }

    /// What the innermost group of the region saved of the semantic nest.
    pub fn nest(&self) -> Option<Nest> {
        self.group_kind()?;
        self.marks.last().and_then(|m| m.nest)
    }

    /// The nests the open groups saved, innermost last.
    pub fn nests(&self) -> impl DoubleEndedIterator<Item = Option<Nest>> + '_ {
        self.marks.iter().map(|m| m.nest)
    }

    /// Whether an `\halign` or `\valign` group is open anywhere.
    pub fn in_alignment(&self) -> bool {
        self.marks.iter().any(|m| m.nest.is_some_and(|n| n.code == Some(crate::engine::mode::group::ALIGN)))
    }

    /// Where each group still open was opened, innermost last.
    pub fn open_group_spans(&self) -> Vec<(GroupKind, Span)> {
        self.marks.iter().map(|mark| (mark.kind, mark.opened)).collect()
    }

    pub fn group_kind(&self) -> Option<GroupKind> {
        self.marks.last().map(|m| m.kind)
    }

    /// Pop a group, restoring assignments. Returns `\aftergroup` tokens (tex.web § 269).
    pub fn pop_group(&mut self, catcodes: &mut CatcodeTable) -> Option<Vec<Token>> {
        self.effective += 1;
        self.group_kind()?;
        let mark = Rc::make_mut(&mut self.marks).pop()?;
        self.lower();
        self.unwind(mark.journal, Some(catcodes));
        Some(mark.after)
    }

    /// A group closed: every open fork's path now stood in fewer groups.
    fn lower(&mut self) {
        let depth = self.marks.len();
        for low in &mut self.lows {
            *low = (*low).min(depth);
        }
    }

    /// Discard a group without undoing it: its assignments join the enclosing
    /// group (for file-end inside open groups).
    pub fn discard_group(&mut self) -> bool {
        self.effective += 1;
        let popped = Rc::make_mut(&mut self.marks).pop().is_some();
        self.lower();
        popped
    }

    /// `\aftergroup⟨token⟩`: save token, insert at group end (tex.web §§ 326, 282).
    pub fn save_after(&mut self, token: Token) {
        if let Some(mark) = Rc::make_mut(&mut self.marks).last_mut() {
            mark.after.push(token);
        }
    }

    pub fn save_catcode(&mut self, character: char, old: Catcode) {
        self.save_catcode_state(character, old, 0);
    }

    /// [`Env::save_catcode`] keeping whether the category was one satex
    /// could not follow, so that the group end puts that back too.
    pub fn save_catcode_state(&mut self, character: char, old: Catcode, old_may: u16) {
        self.effective += 1;
        if !self.marks.is_empty() {
            self.journal.push(Entry::Catcode { character, old, old_may });
        }
    }

    /// The current bindings without a save stack. A sandbox run starts from
    /// them and cannot undo past them.
    pub fn snapshot(&self) -> Env {
        Env {
            slots: self.slots.clone(),
            level: crate::fixed::Chunked::filled(self.level.len(), 0),
            stamp: crate::fixed::Stamps::filled(self.level.len()),
            trailed: crate::fixed::Chunked::filled(self.level.len(), NONE),
            aliases: self.aliases.clone(),
            ..Default::default()
        }
    }

    fn unwind(&mut self, to: usize, catcodes: Option<&mut CatcodeTable>) {
        self.unwind_into(to, catcodes, None);
    }

    fn unwind_into(
        &mut self,
        to: usize,
        mut catcodes: Option<&mut CatcodeTable>,
        mut restored: Option<&mut Vec<(u32, u64)>>,
    ) {
        while self.journal.len() > to {
            let entry = self.journal.pop().expect("journal length checked");
            if self.tracking.get() {
                // The group end restores what the parts held before, including
                // whether it came from before the load. Entry by entry, since a
                // binding made global in between stays.
                let at = self.journal.len();
                let mut reads = self.reads.borrow_mut();
                while let Some(&(saved, sym, written)) = reads.saved.last().filter(|(saved, ..)| *saved >= at) {
                    reads.saved.pop();
                    if saved == at
                        && matches!(entry, Entry::Meaning { sym: s, .. } if s == sym && self.level[s as usize] != 0)
                    {
                        reads.written.insert(sym, written);
                    }
                }
            }
            match entry {
                // tex.web § 283: a binding made global since is retained.
                Entry::Meaning { sym, .. } if self.level[sym as usize] == 0 => {}
                Entry::Meaning { sym, old, level, alias, stamp } => {
                    let index = sym as usize;
                    match alias {
                        Some(alias) => self.aliases.insert(Sym(sym), alias),
                        None => self.aliases.remove(&Sym(sym)),
                    };

                    // The group's value ends here for good ([`Env::rewind`] settles the rest).
                    if self.tracking.get() && self.paths == 0 {
                        self.reads.borrow_mut().ambient.remove(&sym);
                    }
                    self.log(index);
                    self.level[index] = level;
                    self.stamp.set(index, stamp);
                    self.slots[index] = old;
                    self.changed(index);
                    if let Some(r) = restored.as_deref_mut() {
                        r.push((sym, stamp));
                    }
                    if self.tracking.get() {
                        let mut reads = self.reads.borrow_mut();
                        if reads.dirty_seen.insert(sym) {
                            reads.dirty.push(sym);
                        }
                    }
                }
                Entry::Catcode { character, old, old_may } => {
                    if let Some(table) = catcodes.as_deref_mut() {
                        table.restore(character, old, old_may);
                    }
                }
            }
        }
        let mut reads = self.reads.borrow_mut();
        while reads.saved.last().is_some_and(|(at, ..)| *at >= to) {
            reads.saved.pop();
        }
    }

    /// The names assigned since [`Env::clear_recent`], and a way to put them
    /// back, for a package cache that replays a load.
    pub fn recent(&self) -> Vec<Sym> {
        self.recent.iter().map(|w| Sym(w.sym)).collect()
    }

    pub fn set_recent(&mut self, recent: Vec<Sym>) {
        self.clear_recent();
        self.recent = recent.into_iter().map(|s| Write { sym: s.0, depth: 0, at: 0 }).collect();
    }

    pub fn clear_recent(&mut self) {
        self.recent.clear();
        self.recent_from = self.assignments;
        self.recent_kept = 0;
    }

    /// Logs the write of `sym` made now. A log past `limits.write_log`
    /// that doubled since its last fold is folded first.
    fn log_write(&mut self, sym: u32, depth: u32) {
        if self.recent.len() >= self.sets.write_log.max(2 * self.recent_kept) {
            self.fold_recent();
        }
        self.recent.push(Write { sym, depth, at: self.assignments });
    }

    /// Bounds the log by the names it holds, not by the writes of code
    /// that reads no source for long: of each name its first write stays, and each
    /// later one made less deep than all before it. The first write of a
    /// name at most some depth deep is always one of these, so the log
    /// still says what was written since it began. A mark made before the
    /// fold reads as one made before a clear: its own first writes may be
    /// gone (dropping a later write is not sound for a mark between the
    /// two, whose name a rewind may since have put back).
    fn fold_recent(&mut self) {
        let mut least: std::collections::HashMap<u32, u32, crate::engine::tex::NameHash> = Default::default();
        self.recent.retain(|w| match least.entry(w.sym) {
            std::collections::hash_map::Entry::Vacant(e) => {
                e.insert(w.depth);
                true
            }
            std::collections::hash_map::Entry::Occupied(mut e) => {
                let less = w.depth < *e.get();
                if less {
                    e.insert(w.depth);
                }
                less
            }
        });
        self.recent.shrink_to(2 * self.recent.len());
        self.recent_from = self.assignments;
        self.recent_kept = self.recent.len();
    }

    /// Whether the log holds every write made after assignment `mark`: it
    /// was not cleared, set or folded since.
    pub fn logged_since(&self, mark: u64) -> bool {
        self.recent_from <= mark
    }

    /// The log's part after assignment `mark`, all of it for a mark the
    /// log no longer answers for ([`Env::logged_since`]): never too little.
    fn recent_after(&self, mark: u64) -> &[Write] {
        match self.logged_since(mark) {
            true => &self.recent[self.recent.partition_point(|w| w.at <= mark)..],
            false => &self.recent,
        }
    }

    /// The names written after assignment `at`, sorted and deduplicated,
    /// including writes undone since. `None` when the log was cleared since.
    pub fn written_since(&self, at: u64) -> Option<Vec<Sym>> {
        let names: Vec<u32> = self.logged_since(at).then(|| self.recent_after(at).iter().map(|w| w.sym).collect())?;
        Some(distinct_sorted(&names).into_iter().map(Sym).collect())
    }

    /// The names assigned after assignment `mark`, each once (all in the
    /// log for a mark it no longer answers for). Writes undone by a group
    /// end or a rewind are left out.
    pub fn recent_since(&self, mark: u64) -> Vec<Sym> {
        self.recent_within(mark, u32::MAX)
    }

    /// The names in the whole log, as [`Env::recent_since`] gives them.
    pub fn recent_all(&self) -> Vec<Sym> {
        self.recent_names(&self.recent, u32::MAX)
    }

    /// Like [`Env::recent_since`], but only writes that outlive the groups
    /// above `depth`: global ones and local ones made at most `depth` groups
    /// deep. A local write deeper was undone when its group ended (tex.web
    /// § 283), whatever stamp a resume gave the name since.
    pub fn recent_within(&self, mark: u64, depth: u32) -> Vec<Sym> {
        self.recent_names(self.recent_after(mark), depth)
    }

    fn recent_names(&self, log: &[Write], depth: u32) -> Vec<Sym> {
        // Loops write the same few names over and over: keep each name's
        // first write in one pass, then sort only the distinct names.
        let mut first: std::collections::HashMap<u32, u64, crate::engine::tex::NameHash> = Default::default();
        for w in log.iter().filter(|w| w.depth <= depth) {
            first.entry(w.sym).or_insert(w.at);
        }
        let mut syms: Vec<Sym> = first
            .into_iter()
            .filter(|&(sym, at)| self.stamp.get(sym as usize).is_none_or(|&s| s >= at))
            .map(|(sym, _)| Sym(sym))
            .collect();
        syms.sort_unstable();
        syms
    }

    /// The assignment that last wrote `sym` on this path (0: none).
    pub fn stamp_of(&self, sym: Sym) -> u64 {
        self.stamp.get(sym.0 as usize).copied().unwrap_or(0)
    }

    /// The names written after assignment `mark` since [`Env::clear_recent`],
    /// each once.
    pub fn written_after(&self, mark: u64) -> Vec<Sym> {
        // Use the stamps, not `recent`: a widening clears `recent`, and a later
        // cut may reach back further.
        self.stamp.after(mark).map(|i| Sym(i as u32)).collect()
    }

    /// Makes `syms` hold anything of their kind. With `toward`, each is joined
    /// with what `toward` holds and widened instead (an accumulator stays
    /// typed). A name `toward` never wrote becomes fully unknown.
    pub fn widen(&mut self, syms: &[Sym], toward: Option<(&PathEnv, &Fork)>, typed: &Typed) {
        for &sym in syms {
            let i = sym.0 as usize;
            // What the other path holds: its own write, or the fork's binding
            // it kept (so a token list's braces survive the widening).
            let Some(theirs) = toward.map(|(t, fork)| match t.slots.get(&sym.0) {
                Some((b, _)) => b.clone(),
                None => self.at_fork(fork, sym.0),
            }) else {
                self.widen_in_place(sym, typed);
                continue;
            };
            // Unbound here: the other side's write joins "undefined" (D2).
            let mine = self.slots.get(i).cloned().flatten();
            if mine.is_none() && theirs.is_none() {
                continue;
            }
            let joined = join(theirs.clone(), mine.clone(), self.sets, Some(typed));
            let Some(mut joined) = widen_slot(&theirs, joined, self.sets, typed, Reach::Havoc) else {
                self.widen_in_place(sym, typed);
                continue;
            };
            if same_slot(&mine, &Some(joined.clone())) {
                continue;
            }
            self.grow(sym);
            joined.certain = false;
            self.log(i);
            self.wrote_in_place(i);
            self.note_write(sym.0, ReadKind::Meaning.bit() | ReadKind::Value.bit());
            self.slots[i] = Some(joined);
            self.changed(i);
        }
    }

    /// Slot `i` was changed where it stands, at its own level.
    fn wrote_in_place(&mut self, i: usize) {
        self.memo_odd();
        self.assignments += 1;
        self.effective += 1;
        self.slot_changes += 1;
        self.log_write(i as u32, self.level[i]);
        self.stamp.set(i, self.assignments);
    }

    /// Makes `sym` hold anything, at the group level of its last write. A
    /// group end still restores what it saved, and a local write never becomes
    /// global (`binding.global` only says that some write was global).
    fn widen_in_place(&mut self, sym: Sym, typed: &Typed) {
        let i = sym.0 as usize;
        let Some(Some(binding)) = self.slots.get(i).cloned() else { return };
        let binding = widened(binding, typed, self.sets.text_names);
        // Skip a slot that is already fully wide: widening it again changes
        // nothing a later read could see.
        if same_slot(&self.slots[i], &Some(binding.clone())) {
            return;
        }
        self.log(i);
        self.wrote_in_place(i);
        self.note_write(sym.0, ReadKind::Meaning.bit() | ReadKind::Value.bit());
        self.slots[i] = Some(binding);
        self.changed(i);
    }

    #[inline]
    fn log(&mut self, i: usize) {
        self.unpair(i);
        self.touch(i as u32);
        if self.memo_on.get() {
            let memo = self.memo.get_mut();
            if memo.pre.len() >= READ_LOG {
                memo.full = true;
            } else if memo.at.get(&(i as u32)).is_none_or(|&k| k < memo.floor) {
                memo.at.insert(i as u32, memo.pre.len());
                memo.pre.push((i as u32, self.slots.get(i).cloned().flatten()));
            }
        }
        if self.tracking.get() {
            let mut reads = self.reads.borrow_mut();
            reads.touched.insert(i as u32);
            reads.splices.remove(&(i as u32));
            if !reads.ambient.is_empty() {
                // The old content may come back (a rewind, a group's end): its leaves are
                // read exactly. A leaf about to change: so is every function of it.
                let back = self.paths > 0 || self.level.get(i).is_none_or(|l| *l != self.marks.len() as u32);
                if let Some(amb) = reads.ambient.remove(&(i as u32))
                    && back
                {
                    reads.settle(&amb);
                }
                if reads.sources.contains(&(i as u32)) {
                    let mut named = Vec::new();
                    let of: Vec<u32> = reads
                        .ambient
                        .iter()
                        .filter(|(_, amb)| {
                            named.clear();
                            amb.sources(&mut named);
                            named.iter().any(|(s, _)| s.0 == i as u32)
                        })
                        .map(|(slot, _)| *slot)
                        .collect();
                    for slot in of {
                        if let Some(amb) = reads.ambient.remove(&slot) {
                            reads.settle(&amb);
                        }
                    }
                }
            }
        }
        if let Some(&floor) = self.floors.last() {
            let prev = self.trailed[i];
            if prev == NONE || (prev as usize) < floor {
                self.trailed[i] = self.trail.len() as u32;
                self.trail.push((i as u32, self.slots[i].clone(), self.level[i], prev, self.stamp[i]));
            }
        }
    }

    /// Slot `i` was written: every open fork sees it changed.
    #[inline]
    fn changed(&mut self, i: usize) {
        if self.changed.iter().all(Option::is_none) {
            return;
        }
        let entry = (self.slots[i].clone(), self.level[i]);
        for map in self.changed.iter_mut().flatten() {
            map.insert_mut(i as u32, entry.clone());
        }
    }

    /// A point the bindings can be rewound to.
    pub fn fork(&mut self) -> Fork {
        self.forks += 1;
        self.floors.push(self.trail.len());
        self.changed.push(None);
        self.lows.push(self.marks.len());
        Fork {
            trail: self.trail.len(),
            level: self.floors.len() - 1,
            parted: false,
            journal: self.journal.snapshot(),
            marks: self.marks.clone(),
            corr: self.corr.clone(),
        }
    }

    /// Several paths go on from `fork`.
    pub fn part(&mut self, fork: &mut Fork) {
        fork.parted = true;
        self.paths += 1;
    }

    /// The fork is done. The bindings stay as the running path left them.
    pub fn unfork(&mut self, fork: Fork) {
        self.forks -= 1;
        self.paths -= usize::from(fork.parted);
        // Write now what the paths left different from the fork: a later read
        // depends on the load for it, and the load's segment must carry it.
        if fork.parted && self.paths == 0 && self.tracking.get() {
            for k in fork.trail..self.trail.len() {
                let sym = self.trail[k].0;
                let (now, then) = (self.slots[sym as usize].as_ref(), self.at_fork(&fork, sym));
                if !now
                    .zip(then.as_ref())
                    .is_some_and(|(a, b)| a.meaning == b.meaning && same_value(&a.value, &b.value) && a.may == b.may)
                    && (now.is_some() || then.is_some())
                {
                    self.note_write(sym, ReadKind::Meaning.bit() | ReadKind::Value.bit());
                }
            }
        }
        debug_assert_eq!(fork.level + 1, self.floors.len(), "forks end innermost first");
        self.floors.truncate(fork.level);
        self.changed.truncate(fork.level);
        self.lows.truncate(fork.level);
        if self.forks == 0 {
            for (sym, ..) in &self.trail {
                self.trailed[*sym as usize] = NONE;
            }
            self.trail.clear();
        } else {
            debug_assert!(self.trail.len() >= fork.trail);
            self.fold_trail(fork.trail);
        }
    }

    /// The trail past `from`, of a fork just ended, as the enclosing fork
    /// needs it: only a slot's first change since that fork. A later one
    /// saved a binding rewinding passes over, and a loop under nested
    /// undecided conditionals kept every body it ever built that way.
    fn fold_trail(&mut self, from: usize) {
        let Some(&floor) = self.floors.last() else { return };
        let mut kept = from;
        for k in from..self.trail.len() {
            let (sym, prev) = (self.trail[k].0 as usize, self.trail[k].3);
            if prev == NONE || (prev as usize) < floor {
                self.trailed[sym] = kept as u32;
                self.trail.swap(kept, k);
                kept += 1;
            } else if (prev as usize) < from {
                // The slot's first change since the enclosing fork came before this fork.
                self.trailed[sym] = prev;
            }
        }
        self.trail.truncate(kept);
    }

    /// What `sym` held when `fork` was made: from the trail if it changed
    /// since, else its current binding.
    pub fn at_fork(&self, fork: &Fork, sym: u32) -> Option<Binding> {
        let (mut e, mut first) = (self.trailed.get(sym as usize).copied().unwrap_or(NONE), None);
        while e != NONE && e as usize >= fork.trail {
            first = Some(e as usize);
            e = self.trail[e as usize].3;
        }
        match first {
            Some(e) => self.trail[e].1.clone(),
            None => self.slots.get(sym as usize).cloned().flatten(),
        }
    }

    /// What the running path changed since `fork`.
    pub fn path(&mut self, fork: &Fork) -> PathEnv {
        debug_assert_eq!(self.floors.get(fork.level), Some(&fork.trail), "the fork is open");
        let (trail, slots, level, walked) = (self.trail.iter_from(fork.trail), &self.slots, &self.level, &self.walked);
        let slots = self.changed.get_mut(fork.level).map_or_else(Changed::new, |map| {
            let entry = |sym: u32| (sym, (slots[sym as usize].clone(), level[sym as usize]));
            map.get_or_insert_with(|| Changed::collect(trail.map(|e| entry(e.0)), walked)).clone()
        });
        let low = self.lows.get(fork.level).copied().unwrap_or(fork.marks.len());
        PathEnv { slots, journal: self.journal.clone(), marks: self.marks.clone(), low, corr: self.corr.clone() }
    }

    /// Back to the bindings `fork` saw.
    pub fn rewind(&mut self, fork: &Fork) {
        self.switches += 1;
        self.effective += 1;
        // Slots go back without a write: no function of the load's leaves outlives that.
        if self.tracking.get() && self.trail.len() > fork.trail {
            let mut reads = self.reads.borrow_mut();
            for (_, amb) in std::mem::take(&mut reads.ambient) {
                reads.settle(&amb);
            }
        }

        let mut popped = Vec::new();
        while self.trail.len() > fork.trail {
            let (sym, slot, saved, prev, stamp) = self.trail.pop().expect("trail length checked");
            self.stamp.set(sym as usize, stamp);
            self.slots[sym as usize] = slot;
            self.level[sym as usize] = saved;
            self.trailed[sym as usize] = prev;
            popped.push(sym);
        }
        // Nothing changed since the fork. An outer fork sees a slot as it is
        // now, or unchanged if the change came after this fork.
        self.floors.truncate(fork.level + 1);
        self.changed.truncate(fork.level + 1);
        self.lows.truncate(fork.level + 1);
        if let Some(low) = self.lows.get_mut(fork.level) {
            *low = fork.marks.len();
        }
        if let Some(own) = self.changed.get_mut(fork.level) {
            *own = None;
        }
        popped.sort_unstable();
        popped.dedup();
        for sym in popped {
            let i = sym as usize;
            let entry = (self.slots[i].clone(), self.level[i]);
            for (floor, map) in self.floors.iter().zip(&mut self.changed).take(fork.level) {
                let Some(map) = map else { continue };
                if self.trailed[i] != NONE && self.trailed[i] as usize >= *floor {
                    map.insert_mut(sym, entry.clone());
                } else {
                    map.remove_mut(&sym);
                }
            }
        }
        self.journal = fork.journal.clone();
        self.marks = fork.marks.clone();
        self.corr = fork.corr.clone();
    }

    /// Continue `path` from `fork`.
    pub fn resume(&mut self, fork: &Fork, path: &PathEnv) {
        self.switches += 1;
        self.effective += 1;
        self.rewind(fork);
        RESUMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        RESUME_SLOTS.fetch_add(path.slots.size() as u64, std::sync::atomic::Ordering::Relaxed);

        // Share the path's map as the fork's own map. Rebuilding it per resume
        // would give every head and parked path its own copy.
        let own = self.changed.len() == fork.level + 1;
        if own {
            self.changed.pop();
        }
        self.corr = None;
        for (sym, (slot, saved)) in path.slots.iter(&self.walked) {
            let i = *sym as usize;
            // A name the path left as the fork has it does not count as written.
            if self.level[i] == *saved && same_slot(&self.slots[i], slot) {
                continue;
            }
            self.log(i);
            self.slots[i] = slot.clone();
            self.level[i] = *saved;
            // What the path wrote counts as written now.
            self.stamp.set(i, self.assignments);
            self.changed(i);
        }
        if own {
            self.changed.push(Some(path.slots.clone()));
        }
        self.journal = path.journal.clone();
        self.marks = path.marks.clone();
        for low in &mut self.lows {
            *low = (*low).min(path.low);
        }
        self.corr = path.corr.clone();
    }

    /// Rebases `paths` (each from `fork`) onto the running path's current
    /// state, so `fork` can close. For a name the running path changed but a
    /// path did not, that path gets what `fork` saw. The bindings stay as the
    /// running path has them. False when the running path changed nothing since `fork`.
    pub fn rebase(&mut self, fork: &Fork, paths: &mut [&mut PathEnv]) -> bool {
        self.switches += 1;
        self.effective += 1;
        let live = self.path(fork);
        if live.slots.is_empty() {
            return false;
        }
        self.rewind(fork);
        // Build once what `fork` saw of each name the running path changed, and
        // share it. Each path lays its own changes over it, so the cost is the
        // number of names plus each path's changes, not their product.
        let mut seen = Changed::new();
        for sym in live.slots.keys(&self.walked) {
            self.grow(Sym(*sym));
            let i = *sym as usize;
            seen.insert_mut(*sym, (self.slots[i].clone(), self.level[i]));
        }
        for path in paths.iter_mut() {
            // Lay the smaller map over the larger.
            if path.slots.size() * 2 <= seen.size() {
                let mut slots = seen.clone();
                for (sym, slot) in path.slots.iter(&self.walked) {
                    slots.insert_mut(*sym, slot.clone());
                }
                path.slots = slots;
            } else {
                for (sym, slot) in seen.iter(&self.walked) {
                    if !path.slots.contains_key(sym) {
                        path.slots.insert_mut(*sym, slot.clone());
                    }
                }
            }
            path.low = path.low.min(live.low);
        }
        self.resume(fork, &live);
        true
    }

    /// The slot `sym` now holds.
    pub fn slot_at(&self, sym: u32) -> Option<Binding> {
        self.memo_read(sym, true);
        self.slots.get(sym as usize).cloned().flatten()
    }

    /// Continues as if `sym` held `slot`, e.g. a loop head's state joined with
    /// what the loop reached there, which is more than this path knows. Not an
    /// assignment, so the save stack is not involved.
    pub fn assume(&mut self, sym: u32, slot: Option<Binding>) {
        self.memo_odd();
        self.effective += 1;
        let i = sym as usize;
        self.grow(Sym(sym));
        self.log(i);
        self.slots[i] = slot;
        self.changed(i);
    }

    /// Records that a name with several possible meanings holds `meaning` on
    /// this path. Not an assignment, so the save stack is not involved.
    /// `keep` as for [`Env::refine_value`].
    pub fn refine(&mut self, sym: Sym, meaning: Meaning, keep: &[bool]) {
        self.memo_odd();
        self.effective += 1;
        let i = sym.0 as usize;
        let Some(Some(binding)) = self.slots.get(i) else { return };
        let binding = Binding { meaning, may: None, ..binding.clone() };
        self.correlate(i, keep);
        self.log(i);
        self.slots[i] = Some(binding);
        self.changed(i);
    }

    /// [`Env::refine`] to the meanings in `members`. If several remain, the
    /// name keeps them as its meaning set.
    pub fn refine_to(&mut self, sym: Sym, members: &[Meaning], keep: &[bool]) {
        match members {
            [one] => self.refine(sym, one.clone(), keep),
            _ => self.refine_may(sym, members, keep),
        }
    }

    /// [`Env::refine_to`], but `members` stays a set even with one member: the
    /// texts a one-of token still stands for.
    pub fn refine_may(&mut self, sym: Sym, members: &[Meaning], keep: &[bool]) {
        self.memo_odd();
        self.effective += 1;
        let i = sym.0 as usize;
        let Some(Some(binding)) = self.slots.get(i) else { return };
        let binding = Binding { meaning: Meaning::Unknown, may: Some(members.into()), ..binding.clone() };
        self.correlate(i, keep);
        self.log(i);
        self.slots[i] = Some(binding);
        self.changed(i);
    }

    /// The groups the running path stands in, outermost first: each one's
    /// kind and waiting `\aftergroup` tokens.
    pub fn groups(&self) -> Vec<(GroupKind, bool, Vec<Tok>)> {
        groups(&self.marks)
    }

    /// Gives `sym` a meaning determined by its name, the same on every path
    /// and never undone by a rewind.
    pub fn constant(&mut self, sym: Sym, binding: Binding) {
        self.memo_odd();
        self.effective += 1;
        self.grow(sym);
        let i = sym.0 as usize;
        self.unpair(i);
        self.slots[i] = Some(Binding { global: true, ..binding });
        self.level[i] = 0;
        // Not journaled, so no rewind undoes it: it holds on every path.
        let paths = std::mem::take(&mut self.paths);
        self.note_write(sym.0, ReadKind::Meaning.bit() | ReadKind::Value.bit());
        self.paths = paths;
    }

    /// Least upper bound of two paths from `fork` in the same groups. A name
    /// with different meanings on the two paths gets the set of both.
    /// `after[i]` holds the joined `\aftergroup` tokens of group `i` where they
    /// differ (tex.web § 326). With `widen`, a slot that grows becomes anything
    /// of its kind, as [`Reach`] allows. With `paired`, the arms stay paired
    /// ([`Corr`]) in the names the widening left as their join. Also returns the
    /// names where the bound holds more than `a`, and whether its waiting tokens
    /// grew. The bindings are left as `fork` saw them.
    pub fn join_paths(
        &mut self,
        fork: &Fork,
        a: &PathEnv,
        b: &PathEnv,
        typed: &Typed,
        (widen, paired): (Option<Reach>, bool),
        after: &[Option<Vec<Token>>],
    ) -> (PathEnv, Vec<Sym>, bool) {
        self.rewind(fork);
        // A subtree both paths share whose names each join to themselves at
        // their level leaves `a`'s map as it is: no name there is touched.
        let depth = a.marks.len() as u32;
        let mut touched: Vec<u32> = a
            .slots
            .pairs(&b.slots, |s| Changed::settled(s, depth), &self.walked)
            .into_iter()
            .map(|(k, ..)| k)
            .collect();
        touched.sort_unstable();
        let mut grown = Vec::new();
        // Start from `a`'s map, shared where the join keeps `a`'s binding.
        let mut slots = a.slots.clone();
        let (mut apart, mut lost) = (Vec::new(), Vec::new());
        for sym in touched {
            self.grow(Sym(sym));
            let i = sym as usize;
            let side =
                |p: &PathEnv| p.slots.get(&sym).map_or_else(|| (self.slots[i].clone(), self.level[i]), Clone::clone);
            let ((old, x), (new, y)) = (side(a), side(b));
            let differ = !identical(&old, &new);
            let (old, new) = match differ {
                true => (self.settle(a, old, typed), self.settle(b, new, typed)),
                false => (old, new),
            };
            // A group end restores what either path saved (tex.web § 283: only
            // what both made global is kept). A level beyond the joined groups
            // is one only `b` was in.
            let level = x.max(y).min(a.marks.len() as u32);
            let joined = join(old.clone(), new, self.sets, Some(typed));
            let joined = match widen {
                Some(reach) => {
                    let widened = widen_slot(&old, joined.clone(), self.sets, typed, reach);
                    // Arms stay paired in what the widening left as the join.
                    if !identical(&joined, &widened) {
                        lost.push(sym);
                    }
                    widened
                }
                None => joined,
            };
            if differ {
                apart.push(sym);
            }
            if !same_slot(&old, &joined) {
                grown.push(Sym(sym));
            }
            if a.slots.get(&sym).is_some_and(|(kept, l)| *l == level && identical(kept, &joined)) {
                continue;
            }
            slots.insert_mut(sym, (joined, level));
        }
        let mut marks = Vec::clone(&a.marks);
        let mut waiting = false;
        for ((mark, other), joined) in marks.iter_mut().zip(b.marks.iter()).zip(after) {
            if let Some(text) = joined {
                waiting |=
                    !(mark.after.len() == text.len() && mark.after.iter().zip(text).all(|(x, y)| x.tok == y.tok));
                mark.after = text.clone();
            }
            // Join what the groups saved of the semantic nest like the bindings.
            mark.nest = match (mark.nest, other.nest) {
                (Some(x), Some(y)) => Some(x.join(y)),
                (x, y) => x.or(y),
            };
        }
        let low = a.low.min(b.low);
        let journal = self.join_journals(fork, a, b, low, &mut marks);
        let corr = paired.then(|| self.correlated(apart, a, b, &lost)).flatten();
        (PathEnv { slots, journal, marks: Rc::new(marks), low, corr }, grown, waiting)
    }

    /// `binding` as path `p` holds it. A one-of token that `p` narrowed to one
    /// text ([`Env::refine_may`]) becomes that text, so arms that took different
    /// texts join as texts, not as the token both still hold.
    fn settle(&self, p: &PathEnv, binding: Option<Binding>, typed: &Typed) -> Option<Binding> {
        let text = |t: &Token| -> Option<Rc<[Token]>> {
            let sym = t.cs()?;
            let name = typed.interner.name(sym);
            name.strip_prefix(crate::analysis::machine::UNKNOWN_NAME)?
                .strip_prefix(crate::analysis::machine::explore::ONE_OF)?;
            let held = p
                .slots
                .get(&sym.0)
                .map_or_else(|| self.slots.get(sym.0 as usize).cloned().flatten(), |(b, _)| b.clone());
            match held?.may.as_deref() {
                Some([Meaning::Macro(d)]) => Some(d.replacement_text.clone()),
                _ => None,
            }
        };
        let spell = |m: &Meaning| match m {
            Meaning::Macro(d) if d.replacement_text.iter().any(|t| text(t).is_some()) => {
                let body: Vec<Token> =
                    d.replacement_text.iter().flat_map(|t| text(t).map_or_else(|| vec![*t], |x| x.to_vec())).collect();
                Some(Meaning::Macro(Rc::new(crate::engine::tex::MacroDef {
                    replacement_text: body.into(),
                    ..(**d).clone()
                })))
            }
            _ => None,
        };
        let mut b = binding?;
        if let Some(m) = spell(&b.meaning) {
            b.meaning = m;
        }
        if let Some(may) = &b.may
            && may.iter().any(|m| spell(m).is_some())
        {
            b.may = Some(may.iter().map(|m| spell(m).unwrap_or_else(|| m.clone())).collect());
        }
        Some(b)
    }

    /// The arms of a join of `a` and `b` that differ in `apart`, each
    /// path's own arms if it came from a join: `None` past `sets.meanings`
    /// arms, or when fewer than two names differ.
    fn correlated(&self, mut apart: Vec<u32>, a: &PathEnv, b: &PathEnv, lost: &[u32]) -> Option<Rc<Corr>> {
        for c in [&a.corr, &b.corr].into_iter().flatten() {
            for &sym in &c.syms {
                if !apart.contains(&sym) {
                    apart.push(sym);
                }
            }
        }
        apart.retain(|sym| !lost.contains(sym));
        // Names the arms were paired in already come first and are kept.
        let paired = |s: &u32| [&a.corr, &b.corr].into_iter().flatten().any(|c| c.syms.contains(s));
        apart.sort_by_key(|s| !paired(s));
        apart.truncate(self.sets.names);
        if apart.len() < 2 {
            return None;
        }
        let mut arms: Vec<Vec<Option<Binding>>> = Vec::new();
        for p in [a, b] {
            let own = |sym: u32| p.slots.get(&sym).map_or_else(|| self.slots[sym as usize].clone(), |(b, _)| b.clone());
            let theirs: Vec<Option<&[Option<Binding>]>> = match &p.corr {
                Some(c) => c.arms.iter().map(|arm| Some(&arm[..])).collect(),
                None => vec![None],
            };
            for arm in theirs {
                let pick = |sym: u32| match (arm, &p.corr) {
                    (Some(arm), Some(c)) => match c.syms.iter().position(|&s| s == sym) {
                        Some(n) => arm[n].clone(),
                        None => own(sym),
                    },
                    _ => own(sym),
                };
                let row: Vec<Option<Binding>> = apart.iter().map(|&sym| pick(sym)).collect();
                if !arms.iter().any(|r| r.iter().zip(&row).all(|(x, y)| identical(x, y))) {
                    arms.push(row);
                }
            }
        }
        // Past the bound, the name with the most distinct bindings is
        // forgotten first: few-valued pairings (an operation stacked or
        // not) are what tells the arms apart.
        let mut corr = Corr { syms: apart, arms };
        while corr.arms.len() > self.sets.arms && corr.syms.len() > 2 {
            let distinct = |n: usize| {
                let mut seen: Vec<&Option<Binding>> = Vec::new();
                for row in &corr.arms {
                    if !seen.iter().any(|x| same_slot(x, &row[n])) {
                        seen.push(&row[n]);
                    }
                }
                seen.len()
            };
            let drop = (0..corr.syms.len()).max_by_key(|&n| distinct(n)).expect("names");
            let keep: Vec<usize> = (0..corr.syms.len()).filter(|&n| n != drop).collect();
            corr = corr.project(&keep);
            let mut rows: Vec<Vec<Option<Binding>>> = Vec::new();
            for row in corr.arms {
                match rows.iter_mut().find(|r| r.iter().zip(&row).all(|(x, y)| same_slot(x, y))) {
                    Some(r) => *r = r.iter().zip(row).map(|(x, y)| join(x.clone(), y, self.sets, None)).collect(),
                    None => rows.push(row),
                }
            }
            corr.arms = rows;
        }
        (corr.arms.len() >= 2 && corr.arms.len() <= self.sets.arms).then(|| Rc::new(corr))
    }

    /// Whether every state of `here` is one of `head`'s arms, where
    /// `head` came from a join that kept them ([`Corr`]): both paths
    /// from `fork`. Leaves the bindings as `fork` saw them.
    pub fn covers(&mut self, fork: &Fork, head: &PathEnv, here: &PathEnv) -> bool {
        let Some(corr) = &head.corr else { return true };
        self.rewind(fork);
        let own = |sym: u32| here.slots.get(&sym).map_or_else(|| self.slots[sym as usize].clone(), |(b, _)| b.clone());
        let rows: Vec<Vec<Option<Binding>>> = match &here.corr {
            Some(c) => c
                .arms
                .iter()
                .map(|arm| {
                    let at =
                        |sym: u32| c.syms.iter().position(|&s| s == sym).map_or_else(|| own(sym), |n| arm[n].clone());
                    corr.syms.iter().map(|&sym| at(sym)).collect()
                })
                .collect(),
            None => vec![corr.syms.iter().map(|&sym| own(sym)).collect()],
        };
        let within =
            |h: &Option<Binding>, x: &Option<Binding>| same_slot(&join(h.clone(), x.clone(), self.sets, None), h);
        rows.iter().all(|row| corr.arms.iter().any(|arm| arm.iter().zip(row).all(|(h, x)| within(h, x))))
    }

    /// Slot `i` is written: the arms no longer tell its binding. Dropped
    /// once here, not per fork and path that read the [`Corr`].
    #[inline]
    fn unpair(&mut self, i: usize) {
        let Some(corr) = &self.corr else { return };
        let Some(n) = corr.syms.iter().position(|&s| s as usize == i) else { return };
        let keep: Vec<usize> = (0..corr.syms.len()).filter(|&m| m != n).collect();
        self.corr = (keep.len() >= 2).then(|| Rc::new(corr.project(&keep)));
    }

    /// The bindings `sym` has in the arms of the join the running path
    /// came from ([`Corr`]): empty when they are not kept.
    pub fn arms(&self, sym: Sym) -> Vec<Option<Binding>> {
        let Some(corr) = self.corr.clone() else { return Vec::new() };
        let Some(n) = corr.syms.iter().position(|&s| s == sym.0) else { return Vec::new() };
        corr.arms.iter().map(|arm| arm[n].clone()).collect()
    }

    /// Slot `i` is refined to what the arms flagged in `keep` allow: the
    /// other names of the running path's [`Corr`] are refined alike.
    fn correlate(&mut self, i: usize, keep: &[bool]) {
        let Some(corr) = self.corr.clone() else { return };
        let Some(n) = corr.syms.iter().position(|&s| s as usize == i) else { return };
        let keep: Vec<usize> = (0..corr.arms.len()).filter(|&k| keep.get(k) == Some(&true)).collect();
        if keep.is_empty() || keep.len() == corr.arms.len() {
            return;
        }
        self.corr = None;
        for m in (0..corr.syms.len()).filter(|&m| m != n) {
            let joined = keep.iter().map(|&k| corr.arms[k][m].clone()).reduce(|x, y| join(x, y, self.sets, None));
            let j = corr.syms[m] as usize;
            let joined = joined.flatten();
            if !identical(&self.slots[j], &joined) {
                self.log(j);
                self.slots[j] = joined;
                self.changed(j);
            }
        }
        let rest: Vec<usize> = (0..corr.syms.len()).filter(|&m| m != n).collect();
        let corr =
            Corr { syms: corr.syms.clone(), arms: keep.iter().map(|&k| corr.arms[k].clone()).collect() }.project(&rest);
        self.corr = (rest.len() >= 2 && corr.arms.len() >= 2).then(|| Rc::new(corr));
    }

    /// The save stacks of two paths from `fork` joined (tex.web §§ 268-283).
    /// Each group keeps what either path saved, so its end restores either.
    /// A value the other path overrode globally (kept by a group end, § 283)
    /// is joined with the saved one. `marks` are `a`'s, updated to where
    /// their groups now start.
    fn join_journals(&self, fork: &Fork, a: &PathEnv, b: &PathEnv, low: usize, marks: &mut [Mark]) -> Journal {
        // Only the groups neither path left since the fork share what the
        // fork saved; a group closed since took its entries with it.
        let base = fork.marks.get(low).map_or(fork.journal.len(), |m| m.journal);
        let (x, y) = (segments(&a.journal, &a.marks, base), segments(&b.journal, &b.marks, base));
        let global =
            |p: &PathEnv, sym: u32| p.slots.get(&sym).and_then(|(slot, level)| (*level == 0).then(|| slot.clone())?);
        let mut journal = fork.journal.clone();
        while journal.len() > base {
            journal.pop();
        }
        // A group only `a` stands in keeps its saves, joined with what `b`
        // holds; one only `b` stands in is not in the join.
        let mut y = y.into_iter();
        for (g, mine) in x.into_iter().enumerate() {
            let theirs = y.next().unwrap_or_default();
            if g > 0 && marks[g - 1].journal >= base {
                marks[g - 1].journal = journal.len();
            }
            let mut merged: Vec<Entry> = Vec::with_capacity(mine.len() + theirs.len());
            // A slot one path saved twice in a group ends it with what the
            // first save holds: only that one joins.
            let first = |entries: Vec<Entry>| {
                let mut seen = std::collections::HashSet::new();
                entries.into_iter().filter(|e| seen.insert(e.slot())).collect::<Vec<_>>()
            };
            let (mine, theirs) = (first(mine), first(theirs));
            // A slot both paths saved here: the group's end puts back either save.
            let saved = |entries: &[Entry]| entries.iter().map(Entry::slot).collect::<std::collections::HashSet<_>>();
            let (in_mine, in_theirs) = (saved(&mine), saved(&theirs));
            let tagged =
                mine.into_iter().map(|e| (e, b, &in_theirs)).chain(theirs.into_iter().map(|e| (e, a, &in_mine)));
            for (entry, other, both) in tagged {
                let entry = match entry {
                    Entry::Meaning { sym, old, level, alias, stamp } => {
                        let old = match global(other, sym) {
                            Some(kept) => join(old, Some(kept), self.sets, None),
                            // A name the other path did not save in this
                            // group: its end leaves what that path holds, set
                            // outside the group or at the fork.
                            None if is_name(&old) && !both.contains(&(false, sym)) => {
                                let held = match other.slots.get(&sym) {
                                    Some((slot, level)) if (*level as usize) < g => Some(slot.clone()),
                                    Some(_) => None,
                                    None => Some(self.slots.get(sym as usize).cloned().flatten()),
                                };
                                match held {
                                    Some(held) => join(old, held, self.sets, None),
                                    None => old,
                                }
                            }
                            None => old,
                        };
                        Entry::Meaning { sym, old, level, alias, stamp }
                    }
                    catcode => catcode,
                };
                match merged.iter_mut().find(|e| e.same_slot(&entry)) {
                    Some(seen) => seen.join(entry, self.sets),
                    None => merged.push(entry),
                }
            }
            for entry in merged {
                journal.push(entry);
            }
        }
        journal
    }
}

impl Entry {
    /// The slot the entry saves: a name or a character's category.
    fn slot(&self) -> (bool, u32) {
        match self {
            Entry::Meaning { sym, .. } => (false, *sym),
            Entry::Catcode { character, .. } => (true, u32::from(*character)),
        }
    }

    /// Whether two entries save the same slot.
    fn same_slot(&self, other: &Entry) -> bool {
        self.slot() == other.slot()
    }

    /// Two paths saved the same slot: what either may put back.
    fn join(&mut self, other: Entry, sets: SetLimits) {
        match (self, other) {
            (Entry::Meaning { old, stamp, .. }, Entry::Meaning { old: theirs, stamp: at, .. }) => {
                *old = join(old.clone(), theirs, sets, None);
                *stamp = (*stamp).max(at);
            }
            (Entry::Catcode { old, old_may, .. }, Entry::Catcode { old: theirs, old_may: may, .. }) => {
                let cats = |cat: Catcode, may: u16| if may == 0 { 1 << cat as u8 } else { may };
                let joined = cats(*old, *old_may) | cats(theirs, may);
                *old_may = if joined.count_ones() > 1 { joined } else { 0 };
            }
            _ => {}
        }
    }
}

/// What a path saved since the save stack stood `base` entries high, by the
/// group it was saved in: first what lies below every group, then each open
/// group, outermost first.
fn segments(journal: &Journal, marks: &[Mark], base: usize) -> Vec<Vec<Entry>> {
    let mut journal = journal.clone();
    let mut entries = Vec::new();
    while journal.len() > base {
        entries.push(journal.pop().expect("length checked"));
    }
    entries.reverse();
    let mut segments = vec![Vec::new(); marks.len() + 1];
    for (k, entry) in entries.into_iter().enumerate() {
        let at = base + k;
        let group = marks.iter().rposition(|m| m.journal <= at).map_or(0, |g| g + 1);
        segments[group].push(entry);
    }
    segments
}

impl PathEnv {
    /// [`Env::groups`] of a parked path.
    pub fn groups(&self) -> Vec<(GroupKind, bool, Vec<Tok>)> {
        groups(&self.marks)
    }

    /// The groups `from..to` as one that stands for any number of them
    /// (none open there: one inserted, of `kind` with `after` waiting): a
    /// widened stack. Their saves are its saves, the groups above keep
    /// theirs. Only groups opened since the fork, so the fork's own levels
    /// stay as they are.
    pub fn many_from(&mut self, from: usize, to: usize, kind: GroupKind, after: &[Token], id: u64, walked: &Walked) {
        let journal = self.journal.len();
        let marks = Rc::make_mut(&mut self.marks);
        match from < to {
            true => {
                marks[from].many = true;
                marks.drain(from + 1..to);
            }
            false => {
                let journal = marks.get(from).map_or(journal, |m| m.journal);
                let mark =
                    Mark { journal, kind, after: after.to_vec(), opened: Span::default(), nest: None, id, many: true };
                marks.insert(from, mark);
            }
        }
        // A slot written in the merged groups now counts as written in the
        // widened one (a group opened later saves it again); one written above
        // moves with its group.
        let (top, to) = ((from + 1) as u32, to as u32);
        let level = |l: u32| match l {
            l if l < top => l,
            l if l <= to => top,
            l => l + top - to,
        };
        let moved: Vec<(u32, Option<Binding>, u32)> = (self.slots.iter(walked))
            .filter(|(_, (_, l))| level(*l) != *l)
            .map(|(s, (b, l))| (*s, b.clone(), level(*l)))
            .collect();
        for (sym, binding, l) in moved {
            self.slots.insert_mut(sym, (binding, l));
        }
    }

    /// Each open group's id and where it was opened, outermost first.
    pub fn group_marks(&self) -> Vec<(u64, Span)> {
        self.marks.iter().map(|m| (m.id, m.opened)).collect()
    }

    /// Whether the groups from `from` up saved nothing of the nest: no box
    /// or alignment group.
    pub fn plain_from(&self, from: usize) -> bool {
        self.marks.iter().skip(from).all(|m| m.nest.is_none())
    }

    /// The waiting `\aftergroup` tokens of group `i`.
    pub fn after_at(&self, i: usize) -> Vec<Token> {
        self.marks.get(i).map(|m| m.after.clone()).unwrap_or_default()
    }

    /// Where each open group was opened, outermost first.
    pub fn openers(&self) -> Vec<Span> {
        self.marks.iter().map(|m| m.opened).collect()
    }

    /// The `\aftergroup` tokens waiting in each group, outermost first.
    pub fn after(&self) -> Vec<&[Token]> {
        self.marks.iter().map(|m| m.after.as_slice()).collect()
    }
}

fn groups(marks: &[Mark]) -> Vec<(GroupKind, bool, Vec<Tok>)> {
    marks.iter().map(|m| (m.kind, m.many, m.after.iter().map(|t| t.tok).collect())).collect()
}

/// How many meanings a name may be left with by joins before it is widened
/// to an unknown one, unless `limits.meaning_set` says otherwise.
pub const MAY_MEANINGS: usize = 5;

/// How many meanings and how many values a join keeps as a set
/// (`limits.meaning_set`, `limits.value_set`, `limits.interval_set`).
/// What a join of paths kept of its arms: per arm, the bindings of the
/// names they differ in. A path that learns one name's binding learns
/// the others' from the arms that allow it (`\pgfmath`'s operation stack
/// and its top stay paired).
#[derive(serde::Serialize, serde::Deserialize)]
pub struct Corr {
    syms: Vec<u32>,
    arms: Vec<Vec<Option<Binding>>>,
}

impl Corr {
    /// Only the names at `keep`.
    fn project(&self, keep: &[usize]) -> Corr {
        let pick = |row: &Vec<Option<Binding>>| keep.iter().map(|&n| row[n].clone()).collect();
        Corr { syms: keep.iter().map(|&n| self.syms[n]).collect(), arms: self.arms.iter().map(pick).collect() }
    }
}

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct SetLimits {
    pub meanings: usize,
    pub arms: usize,
    pub names: usize,
    pub values: usize,
    pub intervals: usize,
    /// Control sequences typed text names ([`Typed::named`]).
    pub text_names: usize,
    /// Writes the log holds before it is folded ([`Env::fold_recent`]).
    pub write_log: usize,
}

impl Default for SetLimits {
    fn default() -> Self {
        SetLimits {
            meanings: MAY_MEANINGS,
            arms: MAY_MEANINGS,
            names: 64,
            values: crate::engine::value::MAY_VALUES,
            intervals: crate::engine::value::MAY_INTERVALS,
            text_names: 8,
            write_log: 1 << 20,
        }
    }
}

/// Whether two slots hold the same abstract state: the same meaning (or
/// set of meanings), value and certainty.  Which definitions made them does
/// not count: it is what a use depends on, not what it sees.
/// The same binding in every part a join looks at, definition sites too.
fn identical(a: &Option<Binding>, b: &Option<Binding>) -> bool {
    same_slot(a, b)
        && match (a, b) {
            (Some(x), Some(y)) => x.defs == y.defs && x.global == y.global && x.may == y.may,
            _ => true,
        }
}

/// [`same_slot`] of `a` and `b`, or with `value` false what a read of the
/// meaning sees.
pub fn same_part(a: Option<&Binding>, b: Option<&Binding>, value: bool) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            same_meaning(x, y) && x.certain == y.certain && (!value || same_value(&x.value, &y.value))
        }
        _ => false,
    }
}

pub fn same_slot(a: &Option<Binding>, b: &Option<Binding>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => same_meaning(x, y) && same_value(&x.value, &y.value) && x.certain == y.certain,
        _ => false,
    }
}

fn same_meaning(x: &Binding, y: &Binding) -> bool {
    one_meaning(&x.meaning, &y.meaning)
        && match (&x.may, &y.may) {
            (None, None) => true,
            (Some(p), Some(q)) => p.len() == q.len() && p.iter().all(|m| q.contains(m)),
            _ => false,
        }
}

/// `a == b`, without reading a shared definition: meanings compare
/// reflexively, and paths mostly hold the definition they copied.
fn one_meaning(a: &Meaning, b: &Meaning) -> bool {
    matches!((a, b), (Meaning::Macro(p), Meaning::Macro(q)) if Rc::ptr_eq(p, q)) || a == b
}

fn same_value(a: &Value, b: &Value) -> bool {
    matches!((a, b), (Value::Toks(p), Value::Toks(q)) if Rc::ptr_eq(p, q))
        || a == b
        || matches!((a, b), (Value::Unknown, Value::Unknown))
}

/// Why a slot is widened, which decides how much of its texts' structure
/// (the tokens between digit runs: operators, letters) the hull keeps.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reach {
    /// A join a pass runs from: a loop head's, or a summary's that is not
    /// cut. The members are every text a path reaching it holds, so a hull
    /// that keeps what all of them have is their upper bound, and a later
    /// pass that writes another text meets the loop again and widens
    /// further ([`Reach::Havoc`] there).
    Joined,
    /// Writes no pass checks: a summary toward the loop's entry or one cut
    /// short, a call summarized by its effect, a group end not followed.
    /// An unseen write may be any text of its kind, so only the class
    /// survives, or the one text all members are.
    Havoc,
}

/// Widening at a loop head. A slot the loop still changes after as many
/// passes as widening waits for may hold anything of its kind from then on:
/// an unknown meaning (a switch stays an undecided switch) and an unknown
/// value. No later pass can grow past that. At a loop head ([`Reach`]),
/// texts with the same operators between digit runs keep them, one hull
/// per run ([`shaped`]); others keep their texts with every run of digits
/// as one `digits` token ([`numbers`]), as long as that leaves at most
/// `sets.meanings` texts: a loop that appends digits to a number stays a
/// number.
pub fn widen_slot(
    old: &Option<Binding>,
    new: Option<Binding>,
    sets: SetLimits,
    typed: &Typed,
    reach: Reach,
) -> Option<Binding> {
    if same_slot(old, &new) {
        return new;
    }
    let (Some(x), Some(y)) = (old, &new) else { return join(old.clone(), new, sets, Some(typed)) };
    let (meaning_grows, value_grows) = (!same_meaning(x, y), !same_value(&x.value, &y.value));
    let (xv, yv) = (x.value.num(), y.value.num());
    let toks = typed_toks_join(&[&x.value, &y.value], typed, true);
    let mut joined = join(old.clone(), new, sets, Some(typed))?;
    // Only `undefined` joined the meanings the head had: nothing grows past
    // this join, so it is its own widening, and its code keeps its texts.
    if meaning_grows && defined_grows(x, &joined) {
        use crate::engine::builtins::{Cond, Primitive};
        let switch = matches!(joined.meaning.prim(), Some(Primitive::If(_)));
        // The texts are kept only while they are no more than the head
        // had: a set that grows every pass is not a widening, and its
        // hull is.
        let joins = reach == Reach::Joined;
        let had = numbers(x, sets, typed, joins).map_or(usize::MAX, |t| t.len());
        // Lists of names (`\@elt\bx@A…`, LaTeX's float queues) move names
        // between them: their sets stay exact up to the bound, then hull.
        let names = name_lists(&joined, typed);
        let texts = || numbers(&joined, sets, typed, joins).filter(|t| names || t.len() <= had);
        // The run structure first: `numbers` would make the `1` of `+1` digits.
        let ops = || joins.then(|| hull_by(&joined, &|t| shaped(t, typed))).flatten();
        (joined.meaning, joined.may) = match (switch, ops()) {
            (true, _) => (Meaning::Primitive(Primitive::If(Cond::Opaque)), None),
            (_, Some(hull)) => hull,
            _ => match texts() {
                Some(mut texts) if texts.len() == 1 => (texts.pop().expect("one text"), None),
                Some(texts) => (Meaning::Unknown, Some(texts.into())),
                // Macros that take arguments are code, whose texts no pass makes
                // up: the set stays what the join kept, each read with its arguments.
                None if takes_arguments(&joined) => (joined.meaning.clone(), joined.may.clone()),
                None => hull_meaning(&joined, typed, reach, sets.text_names),
            },
        };
    }
    if value_grows {
        // A bound that still moves goes to the end of the kind's range.
        joined.value = match (xv, yv) {
            // An empty hull (a family no write reached yet) grows by joining.
            (Some((a, p)), Some((b, q))) if a == b && p.lo > p.hi => Value::Num { quantity: a, num: p.join(&q) },
            (Some((a, p)), Some((b, q))) if a == b => Value::Num { quantity: a, num: p.widen(&p.join(&q)) },
            // Token lists keep their common ends around typed text, whose
            // class only grows: a pgfmath stack keeps its braces.
            // A few token lists stay a set, each one read on its own path.
            _ => match toks {
                Some(toks) => Value::Toks(toks),
                None if joined.value.token_lists().is_some_and(|l| l.len() > 1) => joined.value,
                None => Value::Unknown,
            },
        };
    }
    Some(joined)
}

/// Whether `new` may have a defined meaning `old` may not.
fn defined_grows(old: &Binding, new: &Binding) -> bool {
    let members = |b: &Binding| b.may.as_deref().map_or_else(|| vec![b.meaning.clone()], <[Meaning]>::to_vec);
    let had = members(old);
    members(new).iter().any(|m| *m != Meaning::Undefined && !had.iter().any(|h| one_meaning(h, m)))
}

/// Whether `b` is a set of meanings, each a macro with parameters.
fn takes_arguments(b: &Binding) -> bool {
    b.may.is_some() && is_code(b)
}

/// Every meaning `b` may have is a macro that takes arguments: code, whose
/// texts no pass makes up ([`widen_slot`]).
pub fn is_code(b: &Binding) -> bool {
    let code = |m: &Meaning| m.as_macro().is_some_and(|d| !d.parameter_text.items.is_empty());
    b.may.as_deref().map_or(code(&b.meaning), |may| may.iter().all(code))
}

/// Whether every meaning `b` may have is a parameterless macro whose text
/// is known control sequences only: a list of names, as a float queue.
fn name_lists(b: &Binding, typed: &Typed) -> bool {
    let members = b.may.as_deref().unwrap_or(std::slice::from_ref(&b.meaning));
    members.iter().all(|m| {
        m.as_macro().is_some_and(|d| {
            d.parameter_text.items.is_empty()
                && !d.parameter_text.brace_end
                && d.replacement_text.iter().all(|t| t.cs().is_some() && typed.classify(t).is_none())
        })
    })
}

type Hulled = (Meaning, Option<std::rc::Rc<[Meaning]>>);

/// The meaning and set a widened binding keeps: the macros' hull, still
/// perhaps undefined where the set held that.
fn hull_meaning(b: &Binding, typed: &Typed, reach: Reach, limit: usize) -> Hulled {
    let hull = |t: &[&[Token]]| names_hull(t, typed, limit).or_else(|| Some(text_hull(t, typed, reach)));
    hull_by(b, &hull).unwrap_or((Meaning::Unknown, None))
}

/// Meanings a family of names was given, widened: two or more parameterless
/// macros of like prefixes are their hull ([`hull_meaning`]), so texts that
/// grow per pass (a note per record read) stay one meaning. `None` where
/// `ms` are no such macros.
pub fn macros_hull(ms: &[Meaning], typed: &Typed, limit: usize) -> Option<Meaning> {
    if ms.len() < 2 {
        return None;
    }
    let b = Binding { may: Some(ms.into()), ..Binding::builtin(Meaning::Unknown) };
    let hull = |t: &[&[Token]]| names_hull(t, typed, limit).or_else(|| Some(text_hull(t, typed, Reach::Havoc)));
    macro_hull(&b, &hull)
}

/// Lists of names (`\@elt\bx@A\@elt\bx@B`) as one typed text that may
/// hold their names, in any order and number: a float queue keeps which
/// boxes it holds. `None` for other texts or past `limit` names.
fn names_hull(texts: &[&[Token]], typed: &Typed, limit: usize) -> Option<Vec<Token>> {
    let (names, one) = typed.names_of(texts, limit)?;
    let span = texts.iter().find_map(|t| t.first()).map_or_else(Span::default, |t| t.span);
    Some(vec![Token::new(Tok::Cs(typed.named(one, &names)), span)])
}

/// How texts are hulled into one: `None` where they are refused.
type TextHull<'a> = dyn Fn(&[&[Token]]) -> Option<Vec<Token>> + 'a;

/// [`hull_meaning`] with the texts' hull `hull`: `None` where `b` is no
/// such macro or `hull` refuses its texts.
fn hull_by(b: &Binding, hull: &TextHull<'_>) -> Option<Hulled> {
    let m = macro_hull(b, hull)?;
    Some(match b.may.as_deref().is_some_and(|m| m.contains(&Meaning::Undefined)) {
        true => (Meaning::Unknown, Some(vec![Meaning::Undefined, m].into())),
        false => (m, None),
    })
}

/// A macro without parameters whose text is any text of the classes its
/// texts have, when every defined meaning `b` may have is such a macro
/// with the same prefixes: widened, it stays one, which `\ifx` can still
/// tell from other kinds. Its length is bounded only where all texts have
/// one length: a widened write that repeats changes it.
fn macro_hull(b: &Binding, hull: &TextHull<'_>) -> Option<Meaning> {
    let members = b.may.as_deref().unwrap_or(std::slice::from_ref(&b.meaning));
    let members: Vec<&Meaning> = members.iter().filter(|m| **m != Meaning::Undefined).collect();
    let mut first: Option<&crate::engine::tex::MacroDef> = None;
    for m in &members {
        let Meaning::Macro(d) = m else { return None };
        let plain = d.plain();
        let f = *first.get_or_insert(d);
        if !plain || (d.long, d.outer, d.protected) != (f.long, f.outer, f.protected) {
            return None;
        }
    }
    let def = first?;
    let texts = members.iter().filter_map(|m| match m {
        Meaning::Macro(d) => Some(&d.replacement_text[..]),
        _ => None,
    });
    let texts: Vec<&[Token]> = texts.collect();
    let text: std::rc::Rc<[Token]> = hull(&texts)?.into();
    Some(Meaning::Macro(std::rc::Rc::new(crate::engine::tex::MacroDef { replacement_text: text, ..def.clone() })))
}

/// Typed text for any text of the classes `texts` have: at a loop head
/// their [`shaped`] hull where they have one, else one token (text
/// inserted between operators is a text that grew, and a summary's write
/// may be any text of its kind).
fn text_hull(texts: &[&[Token]], typed: &Typed, reach: Reach) -> Vec<Token> {
    let shape = (reach == Reach::Joined).then(|| shaped(texts, typed)).flatten();
    shape.unwrap_or_else(|| {
        let texts: Vec<Vec<Token>> = texts.iter().map(|t| unslice(t, typed.interner)).collect();
        run_hull(&texts.iter().map(Vec::as_slice).collect::<Vec<_>>(), typed, false)
    })
}

/// Several texts with the same operators between typed runs at the same
/// places, joined: the operators stay, one hull per run, so `+1` never
/// folds into the digits before it, and a `-` that begins a run is its
/// sign. Sound only where `texts` are all a reaching path may hold
/// ([`Reach::Joined`]). `None` for texts of other shapes.
fn shaped(texts: &[&[Token]], typed: &Typed) -> Option<Vec<Token>> {
    let texts: Vec<Vec<Token>> = texts.iter().map(|t| unslice(t, typed.interner)).collect();
    let lits = |t: &[Token]| {
        let mut lit: Vec<bool> = Vec::with_capacity(t.len());
        for (i, x) in t.iter().enumerate() {
            let sign = x.tok == Tok::Chr('-', Catcode::Other)
                && (i == 0 || lit[i - 1])
                && t.get(i + 1).is_some_and(|n| typed.classify(n).is_some());
            lit.push(typed.classify(x).is_none() && !sign);
        }
        lit
    };
    let op = |t: &Token| matches!(t.tok, Tok::Chr(c, Catcode::Other) if !c.is_ascii_digit());
    // The runs between operators and the operators, in order.
    let parts = |t: &[Token]| {
        let lit = lits(t);
        let (mut runs, mut ops, mut run) = (Vec::new(), Vec::new(), Vec::new());
        for (x, l) in t.iter().zip(lit) {
            if l {
                runs.push(std::mem::take(&mut run));
                ops.push(*x);
            } else {
                run.push(*x);
            }
        }
        runs.push(run);
        (runs, ops)
    };
    let parted: Vec<(Vec<Vec<Token>>, Vec<Token>)> = texts.iter().map(|t| parts(t)).collect();
    let shape = |(runs, ops): &(Vec<Vec<Token>>, Vec<Token>)| {
        (ops.iter().map(|t| t.tok).collect::<Vec<_>>(), runs.iter().map(Vec::is_empty).collect::<Vec<_>>())
    };
    let first = shape(&parted[0]);
    let kept = texts.len() > 1 && !first.0.is_empty() && parted[0].1.iter().all(op);
    if !kept || parted.iter().any(|p| shape(p) != first) {
        return None;
    }
    let mut out = Vec::new();
    for (i, l) in parted[0].1.iter().map(Some).chain([None]).enumerate() {
        if !parted[0].0[i].is_empty() {
            out.extend(run_hull(&parted.iter().map(|(r, _)| &r[i][..]).collect::<Vec<_>>(), typed, true));
        }
        out.extend(l.copied());
    }
    Some(out)
}

/// Typed text for any text of the classes `texts` have, at least one
/// token when each has one. Texts all of one bounded length keep it: no
/// write grew them. Several texts that are one printed number stay it.
/// Between operators (`run`), texts alike stay as they are, and printed
/// integers stay printed integers ([`printed_hull`]).
fn run_hull(texts: &[&[Token]], typed: &Typed, run: bool) -> Vec<Token> {
    if let [[t], rest @ ..] = texts
        && !rest.is_empty()
        && t.cs().is_some_and(|s| typed.interner.name(s).starts_with(crate::engine::scan::TYPED_DIGITS))
        && rest.iter().all(|r| matches!(r, [u] if u.tok == t.tok))
    {
        return vec![*t];
    }
    if run && texts.iter().all(|t| t.iter().map(|x| x.tok).eq(texts[0].iter().map(|x| x.tok))) {
        return texts[0].to_vec();
    }
    if let Some(text) = printed_hull(texts, typed).filter(|_| run) {
        return text;
    }
    let (mut one, mut class, mut span) = (true, TextClass::EMPTY, None);
    let mut lens: Vec<Option<u32>> = Vec::new();
    for text in texts {
        one &= typed.holds_one(text);
        let len = |t: &Token| text_len(typed.interner, t).or(matches!(t.tok, Tok::Chr(..)).then_some(1));
        lens.push(text.iter().try_fold(0, |n, t| Some(n + len(t)?)));
        for t in *text {
            span = span.or(Some(t.span));
            class = class.union(typed.class_of(t).map_or_else(|| TextClass::of(t.tok), |c| c.0));
        }
    }
    let len = lens.first().copied().flatten().filter(|n| *n > 0 && lens.iter().all(|l| *l == Some(*n)));
    vec![Token::new(Tok::Cs(typed.make(class, one, len)), span.unwrap_or_default())]
}

/// Integer digits as `\the` prints them, after a `-` where it may be
/// negative (tex.web §§ 65, 103): whether it may be negative, holds at
/// least one digit, and the most digits it has. `None` for other text,
/// or digits of no bound.
fn printed(text: &[Token], typed: &Typed) -> Option<(bool, bool, u32)> {
    use crate::engine::tex::{UNKNOWN_CHAR, UNKNOWN_DIGIT, UNKNOWN_NONZERO};
    let name = |t: &Token| t.cs().map(|s| typed.interner.name(s));
    // A whole typed number: its interval, sign included.
    let whole = |t: &Token| {
        let (lo, hi) = name(t)?.strip_prefix(crate::engine::scan::TYPED_DIGITS)?.split_once('#')?.0.split_once("..")?;
        Some((lo.parse::<i64>().ok()?, hi.parse::<i64>().ok()?))
    };
    let lead = |t: &Token| {
        name(t).filter(|n| n.contains(crate::analysis::machine::LEAD_MARK))?;
        Some((typed.classify(t)?.1, text_len(typed.interner, t)?))
    };
    let digits = |n: i64| n.unsigned_abs().checked_ilog10().unwrap_or(0) + 1;
    let other = |c: char| Tok::Chr(c, Catcode::Other);
    let (neg, text) = match text {
        [t, rest @ ..] if t.tok == other('-') => (true, rest),
        // A sign or first digit, the digits after it (`unknown_number`).
        [t, l] if t.tok == other(UNKNOWN_CHAR) => return lead(l).filter(|l| !l.0).map(|(_, n)| (true, true, n)),
        _ => (false, text),
    };
    match text {
        [t] if matches!(t.tok, Tok::Chr('0' | UNKNOWN_DIGIT, Catcode::Other)) => Some((neg, true, 1)),
        // After a `-`, at least one digit.
        [t] if let Some((one, n)) = lead(t) => (one || !neg).then_some((neg, one, n)),
        [t] if let Some((lo, hi)) = whole(t) => {
            (!neg || lo >= 0).then(|| (neg || lo < 0, true, digits(lo).max(digits(hi))))
        }
        [f, rest @ ..] => {
            let nonzero = matches!(f.tok, Tok::Chr('1'..='9' | UNKNOWN_NONZERO, Catcode::Other))
                || whole(f).is_some_and(|(lo, _)| lo >= 1);
            let len = |t: &Token| match t.tok {
                Tok::Chr('0'..='9' | UNKNOWN_DIGIT, Catcode::Other) => Some(1),
                _ if typed.classify(t)?.0 == TextClass::DIGIT && whole(t).is_none_or(|(lo, _)| lo >= 0) => {
                    text_len(typed.interner, t)
                }
                _ => None,
            };
            let n = text.iter().try_fold(0, |n, t| Some(n + len(t)?))?;
            (nonzero && rest.iter().all(|t| lead(t).is_none())).then_some((neg, true, n))
        }
        [] => None,
    }
}

/// The printed integer any of `texts` is ([`printed`]): unknown digits as
/// `\the` prints them, the sign one unknown character before them where
/// one may be negative ([`crate::analysis::machine::LEAD_MARK`]). `None`
/// when one is no such text or the digits pass [`MAX_LEN`].
fn printed_hull(texts: &[&[Token]], typed: &Typed) -> Option<Vec<Token>> {
    let (mut neg, mut one, mut len) = (false, true, 0);
    for text in texts {
        let (n, o, l) = printed(text, typed)?;
        (neg, one, len) = (neg || n, one && o, len.max(l));
    }
    let span = texts.iter().find_map(|t| t.first())?.span;
    let lead = |one| Some(Token::new(Tok::Cs(typed.syms[lead_index(one, len)?]), span));
    Some(match neg {
        false => vec![lead(one)?],
        true if one => vec![Token::new(Tok::Chr(crate::engine::tex::UNKNOWN_CHAR, Catcode::Other), span), lead(false)?],
        true => return None,
    })
}

/// `text` with each run of slices that spells all places of a typed
/// number, in order, as that number: a path that read it apart and one
/// that did not then hold one text.
fn unslice(text: &[Token], interner: &Interner) -> Vec<Token> {
    let slice = |t: &Token| crate::engine::scan::slice_of(interner.name(t.cs()?));
    use crate::engine::scan::{complete, continues};
    let mut out = Vec::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        // As a scan reads them back ([`continues`]): each slice goes on
        // where the last stopped, so `n` moves on every step.
        if let Some((mut k, 0, mut j, w)) = slice(&text[i]) {
            let mut n = i + 1;
            while !complete(k, j)
                && let Some((l, a, e, v)) = text.get(n).and_then(slice)
                && v == w
                && continues((k, j), (l, a))
            {
                (k, j, n) = (k.max(l), e, n + 1);
            }
            if complete(k, j)
                && let Some(w) = interner.lookup(&format!("{}{w}", crate::engine::scan::TYPED_DIGITS))
            {
                out.push(Token::new(Tok::Cs(w), text[i].span));
                i = n;
                continue;
            }
        }
        out.push(text[i]);
        i += 1;
    }
    out
}

/// The meanings `b` may have, each a macro without parameters, with every
/// digit run in their texts made one `digits` token: `None` when one is not
/// such a macro or there are more than `sets.meanings` texts. Without
/// `joins` ([`Reach::Havoc`]) only where all are one text.
fn numbers(b: &Binding, sets: SetLimits, typed: &Typed, joins: bool) -> Option<Vec<Meaning>> {
    let members = match (&b.may, &b.meaning) {
        (Some(set), _) => set.to_vec(),
        (None, m) => vec![m.clone()],
    };
    let (mut texts, mut wholes): (Vec<Meaning>, Vec<Meaning>) = (Vec::new(), Vec::new());
    for m in members {
        let Meaning::Macro(def) = m else { return None };
        if !def.parameter_text.items.is_empty() || def.parameter_text.brace_end {
            return None;
        }
        let whole = unslice(&def.replacement_text, typed.interner);
        let text = unknown_runs(&whole, typed);
        for (set, text) in [(&mut wholes, whole.into()), (&mut texts, text)] {
            let m = Meaning::Macro(std::rc::Rc::new(crate::engine::tex::MacroDef {
                replacement_text: text,
                ..(*def).clone()
            }));
            if !set.contains(&m) {
                set.push(m);
            }
        }
    }
    // Texts that differ only in how far a number was read apart are one.
    if wholes.len() == 1 {
        return Some(wholes);
    }
    (joins && texts.len() <= sets.meanings).then_some(texts)
}

/// The names of typed unknown text, for widening without the machine:
/// what a token stands for, and the token for a class.
/// What parts a typed name from its bound: the most tokens it may hold.
pub const LEN_MARK: char = '\u{5}';
/// Before the class a one-of token's name spells
/// ([`Typed::one_of_class`]): no gap, so the name reads as no partly unknown one.
pub const CLASS_MARK: char = '\u{8}';
/// The longest bound a typed name carries: a sign and 19 digits.
pub const MAX_LEN: u32 = 20;
/// The classes whose text carries a bound: what `\the` and `\number`
/// print (tex.web §§ 65, 103), what `\string` prints (an unknown
/// `\escapechar` is at most one character, tex.web § 63), and hex
/// digits as letters and as printed (`\int_to_Hex:n`).
pub const BOUNDED: [TextClass; 5] = [
    TextClass::DIGIT,
    TextClass::NUMBER,
    TextClass::PRINTED,
    TextClass(TextClass::DIGIT.0 | TextClass::LETTER.0 | TextClass::HEX.0),
    TextClass(TextClass::DIGIT.0 | TextClass::OTHER.0 | TextClass::HEX.0),
];

/// Where the typed name of `class`, lower bound `one`, at most `len`
/// tokens and `serial` (0 the canonical one) stands in the typed names.
pub fn bounded_index(class: TextClass, one: bool, len: u32, serial: usize) -> Option<usize> {
    let b = BOUNDED.iter().position(|c| *c == class)?;
    let serials = crate::analysis::machine::REST_SERIALS + 1;
    (1..=MAX_LEN).contains(&len).then(|| {
        crate::analysis::machine::BOUNDED_BASE
            + (((b * 2 + usize::from(one)) * MAX_LEN as usize + len as usize - 1) * serials + serial)
    })
}

/// Where the printed integers ([`crate::analysis::machine::LEAD_MARK`])
/// stand in the typed names, after the bounded ones: per lower bound and
/// bound, one name each, as no test narrows them.
const LEAD_BASE: usize = crate::analysis::machine::BOUNDED_BASE
    + BOUNDED.len() * 2 * MAX_LEN as usize * (crate::analysis::machine::REST_SERIALS + 1);
/// How many typed names the machine makes.
pub const TYPED_NAMES: usize = LEAD_BASE + 2 * MAX_LEN as usize;

/// Where the printed integer of at most `len` digits, at least one where
/// `one`, stands in the typed names.
pub fn lead_index(one: bool, len: u32) -> Option<usize> {
    (1..=MAX_LEN).contains(&len).then(|| LEAD_BASE + usize::from(one) * MAX_LEN as usize + len as usize - 1)
}

/// The most tokens unknown text `t` may hold: one for an unknown
/// character, the bound a typed name carries, or the length of the
/// spelling of the interval typed digits lie in. Known digits have none:
/// a loop's counter is any digits, whatever its length.
pub fn text_len(interner: &Interner, t: &Token) -> Option<u32> {
    let name = match t.tok {
        Tok::Chr(c, _) if crate::engine::tex::is_unknown_char(c) => return Some(1),
        Tok::Cs(sym) => interner.name(sym),
        _ => return None,
    };
    if let Some((_, len)) = name.rsplit_once(LEN_MARK) {
        return len.parse().ok();
    }
    let typed = name.strip_prefix(crate::engine::scan::TYPED_DIGITS)?;
    // A slice `[i, j)` with its end known; one to the end (`j` 0) is
    // bounded by its interval, as other typed digits are.
    if let Some((_, i, j, _)) = crate::engine::scan::slice_of(name)
        && j > i
    {
        return Some(j - i);
    }
    let (lo, hi) = typed.split(['@', '#']).next()?.split_once("..")?;
    let (lo, hi): (i64, i64) = (lo.parse().ok()?, hi.parse().ok()?);
    let digits = |n: i64| n.unsigned_abs().checked_ilog10().unwrap_or(0) + 1;
    Some(digits(lo).max(digits(hi)) + u32::from(lo < 0))
}

/// [`Typed`] of machine `$m` over `$syms` (its `typed_syms`); a macro so
/// the borrow of `$m` splits by field.
macro_rules! typed {
    ($m:expr, $syms:expr) => {
        crate::engine::env::Typed {
            interner: &$m.out.interner,
            syms: &$syms,
            digits: $m.unknown_digits,
            more: $m.unknown_more,
            minted: &$m.minted,
        }
    };
}
pub(crate) use typed;

pub struct Typed<'a> {
    pub interner: &'a Interner,
    /// The name per class and lower bound, `class * 2 + one`.
    pub syms: &'a [Sym],
    pub digits: Sym,
    pub more: Sym,
    /// Typed names made here, not yet interned: the machine interns them
    /// in order after the env call ([`crate::analysis::machine::Machine::mint`]).
    pub minted: &'a std::cell::RefCell<Vec<String>>,
}

/// The name of typed text of `class`, at least one token where `one`,
/// that may hold the control sequences `names` (sorted, each once).
pub fn named_text(class: TextClass, one: bool, names: impl Iterator<Item = impl AsRef<str>>) -> String {
    use crate::analysis::machine::{TYPED_TEXT, UNKNOWN_NAME, UNKNOWN_NAME_GAP};
    let mut name = format!("{UNKNOWN_NAME}{TYPED_TEXT}{}{}", class.spell(), if one { '+' } else { '*' });
    for n in names {
        name.push(UNKNOWN_NAME_GAP);
        name.push_str(n.as_ref());
    }
    name
}

impl Typed<'_> {
    /// Typed text that may hold `names` and nothing else, at least one
    /// token where `one`: a name the interner has, else one minted.
    pub fn named(&self, one: bool, names: &std::collections::BTreeSet<String>) -> Sym {
        let name = named_text(TextClass::EMPTY, one, names.iter());
        if let Some(sym) = self.interner.lookup(&name) {
            return sym;
        }
        let mut minted = self.minted.borrow_mut();
        let at = minted.iter().position(|n| *n == name).unwrap_or_else(|| {
            minted.push(name);
            minted.len() - 1
        });
        Sym((self.interner.len() + at) as u32)
    }

    /// The class, lower bound and control sequences a typed name spells,
    /// `None` for any other name.
    pub fn parse_name(name: &str) -> Option<(TextClass, bool, Vec<&str>)> {
        let name = name.split_once(crate::analysis::machine::REST_SERIAL).map_or(name, |(name, _)| name);
        if name == crate::analysis::machine::MORE_DIGITS {
            return Some((TextClass::DIGIT, false, Vec::new()));
        }
        let rest = name
            .strip_prefix(crate::analysis::machine::UNKNOWN_NAME)?
            .strip_prefix(crate::analysis::machine::TYPED_TEXT)?;
        let mut parts = rest.split(crate::analysis::machine::UNKNOWN_NAME_GAP);
        let head = parts.next()?;
        let (spelled, one) = head.split_at(head.len().checked_sub(1)?);
        Some((TextClass::parse(spelled)?, one == "+", parts.collect()))
    }

    /// What `t` stands for when it is unknown text, one unknown character
    /// or a known digit: its class and whether it is at least one token.
    pub fn classify(&self, t: &Token) -> Option<(TextClass, bool)> {
        match t.tok {
            Tok::Chr(c, Catcode::Other) if c.is_ascii_digit() => Some((TextClass::DIGIT, true)),
            Tok::Chr(c, _) if crate::engine::tex::is_unknown_char(c) => Some((TextClass::of(t.tok), true)),
            Tok::Cs(sym) if sym == self.digits => Some((TextClass::DIGIT, true)),
            Tok::Cs(sym) if sym == self.more => Some((TextClass::DIGIT, false)),
            Tok::Cs(sym) => {
                let name = self.interner.name(sym);
                if name.starts_with(crate::engine::scan::TYPED_DIGITS) {
                    return Some((TextClass::DIGIT, true));
                }
                // Text that may hold named control sequences is left as it
                // is: a run cannot be named here.
                match Self::parse_name(name) {
                    Some((class, one, cs)) if cs.is_empty() => Some((class, one)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// The class of the texts a one-of token ([`crate::analysis::machine::Machine::one_of`])
    /// stands for, which its name spells after its [`CLASS_MARK`].
    pub fn one_of_class(&self, t: &Token) -> Option<(TextClass, bool)> {
        let name = self.interner.name(t.cs()?);
        let rest = name
            .strip_prefix(crate::analysis::machine::UNKNOWN_NAME)?
            .strip_prefix(crate::analysis::machine::explore::ONE_OF)?;
        let (_, head) = rest.rsplit_once(CLASS_MARK)?;
        let head = head.split(crate::analysis::machine::UNKNOWN_NAME_GAP).next()?;
        let (spelled, one) = head.split_at(head.len().checked_sub(1)?);
        Some((TextClass::parse(spelled)?, one == "+"))
    }

    /// The names a one-of token of lists of names spells after its class
    /// ([`Typed::names_of`]): `None` for other tokens.
    pub fn one_of_names(&self, t: &Token) -> Option<Vec<&str>> {
        let name = self.interner.name(t.cs()?);
        let rest = name
            .strip_prefix(crate::analysis::machine::UNKNOWN_NAME)?
            .strip_prefix(crate::analysis::machine::explore::ONE_OF)?;
        let (_, head) = rest.rsplit_once(CLASS_MARK)?;
        let names: Vec<&str> = head.split(crate::analysis::machine::UNKNOWN_NAME_GAP).skip(1).collect();
        (!names.is_empty()).then_some(names)
    }

    /// The names texts of names only may hold, and whether every text holds
    /// one: plain names, typed text of names ([`named_text`]) and one-of
    /// tokens of such lists. `None` for other texts or past `limit` names.
    pub fn names_of(&self, texts: &[&[Token]], limit: usize) -> Option<(std::collections::BTreeSet<String>, bool)> {
        let (mut names, mut one) = (std::collections::BTreeSet::new(), true);
        for text in texts {
            let mut some = false;
            for t in *text {
                let sym = t.cs()?;
                if let Some(cs) = self.one_of_names(t) {
                    names.extend(cs.into_iter().map(str::to_owned));
                    some |= self.one_of_class(t)?.1;
                    continue;
                }
                // Made by this env call: not interned yet.
                let minted = (sym.0 as usize)
                    .checked_sub(self.interner.len())
                    .and_then(|k| self.minted.borrow().get(k).cloned());
                let name = minted.as_deref().unwrap_or_else(|| self.interner.name(sym));
                match Self::parse_name(name) {
                    Some((class, o, cs)) if class == TextClass::EMPTY && !cs.is_empty() => {
                        names.extend(cs.into_iter().map(str::to_owned));
                        some |= o;
                    }
                    // A marker stands for text, not for itself.
                    _ if name.starts_with(|c: char| c < ' ') => return None,
                    _ => {
                        names.insert(name.to_owned());
                        some = true;
                    }
                }
            }
            one &= some;
        }
        (!names.is_empty() && names.len() <= limit).then_some((names, one))
    }

    /// Whether `text` holds at least one token: some token of it is not
    /// typed text that may be empty.
    pub fn holds_one(&self, text: &[Token]) -> bool {
        text.iter().any(|t| self.class_of(t).is_none_or(|(_, one)| one))
    }

    /// The class of typed text or of one of several texts `t` stands for,
    /// and whether it holds at least one token.
    pub fn class_of(&self, t: &Token) -> Option<(TextClass, bool)> {
        self.classify(t).or_else(|| self.one_of_class(t))
    }

    /// The token for typed text, none for text of no class that may be
    /// empty: that is only the empty text. A token for it would be one
    /// `\@elt` delimiter never matches (a float list it heads never pops).
    pub fn text(&self, class: TextClass, one: bool, len: Option<u32>, span: Span) -> Option<Token> {
        (class != TextClass::EMPTY || one).then(|| Token::new(Tok::Cs(self.make(class, one, len)), span))
    }

    pub fn make(&self, class: TextClass, one: bool, len: Option<u32>) -> Sym {
        if let Some(i) = len.and_then(|len| bounded_index(class, one, len, 0)) {
            return self.syms[i];
        }
        match (class, one) {
            (TextClass::DIGIT, true) => self.digits,
            (TextClass::DIGIT, false) => self.more,
            _ => self.syms[class.0 as usize * 2 + usize::from(one)],
        }
    }
}

/// `text` with every run of unknown text (digits `0`–`9` of category 12,
/// unknown characters, typed unknown text) as one typed token of the class
/// the run spans, at least one token when any part of the run is (tex.web
/// § 465 prints a number as such characters).
pub fn unknown_runs(text: &[Token], typed: &Typed) -> std::rc::Rc<[Token]> {
    let mut out: Vec<Token> = Vec::with_capacity(text.len());
    let mut run: Option<(TextClass, bool, Option<u32>, Span)> = None;
    let flush = |run: &mut Option<(TextClass, bool, Option<u32>, Span)>, out: &mut Vec<Token>| {
        if let Some((class, one, len, span)) = run.take() {
            out.extend(typed.text(class, one, len, span));
        }
    };
    for t in text {
        match typed.classify(t) {
            Some((class, one)) => {
                let len = text_len(typed.interner, t);
                run = Some(match run {
                    Some((c, o, l, span)) => (c.union(class), o || one, l.zip(len).map(|(a, b)| a + b), span),
                    None => (class, one, len, t.span),
                });
            }
            None => {
                flush(&mut run, &mut out);
                out.push(*t);
            }
        }
    }
    flush(&mut run, &mut out);
    out.into()
}

/// [`join`] for the loop heads in [`crate::analysis::machine`].
/// `binding` with any value, and where it held a macro the [`macro_hull`]
/// of its meanings, else any meaning.
fn widened(mut binding: Binding, typed: &Typed, limit: usize) -> Binding {
    binding.value = Value::Unknown;
    if !is_code(&binding) && (matches!(binding.meaning, Meaning::Macro(_)) || binding.may.is_some()) {
        (binding.meaning, binding.may) = hull_meaning(&binding, typed, Reach::Havoc, limit);
    }
    binding.certain = false;
    binding
}

pub fn join_slot(a: Option<Binding>, b: Option<Binding>, sets: SetLimits) -> Option<Binding> {
    join(a, b, sets, None)
}

/// Least upper bound of two possible bindings: a name the paths gave
/// different meanings has one of them, up to `sets.meanings` of them.
fn join(a: Option<Binding>, b: Option<Binding>, sets: SetLimits, typed: Option<&Typed>) -> Option<Binding> {
    match (a, b) {
        (None, None) => None,
        // A control sequence the other path never defined is undefined
        // there (a fresh register slot means `Unknown`). It keeps the
        // meaning it has where it is defined, which is where it is used,
        // and a test of whether it is defined sees both (`may`).
        (Some(x), None) | (None, Some(x))
            if x.may.is_some() || !matches!(x.meaning, Meaning::Unknown | Meaning::Undefined) =>
        {
            let mut may: Vec<Meaning> = x.may.as_deref().map_or_else(|| vec![x.meaning.clone()], <[Meaning]>::to_vec);
            if !may.contains(&Meaning::Undefined) {
                may.push(Meaning::Undefined);
            }
            let meaning = if may.len() > sets.meanings + 1 { Meaning::Unknown } else { x.meaning.clone() };
            let may = (may.len() <= sets.meanings + 1).then(|| may.into());
            Some(Binding { meaning, may, certain: false, value: Value::Unknown, ..x })
        }
        // A register or code never assigned on the other path holds its
        // default there, which is not known here.
        (Some(x), None) | (None, Some(x)) => Some(Binding { certain: false, value: Value::Unknown, ..x }),
        (Some(x), Some(y)) => {
            // The meanings either path may have given the name.
            // `None` when one of them is not known.
            let members = |b: &Binding| match (&b.may, &b.meaning) {
                (Some(set), _) => Some(set.to_vec()),
                (None, Meaning::Unknown) => None,
                (None, m) => Some(vec![m.clone()]),
            };
            let set = match (members(&x), members(&y)) {
                (Some(mut a), Some(b)) => {
                    for m in b {
                        if !a.contains(&m) {
                            a.push(m);
                        }
                    }
                    Some(a)
                }
                _ => None,
            };
            // Top on one side only is not the other side's set (D1).
            let same = x.meaning == y.meaning
                && set.as_ref().map_or(members(&x).is_none() && members(&y).is_none(), |s| s.len() == 1);
            let mut defs = x.defs;
            defs.extend(y.defs.iter().copied());
            defs.sort_unstable();
            defs.dedup();
            let mut value = x.value.clone().join(&y.value, sets);
            if matches!(value, Value::Unknown)
                && let Some(typed) = typed
                && let Some(text) = typed_toks_join(&[&x.value, &y.value], typed, false)
            {
                value = Value::Toks(text);
            }
            // A switch set on one path and not on another is still a
            // conditional, only an undecided one. So is a join of switches.
            use crate::engine::builtins::{Cond, Primitive};
            let switch = |m: &Meaning| matches!(m.prim(), Some(Primitive::If(Cond::True | Cond::False | Cond::Opaque)));
            let (meaning, may) = match (same, switch(&x.meaning) && switch(&y.meaning), set) {
                (true, ..) => (x.meaning, x.may),
                (false, true, _) => (Meaning::Primitive(Primitive::If(Cond::Opaque)), None),
                // One meaning, or undefined: read with that meaning, as the
                // one-sided join above leaves it.
                (false, false, Some(set)) if maybe_undefined(&set).is_some() => {
                    (maybe_undefined(&set).expect("checked").clone(), Some(set.into()))
                }
                (false, false, Some(set)) if set.len() <= sets.meanings => (Meaning::Unknown, Some(set.into())),
                // Past the limit, macros whose texts differ in balanced
                // text without control sequences are one macro with that
                // part typed unknown text: what they may hold stays known.
                (false, false, Some(set)) => match typed.and_then(|typed| typed_join(&set, typed, false)) {
                    Some(meaning) => (meaning, None),
                    // Lists of names: one list of the names they hold.
                    None => typed
                        .and_then(|typed| {
                            let b = Binding { may: Some(set.into()), ..Binding::builtin(Meaning::Unknown) };
                            hull_by(&b, &|t| names_hull(t, typed, sets.text_names))
                        })
                        .unwrap_or((Meaning::Unknown, None)),
                },
                (false, false, _) => (Meaning::Unknown, None),
            };
            Some(Binding { meaning, may, value, defs, certain: x.certain && y.certain, global: x.global && y.global })
        }
    }
}

/// Whether a saved slot is a control sequence's meaning, not a register's
/// value (a fresh register slot means `Unknown`).
fn is_name(old: &Option<Binding>) -> bool {
    old.as_ref().is_none_or(|b| b.may.is_some() || !matches!(b.meaning, Meaning::Unknown))
}

/// The one meaning of `set` besides being undefined, when that is all it holds.
fn maybe_undefined(set: &[Meaning]) -> Option<&Meaning> {
    match set {
        [Meaning::Undefined, m] | [m, Meaning::Undefined] if !matches!(m, Meaning::Unknown | Meaning::Undefined) => {
            Some(m)
        }
        _ => None,
    }
}

/// How many tokens all `texts` share at their beginning and at their end,
/// the end not reaching into the beginning. Widened, typed text next to the
/// middle joins it: the next pass then meets one typed token, not one more
/// beside it.
fn common_ends(texts: &[&[Token]], typed: &Typed, widen: bool) -> (usize, usize) {
    let first = texts[0];
    let mut head =
        (0..first.len()).take_while(|&i| texts.iter().all(|t| t.get(i).is_some_and(|x| x.tok == first[i].tok))).count();
    let mut tail = (0..first.len() - head)
        .take_while(|&i| {
            texts.iter().all(|t| t.len() - head > i && t[t.len() - 1 - i].tok == first[first.len() - 1 - i].tok)
        })
        .count();
    while widen && head > 0 && typed.classify(&first[head - 1]).is_some() {
        head -= 1;
    }
    while widen && tail > 0 && typed.classify(&first[first.len() - tail]).is_some() {
        tail -= 1;
    }
    (head, tail)
}

/// The shape of a family of texts (an expl3 kind's constants): the ends all
/// share around typed text of the classes between them, at least one token
/// when each middle has one. `None` for fewer than two texts or no common end.
pub fn ends_hull(texts: &[&[Token]], typed: &Typed) -> Option<Vec<Token>> {
    let (head, tail) = (texts.len() >= 2).then(|| common_ends(texts, typed, true))?;
    if head + tail == 0 {
        return None;
    }
    let first = texts[0];
    let (mut class, mut one) = (TextClass::EMPTY, true);
    for t in texts {
        let middle = &t[head..t.len() - tail];
        one &= typed.holds_one(middle);
        for x in middle {
            class = class.union(typed.class_of(x).map_or_else(|| TextClass::of(x.tok), |c| c.0));
        }
    }
    let span = first.get(head).or(first.last()).map_or(Span::default(), |t| t.span);
    let mut text = first[..head].to_vec();
    text.extend(typed.text(class, one, None, span));
    text.extend_from_slice(&first[first.len() - tail..]);
    Some(text)
}

/// One macro for the parameterless macros `set`: their common beginning
/// and end, and typed unknown text for what lies between, when that is
/// balanced and holds no control sequence. `None` otherwise.
/// Widened, the middle has no length bound: one that grows each pass
/// never meets.
fn typed_join(set: &[Meaning], typed: &Typed, widen: bool) -> Option<Meaning> {
    let mut texts: Vec<&[Token]> = Vec::with_capacity(set.len());
    let mut def = None;
    for m in set {
        let Meaning::Macro(d) = m else { return None };
        if !d.parameter_text.items.is_empty() || d.parameter_text.brace_end || d.arg_spec.is_some() {
            return None;
        }
        texts.push(&d.replacement_text);
        def.get_or_insert(d);
    }
    let first = texts[0];
    let (head, tail) = common_ends(&texts, typed, widen);
    let mut class = TextClass::EMPTY;
    let mut one = true;
    // The hull of the middles' bounds, each the sum of its tokens'.
    let mut len = Some(0);
    for t in &texts {
        let middle = &t[head..t.len() - tail];
        one &= typed.holds_one(middle);
        let sum = middle.iter().try_fold(0, |n, x| Some(n + text_len(typed.interner, x)?));
        len = len.zip(sum).map(|(a, b)| a.max(b));
        let mut depth = 0i32;
        for x in middle {
            match x.tok {
                Tok::Chr(_, Catcode::Begin) => depth += 1,
                Tok::Chr(_, Catcode::End) => depth -= 1,
                _ => {}
            }
            if depth < 0 {
                return None;
            }
            class = class.union(typed.class_of(x).map_or_else(|| TextClass::of(x.tok), |c| c.0));
        }
        if depth != 0 || class.contains(TextClass::CS) {
            return None;
        }
    }
    let span = first.get(head).or(first.last()).map_or(Span::default(), |t| t.span);
    let mut text = first[..head].to_vec();
    text.extend(typed.text(class, one, len.filter(|_| !widen), span));
    text.extend_from_slice(&first[first.len() - tail..]);
    let def = def?;
    Some(Meaning::Macro(std::rc::Rc::new(crate::engine::tex::MacroDef {
        replacement_text: text.into(),
        ..(**def).clone()
    })))
}

/// Path switches and the slots they carried, for `--stats -vvvv`.
pub static RESUMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static RESUME_SLOTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Token lists past `limits.value_set`: their common beginning and end
/// around typed unknown text, as [`typed_join`] widens macros. A pgfmath
/// stack keeps its braces and its numbers stay digits (tex.web § 1227).
fn typed_toks_join(values: &[&Value], typed: &Typed, widen: bool) -> Option<std::rc::Rc<[Token]>> {
    let mut set = Vec::new();
    for v in values {
        for t in v.token_lists()? {
            let def = crate::engine::tex::MacroDef {
                parameter_text: Default::default(),
                arg_spec: None,
                replacement_text: t,
                long: false,
                outer: false,
                protected: false,
            };
            set.push(Meaning::Macro(std::rc::Rc::new(def)));
        }
    }
    match typed_join(&set, typed, widen)? {
        Meaning::Macro(d) => Some(d.replacement_text.clone()),
        _ => None,
    }
}

/// The text a meaning copies through: a macro without parameters, not
/// `\protected` or `\outer`, whose replacement text is non-empty characters
/// expansion does nothing to (none active, no `#`). An `\edef` expanding it
/// appends that text without looking at any of it.
pub fn copied_text(meaning: &Meaning) -> Option<&std::rc::Rc<[Token]>> {
    let Meaning::Macro(m) = meaning else { return None };
    let text = &m.replacement_text;
    let plain = m.arg_spec.is_none()
        && m.parameter_text.items.is_empty()
        && !m.parameter_text.brace_end
        && !m.protected
        && !m.outer
        && !text.is_empty()
        && text
            .iter()
            .all(|t| matches!(t.tok, crate::engine::tex::Tok::Chr(_, cat) if !matches!(cat, Catcode::Active | Catcode::Param)));
    plain.then_some(text)
}

#[cfg(test)]
mod tests {
    /// Bug guard for the fast path: the same set as sorting everything.
    #[test]
    fn distinct_sorted_is_sort_and_dedup() {
        for len in [0usize, 5, 64, 65, 1000] {
            let names: Vec<u32> =
                (0..len as u32).map(|i| (i * 7919 + 13) % (len as u32 / 3 + 2)).chain([100_000]).collect();
            let mut want = names.clone();
            want.sort_unstable();
            want.dedup();
            assert_eq!(super::distinct_sorted(&names), want);
        }
    }

    use super::*;

    /// A record's reads are each name once, by name, a read of the whole value winning over
    /// a read of the meaning, whether the read was its own or an ended inner record's. Pins
    /// the merge of an inner record's sorted part into the outer one's raw reads.
    #[test]
    fn a_record_reads_each_name_once_with_inner_parts_merged() {
        let mut env = Env::with_capacity(8);
        let outer = env.memo_open();
        env.get(Sym(5));
        env.kept(Sym(3));
        let inner = env.memo_open();
        env.kept(Sym(3));
        env.get(Sym(2));
        env.kept(Sym(7));
        env.kept(Sym(2));
        let reads = |r: Option<(Vec<(u32, bool)>, _, _)>| r.expect("not full").0;
        assert_eq!(reads(env.memo_since(inner)), [(2, true), (3, false), (7, false)]);
        env.memo_floor(Some(outer));
        env.kept(Sym(2));
        env.get(Sym(3));
        env.kept(Sym(1));
        assert_eq!(reads(env.memo_since(outer)), [(1, false), (2, true), (3, true), (5, true), (7, false)]);
    }

    /// The shape of a family is the ends all its texts share: l3fp's
    /// constants all are `\s__fp \__fp_chk:w` ... `;`, so a widened fp keeps
    /// the delimiters its parsers read. Pins a widened fp read as any text,
    /// whose `;` scan ran away to the end of the file.
    #[test]
    fn the_ends_a_family_shares_stay_around_typed_text() {
        let mut interner = Interner::default();
        for pad in ["x0", "x1", "x2"] {
            interner.intern(pad);
        }
        let cs = |n: &str, i: &mut Interner| Token::new(Tok::Cs(i.intern(n)), Span::default());
        let (fp, chk, exact) =
            (cs("s__fp", &mut interner), cs("__fp_chk:w", &mut interner), cs("s__fp_exact", &mut interner));
        let syms: Vec<Sym> = (0..TYPED_NAMES as u32).map(|i| Sym(1000 + i)).collect();
        let minted = Default::default();
        let typed = Typed { interner: &interner, syms: &syms, digits: Sym(1), more: Sym(2), minted: &minted };
        let chr = |c, cat| Token::new(Tok::Chr(c, cat), Span::default());
        let other = |c| chr(c, Catcode::Other);
        let zero = [fp, chk, other('0'), other('0'), exact, other(';')];
        let one =
            [fp, chk, other('1'), other('0'), chr('{', Catcode::Begin), other('1'), chr('}', Catcode::End), other(';')];
        let shape = ends_hull(&[&zero, &one], &typed).expect("a shape");
        assert_eq!(shape.len(), 4, "{shape:?}");
        assert_eq!((shape[0], shape[1], shape[3]), (fp, chk, other(';')));
        let middle = shape[2];
        assert!(middle.cs().is_some() && !zero.contains(&middle) && !one.contains(&middle), "the middle is typed text");
        assert!(ends_hull(&[&zero], &typed).is_none(), "one text tells no shape");
        assert!(ends_hull(&[&zero, &[other('x')]], &typed).is_none(), "no common end");
    }

    /// Macros alike but for `\long` join to that text, nothing in between.
    #[test]
    fn hull_of_empty_texts_is_empty() {
        let interner = Interner::default();
        let syms: Vec<Sym> = (0..TYPED_NAMES as u32).map(|i| Sym(1000 + i)).collect();
        let minted = Default::default();
        let typed = Typed { interner: &interner, syms: &syms, digits: Sym(1), more: Sym(2), minted: &minted };
        let def = |long| {
            Meaning::Macro(Rc::new(crate::engine::tex::MacroDef {
                parameter_text: Default::default(),
                arg_spec: None,
                replacement_text: Rc::from(vec![Token::new(Tok::Cs(Sym(7)), Span::default())]),
                long,
                outer: false,
                protected: false,
            }))
        };
        // Texts alike but for `\long`: the middle between them is empty.
        let Some(Meaning::Macro(d)) = typed_join(&[def(false), def(true)], &typed, false) else { panic!("a macro") };
        assert_eq!(d.replacement_text.iter().map(|t| t.tok).collect::<Vec<_>>(), [Tok::Cs(Sym(7))]);
    }

    /// A widened macro of digits stays a parameterless macro of unknown
    /// digits: its class survives, `\ifx` still sees a macro.
    #[test]
    fn widened_macro_keeps_its_text_class() {
        let interner = Interner::default();
        // The table as the machine builds it: 256 one-of names, the rest
        // serials, and the names typed with a length bound.
        let syms: Vec<Sym> = (0..TYPED_NAMES as u32).map(|i| Sym(1000 + i)).collect();
        let minted = Default::default();
        let typed = Typed { interner: &interner, syms: &syms, digits: Sym(1), more: Sym(2), minted: &minted };
        let digit = |c| Token::new(Tok::Chr(c, crate::engine::tex::Catcode::Other), Span::default());
        let def = crate::engine::tex::MacroDef {
            parameter_text: Default::default(),
            arg_spec: None,
            replacement_text: vec![digit('1'), digit('2')].into(),
            long: false,
            outer: false,
            protected: false,
        };
        let b =
            widened(Binding::builtin(Meaning::Macro(std::rc::Rc::new(def))), &typed, SetLimits::default().text_names);
        let Meaning::Macro(d) = &b.meaning else { panic!("not a macro: {:?}", b.meaning) };
        // Two digits widen to digits of that length.
        let two = syms[bounded_index(TextClass::DIGIT, true, 2, 0).expect("bounded")];
        assert_eq!(d.replacement_text.iter().map(|t| t.tok).collect::<Vec<_>>(), vec![Tok::Cs(two)]);
        assert!(!b.certain);
    }

    /// A loop head where a name was defined on one pass and not yet on
    /// another: only `undefined` joins its meaning, which the join itself
    /// bounds. The widening keeps the text (here code, a call with an
    /// argument and `+40`), so the parser still runs it. Pins the hull that
    /// made pgfmath's stripped expression and its dispatch macros any text.
    #[test]
    fn maybe_undefined_at_a_head_keeps_its_text() {
        let interner = Interner::default();
        let syms: Vec<Sym> = (0..TYPED_NAMES as u32).map(|i| Sym(1000 + i)).collect();
        let minted = Default::default();
        let typed = Typed { interner: &interner, syms: &syms, digits: Sym(1), more: Sym(2), minted: &minted };
        let other = |c| Token::new(Tok::Chr(c, Catcode::Other), Span::default());
        let text = vec![Token::new(Tok::Cs(Sym(7)), Span::default()), other('+'), other('4'), other('0')];
        let def = Meaning::Macro(Rc::new(crate::engine::tex::MacroDef {
            parameter_text: Default::default(),
            arg_spec: None,
            replacement_text: text.into(),
            long: false,
            outer: false,
            protected: false,
        }));
        let old = Binding::builtin(def.clone());
        let new = Binding { may: Some(vec![def.clone(), Meaning::Undefined].into()), ..Binding::builtin(def.clone()) };
        for reach in [Reach::Joined, Reach::Havoc] {
            let b = widen_slot(&Some(old.clone()), Some(new.clone()), SetLimits::default(), &typed, reach)
                .expect("a binding");
            let may = b.may.as_deref().expect("still maybe undefined");
            assert!(may.contains(&Meaning::Undefined) && may.contains(&def), "{reach:?}: {may:?}");
        }
    }

    /// Numbers joined at a loop head keep the operator between them, and
    /// the run structure comes before [`numbers`]: `12+1` and `345+1`
    /// widen to a printed integer, `+`, the `1` both hold. A summary
    /// ([`Reach::Havoc`]) keeps no structure: one token.
    #[test]
    fn widened_sum_keeps_its_operator() {
        let interner = Interner::default();
        let syms: Vec<Sym> = (0..TYPED_NAMES as u32).map(|i| Sym(1000 + i)).collect();
        let minted = Default::default();
        let typed = Typed { interner: &interner, syms: &syms, digits: Sym(1), more: Sym(2), minted: &minted };
        let def = |s: &str| {
            Meaning::Macro(Rc::new(crate::engine::tex::MacroDef {
                parameter_text: Default::default(),
                arg_spec: None,
                replacement_text: s.chars().map(|c| Token::new(Tok::Chr(c, Catcode::Other), Span::default())).collect(),
                long: false,
                outer: false,
                protected: false,
            }))
        };
        let mut b = Binding::builtin(Meaning::Unknown);
        b.may = Some(vec![def("12+1"), def("345+1")].into());
        // A summary's widening may be any text of the kind: one token.
        let Meaning::Macro(d) = &widened(b, &typed, SetLimits::default().text_names).meaning else {
            panic!("not a macro")
        };
        assert_eq!(d.replacement_text.len(), 1);
        let plus = Tok::Chr('+', Catcode::Other);
        let lead = syms[lead_index(true, 3).expect("lead")];
        let widen = |a: &str, b: &str, reach| {
            let (old, new) = (Binding::builtin(def(a)), Binding::builtin(def(b)));
            let b = widen_slot(&Some(old), Some(new), SetLimits::default(), &typed, reach).expect("a binding");
            let members = b.may.as_deref().unwrap_or(std::slice::from_ref(&b.meaning)).to_vec();
            let text = |m: &Meaning| match m {
                Meaning::Macro(d) => d.replacement_text.iter().map(|t| t.tok).collect::<Vec<_>>(),
                m => panic!("not a macro: {m:?}"),
            };
            members.iter().map(text).collect::<Vec<_>>()
        };
        assert_eq!(widen("12+1", "345+1", Reach::Joined), [vec![Tok::Cs(lead), plus, Tok::Chr('1', Catcode::Other)]]);
        assert_eq!(widen("12+1", "345+1", Reach::Havoc).iter().map(Vec::len).collect::<Vec<_>>(), [1]);
        // Other shapes: more texts than the head had, a set that grows,
        // so one token at a head too ([`numbers`]), and at a summary.
        for reach in [Reach::Joined, Reach::Havoc] {
            assert_eq!(widen("12+1", "345-1", reach).iter().map(Vec::len).collect::<Vec<_>>(), [1]);
        }
    }

    /// Slices that spell all of a number read back as it, whatever the
    /// cut: a known length, an open one, the whole as one slice. Others
    /// stay as they are, and every input ends (an open length is 0).
    #[test]
    fn slices_spelling_a_whole_number_are_that_number() {
        use crate::engine::scan::{OPEN, TYPED_DIGITS};
        let mut interner = Interner::default();
        let w = interner.intern(&format!("{TYPED_DIGITS}0..214#7"));
        let v = interner.intern(&format!("{TYPED_DIGITS}0..99#8"));
        let mut cut = |n: Sym, k: u32, i: u32, j: u32| {
            let whole = interner.name(n).strip_prefix(TYPED_DIGITS).unwrap_or_default().to_string();
            Token::new(Tok::Cs(interner.intern(&format!("{TYPED_DIGITS}0..9@{k}:{i}:{j}:{whole}"))), Span::default())
        };
        let (o, plus) = (OPEN, Token::new(Tok::Chr('+', Catcode::Other), Span::default()));
        let [a, b, c, d, e, f, g, h] = [
            cut(w, 3, 0, 1),
            cut(w, 3, 1, 3),
            cut(w, o, 0, 1),
            cut(w, o, 1, o),
            cut(w, o, 0, o),
            cut(v, 2, 0, 1),
            cut(w, 3, 2, 3),
            cut(w, 3, 1, 2),
        ];
        let whole = |s: Sym| Token::new(Tok::Cs(s), Span::default());
        let cases: [(Vec<Token>, Vec<Token>); 8] = [
            (vec![a, b], vec![whole(w)]),
            (vec![c, d], vec![whole(w)]),
            (vec![e], vec![whole(w)]),
            (vec![e, e], vec![whole(w), whole(w)]),
            (vec![c, d, plus, a, h, g], vec![whole(w), plus, whole(w)]),
            (vec![a], vec![a]),
            (vec![a, f], vec![a, f]),
            (vec![b, a, g], vec![b, a, g]),
        ];
        for (text, want) in cases {
            let toks = |t: &[Token]| t.iter().map(|t| t.tok).collect::<Vec<_>>();
            assert_eq!(toks(&unslice(&text, &interner)), toks(&want), "{:?}", toks(&text));
        }
    }

    /// Printed decimals and an operator, joined at a loop head, over a table
    /// of signs, widths and decimals: each pair's hull holds both (sound),
    /// keeps `.` and `+1`, and holds no integer with a leading zero or a
    /// lone `-` (no octal or missing-number path pgfmath would take).
    #[test]
    fn printed_numbers_hull_to_printed_numbers() {
        let interner = Interner::default();
        let syms: Vec<Sym> = (0..TYPED_NAMES as u32).map(|i| Sym(1000 + i)).collect();
        let minted = Default::default();
        let typed = Typed { interner: &interner, syms: &syms, digits: Sym(1), more: Sym(2), minted: &minted };
        let mut lang: std::collections::HashMap<Sym, String> =
            std::collections::HashMap::from([(Sym(1), "[0-9]+".into()), (Sym(2), "[0-9]*".into())]);
        for len in 1..=MAX_LEN {
            for one in [false, true] {
                let int = format!("(0|[1-9][0-9]{{0,{}}})", len - 1);
                lang.insert(syms[lead_index(one, len).expect("lead")], if one { int } else { format!("{int}?") });
                let i = bounded_index(TextClass::DIGIT, one, len, 0).expect("bounded");
                lang.insert(syms[i], format!("[0-9]{{{},{len}}}", u32::from(one)));
            }
        }
        let pattern = |text: &[Token]| {
            let mut p = String::from("^");
            for (i, t) in text.iter().enumerate() {
                match t.tok {
                    // A sign or first digit: the digits after it say which.
                    Tok::Chr(crate::engine::tex::UNKNOWN_CHAR, _) => p.push_str("-?"),
                    Tok::Chr(c, _) => p.push_str(&regex::escape(&c.to_string())),
                    Tok::Cs(s)
                        if i > 0 && text[i - 1].tok == Tok::Chr(crate::engine::tex::UNKNOWN_CHAR, Catcode::Other) =>
                    {
                        p.push_str(lang[&s].trim_end_matches('?'))
                    }
                    Tok::Cs(s) => p.push_str(&lang[&s]),
                    Tok::Param(_) => panic!("no parameter in a hull"),
                }
            }
            regex::Regex::new(&(p + "$")).expect("pattern")
        };
        let chars =
            |s: &str| s.chars().map(|c| Token::new(Tok::Chr(c, Catcode::Other), Span::default())).collect::<Vec<_>>();
        let mut members = Vec::new();
        for sign in ["", "-"] {
            for int in ["0", "7", "42", "305", "16383"] {
                for dec in ["0", "5", "25", "125"] {
                    members.push(format!("{sign}{int}.{dec}+1"));
                }
            }
        }
        for a in &members {
            for b in &members {
                let (x, y) = (chars(a), chars(b));
                let hull = text_hull(&[&x, &y], &typed, Reach::Joined);
                let p = pattern(&hull);
                assert!(p.is_match(a) && p.is_match(b), "{a} | {b}: {p}");
                assert!(
                    hull.ends_with(&chars("+1")[..]) && hull.iter().any(|t| t.tok == Tok::Chr('.', Catcode::Other))
                );
                for bad in ["08.5+1", "-.5+1", "-08.0+1", ".5+1"] {
                    assert!(!p.is_match(bad), "{a} | {b}: {p} holds {bad}");
                }
                // Only a negative member brings the sign, then for both.
                let (neg, int) =
                    (a.starts_with('-') || b.starts_with('-'), |s: &str| s.split('.').next().map(str::to_owned));
                let other = |s: &str| s.strip_prefix('-').map_or_else(|| format!("-{s}"), str::to_owned);
                assert_eq!(p.is_match(&other(b)), neg && int(a) != int(b), "{a} | {b}: {p}");
            }
        }
        // The mutant without printed integers: digits of any spelling, so
        // the octal `08` path this hull rules out.
        let (x, y) = (chars("7"), chars("42"));
        assert!(pattern(&run_hull(&[&x, &y], &typed, false)).is_match("08"));
        assert!(!pattern(&run_hull(&[&x, &y], &typed, true)).is_match("08"));
        // A summary's write is any text of its kind: no structure kept,
        // for every pair (the hull still holds both).
        for a in &members {
            for b in members.iter().step_by(7) {
                let hull = text_hull(&[&chars(a), &chars(b)], &typed, Reach::Havoc);
                assert_eq!(hull.len(), 1, "{a} | {b}");
            }
        }
    }

    /// The two widening sites on one pair of passes, `12+1` and `345+1`:
    /// a loop head's join keeps the `+1` (the next pass runs from it), a
    /// summary toward what the passes began with keeps no structure (a
    /// pass not run may write another shape). The mutant that treats the
    /// summary as a join keeps `+`.
    #[test]
    fn a_summary_keeps_no_operator_a_head_join_does() {
        let interner = Interner::default();
        let syms: Vec<Sym> = (0..TYPED_NAMES as u32).map(|i| Sym(1000 + i)).collect();
        let minted = Default::default();
        let typed = Typed { interner: &interner, syms: &syms, digits: Sym(1), more: Sym(2), minted: &minted };
        let mac = |s: &str| {
            Binding::builtin(Meaning::Macro(Rc::new(crate::engine::tex::MacroDef {
                parameter_text: Default::default(),
                arg_spec: None,
                replacement_text: s.chars().map(|c| Token::new(Tok::Chr(c, Catcode::Other), Span::default())).collect(),
                long: false,
                outer: false,
                protected: false,
            })))
        };
        let toks = |b: &Binding| match &b.meaning {
            Meaning::Macro(d) => d.replacement_text.iter().map(|t| t.tok).collect::<Vec<_>>(),
            m => panic!("not a macro: {m:?}"),
        };
        let (x, plus) = (Sym(3), Tok::Chr('+', Catcode::Other));
        let mut env = Env::with_capacity(8);
        env.set(x, mac("12+1"), true);
        let fork = env.fork();
        let began = env.path(&fork);
        env.set(x, mac("345+1"), true);
        let pass = env.path(&fork);
        for (reach, kept) in [(Reach::Joined, true), (Reach::Havoc, false)] {
            let (joined, _, _) = env.join_paths(&fork, &began, &pass, &typed, (Some(reach), false), &[]);
            let b = joined.slots.get(&x.0).and_then(|(b, _)| b.clone()).expect("joined");
            assert_eq!(toks(&b).contains(&plus), kept, "{reach:?}: {:?}", toks(&b));
        }
        env.resume(&fork, &pass);
        env.widen(&[x], Some((&began, &fork)), &typed);
        let b = env.slot(x).expect("widened");
        assert_eq!(toks(b).len(), 1, "a summary keeps one token: {:?}", toks(b));
        env.unfork(fork);
    }

    fn int(n: i64) -> Binding {
        let mut b = Binding::builtin(Meaning::Unknown);
        b.value = Value::Int(n);
        b
    }

    fn value(env: &Env, sym: u32) -> Option<i64> {
        env.slot(Sym(sym)).and_then(|b| b.value.as_int())
    }

    /// A path holds exactly the slots changed since its fork, as they
    /// stand; a rewind puts every slot back, with nested forks too.
    #[test]
    fn paths_hold_what_changed_since_their_fork() {
        let mut env = Env::with_capacity(8);
        env.set(Sym(1), int(1), true);
        let outer = env.fork();
        env.set(Sym(1), int(10), true);
        env.set(Sym(2), int(20), true);
        env.set(Sym(2), int(21), true);
        let inner = env.fork();
        env.set(Sym(2), int(22), true);
        env.set(Sym(3), int(30), true);
        assert_eq!(env.trail.len(), 4, "one entry per slot and fork");
        let deep = env.path(&inner);
        assert_eq!(deep.slots.size(), 2);
        assert_eq!(deep.slots.get(&2).and_then(|(b, _)| b.as_ref()?.value.as_int()), Some(22));
        let wide = env.path(&outer);
        assert_eq!(wide.slots.size(), 3);
        assert_eq!(wide.slots.get(&1).and_then(|(b, _)| b.as_ref()?.value.as_int()), Some(10));
        env.rewind(&inner);
        assert_eq!((value(&env, 2), value(&env, 3)), (Some(21), None));
        let wide = env.path(&outer);
        assert_eq!(wide.slots.size(), 2, "the inner change is gone from the outer path");
        assert_eq!(wide.slots.get(&2).and_then(|(b, _)| b.as_ref()?.value.as_int()), Some(21));
        env.resume(&inner, &deep);
        assert_eq!((value(&env, 2), value(&env, 3)), (Some(22), Some(30)));
        env.unfork(inner);
        env.rewind(&outer);
        assert_eq!((value(&env, 1), value(&env, 2), value(&env, 3)), (Some(1), None, None));
        assert!(env.path(&outer).slots.is_empty());
        env.unfork(outer);
        assert!(env.trail.is_empty() && env.changed.is_empty());
    }

    /// A loop under an open fork whose every round opens and ends a fork
    /// of its own (an undecided conditional) keeps one trail entry per
    /// slot, not one per round: the rounds' saved bindings held every
    /// body the loop built (13 GB on a tikz picture). Rewinding the outer
    /// fork still restores what it saw.
    #[test]
    fn ended_inner_forks_leave_one_entry_per_slot() {
        let rounds = 100;
        let mut env = Env::with_capacity(8);
        env.set(Sym(1), int(1), true);
        let outer = env.fork();
        for n in 0..rounds {
            let inner = env.fork();
            env.set(Sym(1), int(n), true);
            env.set(Sym(2 + (n % 2) as u32), int(n), true);
            env.unfork(inner);
        }
        assert_eq!(env.trail.len(), 3, "one entry per slot the loop wrote");
        assert_eq!(env.path(&outer).slots.size(), 3);
        assert_eq!(value(&env, 1), Some(rounds - 1));
        env.rewind(&outer);
        assert_eq!((value(&env, 1), value(&env, 2), value(&env, 3)), (Some(1), None, None));
        env.unfork(outer);
        assert!(env.trail.is_empty());
    }

    /// Intuition: each record keeps every name it read once, a whole read
    /// over a meaning read. An ended record's part of the log is kept
    /// reduced, and the record around it merges that part instead of
    /// sorting it again. Pins that the merge gives what a sort of the whole
    /// part gives, and that the inner record still sees only its own reads.
    #[test]
    fn a_record_around_an_ended_one_reads_what_both_read() {
        let mut env = Env::with_capacity(8);
        let outer = env.memo_open();
        for _ in 0..1000 {
            env.memo_read(1, false);
            env.memo_read(2, true);
            env.memo_read(2, false);
        }
        let inner = env.memo_open();
        env.memo_read(1, true);
        env.memo_read(2, false);
        let reads = |m| env.memo_since(m).expect("log").0;
        assert_eq!(reads(inner), [(1, true), (2, false)], "the inner record logs its own reads");
        env.memo_read(3, false);
        assert_eq!(reads(outer), [(1, true), (2, true), (3, false)]);
    }

    /// The write log is bounded by the names and group depths it holds, not
    /// by how often code assigns, and folding it loses nothing: read from
    /// its start it says what the whole log says, read from a mark made
    /// before a fold never less, from a later one the same. Pins the abort
    /// of a `\foreach` that assigned for minutes (one entry per write: 2 GiB).
    #[test]
    fn a_folded_write_log_says_what_the_whole_one_says() {
        const NAMES: u32 = 6;
        const DEPTHS: u32 = 3;
        const WRITES: u64 = 1500;
        const CLEAR: u64 = 5;
        let mut cats = crate::engine::tex::CatcodeTable::default();
        let mut whole = Env::with_capacity(8);
        let mut folded = whole.clone();
        whole.sets.write_log = usize::MAX;
        folded.sets.write_log = 4;
        let (mut marks, mut forks, mut seed) = (vec![0u64], Vec::new(), 1u64);
        let (bound, mut grown, mut shallower) = (2 * (NAMES * (DEPTHS + 1)) as usize, 0, 0);
        for n in 0..WRITES {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            let (pick, sym) = ((seed >> 33) % 16, Sym(1 + (seed >> 40) as u32 % NAMES));
            for (i, env) in [&mut whole, &mut folded].into_iter().enumerate() {
                match pick {
                    0 if (env.marks.len() as u32) < DEPTHS => env.push_group(GroupKind::Simple, Span::default()),
                    1 if !env.marks.is_empty() => _ = env.pop_group(&mut cats),
                    2 => forks.push(env.fork()),
                    3 => env.merge_recent(1, DEPTHS as usize),
                    // A token read from a file: the log starts over, inside groups too.
                    5 if n % CLEAR == 0 => env.clear_recent(),
                    // A rewind puts back what the fork saw, stamps too: a later write is undone.
                    4 if !forks.is_empty() => {
                        let fork = forks.remove(forks.len() + i - 2);
                        env.rewind(&fork);
                        env.unfork(fork);
                    }
                    _ => env.set(sym, int(n as i64), pick % 2 == 0),
                }
            }
            assert_eq!(whole.assignments, folded.assignments);
            marks.push(whole.assignments);
            assert_eq!(folded.recent_all(), whole.recent_all(), "after {n} steps");
            for &mark in marks.iter().rev().step_by(31) {
                let exact = folded.logged_since(mark);
                assert!(!exact || folded.written_since(mark) == whole.written_since(mark));
                for depth in 0..=DEPTHS {
                    let (f, w) = (folded.recent_within(mark, depth), whole.recent_within(mark, depth));
                    assert!(if exact { f == w } else { w.iter().all(|s| f.contains(s)) }, "mark {mark}, depth {depth}");
                }
            }
            assert!(folded.recent.len() <= bound, "{} entries", folded.recent.len());
            grown = grown.max(whole.recent.len());
            let first = |sym: u32| whole.recent.iter().find(|w| w.sym == sym).map_or(0, |w| w.depth);
            shallower += usize::from(whole.recent.last().is_some_and(|w| w.depth < first(w.sym)));
        }
        assert!(grown > 4 * bound, "the whole log grows with the writes: {grown}");
        assert!(shallower > 0, "the case a fold must keep: a name written again less deep than its first write");
    }

    /// A path rebased onto where the running path stands resumes from a
    /// fresh fork there as it stood: a slot only the running path changed
    /// goes back to what the old fork saw.
    #[test]
    fn a_rebased_path_resumes_as_it_was() {
        let mut env = Env::with_capacity(8);
        env.set(Sym(1), int(1), true);
        env.set(Sym(2), int(2), true);
        let old = env.fork();
        env.set(Sym(1), int(10), true);
        let mut shelved = env.path(&old);
        env.rewind(&old);
        env.set(Sym(2), int(20), true);
        env.set(Sym(3), int(30), true);
        assert!(env.rebase(&old, &mut [&mut shelved]));
        assert_eq!((value(&env, 1), value(&env, 2), value(&env, 3)), (Some(1), Some(20), Some(30)));
        env.unfork(old);
        let fresh = env.fork();
        env.set(Sym(4), int(40), true);
        env.resume(&fresh, &shelved);
        let state = (value(&env, 1), value(&env, 2), value(&env, 3), value(&env, 4));
        assert_eq!(state, (Some(10), Some(2), None, None));
        env.rewind(&fresh);
        assert_eq!((value(&env, 1), value(&env, 2), value(&env, 3)), (Some(1), Some(20), Some(30)));
        env.unfork(fresh);
    }
}
