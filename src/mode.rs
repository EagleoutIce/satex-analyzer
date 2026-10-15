//! TeX's modes (tex.web §§ 211-213): what `\ifvmode`, `\ifhmode`, `\ifmmode`
//! and `\ifinner` ask. The run keeps the set of modes it may be in; paths
//! that disagree join to their union, and a test decides only when every
//! mode in the set answers alike. The nest lives in the save stack: a group
//! that begins its own list (a box, math, an insertion) saves the modes
//! around it (tex.web § 216 `push_nest`) and restores them at `pop_nest`.

use std::rc::Rc;

use crate::builtins::{BoxCmd, Cond, Material, ModeCmd, Primitive};
use crate::env::GroupKind;
use crate::facts::Severity;
use crate::machine::{Machine, Step};
use crate::tex::{Catcode, Meaning, Span, Sym, Tok, Token};
use crate::value::{Scaled, Value};

/// A set of TeX's six modes (tex.web § 211).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash, serde::Serialize, serde::Deserialize)]
pub struct Modes(u8);

impl Modes {
    pub const NONE: Modes = Modes(0);
    /// `vmode`: the main vertical list.
    pub const VERTICAL: Modes = Modes(1);
    /// `-vmode`: internal vertical, in a `\vbox`.
    pub const INTERNAL_VERTICAL: Modes = Modes(2);
    /// `hmode`: a paragraph.
    pub const HORIZONTAL: Modes = Modes(4);
    /// `-hmode`: restricted horizontal, in an `\hbox`.
    pub const RESTRICTED_HORIZONTAL: Modes = Modes(8);
    /// `mmode`: display math.
    pub const DISPLAY_MATH: Modes = Modes(16);
    /// `-mmode`: non-display math.
    pub const MATH: Modes = Modes(32);
    pub const ANY_VERTICAL: Modes = Modes(1 | 2);
    pub const ANY_HORIZONTAL: Modes = Modes(4 | 8);
    pub const ANY_MATH: Modes = Modes(16 | 32);
    /// tex.web § 211: the negative modes are the inner ones.
    pub const INNER: Modes = Modes(2 | 8 | 32);

    pub fn union(self, other: Modes) -> Modes {
        Modes(self.0 | other.0)
    }

    pub fn and(self, other: Modes) -> Modes {
        Modes(self.0 & other.0)
    }

    pub fn without(self, other: Modes) -> Modes {
        Modes(self.0 & !other.0)
    }

    /// At most one mode: the run knows which it is in.
    pub fn is_known(self) -> bool {
        self.0.count_ones() <= 1
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    pub fn meets(self, other: Modes) -> bool {
        self.0 & other.0 != 0
    }

    pub fn within(self, other: Modes) -> bool {
        self.0 & !other.0 == 0
    }

    /// Whether the run is in one of `class`: `None` when some of the modes
    /// it may be in are and some are not.
    pub fn test(self, class: Modes) -> Option<bool> {
        if self.within(class) && !self.is_empty() {
            Some(true)
        } else if !self.meets(class) {
            Some(false)
        } else {
            None
        }
    }

    /// Each mode of the set taken through `f`.
    pub fn map(self, f: impl Fn(Modes) -> Modes) -> Modes {
        (0..6).map(|bit| Modes(1 << bit)).filter(|m| self.meets(*m)).fold(Modes::NONE, |acc, m| acc.union(f(m)))
    }

    /// The names of the set's modes, as `\showlists` prints them (tex.web § 211).
    pub fn names(self) -> Vec<&'static str> {
        const NAMES: [&str; 6] =
            ["vertical", "internal vertical", "horizontal", "restricted horizontal", "display math", "math"];
        (0..6).filter(|bit| self.0 & (1 << bit) != 0).map(|bit| NAMES[bit]).collect()
    }
}

/// tex.web § 269: the group codes `\currentgrouptype` reports.
pub mod group {
    pub const SIMPLE: u8 = 1;
    pub const HBOX: u8 = 2;
    pub const ADJUSTED_HBOX: u8 = 3;
    pub const VBOX: u8 = 4;
    pub const VTOP: u8 = 5;
    pub const ALIGN: u8 = 6;
    pub const NO_ALIGN: u8 = 7;
    pub const MATH: u8 = 9;
    pub const DISC: u8 = 10;
    pub const INSERT: u8 = 11;
    pub const VCENTER: u8 = 12;
    pub const MATH_CHOICE: u8 = 13;
    pub const SEMI_SIMPLE: u8 = 14;
    pub const MATH_SHIFT: u8 = 15;
}

/// What a group saved of the semantic nest.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Nest {
    /// The modes, and the modes of the enclosing list, before the group.
    pub mode: Modes,
    pub list: Modes,
    /// tex.web § 269; `None` where paths disagree.
    pub code: Option<u8>,
    /// Whether the group holds a list of its own, whose end restores every
    /// mode. A math group `{…}` gives back only math.
    pub level: bool,
    /// Which part of a `\discretionary` or `\mathchoice`, or `EQNO`.
    pub part: u8,
    /// The box register a `\setbox` fills when the box ends, and whether
    /// globally (tex.web § 1077 `box_end`).
    pub target: Option<(Option<i64>, bool, Option<[(i64, i64); 2]>)>,
    /// What `\raise` and relatives shift the box by (tex.web § 1073).
    pub shift: Dim,
    /// How the box is packed (tex.web § 644).
    pub to: Packing,
    /// Whether anything may have been put on the enclosing list.
    pub material: bool,
    /// The enclosing list's natural size, and whether the box goes onto
    /// that list when it ends (tex.web § 1076 `box_end`).
    pub natural: Option<Natural>,
    pub placed: bool,
    /// How many characters the document had set when the group opened.
    pub from: u32,
    /// An `\halign`'s rows, or a `\noalign` in one.
    pub align: Option<Align>,
}

/// What an `\halign` keeps while its rows go onto the enclosing list
/// (tex.web §§ 770, 799).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Align {
    /// The `\tabskip` glue before the first column and after the last (§ 778).
    pub tabskip: Dim,
    /// The widest cell, once a row ended (§ 801).
    pub widest: Option<Dim>,
    /// A row has more than one column, whose widths are not followed.
    pub columns: bool,
    /// The alignment's width where every cell was set with its template
    /// (tex.web §§ 789-791) and is followed: its columns' widths and the
    /// `\tabskip` glue between them, summed (§ 801).
    pub exact: Option<Dim>,
}

impl Align {
    fn join(self, other: Align) -> Align {
        let pair = |a: Option<Dim>, b: Option<Dim>| match (a, b) {
            (Some(a), Some(b)) => Some(a.join(b)),
            (None, None) => None,
            _ => Some(Dim::ANY),
        };
        let exact = self.exact.zip(other.exact).map(|(a, b)| a.join(b));
        Align {
            tabskip: self.tabskip.join(other.tabskip),
            widest: pair(self.widest, other.widest),
            columns: self.columns || other.columns,
            exact,
        }
    }
}

/// The modes of an alignment's list: rows and cells.
const ALIGN_MODES: Modes = Modes(Modes::INTERNAL_VERTICAL.0 | Modes::RESTRICTED_HORIZONTAL.0);

impl Nest {
    pub fn join(self, other: Nest) -> Nest {
        Nest {
            mode: self.mode.union(other.mode),
            list: self.list.union(other.list),
            code: if self.code == other.code { self.code } else { None },
            level: self.level || other.level,
            part: self.part.max(other.part),
            target: if self.target == other.target { self.target } else { None },
            shift: if self.shift == other.shift { self.shift } else { Dim::ANY },
            to: if self.to == other.to { self.to } else { Packing::To(Dim::ANY) },
            material: self.material || other.material,
            natural: join_natural(self.natural, other.natural),
            placed: self.placed && other.placed,
            from: self.from.min(other.from),
            align: match (self.align, other.align) {
                (Some(a), Some(b)) => Some(a.join(b)),
                _ => None,
            },
        }
    }
}

/// A length satex knows only within an interval. `hpack` (tex.web § 649) and
/// `vpack` (§ 668) add the list's items up. An item satex cannot follow
/// widens the interval rather than being left out, to avoid under-approximating.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct Dim {
    pub lo: Scaled,
    pub hi: Scaled,
}

impl Default for Dim {
    fn default() -> Dim {
        Dim::exact(0)
    }
}

impl Dim {
    /// Every length TeX holds: the top of the lattice.
    pub const ANY: Dim = Dim { lo: -crate::value::MAX_DIMEN, hi: crate::value::MAX_DIMEN };

    pub const fn exact(v: Scaled) -> Dim {
        Dim { lo: v, hi: v }
    }

    /// A length whose sum passed `\maxdimen` is TeX's "Dimension too
    /// large", whose outcome is not modeled: any length then.
    fn bounded(lo: Scaled, hi: Scaled) -> Dim {
        let max = crate::value::MAX_DIMEN;
        if lo < -max || hi > max || lo > hi { Dim::ANY } else { Dim { lo, hi } }
    }

    pub fn range(lo: Scaled, hi: Scaled) -> Dim {
        Dim::bounded(lo, hi)
    }

    /// The length itself, when the interval holds only one.
    pub fn known(self) -> Option<Scaled> {
        (self.lo == self.hi).then_some(self.lo)
    }

    pub fn is_any(self) -> bool {
        self == Dim::ANY
    }

    pub fn extended_by(self, other: Dim) -> Dim {
        Dim::bounded(self.lo + other.lo, self.hi + other.hi)
    }

    pub fn reduced_by(self, other: Dim) -> Dim {
        Dim::bounded(self.lo - other.hi, self.hi - other.lo)
    }

    /// tex.web § 649: the largest height on the list.
    pub fn max(self, other: Dim) -> Dim {
        Dim::bounded(self.lo.max(other.lo), self.hi.max(other.hi))
    }

    pub fn join(self, other: Dim) -> Dim {
        Dim::bounded(self.lo.min(other.lo), self.hi.max(other.hi))
    }

    /// The length taken between `lo` and `hi` times.
    fn times(self, lo: u32, hi: u32) -> Dim {
        let (lo, hi) = (i128::from(lo), i128::from(hi));
        let ends =
            [i128::from(self.lo) * lo, i128::from(self.lo) * hi, i128::from(self.hi) * lo, i128::from(self.hi) * hi];
        let (a, b) = (ends.iter().min().copied().unwrap_or(0), ends.iter().max().copied().unwrap_or(0));
        match (Scaled::try_from(a), Scaled::try_from(b)) {
            (Ok(a), Ok(b)) => Dim::bounded(a, b),
            _ => Dim::ANY,
        }
    }

    /// tex.web § 649: `hpack` starts height and depth at zero.
    pub(crate) fn at_least_zero(self) -> Dim {
        self.max(Dim::exact(0))
    }
}

/// The interline glue parameters (tex.web § 679).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
struct Interline {
    baseline: Dim,
    line: Dim,
    limit: Dim,
}

impl Interline {
    /// tex.web § 679: from one baseline to the next, past a depth `d` and a
    /// height `h`: `\baselineskip`, or `d+\lineskip+h` where the glue
    /// left would be below `\lineskiplimit`.
    fn distance(self, d: Dim, h: Dim) -> Dim {
        let s = d.extended_by(h);
        let t = self.baseline.reduced_by(self.limit);
        let skip = (s.lo <= t.hi).then_some(self.baseline);
        let line = (s.hi > t.lo).then(|| Dim::bounded(s.lo.max(t.lo + 1), s.hi).extended_by(self.line));
        match (skip, line) {
            (Some(a), Some(b)) => a.join(b),
            (a, b) => a.or(b).unwrap_or(Dim::ANY),
        }
    }
}

/// The lines a paragraph made at its forced breaks so far (tex.web
/// §§ 813-890), as the vertical list `post_line_break` builds of them, and
/// the line being set since.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Para {
    /// How many lines, at least and at most.
    pub(crate) lines: (u32, u32),
    /// The vertical list the lines go onto, as `vpack` sums it (tex.web
    /// § 668): the length down to its last box's baseline, and that box's
    /// depth. Its first height and `\prevdepth` are the paragraph's
    /// `first` and `prev_depth`.
    along: Dim,
    depth: Dim,
    /// The line parameters the lines were set with: `line_break` reads them
    /// at `\par`, so they must not have changed by then.
    shape: u64,
    /// The lines are not followed.
    pub(crate) lost: bool,
    /// A line before may not be `decent_fit` (tex.web § 817), so
    /// `\adjdemerits` may favour another break.
    pub(crate) rough: bool,
    /// Legal breakpoints (tex.web § 831) on the current line, or more.
    breakpoints: u32,
    /// The vertical list's tail before the lines: `\parskip`, or empty.
    below: Last,
    /// Glue came on the current line: the words after it may be hyphenated
    /// (tex.web § 866, § 894).
    glued: bool,
    /// The current line may hold a better break than none: a negative
    /// penalty, a negative or unknown infinite stretch (tex.web § 859).
    awkward: bool,
    /// After a forced break: the glue, kerns and penalties that follow are
    /// discarded (tex.web § 879).
    fresh: bool,
    /// The current line's items, for the line breaker
    /// ([`crate::linebreak`]); `None` where not followed.
    pub(crate) items: Option<u32>,
    /// The last line's fitness class (tex.web § 817).
    pub(crate) fit: u8,
}

impl Para {
    fn join(self, other: Para) -> Para {
        if self == other {
            return self;
        }
        Para {
            lines: (self.lines.0.min(other.lines.0), self.lines.1.max(other.lines.1)),
            along: self.along.join(other.along),
            depth: self.depth.join(other.depth),
            shape: self.shape,
            lost: self.lost || other.lost || (self.shape != other.shape && self.lines.1.min(other.lines.1) > 0),
            rough: self.rough || other.rough || self.lines != other.lines,
            breakpoints: self.breakpoints.max(other.breakpoints),
            glued: self.glued || other.glued,
            awkward: self.awkward || other.awkward || self.fresh != other.fresh,
            below: self.below.merge(other.below),
            fresh: self.fresh && other.fresh,
            items: if self.items == other.items { self.items } else { None },
            fit: if self.fit == other.fit { self.fit } else { crate::linebreak::UNFIT },
        }
    }
}

impl Dim {
    /// The value a document reads back: a number only where there is one,
    /// and never zero for a length satex could not follow.
    pub fn value(self) -> Value {
        match self.known() {
            Some(v) => Value::Dimen(v),
            None if self.is_any() => Value::Unknown,
            None => Value::from_num(crate::value::Quantity::Dimen, crate::value::Num::range(self.lo, self.hi)),
        }
    }
}

/// tex.web § 644: a box is packed to its list's natural size, `to` a
/// given size, or `spread` past the natural one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Packing {
    Natural,
    To(Dim),
    Spread(Dim),
}

impl Packing {
    /// The size the box gets, from the list's natural size.
    fn size(self, natural: Dim) -> Dim {
        match self {
            Packing::Natural => natural,
            Packing::To(to) => to,
            Packing::Spread(by) => natural.extended_by(by),
        }
    }
}

/// The natural size of the list being built, as `hpack` (tex.web § 649)
/// and `vpack` (§ 668) find it, and the tail `\lastskip` and `\unskip`
/// reach (§ 424, § 1105).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Natural {
    /// The sum along the list: the width of a horizontal list, the height
    /// of a vertical one (tex.web § 668 `x`).
    pub along: Dim,
    /// The largest height on a horizontal list (tex.web § 649 `h`).  Not
    /// used on a vertical one.
    pub height: Dim,
    /// The largest depth on a horizontal list, the depth the last box left
    /// on a vertical one (tex.web § 668 `d`).
    pub depth: Dim,
    /// A vertical list's width: the largest of its boxes' (tex.web § 668
    /// `w`).  Not used on a horizontal list.
    pub wide: Dim,
    /// The height of the first box or rule of a vertical list: `\vtop` takes
    /// it as its height (tex.web § 1087). Zero once anything else came
    /// first, `None` while the list is empty.
    pub first: Option<Dim>,
    /// Whether a vertical list already holds a box or rule, so that the
    /// next one takes interline glue (tex.web § 679 `prev_depth`).  On a
    /// paragraph: whether it may still hold nothing, so that `\par` may
    /// make no line (§ 1096).
    pub prior: bool,
    /// Whether `\boxmaxdepth` was set while this list was being built:
    /// `package` reads it before the group ends (tex.web § 1086), which is
    /// past where satex can still see it.
    pub capped: bool,
    /// The tail of the list, nearest first: what `\lastskip` and relatives
    /// read (tex.web § 424) and what `\unskip` and relatives uncover as
    /// they take items away (§ 1105).  `\sw@slant` in the LaTeX kernel
    /// reaches three deep.
    pub tail: [Last; TAIL],
    /// The font and character of the word being set that ligatures and
    /// kerns with what comes next, not yet counted (tex.web § 1034 `cur_l`).
    pub pending: Option<(u32, u32)>,
    /// A horizontal list's space factor (tex.web § 212), where known.
    pub space_factor: Option<i64>,
    /// Whether the character node `tail[0]` is a ligature (tex.web § 143),
    /// where known: `\lastnodetype` tells the two apart.
    pub ligature: Option<bool>,
    /// A `\noboundary` came: the word it ends, or the one it starts, meets
    /// no boundary character (tex.web §§ 1030, 1038).
    pub no_boundary: bool,
    /// A vertical list whose last item is glue or a kern: that item, and
    /// the height, depth and first height the list had before it (tex.web
    /// § 668 adds the depth before glue or a kern into the height), so
    /// that `\unskip` or `\unkern` taking it away restores them; with the
    /// three after it, which tell that nothing else changed them since.
    pub undo: Option<(Last, [(Dim, Dim, Option<Dim>); 2])>,
    /// How many boxes and rules came onto the list through
    /// [`State::append_item`]: with the tail, whether it holds nothing else.
    pub solid: u8,
    /// An internal vertical list whose tail is [`Last::Box`]: the one line
    /// it holds, its width, height and depth, and the left margin kern
    /// pdfTeX put at its start (`None`: not known).
    pub line: Option<Line>,
    /// In inline math: the last math character (family, font, char), whose
    /// italic correction and kern wait for the next (tex.web §§ 752, 755).
    /// `Some(None)`: not known.
    pub math_prev: Option<Option<(i64, u32, u32)>>,
    /// In inline math: the class of the last atom, for the space before
    /// the next (tex.web § 764). `Some(-1)`: not known.
    pub math_class: Option<i8>,
    /// In inline math: the type `\mathord` … `\mathinner` gave the next
    /// atom (tex.web § 1156).
    pub math_pending: Option<u8>,
    /// In inline math: the last atom is a binary one, which turns ordinary
    /// before a relation, closing, punctuation or the end (tex.web § 729):
    /// what its spacing would then take back. `Some(i64::MIN)`: not known.
    pub math_bin: Option<Scaled>,
    /// In inline math: a `^` (`true`) or `_` came after that character,
    /// whose script is the next character (tex.web § 1176).
    pub script: Option<bool>,
    /// In inline math: the last atom is a box of this height and depth,
    /// the nucleus a script after it sits on (tex.web § 756).
    pub math_box: Option<(Scaled, Scaled)>,
    /// In inline math: the style the list is set in, twice its size (0 text,
    /// 1 script, 2 scriptscript) plus one where cramped (tex.web § 688).
    pub style: u8,
    /// In inline math: the script just set, which a script of the other
    /// kind after it joins (tex.web § 759).
    pub scripted: Option<Scripted>,
    /// In inline math: the formula is no rigid box to the line breaker if
    /// this is not zero. A binary or relation atom came, after which it may
    /// break (tex.web § 761), or glue that may stretch or shrink: how many
    /// of them at most, `u32::MAX` where that is not known.
    pub math_breaks: u32,
    /// A paragraph's lines so far (tex.web § 815).
    pub para: Para,
    /// A vertical list's `\prevdepth` (tex.web § 219), `None` at
    /// `ignore_depth`; on a paragraph, that of the list it goes onto.
    pub prev_depth: Option<Dim>,
}

/// A script placed after its nucleus: the script box, and the list as it
/// was before and after.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Scripted {
    sup: bool,
    size: [Scaled; 3],
    before: [Dim; 3],
    after: Dim,
    prev: Option<Option<(i64, u32, u32)>>,
    boxed: Option<(Scaled, Scaled)>,
}

impl Natural {
    /// A list whose size satex lost but whose tail it still follows is not
    /// the same as one it never followed: every length widens.
    pub fn any() -> Natural {
        Natural {
            along: Dim::ANY,
            height: Dim::ANY,
            depth: Dim::ANY,
            wide: Dim::ANY,
            first: Some(Dim::ANY),
            prior: true,
            capped: true,
            tail: [Last::Unknown; TAIL],
            para: Para { lost: true, ..Para::default() },
            prev_depth: Some(Dim::ANY),
            ..Natural::default()
        }
    }

    /// tex.web § 679 `append_to_vlist`: the interline glue above a box of
    /// height `h`, none after `ignore_depth` (`None`).
    fn interline_glue(&self, h: Dim, skips: Option<Interline>) -> Dim {
        match (self.prev_depth, skips) {
            (None, _) => Dim::exact(0),
            (Some(d), Some(skips)) => skips.distance(d, h).reduced_by(d).reduced_by(h),
            (Some(_), None) => Dim::ANY,
        }
    }

    /// `count` lines of height `h` and depth `d` go onto the vertical list
    /// a paragraph keeps in `para` (tex.web § 890).
    fn stack_lines(&mut self, (lo, hi): (u32, u32), h: Dim, d: Dim, skips: Interline) {
        let glue = self.interline_glue(h, Some(skips));
        let p = &mut self.para;
        p.along = p.along.extended_by(p.depth).extended_by(glue).extended_by(h);
        if hi > 1 {
            p.along = p.along.extended_by(skips.distance(d, h).times(lo - 1, hi - 1));
        }
        p.depth = d;
        p.lines = (p.lines.0.saturating_add(lo), p.lines.1.saturating_add(hi));
        self.first.get_or_insert(h);
        self.prev_depth = Some(d);
    }

    /// An item went onto the list.
    fn pushed(&mut self, item: Last) {
        self.undo = None;
        for k in (1..TAIL).rev() {
            self.tail[k] = self.tail[k - 1];
        }
        // A lone box followed by an item is no longer the list's size (a
        // forced break starts a new line): taken away, it is some box.
        if self.tail[1] == Last::Box {
            self.tail[1] = Last::Other;
        }
        self.tail[0] = item;
        self.ligature = None;
    }

    /// An item came off the list. What it uncovers past the window is not
    /// followed.
    fn popped(&mut self) {
        self.undo = None;
        for k in 1..TAIL {
            self.tail[k - 1] = self.tail[k];
        }
        self.tail[TAIL - 1] = Last::Unknown;
        self.ligature = None;
    }

    /// An item that may or may not have been appended: the tail is the one
    /// or the other.
    fn perhaps(&mut self, item: Last) {
        let mut next = self.tail;
        next[0] = item.merge(self.tail[0]);
        for ((i, prev), cur) in self.tail[0..TAIL - 1].iter().enumerate().zip(self.tail[1..].iter()) {
            next[i + 1] = prev.merge(*cur);
        }
        self.tail = next;
        self.ligature = None;
    }

    /// tex.web § 651, § 669: glue and kerns add their size along the list.
    /// A vertical list first settles the depth the last box left.
    fn advance(&mut self, horizontal: bool, by: Dim) {
        if horizontal {
            // tex.web § 879: discarded after a forced break.
            if !self.para.fresh {
                self.along = self.along.extended_by(by);
            }
        } else {
            self.along = self.along.extended_by(self.depth).extended_by(by);
            self.depth = Dim::exact(0);
            // tex.web § 1087: only a box or rule first gives `\vtop` its
            // height.
            self.first.get_or_insert(Dim::exact(0));
        }
    }

    /// tex.web § 651, § 668: a box or rule of the given size.  A `None`
    /// length is running (§ 463 `null_flag`), which no maximum takes.
    fn boxed(&mut self, horizontal: bool, [w, h, d]: [Option<Dim>; 3], skips: Option<Interline>) {
        if horizontal {
            self.along = self.along.extended_by(w.unwrap_or(Dim::ANY));
            if let Some(h) = h {
                self.height = self.height.max(h);
            }
            if let Some(d) = d {
                self.depth = self.depth.max(d);
            }
            // tex.web § 1056, § 1060: a box or rule sets the space factor.
            self.space_factor = Some(1000);
            // A paragraph holding a box, rule or character makes a line.
            self.prior = false;
            self.para.fresh = false;
        } else {
            let h = h.unwrap_or(Dim::ANY);
            let glue = self.interline_glue(h, skips);
            self.along = self.along.extended_by(self.depth).extended_by(glue).extended_by(h);
            self.depth = d.unwrap_or(Dim::ANY);
            if let Some(w) = w {
                self.wide = self.wide.max(w);
            }
            self.first.get_or_insert(h);
            self.prior = true;
            self.prev_depth = Some(d.unwrap_or(Dim::ANY));
        }
    }

    /// A character, box or rule whose size satex does not know: every
    /// length it could have added widens, so the size is never
    /// under-approximated.
    fn widen(&mut self, horizontal: bool) {
        let prior = self.prior;
        self.boxed(horizontal, [Some(Dim::ANY); 3], None);
        // What was not followed may be nothing: a paragraph may still be
        // empty.
        if horizontal {
            self.prior = prior;
        }
    }

    /// A character on a horizontal list (tex.web § 651 `char_node`).  The
    /// space factor is the reader's, not a box's.
    fn character(&mut self, box_: [Dim; 3]) {
        let sf = self.space_factor;
        self.boxed(true, box_.map(Some), None);
        self.space_factor = sf;
        // tex.web § 894: a word after glue may be hyphenated anywhere.
        if self.para.glued {
            self.para.breakpoints = self.para.breakpoints.saturating_add(1);
        }
    }

    /// The list holds only boxes and rules: no glue, kern, penalty,
    /// character or breakpoint (tex.web § 831), so its natural width is
    /// what `\unhbox` of it adds, and it adds no breakpoint.
    fn rigid(&self) -> bool {
        let k = usize::from(self.solid);
        k < TAIL
            && self.tail[..k].iter().all(|l| matches!(l, Last::Other | Last::Box))
            && self.tail[k] == Last::Empty
            && self.pending.is_none()
            && self.para.breakpoints == 0
            && !self.para.glued
            && !self.para.awkward
    }

    /// The last item of a paragraph goes (tex.web §§ 816, 1105): the
    /// breakpoint it was is gone. Glue was one after a node that is not
    /// discardable (§ 868), a penalty when below 10000 (§ 859).
    fn drop_break(&mut self) {
        let was = match self.tail[0] {
            Last::Glue(..) => {
                !self.para.fresh
                    && !matches!(self.tail[1], Last::Glue(..) | Last::Penalty(_) | Last::Empty | Last::Unknown)
            }
            Last::Penalty(p) => p > -10000 && p < 10000,
            _ => false,
        };
        if was && self.para.breakpoints != u32::MAX {
            self.para.breakpoints = self.para.breakpoints.saturating_sub(1);
        }
    }

    fn join(self, other: Natural) -> Natural {
        let mut joined = Natural {
            along: self.along.join(other.along),
            height: self.height.join(other.height),
            depth: self.depth.join(other.depth),
            wide: self.wide.join(other.wide),
            first: join_first(self.first, other.first),
            prior: self.prior || other.prior,
            capped: self.capped || other.capped,
            tail: std::array::from_fn(|k| self.tail[k].merge(other.tail[k])),
            pending: self.pending,
            space_factor: if self.space_factor == other.space_factor { self.space_factor } else { None },
            ligature: if self.ligature == other.ligature { self.ligature } else { None },
            no_boundary: self.no_boundary || other.no_boundary,
            math_prev: if self.math_prev == other.math_prev { self.math_prev } else { Some(None) },
            math_class: if self.math_class == other.math_class && self.math_pending == other.math_pending {
                self.math_class
            } else {
                Some(-1)
            },
            math_pending: if self.math_pending == other.math_pending { self.math_pending } else { None },
            math_bin: if self.math_bin == other.math_bin { self.math_bin } else { Some(Scaled::MIN) },
            script: if self.script == other.script { self.script } else { None },
            math_box: if self.math_box == other.math_box { self.math_box } else { None },
            style: self.style.max(other.style),
            scripted: if self.scripted == other.scripted { self.scripted } else { None },
            math_breaks: self.math_breaks.max(other.math_breaks),
            solid: self.solid.max(other.solid),
            line: if self.line == other.line { self.line } else { None },
            para: self.para.join(other.para),
            prev_depth: join_prev(self.prev_depth, other.prev_depth),
            undo: if self.undo == other.undo { self.undo } else { None },
        };
        // A character not yet counted differs between the paths: its own
        // size is not known, so the lengths it would add widen.
        if self.pending != other.pending || self.style != other.style {
            joined.pending = None;
            joined.along = Dim::ANY;
            joined.height = joined.height.max(Dim::ANY);
            joined.depth = joined.depth.max(Dim::ANY);
            joined.wide = joined.wide.max(Dim::ANY);
        }
        joined
    }
}

/// Two lists' first heights: one that has none yet joins as zero.
fn join_first(a: Option<Dim>, b: Option<Dim>) -> Option<Dim> {
    match (a, b) {
        (None, None) => None,
        (a, b) => Some(a.unwrap_or(Dim::exact(0)).join(b.unwrap_or(Dim::exact(0)))),
    }
}

/// Two `\prevdepth`s: `ignore_depth` and a depth join as any length.
fn join_prev(a: Option<Dim>, b: Option<Dim>) -> Option<Dim> {
    match (a, b) {
        (None, None) => None,
        (Some(a), Some(b)) => Some(a.join(b)),
        _ => Some(Dim::ANY),
    }
}

/// How far back `\unskip` and relatives are followed.
pub const TAIL: usize = 4;

/// The last item of a list, as far as `\lastskip`, `\lastkern` and
/// `\lastpenalty` tell items apart.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Last {
    /// None, or one that is neither glue, a kern, a penalty nor a
    /// character.
    #[default]
    Other,
    /// A character or ligature node, by the font and character `\/` takes
    /// its italic correction from (tex.web § 1113); `None` where that is
    /// not known.
    Char(Option<(u32, u32)>),
    /// Glue by its natural width, and its whole specification where known.
    Glue(Scaled, Option<crate::value::Glue>),
    Kern(Scaled),
    Penalty(i64),
    /// Nothing at all: the list is empty.
    Empty,
    /// An hbox that is the list's only item and its whole size: the indent
    /// box of a paragraph (tex.web § 1091), or one appended to an empty
    /// list.
    Box,
    /// A whatsit node (tex.web § 1341), such as `\special`.
    Whatsit,
    Unknown,
}

impl Last {
    /// The tail where two paths, or an item that may not have been
    /// appended, leave different ones.
    pub(crate) fn merge(self, other: Last) -> Last {
        match (self, other) {
            _ if self == other => self,
            (Last::Other | Last::Char(_), Last::Other | Last::Char(_)) => Last::Char(None),
            _ => Last::Unknown,
        }
    }
}

/// A line of a paragraph as a box on a vertical list (tex.web § 889):
/// width, height, depth, and the left margin kern pdfTeX put first in it.
pub type Line = (Scaled, Scaled, Scaled, Option<Scaled>);

/// Which item `\unskip`, `\unkern` or `\unpenalty` takes away.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Removed {
    Glue,
    Kern,
    Penalty,
}

/// An item appended to the current list, by what it adds to its size.
pub(crate) enum Item {
    /// Nothing, or a whatsit, mark or insertion, which no list counts
    /// (tex.web §§ 655, 669).  Which of them is not told apart, so what a
    /// later `\unskip` takes stays as it was.
    Zero,
    /// A whatsit, which is surely the new tail (tex.web § 1341).
    Whatsit,
    Penalty(Option<i64>),
    Kern(Option<Scaled>),
    /// Glue by its natural width, and its specification where known.
    Glue(Option<Scaled>, Option<crate::value::Glue>),
    /// A rule: each of its three lengths, `None` where running (tex.web
    /// § 463), `Some(None)` where satex does not know it.
    Rule {
        width: Option<Option<Scaled>>,
        height: Option<Option<Scaled>>,
        depth: Option<Option<Scaled>>,
    },
    Box([Dim; 3]),
    Unknown,
}

/// Two paths' natural sizes: each length joined, so a size that differs
/// widens rather than being dropped.
pub fn join_natural(a: Option<Natural>, b: Option<Natural>) -> Option<Natural> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.join(b)),
        _ => None,
    }
}

/// tex.web § 463 `default_rule`: 0.4pt.
const DEFAULT_RULE: Scaled = 26214;

/// The `part` of the math group `\eqno` begins (tex.web § 1142).
const EQNO: u8 = 1;

/// tex.web § 1071 `box_context`: where the box goes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoxContext {
    /// Appended to the current list.
    Direct,
    /// `\raise` and relatives, by how much the box is shifted.
    Shift(Dim),
    /// `\setbox`: the register, if known, whether the assignment is
    /// global, and the range an unknown register number lies in, if known.
    Set { register: Option<i64>, global: bool, within: Option<[(i64, i64); 2]> },
    /// `\leaders`.
    Leaders,
    /// `\shipout`.
    Ship,
}

impl Machine<'_> {
    /// `\ifvmode`, `\ifhmode`, `\ifmmode`, `\ifinner` (tex.web § 501).
    pub(crate) fn mode_test(&self, cond: Cond) -> Option<bool> {
        let class = match cond {
            Cond::VMode => Modes::ANY_VERTICAL,
            Cond::HMode => Modes::ANY_HORIZONTAL,
            Cond::MathMode => Modes::ANY_MATH,
            _ => Modes::INNER,
        };
        self.mode.test(class)
    }

    /// For a mode test, the modes in which it is true and those in which
    /// it is false.
    pub(crate) fn mode_parts(&self, cond: Cond) -> Option<(Modes, Modes)> {
        let class = match cond {
            Cond::VMode => Modes::ANY_VERTICAL,
            Cond::HMode => Modes::ANY_HORIZONTAL,
            Cond::MathMode => Modes::ANY_MATH,
            Cond::InnerMode => Modes::INNER,
            _ => return None,
        };
        Some((self.mode.and(class), self.mode.without(class)))
    }

    /// A token list parameter such as `\everypar`: `None` when unknown.
    fn every(&mut self, name: &str) -> Option<Rc<[Token]>> {
        let sym = self.intern(name);
        match self.env.value(sym) {
            Value::Toks(tokens) => Some(tokens),
            _ => None,
        }
    }

    /// tex.web § 323 `begin_token_list` of `\everypar` and relatives.
    fn insert_every(&mut self, name: &str, span: Span) {
        match self.every(name) {
            Some(tokens) if !tokens.is_empty() => self.push_tokens(tokens),
            Some(_) => {}
            None => self.diagnose(
                Severity::Imprecision,
                "unknown-token-list",
                span,
                format!("\\{name} is unknown here, and what it holds was not run"),
            ),
        }
    }

    /// Run one path with the modes `part` and one with the rest, each
    /// reading `token` again: what `token` does depends on which it is.
    fn split_mode(&mut self, token: Token, part: Modes) -> bool {
        let rest = self.mode.without(part);
        if part.is_empty() || rest.is_empty() {
            return false;
        }
        let by = token.cs().or_else(|| self.active_cs(token)).unwrap_or_else(|| self.intern("ifvmode"));
        let parts = [part, rest];
        // Each path begins with `token` given back, which may join a frame
        // of tokens already given back: the paths must read it before they
        // can meet, or they meet at once in the modes they split.
        self.split(by, token.span, 2, move |m, arm| {
            let Some(&modes) = parts.get(arm) else { return false };
            m.mode = modes;
            m.unread(token);
            true
        })
    }

    /// Horizontal material in a vertical mode (tex.web § 1090: `back_input;
    /// new_graf`): the paragraph starts and `token` is read again.  `true`
    /// when that happened; `false` when `token` is to be obeyed as it is.
    pub(crate) fn horizontal_material(&mut self, token: Token) -> bool {
        self.material = true;
        // A paragraph the main vertical list gets the lines of.
        if self.mode.meets(Modes::VERTICAL) {
            self.main_tail = Last::Unknown;
        }
        self.align_cell(false);
        let vertical = self.mode.and(Modes::ANY_VERTICAL);
        if vertical.is_empty() {
            return false;
        }
        if vertical != self.mode
            && self.every("everypar").is_none_or(|t| !t.is_empty())
            && self.split_mode(token, vertical)
        {
            return true;
        }
        self.unread(token);
        self.new_graf(token.span, true);
        true
    }

    /// tex.web § 1091 `new_graf`.
    fn new_graf(&mut self, span: Span, indented: bool) {
        self.mode = self.mode.map(|m| if m.meets(Modes::ANY_VERTICAL) { Modes::HORIZONTAL } else { m });
        // A new horizontal list: its tail (`\lastskip`) is empty or the
        // indent box, neither glue, kern nor penalty. Line breaking undoes
        // its size, so only the tail and space factor are followed.  The
        // vertical list the lines go onto keeps its width, first height,
        // `\boxmaxdepth` mark and `\prevdepth` in the fields a horizontal
        // list leaves free, and its length in `para`, for `end_graf` to
        // take up again.
        let outer = self.natural.unwrap_or_else(Natural::any);
        // tex.web § 1091: `\parskip` goes onto a list that is not empty.
        let parskip = self.glue_parameter("parskip");
        let skip = parskip.map_or(Dim::ANY, |g| Dim::exact(g.width));
        let (skip, first, below) = match outer.tail[0] {
            Last::Empty => (Dim::exact(0), outer.first, Last::Empty),
            Last::Unknown if outer.first.is_none() => (Dim::exact(0).join(skip), Some(Dim::ANY), Last::Unknown),
            _ => (
                skip,
                outer.first.or(Some(Dim::exact(0))),
                parskip.map_or(Last::Unknown, |g| Last::Glue(g.width, Some(g))),
            ),
        };
        // tex.web § 1091: the indent box is `\parindent` wide.  The list's
        // width, height and depth are those of the one line it may make.
        let indent = if indented { self.dimen_parameter("parindent") } else { Dim::exact(0) };
        self.natural = Some(Natural {
            undo: None,
            along: indent,
            height: Dim::exact(0),
            depth: Dim::exact(0),
            wide: outer.wide,
            first,
            prior: !indented,
            capped: outer.capped,
            tail: [if indented { Last::Box } else { Last::Empty }; TAIL],
            pending: None,
            space_factor: Some(1000),
            ligature: None,
            no_boundary: false,
            solid: 0,
            line: None,
            math_prev: None,
            math_class: None,
            math_pending: None,
            math_bin: None,
            script: None,
            math_box: None,
            style: 0,
            scripted: None,
            math_breaks: 0,
            para: Para {
                along: outer.along.extended_by(outer.depth).extended_by(skip),
                below,
                items: Some(0),
                fit: 2,
                ..Para::default()
            },
            prev_depth: outer.prev_depth,
        });
        if indented {
            self.record_item(indent.known().map(|w| crate::linebreak::Kp::Box {
                w,
                h: 0,
                d: 0,
                chr: None,
                lig: false,
            }));
        }
        // tex.web § 1091.
        self.set_prevgraf(Value::Int(0));
        self.insert_every("everypar", span);
    }

    /// The one line a paragraph `before` set on the empty vertical list
    /// `after` makes it: its size, and the left margin kern pdfTeX's
    /// `post_line_break` puts first in it, from the character or ligature
    /// it starts with (none known where it starts otherwise).
    fn lone_line(&mut self, before: &Natural, n: &Natural, after: &mut Natural) {
        if before.prior || before.first.is_some() || n.para.below != Last::Empty {
            return;
        }
        let head = before.para.items.and_then(|i| self.kp_list(i).first().copied());
        let [width, reach] = self.line_shape();
        let (Some(w), Some(h), Some(d)) = (width.known(), n.first.and_then(|f| f.known()), n.para.depth.known()) else {
            return;
        };
        if reach != width || w < 0 || n.para.lost || n.para.lines != (1, 1) || n.para.along != Dim::exact(h) {
            return;
        }
        // pdfTeX's `char_pw` takes a ligature's own character (`lig_char`),
        // which is the one a ligature box here holds.
        let kern = match head {
            Some(crate::linebreak::Kp::Box { chr: Some((font, c)), .. }) => self.left_margin_kern(font, c),
            _ => None,
        };
        after.tail = [Last::Empty; TAIL];
        after.tail[0] = Last::Box;
        after.line = Some((w, h, d, kern));
    }

    /// pdfTeX's `char_pw` at the left: `\lpcode` thousandths of the
    /// font's quad, rounded, taken away at the start of a line where
    /// `\pdfprotrudechars` is positive.
    fn left_margin_kern(&mut self, font: u32, c: u32) -> Option<Scaled> {
        if self.int_parameter("pdfprotrudechars")? <= 0 {
            return Some(0);
        }
        let lp = self.lp_code(font, c)?;
        if lp == 0 {
            return Some(0);
        }
        let quad = self.font_dimen(font, 6)?.as_dimen()?;
        if lp < 0 || quad < 0 {
            return None;
        }
        // `round_xn_over_d`: a remainder of half or more rounds up.
        let (x, d) = (i128::from(quad) * i128::from(lp), 1000);
        let w = x / d + i128::from(2 * (x % d) >= d);
        Some(-Scaled::try_from(w).ok()?)
    }

    /// `\prevgraf` (tex.web § 1244): not grouped.
    fn set_prevgraf(&mut self, v: Value) {
        let sym = self.intern("prevgraf");
        self.env.set_value(sym, v, true);
    }

    /// tex.web §§ 847-848, 889: without `\parshape` or `\hangindent`, every
    /// line of a paragraph is packed to exactly `\hsize`.
    pub(crate) fn line_width(&mut self) -> Dim {
        self.line_shape()[0]
    }

    /// The width every line of a paragraph is packed to, and how far its
    /// box reaches right of the vertical list's reference point: its
    /// indent added (tex.web § 889 `shift_amount`). A `\parshape` whose
    /// lines are all alike (§ 848), as `\list` sets, gives both.
    fn line_shape(&mut self) -> [Dim; 2] {
        let parshape = self.intern("\u{4}parshape");
        match self.env.value(parshape) {
            Value::Int(0) => {}
            Value::Unknown if self.env.get(parshape).is_none() => {}
            _ => return self.uniform_par_shape().map_or([Dim::ANY; 2], |(i, l)| [Dim::exact(l), Dim::exact(i + l)]),
        }
        if self.dimen_parameter("hangindent") != Dim::exact(0) {
            return [Dim::ANY; 2];
        }
        let hsize = self.dimen_parameter("hsize");
        [hsize, hsize]
    }

    /// The indent and length of every line of `\parshape`, where all are
    /// known and alike.
    fn uniform_par_shape(&mut self) -> Option<(Scaled, Scaled)> {
        let lines = self.engine_slot_int("parshape", "parshape")?;
        if !(1..=64).contains(&lines) {
            return None;
        }
        let mut shape = None;
        for k in 1..=lines {
            let (i, l) = (self.engine_sym(&format!("parshape.i{k}")), self.engine_sym(&format!("parshape.l{k}")));
            let (i, l) = (self.env.value(i).as_dimen()?, self.env.value(l).as_dimen()?);
            if shape.is_some_and(|s| s != (i, l)) {
                return None;
            }
            shape = Some((i, l));
        }
        shape
    }

    /// `\baselineskip`, `\lineskip` and `\lineskiplimit` (tex.web § 679).
    fn interline(&mut self) -> Option<Interline> {
        let skips = Interline {
            baseline: self.glue_natural("baselineskip"),
            line: self.glue_natural("lineskip"),
            limit: self.dimen_parameter("lineskiplimit"),
        };
        (!skips.baseline.is_any() && !skips.line.is_any() && !skips.limit.is_any()).then_some(skips)
    }

    /// An integer parameter, zero where never set (tex.web § 240).
    pub(crate) fn int_parameter(&mut self, name: &str) -> Option<i64> {
        let sym = self.intern(name);
        match self.env.value(sym) {
            Value::Int(v) => Some(v),
            Value::Unknown if self.env.get(sym).is_none() => Some(0),
            _ => None,
        }
    }

    /// A glue parameter, `zero_glue` where never set.
    pub(crate) fn glue_parameter(&mut self, name: &str) -> Option<crate::value::Glue> {
        let sym = self.intern(name);
        match self.env.value(sym) {
            Value::Glue(g) => Some(g),
            Value::Unknown if self.env.get(sym).is_none() => Some(Default::default()),
            _ => None,
        }
    }

    /// The line the paragraph has set since its last forced break ends:
    /// at a penalty of -10000 or less (tex.web § 851), or at `\par`
    /// (`last`), where `line_break` drops a final glue and appends
    /// `\penalty10000\parfillskip` (§ 816).  Its lines go into `n.para`.
    ///
    /// tex.web §§ 851-859: where the line fits the line width at its
    /// natural width plus `\leftskip` and `\rightskip`, and infinite
    /// stretch makes its badness 0, it is one line of demerits
    /// `\linepenalty`², which no break in it beats: every line costs at
    /// least that much when penalties, `\adjdemerits` and the hyphen
    /// demerits are not negative, the line before is `decent_fit` (§ 817)
    /// and `\looseness` is 0.  Without a legal breakpoint it is one line
    /// however wide.  Otherwise it is one line up to one per breakpoint,
    /// each `hpack`ed (§ 889) between zero and the line's height and depth.
    fn set_line(&mut self, n: &mut Natural, last: bool) {
        use std::hash::{Hash, Hasher};
        let width = self.line_width();
        let left = self.glue_natural("leftskip");
        let right = self.glue_natural("rightskip");
        let skips = self.interline();
        let ints = [
            "looseness",
            "linepenalty",
            "adjdemerits",
            "doublehyphendemerits",
            "finalhyphendemerits",
            "hyphenpenalty",
            "exhyphenpenalty",
            "tolerance",
            "lastlinefit",
        ]
        .map(|name| self.int_parameter(name));
        let mut hasher = std::hash::DefaultHasher::new();
        (width, left, right, skips, ints).hash(&mut hasher);
        let shape = hasher.finish();
        let para = &mut n.para;
        if para.lines.1 > 0 && para.shape != shape {
            para.lost = true;
        }
        para.shape = shape;
        let Some(skips) = skips else {
            para.lost = true;
            return;
        };
        if let Some((lines, fit)) = self.break_lines(n, last) {
            for (h, d) in lines {
                n.stack_lines((1, 1), Dim::exact(h), Dim::exact(d), skips);
            }
            n.para.fit = fit;
            return;
        }
        let [Some(loose), Some(line), Some(adj), Some(double), Some(fin), Some(hyphen), Some(ex), Some(tol), Some(fit)] =
            ints
        else {
            return self.push_line(n, false, skips);
        };
        let stretchy = |g: Option<crate::value::Glue>| g.is_some_and(|g| g.stretch.order > 0 && g.stretch.amount > 0);
        let end = if last {
            self.glue_parameter("parfillskip")
        } else {
            match n.tail[0] {
                Last::Glue(_, g) => g,
                _ => None,
            }
        };
        let stretch = stretchy(end) || stretchy(self.glue_parameter("rightskip"));
        let mut along = n.along;
        if last && let Last::Glue(w, _) = n.tail[0] {
            along = along.reduced_by(Dim::exact(w));
            // § 816 turns it into `\penalty10000`.
            n.drop_break();
        }
        // Finite stretch: the line's badness is at most `b` (§ 108, from
        // the most it falls short and the least it stretches, its glue
        // stretching no less than nothing when not `awkward`). One line
        // costs (l+b)² (§ 859), and no `\adjdemerits` for b ≤ 99 (§ 817);
        // more lines cost at least 2l², so one line wins below that.
        let line_box = along.extended_by(left).extended_by(right);
        let least = [self.glue_parameter("leftskip"), self.glue_parameter("rightskip"), end]
            .into_iter()
            .try_fold(0, |s, g| g.map(|g| s + if g.stretch.order == 0 { g.stretch.amount } else { 0 }));
        let pre = self.int_parameter("pretolerance");
        let low = match (width.known(), least, pre) {
            (Some(w), Some(s), Some(pre)) if line_box.hi <= w => {
                let b = badness(w - line_box.lo, s);
                b <= 99 && (line + b).pow(2) < 2 * line * line && b <= tol && (pre < 0 || b <= pre)
            }
            _ => false,
        };
        let fits = width.known().is_some()
            && loose == 0
            && (1..10000).contains(&line)
            && [adj, double, fin, hyphen, ex, tol].iter().all(|&v| v >= 0)
            && fit <= 0
            && !n.para.awkward
            && (!n.para.rough || adj == 0)
            && (stretch || low)
            && line_box.hi <= width.lo;
        self.push_line(n, fits, skips);
    }

    /// The current line's lines go below the paragraph's: one where `one`
    /// or there is no breakpoint, else up to one per breakpoint.
    fn push_line(&mut self, n: &mut Natural, one: bool, skips: Interline) {
        let (h, d) = (n.height.at_least_zero(), n.depth.at_least_zero());
        if n.para.breakpoints == 0 || one {
            n.stack_lines((1, 1), h, d, skips);
            // A line with infinite stretch has badness 0 (tex.web § 852).
            n.para.fit = if one { 2 } else { crate::linebreak::UNFIT };
        } else {
            n.para.fit = crate::linebreak::UNFIT;
            let zero = Dim::exact(0);
            n.stack_lines((1, n.para.breakpoints.saturating_add(1)), zero.join(h), zero.join(d), skips);
            n.para.rough = true;
        }
    }

    /// A forced break in a paragraph (tex.web § 851): the line so far is
    /// set and a new one starts.
    fn forced_break(&mut self, n: &mut Natural) {
        self.set_line(n, false);
        n.along = Dim::exact(0);
        n.height = Dim::exact(0);
        n.depth = Dim::exact(0);
        let p = &mut n.para;
        (p.breakpoints, p.glued, p.awkward, p.fresh, p.items) = (0, false, false, true, Some(0));
    }

    /// The natural width of a glue parameter such as `\parskip`.
    fn glue_natural(&mut self, name: &str) -> Dim {
        let sym = self.intern(name);
        match self.env.value(sym) {
            Value::Glue(g) => Dim::exact(g.width),
            Value::Unknown if self.env.get(sym).is_none() => Dim::exact(0),
            _ => Dim::ANY,
        }
    }

    /// Vertical material in horizontal mode (tex.web § 1094
    /// `head_for_vmode`): a `\par` token goes in front of it.  `true` when
    /// `token` was put back.
    pub(crate) fn vertical_material(&mut self, token: Token) -> bool {
        let horizontal = self.mode.and(Modes::HORIZONTAL);
        if horizontal.is_empty() {
            self.par_inserted = None;
            return false;
        }
        let par = self.intern("par");
        // Back at the same command after its `\par`, still horizontal: TeX
        // would insert `\par` forever, so a run that compiles had it end the
        // paragraph. Only an approximated `\par` gets here.
        if self.par_inserted.replace(token.span) == Some(token.span) && horizontal == self.mode {
            self.diagnose(
                Severity::Assumption,
                "par-assumed-to-end-paragraph",
                token.span,
                "\\par was put before this command and the paragraph did not end; as in a run that compiles, it is assumed to have ended".into(),
            );
            self.par_inserted = None;
            self.end_graf();
            self.normal_paragraph(false);
            return false;
        }
        let primitive = self.env.meaning(par) == Meaning::Primitive(Primitive::Mode(ModeCmd::Par));
        if horizontal != self.mode {
            if primitive {
                // The paragraph ends where there is one, as `\par` would.
                self.end_graf();
                self.normal_paragraph(false);
                return false;
            }
            if self.split_mode(token, horizontal) {
                return true;
            }
        }
        self.unread(token);
        self.unread(Token::new(Tok::Cs(par), token.span));
        true
    }

    /// tex.web § 1096 `end_graf`: horizontal mode goes back to the vertical
    /// list the paragraph belongs to.
    pub(crate) fn end_graf(&mut self) {
        self.par_inserted = None;
        // tex.web § 1096: the error count starts over with each paragraph.
        self.tex_errors = 0;
        let list = self.list.and(Modes::ANY_VERTICAL);
        let to = if list.is_empty() { Modes::VERTICAL } else { list };
        let graf = self.mode.and(Modes::HORIZONTAL);
        let only = graf == self.mode;
        self.mode = self.mode.map(|m| if m == Modes::HORIZONTAL { to } else { m });
        // tex.web § 1094: a `\par` in vertical mode leaves the list alone.
        if graf.is_empty() {
            self.mark_hyphens(false);
            return;
        }
        let Some(mut n) = self.natural else {
            self.mark_hyphens(false);
            self.set_prevgraf(Value::Unknown);
            return;
        };
        // tex.web § 889: each line is a box of the line width, so the
        // vertical list is at least that wide.  A paragraph that may hold
        // nothing may add no line at all (§ 1096).
        let lined = n.wide.max(self.line_shape()[1]);
        // The last line's box, or a null paragraph's list as it was.
        let mut tail = [Last::Unknown; TAIL];
        tail[0] = match (n.tail[0], n.prior) {
            (Last::Empty, _) => n.para.below,
            (_, true) => Last::Other.merge(n.para.below),
            _ => Last::Other,
        };
        // tex.web § 1096: a null paragraph makes no line; else the last
        // line is set (§ 816) and the lines go onto the vertical list (§ 890).
        let before = n;
        if n.tail[0] != Last::Empty {
            self.set_line(&mut n, true);
        }
        // A hyphen is set only at a line's end (§ 902): one line, none.
        self.mark_hyphens(n.para.lines.1 <= 1 && !n.para.lost);
        if n.para.lost {
            (n.para.along, n.para.depth, n.prev_depth) = (Dim::ANY, Dim::ANY, Some(Dim::ANY));
            n.first = n.first.or(Some(Dim::ANY));
        }
        // tex.web § 890: `\prevgraf` counts the lines.
        let (lo, hi) = if n.para.lost {
            (0, crate::value::WORD_MAX)
        } else {
            (i64::from(n.para.lines.0), i64::from(n.para.lines.1))
        };
        let lo = if n.prior || !only { 0 } else { lo };
        self.set_prevgraf(if lo == hi {
            Value::Int(lo)
        } else {
            Value::from_num(crate::value::Quantity::Int, crate::value::Num::range(lo, hi))
        });
        // A paragraph that may hold nothing may add no line.
        if n.prior {
            n.para.along = n.para.along.join(before.para.along);
            n.para.depth = n.para.depth.join(before.para.depth);
            n.first = join_first(n.first, before.first);
            n.prev_depth = join_prev(n.prev_depth, before.prev_depth);
        }
        let after = Natural {
            along: n.para.along,
            height: Dim::ANY,
            depth: n.para.depth,
            // § 1096: a surely null paragraph adds nothing.
            wide: match (n.tail[0], n.prior) {
                (Last::Empty, _) => n.wide,
                (_, true) => n.wide.join(lined),
                _ => lined,
            },
            first: n.first,
            prior: true,
            capped: n.capped,
            tail,
            prev_depth: n.prev_depth,
            ..Natural::default()
        };
        // A paragraph of one character alone on an empty internal vertical
        // list makes it one line, packed to the line width with no indent.
        let mut after = after;
        if only && to == Modes::INTERNAL_VERTICAL {
            self.lone_line(&before, &n, &mut after);
        }
        // Where the mode was in doubt, the vertical list may be as it was.
        self.natural = Some(if only { after } else { after.join(n) });
    }

    /// tex.web § 1070 `normal_paragraph`: the paragraph shape parameters go
    /// back to their defaults.  Not `certain` when only some paths reset
    /// them.
    pub(crate) fn normal_paragraph(&mut self, certain: bool) {
        let engine = |m: &mut Self, key: &str| m.intern(&format!("\u{4}{key}"));
        let mut resets = Vec::new();
        for (name, default) in
            [("looseness", Value::Int(0)), ("hangindent", Value::Dimen(0)), ("hangafter", Value::Int(1))]
        {
            resets.push((self.intern(name), default));
        }
        for key in ["parshape", "interlinepenalties.count"] {
            resets.push((engine(self, key), Value::Int(0)));
        }
        for (sym, default) in resets {
            let value = self.env.value(sym);
            if value != default {
                let value = if certain { default } else { value.join(&default, crate::env::SetLimits::default()) };
                self.env.set_value(sym, value, false);
            }
        }
    }

    /// `\par` (tex.web § 1094).
    fn par(&mut self, span: Span) {
        // A `\par` in math mode is an error that inserts the `$` it lacks
        // (tex.web § 1047 `insert_dollar_sign`).
        if self.mode.within(Modes::ANY_MATH)
            && !self.mode.is_empty()
            && matches!(self.env.group_kind(), Some(GroupKind::Math { .. }))
        {
            self.diagnose(Severity::Warning, "missing-dollar", span, "missing $ inserted before \\par".into());
            let par = self.intern("par");
            self.unread(Token::new(Tok::Cs(par), span));
            self.unread(Token::new(Tok::Chr('$', Catcode::Math), span));
            return;
        }
        let acting = self.mode.and(Modes::ANY_VERTICAL.union(Modes::HORIZONTAL));
        if acting.is_empty() {
            return;
        }
        crate::observe::close_overhang(self);
        self.end_graf();
        self.normal_paragraph(acting == self.mode);
    }

    /// An unknown command may have started or ended a paragraph.
    pub(crate) fn widen_mode(&mut self) {
        self.material = true;
        self.natural = None;
        let mut mode = self.mode;
        if mode.meets(Modes::ANY_VERTICAL) {
            mode = mode.union(Modes::HORIZONTAL);
        }
        if mode.meets(Modes::HORIZONTAL) {
            let list = self.list.and(Modes::ANY_VERTICAL);
            mode = mode.union(if list.is_empty() { Modes::VERTICAL } else { list });
        }
        self.mode = mode;
    }

    /// The `{` a box, an insertion or a math part begins with (tex.web
    /// § 403 `scan_left_brace`).  TeX inserts a missing one.
    fn box_left_brace(&mut self, span: Span) -> Span {
        loop {
            let Some(token) = self.next_x_token() else { return span };
            match token.tok {
                Tok::Chr(_, Catcode::Space) => {}
                Tok::Chr(_, Catcode::Begin) => return token.span,
                Tok::Cs(sym) => match self.env.meaning(sym) {
                    Meaning::Primitive(Primitive::Relax) => {}
                    Meaning::Char(_, Catcode::Space) => {}
                    Meaning::Char(_, Catcode::Begin) => return token.span,
                    _ => {
                        self.unread(token);
                        break;
                    }
                },
                _ => {
                    self.unread(token);
                    break;
                }
            }
        }
        self.diagnose(Severity::Warning, "missing-left-brace", span, "missing { inserted".into());
        span
    }

    /// Open the group of a list of its own, in `modes`.
    fn open_list(&mut self, code: Option<u8>, modes: Modes, part: u8, span: Span) {
        crate::observe::box_break(self);
        let at = self.box_left_brace(span);
        self.open_group(GroupKind::Simple, at);
        self.record(Step::OpenGroup, Sym(0), at);
        self.env.set_nest(self.nest_here(code, true, part));
        self.material = false;
        // tex.web § 1083: a box's horizontal list starts at 1000.
        self.natural = Some(Natural { space_factor: Some(1000), tail: [Last::Empty; TAIL], ..Natural::default() });
        self.mode = modes;
        self.list = modes;
        // tex.web § 1083: a box's vertical list starts with `\prevdepth` at
        // `ignore_depth`; the group gives the enclosing list's back at `}`.
        if modes == Modes::INTERNAL_VERTICAL {
            let sym = self.intern("prevdepth");
            self.env.set_value(sym, Value::Dimen(crate::builtins::IGNORE_DEPTH), false);
        }
    }

    /// What a group opened here saves of the nest: the modes, the list and
    /// its natural size, nothing to shift, pack or fill.
    fn nest_here(&self, code: Option<u8>, level: bool, part: u8) -> Nest {
        Nest {
            mode: self.mode,
            list: self.list,
            code,
            level,
            part,
            target: None,
            shift: Dim::exact(0),
            to: Packing::Natural,
            material: self.material,
            natural: self.natural,
            placed: false,
            from: self.out.facts.text.at.len() as u32,
            align: None,
        }
    }

    /// A `{` of a simple group: in math it begins a math list (tex.web
    /// § 1153 `scan_math`, `push_math(math_group)`).
    pub(crate) fn simple_group_opened(&mut self) {
        let math = self.mode.and(Modes::ANY_MATH);
        if math.is_empty() {
            return;
        }
        let code = if math == self.mode { Some(group::MATH) } else { None };
        // tex.web §§ 1153, 1186: a math group is one atom, ordinary or of
        // the type `\mathord` … `\mathinner` gave it, whose list starts
        // afresh. Its space before it goes on now.
        let mut part = 0;
        let script = self.natural.and_then(|n| n.script).filter(|_| code.is_some());
        if script.is_some() {
            // A `{…}` script is a list of its own, in the style a script
            // gets (§ 1176); the nucleus waits in the group's saved list.
            part = SCRIPT_GROUP;
        } else if code.is_some()
            && let Some(mut n) = self.natural
        {
            n.math_box = None;
            part = self.next_atom(&mut n);
            self.natural = Some(n);
        }
        self.env.set_nest(self.nest_here(code, false, part));
        if code.is_some()
            && let Some(n) = &mut self.natural
        {
            (n.math_class, n.math_bin, n.math_prev, n.math_pending) = (None, None, None, None);
            n.scripted = None;
            if let Some(sup) = script {
                (n.style, n.script, n.math_box) = (Self::script_style(n.style, sup), None, None);
                (n.along, n.height, n.depth) = (Dim::exact(0), Dim::exact(0), Dim::exact(0));
            }
        }
        self.mode = self.mode.map(|m| if m.meets(Modes::ANY_MATH) { Modes::MATH } else { m });
    }

    /// A group ended: the modes it saved come back (tex.web § 1085
    /// `package`, § 1186, § 1100), and a `\discretionary` or `\mathchoice`
    /// goes on to its next part (tex.web §§ 1119, 1174).
    pub(crate) fn group_closed(&mut self, nest: Option<Nest>, span: Span) {
        let Some(mut nest) = nest else { return };
        // tex.web § 1132: a `}` ending a row stands for a missing `\cr`.
        if nest.code == Some(group::ALIGN) && nest.align.is_some() {
            if self.mode != ALIGN_MODES {
                self.end_row(&mut nest);
                // § 1132: a row the `}` ended had no `v` part.
                if let Some(a) = &mut nest.align {
                    a.exact = None;
                }
            }
            let width = Self::align_width(&nest);
            if let (Some(n), Some(w)) = (&mut nest.natural, width) {
                n.wide = n.wide.max(w);
            }
        }
        if !nest.level {
            self.mode = self.mode.without(Modes::ANY_MATH).union(nest.mode.and(Modes::ANY_MATH));
            // The group's list, boxed (tex.web §§ 720, 755): its last
            // character's italic correction, then the atom it is.
            if nest.code == Some(group::MATH) && nest.part == SCRIPT_GROUP {
                let outer = nest.natural.unwrap_or_else(Natural::any);
                self.close_script_group(outer);
            } else if nest.code == Some(group::MATH)
                && let Some(mut n) = self.natural
            {
                let italic = self.prev_italic(n.math_prev);
                Self::end_atoms(&mut n);
                n.along = n.along.extended_by(italic);
                let outer = nest.natural.unwrap_or_else(Natural::any);
                (n.math_class, n.math_bin, n.math_prev) = (outer.math_class, outer.math_bin, None);
                n.scripted = None;
                // tex.web §§ 1153, 1186: the group's noad is the tail again.
                n.pushed(Last::Other);
                self.natural = (n.script.is_none() && n.math_pending.is_none()).then_some(n);
            }
            return;
        }
        // tex.web § 1085: a `\vbox` ends the paragraph still open in it.
        if self.mode.meets(Modes::HORIZONTAL) {
            self.end_graf();
        }
        self.mode = nest.mode;
        self.list = nest.list;
        self.material = nest.material;
        self.end_word();
        let natural = std::mem::replace(&mut self.natural, nest.natural);
        let kind = match nest.code {
            Some(group::HBOX | group::ADJUSTED_HBOX) => BoxState::HBox,
            Some(group::VBOX | group::VTOP | group::VCENTER) => BoxState::VBox,
            _ => BoxState::Unknown,
        };
        // tex.web §§ 649, 668: the box's size is its list's natural size,
        // or its `to` size in its own direction.
        let n = natural.unwrap_or_else(Natural::any);
        let dims = match kind {
            BoxState::Unknown => [Dim::ANY; 3],
            // tex.web § 649: `hpack` starts height and depth at zero.
            BoxState::HBox => [nest.to.size(n.along), n.height.max(Dim::exact(0)), n.depth.max(Dim::exact(0))],
            _ => {
                // tex.web § 668: `vpackage` moves the depth past
                // `\boxmaxdepth` into the height.
                let limit = if n.capped { Dim::ANY } else { self.dimen_parameter("boxmaxdepth") };
                let (along, depth) = if n.depth.hi <= limit.lo {
                    (n.along, n.depth)
                } else if n.depth.lo >= limit.hi {
                    (n.along.extended_by(n.depth).reduced_by(limit), limit)
                } else {
                    (n.along.extended_by(n.depth).reduced_by(limit).join(n.along), n.depth.join(limit))
                };
                let n = Natural { along, depth, undo: None, ..n };
                let height = nest.to.size(n.along);
                // tex.web § 1087: a `\vtop` takes its height from its first
                // box, and the rest of the packed box is its depth.
                if nest.code == Some(group::VTOP) {
                    let first = n.first.unwrap_or(Dim::exact(0));
                    [n.wide, first, height.extended_by(n.depth).reduced_by(first)]
                } else if nest.code == Some(group::VCENTER) {
                    // tex.web § 736: centred on the axis of the text size,
                    // as `math_char` reads its fonts.
                    let by = self.intern("textfont");
                    let axis = self.family_font(by, 2).and_then(|f| self.font_dimen(f, 22)?.as_dimen());
                    let total = height.extended_by(n.depth);
                    let h = match (axis, total.known()) {
                        // `half` (§ 100) rounds an odd total up.
                        (Some(a), Some(t)) => Dim::exact(a + if t % 2 != 0 { (t + 1) / 2 } else { t / 2 }),
                        _ => Dim::ANY,
                    };
                    [n.wide.at_least_zero(), h, total.reduced_by(h)]
                } else {
                    // tex.web § 668: `vpack` starts the width at zero.
                    [n.wide.at_least_zero(), height, n.depth]
                }
            }
        };
        let now = self.out.facts.text.at.len() as u32;
        let inside = nest.list.within(Modes::INTERNAL_VERTICAL.union(Modes::RESTRICTED_HORIZONTAL));
        if let Some((register, global, within)) = nest.target {
            let mut ranges = std::mem::take(&mut self.carried_chars);
            ranges.push((nest.from, now));
            self.set_box_chars(register, ranges);
            self.set_box(register, kind, global, within);
            if kind != BoxState::Unknown {
                self.set_box_dims(register, dims);
            }
            // A naturally packed vbox of one line or nothing: what
            // `\vsplit` and `\unvbox` find in it.
            if nest.code == Some(group::VBOX) && nest.to == Packing::Natural && !n.capped {
                match (n.tail[0], n.line) {
                    (Last::Empty, _) => self.set_box_line(register, None),
                    (Last::Box, Some(line)) => self.set_box_line(register, Some(line)),
                    _ => {}
                }
            }
            // What `\unvbox` of it starts with (§ 1110): its list's first
            // height, when the list is as it was packed. `\unhbox` adds the
            // sum along its list.
            if kind == BoxState::VBox && nest.to == Packing::Natural && !n.capped {
                self.set_box_first(register, n.first.unwrap_or(Dim::exact(0)));
            } else if kind == BoxState::HBox {
                self.set_box_first(register, n.along);
                self.set_box_last(register, n.tail[0]);
            }
            // An hbox of boxes and rules alone: its natural width (§ 649).
            if kind == BoxState::HBox && nest.to == Packing::Natural && n.rigid() {
                self.set_box_first(register, n.along);
                self.set_box_slot(register, "solid", Value::Int(i64::from(n.solid)));
            }
            // What `\unskip` after `\unhbox` of it sees (§ 1105).
            if kind == BoxState::HBox {
                self.set_box_last(register, n.tail[0]);
            }
        } else if nest.placed {
            self.place_chars(vec![(nest.from, now)], inside);
            let dims = self.shifted(dims, nest.shift);
            self.append_item(Item::Box(dims));
            // § 1076: `\vadjust` material in it follows it on the list.
            if nest.code == Some(group::ADJUSTED_HBOX) && n.para.lost && self.list_kind() != Some(true) {
                self.append_item(Item::Unknown);
            }
        } else if nest.code == Some(group::INSERT) {
            self.append_item(Item::Zero);
            // tex.web § 889: `\vadjust` material follows the line, as wide
            // as it is (§ 668).
            if nest.part == 1
                && let Some(n) = &mut self.natural
            {
                n.wide = n.wide.max(natural.map_or(Dim::ANY, |m| m.wide.at_least_zero()));
            }
        } else if nest.code == Some(group::MATH_SHIFT) {
            self.close_math(natural);
        } else if nest.code == Some(group::DISC) {
            // tex.web §§ 651, 1119-1120: `hpack` skips the break texts, and
            // the text without a break follows the node as part of the list.
            // Its items are characters, boxes, rules and kerns (§ 1121).
            if nest.part == 3 {
                match (natural, self.natural) {
                    (Some(inner), Some(outer)) => {
                        let dims = [inner.along, inner.height.max(Dim::exact(0)), inner.depth.max(Dim::exact(0))];
                        self.append_item(Item::Box(dims));
                        if let Some(n) = &mut self.natural {
                            (n.space_factor, n.solid) = (outer.space_factor, outer.solid);
                        }
                    }
                    _ => self.natural = None,
                }
            }
        } else if nest.code == Some(group::MATH_CHOICE) {
            self.choice_closed(nest.part, natural);
            // tex.web § 1174: the choice node is the tail of the list
            // around it.
            if nest.part & 15 == 4
                && let Some(n) = &mut self.natural
            {
                n.pushed(Last::Other);
            }
        } else if nest.align.is_some() {
            // The rows, or the `\noalign` material, went onto the
            // alignment's vertical list.
            if nest.code == Some(group::NO_ALIGN)
                && let Some(mut rows) = self.halign()
            {
                rows.natural = natural;
                self.env.set_nest(rows);
                self.template_row_start();
            }
        } else if kind == BoxState::Unknown {
            self.natural = None;
        }
        match nest.code {
            Some(group::DISC) if nest.part < 3 => {
                self.open_list(Some(group::DISC), Modes::RESTRICTED_HORIZONTAL, nest.part + 1, span);
            }
            Some(group::MATH_CHOICE) if nest.part & 15 < 4 => self.open_choice(nest.part + 1, span),
            _ => {}
        }
    }

    /// The `k`-th list of a `\mathchoice` (low four bits of `part`; the
    /// high ones the list the style picks, or 0). The picked one goes on
    /// from the list around it: it takes the choice's place (tex.web § 731).
    fn open_choice(&mut self, part: u8, span: Span) {
        let outer = self.natural;
        self.open_list(Some(group::MATH_CHOICE), Modes::MATH, part, span);
        if part >> 4 == part & 15
            && let Some(mut n) = outer
        {
            // Its list starts empty (tex.web § 1174).
            n.tail = [Last::Empty; TAIL];
            self.natural = Some(n);
        }
    }

    /// A `\mathchoice` list ended: the picked one is the list around it
    /// from here on; the others are dropped.
    fn choice_closed(&mut self, part: u8, inner: Option<Natural>) {
        match part >> 4 {
            0 => self.natural = None,
            chosen if chosen == part & 15 => self.natural = inner,
            _ => {}
        }
    }

    /// A math shift character (tex.web §§ 1137-1138, 1193-1194).
    pub(crate) fn math_shift_token(&mut self, token: Token) {
        let span = token.span;
        if let Some(GroupKind::Math { display }) = self.env.group_kind() {
            // tex.web § 1194: the `$` that ends an `\eqno` ends the display
            // too, and needs its second `$`.
            let eqno = self.env.nest().is_some_and(|n| n.part == EQNO);
            if display || eqno {
                self.take_math_shift();
            }
            self.close_group(GroupKind::Math { display }, token);
            if eqno && let Some(GroupKind::Math { display: true }) = self.env.group_kind() {
                self.close_group(GroupKind::Math { display: true }, token);
            }
            return;
        }
        // tex.web § 1193: a `$` that meets another group in math is `off_save`.
        if self.mode.within(Modes::ANY_MATH) {
            return self.close_group(GroupKind::Math { display: self.mode == Modes::DISPLAY_MATH }, token);
        }
        if self.horizontal_material(token) {
            return;
        }
        self.material = true;
        self.end_word();
        // tex.web § 1138: `$$` is display math only in unrestricted
        // horizontal mode.
        let display = self.mode.meets(Modes::HORIZONTAL) && self.take_math_shift();
        // Inline math keeps the list around it, for its size to be added.
        let outer = if display || self.list_kind() != Some(true) { None } else { self.natural };
        self.natural = None;
        self.open_group(GroupKind::Math { display }, span);
        self.env.set_nest(Nest { natural: outer, from: 0, ..self.nest_here(Some(group::MATH_SHIFT), true, 0) });
        let inner = if display { Modes::DISPLAY_MATH } else { Modes::MATH };
        self.mode = inner;
        self.list = inner;
        // tex.web § 1139 `push_math`: `\fam` is -1 in the new math list.
        let fam = self.intern("fam");
        self.env.set_value(fam, Value::Int(-1), false);
        // The math list's size, while it holds only plain characters. It
        // starts empty (tex.web § 1139 `push_math`).
        self.natural = outer.map(|_| Natural { tail: [Last::Empty; TAIL], ..Natural::default() });
        if display {
            // tex.web § 1145: the display's size and place come from the
            // paragraph before it, which satex does not break into lines.
            for name in ["predisplaysize", "displaywidth", "displayindent"] {
                let sym = self.intern(name);
                self.env.set_value(sym, Value::Unknown, false);
            }
        }
        self.insert_every(if display { "everydisplay" } else { "everymath" }, span);
    }

    /// A command the mode governs (tex.web § 1045).
    pub(crate) fn mode_command(&mut self, cmd: ModeCmd, by: Sym, span: Span) {
        let token = Token::new(Tok::Cs(by), span);
        match cmd {
            ModeCmd::Par => self.par(span),
            ModeCmd::StartPar { .. } | ModeCmd::QuitVMode => {
                // tex.web § 1093: `\indent` in horizontal mode appends an
                // empty box `\parindent` wide.
                if matches!(cmd, ModeCmd::StartPar { indent: true }) && self.mode.meets(Modes::ANY_HORIZONTAL) {
                    let indent = self.dimen_parameter("parindent");
                    self.append_item(Item::Box([indent, Dim::exact(0), Dim::exact(0)]));
                }
                let vertical = self.mode.and(Modes::ANY_VERTICAL);
                if vertical.is_empty() {
                    return;
                }
                if vertical != self.mode
                    && self.every("everypar").is_none_or(|t| !t.is_empty())
                    && self.split_mode(token, vertical)
                {
                    return;
                }
                self.new_graf(span, matches!(cmd, ModeCmd::StartPar { indent: true }));
            }
            ModeCmd::Horizontal(material) => {
                if !self.horizontal_material(token) {
                    self.scan_material(material, span);
                }
            }
            ModeCmd::Vertical(Material::Align) => {
                if !self.vertical_material(token) {
                    self.begin_halign(span);
                }
            }
            ModeCmd::Vertical(material) => {
                if !self.vertical_material(token) {
                    self.scan_material(material, span);
                }
            }
            ModeCmd::Box(kind) => self.begin_box(kind, BoxContext::Direct, span),
            ModeCmd::Shift { .. } => {
                // tex.web § 1073: `\raise` and `\moveleft` shift the other
                // way round from `\lower` and `\moveright`.
                let back = {
                    let name = self.name(self.env.identity(by));
                    name == "raise" || name == "moveleft"
                };
                let amount = self.scan_dimen().map_or(Dim::ANY, Dim::exact);
                let shift = if back { Dim::exact(0).reduced_by(amount) } else { amount };
                self.scan_box(BoxContext::Shift(shift), span);
            }
            ModeCmd::Leaders => self.scan_box(BoxContext::Leaders, span),
            ModeCmd::ShipOut => self.scan_box(BoxContext::Ship, span),
            // tex.web § 1099 `begin_insert_or_adjust`.
            ModeCmd::Insert { vadjust } => {
                if !vadjust {
                    self.scan_number();
                }
                // tex.web § 889: `\vadjust` material of any width follows
                // the line.
                if vadjust && let Some(n) = &mut self.natural {
                    // tex.web § 889: it goes onto the vertical list after its
                    // line, which grows by what satex does not follow.
                    // tex.web §§ 796, 1076: from a box appended to a
                    // vertical list, or an alignment's cell, it moves out the
                    // same way; the mark makes that list's size unknown.
                    n.para.lost = true;
                }
                if vadjust && self.mode.meets(Modes::RESTRICTED_HORIZONTAL) {
                    self.template_lost();
                }
                self.open_list(Some(group::INSERT), Modes::INTERNAL_VERTICAL, u8::from(vadjust), span);
                self.normal_paragraph(true);
            }
            // tex.web § 785: the material goes into the alignment's own
            // list, which satex does not tell from its cells.
            ModeCmd::NoAlign if self.between_rows() => {
                // tex.web § 785: no list of its own, the material goes
                // between the rows.
                let rows = self.halign().and_then(|n| n.natural);
                self.open_list(Some(group::NO_ALIGN), Modes::INTERNAL_VERTICAL, 0, span);
                let mut nest = self.env.nest().expect("the group is open");
                nest.align = Some(Align::default());
                self.env.set_nest(nest);
                self.natural = rows;
                let sym = self.intern("prevdepth");
                let depth = match rows.map(|n| n.prev_depth) {
                    Some(None) => Value::Dimen(crate::builtins::IGNORE_DEPTH),
                    Some(Some(d)) => d.known().map_or(Value::Unknown, Value::Dimen),
                    None => Value::Unknown,
                };
                self.env.set_value(sym, depth, false);
                self.normal_paragraph(false);
            }
            ModeCmd::NoAlign => {
                let modes = Modes::INTERNAL_VERTICAL.union(Modes::RESTRICTED_HORIZONTAL);
                self.open_list(Some(group::NO_ALIGN), modes, 0, span);
                self.normal_paragraph(false);
            }
            // tex.web § 1172 `append_choices`.
            ModeCmd::MathChoice => {
                if self.mode.meets(Modes::ANY_MATH) {
                    // In inline math the style here picks the list (tex.web
                    // § 731): the text, script or scriptscript one.
                    let chosen = match (self.mode, self.natural) {
                        (Modes::MATH, Some(n)) if n.script.is_none() && n.math_pending.is_none() => 2 + (n.style >> 1),
                        _ => 0,
                    };
                    self.open_choice(1 | chosen << 4, span);
                }
            }
            // tex.web § 1171: a style node; the atoms after it are set in
            // that style (§ 763), and the character before it keeps its
            // italic correction (§ 755).
            ModeCmd::MathStyle(code) => {
                if !self.mode.meets(Modes::ANY_MATH) {
                    return;
                }
                match self.natural {
                    Some(mut n)
                        if self.mode == Modes::MATH && code > 0 && n.script.is_none() && n.math_pending.is_none() =>
                    {
                        let italic = self.prev_italic(n.math_prev);
                        n.along = n.along.extended_by(italic);
                        (n.math_prev, n.math_box, n.scripted) = (None, None, None);
                        n.style = code - 2;
                        n.pushed(Last::Other);
                        self.natural = Some(n);
                    }
                    _ => self.natural = None,
                }
            }
            // tex.web § 1142 `start_eq_no`: only in display math.
            ModeCmd::EqNo => {
                if self.mode == Modes::DISPLAY_MATH
                    && matches!(self.env.group_kind(), Some(GroupKind::Math { display: true }))
                {
                    self.open_group(GroupKind::Math { display: false }, span);
                    self.env.set_nest(Nest {
                        natural: None,
                        from: 0,
                        ..self.nest_here(Some(group::MATH_SHIFT), true, EQNO)
                    });
                    self.record_item(None);
                    self.mode = Modes::MATH;
                    self.list = Modes::MATH;
                    let fam = self.intern("fam");
                    self.env.set_value(fam, Value::Int(-1), false);
                    self.insert_every("everymath", span);
                }
            }
            // tex.web § 1191: `\left` begins an inner math list that
            // `\right` ends. satex keeps no group for it, so the display
            // may be inner from here on.
            ModeCmd::Left => {
                self.skip_spaces();
                self.next_token();
                if self.mode.meets(Modes::DISPLAY_MATH) {
                    self.mode = self.mode.union(Modes::MATH);
                }
            }
        }
    }

    /// What a horizontal or vertical command scans once the mode is right.
    fn scan_material(&mut self, material: Material, span: Span) {
        match material {
            Material::Char => match self.scan_number() {
                Some(c) if c >= 0 => {
                    if let Some(ch) = u32::try_from(c).ok().and_then(char::from_u32) {
                        self.note_text(ch, span);
                    }
                    self.append_char(c as u32)
                }
                _ => self.append_item(Item::Unknown),
            },
            // tex.web § 1110: `\unhbox` and `\unvbox` empty the register,
            // the `copy` forms do not.
            Material::Unpackage { take } => {
                let n = self.scan_number();
                let dims = self.box_dims(n);
                let first = self.box_first(n);
                let solid = n.map(|n| self.intern(&format!("\u{4}box.solid.{n}")));
                let solid = match solid.map(|k| self.env.value(k)) {
                    Some(Value::Int(k)) => u8::try_from(k).ok(),
                    _ => None,
                };
                let last = self.box_last(n);
                let line = self.box_line(n);
                let state = self.fetch_box(n, take);
                // A void register adds nothing.
                let ranges = std::mem::take(&mut self.fetched);
                if state != BoxState::Void {
                    self.place_chars(
                        ranges,
                        self.mode.within(Modes::INTERNAL_VERTICAL.union(Modes::RESTRICTED_HORIZONTAL)),
                    );
                }
                let empty = self.list_kind() == Some(false)
                    && self
                        .natural
                        .is_some_and(|n| n.tail[0] == Last::Empty && n.along == Dim::exact(0) && n.first.is_none());
                if state != BoxState::Void && self.mode.meets(Modes::ANY_MATH) {
                    self.natural = None;
                } else if let (BoxState::VBox, true, Some(line)) = (state, empty, line)
                    && dims.iter().all(|d| d.known().is_some())
                    && dims[0] == Dim::exact(line.map_or(0, |(w, ..)| w.max(0)))
                {
                    // tex.web § 1110: the line goes onto the empty list as it
                    // is, with no interline glue; nothing, for an empty box.
                    if let (Some((w, h, d, _)), Some(n)) = (line, &mut self.natural) {
                        n.along = Dim::exact(h);
                        n.depth = Dim::exact(d);
                        n.wide = n.wide.max(Dim::exact(w));
                        n.first = Some(Dim::exact(h));
                        n.tail = [Last::Empty; TAIL];
                        n.tail[0] = Last::Box;
                        n.line = line;
                        self.material = true;
                    }
                } else if state != BoxState::Void {
                    self.unpackage(state, dims, first, solid);
                    let horizontal = self.list_kind() == Some(true);
                    // § 1110: an empty list leaves `tail` where it was.
                    if state == BoxState::HBox
                        && horizontal
                        && solid != Some(0)
                        && let Some(n) = &mut self.natural
                    {
                        n.tail[0] = last;
                    }
                }
            }
            // tex.web § 463 `scan_rule_spec`: a `\vrule` is 0.4pt wide unless
            // said otherwise, its height and depth running. An `\hrule` is
            // 0.4pt high and nothing deep, its width running.
            Material::Rule => {
                let [width, height, depth] = self.scan_rule_spec();
                let item = match self.list_kind() {
                    Some(true) => Item::Rule { width: Some(width.unwrap_or(Some(DEFAULT_RULE))), height, depth },
                    Some(false) => Item::Rule {
                        width,
                        height: Some(height.unwrap_or(Some(DEFAULT_RULE))),
                        depth: Some(depth.unwrap_or(Some(0))),
                    },
                    None => Item::Unknown,
                };
                self.append_item(item);
            }
            Material::Glue => {
                let glue = self.scan_glue(false);
                self.append_item(Item::Glue(glue.map(|g| g.width), glue));
            }
            // tex.web § 1060: `fil`, `fill`, `ss` and `fil_neg` glue.
            Material::Fil(code) => {
                use crate::value::{Glue, Stretch};
                let unit = |amount: Scaled, order| Stretch { amount: amount * 65536, order };
                let (stretch, shrink) = match code {
                    0 => (unit(1, 1), unit(0, 0)),
                    1 => (unit(1, 2), unit(0, 0)),
                    2 => (unit(1, 1), unit(1, 1)),
                    _ => (unit(-1, 1), unit(0, 0)),
                };
                self.append_item(Item::Glue(Some(0), Some(Glue { width: 0, stretch, shrink })))
            }
            Material::NoBoundary => {
                if let Some(n) = &mut self.natural {
                    n.no_boundary = true;
                }
                self.end_word();
            }
            Material::Discretionary => {
                // tex.web § 831: a discretionary is a legal breakpoint, and the last node.
                if let Some(n) = &mut self.natural {
                    n.para.breakpoints = n.para.breakpoints.saturating_add(1);
                    n.para.items = None;
                    n.pushed(Last::Other);
                }
                self.open_list(Some(group::DISC), Modes::RESTRICTED_HORIZONTAL, 1, span);
            }
            // tex.web § 774 `init_align`: the preamble and the rows are one
            // group. satex runs the cells in the alignment's list.
            Material::Align => {
                if self.scan_keyword("to") || self.scan_keyword("spread") {
                    self.scan_dimen();
                }
                let modes = Modes::INTERNAL_VERTICAL.union(Modes::RESTRICTED_HORIZONTAL);
                self.open_list(Some(group::ALIGN), modes, 0, span);
            }
            // A control space sets interword glue, a space on the page.
            Material::Space => {
                self.note_text(' ', span);
                let (width, spec) = self.space_glue(true);
                self.append_item(Item::Glue(width, spec));
            }
            Material::Discards | Material::Hyphen => self.append_item(Item::Unknown),
        }
    }

    /// tex.web § 645 `scan_spec`: `to` or `spread` and a dimension, or neither.
    fn scan_spec(&mut self) -> Packing {
        let dimen = |m: &mut Self| m.scan_dimen().map_or(Dim::ANY, Dim::exact);
        if self.scan_keyword("to") {
            Packing::To(dimen(self))
        } else if self.scan_keyword("spread") {
            Packing::Spread(dimen(self))
        } else {
            Packing::Natural
        }
    }

    /// tex.web § 774 `init_align` for an `\halign`: its cells are
    /// restricted horizontal lists (§ 785), its rows go onto the enclosing
    /// vertical list (§ 799). The preamble is read (§ 777), then
    /// `\everycr` and the first row follow (§ 785).
    fn begin_halign(&mut self, span: Span) {
        let to = self.scan_spec();
        self.open_list(Some(group::ALIGN), Modes::RESTRICTED_HORIZONTAL, 0, span);
        let mut nest = self.env.nest().expect("the alignment's group is open");
        nest.to = to;
        nest.align = Some(Align { tabskip: self.glue_natural("tabskip"), ..Align::default() });
        self.env.set_nest(nest);
        if !self.capture_templates(span) {
            return;
        }
        let mut nest = self.env.nest().expect("the alignment's group is open");
        nest.align = nest.align.map(|a| Align { columns: self.tpl_int("n") > 1, ..a });
        self.env.set_nest(nest);
        self.mode = ALIGN_MODES;
        self.natural = Some(Natural { space_factor: Some(1000), tail: [Last::Empty; TAIL], ..Natural::default() });
        self.insert_every("everycr", span);
        self.template_row_start();
    }

    /// The name of the template state `what` of the innermost alignment.
    fn tpl(&mut self, what: &str) -> Sym {
        let depth = self.intern("\u{4}tpl.depth");
        let d = self.env.value(depth).as_int().unwrap_or(0);
        self.intern(&format!("\u{4}tpl.{what}.{d}"))
    }

    fn tpl_int(&mut self, what: &str) -> i64 {
        let sym = self.tpl(what);
        self.env.value(sym).as_int().unwrap_or(0)
    }

    /// Set at the alignment's level, from within the groups a template
    /// may have opened in a cell.
    fn tpl_set(&mut self, what: &str, value: Value) {
        let sym = self.tpl(what);
        self.env.set_value_in_place(sym, value);
    }

    /// A `\cr`, `\crcr`, `\span` or `\omit` primitive: its name.
    fn align_primitive(&self, token: Token) -> Option<&str> {
        let Tok::Cs(sym) = token.tok else { return None };
        if self.env.meaning(sym) != Meaning::Primitive(Primitive::Command(crate::builtins::Syntax::Nothing)) {
            return None;
        }
        Some(self.name(self.env.identity(sym))).filter(|n| matches!(*n, "cr" | "crcr" | "span" | "omit"))
    }

    /// tex.web §§ 777-784: the preamble's templates `u_j # v_j`, read as
    /// TeX reads them: unexpanded but for what `\span` expands, `\tabskip`
    /// assigned where it stands, `&` and `\cr` only outside braces. The
    /// preamble is not run; each cell gets its template (§§ 789-791).
    /// `false` where the input ended first.
    fn capture_templates(&mut self, span: Span) -> bool {
        let depth = self.intern("\u{4}tpl.depth");
        let d = self.env.value(depth).as_int().unwrap_or(0) + 1;
        self.env.set_value(depth, Value::Int(d), false);
        // Each template with the `\tabskip` glue that follows its column.
        let mut columns: Vec<(Vec<Token>, Vec<Token>, Dim)> = Vec::new();
        let (mut u, mut v, mut sharp) = (Vec::new(), Vec::new(), false);
        // § 779: an `&` that begins a template starts the periodic part.
        let mut period = None;
        let outside = self.align_state;
        let ended = loop {
            let Some(t) = self.preamble_token() else { break false };
            let meaning = match t.tok {
                Tok::Cs(sym) => Some(self.env.meaning(sym)),
                _ => None,
            };
            let tab = matches!(t.tok, Tok::Chr(_, Catcode::Tab))
                || matches!(meaning, Some(Meaning::Primitive(Primitive::AlignTab) | Meaning::Char(_, Catcode::Tab)));
            let cr = matches!(self.align_primitive(t), Some("cr" | "crcr"));
            let param = matches!(t.tok, Tok::Chr(_, Catcode::Param))
                || matches!(meaning, Some(Meaning::Char(_, Catcode::Param) | Meaning::Primitive(Primitive::AlignMark)));
            let ends = (tab || cr) && self.align_state == outside;
            if !sharp {
                if param {
                    sharp = true;
                } else if ends && tab && u.is_empty() && period.is_none() {
                    period = Some(columns.len());
                } else if ends {
                    // § 783: the `#` is taken to stand before this token.
                    self.diagnose(
                        Severity::Warning,
                        "missing-sharp",
                        t.span,
                        "a template of the alignment's preamble has no `#`".into(),
                    );
                    self.unread(t);
                    sharp = true;
                } else if !u.is_empty()
                    || !(matches!(t.tok, Tok::Chr(_, Catcode::Space))
                        || matches!(meaning, Some(Meaning::Char(_, Catcode::Space))))
                {
                    u.push(t);
                }
                continue;
            }
            if ends {
                columns.push((std::mem::take(&mut u), std::mem::take(&mut v), self.glue_natural("tabskip")));
                sharp = false;
                if cr {
                    break true;
                }
            } else if param {
                self.diagnose(Severity::Warning, "extra-sharp", t.span, "a template has a second `#`".into());
            } else {
                v.push(t);
            }
        };
        if !ended || columns.is_empty() {
            self.diagnose(Severity::Warning, "file-ended", span, "the input ended in the alignment's preamble".into());
            return false;
        }
        // The state lives at the alignment's level (`tpl_set`).
        let local = |m: &mut Self, what: &str, value: Value| {
            let sym = m.tpl(what);
            m.env.set_value(sym, value, false);
        };
        local(self, "ok", Value::Int(1));
        local(self, "n", Value::Int(columns.len() as i64));
        // A `&&` before the `\cr` makes nothing periodic.
        local(self, "period", Value::Int(period.filter(|p| *p < columns.len()).map_or(-1, |p| p as i64)));
        local(self, "m", Value::Int(columns.len() as i64));
        for what in ["col", "incell", "omit", "vdone", "cdepth", "spans"] {
            local(self, what, Value::Int(0));
        }
        local(self, "start", Value::Unknown);
        for (j, (u, v, skip)) in columns.into_iter().enumerate() {
            local(self, &format!("u.{j}"), Value::Toks(Rc::from(u)));
            local(self, &format!("v.{j}"), Value::Toks(Rc::from(v)));
            local(self, &format!("t.{j}"), skip.known().map_or(Value::Unknown, Value::Dimen));
            // Not set yet: no cell of the column has ended.
            local(self, &format!("w.{j}"), Value::Int(-1));
        }
        true
    }

    /// tex.web § 782 `get_preamble_token`: the next token of a preamble,
    /// past what `\span` expands and the `\tabskip` glue assigned there.
    fn preamble_token(&mut self) -> Option<Token> {
        loop {
            let t = self.next_token()?;
            if self.align_primitive(t) == Some("span") {
                self.expand_once();
                continue;
            }
            if let Tok::Cs(sym) = t.tok
                && self.name(self.env.identity(sym)) == "tabskip"
                && self.env.meaning(sym) == Meaning::Primitive(Primitive::GlueParameter { mu: false })
            {
                self.execute_extra(Primitive::GlueParameter { mu: false }, sym, t.span);
                continue;
            }
            return Some(t);
        }
    }

    /// Whether the cells of the innermost alignment, past its preamble, are
    /// set with their templates.
    fn templated(&mut self) -> bool {
        self.tpl_int("ok") == 1
    }

    /// An `&` or `\cr` ends a cell (tex.web § 791): its `v` part goes first,
    /// then the cell's width counts toward its column's (§ 796, 801).
    /// `true` where `token` was given back behind the `v` part.
    pub(crate) fn template_cell_end(&mut self, token: Token, spans: bool) -> bool {
        if !self.templated() || self.between_rows() {
            return false;
        }
        if self.tpl_int("incell") != 1 {
            self.tpl_fail();
            return false;
        }
        let j = self.tpl_int("col");
        let back = self.tpl_int("vdone") == 1 || self.tpl_int("omit") == 1;
        // Past the `v` part the cell's groups are closed (§ 1131).
        if back && self.env.depth() as i64 != self.tpl_int("cdepth") {
            self.tpl_fail();
            return false;
        }
        self.cell_base = None;
        if self.tpl_int("omit") == 0 && self.tpl_int("vdone") == 0 {
            self.tpl_set("vdone", Value::Int(1));
            let v = self.template_of(j);
            let v = self.tpl(&format!("v.{v}"));
            let Value::Toks(v) = self.env.value(v) else {
                self.tpl_fail();
                return false;
            };
            self.unread(token);
            self.push_tokens(v);
            return true;
        }
        if spans {
            // § 791: a `\span` packs nothing; the cell goes on in the next
            // column, and its width counts toward the columns it spans.
            self.tpl_set("spans", Value::Int(1));
            self.tpl_set("vdone", Value::Int(0));
            return false;
        }
        self.end_word();
        let along = self.natural.map_or(Dim::ANY, |n| n.along);
        let start = self.tpl("start");
        let start = match self.env.value(start) {
            Value::Dimen(s) => Dim::exact(s),
            _ => Dim::ANY,
        };
        let width = match (along.known(), start.known()) {
            (Some(a), Some(s)) => Some(a - s),
            _ => None,
        };
        let key = self.tpl(&format!("w.{j}"));
        // § 801: a spanned cell widens its last column by what the columns
        // it spans lack, which is not followed.
        let width = width.filter(|_| self.tpl_int("spans") == 0);
        let widest = match (self.env.value(key), width) {
            (Value::Int(-1), Some(w)) => Value::Dimen(w),
            (Value::Dimen(b), Some(w)) => Value::Dimen(b.max(w)),
            _ => Value::Unknown,
        };
        self.env.set_value_in_place(key, widest);
        for what in ["incell", "vdone", "spans"] {
            self.tpl_set(what, Value::Int(0));
        }
        false
    }

    /// The template of column `j`: past the last, the periodic part's
    /// again (tex.web § 793).
    fn template_of(&mut self, j: i64) -> i64 {
        let (n, p) = (self.tpl_int("n"), self.tpl_int("period"));
        if j < n || p < 0 { j } else { p + (j - p) % (n - p) }
    }

    /// The cells' templates are no longer followed.
    fn tpl_fail(&mut self) {
        self.cell_base = None;
        self.tpl_set("ok", Value::Int(0));
    }

    /// tex.web § 342: an `&` or `\cr` (`\crcr`) that `get_next` reads with
    /// `align_state` zero ends the cell where it is read, whoever reads it:
    /// a `\futurelet` looking ahead meets the cell's `v` part instead
    /// (§ 789), as do argument and skipped-text scans. `true` where the
    /// token was given back behind that `v` part.
    pub(crate) fn template_end_read(&mut self, token: Token) -> bool {
        if self.cell_base != Some(self.align_state) {
            return false;
        }
        let ends = match token.tok {
            Tok::Chr(_, Catcode::Tab) => true,
            // Engine bookkeeping, as § 336's `\outer` check: not a read of
            // the meaning.
            Tok::Cs(sym) => match self.env.slot(sym).map(|b| &b.meaning) {
                Some(Meaning::Char(_, Catcode::Tab) | Meaning::Primitive(Primitive::AlignTab)) => true,
                Some(Meaning::Primitive(Primitive::Command(crate::builtins::Syntax::Nothing))) => {
                    matches!(self.name(self.env.identity(sym)), "cr" | "crcr" | "span")
                }
                _ => false,
            },
            _ => false,
        };
        if !ends || !self.templated() || self.between_rows() {
            return false;
        }
        if self.tpl_int("incell") != 1 || self.tpl_int("vdone") != 0 || self.tpl_int("omit") != 0 {
            return false;
        }
        let spans = self.align_primitive(token) == Some("span");
        self.template_cell_end(token, spans)
    }

    /// A cell boundary satex does not follow (`\span`, an `&` it does not
    /// read as one): the cells' templates are no longer followed.
    pub(crate) fn template_lost(&mut self) {
        self.tpl_fail();
    }

    /// After an `&` or a `\span`: the next cell begins, or with `spans`
    /// the cell goes on in the next column (tex.web § 791).
    pub(crate) fn template_next_cell(&mut self, spans: bool, span: Span) {
        if !self.templated() || self.between_rows() {
            return;
        }
        let j = self.tpl_int("col") + 1;
        if j >= self.tpl_int("n") && self.tpl_int("period") < 0 {
            // § 792: the row ends as at a `\cr`.
            self.diagnose(
                Severity::Warning,
                "extra-alignment-tab",
                span,
                "a row has more cells than the preamble has templates".into(),
            );
            if spans {
                self.tpl_fail();
            }
            if self.align_cr(false, span) {
                self.template_row_start();
            }
            return;
        }
        if j >= self.tpl_int("m") {
            self.tpl_set("m", Value::Int(j + 1));
            let w = self.tpl(&format!("w.{j}"));
            self.env.set_value_in_place(w, Value::Int(-1));
        }
        self.tpl_set("col", Value::Int(j));
        let Some(t) = self.next_nonblank_x_token() else { return };
        self.template_cell_start(j, t, !spans);
    }

    /// After a `\cr`, a `\crcr` or a `\noalign`: the next row begins
    /// with its first cell, unless the alignment or the row is something
    /// else (tex.web § 785 `align_peek`).
    pub(crate) fn template_row_start(&mut self) {
        if !self.templated() || !self.between_rows() {
            return;
        }
        let Some(t) = self.next_nonblank_x_token() else { return };
        let end = match t.tok {
            Tok::Chr(_, Catcode::End) => true,
            Tok::Cs(sym) => {
                matches!(self.env.meaning(sym), Meaning::Char(_, Catcode::End))
                    || self.env.meaning(sym) == Meaning::Primitive(Primitive::Mode(ModeCmd::NoAlign))
                    || self.align_primitive(t) == Some("crcr")
            }
            _ => false,
        };
        if end {
            self.unread(t);
            return;
        }
        self.tpl_set("col", Value::Int(0));
        self.template_cell_start(0, t, true);
    }

    /// Cell `j` begins at `t`, the first token of it after expansion:
    /// `\omit` drops the template, else its `u` part goes first (§ 791).
    fn template_cell_start(&mut self, j: i64, t: Token, fresh: bool) {
        let omit = self.align_primitive(t) == Some("omit");
        if !omit {
            self.unread(t);
            let u = self.template_of(j);
            let u = self.tpl(&format!("u.{u}"));
            match self.env.value(u) {
                Value::Toks(u) => self.push_tokens(u),
                _ => {
                    self.tpl_fail();
                    return;
                }
            }
        }
        self.tpl_set("omit", Value::Int(i64::from(omit)));
        // § 324: past the `u` part `align_state` is zero; the `u` part is
        // balanced, so the cell's text begins at what it is now.
        self.cell_base = (!omit).then_some(self.align_state);
        self.tpl_set("cdepth", Value::Int(self.env.depth() as i64));
        self.tpl_set("incell", Value::Int(1));
        self.tpl_set("vdone", Value::Int(0));
        // § 791: a spanned cell goes on in the same list.
        if !fresh {
            return;
        }
        self.end_word();
        let start = self.natural.and_then(|n| n.along.known()).map_or(Value::Unknown, Value::Dimen);
        self.tpl_set("start", start);
        // § 787: each cell is a list of its own.
        if let Some(n) = &mut self.natural {
            n.tail = [Last::Empty; TAIL];
            (n.space_factor, n.ligature) = (Some(1000), None);
        }
    }

    fn next_nonblank_x_token(&mut self) -> Option<Token> {
        loop {
            let t = self.next_x_or_protected()?;
            if !matches!(t.tok, Tok::Chr(_, Catcode::Space)) {
                return Some(t);
            }
        }
    }

    /// The `\halign` whose group is the innermost.
    fn halign(&self) -> Option<Nest> {
        self.env.nest().filter(|n| n.code == Some(group::ALIGN) && n.align.is_some())
    }

    /// Between the rows of an `\halign`, where TeX only looks for what
    /// comes next (tex.web § 785 `align_peek`).
    pub(crate) fn between_rows(&self) -> bool {
        self.mode == ALIGN_MODES && self.list == Modes::RESTRICTED_HORIZONTAL && self.halign().is_some()
    }

    /// tex.web § 785: what a primitive does between rows. Anything but
    /// `\crcr` or `\noalign` begins a row.
    pub(crate) fn align_peek(&mut self, p: Primitive, by: Sym) {
        // § 785 looks past what expands: it begins no row.
        if p == Primitive::Mode(ModeCmd::NoAlign) || !self.between_rows() || crate::builtins::expandable(p) {
            return;
        }
        if p != Primitive::Command(crate::builtins::Syntax::Nothing) || self.name(self.env.identity(by)) != "crcr" {
            self.align_cell(false);
        }
    }

    /// tex.web § 785: anything but a space, `\crcr` or `\noalign` between
    /// rows begins a row. `tab`: a `&` or `\span`, so the row has columns.
    pub(crate) fn align_cell(&mut self, tab: bool) {
        if self.between_rows() {
            self.mode = Modes::RESTRICTED_HORIZONTAL;
        }
        if tab && let Some(mut nest) = self.halign() {
            nest.align = nest.align.map(|a| Align { columns: true, ..a });
            self.env.set_nest(nest);
        }
    }

    /// A `\cr` or `\crcr` ends the preamble or a row (tex.web §§ 779,
    /// 799), and `\everycr` follows. False where no `\halign` of satex's
    /// takes it.
    fn align_cr(&mut self, crcr: bool, span: Span) -> bool {
        let Some(mut nest) = self.halign() else { return false };
        // § 785: a `\crcr` right after a `\cr` does nothing.
        if !(crcr && self.between_rows()) {
            self.end_row(&mut nest);
            self.env.set_nest(nest);
            self.insert_every("everycr", span);
        }
        true
    }

    /// tex.web §§ 796, 799: each cell and its row are packed naturally, so
    /// the row's height and depth are maxima from zero.
    fn end_row(&mut self, nest: &mut Nest) {
        self.end_word();
        let Some(mut align) = nest.align else { return };
        let size = self.natural.map_or([Dim::ANY; 3], |n| [n.along, n.height.at_least_zero(), n.depth.at_least_zero()]);
        match self.templated() {
            true => {
                // Each cell holds its template: the row is as high and deep
                // as its cells (§ 799), the alignment as wide as its
                // columns and the glue after each used one (§ 801).
                let [_, h, d] = size;
                let lost = self.natural.is_some_and(|n| n.para.lost);
                let mut sum = align.tabskip.known();
                for j in 0..self.tpl_int("m") {
                    let (w, t) = (self.tpl(&format!("w.{j}")), self.template_of(j));
                    let t = self.tpl(&format!("t.{t}"));
                    sum = match (sum, self.env.value(w), self.env.value(t)) {
                        (s, Value::Int(-1), _) => s,
                        (Some(s), Value::Dimen(w), Value::Dimen(t)) => Some(s + w + t),
                        _ => None,
                    };
                }
                align.exact = sum.map(Dim::exact);
                align.widest = Some(Dim::ANY);
                let skips = self.interline();
                if let Some(n) = &mut nest.natural {
                    n.boxed(false, [None, Some(h), Some(d)], skips);
                    n.pushed(Last::Other);
                    if lost {
                        n.widen(false);
                    }
                }
            }
            // Cells past a boundary satex lost hold templates it did not
            // set: they add what they make.
            false => {
                align.exact = None;
                let [w, h, d] = size;
                let w = w.join(w.extended_by(Dim::ANY));
                let (h, d) = (h.join(h.max(Dim::ANY)), d.join(d.max(Dim::ANY)));
                align.widest = Some(align.widest.map_or(w, |m| m.max(w)));
                let lost = self.natural.is_some_and(|n| n.para.lost);
                let skips = self.interline();
                if let Some(n) = &mut nest.natural {
                    n.boxed(false, [None, Some(h), Some(d)], skips);
                    n.pushed(Last::Other);
                    // § 799: a cell's `\vadjust` material follows the row.
                    if lost {
                        n.widen(false);
                    }
                }
            }
        }
        nest.align = Some(align);
        self.mode = ALIGN_MODES;
        self.natural = Some(Natural { space_factor: Some(1000), tail: [Last::Empty; TAIL], ..Natural::default() });
    }

    /// tex.web §§ 800-812 `fin_align`: every row is as wide as the
    /// alignment, its columns' widths and `\tabskip` glue or its `to` size.
    fn align_width(nest: &Nest) -> Option<Dim> {
        let align = nest.align?;
        let widest = align.widest?;
        let natural = match align.exact {
            Some(exact) => exact,
            _ if align.columns => Dim::ANY,
            _ => widest.extended_by(align.tabskip),
        };
        Some(nest.to.size(natural))
    }

    /// tex.web § 1084 `scan_box`.
    pub(crate) fn scan_box(&mut self, context: BoxContext, span: Span) {
        loop {
            let Some(token) = self.next_x_token() else { return };
            let meaning = match token.tok {
                Tok::Chr(_, Catcode::Space) => continue,
                Tok::Cs(sym) => self.env.meaning(sym),
                Tok::Chr(c, Catcode::Active) => {
                    let sym = self.active_sym(c);
                    self.env.meaning(sym)
                }
                _ => Meaning::Undefined,
            };
            match meaning {
                Meaning::Primitive(Primitive::Relax) | Meaning::Char(_, Catcode::Space) => continue,
                Meaning::Primitive(Primitive::Mode(ModeCmd::Box(kind))) => {
                    return self.begin_box(kind, context, token.span);
                }
                // `\box⟨n⟩` fetches a register and leaves it void
                // (tex.web § 1079).
                Meaning::Primitive(Primitive::Register(crate::tex::RegKind::Box)) => {
                    let n = self.scan_number();
                    let dims = self.box_dims(n);
                    let fetched = self.fetch_box(n, true);
                    return self.box_end(context, fetched, dims);
                }
                // tex.web § 463: a `\vrule` 0.4pt wide, an `\hrule` 0.4pt
                // high and nothing deep, the rest running.
                Meaning::Primitive(Primitive::Mode(
                    rule @ (ModeCmd::Horizontal(Material::Rule) | ModeCmd::Vertical(Material::Rule)),
                )) if context == BoxContext::Leaders => {
                    let vrule = matches!(rule, ModeCmd::Horizontal(_));
                    let [w, h, d] = self.scan_rule_spec();
                    let size = |given: Option<Option<Scaled>>, default: Option<Scaled>| match given {
                        Some(Some(v)) => Some(Dim::exact(v)),
                        Some(None) => Some(Dim::ANY),
                        None => default.map(Dim::exact),
                    };
                    let w = size(w, vrule.then_some(DEFAULT_RULE));
                    let h = size(h, (!vrule).then_some(DEFAULT_RULE));
                    let d = size(d, (!vrule).then_some(0));
                    return self.leader_size([w, h, d]);
                }
                _ => {
                    self.diagnose(Severity::Warning, "missing-box", span, "a <box> was supposed to be here".into());
                    self.unread(token);
                    return;
                }
            }
        }
    }

    /// tex.web § 1079 `begin_box`.
    pub(crate) fn begin_box(&mut self, kind: BoxCmd, context: BoxContext, span: Span) {
        let (code, modes) = match kind {
            BoxCmd::Copy => {
                let n = self.scan_number();
                let dims = self.box_dims(n);
                let state = self.fetch_box(n, false);
                return self.box_end(context, state, dims);
            }
            // tex.web § 1080: `\lastbox` is the box at the tail of the
            // current list, and void where the tail is no box. The list
            // loses what it gives.
            BoxCmd::LastBox => {
                let tail = self.natural.map(|n| n.tail[0]);
                let state = match tail {
                    Some(Last::Glue(..) | Last::Kern(_) | Last::Penalty(_) | Last::Char(_) | Last::Empty) => {
                        BoxState::Void
                    }
                    // The list's only item is an hbox: the indent box
                    // (tex.web § 1091) or one appended to an empty list. It
                    // was the list's size, and the list is empty again.
                    Some(Last::Box) if self.list_kind() == Some(false) => {
                        let mut n = self.natural.expect("tail read");
                        let Some((w, h, d, kern)) = n.line else {
                            self.natural = None;
                            return self.box_end(context, BoxState::Unknown, [Dim::ANY; 3]);
                        };
                        // tex.web § 1080: the list is empty again;
                        // `\prevdepth` stays as it was.
                        (n.along, n.depth, n.wide, n.first, n.line) =
                            (Dim::exact(0), Dim::exact(0), Dim::exact(0), None, None);
                        n.tail = [Last::Empty; TAIL];
                        self.natural = Some(n);
                        self.box_end(context, BoxState::HBox, [w, h, d].map(Dim::exact));
                        if let BoxContext::Set { register, .. } = context {
                            self.set_box_line(register, Some((w, h, d, kern)));
                        }
                        return;
                    }
                    Some(Last::Box) => {
                        let mut n = self.natural.expect("tail read");
                        let dims = [n.along, n.height, n.depth];
                        n.along = Dim::exact(0);
                        n.height = Dim::exact(0);
                        n.depth = Dim::exact(0);
                        n.prior = true;
                        n.solid = 0;
                        n.tail = [Last::Empty; TAIL];
                        if n.para.items.is_some() {
                            n.para.items = Some(0);
                        }
                        self.natural = Some(n);
                        return self.box_end(context, BoxState::HBox, dims);
                    }
                    _ => {
                        // A horizontal list loses a box of some size; the
                        // vertical list a paragraph is in keeps its width.
                        match (self.list_kind(), &mut self.natural) {
                            (Some(true), Some(n)) => {
                                n.along = Dim::ANY;
                                n.height = Dim::ANY;
                                n.depth = Dim::ANY;
                                n.prior = true;
                                n.tail = [Last::Unknown; TAIL];
                                n.pending = None;
                                n.space_factor = None;
                                n.ligature = None;
                            }
                            _ => self.natural = None,
                        }
                        BoxState::Unknown
                    }
                };
                return self.box_end(context, state, [Dim::ANY; 3]);
            }
            // tex.web § 977: the split-off part is a vbox, or void when
            // the register is.
            BoxCmd::VSplit => {
                let n = self.scan_number();
                self.scan_keyword("to");
                let to = self.scan_dimen();
                let line = self.box_line(n);
                let dims = self.box_dims(n);
                let real = n.map(|k| self.box_state(k));
                let state = match self.fetch_box(n, false) {
                    BoxState::Void => BoxState::Void,
                    _ => BoxState::Unknown,
                };
                // tex.web §§ 970-977: a vbox of one line or nothing has no
                // legal breakpoint but its end, so `vert_break` splits all
                // of it off, packed to the height asked for, and the
                // register is void.
                let max = self.dimen_parameter("splitmaxdepth").known();
                if let (BoxState::Unknown, Some(line), Some(h), Some(max)) = (state, line, to, max)
                    && real == Some(BoxState::VBox)
                    && dims == line.map_or([Dim::exact(0); 3], |(w, h, d, _)| [w, h, d].map(Dim::exact))
                {
                    let (w, d) = line.map_or((0, 0), |(w, _, d, _)| (w, d));
                    let d = if d > max { max.max(0) } else { d };
                    self.box_changed(n, BoxState::Void);
                    self.fetched.clear();
                    self.box_end(context, BoxState::VBox, [Dim::exact(w.max(0)), Dim::exact(h), Dim::exact(d)]);
                    if let BoxContext::Set { register, .. } = context {
                        self.set_box_line(register, line);
                    }
                    return;
                }
                self.box_changed(n, BoxState::Unknown);
                self.fetched.clear();
                return self.box_end(context, state, [Dim::ANY; 3]);
            }
            BoxCmd::HBox => {
                // A box appended to a vertical list may take `\vadjust`
                // material with it.
                let code = match context {
                    BoxContext::Direct | BoxContext::Shift(_) => {
                        self.mode.test(Modes::ANY_VERTICAL).map(|v| if v { group::ADJUSTED_HBOX } else { group::HBOX })
                    }
                    _ => Some(group::HBOX),
                };
                (code, Modes::RESTRICTED_HORIZONTAL)
            }
            BoxCmd::VBox => (Some(group::VBOX), Modes::INTERNAL_VERTICAL),
            BoxCmd::VTop => (Some(group::VTOP), Modes::INTERNAL_VERTICAL),
            BoxCmd::VCenter => (Some(group::VCENTER), Modes::INTERNAL_VERTICAL),
        };
        let to = self.scan_spec();
        self.open_list(code, modes, 0, span);
        let mut nest = self.env.nest().expect("the box's group is open");
        nest.to = to;
        match context {
            BoxContext::Set { register, global, within } => nest.target = Some((register, global, within)),
            BoxContext::Direct => nest.placed = true,
            BoxContext::Shift(shift) => {
                nest.placed = true;
                nest.shift = shift;
            }
            // tex.web § 1078: the leader box itself goes on no list. The
            // glue that follows it is appended on its own.
            BoxContext::Leaders | BoxContext::Ship => {}
        }
        self.env.set_nest(nest);
        if modes == Modes::INTERNAL_VERTICAL {
            self.normal_paragraph(true);
            self.insert_every("everyvbox", span);
        } else {
            self.insert_every("everyhbox", span);
        }
    }
}

impl Machine<'_> {
    /// tex.web § 1075 `box_end` for a box that is already made: it goes
    /// into a register, onto the current list, or somewhere satex does not
    /// follow.
    fn box_end(&mut self, context: BoxContext, state: BoxState, dims: [Dim; 3]) {
        crate::observe::box_break(self);
        match context {
            BoxContext::Set { register, global, within } => {
                let ranges = std::mem::take(&mut self.fetched);
                self.set_box_chars(register, ranges);
                self.set_box(register, state, global, within);
                if state != BoxState::Unknown {
                    self.set_box_dims(register, dims);
                }
            }
            BoxContext::Direct => self.append_box(state, dims),
            BoxContext::Shift(shift) => {
                let dims = self.shifted(dims, shift);
                self.append_box(state, dims);
                // `\lastbox` takes the box back with its shift (§ 1081),
                // not at the size it gave the list.
                if let Some(n) = &mut self.natural
                    && n.tail[0] == Last::Box
                {
                    n.tail[0] = Last::Other;
                }
            }
            BoxContext::Ship => {
                let ranges = std::mem::take(&mut self.fetched);
                self.place_chars(ranges, false);
            }
            BoxContext::Leaders => {
                self.fetched.clear();
                if state != BoxState::Void {
                    self.leader_size(dims.map(Some));
                }
            }
        }
    }

    /// tex.web §§ 656, 671: the box or rule of leaders makes a horizontal
    /// list as high and deep as it is, a vertical one as wide; a running
    /// length (`None`) does neither.
    fn leader_size(&mut self, [w, h, d]: [Option<Dim>; 3]) {
        let horizontal = self.list_kind();
        let Some(n) = &mut self.natural else { return };
        match horizontal {
            Some(true) => {
                if let Some(h) = h {
                    n.height = n.height.max(h);
                }
                if let Some(d) = d {
                    n.depth = n.depth.max(d);
                }
            }
            Some(false) => {
                if let Some(w) = w {
                    n.wide = n.wide.max(w);
                }
            }
            None => {}
        }
    }

    /// tex.web § 653 and § 670: a shifted box raises the height and lowers
    /// the depth of a horizontal list, and widens a vertical one.
    fn shifted(&self, [w, h, d]: [Dim; 3], shift: Dim) -> [Dim; 3] {
        match self.list_kind() {
            Some(true) => [w, h.reduced_by(shift), d.extended_by(shift)],
            _ => [w.extended_by(shift), h, d],
        }
    }

    /// A box register's contents appended to the current list. A void
    /// register appends nothing (tex.web § 1076).
    pub(crate) fn append_box(&mut self, state: BoxState, dims: [Dim; 3]) {
        let ranges = std::mem::take(&mut self.fetched);
        self.place_chars(ranges, self.mode.within(Modes::INTERNAL_VERTICAL.union(Modes::RESTRICTED_HORIZONTAL)));
        crate::observe::box_break(self);
        // An hbox of known size, none of it above or below what an empty
        // horizontal list starts at, onto an empty horizontal list: the
        // list is that box, as `\lastbox` takes it back (§ 1080).
        let alone = state == BoxState::HBox
            && self.list_kind() == Some(true)
            && self.natural.is_some_and(|n| {
                n.tail[0] == Last::Empty && [n.along, n.height, n.depth].iter().all(|d| *d == Dim::exact(0))
            })
            && dims[0].known().is_some()
            && dims[1..].iter().all(|d| d.known().is_some_and(|v| v >= 0));
        match state {
            BoxState::Void => {}
            BoxState::Unknown => self.append_item(Item::Unknown),
            _ => self.append_item(Item::Box(dims)),
        }
        if alone && let Some(n) = &mut self.natural {
            n.tail = [Last::Empty; TAIL];
            n.tail[0] = Last::Box;
        }
    }

    /// A box or rule may have reached the main vertical list, unless the
    /// mode says it certainly went into a box of its own (tex.web § 1000).
    /// Past this point the page dimensions are the page builder's, not read
    /// as an empty page's, since satex does not run the page builder.
    pub(crate) fn touch_page(&mut self) {
        if self.mode.test(Modes::INNER) != Some(true) {
            self.page_touched = true;
        }
    }

    /// Whether the page builder has certainly not frozen the page
    /// dimensions yet (tex.web § 986).
    pub(crate) fn page_empty(&self) -> bool {
        !self.page_touched
    }

    /// Which list is being built: `Some(true)` horizontal, `Some(false)` a
    /// vertical list of its own. `None` for the main vertical list (whose
    /// tail the page builder may take away, tex.web § 1000), for math, and
    /// where the mode is not known.
    pub(crate) fn list_kind(&self) -> Option<bool> {
        if self.mode.is_empty() {
            None
        } else if self.mode.within(Modes::ANY_HORIZONTAL) {
            Some(true)
        } else if self.mode == Modes::INTERNAL_VERTICAL {
            Some(false)
        } else {
            None
        }
    }

    /// An item goes onto the current list: its natural size grows by it
    /// (tex.web §§ 651-656 `hpack`, 669-670 `vpack`).
    pub(crate) fn append_item(&mut self, item: Item) {
        self.material = true;
        if self.mode.meets(Modes::VERTICAL) {
            self.main_tail = match (self.mode == Modes::VERTICAL, &item) {
                (false, _) | (_, Item::Unknown | Item::Glue(None, _) | Item::Glue(_, None)) => Last::Unknown,
                (_, Item::Glue(Some(w), spec)) => Last::Glue(*w, *spec),
                (_, Item::Kern(Some(k))) => Last::Kern(*k),
                (_, Item::Penalty(Some(p))) => Last::Penalty(*p),
                (_, Item::Kern(None) | Item::Penalty(None)) => Last::Unknown,
                _ => Last::Other,
            };
        }
        // tex.web §§ 987, 1000: the page builder freezes the page dimensions
        // when a box or rule reaches the main vertical list, not for glue,
        // kerns, penalties, marks or whatsits, and never for a box's contents.
        if matches!(item, Item::Box(_) | Item::Rule { .. } | Item::Unknown) {
            self.touch_page();
        }
        self.end_word();
        let Some(mut n) = self.natural else { return };
        // A whatsit in inline math has no size (tex.web § 730).
        if matches!(item, Item::Zero | Item::Whatsit) && self.mode == Modes::MATH {
            // It is the math list's tail all the same (tex.web § 1105 sees it).
            match item {
                Item::Whatsit => n.pushed(Last::Whatsit),
                _ => n.perhaps(Last::Other),
            }
            self.natural = Some(n);
            return;
        }
        n.scripted = None;
        // A box in inline math is an ordinary atom (tex.web §§ 1076, 736):
        // the character before it takes its italic correction (§ 755), and
        // two ordinary atoms have no space between them (§ 764).
        if let (Item::Box([w, h, d]), Modes::MATH) = (&item, self.mode) {
            n.math_box = (h.lo == h.hi && d.lo == d.hi).then_some((h.lo, d.lo));
            self.next_atom(&mut n);
            n.along = n.along.extended_by(*w);
            (n.height, n.depth) = (n.height.max(*h), n.depth.max(*d));
            self.natural = Some(n);
            return;
        }
        // Glue or a kern in inline math is no atom: the spacing between
        // atoms passes over it (tex.web § 761), and the character before it
        // takes its italic correction (§§ 752, 755).
        if let (Item::Glue(w, _) | Item::Kern(w), Modes::MATH) = (&item, self.mode)
            && n.script.is_none()
        {
            n.math_box = None;
            if let Item::Glue(_, spec) = item {
                n.math_breaks = Self::more_breaks(n.math_breaks, spec.map(|g| (g.stretch.amount, g.shrink.amount)));
            }
            let italic = self.prev_italic(n.math_prev);
            n.along = n.along.extended_by(italic).extended_by(w.map_or(Dim::ANY, Dim::exact));
            n.math_prev = None;
            // The glue's specification is not kept: `\mskip` is in math
            // units, which `\lastskip` reports as such (tex.web § 424).
            n.pushed(match (&item, *w) {
                (Item::Glue(..), Some(w)) => Last::Glue(w, None),
                (Item::Kern(_), Some(w)) => Last::Kern(w),
                _ => Last::Unknown,
            });
            self.natural = Some(n);
            return;
        }
        let Some(horizontal) = self.list_kind() else {
            self.natural = None;
            return;
        };
        let skips = self.interline();
        if horizontal && n.para.items.is_some() {
            use crate::linebreak::Kp;
            let known = |d: Option<Option<Scaled>>| d.flatten();
            let k = match item {
                Item::Penalty(Some(..=-10000)) if self.mode == Modes::HORIZONTAL => None,
                Item::Penalty(Some(p)) => Some(Some(Kp::Penalty(p))),
                Item::Glue(_, Some(g)) => Some(Some(Kp::glue(g))),
                Item::Kern(Some(w)) => Some(Some(Kp::Kern { w, explicit: true })),
                Item::Whatsit => Some(Some(Kp::Whatsit)),
                Item::Box(b) => {
                    Some(b.map(|x| x.known()).iter().copied().collect::<Option<Vec<_>>>().map(|b| Kp::Box {
                        w: b[0],
                        h: b[1],
                        d: b[2],
                        chr: None,
                        lig: false,
                    }))
                }
                Item::Rule { width, height, depth } => {
                    Some(known(width).zip(known(height)).zip(known(depth)).map(|((w, h), d)| Kp::Box {
                        w,
                        h,
                        d,
                        chr: None,
                        lig: false,
                    }))
                }
                _ => Some(None),
            };
            match k {
                Some(Some(k)) => self.kp_push(&mut n.para.items, k),
                Some(None) => n.para.items = None,
                None => {}
            }
        }
        // tex.web § 463: a running length is `null_flag`, which neither
        // maximum in § 651 nor § 668 ever takes.
        // tex.web § 1096: glue, a kern, a penalty or a whatsit makes the
        // paragraph not null.
        if horizontal && matches!(item, Item::Glue(..) | Item::Kern(_) | Item::Penalty(_) | Item::Whatsit) {
            n.prior = false;
        }
        let rule = |d: Option<Option<Scaled>>| match d {
            None => None,
            Some(Some(v)) => Some(Dim::exact(v)),
            Some(None) => Some(Dim::ANY),
        };
        match item {
            // Nothing, or a whatsit: the tail is the new item or the one
            // that was there, so a last glue, kern or penalty is in doubt.
            Item::Zero => {
                // A mark or insertion would end the discarding (tex.web § 879).
                n.para.awkward |= n.para.fresh;
                n.para.fresh = false;
                n.perhaps(Last::Other)
            }
            Item::Whatsit => {
                n.para.fresh = false;
                n.pushed(Last::Whatsit)
            }
            Item::Penalty(p) => {
                // tex.web § 859: a negative penalty may make a break pay.
                if p.is_none_or(|p| p > -10000 && p < 10000) {
                    n.para.breakpoints = n.para.breakpoints.saturating_add(1);
                    n.para.awkward |= p.is_none_or(|p| p < 0);
                }
                // tex.web § 851: a penalty of -10000 or less forces a line
                // break.
                match p {
                    Some(p) if p <= -10000 && self.mode == Modes::HORIZONTAL => self.forced_break(&mut n),
                    _ if self.mode.meets(Modes::HORIZONTAL) && p.is_none_or(|p| p <= -10000) => {
                        n.along = Dim::ANY;
                        n.para.lost = true;
                    }
                    _ => {}
                }
                n.pushed(p.map_or(Last::Unknown, Last::Penalty));
            }
            Item::Kern(k) => {
                let before = (n.along, n.depth, n.first);
                n.advance(horizontal, k.map_or(Dim::ANY, Dim::exact));
                n.pushed(k.map_or(Last::Unknown, Last::Kern));
                if !horizontal {
                    n.undo = Some((n.tail[0], [before, (n.along, n.depth, n.first)]));
                }
            }
            Item::Glue(w, spec) => {
                // tex.web § 868: glue is a legal breakpoint only after a
                // node that is not discardable, or after a kern.
                // Any glue lets the next word be hyphenated (§ 866).
                let after = !matches!(n.tail[0], Last::Glue(..) | Last::Penalty(_) | Last::Empty);
                if horizontal && !n.para.fresh {
                    if after {
                        n.para.breakpoints = n.para.breakpoints.saturating_add(1);
                    }
                    n.para.glued = true;
                    // tex.web § 859: negative infinite stretch may make a
                    // break pay; negative finite stretch, a line worse
                    // than `set_line` bounds it.
                    n.para.awkward |= spec.is_none_or(|g| g.stretch.amount < 0);
                }
                let before = (n.along, n.depth, n.first);
                n.advance(horizontal, w.map_or(Dim::ANY, Dim::exact));
                n.pushed(match w {
                    Some(w) => Last::Glue(w, spec),
                    None => Last::Unknown,
                });
                if !horizontal {
                    n.undo = Some((n.tail[0], [before, (n.along, n.depth, n.first)]));
                }
            }
            Item::Rule { width, height, depth } => {
                n.boxed(horizontal, [rule(width), rule(height), rule(depth)], skips);
                n.solid = n.solid.saturating_add(1);
                // tex.web § 1056: no interline glue after a rule.
                if !horizontal {
                    n.prev_depth = None;
                }
                n.pushed(Last::Other);
            }
            Item::Box(dims) => {
                n.boxed(horizontal, dims.map(Some), skips);
                n.pushed(Last::Other);
                n.solid = n.solid.saturating_add(1);
            }
            Item::Unknown => {
                n.widen(horizontal);
                n.para.breakpoints = u32::MAX;
                n.para.awkward = true;
                // Material satex did not follow may have set it.
                n.space_factor = None;
                n.tail = [Last::Unknown; TAIL];
            }
        }
        self.natural = Some(n);
    }

    /// The node that ends a formula, `w` wide (tex.web § 1196): a
    /// breakpoint where glue follows (§ 866), its width gone at a break
    /// (§ 881) and discarded after one (§ 879), as an explicit kern's. A
    /// formula that breaks at its own penalties (§ 761) or stretches
    /// (`breaks` of them at most) is not followed: its breakpoints are
    /// counted, and a penalty that may make a break pay makes the list
    /// awkward.
    fn math_off(&mut self, w: Dim, breaks: u32) {
        let Some(mut n) = self.natural else { return };
        n.advance(true, w);
        let kern = w.known().filter(|_| breaks == 0);
        if let Some(w) = kern {
            self.kp_push(&mut n.para.items, crate::linebreak::Kp::Kern { w, explicit: true });
        }
        self.natural = Some(n);
        if breaks == 0 && kern.is_none() {
            self.unfollowed_breaks();
        } else if breaks > 0 {
            let pens = ["binoppenalty", "relpenalty"].map(|name| self.int_parameter(name));
            let odd = breaks == u32::MAX || pens.iter().any(|p| p.is_none_or(|p| p < 0));
            if let Some(n) = &mut self.natural {
                n.para.breakpoints = n.para.breakpoints.saturating_add(breaks).saturating_add(2);
                n.para.awkward |= odd;
                n.para.items = None;
            }
        }
    }

    /// One more place a formula may break or stretch at, given the stretch
    /// and shrink of the glue there (`None`: not known): glue that is
    /// negative or unknown can be anywhere.
    fn more_breaks(count: u32, spec: Option<(Scaled, Scaled)>) -> u32 {
        match spec {
            Some((0, 0)) => count,
            Some((st, sh)) if st >= 0 && sh >= 0 => count.saturating_add(1),
            _ => u32::MAX,
        }
    }

    /// Material whose breakpoints and penalties are not followed went onto
    /// the line.
    fn unfollowed_breaks(&mut self) {
        if let Some(n) = &mut self.natural {
            n.para.breakpoints = u32::MAX;
            n.para.awkward = true;
            n.para.items = None;
        }
    }

    /// tex.web § 1110 `unpackage`: the box's list goes onto the current
    /// one.  `hpack` (§ 649) takes the largest height and depth, which the
    /// box has whatever it was packed to; `vpack` (§ 668) the largest
    /// width.  The sum along the list is followed only for an hbox of
    /// boxes and rules packed naturally, recorded as its `first`, with
    /// `solid` of them: § 1110 moves `tail` to the list's last, so the
    /// tail is then known. Any other hbox adds its recorded sum along.
    fn unpackage(&mut self, state: BoxState, [w, h, d]: [Dim; 3], first: Option<Dim>, solid: Option<u8>) {
        crate::observe::box_break(self);
        match (state, self.list_kind()) {
            (BoxState::HBox, Some(true)) if let Some((along, k)) = first.zip(solid) => {
                if k == 0 {
                    return;
                }
                self.append_item(Item::Box([along, h, d]));
                if let Some(n) = &mut self.natural {
                    for _ in 1..k {
                        n.pushed(Last::Other);
                    }
                    n.solid = n.solid.saturating_add(k - 1);
                }
                return;
            }
            // Any other hbox: its items keep their natural widths (§ 1110
            // moves the list, glue unset), so they add the sum along it
            // that was recorded when it was packed.
            (BoxState::HBox, Some(true)) => {
                self.append_item(Item::Box([first.unwrap_or(Dim::ANY), h, d]));
                self.unfollowed_breaks();
            }
            // A naturally packed list adds up to the box's height and
            // depth, and begins with its recorded first box.
            (BoxState::VBox, Some(false)) if first.is_some() => {
                let had = self.natural.and_then(|n| n.first);
                self.append_item(Item::Box([w, h, d]));
                if let Some(n) = &mut self.natural {
                    n.first = had.or(first);
                }
            }
            (BoxState::VBox, Some(false)) => {
                self.append_item(Item::Box([w, Dim::ANY, Dim::ANY]));
            }
            _ => return self.append_item(Item::Unknown),
        }
        // What the list ends in, and its space factor, are the box's.
        if let Some(n) = &mut self.natural {
            n.tail = [Last::Unknown; TAIL];
            n.space_factor = None;
        }
    }

    /// Interword glue (tex.web §§ 1041-1044): `\spaceskip` or the font's
    /// space/stretch/shrink, scaled by the space factor unless it is 1000,
    /// and at 2000 or more `\xspaceskip` or that plus the font's extra space.
    /// A control space takes the first (§ 1045). Where the space factor is
    /// not known, the width is known only if both choices agree.
    pub(crate) fn space_glue(&mut self, control: bool) -> (Option<Scaled>, Option<crate::value::Glue>) {
        use crate::value::{Glue, Stretch};
        let factor = self.natural.and_then(|n| n.space_factor);
        let font = [2, 3, 4, 7].map(|k| self.current_font_dimen(k));
        let mut skip = |name: &str| {
            let sym = self.intern(name);
            match self.env.value(sym) {
                // A zero skip is `zero_glue` (§ 1229), which § 1041 passes over.
                Value::Glue(g) => Some((g != Glue::default()).then_some(g)),
                _ => None,
            }
        };
        let (Some(space_skip), Some(xspace_skip), [Some(space), Some(stretch), Some(shrink), Some(extra)]) =
            (skip("spaceskip"), skip("xspaceskip"), font)
        else {
            return (None, None);
        };
        let normal = space_skip.unwrap_or(Glue {
            width: space,
            stretch: Stretch { amount: stretch, order: 0 },
            shrink: Stretch { amount: shrink, order: 0 },
        });
        let spaced = |sf: i64| -> Glue {
            if control || sf == 1000 {
                return normal;
            }
            if sf >= 2000
                && let Some(x) = xspace_skip
            {
                return x;
            }
            let mut g = normal;
            if sf >= 2000 {
                g.width += extra;
            }
            g.stretch.amount = crate::scan::xn_over_d(g.stretch.amount, sf, 1000).0;
            g.shrink.amount = crate::scan::xn_over_d(g.shrink.amount, 1000, sf).0;
            g
        };
        match factor {
            Some(sf) => {
                let g = spaced(sf);
                (Some(g.width), Some(g))
            }
            None => {
                let wide = xspace_skip.map_or(normal.width + extra, |x| x.width);
                ((control || wide == normal.width).then_some(normal.width), control.then_some(normal))
            }
        }
    }

    /// What a primitive that may put an item on the current list does to its
    /// natural size before it is obeyed. Primitives that scan their size
    /// first (kerns, penalties, glue, rules, boxes) add it once they have.
    pub(crate) fn natural_before(&mut self, p: Primitive, by: Sym, at: Span) {
        use crate::builtins::{PdfOp, Syntax, Typeset};
        // tex.web § 1113 `append_italic_correction`: `\/` is a kern, not
        // nothing, so it may not be passed over as a zero-size command.
        if p == Primitive::Command(Syntax::Nothing) {
            let name = self.name(self.env.identity(by));
            let (cr, crcr, span) = (name == "cr", name == "crcr", name == "span");
            if name == "/" {
                return self.italic_correction();
            }
            if (cr || crcr || span) && self.template_cell_end(Token::new(Tok::Cs(by), at), span) {
                return;
            }
            if (cr || crcr) && self.align_cr(crcr, at) {
                self.template_row_start();
                return;
            }
            if span {
                self.align_cell(true);
                self.template_next_cell(true, at);
            }
        }
        // `\pdfrefximage` adds its box once its number is read, and so do
        // `\mathchar` and `\delimiter` their atom in inline math.
        if p == Primitive::Command(Syntax::Number)
            && (self.name(self.env.identity(by)) == "pdfrefximage" || self.math_char_command(by).is_some())
        {
            return;
        }
        let item = match p {
            Primitive::Mode(
                ModeCmd::Horizontal(_)
                | ModeCmd::Vertical(_)
                | ModeCmd::Box(_)
                | ModeCmd::Shift { .. }
                | ModeCmd::Leaders
                | ModeCmd::ShipOut
                | ModeCmd::Insert { .. }
                | ModeCmd::StartPar { .. }
                | ModeCmd::QuitVMode
                | ModeCmd::MathStyle(_)
                | ModeCmd::MathChoice,
            )
            | Primitive::Typeset(Typeset::Number | Typeset::Dimen | Typeset::Glue) => return,
            // tex.web § 785: its material goes between the rows.
            Primitive::Mode(ModeCmd::NoAlign) if self.between_rows() => return,
            // These can't be `\immediate`, so they always add a whatsit.
            Primitive::Pdf(PdfOp::Literal | PdfOp::Special)
            | Primitive::Command(Syntax::PdfColorStack | Syntax::PdfStartLink | Syntax::Whatsit) => Item::Whatsit,
            // tex.web § 1375: `\immediate` acts at once and leaves the list as
            // it was; otherwise a whatsit goes on it (§§ 1350-1352).
            Primitive::Write | Primitive::Command(Syntax::OpenOut | Syntax::CloseOut) if self.prefixes.immediate => {
                return;
            }
            Primitive::Write | Primitive::Command(Syntax::OpenOut | Syntax::CloseOut) => Item::Whatsit,
            Primitive::Pdf(PdfOp::Object | PdfOp::Resources) => Item::Zero,
            // What a command appends has no size, except for math material
            // and images and forms placed as boxes (tex.web § 1058, the
            // pdfTeX, XeTeX and LuaTeX manuals).
            Primitive::Command(syntax) => match syntax {
                Syntax::Number
                | Syntax::Dimen
                | Syntax::Delimiters { .. }
                | Syntax::Numbers(_)
                | Syntax::MathAccent
                | Syntax::PicFile
                | Syntax::RuleSpec
                | Syntax::BoxSpec
                | Syntax::VSplit => Item::Unknown,
                // tex.web § 1241: `\setbox` is an assignment and puts
                // nothing on the current list, so the tail stays as it is.
                // So are font and code assignments, messages (§ 1293) and
                // the document-level PDF objects (pdfTeX manual).
                Syntax::SetBox
                | Syntax::Interaction(_)
                | Syntax::Token
                | Syntax::Internal
                | Syntax::FamilyFont
                | Syntax::CopyFont { .. }
                | Syntax::FontExpand
                | Syntax::ReadLine
                | Syntax::Font
                | Syntax::FontText
                | Syntax::PdfCatalog
                | Syntax::PdfOutline
                | Syntax::TwoTexts
                | Syntax::UMathCharDef { .. }
                | Syntax::SetFontId => return,
                _ => Item::Zero,
            },
            _ => Item::Unknown,
        };
        self.append_item(item);
    }

    /// A character typeset in the current font (tex.web §§ 1034-1040): it
    /// kerns or forms a `=:` ligature with the character before it in the
    /// same word, or with the font's left boundary at the word's start.
    /// Other ligatures, missing characters and native fonts are not followed.
    pub(crate) fn append_char(&mut self, c: u32) {
        self.material = true;
        // The lines a paragraph breaks into are boxes, so a character set
        // outside a box will reach the page.
        self.touch_page();
        if self.natural.is_none() {
            return;
        }
        if self.mode == Modes::MATH {
            return self.math_char(c);
        }
        if self.list_kind() != Some(true) {
            self.natural = None;
            return;
        }
        let font = self.current_font();
        let known = font.and_then(|f| Some((f, self.font_metrics(f)?, self.font_size(f)?)));
        let Some((font, metrics, size)) = known else { return self.unmetered_char(c) };
        if !metrics.has_char(c) {
            return self.unmetered_char(c);
        }
        let mut n = self.natural.expect("checked above");
        // tex.web § 1039: a character equal to `\hyphenchar` is followed by a discretionary.
        if !matches!(self.font_field(font, "hyphenchar"), Value::Int(h) if h != i64::from(c)) {
            n.para.breakpoints = n.para.breakpoints.saturating_add(1);
        }
        let within = matches!(n.pending, Some((f, _)) if f == font);
        let step = match n.pending {
            Some((f, left)) if f == font => metrics.lig_kern(left, c, size),
            // A word starts: it meets the font's left boundary first.
            _ => {
                self.end_word();
                let Some(next) = self.natural else { return };
                n = next;
                match std::mem::take(&mut n.no_boundary) {
                    true => None,
                    false => metrics.left_boundary(c, size),
                }
            }
        };
        match step {
            // The boundary and the first character make one ligature node.
            Some(crate::tfm::LigKern::Lig { op: 0, char }) if !within && metrics.has_char(char) => {
                n.pending = Some((font, char));
                n.pushed(Last::Char(None));
                n.ligature = Some(true);
            }
            // The ligature takes the place of the character before it, so
            // the tail is still the one node.
            Some(crate::tfm::LigKern::Lig { op: 0, char }) if metrics.has_char(char) => {
                n.pending = Some((font, char));
                n.tail[0] = Last::Char(None);
                n.ligature = Some(true);
            }
            // The other ligature operations keep or repeat characters. The
            // size of what they leave is not followed.
            Some(crate::tfm::LigKern::Lig { .. }) => {
                n.pending = None;
                n.para.items = None;
                let sf = n.space_factor;
                n.widen(true);
                n.space_factor = sf;
                n.tail = [Last::Unknown; TAIL];
                n.tail[0] = Last::Char(None);
            }
            step => {
                self.settle_char();
                let Some(next) = self.natural else { return };
                n = next;
                if let Some(crate::tfm::LigKern::Kern(k)) = step {
                    n.along = n.along.extended_by(Dim::exact(k));
                    self.kp_push(&mut n.para.items, crate::linebreak::Kp::Kern { w: k, explicit: false });
                }
                n.pending = Some((font, c));
                n.pushed(Last::Char(None));
                n.ligature = Some(false);
            }
        }
        n.space_factor = self.space_factor_after(c, n.space_factor);
        self.natural = Some(n);
    }

    /// tex.web §§ 1194-1196: inline math ends. A list of plain characters
    /// adds its size and two `\mathsurround` kerns; anything else widens.
    fn close_math(&mut self, math: Option<Natural>) {
        if self.natural.is_none() || self.list_kind() != Some(true) {
            self.natural = None;
            return;
        }
        match math.filter(|m| m.script.is_none()) {
            Some(mut m) => {
                Self::end_atoms(&mut m);
                let surround = self.dimen_parameter("mathsurround");
                let last = self.prev_italic(m.math_prev);
                let width = m.along.extended_by(last).extended_by(surround);
                self.append_item(Item::Box([width, m.height, m.depth]));
                self.math_off(surround, m.math_breaks);
                if let Some(n) = self.natural.as_mut() {
                    n.space_factor = Some(1000);
                }
            }
            // Its size is not known, but tex.web § 1196 ends the formula
            // with a math node and sets the space factor to 1000 whatever
            // it holds: `\lastskip`, `\lastkern` and `\lastpenalty` are
            // zero after it, and `\unskip` takes nothing away (§§ 424, 1105).
            None => {
                self.append_item(Item::Unknown);
                if let Some(n) = self.natural.as_mut() {
                    n.pushed(Last::Other);
                    n.space_factor = Some(1000);
                }
            }
        }
    }

    /// The italic correction of the character an atom follows, as text
    /// does not take it (tex.web § 755): none after no character, unknown
    /// after an unknown one.
    fn prev_italic(&mut self, prev: Option<Option<(i64, u32, u32)>>) -> Dim {
        match prev {
            Some(Some((_, font, c))) => self.math_italic(font, c, false),
            Some(None) => Dim::ANY,
            None => Dim::exact(0),
        }
    }

    /// tex.web § 755: the italic correction a math character adds, none for
    /// a text character in a font with interword space.
    fn math_italic(&mut self, font: u32, c: u32, text: bool) -> Dim {
        let known = self.font_metrics(font).zip(self.font_size(font));
        let Some((m, z)) = known else { return Dim::ANY };
        let Some(b) = m.char_box(c, z) else { return Dim::ANY };
        match self.font_dimen(font, 2).and_then(|v| v.as_dimen()) {
            Some(space) if text && space != 0 => Dim::exact(0),
            Some(_) => Dim::exact(b.italic),
            None if text => Dim::range(b.italic.min(0), b.italic.max(0)),
            None => Dim::exact(b.italic),
        }
    }

    /// tex.web §§ 1151-1155, 752-755: a character in inline math, while the
    /// list holds only characters. A plain Ord adds its box, its italic
    /// correction and its kern with the one before it. Other
    /// classes add math spacing, so the width is lost. An operator is
    /// shifted, so the whole size is.
    fn math_char(&mut self, c: u32) {
        if self.natural.is_none() {
            return;
        }
        let Some((class, fam, ch)) = self.math_class(c) else {
            self.natural = None;
            return;
        };
        self.math_atom(class, fam, ch);
    }

    /// In inline math, `\mathchar` and `\delimiter` are atoms of the math
    /// code their number gives: the number itself, and a delimiter's small
    /// variant (tex.web §§ 1151, 1160). The shift that takes the code from
    /// the number.
    pub(crate) fn math_char_command(&self, by: Sym) -> Option<u32> {
        if self.mode != Modes::MATH {
            return None;
        }
        match self.name(self.env.identity(by)) {
            "mathchar" => Some(0),
            "delimiter" => Some(12),
            _ => None,
        }
    }

    /// tex.web § 1155: `\mathchar` and a `\mathchardef` constant, in inline
    /// math: the atom of math code `code`, its family from `\fam` for class 7.
    pub(crate) fn math_code_char(&mut self, code: Option<i64>) {
        if self.natural.is_none() {
            return;
        }
        let Some(code) = code.filter(|&v| (0..0x8000).contains(&v)) else {
            self.natural = None;
            return;
        };
        let (class, fam, ch) = (code >> 12, (code >> 8) & 0xf, (code & 0xff) as u32);
        let fam = match class {
            7 => {
                let sym = self.intern("fam");
                match self.env.value(sym) {
                    Value::Int(f) if (0..16).contains(&f) => f,
                    Value::Int(_) => fam,
                    _ => {
                        self.natural = None;
                        return;
                    }
                }
            }
            _ => fam,
        };
        self.math_atom(class, fam, ch);
    }

    /// tex.web §§ 764-766: the space between atoms of type `left` and
    /// `right` in text style (0 ord … 7 inner): none, or `\thinmuskip`,
    /// `\medmuskip` or `\thickmuskip` at their natural width.
    fn math_space(&mut self, left: Option<i8>, right: u8, size: u8) -> Dim {
        let Some(left) = left else { return Dim::exact(0) };
        // The spaces of the table that hold in text style only.
        if matches!(Self::space_code(left, right), Some(b'0'))
            || (size > 0 && matches!(Self::space_code(left, right), Some(b'2'..=b'4')))
        {
            return Dim::exact(0);
        }
        let Some(name) = Self::space_name(Some(left), right) else { return Dim::ANY };
        let sym = self.intern(name);
        let Value::MuGlue(g) = self.env.value(sym) else { return Dim::ANY };
        self.mu_width(g.width, size).map_or(Dim::ANY, Dim::exact)
    }

    /// tex.web § 764: the entry of the spacing table between atoms of
    /// types `left` and `right`.
    fn space_code(left: i8, right: u8) -> Option<u8> {
        const TABLE: [&[u8; 8]; 8] =
            [b"02340001", b"22*40001", b"33**3**3", b"44*04004", b"00*00000", b"02340001", b"11*11111", b"12341011"];
        let row = usize::try_from(left).ok().and_then(|l| TABLE.get(l))?;
        row.get(usize::from(right)).copied()
    }

    /// The muskip that spaces atoms of types `left` and `right` in text
    /// style, where one does.
    fn space_name(left: Option<i8>, right: u8) -> Option<&'static str> {
        match Self::space_code(left?, right)? {
            b'1' | b'2' => Some("thinmuskip"),
            b'3' => Some("medmuskip"),
            b'4' => Some("thickmuskip"),
            _ => None,
        }
    }

    /// The stretch and shrink of the muskip `name`, where known.
    fn muskip_stretch(&mut self, name: &str) -> Option<(Scaled, Scaled)> {
        let sym = self.intern(name);
        match self.env.value(sym) {
            Value::MuGlue(g) => Some((g.stretch.amount, g.shrink.amount)),
            _ => None,
        }
    }

    /// A width in math units in math size `size` (tex.web §§ 716-717, 732):
    /// `cur_mu` is the math quad over 18, applied as `n.f`.
    pub(crate) fn mu_width(&mut self, width: Scaled, size: u8) -> Option<Scaled> {
        const UNIT: i64 = 1 << 16;
        let by = self.size_font(size);
        let quad = self.family_font(by, 2).and_then(|f| self.font_dimen(f, 6)).and_then(|v| v.as_dimen());
        let m = quad.and_then(|q| crate::value::x_over_n(q, 18))?;
        let (n, f) = (m.div_euclid(UNIT), m.rem_euclid(UNIT));
        let y = crate::scan::xn_over_d(width, f, UNIT).0;
        n.checked_mul(width).and_then(|v| v.checked_add(y)).filter(|w| w.abs() <= crate::value::MAX_DIMEN)
    }

    /// The math size of the formula being built: 0 text, 1 script, 2
    /// scriptscript.
    pub(crate) fn math_size(&self) -> u8 {
        self.natural.map_or(0, |n| n.style >> 1)
    }

    /// Whether the list being built is inline math whose atoms are followed.
    pub(crate) fn math_followed(&self) -> bool {
        self.mode == Modes::MATH && self.natural.is_some_and(|n| n.script.is_none())
    }

    /// The pending atom follows the character before it: that character's
    /// italic correction, then the space before the atom. Its type.
    fn next_atom(&mut self, n: &mut Natural) -> u8 {
        let italic = self.prev_italic(n.math_prev);
        let t = n.math_pending.take().unwrap_or(0);
        let space = self.begin_atom(n, t);
        n.along = n.along.extended_by(italic).extended_by(space);
        n.math_prev = None;
        // The noad that holds it is the list's new tail.
        n.pushed(Last::Other);
        t
    }

    /// tex.web §§ 728-729, 764: an atom of type `t` follows. A binary atom
    /// first in the list or after a binary, operator, relation, opening or
    /// punctuation is ordinary. A binary atom before a relation, closing or
    /// punctuation was ordinary, so its spacing is taken back. The space
    /// before the atom.
    fn begin_atom(&mut self, n: &mut Natural, t: u8) -> Dim {
        let t = if t == 2 && matches!(n.math_class, None | Some(1..=4) | Some(6)) { 0 } else { t };
        let mut left = n.math_class;
        if let Some(back) = n.math_bin.take()
            && matches!(t, 3 | 5 | 6)
        {
            n.along = match back {
                Scaled::MIN => Dim::ANY,
                b => n.along.reduced_by(Dim::exact(b)),
            };
            left = Some(0);
        }
        let size = n.style >> 1;
        let space = self.math_space(left, t, size);
        if let Some(name) = Self::space_name(left, t) {
            let spec = self.muskip_stretch(name);
            n.math_breaks = Self::more_breaks(n.math_breaks, spec);
        }
        if t == 2 {
            let ord = self.math_space(left, 0, size);
            n.math_bin = Some(match (space.known(), ord.known()) {
                (Some(a), Some(b)) => a - b,
                _ => Scaled::MIN,
            });
        }
        n.math_class = Some(t as i8);
        if matches!(t, 2 | 3) {
            n.math_breaks = n.math_breaks.saturating_add(1);
        }
        space
    }

    /// tex.web § 729: a binary atom last in its list is ordinary.
    fn end_atoms(n: &mut Natural) {
        if let Some(back) = n.math_bin.take() {
            n.along = match back {
                Scaled::MIN => Dim::ANY,
                b => n.along.reduced_by(Dim::exact(b)),
            };
        }
    }

    /// `\mathord` … `\mathinner` in inline math: the next atom's type.
    pub(crate) fn math_comp(&mut self, t: u8) {
        if let Some(n) = &mut self.natural {
            n.math_pending = Some(t);
        }
    }

    /// One atom of class `class` from family `fam`, while the list holds
    /// only characters (tex.web §§ 752-755, 764).
    fn math_atom(&mut self, class: i64, fam: i64, ch: u32) {
        let Some(mut n) = self.natural else { return };
        if let Some(sup) = n.script {
            return self.math_script_char(n, sup, class, fam, ch);
        }
        (n.math_box, n.scripted) = (None, None);
        // tex.web § 1151: a noad is the list's new tail.
        n.pushed(Last::Other);
        let t = n.math_pending.take().unwrap_or(if class == 7 { 0 } else { class as u8 });
        let by = self.size_font(n.style >> 1);
        let font = self.family_font(by, fam);
        let known = font.and_then(|f| Some((f, self.font_metrics(f)?, self.font_size(f)?)));
        let Some((font, m, z, b)) = known.and_then(|(f, m, z)| Some((f, m.clone(), z, m.char_box(ch, z)?))) else {
            self.natural = None;
            return;
        };
        // tex.web § 752: a character before one of the same family is text:
        // it kerns with it, and keeps its italic correction only in a font
        // without interword space (§ 755).
        let ord = n.math_class == Some(0);
        match n.math_prev {
            Some(Some((f, pf, left))) => {
                let text = f == fam && ord && t <= 6;
                n.along = n.along.extended_by(self.math_italic(pf, left, text));
                if text {
                    match m.lig_kern(left, ch, z) {
                        Some(crate::tfm::LigKern::Kern(k)) => n.along = n.along.extended_by(Dim::exact(k)),
                        Some(crate::tfm::LigKern::Lig { .. }) => {
                            self.natural = None;
                            return;
                        }
                        None => {}
                    }
                }
            }
            Some(None) => n.along = Dim::ANY,
            None => {}
        }
        let space = self.begin_atom(&mut n, t);
        n.along = n.along.extended_by(space).extended_by(Dim::exact(b.width));
        if t == 1 {
            // tex.web § 749: an operator character in text style is boxed
            // with its italic correction and centred on the axis.
            let axis = self.family_font(by, 2).and_then(|f| self.font_dimen(f, 22)?.as_dimen());
            let Some(axis) = axis else {
                self.natural = None;
                return;
            };
            let half = |x: Scaled| if x % 2 != 0 { (x + 1).div_euclid(2) } else { x / 2 };
            let shift = half(b.height - b.depth) - axis;
            n.along = n.along.extended_by(Dim::exact(b.italic));
            n.height = n.height.max(Dim::exact(b.height - shift));
            n.depth = n.depth.max(Dim::exact(b.depth + shift));
            n.math_prev = None;
            self.natural = Some(n);
            return;
        }
        n.height = n.height.max(Dim::exact(b.height));
        n.depth = n.depth.max(Dim::exact(b.depth));
        n.math_prev = Some(Some((fam, font, ch)));
        self.natural = Some(n);
    }

    /// tex.web §§ 1151-1155: the class, family and position of a math
    /// character, the family of a variable one from `\fam` where that is one.
    fn math_class(&mut self, c: u32) -> Option<(i64, i64, u32)> {
        let code = char::from_u32(c).and_then(|ch| self.char_code_known(crate::builtins::CodeTable::Math, ch));
        let code = code.filter(|&v| (0..0x8000).contains(&v))?;
        let (class, fam, ch) = (code >> 12, (code >> 8) & 0xf, (code & 0xff) as u32);
        if class != 7 {
            return Some((class, fam, ch));
        }
        let sym = self.intern("fam");
        match self.env.value(sym) {
            Value::Int(f) if (0..16).contains(&f) => Some((class, f, ch)),
            Value::Int(_) => Some((class, fam, ch)),
            _ => None,
        }
    }

    /// tex.web § 1176: `^` or `_` after a character in inline math. Its
    /// script is the next character; anything else loses the size.
    pub(crate) fn math_script(&mut self, sup: bool) {
        match self.natural {
            Some(mut n)
                if (matches!(n.math_prev, Some(Some(_)))
                    || (n.math_prev.is_none() && n.math_box.is_some())
                    || n.scripted.is_some_and(|r| r.after == n.along && r.sup != sup))
                    && n.script.is_none() =>
            {
                n.script = Some(sup);
                self.natural = Some(n);
            }
            _ => self.natural = None,
        }
    }

    /// The style of a script in `style` (tex.web § 702): one size down, to
    /// scriptscript at most, and cramped for a subscript.
    fn script_style(style: u8, sup: bool) -> u8 {
        let size = ((style >> 1) + 1).min(2);
        size * 2 + if sup { style & 1 } else { 1 }
    }

    /// The name of the font table (`\textfont` …) of math size `size`.
    pub(crate) fn size_font(&mut self, size: u8) -> Sym {
        self.intern(["textfont", "scriptfont", "scriptscriptfont"][usize::from(size.min(2))])
    }

    /// tex.web §§ 755-758: an ordinary character as the one script of a
    /// nucleus, set in the font of the size below it.
    fn math_script_char(&mut self, n: Natural, sup: bool, class: i64, fam: i64, c: u32) {
        let by = self.size_font(((n.style >> 1) + 1).min(2));
        let font = self.family_font(by, fam).filter(|_| matches!(class, 0 | 7));
        let script = font.and_then(|f| self.font_metrics(f)?.char_box(c, self.font_size(f)?));
        let Some(b) = script else {
            self.natural = None;
            return;
        };
        let (w, h, d) = (b.width + b.italic, b.height.max(0), b.depth.max(0));
        self.place_script(n, sup, w, h, d);
    }

    /// The end of a `{…}` script: the list it held, boxed.
    fn close_script_group(&mut self, outer: Natural) {
        let Some(mut n) = self.natural.filter(|n| n.script.is_none()) else {
            self.natural = None;
            return;
        };
        let italic = self.prev_italic(n.math_prev);
        Self::end_atoms(&mut n);
        let size = n.along.extended_by(italic).known().zip(n.height.known()).zip(n.depth.known());
        match (size, outer.script) {
            (Some(((w, h), d)), Some(sup)) => self.place_script(outer, sup, w, h, d),
            _ => self.natural = None,
        }
    }

    /// tex.web §§ 756-759: a script box of this size set after the nucleus
    /// of `n`, a character or a box, with its italic correction (but for a
    /// subscript alone) and `\scriptspace` (§ 720), shifted by the
    /// parameters of the math fonts of the nucleus' size. A script of the
    /// other kind after one sets both in a column.
    fn place_script(&mut self, mut n: Natural, sup: bool, w: Scaled, height: Scaled, depth: Scaled) {
        let mut size = [[w, height, depth]; 2];
        let mut flip = None;
        if let Some(r) = n.scripted.filter(|r| r.after == n.along && r.sup != sup) {
            (n.along, n.height, n.depth) = (r.before[0], r.before[1], r.before[2]);
            (n.math_prev, n.math_box) = (r.prev, r.boxed);
            size[usize::from(r.sup)] = r.size;
            flip = Some(r.sup);
        }
        let record = Scripted {
            sup,
            size: size[usize::from(sup)],
            before: [n.along, n.height, n.depth],
            after: n.along,
            prev: n.math_prev,
            boxed: n.math_box,
        };
        // A character nucleus, or a box one sitting the script by the
        // script font's drops (tex.web § 756).
        let char_nucleus = match (n.math_prev, n.math_box) {
            (Some(Some((_, nucleus, left))), _) => Some((nucleus, left)),
            (None, Some(_)) => None,
            _ => {
                self.natural = None;
                return;
            }
        };
        let (here, below) = (n.style >> 1, ((n.style >> 1) + 1).min(2));
        let (here, below) = (self.size_font(here), self.size_font(below));
        let drops = match n.math_box {
            Some((bh, bd)) if char_nucleus.is_none() => {
                let ss = self.family_font(below, 2);
                let mut sparam = |k| ss.and_then(|f| self.font_dimen(f, k)).and_then(|v| v.as_dimen());
                let (Some(sup_drop), Some(sub_drop)) = (sparam(18), sparam(19)) else {
                    self.natural = None;
                    return;
                };
                (bh - sup_drop, bd + sub_drop)
            }
            _ => (0, 0),
        };
        let (sy, ex) = (self.family_font(here, 2), self.family_font(here, 3));
        let mut param = |k| sy.and_then(|f| self.font_dimen(f, k)).and_then(|v| v.as_dimen());
        // The shift up is `sup2`, `sup3` where cramped; the shift down
        // `sub1`, `sub2` beside a superscript (§§ 757-759).
        let both = flip.is_some();
        let (up, down) = (param(14 + usize::from(n.style & 1)), param(16 + usize::from(both)));
        let (Some(x_height), Some(up), Some(down)) = (param(5), up, down) else {
            self.natural = None;
            return;
        };
        let (sup_box, sub_box) = (size[1], size[0]);
        let delta = char_nucleus.map_or(Dim::exact(0), |(nucleus, left)| self.math_italic(nucleus, left, false));
        let ss = self.dimen_parameter("scriptspace");
        let mut shift_up = drops.0.max(up).max(sup_box[2] + x_height.abs() / 4);
        let mut shift_down = drops.1.max(down);
        let mut cols = Dim::exact(w);
        if both {
            let Some(thick) = ex.and_then(|f| self.font_dimen(f, 8)?.as_dimen()) else {
                self.natural = None;
                return;
            };
            let gap = (shift_up - sup_box[2]) - (sub_box[1] - shift_down);
            if gap < 4 * thick {
                shift_down = 4 * thick - (shift_up - sup_box[2]) + sub_box[1];
                let psi = (x_height * 4).abs() / 5 - (shift_up - sup_box[2]);
                if psi > 0 {
                    shift_up += psi;
                    shift_down -= psi;
                }
            }
            cols = Dim::exact(sub_box[0]).max(delta.extended_by(Dim::exact(sup_box[0])));
        } else {
            shift_down = shift_down.max(height - (x_height * 4).abs() / 5);
        }
        let (h, d) = match (both, sup) {
            (true, _) => (sup_box[1] + shift_up, sub_box[2] + shift_down),
            (_, true) => (height + shift_up, depth - shift_up),
            _ => (height - shift_down, depth + shift_down),
        };
        if sup && !both {
            n.along = n.along.extended_by(delta);
        }
        n.along = n.along.extended_by(cols).extended_by(ss);
        n.height = n.height.max(Dim::exact(h));
        n.depth = n.depth.max(Dim::exact(d));
        (n.math_prev, n.script, n.math_box) = (None, None, None);
        n.scripted = (!both).then_some(Scripted { after: n.along, ..record });
        self.natural = Some(n);
    }

    /// A character whose font satex has no metrics for, or which the font
    /// lacks: its box widens the list, but it is still a character node
    /// for `\lastskip` and it still sets the space factor (tex.web § 1034).
    fn unmetered_char(&mut self, c: u32) {
        self.end_word();
        let Some(mut n) = self.natural else { return };
        let sf = n.space_factor;
        n.widen(true);
        n.space_factor = self.space_factor_after(c, sf);
        n.pushed(Last::Char(None));
        self.natural = Some(n);
    }

    /// tex.web § 1034 `adjust_space_factor`.
    fn space_factor_after(&mut self, c: u32, sf: Option<i64>) -> Option<i64> {
        // A `\sfcode` satex could not follow leaves the space factor unknown.
        let code = self.char_code_known(crate::builtins::CodeTable::Space, char::from_u32(c).unwrap_or('\0'))?;
        match (code, sf) {
            (1000, _) => Some(1000),
            (0, sf) => sf,
            (code, _) if code < 1000 => Some(code),
            (_, Some(sf)) if sf < 1000 => Some(1000),
            (code, Some(_)) => Some(code),
            (_, None) => None,
        }
    }

    /// A dimension parameter as an interval; unset it is zero (tex.web
    /// § 240: every parameter starts at zero).
    pub(crate) fn dimen_parameter(&mut self, name: &str) -> Dim {
        let sym = self.intern(name);
        match self.env.value(sym) {
            Value::Dimen(d) => Dim::exact(d),
            Value::Num { quantity: crate::value::Quantity::Dimen, num } => Dim::range(num.lo, num.hi),
            Value::Unknown if self.env.get(sym).is_none() => Dim::exact(0),
            _ => Dim::ANY,
        }
    }

    /// tex.web § 1113 `append_italic_correction`: a kern of the last
    /// character's italic correction, and nothing where the tail is not a
    /// character or ligature.
    fn italic_correction(&mut self) {
        crate::observe::close_overhang(self);
        self.material = true;
        self.end_word();
        let Some(n) = self.natural else { return };
        match n.tail[0] {
            Last::Char(Some((font, c))) => {
                let bx = self.font_size(font).zip(self.font_metrics(font)).and_then(|(z, m)| m.char_box(c, z));
                self.append_item(Item::Kern(bx.map(|b| b.italic)));
            }
            // A character whose italic correction is not known, or a tail
            // that may be one: the kern it adds is not known either.
            Last::Char(None) | Last::Unknown => self.append_item(Item::Kern(None)),
            // The tail is no character, so `\/` appends nothing.
            _ => {}
        }
    }

    /// The word ends: its last character is settled and meets the font's
    /// right boundary character (tex.web § 1038 `wrapup`), unless a
    /// `\noboundary` came.
    pub(crate) fn end_word(&mut self) {
        let Some((font, c)) = self.natural.and_then(|n| n.pending) else { return };
        self.settle_char();
        let Some(mut n) = self.natural else { return };
        if std::mem::take(&mut n.no_boundary) {
            self.natural = Some(n);
            return;
        }
        let size = self.font_size(font);
        let step = self
            .font_metrics(font)
            .zip(size)
            .and_then(|(m, z)| Some((m.boundary_char()?, m, z)))
            .and_then(|(b, m, z)| m.lig_kern(c, b, z));
        match step {
            Some(crate::tfm::LigKern::Kern(k)) => {
                n.along = n.along.extended_by(Dim::exact(k));
                n.pushed(Last::Kern(k));
                self.kp_push(&mut n.para.items, crate::linebreak::Kp::Kern { w: k, explicit: false });
            }
            // A ligature with the boundary changes the word's last node in
            // a way not followed: its size and tail are not known.
            Some(crate::tfm::LigKern::Lig { .. }) => {
                let sf = n.space_factor;
                n.para.items = None;
                n.widen(true);
                n.space_factor = sf;
                n.tail = [Last::Unknown; TAIL];
                n.ligature = None;
            }
            None => {}
        }
        self.natural = Some(n);
    }

    /// The character not yet counted goes onto the list: its box is added
    /// and `\/` can read its italic correction.
    fn settle_char(&mut self) {
        let Some(mut n) = self.natural else { return };
        let Some((font, c)) = n.pending.take() else { return };
        let size = self.font_size(font);
        match self.font_metrics(font).zip(size).and_then(|(m, z)| m.char_box(c, z)) {
            Some(b) => {
                n.character([b.width, b.height, b.depth].map(Dim::exact));
                let lig = n.ligature != Some(false);
                self.kp_push(
                    &mut n.para.items,
                    crate::linebreak::Kp::Box { w: b.width, h: b.height, d: b.depth, chr: Some((font, c)), lig },
                );
                // tex.web § 1039: an empty discretionary follows `\hyphenchar`.
                match self.font_field(font, "hyphenchar") {
                    Value::Int(h) if h == i64::from(c) => {
                        self.kp_push(&mut n.para.items, crate::linebreak::Kp::Disc(None, 0))
                    }
                    Value::Int(_) => {}
                    _ => n.para.items = None,
                }
                if n.tail[0] == Last::Char(None) {
                    n.tail[0] = Last::Char(Some((font, c)));
                }
            }
            None => {
                n.character([Dim::ANY; 3]);
                n.para.items = None;
            }
        }
        self.natural = Some(n);
    }

    /// `\unskip`, `\unkern` or `\unpenalty` (tex.web § 1105): each takes
    /// away a last item of its own kind. Which kind is not told apart here.
    /// Before primitive `p`: where it may put items on the main vertical
    /// list other than through [`Machine::append_item`], or take them away
    /// (`\unskip` there may find the page builder took the item, tex.web
    /// § 1105), its last item is no longer known.
    pub(crate) fn forget_main_tail(&mut self, p: crate::builtins::Primitive) {
        use crate::builtins::Primitive as P;
        if !self.mode.meets(Modes::VERTICAL) || self.main_tail == Last::Unknown {
            return;
        }
        use crate::tex::RegKind as K;
        // Assignments, groups and messages put nothing on a list.
        let quiet = matches!(
            p,
            P::Mode(ModeCmd::Par)
                | P::Def { .. }
                | P::Let { .. }
                | P::Prefix(_)
                | P::BeginGroup
                | P::EndGroup
                | P::Defer(_)
                | P::Allocate(_)
                | P::Register(K::Count | K::Dimen | K::Skip | K::MuSkip | K::Toks)
                | P::RegisterDef(_)
                | P::FontDef
                | P::Arith(_)
                | P::IntegerParameter
                | P::DimenParameter
                | P::GlueParameter { .. }
                | P::TokensParameter
                | P::CatcodeAssign
                | P::CharCode(_)
                | P::Message { .. }
                | P::Relax
                | P::LastItem(_)
                | P::FontParam(_)
                | P::FontIdent(_)
                | P::IgnoreSpaces
        );
        if !quiet {
            self.main_tail = Last::Unknown;
        }
    }

    pub(crate) fn remove_item(&mut self, removed: Removed) {
        self.end_word();
        let Some(mut n) = self.natural else { return };
        // tex.web § 1105 in inline math: an item of that kind last in the
        // math list goes, which loses the list's size here; any other tail
        // stays.
        if self.mode == Modes::MATH {
            let goes = matches!(
                (removed, n.tail[0]),
                (_, Last::Unknown)
                    | (Removed::Glue, Last::Glue(..))
                    | (Removed::Kern, Last::Kern(_))
                    | (Removed::Penalty, Last::Penalty(_))
            );
            if goes {
                self.natural = None;
            }
            return;
        }
        let horizontal = self.list_kind();
        self.kp_pop(&mut n.para.items, removed, n.tail[0]);
        let taken = match (removed, n.tail[0]) {
            // What the tail is is not known, so neither is what went. The
            // size widens rather than staying as it was.
            (_, Last::Unknown) => {
                match horizontal {
                    // tex.web § 1105: only glue, a kern or a penalty goes,
                    // so a horizontal list keeps its height and depth.
                    Some(true) => {
                        n.along = Dim::ANY;
                        n.para.breakpoints = u32::MAX;
                        n.para.awkward = true;
                        n.para.lost |= removed == Removed::Penalty;
                    }
                    Some(h) => {
                        n.widen(h);
                        // A forced break may be what went.
                        n.para.lost |= removed == Removed::Penalty;
                    }
                    None => {
                        self.natural = None;
                        return;
                    }
                }
                n.tail = [Last::Unknown; TAIL];
                self.natural = Some(n);
                return;
            }
            (Removed::Glue, Last::Glue(w, _)) | (Removed::Kern, Last::Kern(w)) => w,
            (Removed::Penalty, Last::Penalty(p)) => {
                // The paragraph's lines were set as if it broke there.
                n.para.lost |= p <= -10000;
                0
            }
            _ => return,
        };
        match horizontal {
            // Discarded after a forced break, so never counted.
            Some(true) if n.para.fresh => {}
            // tex.web § 668: the depth the item before it left is not kept,
            // so a vertical list only widens.
            Some(true) => n.along = n.along.reduced_by(Dim::exact(taken)),
            // A penalty has no size on a vertical list (tex.web § 668).
            Some(false) if removed == Removed::Penalty => {}
            // The glue or kern appended last: the list as it was before.
            Some(false)
                if n.undo.is_some_and(|(item, [_, after])| {
                    item == n.tail[0] && item != Last::Unknown && after == (n.along, n.depth, n.first)
                }) =>
            {
                let (_, [before, _]) = n.undo.expect("checked");
                (n.along, n.depth, n.first) = before;
            }
            _ => {
                n.along = Dim::ANY;
                n.depth = Dim::ANY;
            }
        }
        if horizontal == Some(true) {
            n.drop_break();
        }
        n.popped();
        // tex.web § 1096: what is left may be a null paragraph again.
        if horizontal == Some(true) && matches!(n.tail[0], Last::Empty | Last::Unknown) {
            n.prior = true;
        }
        self.natural = Some(n);
    }

    /// `\lastskip`, `\lastkern` or `\lastpenalty` of a list whose last item
    /// is known: zero unless it is of that kind (tex.web § 424).
    pub(crate) fn last_value(&mut self, removed: Removed) -> Option<Value> {
        // The main vertical list: its last item, whether the page builder
        // took it or not. A penalty may have been the page break, which
        // makes it 10000 (tex.web § 1013), so `\lastpenalty` stays unknown.
        if self.natural.is_none() && self.mode == Modes::VERTICAL {
            return match (removed, self.main_tail) {
                (_, Last::Unknown) | (Removed::Penalty, Last::Penalty(_)) => None,
                (Removed::Glue, Last::Glue(_, spec)) => spec.map(Value::Glue),
                (Removed::Glue, _) => Some(Value::Glue(crate::value::Glue::default())),
                (Removed::Kern, Last::Kern(k)) => Some(Value::Dimen(k)),
                (Removed::Kern, _) => Some(Value::Dimen(0)),
                (Removed::Penalty, _) => Some(Value::Int(0)),
            };
        }
        let n = self.natural?;
        // tex.web §§ 1034-1036: the word being set is on the list already
        // and goes on after the expansion that asks, kerning with its next
        // character.
        let tail = if n.pending.is_some() { Last::Char(None) } else { n.tail[0] };
        match (removed, tail) {
            (_, Last::Unknown) => None,
            (Removed::Glue, Last::Glue(_, spec)) => spec.map(Value::Glue),
            (Removed::Glue, _) => Some(Value::Glue(crate::value::Glue::default())),
            (Removed::Kern, Last::Kern(k)) => Some(Value::Dimen(k)),
            (Removed::Kern, _) => Some(Value::Dimen(0)),
            (Removed::Penalty, Last::Penalty(p)) => Some(Value::Int(p)),
            (Removed::Penalty, _) => Some(Value::Int(0)),
        }
    }
}

impl Machine<'_> {
    /// Unknown digits (see [`Machine::unknown_digits`]).
    pub(crate) fn unknown_digits_token(&mut self, span: Span) -> Token {
        let meaning = Meaning::Char(crate::tex::UNKNOWN_CHAR, crate::tex::Catcode::Other);
        if self.env.meaning(self.unknown_digits) != meaning {
            self.env.set(self.unknown_digits, crate::env::Binding::builtin(meaning), true);
        }
        Token::new(Tok::Cs(self.unknown_digits), span)
    }

    /// Whether `sym` is one of the markers for unknown text.
    pub(crate) fn is_unknown_marker(&self, sym: Sym) -> bool {
        sym == self.unknown || self.is_unknown_digits(sym) || self.is_unknown_name(sym)
    }

    /// How many tokens text with unknown parts may stand for: unknown text
    /// may be empty or anything, unknown digits are at least one character.
    /// `None` as the upper end means no bound.
    pub(crate) fn text_length(&self, tokens: &[Token]) -> (usize, Option<usize>) {
        let mut min = 0;
        let mut bounded = true;
        for t in tokens {
            match t.tok {
                Tok::Cs(sym) if self.is_unknown_digits(sym) && !self.is_more_digits(sym) => {
                    min += 1;
                    bounded = false;
                }
                Tok::Cs(sym) if self.is_unknown_marker(sym) => bounded = false,
                _ => min += 1,
            }
        }
        (min, bounded.then_some(min))
    }

    pub(crate) fn has_unknown(&self, tokens: &[Token]) -> bool {
        tokens.iter().any(|t| match t.tok {
            Tok::Cs(sym) => self.is_unknown_marker(sym),
            _ => false,
        })
    }
}

/// What a box register holds, as far as the run knows (tex.web § 230).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoxState {
    Void,
    HBox,
    VBox,
    Unknown,
}

impl Machine<'_> {
    fn box_sym(&mut self, n: i64) -> Sym {
        self.register_sym(crate::tex::RegKind::Box, n)
    }

    /// The box register `n`: void until something is put in it.
    pub(crate) fn box_state(&mut self, n: i64) -> BoxState {
        let sym = self.box_sym(n);
        let value = self.env.get(sym).map(|b| b.value.clone());
        match value {
            // A `\setbox` through a number satex could not pin down may
            // have reached a register it has not seen written.
            None if self.any_register_written(crate::tex::RegKind::Box, Some(n)) => BoxState::Unknown,
            None => BoxState::Void,
            Some(Value::Int(0)) => BoxState::Void,
            Some(Value::Int(1)) => BoxState::HBox,
            Some(Value::Int(2)) => BoxState::VBox,
            Some(_) => BoxState::Unknown,
        }
    }

    /// A write through a box number satex could not pin down: every box
    /// register may hold what was set, and one never set is no longer
    /// known to be void (tex.web §§ 1077, 1079).
    fn weaken_boxes(&mut self, global: bool) {
        self.weaken_registers(crate::tex::RegKind::Box, None, Value::Unknown, global);
        self.weaken_box_dimens(None, global);
    }

    /// A write through a box register number known to lie in one of two
    /// intervals
    /// (tex.web § 1241): only those registers and their sizes may change.
    fn weaken_boxes_within(&mut self, within: [(i64, i64); 2], global: bool) {
        let range = within.map(|(lo, hi)| crate::value::Num::range(lo, hi));
        self.weaken_registers(crate::tex::RegKind::Box, Some(&range), Value::Unknown, global);
        let targets: Vec<Sym> = self
            .env
            .bound()
            .into_iter()
            .filter(|&s| {
                let name = self.name(s);
                name.strip_prefix("\u{4}box.")
                    .and_then(|rest| rest.rsplit_once('.'))
                    .and_then(|(_, n)| n.parse::<i64>().ok())
                    .is_some_and(|n| within.iter().any(|&(lo, hi)| lo <= n && n <= hi))
            })
            .collect();
        for sym in targets {
            self.env.set_value(sym, Value::Unknown, global);
        }
    }

    /// The same for `\wd`, `\ht` and `\dp`: `which` names one of them, or
    /// `None` all three.  A void box keeps its zeroes, which is what
    /// `alter_box_dimen` leaves them (tex.web § 1247).
    fn weaken_box_dimens(&mut self, which: Option<&str>, global: bool) {
        let prefix = match which {
            Some(which) => format!("\u{4}box.{which}."),
            None => "\u{4}box.".to_string(),
        };
        let targets: Vec<Sym> = self.env.bound().into_iter().filter(|&s| self.name(s).starts_with(&prefix)).collect();
        for sym in targets {
            self.env.set_value(sym, Value::Unknown, global);
        }
    }

    /// `\box`, `\copy`, `\unhbox` and relatives take the register's box.
    /// `take` leaves the register void, without a save-stack entry
    /// (tex.web § 1079 `box(cur_val):=null`).
    pub(crate) fn fetch_box(&mut self, n: Option<i64>, take: bool) -> BoxState {
        let Some(n) = n else {
            // A number satex could not pin down names any register, and
            // taking its box leaves that one void.
            if take {
                self.weaken_boxes(true);
            }
            return BoxState::Unknown;
        };
        let state = self.box_state(n);
        self.fetched = if take {
            self.box_chars.remove(&n).unwrap_or_default()
        } else {
            self.box_chars.get(&n).cloned().unwrap_or_default()
        };
        if take {
            self.box_changed(Some(n), BoxState::Void);
        }
        state
    }

    /// Box register `register` now holds text set in these character ranges.
    fn set_box_chars(&mut self, register: Option<i64>, ranges: Vec<(u32, u32)>) {
        if let Some(n) = register {
            self.box_chars.insert(n, ranges);
        }
    }

    /// A box's text reached the list: the page when no box encloses the
    /// list, else it travels with the enclosing box.
    fn place_chars(&mut self, ranges: Vec<(u32, u32)>, inside: bool) {
        if inside {
            self.carried_chars.extend(ranges);
        } else {
            let mut all = std::mem::take(&mut self.carried_chars);
            all.extend(ranges);
            self.out.facts.text.placed.extend(all.into_iter().filter(|r| r.0 < r.1));
        }
    }

    /// A register changed in place, as `\box` and `\vsplit` change it.
    fn box_changed(&mut self, n: Option<i64>, state: BoxState) {
        // tex.web § 1079 saves nothing, so the change outlives its group.
        let Some(n) = n else { return self.weaken_boxes(true) };
        if self.box_state(n) == state {
            return;
        }
        let sym = self.box_sym(n);
        self.env.set_value_in_place(sym, box_value(state));
        self.reset_box_dimens(n);
    }

    /// `\setbox`: a register assignment (tex.web § 1077).
    pub(crate) fn set_box(
        &mut self,
        register: Option<i64>,
        state: BoxState,
        global: bool,
        within: Option<[(i64, i64); 2]>,
    ) {
        let Some(n) = register else {
            return match within {
                Some(range) => self.weaken_boxes_within(range, global),
                None => self.weaken_boxes(global),
            };
        };
        let sym = self.box_sym(n);
        self.env.set_value(sym, box_value(state), global);
        for dimen in ["wd", "ht", "dp", "first", "solid", "last", "one", "lw", "lh", "ld", "lmk"] {
            let key = self.intern(&format!("\u{4}box.{dimen}.{n}"));
            self.env.set_value(key, Value::Unknown, global);
        }
    }

    fn set_box_first(&mut self, register: Option<i64>, first: Dim) {
        self.set_box_slot(register, "first", first.value());
    }

    /// What register `register`'s vbox holds: nothing (`None`), or one
    /// line, an hbox (tex.web § 889).
    fn set_box_line(&mut self, register: Option<i64>, line: Option<Line>) {
        self.set_box_slot(register, "one", Value::Int(i64::from(line.is_some())));
        if let Some((w, h, d, kern)) = line {
            self.set_box_slot(register, "lw", Value::Dimen(w));
            self.set_box_slot(register, "lh", Value::Dimen(h));
            self.set_box_slot(register, "ld", Value::Dimen(d));
            self.set_box_slot(register, "lmk", kern.map_or(Value::Unknown, Value::Dimen));
        }
    }

    /// What register `n` holds where recorded: a vbox of nothing
    /// (`Some(None)`) or of one line, or an hbox that is a line.
    pub(crate) fn box_line(&mut self, n: Option<i64>) -> Option<Option<Line>> {
        let n = n?;
        let slot = |m: &mut Self, which: &str| {
            let key = m.intern(&format!("\u{4}box.{which}.{n}"));
            m.env.value(key)
        };
        match slot(self, "one") {
            Value::Int(0) => Some(None),
            Value::Int(1) => {
                let [w, h, d] = ["lw", "lh", "ld"].map(|k| slot(self, k).as_dimen());
                let kern = slot(self, "lmk").as_dimen();
                Some(Some((w?, h?, d?, kern)))
            }
            _ => None,
        }
    }

    fn set_box_slot(&mut self, register: Option<i64>, which: &str, value: Value) {
        let Some(n) = register else { return };
        let key = self.intern(&format!("\u{4}box.{which}.{n}"));
        self.env.set_value_in_place(key, value);
    }

    /// An hbox's last item, which `\unhbox` uncovers for `\unskip` and
    /// relatives (tex.web §§ 1105, 1110). Glue, a kern, a penalty, or
    /// neither (empty toks); joins of different ones read as unknown.
    fn set_box_last(&mut self, register: Option<i64>, last: Last) {
        let Some(n) = register else { return };
        let value = match last {
            Last::Glue(_, Some(g)) => Value::Glue(g),
            Last::Kern(k) => Value::Dimen(k),
            Last::Penalty(p) => Value::Int(p),
            Last::Other | Last::Char(_) | Last::Box | Last::Whatsit => Value::Toks(Rc::from([])),
            Last::Glue(_, None) | Last::Empty | Last::Unknown => return,
        };
        let key = self.intern(&format!("\u{4}box.last.{n}"));
        self.env.set_value_in_place(key, value);
    }

    fn box_last(&mut self, n: Option<i64>) -> Last {
        let Some(n) = n else { return Last::Unknown };
        let key = self.intern(&format!("\u{4}box.last.{n}"));
        match self.env.value(key) {
            Value::Glue(g) => Last::Glue(g.width, Some(g)),
            Value::Dimen(k) => Last::Kern(k),
            Value::Int(p) => Last::Penalty(p),
            Value::Toks(t) if t.is_empty() => Last::Char(None),
            _ => Last::Unknown,
        }
    }

    /// A box dimension as stored: `Env::value` reads an interval as unknown.
    fn box_slot(&mut self, key: Sym) -> Value {
        match self.env.num(key) {
            Some((quantity @ crate::value::Quantity::Dimen, num)) => Value::from_num(quantity, num),
            _ => self.env.value(key),
        }
    }

    /// The height of the first box in register `n`'s vertical list, or
    /// the width along its horizontal list, where that is recorded.
    fn box_first(&mut self, n: Option<i64>) -> Option<Dim> {
        let n = n?;
        let key = self.intern(&format!("\u{4}box.first.{n}"));
        match self.box_slot(key) {
            Value::Dimen(d) => Some(Dim::exact(d)),
            Value::Num { quantity: crate::value::Quantity::Dimen, num } => Some(Dim::range(num.lo, num.hi)),
            _ => None,
        }
    }

    /// The width, height and depth of box register `n`, each within what
    /// is known of it.
    pub(crate) fn box_dims(&mut self, n: Option<i64>) -> [Dim; 3] {
        let Some(n) = n else { return [Dim::ANY; 3] };
        // tex.web § 420: a void box reads as zero in all three.
        if self.box_state(n) == BoxState::Void {
            return [Dim::exact(0); 3];
        }
        ["wd", "ht", "dp"].map(|which| {
            let key = self.intern(&format!("\u{4}box.{which}.{n}"));
            match self.box_slot(key) {
                Value::Dimen(d) => Dim::exact(d),
                Value::Num { quantity: crate::value::Quantity::Dimen, num } => Dim::range(num.lo, num.hi),
                _ => Dim::ANY,
            }
        })
    }

    /// What `hpack` and `vpack` found, kept as the interval it is: a
    /// length satex could not follow is never written down as zero.
    fn set_box_dims(&mut self, register: Option<i64>, dims: [Dim; 3]) {
        let Some(n) = register else { return self.weaken_box_dimens(None, true) };
        if self.box_state(n) == BoxState::Void {
            return;
        }
        for (which, dim) in ["wd", "ht", "dp"].into_iter().zip(dims) {
            let key = self.intern(&format!("\u{4}box.{which}.{n}"));
            self.env.set_value_in_place(key, dim.value());
        }
    }

    fn reset_box_dimens(&mut self, n: i64) {
        for dimen in ["wd", "ht", "dp", "first", "solid", "last", "one", "lw", "lh", "ld", "lmk"] {
            let key = self.intern(&format!("\u{4}box.{dimen}.{n}"));
            if self.env.get(key).is_some() {
                self.env.set_value_in_place(key, Value::Unknown);
            }
        }
    }

    /// `\wd`, `\ht`, `\dp` (tex.web § 420): zero for a void box; what an
    /// assignment gave it, or unknown, otherwise.
    pub(crate) fn box_dimen(&mut self, which: &str, n: Option<i64>) -> Option<Value> {
        let n = n?;
        match self.box_state(n) {
            BoxState::Void => Some(Value::Dimen(0)),
            _ => {
                let key = self.intern(&format!("\u{4}box.{which}.{n}"));
                match self.box_slot(key) {
                    // tex.web §§ 649, 668: `hpack` starts height and depth
                    // at zero and takes the maximum, `vpack` the width, so
                    // the bound `set_box_dims` kept is what the test gets.
                    Value::Num { quantity: crate::value::Quantity::Dimen, num } => {
                        self.abs = Some(num);
                        self.abs_exact = None;
                        self.abs_from = None;
                        None
                    }
                    Value::Unknown => {
                        self.abs = None;
                        self.abs_exact = None;
                        self.abs_from = None;
                        None
                    }
                    value => Some(value),
                }
            }
        }
    }

    /// `\wd⟨n⟩=⟨dimen⟩` (tex.web § 1247): a void box is left alone, and the
    /// change is made in place, as `\box` makes its own.
    pub(crate) fn set_box_dimen(&mut self, which: &str, n: Option<i64>, value: Option<i64>) {
        let Some(n) = n else { return self.weaken_box_dimens(Some(which), true) };
        match self.box_state(n) {
            BoxState::Void => {}
            state => {
                let key = self.intern(&format!("\u{4}box.{which}.{n}"));
                let value = if state == BoxState::Unknown {
                    Value::Unknown
                } else {
                    value.map_or(Value::Unknown, Value::Dimen)
                };
                self.env.set_value_in_place(key, value);
            }
        }
    }
}

fn box_value(state: BoxState) -> Value {
    match state {
        BoxState::Void => Value::Int(0),
        BoxState::HBox => Value::Int(1),
        BoxState::VBox => Value::Int(2),
        BoxState::Unknown => Value::Unknown,
    }
}

/// tex.web § 108 `badness`: about 100(t/s)³, 10000 when s ≤ 0 or past it.
fn badness(t: Scaled, s: Scaled) -> i64 {
    const INF_BAD: i64 = 10000;
    if t == 0 {
        return 0;
    }
    if s <= 0 {
        return INF_BAD;
    }
    let r = if t <= 7_230_584 {
        t * 297 / s
    } else if s >= 1_663_497 {
        t / (s / 297)
    } else {
        t
    };
    if r > 1290 { INF_BAD } else { (r * r * r + 0o400000) / 0o1000000 }
}

/// [`Nest::part`] of a math group that is a script.
const SCRIPT_GROUP: u8 = 0xff;
