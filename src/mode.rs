//! TeX's modes (tex.web §§ 211-213): what `\ifvmode`, `\ifhmode`,
//! `\ifmmode` and `\ifinner` ask.  The run keeps the set of modes it may be
//! in. Paths that disagree join to their union, and a test is decided only
//! when every mode in the set answers alike.  The semantic nest itself
//! lives in the save stack: a group that begins a list of its own (a box,
//! math, an insertion) saves the modes around it (tex.web § 216
//! `push_nest`) and its end gives them back (`pop_nest`).

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
    pub target: Option<(Option<i64>, bool)>,
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
}

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
        }
    }
}

/// The modes around an undecided condition whose arms run one after the
/// other (see `Machine::undecided_conditional`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ArmModes {
    /// Where every arm starts.
    pub entry: (Modes, Modes),
    /// Where the arms already run ended.
    pub seen: (Modes, Modes),
    /// Whether an `\else` was met, so that some arm is always taken.
    pub other: bool,
    /// The natural size of the list where every arm starts, and the join
    /// of those the arms already run left.
    pub size: Option<Natural>,
    pub sizes: Option<Option<Natural>>,
}

/// A length satex knows only within an interval.  `hpack` (tex.web § 649)
/// and `vpack` (§ 668) add the list's items up. An item satex cannot
/// follow widens the interval instead of being left out, which would
/// under-approximate the size.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
    /// large", whose outcome is not modelled: any length then.
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

    /// The value a document reads back: a number only where there is one,
    /// and never zero for a length satex could not follow.
    pub fn value(self) -> Value {
        match self.known() {
            Some(v) => Value::Dimen(v),
            None if self.is_any() => Value::Unknown,
            None => Value::from_num(true, crate::value::Num::range(self.lo, self.hi)),
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
    /// The height of the first box or rule of a vertical list, which
    /// `\vtop` takes for its height (tex.web § 1087); zero once anything
    /// else came first, `None` while the list is empty.
    pub first: Option<Dim>,
    /// Whether a vertical list already holds a box or rule, so that the
    /// next one takes interline glue (tex.web § 679 `prev_depth`).
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
            pending: None,
            space_factor: None,
        }
    }

    /// An item went onto the list.
    fn pushed(&mut self, item: Last) {
        for k in (1..TAIL).rev() {
            self.tail[k] = self.tail[k - 1];
        }
        self.tail[0] = item;
    }

    /// An item came off the list. What it uncovers past the window is not
    /// followed.
    fn popped(&mut self) {
        for k in 1..TAIL {
            self.tail[k - 1] = self.tail[k];
        }
        self.tail[TAIL - 1] = Last::Unknown;
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
    }

    /// tex.web § 651, § 669: glue and kerns add their size along the list.
    /// A vertical list first settles the depth the last box left.
    fn advance(&mut self, horizontal: bool, by: Dim) {
        if horizontal {
            self.along = self.along.extended_by(by);
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
    fn boxed(&mut self, horizontal: bool, [w, h, d]: [Option<Dim>; 3]) {
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
        } else {
            let h = h.unwrap_or(Dim::ANY);
            // tex.web § 679 `append_to_vlist`: every box but the first
            // takes interline glue, which satex does not follow, so only
            // the height grows unknown, not the width.
            let glue = if self.prior { Dim::ANY } else { Dim::exact(0) };
            self.along = self.along.extended_by(self.depth).extended_by(glue).extended_by(h);
            self.depth = d.unwrap_or(Dim::ANY);
            if let Some(w) = w {
                self.wide = self.wide.max(w);
            }
            self.first.get_or_insert(h);
            self.prior = true;
        }
    }

    /// A character, box or rule whose size satex does not know: every
    /// length it could have added widens, so the size is never
    /// under-approximated.
    fn widen(&mut self, horizontal: bool) {
        self.boxed(horizontal, [Some(Dim::ANY); 3]);
    }

    /// A character on a horizontal list (tex.web § 651 `char_node`).  The
    /// space factor is the reader's, not a box's.
    fn character(&mut self, box_: [Dim; 3]) {
        let sf = self.space_factor;
        self.boxed(true, box_.map(Some));
        self.space_factor = sf;
    }

    fn join(self, other: Natural) -> Natural {
        let mut joined = Natural {
            along: self.along.join(other.along),
            height: self.height.join(other.height),
            depth: self.depth.join(other.depth),
            wide: self.wide.join(other.wide),
            first: match (self.first, other.first) {
                (None, None) => None,
                (a, b) => Some(a.unwrap_or(Dim::exact(0)).join(b.unwrap_or(Dim::exact(0)))),
            },
            prior: self.prior || other.prior,
            capped: self.capped || other.capped,
            tail: std::array::from_fn(|k| self.tail[k].merge(other.tail[k])),
            pending: self.pending,
            space_factor: if self.space_factor == other.space_factor { self.space_factor } else { None },
        };
        // A character not yet counted differs between the paths: its own
        // size is not known, so the lengths it would add widen.
        if self.pending != other.pending {
            joined.pending = None;
            joined.along = Dim::ANY;
            joined.height = joined.height.max(Dim::ANY);
            joined.depth = joined.depth.max(Dim::ANY);
            joined.wide = joined.wide.max(Dim::ANY);
        }
        joined
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
    Unknown,
}

impl Last {
    /// The tail where two paths, or an item that may not have been
    /// appended, leave different ones.
    fn merge(self, other: Last) -> Last {
        match (self, other) {
            _ if self == other => self,
            (Last::Other | Last::Char(_), Last::Other | Last::Char(_)) => Last::Char(None),
            _ => Last::Unknown,
        }
    }
}

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
    /// `\setbox`: the register, if known, and whether the assignment is
    /// global.
    Set { register: Option<i64>, global: bool },
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
            Some(tokens) if !tokens.is_empty() => self.push_tokens(tokens, None),
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
        let by = match token.cs().or_else(|| self.active_cs(token)) {
            Some(sym) => sym,
            None => self.intern("ifvmode"),
        };
        let parts = [part, rest];
        // Each path begins with `token` given back, which may join a frame
        // of tokens already given back: the paths must read it before they
        // can meet, or they meet at once in the modes they split.
        self.split_moving(by, token.span, move |m, arm| {
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
        self.new_graf(token.span);
        true
    }

    /// tex.web § 1091 `new_graf`.
    fn new_graf(&mut self, span: Span) {
        self.mode = self.mode.map(|m| if m.meets(Modes::ANY_VERTICAL) { Modes::HORIZONTAL } else { m });
        // A new horizontal list, whose tail `\lastskip` reads: empty, or
        // the indent box, which is neither glue nor a kern nor a penalty.
        // Its size is the paragraph's, which line breaking undoes, so only
        // the tail and the space factor are followed.
        self.natural = Some(Natural {
            along: Dim::ANY,
            height: Dim::ANY,
            depth: Dim::ANY,
            wide: Dim::ANY,
            first: Some(Dim::ANY),
            prior: true,
            capped: false,
            tail: [Last::Other; TAIL],
            pending: None,
            space_factor: Some(1000),
        });
        self.insert_every("everypar", span);
    }

    /// Vertical material in horizontal mode (tex.web § 1094
    /// `head_for_vmode`): a `\par` token goes in front of it.  `true` when
    /// `token` was put back.
    pub(crate) fn vertical_material(&mut self, token: Token) -> bool {
        let horizontal = self.mode.and(Modes::HORIZONTAL);
        if horizontal.is_empty() {
            return false;
        }
        let par = self.intern("par");
        let primitive = self.env.meaning(par) == Meaning::Primitive(Primitive::Mode(ModeCmd::Par));
        if horizontal != self.mode {
            if primitive {
                // The paragraph ends where there is one, as `\par` would.
                self.normal_paragraph(false);
                self.end_graf();
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
    fn end_graf(&mut self) {
        let list = self.list.and(Modes::ANY_VERTICAL);
        let to = if list.is_empty() { Modes::VERTICAL } else { list };
        self.mode = self.mode.map(|m| if m == Modes::HORIZONTAL { to } else { m });
        // The lines the paragraph breaks into are not followed, so neither
        // is the vertical list they go onto.
        self.natural = None;
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
                let value = if certain { default } else { value.join(&default, crate::value::MAY_VALUES) };
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
        self.normal_paragraph(acting == self.mode);
        self.end_graf();
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
        let at = self.box_left_brace(span);
        self.open_group(GroupKind::Simple, at);
        self.record(Step::OpenGroup, Sym(0), at);
        self.env.set_nest(Nest {
            mode: self.mode,
            list: self.list,
            code,
            level: true,
            part,
            target: None,
            shift: Dim::exact(0),
            to: Packing::Natural,
            material: self.material,
            natural: self.natural,
            placed: false,
        });
        self.material = false;
        // tex.web § 1083: a box's horizontal list starts at 1000.
        self.natural = Some(Natural { space_factor: Some(1000), ..Natural::default() });
        self.mode = modes;
        self.list = modes;
    }

    /// A `{` of a simple group: in math it begins a math list (tex.web
    /// § 1153 `scan_math`, `push_math(math_group)`).
    pub(crate) fn simple_group_opened(&mut self) {
        let math = self.mode.and(Modes::ANY_MATH);
        if math.is_empty() {
            return;
        }
        let code = if math == self.mode { Some(group::MATH) } else { None };
        self.env.set_nest(Nest {
            mode: self.mode,
            list: self.list,
            code,
            level: false,
            part: 0,
            target: None,
            shift: Dim::exact(0),
            to: Packing::Natural,
            material: self.material,
            natural: self.natural,
            placed: false,
        });
        self.mode = self.mode.map(|m| if m.meets(Modes::ANY_MATH) { Modes::MATH } else { m });
    }

    /// A group ended: the modes it saved come back (tex.web § 1085
    /// `package`, § 1186, § 1100), and a `\discretionary` or `\mathchoice`
    /// goes on to its next part (tex.web §§ 1119, 1174).
    pub(crate) fn group_closed(&mut self, nest: Option<Nest>, span: Span) {
        let Some(nest) = nest else { return };
        if !nest.level {
            self.mode = self.mode.without(Modes::ANY_MATH).union(nest.mode.and(Modes::ANY_MATH));
            return;
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
            BoxState::HBox => [nest.to.size(n.along), n.height, n.depth],
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
                let n = Natural { along, depth, ..n };
                let height = nest.to.size(n.along);
                // tex.web § 1087: a `\vtop` takes its height from its first
                // box, and the rest of the packed box is its depth.
                if nest.code == Some(group::VTOP) {
                    let first = n.first.unwrap_or(Dim::exact(0));
                    [n.wide, first, height.extended_by(n.depth).reduced_by(first)]
                } else {
                    [n.wide, height, n.depth]
                }
            }
        };
        if let Some((register, global)) = nest.target {
            self.set_box(register, kind, global);
            if kind != BoxState::Unknown {
                self.set_box_dims(register, dims);
            }
        } else if nest.placed {
            let dims = self.shifted(dims, nest.shift);
            self.append_item(Item::Box(dims));
        } else if nest.code == Some(group::INSERT) {
            self.append_item(Item::Zero);
        } else if kind == BoxState::Unknown {
            self.natural = None;
        }
        match nest.code {
            Some(group::DISC) if nest.part < 3 => {
                self.open_list(Some(group::DISC), Modes::RESTRICTED_HORIZONTAL, nest.part + 1, span);
            }
            Some(group::MATH_CHOICE) if nest.part < 4 => {
                self.open_list(Some(group::MATH_CHOICE), Modes::MATH, nest.part + 1, span);
            }
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
            self.close_group(GroupKind::Math { display }, span);
            if eqno && let Some(GroupKind::Math { display: true }) = self.env.group_kind() {
                self.close_group(GroupKind::Math { display: true }, span);
            }
            return;
        }
        if self.horizontal_material(token) {
            return;
        }
        self.material = true;
        self.natural = None;
        // tex.web § 1138: `$$` is display math only in unrestricted
        // horizontal mode.
        let display = self.mode.meets(Modes::HORIZONTAL) && self.take_math_shift();
        self.open_group(GroupKind::Math { display }, span);
        self.env.set_nest(Nest {
            mode: self.mode,
            list: self.list,
            code: Some(group::MATH_SHIFT),
            level: true,
            part: 0,
            target: None,
            shift: Dim::exact(0),
            to: Packing::Natural,
            material: self.material,
            natural: None,
            placed: false,
        });
        let inner = if display { Modes::DISPLAY_MATH } else { Modes::MATH };
        self.mode = inner;
        self.list = inner;
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
                self.new_graf(span);
            }
            ModeCmd::Horizontal(material) => {
                if !self.horizontal_material(token) {
                    self.scan_material(material, span);
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
                self.open_list(Some(group::INSERT), Modes::INTERNAL_VERTICAL, 0, span);
                self.normal_paragraph(true);
            }
            // tex.web § 785: the material goes into the alignment's own
            // list, which satex does not tell from its cells.
            ModeCmd::NoAlign => {
                let modes = Modes::INTERNAL_VERTICAL.union(Modes::RESTRICTED_HORIZONTAL);
                self.open_list(Some(group::NO_ALIGN), modes, 0, span);
                self.normal_paragraph(false);
            }
            // tex.web § 1172 `append_choices`.
            ModeCmd::MathChoice => {
                if self.mode.meets(Modes::ANY_MATH) {
                    self.open_list(Some(group::MATH_CHOICE), Modes::MATH, 1, span);
                }
            }
            // tex.web § 1142 `start_eq_no`: only in display math.
            ModeCmd::EqNo => {
                if self.mode == Modes::DISPLAY_MATH
                    && matches!(self.env.group_kind(), Some(GroupKind::Math { display: true }))
                {
                    self.open_group(GroupKind::Math { display: false }, span);
                    self.env.set_nest(Nest {
                        mode: self.mode,
                        list: self.list,
                        code: Some(group::MATH_SHIFT),
                        level: true,
                        part: EQNO,
                        target: None,
                        shift: Dim::exact(0),
                        to: Packing::Natural,
                        material: self.material,
                        natural: None,
                        placed: false,
                    });
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
                // A void register adds nothing. A list taken out of a box is
                // not followed.
                if self.fetch_box(n, take) != BoxState::Void {
                    self.append_item(Item::Unknown);
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
            Material::Fil => self.append_item(Item::Glue(Some(0), None)),
            Material::NoBoundary => {}
            Material::Discretionary => {
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
            Material::Space => {
                let (width, spec) = self.space_glue(true);
                self.append_item(Item::Glue(width, spec));
            }
            Material::Discards | Material::Hyphen => self.append_item(Item::Unknown),
        }
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
                Meaning::Primitive(Primitive::Mode(
                    ModeCmd::Horizontal(Material::Rule) | ModeCmd::Vertical(Material::Rule),
                )) if context == BoxContext::Leaders => {
                    self.scan_rule_spec();
                    return;
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
                    Some(Last::Glue(..) | Last::Kern(_) | Last::Penalty(_) | Last::Char(_)) => BoxState::Void,
                    _ => {
                        self.natural = None;
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
                self.scan_dimen();
                let state = match self.fetch_box(n, false) {
                    BoxState::Void => BoxState::Void,
                    _ => BoxState::Unknown,
                };
                self.box_changed(n, BoxState::Unknown);
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
        let dimen = |m: &mut Self| m.scan_dimen().map_or(Dim::ANY, Dim::exact);
        let to = if self.scan_keyword("to") {
            Packing::To(dimen(self))
        } else if self.scan_keyword("spread") {
            Packing::Spread(dimen(self))
        } else {
            Packing::Natural
        };
        self.open_list(code, modes, 0, span);
        let mut nest = self.env.nest().expect("the box's group is open");
        nest.to = to;
        match context {
            BoxContext::Set { register, global } => nest.target = Some((register, global)),
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
        match context {
            BoxContext::Set { register, global } => {
                self.set_box(register, state, global);
                if state != BoxState::Unknown {
                    self.set_box_dims(register, dims);
                }
            }
            BoxContext::Direct => self.append_box(state, dims),
            BoxContext::Shift(shift) => {
                let dims = self.shifted(dims, shift);
                self.append_box(state, dims);
            }
            BoxContext::Leaders | BoxContext::Ship => {}
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
        match state {
            BoxState::Void => {}
            BoxState::Unknown => self.append_item(Item::Unknown),
            _ => self.append_item(Item::Box(dims)),
        }
    }

    /// Which list is being built: `Some(true)` a horizontal one,
    /// `Some(false)` a vertical one of its own.  `None` for the main
    /// vertical list, whose tail the page builder may take away (tex.web
    /// § 1000), for math, and where the mode is not known.
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
        self.end_word();
        let Some(mut n) = self.natural else { return };
        let Some(horizontal) = self.list_kind() else {
            self.natural = None;
            return;
        };
        // tex.web § 463: a running length is `null_flag`, which neither
        // maximum in § 651 nor § 668 ever takes.
        let rule = |d: Option<Option<Scaled>>| match d {
            None => None,
            Some(Some(v)) => Some(Dim::exact(v)),
            Some(None) => Some(Dim::ANY),
        };
        match item {
            // Nothing, or a whatsit: the tail is the new item or the one
            // that was there, so a last glue, kern or penalty is in doubt.
            Item::Zero => n.perhaps(Last::Other),
            Item::Penalty(p) => n.pushed(p.map_or(Last::Unknown, Last::Penalty)),
            Item::Kern(k) => {
                n.advance(horizontal, k.map_or(Dim::ANY, Dim::exact));
                n.pushed(k.map_or(Last::Unknown, Last::Kern));
            }
            Item::Glue(w, spec) => {
                n.advance(horizontal, w.map_or(Dim::ANY, Dim::exact));
                n.pushed(match w {
                    Some(w) => Last::Glue(w, spec),
                    None => Last::Unknown,
                });
            }
            Item::Rule { width, height, depth } => {
                n.boxed(horizontal, [rule(width), rule(height), rule(depth)]);
                n.pushed(Last::Other);
            }
            Item::Box(dims) => {
                n.boxed(horizontal, dims.map(Some));
                n.pushed(Last::Other);
            }
            Item::Unknown => {
                n.widen(horizontal);
                // Material satex did not follow may have set it.
                n.space_factor = None;
                n.tail = [Last::Unknown; TAIL];
            }
        }
        self.natural = Some(n);
    }

    /// Interword glue (tex.web §§ 1041-1044): `\spaceskip` or the font's
    /// space, stretch and shrink, changed by the space factor unless it
    /// is 1000, and at 2000 or more `\xspaceskip` or that plus the font's
    /// extra space.  A control space takes the first (§ 1045).  Where the
    /// space factor is not known the width is known if both choices agree.
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

    /// What a primitive that may put an item on the current list does to
    /// its natural size before the primitive is obeyed.  Those that scan
    /// their size first (kerns, penalties, glue, rules and boxes) add it
    /// when they have.
    pub(crate) fn natural_before(&mut self, p: Primitive, by: Sym) {
        use crate::builtins::{PdfOp, Syntax, Typeset};
        // tex.web § 1113 `append_italic_correction`: `\/` is a kern, not
        // nothing, so it may not be passed over as a zero-size command.
        if p == Primitive::Command(Syntax::Nothing) && self.name(self.env.identity(by)) == "/" {
            return self.italic_correction();
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
                | ModeCmd::QuitVMode,
            )
            | Primitive::Typeset(Typeset::Number | Typeset::Dimen) => return,
            Primitive::Pdf(PdfOp::Literal | PdfOp::Special | PdfOp::Object | PdfOp::Resources) | Primitive::Write => {
                Item::Zero
            }
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
                Syntax::SetBox => return,
                _ => Item::Zero,
            },
            _ => Item::Unknown,
        };
        self.append_item(item);
    }

    /// A character typeset in the current font (tex.web §§ 1034-1040): it
    /// kerns or forms a `=:` ligature with the character before it in the
    /// same word.  Other ligatures, boundary characters, missing characters
    /// and native fonts are not followed.
    pub(crate) fn append_char(&mut self, c: u32) {
        self.material = true;
        if self.natural.is_none() {
            return;
        }
        if self.list_kind() != Some(true) {
            self.natural = None;
            return;
        }
        let font = self.current_font();
        let known = font.and_then(|f| Some((f, self.font_metrics(f)?, self.font_size(f)?)));
        let Some((font, metrics, size)) = known else { return self.unmetered_char(c) };
        if metrics.has_boundary() || !metrics.has_char(c) {
            return self.unmetered_char(c);
        }
        let mut n = self.natural.expect("checked above");
        let step = match n.pending {
            Some((f, left)) if f == font => metrics.lig_kern(left, c, size),
            _ => None,
        };
        match step {
            // The ligature takes the place of the character before it, so
            // the tail is still the one node.
            Some(crate::tfm::LigKern::Lig { op: 0, char }) if metrics.has_char(char) => {
                n.pending = Some((font, char));
                n.tail[0] = Last::Char(None);
            }
            // The other ligature operations keep or repeat characters. The
            // size of what they leave is not followed.
            Some(crate::tfm::LigKern::Lig { .. }) => {
                n.pending = None;
                let sf = n.space_factor;
                n.widen(true);
                n.space_factor = sf;
                n.tail = [Last::Unknown; TAIL];
                n.tail[0] = Last::Char(None);
            }
            step => {
                self.end_word();
                let Some(next) = self.natural else { return };
                n = next;
                if let Some(crate::tfm::LigKern::Kern(k)) = step {
                    n.along = n.along.extended_by(Dim::exact(k));
                }
                n.pending = Some((font, c));
                n.pushed(Last::Char(None));
            }
        }
        n.space_factor = self.space_factor_after(c, n.space_factor);
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
    fn dimen_parameter(&mut self, name: &str) -> Dim {
        let sym = self.intern(name);
        match self.env.value(sym) {
            Value::Dimen(d) => Dim::exact(d),
            Value::Range { dimen: true, num } => Dim::range(num.lo, num.hi),
            Value::Unknown if self.env.get(sym).is_none() => Dim::exact(0),
            _ => Dim::ANY,
        }
    }

    /// tex.web § 1113 `append_italic_correction`: a kern of the last
    /// character's italic correction, and nothing where the tail is not a
    /// character or ligature.
    fn italic_correction(&mut self) {
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

    /// The word being set ends: its last character is counted (tex.web
    /// § 1034 `main_loop_wrapup`).
    pub(crate) fn end_word(&mut self) {
        let Some(mut n) = self.natural else { return };
        let Some((font, c)) = n.pending.take() else { return };
        let size = self.font_size(font);
        match self.font_metrics(font).zip(size).and_then(|(m, z)| m.char_box(c, z)) {
            Some(b) => {
                n.character([b.width, b.height, b.depth].map(Dim::exact));
                // The node is settled, so `\/` can read its italic correction.
                if n.tail[0] == Last::Char(None) {
                    n.tail[0] = Last::Char(Some((font, c)));
                }
            }
            None => n.character([Dim::ANY; 3]),
        }
        self.natural = Some(n);
    }

    /// `\unskip`, `\unkern` or `\unpenalty` (tex.web § 1105): each takes
    /// away a last item of its own kind. Which kind is not told apart here.
    pub(crate) fn remove_item(&mut self, removed: Removed) {
        self.end_word();
        let Some(mut n) = self.natural else { return };
        let horizontal = self.list_kind();
        let taken = match (removed, n.tail[0]) {
            // What the tail is is not known, so neither is what went. The
            // size widens rather than staying as it was.
            (_, Last::Unknown) => {
                match horizontal {
                    Some(h) => n.widen(h),
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
            (Removed::Penalty, Last::Penalty(_)) => 0,
            _ => return,
        };
        match horizontal {
            // tex.web § 668: the depth the item before it left is not kept,
            // so a vertical list only widens.
            Some(true) => n.along = n.along.reduced_by(Dim::exact(taken)),
            _ => {
                n.along = Dim::ANY;
                n.depth = Dim::ANY;
            }
        }
        n.popped();
        self.natural = Some(n);
    }

    /// `\lastskip`, `\lastkern` or `\lastpenalty` of a list whose last item
    /// is known: zero unless it is of that kind (tex.web § 424).
    pub(crate) fn last_value(&mut self, removed: Removed) -> Option<Value> {
        self.end_word();
        let n = self.natural?;
        match (removed, n.tail[0]) {
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
    /// The token standing for unknown text (see [`Machine::unknown`]).
    pub(crate) fn unknown_token(&mut self, span: Span) -> Token {
        if !matches!(self.env.meaning(self.unknown), Meaning::Unknown) {
            self.env.set(self.unknown, crate::env::Binding::builtin(Meaning::Unknown), true);
        }
        Token::new(Tok::Cs(self.unknown), span)
    }

    /// The rest of unknown text after a delimiter was taken to be in it.
    pub(crate) fn unknown_rest_token(&mut self, span: Span) -> Token {
        if !matches!(self.env.meaning(self.unknown_rest), Meaning::Unknown) {
            self.env.set(self.unknown_rest, crate::env::Binding::builtin(Meaning::Unknown), true);
        }
        Token::new(Tok::Cs(self.unknown_rest), span)
    }

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
        sym == self.unknown
            || sym == self.unknown_rest
            || sym == self.unknown_digits
            || sym == self.unknown_more
            || self.is_unknown_name(sym)
    }

    /// Whether text holds the unknown marker, so that what is made of it is
    /// unknown too.
    /// How many tokens text with unknown parts in it may stand for:
    /// unknown text may be empty or anything, unknown digits are at least
    /// one character.  `None` as the upper end: no bound.
    pub(crate) fn text_length(&self, tokens: &[Token]) -> (usize, Option<usize>) {
        let mut min = 0;
        let mut bounded = true;
        for t in tokens {
            match t.tok {
                Tok::Cs(sym) if sym == self.unknown_digits => {
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
            None if self.any_register_written(crate::tex::RegKind::Box) => BoxState::Unknown,
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
        if take {
            self.box_changed(Some(n), BoxState::Void);
        }
        state
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
    pub(crate) fn set_box(&mut self, register: Option<i64>, state: BoxState, global: bool) {
        let Some(n) = register else { return self.weaken_boxes(global) };
        let sym = self.box_sym(n);
        self.env.set_value(sym, box_value(state), global);
        for dimen in ["wd", "ht", "dp"] {
            let key = self.intern(&format!("\u{4}box.{dimen}.{n}"));
            self.env.set_value(key, Value::Unknown, global);
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
            match self.env.value(key) {
                Value::Dimen(d) => Dim::exact(d),
                Value::Range { dimen: true, num } => Dim::range(num.lo, num.hi),
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
        for dimen in ["wd", "ht", "dp"] {
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
                match self.env.value(key) {
                    // tex.web §§ 649, 668: `hpack` starts height and depth
                    // at zero and takes the maximum, `vpack` the width, so
                    // the bound `set_box_dims` kept is what the test gets.
                    Value::Range { dimen: true, num } => {
                        self.abs = Some(num);
                        self.abs_from = None;
                        None
                    }
                    Value::Unknown => {
                        self.abs = None;
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
