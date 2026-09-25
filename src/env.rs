//! The save stack: dense slot table plus undo journal (The TeXbook § 275).
//! Drives conditional analysis: run branch, capture, rollback, join.

use std::collections::HashMap;

use crate::tex::{Catcode, CatcodeTable, Meaning, Sym, Token, Span};
use crate::mode::Nest;
use crate::value::Value;

pub type NodeId = u32;

/// What one control sequence is bound to: its [`Meaning`], its [`Value`], and
/// the dependency-graph nodes ([`NodeId`]) that defined it. [`Env`] stores one per
/// active [`Sym`].
#[derive(Clone, Debug)]
pub struct Binding {
    pub meaning: Meaning,
    pub value: Value,
    pub defs: Vec<NodeId>,
    pub certain: bool,
    /// Set by `\global`, `\gdef`, `\xdef`, `\glet`.
    pub global: bool,
    /// The meanings a join of paths left the name with, when they differ
    /// and are few: it has one of them.  `meaning` is then unknown, which is
    /// what everything that does not look at the set sees.
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
    },
    Catcode {
        character: char,
        old: Catcode,
    },
}

/// Kind of group (tex.web § 269): `{`/`}`, `\begingroup`/`\endgroup`, or a
/// pair of `$`. Each closer closes only its opener's kind, which
/// [`GroupKind::closes`] checks and [`Env::pop_group`] enforces.
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

    /// Each entry from position `mark` on, newest first.
    fn since(&self, mark: usize) -> impl Iterator<Item = &Entry> {
        let above = self.frozen_len.saturating_sub(mark);
        let mut chunks = std::iter::successors(self.frozen.as_ref(), |(chunk, _)| chunk.parent.as_ref())
            .flat_map(|(chunk, n)| chunk.entries[..*n].iter().rev());
        let frozen = std::iter::from_fn(move || chunks.next()).take(above);
        let skip = mark.saturating_sub(self.frozen_len);
        self.top.iter().skip(skip).rev().chain(frozen)
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
}

/// The meaning table and the save stack: [`Binding`]s are pushed and undone
/// per group, which is what `\begingroup` and `}` do.
#[derive(Default)]
pub struct Env {
    slots: Vec<Option<Binding>>,
    /// The group level each slot's binding was made at, 0 for level one
    /// (tex.web § 221 `eq_level`): a local assignment saves the old binding
    /// only once per level (§ 277), and a `\global` one makes it level one,
    /// which keeps it when the saving group ends (§ 283).
    level: Vec<u32>,
    journal: Journal,
    marks: Vec<Mark>,
    /// Group level floors per analyzed region (closers belong to arms).
    floors: Vec<usize>,
    /// While a path through an undecided conditional runs, every slot it
    /// changes, with what the slot held before, so that the next path can
    /// start from the same bindings.
    trail: Vec<(u32, Option<Binding>, u32)>,
    forks: usize,
    /// Names assigned since [`Env::clear_recent`], for widening a loop that
    /// does not end to every state it could have left.
    recent: Vec<u32>,
    /// `\let\tex_escapechar:D\escapechar`: the engine primitive a name was
    /// made equal to, for primitives satex models alike (tex.web keeps the
    /// primitive's `chr` code in the copy).  A parameter's value lives with
    /// that primitive, and `\meaning` and `\ifx` tell such copies apart.
    pub aliases: HashMap<Sym, Sym>,
    /// Assignments made so far: a loop that assigns is changing state, which
    /// a loop that only re-expands itself is not.
    pub assignments: u64,
    /// How large a set of meanings or values a join keeps.
    pub sets: SetLimits,
    /// While a package cache is being captured: every slot read, as a bit
    /// set, which is what the package's effect may depend on.
    tracking: std::cell::Cell<bool>,
    reads: std::cell::RefCell<Reads>,
}

/// Which part of a binding a read looked at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
    /// Only whether the name has definition sites: what the dependency graph
    /// links to them looks at; which sites they are, a package cache links
    /// again where it is replayed.
    Defs,
    /// Only whether the meaning is a text copied through: a macro without
    /// parameters whose replacement text, all characters, an `\edef` of the
    /// same name copies into its new text without looking at it.  What the
    /// text is flows into that definition, which a replay makes again from
    /// the text it finds.
    Copy,
    /// Only what `\ifx` compares a macro with anything else by: that it is
    /// a macro, or else the whole meaning (tex.web § 507).
    Shape,
}

impl ReadKind {
    fn bit(self) -> u8 {
        match self {
            ReadKind::Meaning => 1,
            ReadKind::Value => 2,
            ReadKind::Kept => 4,
            ReadKind::Defined => 8,
            ReadKind::Skip => 16,
            ReadKind::Defs => 32,
            ReadKind::Copy => 64,
            ReadKind::Shape => 128,
        }
    }
}

/// The reads recorded while a package cache is captured: per slot, which
/// parts were read while they still held what they held before the load, and
/// which parts the load has since overwritten, so that a later read of them
/// depends on nothing from before.  A group that ends puts back what the
/// parts held, and whether that came from before the load.
#[derive(Default)]
struct Reads {
    read: HashMap<u32, u8>,
    written: HashMap<u32, u8>,
    /// Slots assigned since the last [`Env::take_dirty`], in order.
    dirty: Vec<u32>,
    dirty_seen: std::collections::HashSet<u32>,
    /// Journal position, slot, and what was written of it when the journal
    /// saved it.
    saved: Vec<(usize, u32, u8)>,
    /// Slots whose text the load copied a text from before it into: a read
    /// of them reads that text, however the load has written them since.
    carrying: std::collections::HashSet<u32>,
    /// The slot whose meaning is being copied through, which a read of its
    /// meaning then is.
    copying: Option<u32>,
    /// Slots assigned since the segment began, and of those, the ones whose
    /// text holds what the slot held then, where and how long.
    touched: std::collections::HashSet<u32>,
    splices: HashMap<u32, (usize, usize)>,
}

/// The save stack as it stood where paths split: groups can open and close
/// on one path only, so it is copied rather than trailed.
#[derive(Clone)]
pub struct Fork {
    trail: usize,
    journal: Journal,
    marks: Vec<Mark>,
    floors: Vec<usize>,
}

/// What one path did to the bindings, relative to its [`Fork`].
#[derive(Clone)]
pub struct PathEnv {
    slots: HashMap<u32, (Option<Binding>, u32)>,
    journal: Journal,
    marks: Vec<Mark>,
    floors: Vec<usize>,
}

impl Env {
    pub fn with_capacity(n: usize) -> Self {
        Self { slots: vec![None; n], level: vec![0; n], ..Default::default() }
    }

    #[inline]
    fn grow(&mut self, sym: Sym) {
        let n = sym.0 as usize + 1;
        if self.slots.len() < n {
            self.slots.resize_with(n, || None);
            self.level.resize(n, 0);
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
        if self.forks == 0 {
            *reads.written.entry(sym).or_insert(0) |= bits;
        }
    }

    /// The reads recorded since the last call, which starts a new record of
    /// them; what has been written stays known.
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
        self.forks > 0
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

    /// Start or stop recording which slots are read; returns whether it was
    /// recording before.
    pub fn track_reads(&self, on: bool) -> bool {
        self.tracking.replace(on)
    }

    #[inline]
    pub fn meaning(&self, sym: Sym) -> Meaning {
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Meaning);
        }
        match self.slot(sym) {
            Some(b) => b.meaning.clone(),
            None => Meaning::Undefined,
        }
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

    /// The name's definition sites, to link a dependency-graph edge to: whatever
    /// they are, a package cache links such an edge again where it is
    /// replayed, so this reads nothing.
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

    /// How many names currently have a meaning.
    pub fn defined_count(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }

    /// Slots that hold a meaning, which is what a format cache stores: a
    /// rebuild and a cache hit then report the same number.
    pub fn meaning_count(&self) -> usize {
        self.slots
            .iter()
            .filter(|slot| {
                slot.as_ref().is_some_and(|binding| !matches!(binding.meaning, Meaning::Undefined))
            })
            .count()
    }

    /// Whether a path through an undecided conditional is running.
    pub fn forking(&self) -> bool {
        self.forks > 0
    }

    pub fn journal_len(&self) -> usize {
        self.journal.len()
    }

    /// Group depth.  0 is the outer, "global" level.
    pub fn depth(&self) -> usize {
        self.marks.len()
    }

    /// The group level TeX would report: analyzing an undecided arm pushes a
    /// mark of its own, which a real run never opens.
    pub fn group_level(&self) -> usize {
        self.marks.len() - self.floors.len()
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
        self.recent.push(sym.0);
        self.assignments += 1;
        if global {
            self.note_write(sym.0, writes);
            binding.global = true;
            self.level[i] = 0;
            self.slots[i] = Some(binding);
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
            self.journal.push(Entry::Meaning { sym: sym.0, old, level: self.level[i] });
        }
        self.note_write(sym.0, writes);
        self.level[i] = current;
        self.slots[i] = Some(binding);
    }

    pub fn value(&self, sym: Sym) -> Value {
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Value);
        }
        match self.slot(sym).map(|b| &b.value) {
            None | Some(Value::Range { .. }) => Value::Unknown,
            Some(value) => value.clone(),
        }
    }

    /// The abstract number a register holds (whether it is a dimension,
    /// and its interval): exact when known, `None` when not a number.
    pub fn num(&self, sym: Sym) -> Option<(bool, crate::value::Num)> {
        if self.tracking.get() {
            self.note_read(sym, ReadKind::Value);
        }
        self.slot(sym)?.value.num()
    }

    /// What a test taught a path of a register: that its value is `value`.
    /// Not an assignment, so the save stack is not told.
    pub fn refine_value(&mut self, sym: Sym, value: Value) {
        let i = sym.0 as usize;
        let Some(Some(binding)) = self.slots.get(i) else { return };
        let binding = Binding { value, ..binding.clone() };
        self.log(i);
        self.slots[i] = Some(binding);
    }

    /// Assign a value, keeping the rest of the binding: this reads nothing
    /// of the old value, only what is kept (an empty slot keeps what a new
    /// register slot has).
    pub fn set_value(&mut self, sym: Sym, value: Value, global: bool) {
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
        self.grow(sym);
        let i = sym.0 as usize;
        self.log(i);
        self.recent.push(sym.0);
        self.assignments += 1;
        self.note_write(sym.0, ReadKind::Value.bit());
        match &mut self.slots[i] {
            Some(binding) => binding.value = value,
            slot => {
                let mut binding = Binding::builtin(Meaning::Unknown);
                binding.value = value;
                *slot = Some(binding);
            }
        }
    }

    pub fn push_group(&mut self, kind: GroupKind, opened: Span) {
        self.marks.push(Mark { journal: self.journal.len(), kind, after: Vec::new(), opened, nest: None });
    }

    /// Record what the innermost group saved of the semantic nest.
    pub fn set_nest(&mut self, nest: Nest) {
        if let Some(mark) = self.marks.last_mut() {
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
        (self.marks.len() > self.floors.last().copied().unwrap_or(0))
            .then(|| self.marks.last().map(|m| m.kind))
            .flatten()
    }

    /// Pop a group, restoring assignments. Returns `\aftergroup` tokens (tex.web § 269).
    pub fn pop_group(&mut self, catcodes: &mut CatcodeTable) -> Option<Vec<Token>> {
        self.group_kind()?;
        let mark = self.marks.pop()?;
        self.unwind(mark.journal, Some(catcodes));
        Some(mark.after)
    }

    /// Discard a group (assignments join enclosing group; for file-end inside groups).
    pub fn discard_group(&mut self) -> bool {
        if self.marks.len() <= self.floors.last().copied().unwrap_or(0) {
            return false;
        }
        self.marks.pop().is_some()
    }

    /// `\aftergroup⟨token⟩`: save token, insert at group end (tex.web §§ 326, 282).
    pub fn save_after(&mut self, token: Token) {
        if let Some(mark) = self.marks.last_mut() {
            mark.after.push(token);
        }
    }

    pub fn save_catcode(&mut self, character: char, old: Catcode) {
        if !self.marks.is_empty() {
            self.journal.push(Entry::Catcode { character, old });
        }
    }

    /// The bindings as they stand, with no save stack: a sandbox run starts
    /// from them and cannot undo past them.
    pub fn snapshot(&self) -> Env {
        Env {
            slots: self.slots.clone(),
            level: vec![0; self.level.len()],
            aliases: self.aliases.clone(),
            ..Default::default()
        }
    }

    pub fn mark(&mut self) -> usize {
        self.push_group(GroupKind::Simple, Span::default());
        self.floors.push(self.marks.len());
        self.journal.len()
    }

    fn unwind(&mut self, to: usize, mut catcodes: Option<&mut CatcodeTable>) {
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
                Entry::Meaning { sym, old, level } => {
                    let index = sym as usize;
                    self.log(index);
                    self.level[index] = level;
                    self.slots[index] = old;
                    if self.tracking.get() {
                        let mut reads = self.reads.borrow_mut();
                        if reads.dirty_seen.insert(sym) {
                            reads.dirty.push(sym);
                        }
                    }
                }
                Entry::Catcode { character, old } => {
                    if let Some(table) = catcodes.as_deref_mut() {
                        table.set(character, old);
                    }
                }
            }
        }
    }

    /// Capture assignments since mark, then rollback (self-contained arm analysis).
    pub fn capture_and_rollback(
        &mut self,
        mark: usize,
        depth: usize,
        catcodes: &mut CatcodeTable,
    ) -> Captured {
        self.floors.pop();
        self.marks.truncate(depth);
        let mark = mark.min(self.journal.len());
        let mut out = HashMap::new();
        for entry in self.journal.since(mark) {
            if let Entry::Meaning { sym, .. } = entry
                && self.level[*sym as usize] != 0
            {
                out.entry(*sym).or_insert_with(|| self.slots[*sym as usize].clone());
            }
        }
        self.unwind(mark, Some(catcodes));
        out
    }

    /// The names assigned since [`Env::clear_recent`], and a way to put them
    /// back, for a package cache that replays a load.
    pub fn recent(&self) -> Vec<Sym> {
        self.recent.iter().map(|s| Sym(*s)).collect()
    }

    pub fn set_recent(&mut self, recent: Vec<Sym>) {
        self.recent = recent.into_iter().map(|s| s.0).collect();
    }

    pub fn clear_recent(&mut self) {
        self.recent.clear();
    }

    /// Every name assigned since [`Env::clear_recent`] may hold anything:
    /// its value is unknown, and so is its meaning if that is a macro.
    pub fn widen_recent(&mut self) {
        let mut recent = std::mem::take(&mut self.recent);
        recent.sort_unstable();
        recent.dedup();
        for sym in recent {
            if let Some(mut binding) = self.get(Sym(sym)).cloned() {
                binding.value = Value::Unknown;
                if matches!(binding.meaning, Meaning::Macro(_)) || binding.may.is_some() {
                    binding.meaning = Meaning::Unknown;
                    binding.may = None;
                }
                binding.certain = false;
                let global = binding.global;
                self.set(Sym(sym), binding, global);
            }
        }
        self.recent.clear();
    }

    #[inline]
    fn log(&mut self, i: usize) {
        if self.tracking.get() {
            let mut reads = self.reads.borrow_mut();
            reads.touched.insert(i as u32);
            reads.splices.remove(&(i as u32));
        }
        if self.forks > 0 {
            self.trail.push((i as u32, self.slots[i].clone(), self.level[i]));
        }
    }

    /// Paths split here.
    pub fn fork(&mut self) -> Fork {
        self.forks += 1;
        Fork {
            trail: self.trail.len(),
            journal: self.journal.snapshot(),
            marks: self.marks.clone(),
            floors: self.floors.clone(),
        }
    }

    /// The paths are done; the bindings stay as the last one left them.
    pub fn unfork(&mut self, fork: Fork) {
        self.forks -= 1;
        if self.forks == 0 {
            self.trail.clear();
        } else {
            debug_assert!(self.trail.len() >= fork.trail);
        }
    }

    /// What the running path changed since `fork`.
    pub fn path(&self, fork: &Fork) -> PathEnv {
        let mut slots = HashMap::new();
        for (sym, ..) in &self.trail[fork.trail..] {
            let i = *sym as usize;
            slots.entry(*sym).or_insert_with(|| (self.slots[i].clone(), self.level[i]));
        }
        PathEnv {
            slots,
            journal: self.journal.clone(),
            marks: self.marks.clone(),
            floors: self.floors.clone(),
        }
    }

    /// Back to the bindings `fork` saw.
    pub fn rewind(&mut self, fork: &Fork) {
        while self.trail.len() > fork.trail {
            let (sym, slot, saved) = self.trail.pop().expect("trail length checked");
            self.slots[sym as usize] = slot;
            self.level[sym as usize] = saved;
        }
        self.journal = fork.journal.clone();
        self.marks = fork.marks.clone();
        self.floors = fork.floors.clone();
    }

    /// Continue `path` from `fork`.
    pub fn resume(&mut self, fork: &Fork, path: &PathEnv) {
        self.rewind(fork);
        for (sym, (slot, saved)) in &path.slots {
            let i = *sym as usize;
            self.log(i);
            self.slots[i] = slot.clone();
            self.level[i] = *saved;
        }
        self.journal = path.journal.clone();
        self.marks = path.marks.clone();
        self.floors = path.floors.clone();
    }

    /// Where the trail stands: the names changed after this are what a
    /// path did since, for comparing states at a loop head.
    pub fn trail_mark(&self) -> usize {
        self.trail.len()
    }

    /// The slots changed since `mark`, each with what it held at `mark`
    /// and what it holds now.
    pub fn changed_since(&self, mark: usize) -> Vec<(u32, Option<Binding>, Option<Binding>)> {
        let mut seen = std::collections::HashSet::new();
        let mut out = Vec::new();
        for (sym, old, _) in self.trail.get(mark..).unwrap_or_default() {
            if seen.insert(*sym) {
                out.push((*sym, old.clone(), self.slots[*sym as usize].clone()));
            }
        }
        out
    }

    /// The slot `sym` now holds.
    pub fn slot_at(&self, sym: u32) -> Option<Binding> {
        self.slots.get(sym as usize).cloned().flatten()
    }

    /// Continue as if `sym` held `slot`: a state at a loop head joined with
    /// what the loop reached there, which holds more than this path knows.
    /// Not an assignment, so the save stack is not told.
    pub fn assume(&mut self, sym: u32, slot: Option<Binding>) {
        let i = sym as usize;
        self.grow(Sym(sym));
        self.log(i);
        self.slots[i] = slot;
    }

    /// Whether two paths have the same `\aftergroup` tokens waiting,
    /// which a join cannot combine.
    pub fn same_after(a: &PathEnv, b: &PathEnv) -> bool {
        a.marks.len() == b.marks.len()
            && a.marks.iter().zip(&b.marks).all(|(x, y)| {
                x.after.len() == y.after.len() && x.after.iter().zip(&y.after).all(|(p, q)| p.tok == q.tok)
            })
    }

    /// Whether two paths stand in the same groups, which a join needs.
    pub fn same_groups(a: &PathEnv, b: &PathEnv) -> bool {
        a.marks.len() == b.marks.len()
            && a.marks.iter().zip(&b.marks).all(|(x, y)| x.kind == y.kind)
    }

    /// What a path has learned of a name that holds one of several
    /// meanings: that it holds `meaning`.  Not an assignment, so the save
    /// stack is not told.
    pub fn refine(&mut self, sym: Sym, meaning: Meaning) {
        let i = sym.0 as usize;
        let Some(Some(binding)) = self.slots.get(i) else { return };
        let binding = Binding { meaning, may: None, ..binding.clone() };
        self.log(i);
        self.slots[i] = Some(binding);
    }

    /// Continue from `fork` with the least upper bound of the paths: a name
    /// they left with different meanings has an unknown one.  The save stack
    /// is the first path's.
    pub fn join_paths(&mut self, fork: &Fork, paths: &[PathEnv]) {
        let Some(first) = paths.first() else { return };
        self.rewind(fork);
        let mut touched: Vec<u32> = paths.iter().flat_map(|p| p.slots.keys().copied()).collect();
        touched.sort_unstable();
        touched.dedup();
        for sym in touched {
            let i = sym as usize;
            let base = self.slots[i].clone();
            let joined = paths
                .iter()
                .map(|p| p.slots.get(&sym).map_or_else(|| base.clone(), |(slot, _)| slot.clone()))
                .reduce(|a, b| join(a, b, self.sets))
                .flatten();
            let saved = first.slots.get(&sym).map_or(self.level[i], |(_, saved)| *saved);
            self.log(i);
            self.slots[i] = joined;
            self.level[i] = saved;
        }
        self.journal = first.journal.clone();
        self.marks = first.marks.clone();
        // What the groups saved of the nest is joined like the bindings.
        for path in &paths[1..] {
            for (mark, other) in self.marks.iter_mut().zip(&path.marks) {
                mark.nest = match (mark.nest, other.nest) {
                    (Some(a), Some(b)) => Some(a.join(b)),
                    (a, b) => a.or(b),
                };
            }
        }
        self.floors = first.floors.clone();
    }

    pub fn merge(&mut self, alternatives: Vec<Captured>) {
        let mut touched: Vec<u32> = alternatives.iter().flat_map(|c| c.keys().copied()).collect();
        touched.sort_unstable();
        touched.dedup();
        for sym in touched {
            let base = self.slots.get(sym as usize).and_then(Clone::clone);
            let joined = alternatives
                .iter()
                .map(|c| c.get(&sym).cloned().unwrap_or_else(|| base.clone()))
                .reduce(|a, b| join(a, b, self.sets))
                .flatten();
            if let Some(b) = joined {
                self.set(Sym(sym), b, false);
            }
        }
    }
}

pub type Captured = HashMap<u32, Option<Binding>>;

/// How many meanings a name may be left with by joins before it is widened
/// to an unknown one, unless `limits.meaning_set` says otherwise.
pub const MAY_MEANINGS: usize = 5;

/// How many meanings and how many values a join keeps as a set
/// (`limits.meaning_set`, `limits.value_set`).
#[derive(Clone, Copy, Debug)]
pub struct SetLimits {
    pub meanings: usize,
    pub values: usize,
}

impl Default for SetLimits {
    fn default() -> Self {
        SetLimits { meanings: MAY_MEANINGS, values: crate::value::MAY_VALUES }
    }
}

/// Whether two slots hold the same abstract state: the same meaning (or
/// set of meanings), value and certainty.  Which definitions made them does
/// not count: it is what a use depends on, not what it sees.
pub fn same_slot(a: &Option<Binding>, b: &Option<Binding>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            same_meaning(x, y) && same_value(&x.value, &y.value) && x.certain == y.certain
        }
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

/// Widening at a loop head: a slot the loop still changes after as many
/// passes as widening waits for may hold anything of its kind from then
/// on — an unknown meaning (a switch stays an undecided switch) and an
/// unknown value — which no later pass can grow past.
pub fn widen_slot(old: &Option<Binding>, new: Option<Binding>, sets: SetLimits) -> Option<Binding> {
    if same_slot(old, &new) {
        return new;
    }
    let (Some(x), Some(y)) = (old, &new) else { return join(old.clone(), new, sets) };
    let (meaning_grows, value_grows) = (!same_meaning(x, y), !same_value(&x.value, &y.value));
    let (xv, yv) = (x.value.num(), y.value.num());
    let mut joined = join(old.clone(), new, sets)?;
    if meaning_grows {
        use crate::builtins::{Cond, Primitive};
        let switch = matches!(joined.meaning.prim(), Some(Primitive::If(_)));
        joined.meaning = if switch { Meaning::Primitive(Primitive::If(Cond::Opaque)) } else { Meaning::Unknown };
        joined.may = None;
    }
    if value_grows {
        // A bound that still moves goes to the end of the kind's range.
        joined.value = match (xv, yv) {
            (Some((a, p)), Some((b, q))) if a == b => Value::Range { dimen: a, num: p.widen(&p.join(&q, sets.values), a) },
            _ => Value::Unknown,
        };
    }
    Some(joined)
}

/// [`join`] for the loop heads in [`crate::machine`].
pub fn join_slot(a: Option<Binding>, b: Option<Binding>, sets: SetLimits) -> Option<Binding> {
    join(a, b, sets)
}

/// Least upper bound of two possible bindings: a name the paths gave
/// different meanings has one of them, up to `sets.meanings` of them.
fn join(a: Option<Binding>, b: Option<Binding>, sets: SetLimits) -> Option<Binding> {
    match (a, b) {
        (None, None) => None,
        // A register or code never assigned on the other path holds its
        // default there, which is not known here.
        (Some(x), None) | (None, Some(x)) => Some(Binding { certain: false, value: Value::Unknown, ..x }),
        (Some(x), Some(y)) => {
            // The meanings either path may have given the name; `None` when
            // one of them is not known.
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
            let value = x.value.clone().join(&y.value, sets.values);
            // A switch set on one path and not on another is still a
            // conditional, only an undecided one; so is a join of switches.
            use crate::builtins::{Cond, Primitive};
            let switch = |m: &Meaning| matches!(m.prim(), Some(Primitive::If(Cond::True | Cond::False | Cond::Opaque)));
            let (meaning, may) = match (same, switch(&x.meaning) && switch(&y.meaning), set) {
                (true, ..) => (x.meaning, x.may),
                (false, true, _) => (Meaning::Primitive(Primitive::If(Cond::Opaque)), None),
                (false, false, Some(set)) if set.len() <= sets.meanings => (Meaning::Unknown, Some(set.into())),
                (false, false, _) => (Meaning::Unknown, None),
            };
            Some(Binding {
                meaning,
                may,
                value,
                defs,
                certain: x.certain && y.certain,
                global: x.global && y.global,
            })
        }
    }
}

/// The text a meaning copies through: a macro without parameters, not
/// `\protected` or `\outer`, whose replacement text is characters that
/// expanding does nothing to (none active, no `#`) and is not empty.
/// Expanding it in an `\edef` appends that text and looks at none of it.
pub fn copied_text(meaning: &Meaning) -> Option<&std::rc::Rc<[Token]>> {
    let Meaning::Macro(m) = meaning else { return None };
    let text = &m.replacement_text;
    let plain = m.arg_spec.is_none()
        && m.parameter_text.items.is_empty()
        && !m.parameter_text.brace_end
        && !m.protected
        && !m.outer
        && !text.is_empty()
        && text.iter().all(|t| matches!(t.tok, crate::tex::Tok::Chr(_, cat) if !matches!(cat, Catcode::Active | Catcode::Param)));
    plain.then_some(text)
}
