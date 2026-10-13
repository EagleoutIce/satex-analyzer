//! The save stack: dense slot table plus undo journal (The TeXbook § 275).
//! Drives conditional analysis: run branch, capture, rollback, join.

use crate::fixed::{HashMap, LookupMap, LookupSet};
use std::rc::Rc;

use crate::mode::Nest;
use crate::tex::{Catcode, CatcodeTable, Interner, Meaning, Span, Sym, TextClass, Tok, Token};
use crate::value::Value;

/// The slots a path changed since a fork, as they stand now: a persistent
/// map, so a snapshot is a pointer copy and shares its nodes with the live
/// one. `rpds` is maintained, has an `Rc` variant, and its `_mut` methods
/// change a node in place while nothing shares it.
type Changed = rpds::HashTrieMap<u32, (Option<Binding>, u32)>;
/// No trail entry.
const NONE: u32 = u32::MAX;

pub type NodeId = u32;

/// What a control sequence is bound to: its [`Meaning`], [`Value`], and the
/// [`NodeId`]s that defined it. [`Env`] stores one per active [`Sym`].
#[derive(Clone, Debug)]
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
#[derive(Clone)]
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
        /// Whether the category was one satex could not follow.
        old_unknown: bool,
    },
}

/// Kind of group (tex.web § 269): `{`/`}`, `\begingroup`/`\endgroup`, or `$...$`.
/// A closer closes only its opener's kind ([`GroupKind::closes`], enforced by
/// [`Env::pop_group`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GroupKind {
    Simple,
    SemiSimple,
    Math { display: bool },
}

impl GroupKind {
    pub fn closes(self, group: GroupKind) -> bool {
        matches!(
            (self, group),
            (GroupKind::Simple, GroupKind::Simple)
                | (GroupKind::SemiSimple, GroupKind::SemiSimple)
                | (GroupKind::Math { .. }, GroupKind::Math { .. })
        )
    }
}

/// The save stack.  A path through an undecided conditional copies it, so
/// what lies below the last copy is frozen in shared chunks: a copy costs
/// what was pushed since, not the whole stack.
#[derive(Clone, Default)]
struct Journal {
    /// The first `n` entries of a chunk, and below them its parent's.
    frozen: Option<(std::rc::Rc<Chunk>, usize)>,
    frozen_len: usize,
    top: Vec<Entry>,
}

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

/// Open group: journal start, opener kind, saved `\aftergroup` tokens.
#[derive(Clone)]
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
}

/// The meaning table and the save stack: [`Binding`]s are pushed and undone
/// per group, which is what `\begingroup` and `}` do.
#[derive(Default)]
pub struct Env {
    slots: Vec<Option<Binding>>,
    /// The group level each slot's binding was made at, 0 for level one
    /// (tex.web § 221 `eq_level`). A local assignment saves the old binding
    /// only once per level (§ 277). A `\global` one is made at level one, so
    /// it survives the saving group's end (§ 283).
    level: Vec<u32>,
    journal: Journal,
    /// Shared with the forks and paths that saw it: a fork per command
    /// copies a pointer, and only a group opened or closed copies the stack.
    marks: Rc<Vec<Mark>>,
    /// The last group id given out.
    group_ids: u64,
    /// While a path through an undecided conditional runs, every slot it
    /// changes, with what the slot held before, so that the next path can
    /// start from the same bindings. One entry per slot and fork: the
    /// first change under a fork saves what its rewind restores, and the
    /// entry names the slot's previous entry, put back on rewind.
    trail: Vec<(u32, Option<Binding>, u32, u32, u64)>,
    /// Per slot, its latest trail entry, `NONE` for none.
    trailed: Vec<u32>,
    /// Per open fork, the fewest groups the running path stood in since.
    lows: Vec<usize>,
    /// Per open fork, innermost last: where its trail begins, and the
    /// slots changed since it as they stand now.
    floors: Vec<usize>,
    changed: Vec<Changed>,
    forks: usize,
    /// Forks several paths went on from, which a load's record of what
    /// it wrote does not follow.
    paths: usize,
    /// Names assigned since [`Env::clear_recent`], for widening a loop that
    /// does not end to every state it could have left.
    recent: Vec<u32>,
    /// Per `recent` entry, [`Env::assignments`] when it was made; 0 when
    /// unknown.
    recent_at: Vec<u64>,
    /// Per slot, [`Env::assignments`] at the write its binding came from.
    /// A group end puts back the old one, so a name whose write a group
    /// end undid is not widened.
    stamp: Vec<u64>,
    /// `\let\tex_escapechar:D\escapechar`: the engine primitive a name was
    /// made equal to, for primitives satex models alike (tex.web keeps the
    /// primitive's `chr` code in the copy). A parameter's value lives with
    /// that primitive. `\meaning` and `\ifx` tell such copies apart.
    pub aliases: HashMap<Sym, Sym>,
    /// Assignments made so far: a loop that assigns is changing state, which
    /// a loop that only re-expands itself is not.
    pub assignments: u64,
    /// Writes that changed what a name or register holds, and other
    /// state a later read may see (catcodes, `\\read`): equal counts
    /// mean an unchanged state.
    pub effective: u64,
    /// How large a set of meanings or values a join keeps.
    pub sets: SetLimits,
    /// While a package cache is being captured: every slot read, as a bit
    /// set, which is what the package's effect may depend on.
    tracking: std::cell::Cell<bool>,
    reads: std::cell::RefCell<Reads>,
    /// While calls are recorded: every name read, in order.
    logging: std::cell::Cell<bool>,
    read_log: std::cell::RefCell<Vec<u32>>,
}

/// Which part of a binding a read looked at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum ReadKind {
    /// The meaning, with the flags and definition sites that go with it.
    Meaning,
    /// The value a register or parameter holds.
    Value,
    /// What assigning a value keeps: the binding apart from its value, an
    /// empty slot counting as a new register slot.
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

/// Reads recorded while a package cache is captured. Per slot: which parts
/// were read while still holding their pre-load value, and which parts the
/// load has since overwritten, so a later read of them owes nothing to
/// before. A group end restores what the parts held, and whether that was
/// pre-load.
#[derive(Default)]
struct Reads {
    read: LookupMap<u32, u8>,
    written: LookupMap<u32, u8>,
    /// Slots assigned since the last [`Env::take_dirty`], in order.
    dirty: Vec<u32>,
    dirty_seen: LookupSet<u32>,
    /// Journal position, slot, and what was written of it when the journal
    /// saved it.
    saved: Vec<(usize, u32, u8)>,
    /// Slots whose text the load copied a text from before it into: a read
    /// of them reads that text, however the load has written them since.
    carrying: LookupSet<u32>,
    /// The slot whose meaning is being copied through, which a read of its
    /// meaning then is.
    copying: Option<u32>,
    /// Slots assigned since the segment began, and of those, the ones whose
    /// text holds what the slot held then, where and how long.
    touched: LookupSet<u32>,
    splices: LookupMap<u32, (usize, usize)>,
}

/// The save stack as it stood where paths split: groups can open and close
/// on one path only, so it is copied rather than trailed.
#[derive(Clone)]
pub struct Fork {
    trail: usize,
    /// Its index in [`Env::floors`].
    level: usize,
    /// Several paths went on from it ([`Env::part`]).
    parted: bool,
    journal: Journal,
    marks: Rc<Vec<Mark>>,
}

/// What one path did to the bindings, relative to its [`Fork`].
#[derive(Clone)]
pub struct PathEnv {
    slots: Changed,
    journal: Journal,
    marks: Rc<Vec<Mark>>,
    /// The fewest groups the path stood in since the fork: the groups up
    /// to there, and what they saved before it, are the fork's.
    low: usize,
}

impl PathEnv {
    /// The names this path and `other` bind differently, both taken from
    /// their one fork: what one of them changed that the other does not
    /// hold.
    pub fn differing(&self, other: &PathEnv) -> Vec<Sym> {
        let mut out: Vec<Sym> = (self.slots.iter())
            .filter(|(k, (mine, _))| other.slots.get(k).is_none_or(|(theirs, _)| !same_slot(mine, theirs)))
            .map(|(k, _)| Sym(*k))
            .collect();
        out.extend(other.slots.keys().filter(|k| !self.slots.contains_key(k)).map(|k| Sym(*k)));
        out
    }

    /// How many slots the path changed since its fork.
    pub fn len(&self) -> usize {
        self.slots.size()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }
}

impl Env {
    pub fn with_capacity(n: usize) -> Self {
        Self {
            slots: vec![None; n],
            level: vec![0; n],
            trailed: vec![NONE; n],
            stamp: vec![0; n],
            ..Default::default()
        }
    }

    #[inline]
    fn grow(&mut self, sym: Sym) {
        let n = sym.0 as usize + 1;
        if self.slots.len() < n {
            self.slots.resize_with(n, || None);
            self.level.resize(n, 0);
            self.trailed.resize(n, NONE);
            self.stamp.resize(n, 0);
        }
    }

    /// The primitive `sym` is a copy of, or `sym` itself.  A copy that has
    /// since been given a meaning that is no primitive is itself again.
    pub fn identity(&self, sym: Sym) -> Sym {
        match self.aliases.get(&sym) {
            Some(&original) if matches!(self.get(sym).map(|b| &b.meaning), Some(Meaning::Primitive(_))) => original,
            _ => sym,
        }
    }

    /// The whole binding: its meaning and its value.
    #[inline]
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
    pub fn slot(&self, sym: Sym) -> Option<&Binding> {
        self.slots.get(sym.0 as usize).and_then(|s| s.as_ref())
    }

    /// Every name that currently has a binding: a sweep over a family of
    /// names, such as the registers a number satex could not pin down.
    pub fn bound(&self) -> Vec<Sym> {
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

    fn note_write(&self, sym: u32, bits: u8) {
        if !self.tracking.get() {
            return;
        }
        let mut reads = self.reads.borrow_mut();
        if reads.dirty_seen.insert(sym) {
            reads.dirty.push(sym);
        }
        // A path through an undecided conditional is rewound for the next
        // one, so what it writes is no reason to stop recording reads.
        if self.paths == 0 {
            *reads.written.entry(sym).or_insert(0) |= bits;
        }
    }

    /// The reads recorded since the last call. Starts a new record of them.
    /// What has been written stays known.
    pub fn take_read_marks(&self) -> Vec<(Sym, u8)> {
        let read = std::mem::take(&mut self.reads.borrow_mut().read);
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

    /// Whether a read of `sym`'s meaning reads something from before the
    /// load: it was not written, or it holds a text copied from then.
    pub fn meaning_fresh(&self, sym: Sym) -> bool {
        let reads = self.reads.borrow();
        !self.meaning_written(sym) || reads.carrying.contains(&sym.0)
    }

    /// `sym` now holds a text from before the load.
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

    /// Where the text `sym` held when the segment began stands in its text
    /// now, when it does: `sym` was not assigned since (at 0, the whole
    /// `len`), or only by copying it.
    pub fn splice_of(&self, sym: Sym, len: usize) -> Option<(usize, usize)> {
        let reads = self.reads.borrow();
        match reads.splices.get(&sym.0) {
            Some(&splice) => Some(splice),
            None if !reads.touched.contains(&sym.0) => Some((0, len)),
            None => None,
        }
    }

    /// Where a copy put that text, when one did and nothing assigned since.
    pub fn splice(&self, sym: Sym) -> Option<(usize, usize)> {
        self.reads.borrow().splices.get(&sym.0).copied()
    }

    pub fn set_splice(&self, sym: Sym, at: usize, len: usize) {
        if self.tracking.get() {
            self.reads.borrow_mut().splices.insert(sym.0, (at, len));
        }
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
        if self.logging.get() {
            self.read_log.borrow_mut().push(sym.0);
        }
    }

    /// Log every name read from now on (`on`), or stop and forget the log.
    pub fn log_reads(&self, on: bool) {
        self.logging.set(on);
        if !on {
            self.read_log.borrow_mut().clear();
        }
    }

    pub fn read_log_len(&self) -> usize {
        self.read_log.borrow().len()
    }

    /// The names read since the log was `from` long, each once, sorted;
    /// `None` when the log does not reach back that far.
    pub fn reads_since(&self, from: usize) -> Option<Vec<Sym>> {
        let log = self.read_log.borrow();
        let mut syms: Vec<u32> = log.get(from..)?.to_vec();
        syms.sort_unstable();
        syms.dedup();
        Some(syms.into_iter().map(Sym).collect())
    }

    pub fn track_reads(&self, on: bool) -> bool {
        self.tracking.replace(on)
    }

    #[inline]
    pub fn meaning(&self, sym: Sym) -> Meaning {
        self.log_read(sym);
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Meaning);
        }
        match self.slot(sym) {
            Some(b) => b.meaning.clone(),
            None => Meaning::Undefined,
        }
    }

    /// Whether `sym` means an `\outer` macro. tex.web § 336 checks this for
    /// every token read — engine bookkeeping, not a read of the meaning, so
    /// nothing is recorded or cloned.
    pub fn is_outer(&self, sym: Sym) -> bool {
        self.slot(sym).is_some_and(|b| matches!(&b.meaning, Meaning::Macro(m) if m.outer))
    }

    /// The meaning, as text being skipped sees it: only whether it is a
    /// conditional, `\else`, `\or` or `\fi` counts as read.
    pub fn skip_meaning(&self, sym: Sym) -> Meaning {
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Skip);
        }
        match self.slot(sym) {
            Some(b) => b.meaning.clone(),
            None => Meaning::Undefined,
        }
    }

    /// The name's definition sites, to link a dependency-graph edge to.
    /// Whatever they are, a package cache links the edge again on replay,
    /// so this reads nothing.
    pub fn link_defs(&self, sym: Sym) -> Vec<NodeId> {
        self.slot(sym).map(|b| b.defs.clone()).unwrap_or_default()
    }

    /// The name's definition sites, when what is done depends on whether
    /// there are any.
    pub fn defs(&self, sym: Sym) -> Vec<NodeId> {
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Defs);
        }
        self.slot(sym).map(|b| b.defs.clone()).unwrap_or_default()
    }

    pub fn is_defined(&self, sym: Sym) -> bool {
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Defined);
        }
        !matches!(self.slot(sym).map(|b| &b.meaning), None | Some(Meaning::Undefined))
    }

    /// Slots that hold a meaning, which is what a format cache stores: a
    /// rebuild and a cache hit then report the same number.
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

    /// The group level TeX reports.
    pub fn group_level(&self) -> usize {
        self.marks.len()
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
        }
        self.recent.push(sym.0);
        self.recent_at.push(self.assignments);
        let stamp = std::mem::replace(&mut self.stamp[i], self.assignments);
        if global {
            self.note_write(sym.0, writes);
            binding.global = true;
            self.level[i] = 0;
            self.slots[i] = Some(binding);
            self.changed(i);
            return;
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
    pub fn num(&self, sym: Sym) -> Option<(crate::value::Quantity, crate::value::Num)> {
        self.log_read(sym);
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Value);
        }
        self.slot(sym)?.value.num()
    }

    /// What a test taught a path of a register: that its value is `value`.
    /// Not an assignment, so the save stack is not told.
    pub fn refine_value(&mut self, sym: Sym, value: Value) {
        self.effective += 1;
        let i = sym.0 as usize;
        let Some(Some(binding)) = self.slots.get(i) else { return };
        let binding = Binding { value, ..binding.clone() };
        self.log(i);
        self.slots[i] = Some(binding);
        self.changed(i);
    }

    /// Assign a value, keeping the rest of the binding: this reads nothing
    /// of the old value, only what is kept (an empty slot keeps what a new
    /// register slot has).
    pub fn set_value(&mut self, sym: Sym, value: Value, global: bool) {
        self.effective += 1;
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Kept);
        }
        let mut binding = match self.slot(sym) {
            Some(b) => b.clone(),
            None => Binding::builtin(Meaning::Unknown),
        };
        binding.value = value;
        self.grow(sym);
        self.set_tracked(sym, binding, global, ReadKind::Value.bit());
    }

    /// Change a value where it stands, saving nothing: TeX's `box(n):=null`
    /// (tex.web § 1079), which a group that saved the register undoes and
    /// one that did not keeps.
    pub fn set_value_in_place(&mut self, sym: Sym, value: Value) {
        self.effective += 1;
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
            Mark { journal: self.journal.len(), kind, after: Vec::new(), opened, nest: None, id: self.group_ids },
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

    /// Close the groups opened after group `id`, innermost first, as
    /// `\endgroup` would (tex.web §§ 281-283), up to a math group. A value
    /// they put back that was itself written since assignment `since` may
    /// hold anything: the dropped text would have run on and written again.
    /// Returns how many closed.
    pub fn close_since(&mut self, id: u64, since: u64, catcodes: &mut CatcodeTable, typed: &Typed) -> usize {
        let mut n = 0;
        while let Some(mark) = self.marks.last()
            && mark.id > id
            && matches!(mark.kind, GroupKind::Simple | GroupKind::SemiSimple)
        {
            let mut restored = Vec::new();
            self.effective += 1;
            let mark = Rc::make_mut(&mut self.marks).pop().expect("checked");
            self.lower();
            self.unwind_into(mark.journal, Some(&mut *catcodes), Some(&mut restored));
            for (sym, stamp) in restored {
                if stamp >= since {
                    self.widen_in_place(Sym(sym), typed);
                }
            }
            n += 1;
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
        self.save_catcode_state(character, old, false);
    }

    /// [`Env::save_catcode`] keeping whether the category was one satex
    /// could not follow, so that the group end puts that back too.
    pub fn save_catcode_state(&mut self, character: char, old: Catcode, old_unknown: bool) {
        self.effective += 1;
        if !self.marks.is_empty() {
            self.journal.push(Entry::Catcode { character, old, old_unknown });
        }
    }

    /// The bindings as they stand, with no save stack: a sandbox run starts
    /// from them and cannot undo past them.
    pub fn snapshot(&self) -> Env {
        Env {
            slots: self.slots.clone(),
            level: vec![0; self.level.len()],
            stamp: vec![0; self.level.len()],
            trailed: vec![NONE; self.level.len()],
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
        if self.tracking.get() {
            // What a group puts back is what the parts held before, and it
            // came from before the load exactly when it did then.
            let mut reads = self.reads.borrow_mut();
            while reads.saved.last().is_some_and(|(at, ..)| *at >= to) {
                let (_, sym, written) = reads.saved.pop().expect("checked");
                // tex.web § 283: a binding made global since stays.
                if self.level[sym as usize] != 0 {
                    reads.written.insert(sym, written);
                }
            }
        }
        while self.journal.len() > to {
            match self.journal.pop().expect("journal length checked") {
                // tex.web § 283: a binding made global since is retained.
                Entry::Meaning { sym, .. } if self.level[sym as usize] == 0 => {}
                Entry::Meaning { sym, old, level, alias, stamp } => {
                    let index = sym as usize;
                    match alias {
                        Some(alias) => self.aliases.insert(Sym(sym), alias),
                        None => self.aliases.remove(&Sym(sym)),
                    };

                    self.log(index);
                    self.level[index] = level;
                    self.stamp[index] = stamp;
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
                Entry::Catcode { character, old, old_unknown } => {
                    if let Some(table) = catcodes.as_deref_mut() {
                        table.restore(character, old, old_unknown);
                    }
                }
            }
        }
    }

    /// The names assigned since [`Env::clear_recent`], and a way to put them
    /// back, for a package cache that replays a load.
    pub fn recent(&self) -> Vec<Sym> {
        self.recent.iter().map(|s| Sym(*s)).collect()
    }

    pub fn set_recent(&mut self, recent: Vec<Sym>) {
        self.recent = recent.into_iter().map(|s| s.0).collect();
        self.recent_at = vec![0; self.recent.len()];
    }

    pub fn clear_recent(&mut self) {
        self.recent.clear();
        self.recent_at.clear();
    }

    pub fn recent_len(&self) -> usize {
        self.recent.len()
    }

    /// The names assigned since `recent_len` was `from`, each once; all of
    /// them when the list was cleared since.
    /// A write a group end or a rewind undid since is left out.
    pub fn recent_since(&self, from: usize) -> Vec<Sym> {
        let from = if from > self.recent.len() { 0 } else { from };
        let mut syms: Vec<(u32, u64)> =
            self.recent[from..].iter().copied().zip(self.recent_at[from..].iter().copied()).collect();
        syms.sort_unstable();
        syms.dedup_by_key(|(sym, _)| *sym);
        syms.into_iter()
            .filter(|&(sym, at)| self.stamp.get(sym as usize).is_none_or(|&s| s >= at))
            .map(|(sym, _)| Sym(sym))
            .collect()
    }

    /// The names written after assignment `mark` since [`Env::clear_recent`],
    /// each once.
    pub fn written_after(&self, mark: u64) -> Vec<Sym> {
        let syms = self.recent_since(0);
        syms.into_iter().filter(|s| self.stamp.get(s.0 as usize).is_none_or(|&at| at > mark)).collect()
    }

    /// `syms` may hold anything of their kind. With `toward`, each is
    /// joined with what `toward` holds and widened (an accumulator stays
    /// typed); a name `toward` never wrote may hold anything.
    pub fn widen(&mut self, syms: &[Sym], toward: Option<&PathEnv>, typed: &Typed) {
        for &sym in syms {
            let i = sym.0 as usize;
            let Some((theirs, _)) = toward.and_then(|t| t.slots.get(&sym.0).cloned()) else {
                self.widen_in_place(sym, typed);
                continue;
            };
            let Some(Some(mine)) = self.slots.get(i).cloned() else { continue };
            let joined = join(theirs.clone(), Some(mine.clone()), self.sets, Some(typed));
            let Some(mut joined) = widen_slot(&theirs, joined, self.sets, typed) else {
                self.widen_in_place(sym, typed);
                continue;
            };
            if same_slot(&Some(mine.clone()), &Some(joined.clone())) {
                continue;
            }
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
        self.assignments += 1;
        self.effective += 1;
        self.recent.push(i as u32);
        self.recent_at.push(self.assignments);
        self.stamp[i] = self.assignments;
    }

    /// `sym` may hold anything, at the group level it was last written at:
    /// a group end still puts back what it saved, and a local write never
    /// becomes a global one (`binding.global` only says some write was).
    fn widen_in_place(&mut self, sym: Sym, typed: &Typed) {
        let i = sym.0 as usize;
        let Some(Some(binding)) = self.slots.get(i).cloned() else { return };
        let binding = widened(binding, typed);
        // A slot already as wide as it gets is not written: widening it
        // again changes nothing a later read could see.
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
        if self.tracking.get() {
            let mut reads = self.reads.borrow_mut();
            reads.touched.insert(i as u32);
            reads.splices.remove(&(i as u32));
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
        if self.changed.is_empty() {
            return;
        }
        let entry = (self.slots[i].clone(), self.level[i]);
        for map in &mut self.changed {
            map.insert_mut(i as u32, entry.clone());
        }
    }

    /// A point the bindings can be rewound to.
    pub fn fork(&mut self) -> Fork {
        self.forks += 1;
        self.floors.push(self.trail.len());
        self.changed.push(Changed::new());
        self.lows.push(self.marks.len());
        Fork {
            trail: self.trail.len(),
            level: self.floors.len() - 1,
            parted: false,
            journal: self.journal.snapshot(),
            marks: self.marks.clone(),
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
        }
    }

    /// What the running path changed since `fork`.
    pub fn path(&self, fork: &Fork) -> PathEnv {
        debug_assert_eq!(self.floors.get(fork.level), Some(&fork.trail), "the fork is open");
        let slots = self.changed.get(fork.level).cloned().unwrap_or_default();
        let low = self.lows.get(fork.level).copied().unwrap_or(fork.marks.len());
        PathEnv { slots, journal: self.journal.clone(), marks: self.marks.clone(), low }
    }

    /// Back to the bindings `fork` saw.
    pub fn rewind(&mut self, fork: &Fork) {
        self.effective += 1;

        let mut popped = Vec::new();
        while self.trail.len() > fork.trail {
            let (sym, slot, saved, prev, stamp) = self.trail.pop().expect("trail length checked");
            self.stamp[sym as usize] = stamp;
            self.slots[sym as usize] = slot;
            self.level[sym as usize] = saved;
            self.trailed[sym as usize] = prev;
            popped.push(sym);
        }
        // Nothing changed since the fork; an outer fork sees a slot as it
        // stands, or unchanged when the change was after this fork.
        self.floors.truncate(fork.level + 1);
        self.changed.truncate(fork.level + 1);
        self.lows.truncate(fork.level + 1);
        if let Some(low) = self.lows.get_mut(fork.level) {
            *low = fork.marks.len();
        }
        if let Some(own) = self.changed.get_mut(fork.level) {
            *own = Changed::new();
        }
        popped.sort_unstable();
        popped.dedup();
        for sym in popped {
            let i = sym as usize;
            let entry = (self.slots[i].clone(), self.level[i]);
            for (floor, map) in self.floors.iter().zip(&mut self.changed).take(fork.level) {
                if self.trailed[i] != NONE && self.trailed[i] as usize >= *floor {
                    map.insert_mut(sym, entry.clone());
                } else {
                    map.remove_mut(&sym);
                }
            }
        }
        self.journal = fork.journal.clone();
        self.marks = fork.marks.clone();
    }

    /// Continue `path` from `fork`.
    pub fn resume(&mut self, fork: &Fork, path: &PathEnv) {
        self.effective += 1;
        self.rewind(fork);
        RESUMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        RESUME_SLOTS.fetch_add(path.slots.size() as u64, std::sync::atomic::Ordering::Relaxed);

        // The fork's own map is the path's, shared: rebuilt per resume, every
        // head and parked path would hold a copy of its own.
        let own = (self.changed.len() == fork.level + 1).then(|| self.changed.pop()).flatten();
        for (sym, (slot, saved)) in path.slots.iter() {
            let i = *sym as usize;
            self.log(i);
            self.slots[i] = slot.clone();
            self.level[i] = *saved;
            // What the path wrote counts as written now.
            self.stamp[i] = self.assignments;
            self.changed(i);
        }
        if own.is_some() {
            self.changed.push(path.slots.clone());
        }
        self.journal = path.journal.clone();
        self.marks = path.marks.clone();
        for low in &mut self.lows {
            *low = (*low).min(path.low);
        }
    }

    /// `paths`, each a path from `fork`, made paths from where the running
    /// path stands, so that `fork` can close: a name the running path
    /// changed since and a path did not holds there what `fork` saw. The
    /// bindings stay as the running path has them. False when the running
    /// path changed nothing since `fork`.
    pub fn rebase(&mut self, fork: &Fork, paths: &mut [&mut PathEnv]) -> bool {
        self.effective += 1;
        let live = self.path(fork);
        if live.slots.is_empty() {
            return false;
        }
        self.rewind(fork);
        for path in paths.iter_mut() {
            for sym in live.slots.keys() {
                if !path.slots.contains_key(sym) {
                    self.grow(Sym(*sym));
                    let i = *sym as usize;
                    path.slots.insert_mut(*sym, (self.slots[i].clone(), self.level[i]));
                }
            }
            path.low = path.low.min(live.low);
        }
        self.resume(fork, &live);
        true
    }

    /// The slot `sym` now holds.
    pub fn slot_at(&self, sym: u32) -> Option<Binding> {
        self.slots.get(sym as usize).cloned().flatten()
    }

    /// Continue as if `sym` held `slot`: a state at a loop head joined with
    /// what the loop reached there, which holds more than this path knows.
    /// Not an assignment, so the save stack is not told.
    pub fn assume(&mut self, sym: u32, slot: Option<Binding>) {
        self.effective += 1;
        let i = sym as usize;
        self.grow(Sym(sym));
        self.log(i);
        self.slots[i] = slot;
        self.changed(i);
    }

    /// What a path has learned of a name that holds one of several
    /// meanings: that it holds `meaning`.  Not an assignment, so the save
    /// stack is not told.
    pub fn refine(&mut self, sym: Sym, meaning: Meaning) {
        self.effective += 1;
        let i = sym.0 as usize;
        let Some(Some(binding)) = self.slots.get(i) else { return };
        let binding = Binding { meaning, may: None, ..binding.clone() };
        self.log(i);
        self.slots[i] = Some(binding);
        self.changed(i);
    }

    /// The groups the running path stands in, outermost first: each one's
    /// kind and waiting `\aftergroup` tokens.
    pub fn groups(&self) -> Vec<(GroupKind, Vec<Tok>)> {
        groups(&self.marks)
    }

    /// Give `sym` a meaning its name determines, so that it is the same on
    /// every path and no rewind undoes it.
    pub fn constant(&mut self, sym: Sym, binding: Binding) {
        self.effective += 1;
        self.grow(sym);
        let i = sym.0 as usize;
        self.slots[i] = Some(Binding { global: true, ..binding });
        self.level[i] = 0;
        self.note_write(sym.0, ReadKind::Meaning.bit() | ReadKind::Value.bit());
    }

    /// Least upper bound of two paths from `fork` that stand in the same
    /// groups. A name they left with different meanings has one of them.
    /// `after[i]` is the `\aftergroup` tokens of group `i` where they differ
    /// (tex.web § 326). With `widen`, a slot that grows holds anything of its
    /// kind. Also returns the names the bound holds more of than `a`, and
    /// whether its waiting tokens grew. The bindings are left as `fork` saw
    /// them.
    pub fn join_paths(
        &mut self,
        fork: &Fork,
        a: &PathEnv,
        b: &PathEnv,
        typed: &Typed,
        widen: bool,
        after: &[Option<Vec<Token>>],
    ) -> (PathEnv, Vec<Sym>, bool) {
        self.rewind(fork);
        let mut touched: Vec<u32> = a.slots.keys().chain(b.slots.keys()).copied().collect();
        touched.sort_unstable();
        touched.dedup();
        let mut grown = Vec::new();
        // `a`'s map, shared where the join keeps its binding.
        let mut slots = a.slots.clone();
        for sym in touched {
            self.grow(Sym(sym));
            let i = sym as usize;
            let side =
                |p: &PathEnv| p.slots.get(&sym).map_or_else(|| (self.slots[i].clone(), self.level[i]), Clone::clone);
            let ((old, x), (new, y)) = (side(a), side(b));
            // A group end puts back what either path saved (tex.web § 283
            // keeps only what both made global).
            let level = x.max(y);
            let joined = join(old.clone(), new, self.sets, Some(typed));
            let joined = match widen {
                true => widen_slot(&old, joined, self.sets, typed),
                false => joined,
            };
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
            // What the groups saved of the nest is joined like the bindings.
            mark.nest = match (mark.nest, other.nest) {
                (Some(x), Some(y)) => Some(x.join(y)),
                (x, y) => x.or(y),
            };
        }
        let low = a.low.min(b.low);
        let journal = self.join_journals(fork, a, b, low, &mut marks);
        (PathEnv { slots, journal, marks: Rc::new(marks), low }, grown, waiting)
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
        for (g, (mine, theirs)) in x.into_iter().zip(y).enumerate() {
            if g > 0 && marks[g - 1].journal >= base {
                marks[g - 1].journal = journal.len();
            }
            let mut merged: Vec<Entry> = Vec::with_capacity(mine.len() + theirs.len());
            for (entry, other) in mine.into_iter().map(|e| (e, b)).chain(theirs.into_iter().map(|e| (e, a))) {
                let entry = match entry {
                    Entry::Meaning { sym, old, level, alias, stamp } => {
                        let old = match global(other, sym) {
                            Some(kept) => join(old, Some(kept), self.sets, None),
                            // A name the other path did not save in this
                            // group: its end leaves what that path holds, set
                            // outside the group or at the fork.
                            None if is_name(&old) => {
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
    /// Whether two entries save the same slot.
    fn same_slot(&self, other: &Entry) -> bool {
        match (self, other) {
            (Entry::Meaning { sym: a, .. }, Entry::Meaning { sym: b, .. }) => a == b,
            (Entry::Catcode { character: a, .. }, Entry::Catcode { character: b, .. }) => a == b,
            _ => false,
        }
    }

    /// Two paths saved the same slot: what either may put back.
    fn join(&mut self, other: Entry, sets: SetLimits) {
        match (self, other) {
            (Entry::Meaning { old, stamp, .. }, Entry::Meaning { old: theirs, stamp: at, .. }) => {
                *old = join(old.clone(), theirs, sets, None);
                *stamp = (*stamp).max(at);
            }
            (Entry::Catcode { old, old_unknown, .. }, Entry::Catcode { old: theirs, old_unknown: unknown, .. }) => {
                *old_unknown |= unknown || *old != theirs;
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
    pub fn groups(&self) -> Vec<(GroupKind, Vec<Tok>)> {
        groups(&self.marks)
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

fn groups(marks: &[Mark]) -> Vec<(GroupKind, Vec<Tok>)> {
    marks.iter().map(|m| (m.kind, m.after.iter().map(|t| t.tok).collect())).collect()
}

/// How many meanings a name may be left with by joins before it is widened
/// to an unknown one, unless `limits.meaning_set` says otherwise.
pub const MAY_MEANINGS: usize = 5;

/// How many meanings and how many values a join keeps as a set
/// (`limits.meaning_set`, `limits.value_set`, `limits.interval_set`).
#[derive(Clone, Copy, Debug)]
pub struct SetLimits {
    pub meanings: usize,
    pub values: usize,
    pub intervals: usize,
}

impl Default for SetLimits {
    fn default() -> Self {
        SetLimits { meanings: MAY_MEANINGS, values: crate::value::MAY_VALUES, intervals: crate::value::MAY_INTERVALS }
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

pub fn same_slot(a: &Option<Binding>, b: &Option<Binding>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => same_meaning(x, y) && same_value(&x.value, &y.value) && x.certain == y.certain,
        _ => false,
    }
}

fn same_meaning(x: &Binding, y: &Binding) -> bool {
    x.meaning == y.meaning
        && match (&x.may, &y.may) {
            (None, None) => true,
            (Some(p), Some(q)) => p.len() == q.len() && p.iter().all(|m| q.contains(m)),
            _ => false,
        }
}

fn same_value(a: &Value, b: &Value) -> bool {
    a == b || matches!((a, b), (Value::Unknown, Value::Unknown))
}

/// Widening at a loop head. A slot the loop still changes after as many
/// passes as widening waits for may hold anything of its kind from then on:
/// an unknown meaning (a switch stays an undecided switch) and an unknown
/// value. No later pass can grow past that. Macros without parameters keep
/// their texts with every run of digits as one `digits` token
/// ([`digit_runs`]), as long as that leaves at most `sets.meanings` texts:
/// a loop that appends digits to a number stays a number.
pub fn widen_slot(old: &Option<Binding>, new: Option<Binding>, sets: SetLimits, typed: &Typed) -> Option<Binding> {
    if same_slot(old, &new) {
        return new;
    }
    let (Some(x), Some(y)) = (old, &new) else { return join(old.clone(), new, sets, Some(typed)) };
    let (meaning_grows, value_grows) = (!same_meaning(x, y), !same_value(&x.value, &y.value));
    let (xv, yv) = (x.value.num(), y.value.num());
    let mut joined = join(old.clone(), new, sets, Some(typed))?;
    if meaning_grows {
        use crate::builtins::{Cond, Primitive};
        let switch = matches!(joined.meaning.prim(), Some(Primitive::If(_)));
        // The texts are kept only while they are no more than the head
        // had: a set that grows every pass is not a widening, and its
        // hull is.
        let had = numbers(x, sets, typed).map_or(usize::MAX, |t| t.len());
        let texts = numbers(&joined, sets, typed).filter(|t| t.len() <= had);
        (joined.meaning, joined.may) = match texts {
            _ if switch => (Meaning::Primitive(Primitive::If(Cond::Opaque)), None),
            Some(mut texts) if texts.len() == 1 => (texts.pop().expect("one text"), None),
            Some(texts) => (Meaning::Unknown, Some(texts.into())),
            None => (macro_hull(&joined, typed).unwrap_or(Meaning::Unknown), None),
        };
    }
    if value_grows {
        // A bound that still moves goes to the end of the kind's range.
        joined.value = match (xv, yv) {
            // An empty hull (a family no write reached yet) grows by joining.
            (Some((a, p)), Some((b, q))) if a == b && p.lo > p.hi => Value::Num { quantity: a, num: p.join(&q) },
            (Some((a, p)), Some((b, q))) if a == b => Value::Num { quantity: a, num: p.widen(&p.join(&q)) },
            _ => Value::Unknown,
        };
    }
    Some(joined)
}

/// A macro without parameters whose text is any text of the classes its
/// texts have, when every meaning `b` may have is such a macro with the
/// same prefixes: widened, it stays one, which `\ifx` can still tell from
/// other kinds. Its length is not bounded: a widened write may repeat.
fn macro_hull(b: &Binding, typed: &Typed) -> Option<Meaning> {
    let members = b.may.as_deref().unwrap_or(std::slice::from_ref(&b.meaning));
    let mut first: Option<&crate::tex::MacroDef> = None;
    for m in members {
        let Meaning::Macro(d) = m else { return None };
        let plain = d.parameter_text.items.is_empty() && !d.parameter_text.brace_end && d.arg_spec.is_none();
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
    let text: std::rc::Rc<[Token]> = [text_hull(texts, typed)].into();
    Some(Meaning::Macro(std::rc::Rc::new(crate::tex::MacroDef { replacement_text: text, ..def.clone() })))
}

/// One typed token for any text of the classes `texts` have, at least
/// one token when each has one.
fn text_hull<'t>(texts: impl Iterator<Item = &'t [Token]>, typed: &Typed) -> Token {
    let (mut one, mut class, mut span) = (true, TextClass::EMPTY, None);
    for text in texts {
        one &= !text.is_empty();
        for t in text {
            span = span.or(Some(t.span));
            class = class.union(typed.classify(t).map_or_else(|| TextClass::of(t.tok), |c| c.0));
        }
    }
    Token::new(Tok::Cs(typed.make(class, one, None)), span.unwrap_or_default())
}

/// The meanings `b` may have, each a macro without parameters, with every
/// digit run in their texts made one `digits` token: `None` when one is not
/// such a macro or there are more than `sets.meanings` texts.
fn numbers(b: &Binding, sets: SetLimits, typed: &Typed) -> Option<Vec<Meaning>> {
    let members = match (&b.may, &b.meaning) {
        (Some(set), _) => set.to_vec(),
        (None, m) => vec![m.clone()],
    };
    let mut texts: Vec<Meaning> = Vec::new();
    for m in members {
        let Meaning::Macro(def) = m else { return None };
        if !def.parameter_text.items.is_empty() || def.parameter_text.brace_end {
            return None;
        }
        let text = unknown_runs(&def.replacement_text, typed);
        let m = Meaning::Macro(std::rc::Rc::new(crate::tex::MacroDef { replacement_text: text, ..(*def).clone() }));
        if !texts.contains(&m) {
            texts.push(m);
        }
    }
    (texts.len() <= sets.meanings).then_some(texts)
}

/// The names of typed unknown text, for widening without the machine:
/// what a token stands for, and the token for a class.
/// What parts a typed name from its bound: the most tokens it may hold.
pub const LEN_MARK: char = '\u{5}';
/// The longest bound a typed name carries: a sign and 19 digits.
pub const MAX_LEN: u32 = 20;
/// The classes whose text carries a bound: what `\the` and `\number`
/// print (tex.web §§ 65, 103).
pub const BOUNDED: [TextClass; 2] = [TextClass::DIGIT, TextClass::NUMBER];

/// Where the typed name of `class`, lower bound `one`, at most `len`
/// tokens and `serial` (0 the canonical one) stands in the typed names.
pub fn bounded_index(class: TextClass, one: bool, len: u32, serial: usize) -> Option<usize> {
    let b = BOUNDED.iter().position(|c| *c == class)?;
    let serials = crate::machine::REST_SERIALS + 1;
    (1..=MAX_LEN).contains(&len).then(|| {
        crate::machine::BOUNDED_BASE
            + (((b * 2 + usize::from(one)) * MAX_LEN as usize + len as usize - 1) * serials + serial)
    })
}

/// The most tokens unknown text `t` may hold: one for an unknown
/// character, the bound a typed name carries, or the length of the
/// spelling of the interval typed digits lie in. Known digits have none:
/// a loop's counter is any digits, whatever its length.
pub fn text_len(interner: &Interner, t: &Token) -> Option<u32> {
    let name = match t.tok {
        Tok::Chr(c, _) if crate::tex::is_unknown_char(c) => return Some(1),
        Tok::Cs(sym) => interner.name(sym),
        _ => return None,
    };
    if let Some((_, len)) = name.rsplit_once(LEN_MARK) {
        return len.parse().ok();
    }
    let typed = name.strip_prefix(crate::scan::TYPED_DIGITS)?;
    if let Some((_, cut)) = typed.split_once('@') {
        let places: Vec<u32> = cut.split(':').filter_map(|x| x.parse().ok()).collect();
        return Some(places.get(3)? - places.get(2)?);
    }
    let (lo, hi) = typed.split_once('#')?.0.split_once("..")?;
    let (lo, hi): (i64, i64) = (lo.parse().ok()?, hi.parse().ok()?);
    let digits = |n: i64| n.unsigned_abs().checked_ilog10().unwrap_or(0) + 1;
    Some(digits(lo).max(digits(hi)) + u32::from(lo < 0))
}

/// [`Typed`] of machine `$m` over `$syms` (its `typed_syms`); a macro so
/// the borrow of `$m` splits by field.
macro_rules! typed {
    ($m:expr, $syms:expr) => {
        crate::env::Typed { interner: &$m.out.interner, syms: &$syms, digits: $m.unknown_digits, more: $m.unknown_more }
    };
}
pub(crate) use typed;

pub struct Typed<'a> {
    pub interner: &'a Interner,
    /// The name per class and lower bound, `class * 2 + one`.
    pub syms: &'a [Sym],
    pub digits: Sym,
    pub more: Sym,
}

impl Typed<'_> {
    /// The class, lower bound and control sequences a typed name spells,
    /// `None` for any other name.
    pub fn parse_name(name: &str) -> Option<(TextClass, bool, Vec<&str>)> {
        let name = name.split_once(crate::machine::REST_SERIAL).map_or(name, |(name, _)| name);
        if name == crate::machine::MORE_DIGITS {
            return Some((TextClass::DIGIT, false, Vec::new()));
        }
        let rest = name.strip_prefix(crate::machine::UNKNOWN_NAME)?.strip_prefix(crate::machine::TYPED_TEXT)?;
        let mut parts = rest.split(crate::machine::UNKNOWN_NAME_GAP);
        let head = parts.next()?;
        let (spelled, one) = head.split_at(head.len().checked_sub(1)?);
        Some((TextClass::parse(spelled)?, one == "+", parts.collect()))
    }

    /// What `t` stands for when it is unknown text, one unknown character
    /// or a known digit: its class and whether it is at least one token.
    pub fn classify(&self, t: &Token) -> Option<(TextClass, bool)> {
        match t.tok {
            Tok::Chr(c, Catcode::Other) if c.is_ascii_digit() => Some((TextClass::DIGIT, true)),
            Tok::Chr(c, _) if crate::tex::is_unknown_char(c) => Some((TextClass::of(t.tok), true)),
            Tok::Cs(sym) if sym == self.digits => Some((TextClass::DIGIT, true)),
            Tok::Cs(sym) if sym == self.more => Some((TextClass::DIGIT, false)),
            Tok::Cs(sym) => {
                let name = self.interner.name(sym);
                if name.starts_with(crate::scan::TYPED_DIGITS) {
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
            out.push(Token::new(Tok::Cs(typed.make(class, one, len)), span));
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

/// [`join`] for the loop heads in [`crate::machine`].
/// `binding` with any value, and where it held a macro the [`macro_hull`]
/// of its meanings, else any meaning.
fn widened(mut binding: Binding, typed: &Typed) -> Binding {
    binding.value = Value::Unknown;
    if matches!(binding.meaning, Meaning::Macro(_)) || binding.may.is_some() {
        binding.meaning = macro_hull(&binding, typed).unwrap_or(Meaning::Unknown);
        binding.may = None;
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
            let same = x.meaning == y.meaning && set.as_ref().is_none_or(|s| s.len() == 1);
            let mut defs = x.defs;
            defs.extend(y.defs.iter().copied());
            defs.sort_unstable();
            defs.dedup();
            let mut value = x.value.clone().join(&y.value, sets);
            if matches!(value, Value::Unknown)
                && let Some(typed) = typed
                && let Some(text) = typed_toks_join(&[&x.value, &y.value], typed)
            {
                value = Value::Toks(text);
            }
            // A switch set on one path and not on another is still a
            // conditional, only an undecided one. So is a join of switches.
            use crate::builtins::{Cond, Primitive};
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
                (false, false, Some(set)) => match typed.and_then(|typed| typed_join(&set, typed)) {
                    Some(meaning) => (meaning, None),
                    None => (Meaning::Unknown, None),
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

/// One macro for the parameterless macros `set`: their common beginning
/// and end, and typed unknown text for what lies between, when that is
/// balanced and holds no control sequence. `None` otherwise.
fn typed_join(set: &[Meaning], typed: &Typed) -> Option<Meaning> {
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
    let head =
        (0..first.len()).take_while(|&i| texts.iter().all(|t| t.get(i).is_some_and(|x| x.tok == first[i].tok))).count();
    let tail = (0..first.len() - head)
        .take_while(|&i| {
            texts.iter().all(|t| t.len() - head > i && t[t.len() - 1 - i].tok == first[first.len() - 1 - i].tok)
        })
        .count();
    let mut class = TextClass::EMPTY;
    let mut one = true;
    // The hull of the middles' bounds, each the sum of its tokens'.
    let mut len = Some(0);
    for t in &texts {
        let middle = &t[head..t.len() - tail];
        one &= !middle.is_empty();
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
            class = class.union(match typed.classify(x) {
                Some((c, _)) => c,
                None => TextClass::of(x.tok),
            });
        }
        if depth != 0 || class.contains(TextClass::CS) {
            return None;
        }
    }
    let span = first.get(head).or(first.last()).map_or(Span::default(), |t| t.span);
    let mut text = first[..head].to_vec();
    text.push(Token::new(Tok::Cs(typed.make(class, one, len)), span));
    text.extend_from_slice(&first[first.len() - tail..]);
    let def = def?;
    Some(Meaning::Macro(std::rc::Rc::new(crate::tex::MacroDef { replacement_text: text.into(), ..(**def).clone() })))
}

/// Path switches and the slots they carried, for `--stats -vvvv`.
pub static RESUMES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub static RESUME_SLOTS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Token lists past `limits.value_set`: their common beginning and end
/// around typed unknown text, as [`typed_join`] widens macros. A pgfmath
/// stack keeps its braces and its numbers stay digits (tex.web § 1227).
fn typed_toks_join(values: &[&Value], typed: &Typed) -> Option<std::rc::Rc<[Token]>> {
    let mut set = Vec::new();
    for v in values {
        let lists = match v {
            Value::Toks(t) => vec![t.clone()],
            Value::OneOf(m) => {
                m.iter().map(|v| if let Value::Toks(t) = v { Some(t.clone()) } else { None }).collect::<Option<_>>()?
            }
            _ => return None,
        };
        for t in lists {
            let def = crate::tex::MacroDef {
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
    match typed_join(&set, typed)? {
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
            .all(|t| matches!(t.tok, crate::tex::Tok::Chr(_, cat) if !matches!(cat, Catcode::Active | Catcode::Param)));
    plain.then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A widened macro of digits stays a parameterless macro of unknown
    /// digits: its class survives, `\ifx` still sees a macro.
    #[test]
    fn widened_macro_keeps_its_text_class() {
        let interner = Interner::default();
        let syms: Vec<Sym> = (0..256).map(|i| Sym(1000 + i)).collect();
        let typed = Typed { interner: &interner, syms: &syms, digits: Sym(1), more: Sym(2) };
        let digit = |c| Token::new(Tok::Chr(c, crate::tex::Catcode::Other), Span::default());
        let def = crate::tex::MacroDef {
            parameter_text: Default::default(),
            arg_spec: None,
            replacement_text: vec![digit('1'), digit('2')].into(),
            long: false,
            outer: false,
            protected: false,
        };
        let b = widened(Binding::builtin(Meaning::Macro(std::rc::Rc::new(def))), &typed);
        let Meaning::Macro(d) = &b.meaning else { panic!("not a macro: {:?}", b.meaning) };
        assert_eq!(d.replacement_text.iter().map(|t| t.tok).collect::<Vec<_>>(), vec![Tok::Cs(Sym(1))]);
        assert!(!b.certain);
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
