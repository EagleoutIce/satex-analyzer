//! TeX's modes (tex.web §§ 211-213): what `\ifvmode`, `\ifhmode`,
//! `\ifmmode` and `\ifinner` ask.  The run keeps the set of modes it may be
//! in; paths that disagree join to their union, and a test is decided only
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
        const NAMES: [&str; 6] = ["vertical", "internal vertical", "horizontal", "restricted horizontal", "display math", "math"];
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
    /// mode; a math group `{…}` gives back only math.
    pub level: bool,
    /// Which part of a `\discretionary` or `\mathchoice`, or `EQNO`.
    pub part: u8,
    /// The box register a `\setbox` fills when the box ends, and whether
    /// globally (tex.web § 1077 `box_end`).
    pub target: Option<(Option<i64>, bool)>,
    /// A box's `to` size: `Some(None)` when it is not known.
    pub to: Option<Option<i64>>,
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
            to: if self.to == other.to { self.to } else { Some(None) },
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

/// The natural size of the list being built, as `hpack` (tex.web § 649)
/// and `vpack` (§ 668) find it, while every item on the list is known.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Natural {
    /// The sum along the list: the width of a horizontal list, the height
    /// of a vertical one.
    pub along: Scaled,
    /// The largest height and depth on a horizontal list.  A vertical one
    /// holds only glue and kerns here, so its width and depth are zero.
    pub height: Scaled,
    pub depth: Scaled,
    /// The last item, which `\lastskip` and relatives read (tex.web § 424)
    /// and `\unskip` and relatives take away (§ 1105).
    pub last: Last,
    /// The item before it, which `\unskip` and relatives uncover.
    pub before: Last,
    /// The font and character of the word being set that ligatures and
    /// kerns with what comes next, not yet counted (tex.web § 1034 `cur_l`).
    pub pending: Option<(u32, u32)>,
    /// A horizontal list's space factor (tex.web § 212), where known.
    pub space_factor: Option<i64>,
}

/// The last item of a list, as far as `\lastskip`, `\lastkern` and
/// `\lastpenalty` tell items apart.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Last {
    /// None, or one that is not glue, a kern or a penalty.
    #[default]
    Other,
    /// Glue by its natural width, and its whole specification where known.
    Glue(Scaled, Option<crate::value::Glue>),
    Kern(Scaled),
    Penalty(i64),
    Unknown,
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
    /// A `\vrule`: its width, and the height and depth it gives, `None`
    /// where running (tex.web § 463).
    Rule { width: Option<Scaled>, height: Option<Option<Scaled>>, depth: Option<Option<Scaled>> },
    Box([Option<Scaled>; 3]),
    Unknown,
}

/// Two paths' natural sizes: the size where they agree, unknown otherwise.
pub fn join_natural(a: Option<Natural>, b: Option<Natural>) -> Option<Natural> {
    if a == b { a } else { None }
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
    /// `\raise` and relatives.
    Shift,
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
        if vertical != self.mode && self.every("everypar").is_none_or(|t| !t.is_empty()) && self.split_mode(token, vertical) {
            return true;
        }
        self.unread(token);
        self.new_graf(token.span);
        true
    }

    /// tex.web § 1091 `new_graf`.
    fn new_graf(&mut self, span: Span) {
        self.mode = self.mode.map(|m| if m.meets(Modes::ANY_VERTICAL) { Modes::HORIZONTAL } else { m });
        // The lines the paragraph breaks into are not followed.
        self.natural = None;
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
    }

    /// tex.web § 1070 `normal_paragraph`: the paragraph shape parameters go
    /// back to their defaults.  Not `certain` when only some paths reset
    /// them.
    pub(crate) fn normal_paragraph(&mut self, certain: bool) {
        let engine = |m: &mut Self, key: &str| m.intern(&format!("\u{4}{key}"));
        let mut resets = Vec::new();
        for (name, default) in [("looseness", Value::Int(0)), ("hangindent", Value::Dimen(0)), ("hangafter", Value::Int(1))] {
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
    /// § 403 `scan_left_brace`); TeX inserts a missing one.
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
            to: None,
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
            to: None,
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
        let dims = if kind == BoxState::Unknown || nest.code == Some(group::VTOP) {
            [None; 3]
        } else {
            let along = nest.to.unwrap_or(natural.map(|n| n.along));
            match natural {
                _ if kind == BoxState::HBox => [along, natural.map(|n| n.height), natural.map(|n| n.depth)],
                Some(_) => [Some(0), along, Some(0)],
                None => [None, along, None],
            }
        };
        if let Some((register, global)) = nest.target {
            self.set_box(register, kind, global);
            if kind != BoxState::Unknown {
                self.set_box_dims(register, dims);
            }
        } else if nest.placed {
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
            to: None,
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
                self.scan_dimen();
                self.scan_box(BoxContext::Shift, span);
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
                        to: None,
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
            // `\right` ends; satex keeps no group for it, so the display
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
                Some(c) if c >= 0 => self.append_char(c as u32),
                _ => self.append_item(Item::Unknown),
            },
            // tex.web § 1110: `\unhbox` and `\unvbox` empty the register,
            // the `copy` forms do not.
            Material::Unpackage { take } => {
                let n = self.scan_number();
                // A void register adds nothing; a list taken out of a box is
                // not followed.
                if self.fetch_box(n, take) != BoxState::Void {
                    self.append_item(Item::Unknown);
                }
            }
            // tex.web § 463: a `\vrule` is 0.4pt wide unless said otherwise,
            // its height and depth running; an `\hrule` is a vertical one.
            Material::Rule => {
                let [width, height, depth] = self.scan_rule_spec();
                let item = if self.mode.meets(Modes::ANY_HORIZONTAL) {
                    Item::Rule { width: width.unwrap_or(Some(DEFAULT_RULE)), height, depth }
                } else {
                    Item::Unknown
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
            // group; satex runs the cells in the alignment's list.
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
                Meaning::Primitive(Primitive::Mode(ModeCmd::Horizontal(Material::Rule) | ModeCmd::Vertical(Material::Rule)))
                    if context == BoxContext::Leaders =>
                {
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
            // The last box of the current list is typesetting, and the
            // list loses it.
            BoxCmd::LastBox => {
                self.natural = None;
                return self.box_end(context, BoxState::Unknown, [None; 3]);
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
                return self.box_end(context, state, [None; 3]);
            }
            BoxCmd::HBox => {
                // A box appended to a vertical list may take `\vadjust`
                // material with it.
                let code = match context {
                    BoxContext::Direct | BoxContext::Shift => {
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
        let to = if self.scan_keyword("to") {
            Some(self.scan_dimen())
        } else {
            if self.scan_keyword("spread") {
                self.scan_dimen();
                Some(None)
            } else {
                None
            }
        };
        if matches!(context, BoxContext::Shift | BoxContext::Leaders) {
            self.natural = None;
        }
        self.open_list(code, modes, 0, span);
        let mut nest = self.env.nest().expect("the box's group is open");
        nest.to = to;
        match context {
            BoxContext::Set { register, global } => nest.target = Some((register, global)),
            BoxContext::Direct => nest.placed = true,
            _ => {}
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
    fn box_end(&mut self, context: BoxContext, state: BoxState, dims: [Option<Scaled>; 3]) {
        match context {
            BoxContext::Set { register, global } => {
                self.set_box(register, state, global);
                if state != BoxState::Unknown {
                    self.set_box_dims(register, dims);
                }
            }
            BoxContext::Direct => self.append_box(state, dims),
            BoxContext::Shift | BoxContext::Leaders => self.natural = None,
            BoxContext::Ship => {}
        }
    }

    /// A box register's contents appended to the current list; a void
    /// register appends nothing (tex.web § 1076).
    pub(crate) fn append_box(&mut self, state: BoxState, dims: [Option<Scaled>; 3]) {
        match state {
            BoxState::Void => {}
            BoxState::Unknown => self.append_item(Item::Unknown),
            _ => self.append_item(Item::Box(dims)),
        }
    }

    /// An item goes onto the current list: its natural size grows by it
    /// (tex.web §§ 651-656 `hpack`, 669-670 `vpack`).
    pub(crate) fn append_item(&mut self, item: Item) {
        self.material = true;
        self.end_word();
        let Some(mut n) = self.natural else { return };
        let horizontal = self.mode == Modes::RESTRICTED_HORIZONTAL;
        let vertical = self.mode == Modes::INTERNAL_VERTICAL;
        self.natural = match item {
            // What may or may not be a whatsit leaves a last glue, kern or
            // penalty in doubt.
            Item::Zero => {
                if n.last != Last::Other {
                    n.last = Last::Unknown;
                }
                if n.last != Last::Other || n.before != Last::Other {
                    n.before = Last::Unknown;
                }
                Some(n)
            }
            Item::Penalty(p) => {
                n.before = n.last;
                n.last = p.map_or(Last::Unknown, Last::Penalty);
                Some(n)
            }
            Item::Kern(Some(k)) if horizontal || vertical => {
                n.along += k;
                n.before = n.last;
                n.last = Last::Kern(k);
                Some(n)
            }
            Item::Glue(Some(w), spec) if horizontal || vertical => {
                n.along += w;
                n.before = n.last;
                n.last = Last::Glue(w, spec);
                Some(n)
            }
            Item::Rule { width: Some(w), height, depth } if horizontal && height.is_none_or(|h| h.is_some()) && depth.is_none_or(|d| d.is_some()) => {
                n.along += w;
                n.space_factor = Some(1000);
                n.height = n.height.max(height.flatten().unwrap_or(0));
                n.depth = n.depth.max(depth.flatten().unwrap_or(0));
                n.before = n.last;
                n.last = Last::Other;
                Some(n)
            }
            Item::Box([Some(w), Some(h), Some(d)]) if horizontal => {
                n.along += w;
                n.space_factor = Some(1000);
                n.height = n.height.max(h);
                n.depth = n.depth.max(d);
                n.before = n.last;
                n.last = Last::Other;
                Some(n)
            }
            _ => None,
        };
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
            if sf >= 2000 && let Some(x) = xspace_skip {
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
    pub(crate) fn natural_before(&mut self, p: Primitive) {
        use crate::builtins::{PdfOp, Syntax, Typeset};
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
            Primitive::Pdf(PdfOp::Literal | PdfOp::Special | PdfOp::Object | PdfOp::Resources) | Primitive::Write => Item::Zero,
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
        let Some(mut n) = self.natural else { return };
        let font = (self.mode == Modes::RESTRICTED_HORIZONTAL).then(|| self.current_font()).flatten();
        let Some((font, metrics, size)) = font.and_then(|f| Some((f, self.font_metrics(f)?, self.font_size(f)?))) else {
            self.natural = None;
            return;
        };
        if metrics.has_boundary() || !metrics.has_char(c) {
            self.natural = None;
            return;
        }
        let step = match n.pending {
            Some((f, left)) if f == font => metrics.lig_kern(left, c, size),
            _ => None,
        };
        match step {
            Some(crate::tfm::LigKern::Lig { op: 0, char }) if metrics.has_char(char) => n.pending = Some((font, char)),
            Some(crate::tfm::LigKern::Lig { .. }) => {
                self.natural = None;
                return;
            }
            step => {
                self.end_word();
                let Some(next) = self.natural else { return };
                n = next;
                if let Some(crate::tfm::LigKern::Kern(k)) = step {
                    n.along += k;
                }
                n.pending = Some((font, c));
            }
        }
        // tex.web § 1034 `adjust_space_factor`.
        let code = self.char_code(crate::builtins::CodeTable::Space, char::from_u32(c).unwrap_or('\0'));
        n.space_factor = match (code, n.space_factor) {
            (1000, _) => Some(1000),
            (0, sf) => sf,
            (code, _) if code < 1000 => Some(code),
            (_, Some(sf)) if sf < 1000 => Some(1000),
            (code, Some(_)) => Some(code),
            (_, None) => None,
        };
        n.before = n.last;
        n.last = Last::Other;
        self.natural = Some(n);
    }

    /// The word being set ends: its last character is counted (tex.web
    /// § 1034 `main_loop_wrapup`).
    pub(crate) fn end_word(&mut self) {
        let Some(mut n) = self.natural else { return };
        let Some((font, c)) = n.pending.take() else { return };
        let size = self.font_size(font);
        let bx = self.font_metrics(font).zip(size).and_then(|(m, z)| m.char_box(c, z));
        self.natural = bx.map(|b| {
            n.along += b.width;
            n.height = n.height.max(b.height);
            n.depth = n.depth.max(b.depth);
            n
        });
    }

    /// `\unskip`, `\unkern` or `\unpenalty` (tex.web § 1105): each takes
    /// away a last item of its own kind; which kind is not told apart here.
    pub(crate) fn remove_item(&mut self, removed: Removed) {
        self.end_word();
        let Some(mut n) = self.natural else { return };
        let taken = match (removed, n.last) {
            (_, Last::Unknown) => {
                self.natural = None;
                return;
            }
            (Removed::Glue, Last::Glue(w, _)) | (Removed::Kern, Last::Kern(w)) => w,
            (Removed::Penalty, Last::Penalty(_)) => 0,
            _ => return,
        };
        n.along -= taken;
        n.last = std::mem::replace(&mut n.before, Last::Unknown);
        self.natural = Some(n);
    }

    /// `\lastskip`, `\lastkern` or `\lastpenalty` of a list whose last item
    /// is known: zero unless it is of that kind (tex.web § 424).
    pub(crate) fn last_value(&mut self, removed: Removed) -> Option<Value> {
        self.end_word();
        let n = self.natural?;
        match (removed, n.last) {
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
        sym == self.unknown || sym == self.unknown_rest || sym == self.unknown_digits || sym == self.unknown_more || self.is_unknown_name(sym)
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
        match self.env.get(sym) {
            None => BoxState::Void,
            Some(b) => match b.value {
                Value::Int(0) => BoxState::Void,
                Value::Int(1) => BoxState::HBox,
                Value::Int(2) => BoxState::VBox,
                _ => BoxState::Unknown,
            },
        }
    }

    /// `\box`, `\copy`, `\unhbox` and relatives take the register's box;
    /// `take` leaves the register void, without a save-stack entry
    /// (tex.web § 1079 `box(cur_val):=null`).
    pub(crate) fn fetch_box(&mut self, n: Option<i64>, take: bool) -> BoxState {
        let Some(n) = n else { return BoxState::Unknown };
        let state = self.box_state(n);
        if take {
            self.box_changed(Some(n), BoxState::Void);
        }
        state
    }

    /// A register changed in place, as `\box` and `\vsplit` change it.
    fn box_changed(&mut self, n: Option<i64>, state: BoxState) {
        let Some(n) = n else { return };
        if self.box_state(n) == state {
            return;
        }
        let sym = self.box_sym(n);
        self.env.set_value_in_place(sym, box_value(state));
        self.reset_box_dimens(n);
    }

    /// `\setbox`: a register assignment (tex.web § 1077).
    pub(crate) fn set_box(&mut self, register: Option<i64>, state: BoxState, global: bool) {
        let Some(n) = register else { return };
        let sym = self.box_sym(n);
        self.env.set_value(sym, box_value(state), global);
        for dimen in ["wd", "ht", "dp"] {
            let key = self.intern(&format!("\u{4}box.{dimen}.{n}"));
            self.env.set_value(key, Value::Unknown, global);
        }
    }

    /// The width, height and depth of box register `n`, where known.
    pub(crate) fn box_dims(&mut self, n: Option<i64>) -> [Option<Scaled>; 3] {
        let Some(n) = n else { return [None; 3] };
        if self.box_state(n) == BoxState::Void {
            return [Some(0); 3];
        }
        ["wd", "ht", "dp"].map(|which| {
            let key = self.intern(&format!("\u{4}box.{which}.{n}"));
            match self.env.value(key) {
                Value::Dimen(d) => Some(d),
                _ => None,
            }
        })
    }

    fn set_box_dims(&mut self, register: Option<i64>, dims: [Option<Scaled>; 3]) {
        for (which, value) in ["wd", "ht", "dp"].into_iter().zip(dims) {
            self.set_box_dimen(which, register, value);
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
                    // those of a packed box are at least zero.
                    Value::Unknown => {
                        let packed = match self.box_state(n) {
                            BoxState::HBox => which != "wd",
                            BoxState::VBox => which == "wd",
                            _ => false,
                        };
                        self.abs = packed.then(|| crate::value::Num::range(0, crate::value::MAX_DIMEN));
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
        let Some(n) = n else { return };
        match self.box_state(n) {
            BoxState::Void => {}
            state => {
                let key = self.intern(&format!("\u{4}box.{which}.{n}"));
                let value = if state == BoxState::Unknown { Value::Unknown } else { value.map_or(Value::Unknown, Value::Dimen) };
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
