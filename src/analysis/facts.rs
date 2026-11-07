//! What the run observed. `query.rs` renders it.

use std::rc::Rc;

use crate::analysis::graph::ControlDep;
use crate::engine::builtins::{DefMode, LoadKind, OccKind};
use crate::engine::env::NodeId;
use crate::engine::tex::{FileId, MacroDef, Span, Sym, Token};

/// A name an expl3 module function took as its variable, where the user's
/// file passed it.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct VarUse {
    pub name: Sym,
    pub kind: crate::analysis::roles::VarKind,
    /// The module function that made or set it (`\tl_gset:Nn`).
    pub by: Sym,
    pub span: Span,
}

/// One `\def`-family definition or assignment the run observed: what name,
/// by which command, with what meaning. Part of [`Facts::defs`].
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Definition {
    pub name: Sym,
    pub by: Sym,
    #[serde(with = "crate::fixed::leaked")]
    pub tag: crate::fixed::Str,
    pub subject: Option<Sym>,
    pub mode: DefMode,
    pub span: Span,
    pub package: Option<Sym>,
    pub node: NodeId,
    pub mac: Option<Rc<MacroDef>>,
    pub depth: u16,
    pub global: bool,
    pub redefines: bool,
    pub certain: bool,
    pub cds: Vec<ControlDep>,
    /// The environments open when this definition was made, outermost
    /// first (`\@currenvir` at the time, generalized to the whole stack):
    /// empty outside any environment.  What `explain` groups a name's
    /// distinct meanings by, alongside `depth`/`global` for group nesting.
    pub context: Rc<[Sym]>,
    /// The macro expanding right when this definition was made, innermost
    /// (`Machine::within`'s top), when there was one: `\list` for `\list`'s
    /// own `\def\@itemlabel{#1}`, re-run at the same source site by every
    /// environment built on it.  What `explain` derives "which environments
    /// reach this" from, generically, instead of the raw environment stack.
    pub via: Option<Sym>,
    /// The name is a token the document's own files hold (tex.web § 1215,
    /// `get_r_token`): written there, or built by a `\csname` written
    /// there.  A name package code builds for the document is not.
    pub spelled: bool,
    /// The definition is part of the text the file's current call reads: a
    /// statement on the file's own path, not a line of a macro body that
    /// runs per call.
    pub stated: bool,
}

impl Definition {
    pub fn arity(&self) -> u8 {
        self.mac.as_ref().map_or(0, |m| m.arity())
    }
}

/// One `\errmessage`-family error a real call in the run raised, while
/// `name` was on the call stack (`Machine::within`): what `explain` shows
/// beside a probed error for a meaning that only misbehaves in some
/// contexts (`\item` outside a list, say), found by running the document,
/// not guessed from a table of names.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct RaisedError {
    pub name: Sym,
    pub text: String,
    pub span: Span,
    /// The environment stack in force where the error was reached.
    pub context: Rc<[Sym]>,
    pub certain: bool,
}

/// The kind of meaning an [`Expansion`] found at its control sequence:
/// primitive, macro, register, character, undefined, or merely known to
/// exist ([`crate::engine::tex::Meaning::Unknown`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum MeaningKind {
    Primitive,
    Macro(NodeId),
    Register,
    Char,
    Undefined,
    /// Undefined, met on a path the analysis approximated since the paths
    /// parted: TeX may never expand it there.
    UndefinedApproximated,
    Unknown,
}

impl MeaningKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MeaningKind::Primitive => "primitive",
            MeaningKind::Macro(_) => "macro",
            MeaningKind::Register => "register",
            MeaningKind::Char => "char",
            MeaningKind::Undefined => "undefined",
            MeaningKind::UndefinedApproximated => "undefined-approximated",
            MeaningKind::Unknown => "unknown",
        }
    }
}

/// One control sequence expanded or executed: its [`MeaningKind`], the
/// arguments it took, and how many times the site fired. Part of
/// [`Facts::expansions`].
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Expansion {
    pub name: Sym,
    pub span: Span,
    pub package: Option<Sym>,
    pub node: Option<NodeId>,
    pub meaning: MeaningKind,
    pub within: Option<Sym>,
    pub arguments: Vec<Box<[Token]>>,
    pub cds: Vec<ControlDep>,
    pub count: u32,
    /// The modes the run was in at the uses (tex.web § 211).
    pub mode: crate::engine::mode::Modes,
    /// The group level at the first use; 0 for one a package cache replays.
    pub depth: u16,
}

/// How a `\usepackage`-family [`Load`] turned out: read, skipped by
/// configuration, already loaded, or never found.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum LoadStatus {
    Read,
    NotFollowed,
    Skipped,
    AlreadyLoaded,
    TooDeep,
    NotFound,
    Unreadable,
    CaseMismatch { on_disk: Sym },
}

impl LoadStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            LoadStatus::Read => "read",
            LoadStatus::NotFollowed => "not-followed",
            LoadStatus::Skipped => "skipped",
            LoadStatus::AlreadyLoaded => "already-loaded",
            LoadStatus::TooDeep => "too-deep",
            LoadStatus::NotFound => "not-found",
            LoadStatus::Unreadable => "unreadable",
            LoadStatus::CaseMismatch { .. } => "case-mismatch",
        }
    }
}

/// `\usepackage`, `\documentclass`, `\input`, `\include`, `\RequirePackage`.
/// [`Facts::loads`] holds one per site, with its [`LoadStatus`].
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Load {
    pub name: String,
    pub kind: LoadKind,
    pub options: Vec<String>,
    pub span: Span,
    pub by: Option<Sym>,
    pub file: Option<FileId>,
    pub path: Option<String>,
    pub status: LoadStatus,
    /// `\usepackage{p}[2020/01/01]`: the oldest release that will do.
    pub required: Option<String>,
    /// `\ProvidesPackage{p}[2019/01/01 v1.0 …]`: what the file says it is.
    pub provided: Option<String>,
    /// Nesting depth of the load, for the dependency tree.
    pub depth: u16,
    /// Made on every path: no undecided conditional encloses it.
    pub certain: bool,
    /// The call in the user's own files the load happened under
    /// (`\usetikzlibrary{calc}` for `tikzlibrarycalc.code.tex`).
    pub call: Option<Span>,
}

/// Anything named that is not itself a control sequence: labels, citations,
/// environment delimiters, hook names, identification banners, messages.
/// Tagged by [`OccKind`]. Part of [`Facts::occurrences`].
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Occurrence {
    pub kind: OccKind,
    pub key: String,
    pub detail: Option<String>,
    /// Where it stands: the call the file made, or, for a key derived from
    /// a written line, the key's own text.
    pub span: Span,
    /// Where `span` was reached from: the call that expanded the full
    /// target, or the `\write` whose line declared the key.
    pub expanded: Option<Span>,
    /// The call read straight from a file that was running: the site a
    /// key built in a macro body belongs to.
    pub call: Option<(Sym, Span)>,
    /// The innermost command of `call`'s own text it came out of: a
    /// `\begin` inside the call's braced argument.
    pub site: Option<(Sym, Span)>,
    pub package: Option<Sym>,
    pub node: NodeId,
    /// The sectioning unit this stands in, or `None` before the first one.
    pub section: Option<Rc<str>>,
    /// Whether every run reaches it: not on one of several paths, and not
    /// after the mode was widened to several.
    pub certain: bool,
}

/// A number and the letters set right after it, as the run typeset them
/// (tex.web § 1034: both are characters of one word).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct Quantity {
    pub number: String,
    pub unit: String,
    /// The first digit.
    pub span: Span,
    /// The last digit.
    pub number_end: Span,
    /// The first character of the unit.
    pub unit_span: Span,
    /// The control sequence the file called where the unit was set.
    pub via: Option<Sym>,
    /// The files the macro bodies running at the number live in.
    pub within: Rc<[FileId]>,
    /// Set on every run: no undecided conditional or split path around it.
    pub certain: bool,
}

/// A color the run defined, read off the `\color@⟨name⟩` macro xcolor
/// stores it in (xcolor § 2.5): `\definecolor`, `\providecolor`,
/// `\colorlet` and `\definecolorset` all end in one of these, so the fact
/// is what the interpreter did and not a list of names.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ColorDef {
    pub name: String,
    /// The part of `name` the document's call wrote: `comments` for the
    /// `xlst@colors@lst@comments` a package's `\lstcolorlet{comments}` makes.
    pub shown: String,
    /// The definition `\let`s another color's control sequence: xcolor's
    /// `\colorlet` given a spec that is exactly a defined color's name.
    pub alias: bool,
    /// The color as xcolor stored it, always in one of its core models
    /// (`\XC@coremodel`).  `None` when satex could not read the macro.
    pub value: Option<crate::engine::value::Colors>,
    /// The model and the spec the document's own call wrote, before xcolor
    /// converted them.  `None` when no document call made the definition.
    pub declared: Option<(crate::engine::value::Model, String)>,
    /// Where the declared spec stands, for a finding that names it.
    pub spec_span: Option<Span>,
    /// Where the document's call stands, or the definition itself when a
    /// package made it.
    pub span: Span,
    /// The control sequence the file called: `\definecolor`, `\colorlet`.
    pub via: Option<Sym>,
    pub package: Option<Sym>,
    /// Whether every run reaches the definition.
    pub certain: bool,
}

/// An `\item` the document wrote that stepped its list's counter: the
/// `\@currentlabel` its `\refstepcounter` made, which `\ref` prints.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct ListItem {
    /// The `\item` call.
    pub span: Span,
    /// The counter `\@listctr` names: `enumi`.
    pub counter: String,
    /// Its value after the step: `[lo, hi]`, `None` when not a number.
    pub value: Option<(i64, i64)>,
    /// What `\ref` prints: the counter's representation expanded, `2`, `(b)`, `Step~2`.
    pub label: String,
    /// Whether every run made it.
    pub certain: bool,
}

/// A color named by a `\csname\color@⟨name⟩\endcsname` the run formed:
/// xcolor reaches every color by that one control sequence, so a use, a
/// test and a `\colorlet` source all arrive here (xcolor § 2.5).
#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct ColorUse {
    pub name: String,
    /// What the name was formed for ([`CsnameRole::Use`] for `\color`).
    pub role: CsnameRole,
    /// Whether the color was defined when the run reached the name.
    pub defined: bool,
    /// Where the document's call stands.
    pub span: Span,
    pub via: Option<Sym>,
    pub package: Option<Sym>,
    pub certain: bool,
}

named_enum! {
    /// What a `\pdfcolorstack` does to its stack (pdfTeX manual, § 8.9.8).
    #[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
    pub enum StackOp {
        Push => "push",
        Pop => "pop",
        Set => "set",
        Current => "current",
    }
}

/// One `\pdfcolorstack` the run executed: which stack, what it did to it,
/// and where.  [`crate::linter::pdf`] balances the pushes against the pops
/// the way it balances `q` against `Q`.
#[derive(Clone, Copy, serde::Serialize, serde::Deserialize)]
pub struct ColorStack {
    /// The stack number, `None` when the run could not decide it.
    pub stack: Option<i64>,
    pub op: StackOp,
    pub span: Span,
    pub certain: bool,
}

/// The font in force at a moment of the run, as LaTeX's font selection
/// holds it (fntguide § 2): the five macros `\selectfont` reads, and the
/// font the engine had actually loaded.  [`Facts`] keeps one row per
/// distinct state and [`Spot`] points at it.
#[derive(Clone, Default, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub struct FontState {
    /// `\cf@encoding`.
    pub encoding: Rc<str>,
    /// `\f@family`.
    pub family: Rc<str>,
    /// `\f@series`.
    pub series: Rc<str>,
    /// `\f@shape`.
    pub shape: Rc<str>,
    /// `\f@size`.
    pub size: Rc<str>,
    /// `\current@color`: the driver code the run's color selection set.
    pub color: Rc<str>,
    /// `\the\font`, 0 for none: what [`crate::analysis::machine::Analysis::fonts`]
    /// names the metrics of.  NFSS says what the document asked for, this
    /// says what the engine loaded, and a lint needs both.
    pub font: u16,
}

impl FontState {
    /// `OT1/cmr/m/it/10`, the shape as fntguide § 2 writes it.
    pub fn nfss(&self) -> String {
        format!("{}/{}/{}/{}/{}", self.encoding, self.family, self.series, self.shape, self.size)
    }
}

/// One character the run typeset outside package code, with where the
/// token that made it stood and the modes it was set in (tex.web § 211).
/// Recorded before TeX's ligature program runs, so `--` is still two
/// characters here.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct Spot {
    pub span: Span,
    /// The font it was set in, as an index into
    /// [`crate::analysis::machine::Analysis::font_states`], 0 when the run did not
    /// know.  A `u16` keeps the struct at the 16 bytes it had without it.
    pub font_state: u16,
    pub modes: crate::engine::mode::Modes,
    /// Whether something that is not a character came right before it, so
    /// TeX starts a new word and no ligature joins it to the character
    /// before (tex.web § 1034).
    pub broken: bool,
    /// Whether a box began or ended, or a box's list was placed, since the
    /// character before it.  Characters are kept in the order TeX set them,
    /// so the two may not stand side by side on the page.
    pub apart: bool,
    /// Whether every run sets it: not on one of several paths through an
    /// undecided conditional, and not after the mode was widened.
    pub certain: bool,
}

/// Every character the run typeset outside package code, in order: the
/// document as the reader sees it, with each character's source position.
/// [`Facts::text`] holds it, and the typographic lints read it.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Text {
    pub chars: String,
    /// One per character of `chars`, in the same order.
    pub at: Vec<Spot>,
    /// Where a character's overhang cannot reach the next one: after a
    /// `\/` (tex.web § 1113) or at a paragraph's end, as the number of
    /// characters set before it, ascending.
    pub closed: Vec<u32>,
    /// Where `\par` ended a paragraph, as the number of characters set
    /// before it, ascending.
    pub pars: Vec<u32>,
    /// Where a paragraph began (tex.web § 1091): the number of characters set before it, and
    /// the command of the user's files that began it. A key read from there on is the paragraph's.
    pub par_starts: Vec<(u32, Span)>,
    /// The ends in `pars` a `\penalty` of at least 10000 follows before any
    /// further text: no page break may part the paragraph from what comes
    /// next, as after a heading (LaTeX's `\@xsect`, plain's `\beginsection`).
    pub kept: Vec<u32>,
    /// The ends in `pars` some path made and another may not have: there
    /// the paragraph may go on, ascending.
    pub open: Vec<u32>,
    /// For a character set under an undecided conditional: its index, the
    /// outermost conditional's node and the arm taken. One both arms set
    /// is listed once per arm.
    pub arms: Vec<(u32, crate::engine::env::NodeId, bool)>,
    /// Characters a loop set again, so any number of times, ascending.
    pub repeats: Vec<u32>,
    /// Characters a discretionary hyphen may precede (tex.web § 923), ascending.
    pub hyphens: Vec<u32>,
    /// How many characters [`crate::engine::hyph`] has looked at.
    pub hyphenated: u32,
    /// One per character: the place in the user's own files whose reading
    /// set it, the innermost such file being read (the call site, not the
    /// macro body).
    pub site: Vec<Span>,
    /// Character ranges (start, end) of boxes placed on a list or shipped,
    /// so their text reaches the page.
    pub placed: Vec<(u32, u32)>,
    /// Character ranges of boxes lost unused: a register overwritten or put back at a group's
    /// end (tex.web § 283) while nothing else held them. Never on the page unless also `placed`.
    pub dropped: Vec<(u32, u32)>,
    /// Character ranges (start, end) set in an `\insert` (a footnote's text): the page
    /// places them at its foot, not where they were set.
    pub inserts: Vec<(u32, u32)>,
    /// Rules set between characters (tex.web § 463): the number of
    /// characters set before each, its kind, the user's line it was set from
    /// and the number of [`Text::align`] marks before it, which orders the
    /// two at one place (a `\bottomrule` before its alignment's end).
    pub rules: Vec<(u32, Rule, Span, u32)>,
    /// An alignment's (`\halign`, a `tabular`'s) parts, an `\hbox to` set
    /// in the line and leaders between characters: the number of characters
    /// set before each.
    pub align: Vec<(u32, Align)>,
    /// For a character set in math mode: its index and the math code
    /// (`\mathcode`: class, family and slot) it was set with, ascending.
    pub math: Vec<(u32, u16)>,
    /// For the first character of a paragraph: the width of `\parskip` (sp)
    /// the run had there, when it was exactly one value.
    pub skips: Vec<(u32, crate::engine::value::Scaled)>,
    /// For the first character of a paragraph: how its lines are aligned,
    /// from `\leftskip` and `\rightskip` as the run had them there.
    pub just: Vec<(u32, Just)>,
    /// Where `\shipout` sent a page out: the number of characters set
    /// before it, ascending, and the place in the user's files read then.
    pub pages: Vec<(u32, Span)>,
    /// Where the page passed another `\vsize` while pages break only where forced
    /// (`analysis.assume_forced_page_breaks`): pdfTeX breaks a page before it, surely when the
    /// shortest the page can be has passed it (`true`), else when the page may have; then the
    /// number of pages shipped before it.
    pub fills: Vec<(u32, bool, u32)>,
    /// The pages, by how many were shipped before, whose length had no upper bound: any
    /// number of pages may have filled there.
    pub open_pages: Vec<u32>,
    /// Character ranges of boxes used in a box that has rules on its sides
    /// and no text of its own (`\fbox`): one framed box around them.
    pub frames: Vec<(u32, u32)>,
}

/// How a paragraph's lines sit between the margins.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Just {
    Justified,
    Left,
    Right,
    Center,
    /// A skip the run could not decide.
    Unknown,
}

impl Just {
    pub fn name(self) -> &'static str {
        match self {
            Just::Justified => "justified",
            Just::Left => "left",
            Just::Right => "right",
            Just::Center => "center",
            Just::Unknown => "unknown",
        }
    }
}

/// Where an alignment begins, ends a cell (`&`) or a row (`\cr`), and
/// ends; where a box of a set width begins in the line and ends (a table of
/// contents' `\numberline`, its page number), and where leaders fill it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Align {
    Begin,
    Cell,
    Row,
    End,
    /// An `\hbox to` this width (sp).
    BoxTo(crate::engine::value::Scaled),
    BoxEnd,
    Leaders,
    /// A forced line break in a paragraph (`\\`, `\newline`: a penalty of -10000).
    Break,
    /// Glue in a line that stretches infinitely (`\hfil`, `\hfill`, `\hss`,
    /// `\hspace{\fill}`): where it stands in a box or cell sets how the rest sits in it.
    Fill(crate::engine::value::Glue),
    /// Other glue that may stretch or shrink: in a line (`false`, an `\hskip`) or
    /// between lines (`true`, `\vfill`, a `\vskip` with `plus`).
    Glue(crate::engine::value::Glue, bool),
    /// `\parskip` before a paragraph, when it may stretch or shrink.
    ParSkip(crate::engine::value::Glue),
    /// A math script begins (`^`: true, `_`: false), and ends.
    Script(bool),
    ScriptEnd,
    /// `\eqno` (`false`) or `\leqno` (`true`): the equation's number follows, set at the margin.
    EqNo(bool),
    /// `\limits` (`true`) or `\nolimits` (`false`) after an operator (tex.web § 1159).
    Limits(bool),
}

/// A glue's spec as TeX shows it: `0.0pt plus 1.0fill`.
fn spec(g: crate::engine::value::Glue) -> String {
    crate::engine::value::Value::Glue(g).render()
}

impl Align {
    pub fn name(self) -> String {
        match self {
            Align::Begin => "begin".into(),
            Align::Cell => "cell".into(),
            Align::Row => "row".into(),
            Align::End => "end".into(),
            Align::BoxTo(width) => format!("box {:.2}", width as f64 / 65536.0),
            Align::BoxEnd => "boxend".into(),
            Align::Leaders => "leaders".into(),
            Align::Break => "break".into(),
            Align::Fill(g) => format!("fill {}", spec(g)),
            Align::Glue(g, vertical) => format!("{} {}", if vertical { "vskip" } else { "hskip" }, spec(g)),
            Align::ParSkip(g) => format!("parskip {}", spec(g)),
            Align::Script(sup) => (if sup { "sup" } else { "sub" }).into(),
            Align::ScriptEnd => "scriptend".into(),
            Align::EqNo(left) => (if left { "leqno" } else { "eqno" }).into(),
            Align::Limits(on) => (if on { "limits" } else { "nolimits" }).into(),
        }
    }
}

/// A rule [`Text::rules`] keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Rule {
    /// A thin `\vrule`: a table's `|`.
    Vertical,
    /// An `\hrule`: `\hline`, a line across.
    Horizontal,
    /// A `\vrule` wider than 1pt: a bar in the line (`\rule{1cm}{2pt}`),
    /// its width and its height plus depth (sp), 0 where the run did not know it.
    Bar(crate::engine::value::Scaled, crate::engine::value::Scaled),
}

impl Rule {
    /// How the typeset text shows it.
    pub fn glyph(self) -> &'static str {
        match self {
            Rule::Vertical => "│",
            Rule::Horizontal => "―",
            Rule::Bar(..) => "▬",
        }
    }
}

impl Text {
    pub fn is_empty(&self) -> bool {
        self.at.is_empty()
    }

    pub fn len(&self) -> usize {
        self.at.len()
    }
}

/// How sure the interpreter is about what it saw. [`Diagnostic::severity`]
/// carries it, from a plain note up to an imprecision the analysis had to
/// make.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Severity {
    Info,
    Warning,
    /// The analysis had to widen here, and says so.  Widening and an
    /// undecided conditional are how satex stays sound, not places it failed.
    Imprecision,
    /// A construct satex cannot follow yet: an engine primitive it does not
    /// interpret, a scan that ran off its end, a budget it could not keep
    /// within.  These are the analysis's real limits: what follows may be
    /// wrong.
    Unsupported,
    /// A soundiness assumption the run made where it could not follow every
    /// run (a loop is taken to end, a file to close the groups it opened):
    /// the results are sound if it holds.
    Assumption,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Unsupported => "unsupported",
            Severity::Warning => "warning",
            Severity::Imprecision => "imprecision",
            Severity::Assumption => "assumption",
        }
    }
}

/// One thing the analysis wants the user to know: a message, a [`Severity`],
/// and the [`Span`] it concerns. [`Facts::diagnostics`] is what `satex
/// lint`'s correctness rules and `query diagnostics` read.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Diagnostic {
    pub severity: Severity,
    #[serde(with = "crate::fixed::leaked")]
    pub code: crate::fixed::Str,
    pub message: String,
    pub span: Span,
    /// How many times the run raised it: identical diagnostics at one place
    /// are one row.
    pub count: u32,
    /// The innermost place in the user's own files that led here: `span`
    /// itself when it is one, `None` when nothing on the input stack is.
    pub origin: Option<Span>,
    /// How many [`Occurrence`]s the run had recorded when it first raised
    /// this: its place in run order among them.
    pub after: usize,
}

/// What a run observed: [`Definition`]s, [`Expansion`]s, [`Occurrence`]s and
/// [`Load`]s, which every query and lint reads.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Facts {
    pub defs: Vec<Definition>,
    /// The files `\write` handed lines to, as `\openout` named them.
    pub written_files: std::collections::BTreeSet<String>,
    /// The files `\openin` or `\pdffilesize` looked for and did not find:
    /// what an installation that has them would read.
    pub absent: std::collections::BTreeSet<String>,
    /// The paths they found: the run takes another path without them, so a
    /// copy of the installation (the playground's) must hold them too.
    #[serde(default)]
    pub found: std::collections::BTreeSet<String>,
    /// What each command was seen to read, as `{}` for a mandatory argument,
    /// `[]` for an optional one and `*` for a star: observed while it ran,
    /// not declared anywhere.
    pub shapes: crate::fixed::HashMap<Sym, String>,
    /// What TeX read each token of the user's files as, joined over the
    /// run ([`crate::analysis::roles::Ty`]): a scanner's quantity, a register an
    /// assignment named, a register's name a `\csname` formed.
    pub types: crate::fixed::HashMap<Span, crate::analysis::roles::Ty>,
    /// The font states ([`crate::analysis::machine::Analysis::font_states`] rows) in force where a
    /// length was scanned at a span of the user's files: what its `em` and `ex` mean there.
    pub length_fonts: crate::fixed::HashMap<Span, Vec<u16>>,
    /// Each expl3 variable a module function of the user's files made or
    /// set, with that function ([`crate::analysis::roles::VarKind::of_function`]):
    /// what the conventions lints check names against.
    pub vars: Vec<VarUse>,
    /// Where a non-alphabetic character became a letter or stopped being
    /// one, at the user's line that caused it (`\makeatletter`); a file's
    /// line 0 for the letters it opened with: what names can be typed where.
    pub letters: Vec<(crate::engine::tex::Span, char, bool)>,
    pub expansions: Vec<Expansion>,
    pub loads: Vec<Load>,
    pub occurrences: Vec<Occurrence>,
    /// Every `\pdfliteral` the run wrote, in order: what a picture draws.
    pub literals: Vec<Literal>,
    pub diagnostics: Vec<Diagnostic>,
    /// What each `\csname` site formed its control sequence for, the first
    /// time it ran.
    pub csnames: crate::fixed::HashMap<Span, CsnameRole>,
    /// Per first character of text the user's files wrote into a
    /// `\csname` name, what the code put before it: the kind of name
    /// typed there (`pgf@sh@ns@` a node, `r@` a label). Completion offers
    /// the names defined with the same prefix.
    pub slots: crate::fixed::HashMap<Span, String>,
    /// `\begingroup … \endgroup` read straight from a user's file over several
    /// lines: where each opened and where it closed.
    pub groups: Vec<(Span, Span)>,
    /// Real `\errmessage`s the run reached, attributed to every name on the
    /// call stack at the time (`explain` filters by the one it was asked
    /// about). Deduplicated by name, text and context.
    pub raised: Vec<RaisedError>,
    /// The modes each space token was read in outside package code
    /// (tex.web § 1043: glue in horizontal mode, nothing otherwise).
    pub spaces: crate::fixed::HashMap<Span, crate::engine::mode::Modes>,
    /// The spaces whose glue a character, box or rule followed on its list,
    /// so it shows: neither a paragraph's end (tex.web § 816) nor
    /// `\unskip` (§ 1105) took it away.
    pub kept_spaces: crate::fixed::HashSet<Span>,
    /// Every conditional the run evaluated outside package code.
    pub conditionals: Vec<Conditional>,
    /// Numbers with letters set right after them, outside package code.
    pub quantities: Vec<Quantity>,
    /// The colors the run defined, in order.
    pub colors: Vec<ColorDef>,
    /// Every `\item` of the document's files that stepped a list counter, in order.
    pub items: Vec<ListItem>,
    /// Every `\color@⟨name⟩` the run formed a control sequence for.
    pub color_uses: Vec<ColorUse>,
    /// Every `\pdfcolorstack` the run executed, in order.
    pub color_stack: Vec<ColorStack>,
    /// Every character set outside package code, in order.
    pub text: Text,
    /// Each line of the user's files a mouth read to its end, merged over every read: the
    /// comment column all of them had, the spaces all of them skipped.
    pub lines: crate::fixed::HashMap<(crate::engine::tex::FileId, u32), crate::engine::tex::LineRead>,
}

/// A conditional the run evaluated (tex.web §§ 487-510), merged over every
/// time it ran.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Conditional {
    pub name: Sym,
    /// The `\if…`.
    pub at: Span,
    /// The `\else`s and `\or`s met.
    pub arms: Vec<Span>,
    /// The `\fi`, once met.
    pub fi: Option<Span>,
    /// The arms a decided test selected (0 the first, `None` none).
    pub taken: Vec<Option<u32>>,
    /// Whether some test was undecided, so that every arm ran.
    pub undecided: bool,
    /// Where the first undecided test ended: the first token after it.
    pub test_end: Option<Span>,
}

/// What a `\csname…\endcsname` was formed for, read off the command an
/// `\expandafter` held back while the name was formed (tex.web § 368).
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum CsnameRole {
    /// On its own: the control sequence is expanded or executed.
    Use,
    /// `\expandafter\def\csname…`: it is defined.
    Define { global: bool, expand: bool },
    /// `\expandafter\let\csname…`: it is made an alias of what follows.
    Let,
    /// `\expandafter\let\expandafter\x\csname…`: an alias is made of it.
    LetTo,
    /// `\expandafter\ifx\csname…\endcsname\relax`: it is tested for being
    /// undefined.
    Test,
}

impl Facts {
    /// Merges reads of lines: a comment column only where every read had
    /// one (the last of them), a skipped space only where every read skipped it.
    pub fn note_reads(&mut self, file: crate::engine::tex::FileId, reads: Vec<crate::engine::tex::LineRead>) {
        for read in reads {
            self.lines
                .entry((file, read.line))
                .and_modify(|r| {
                    r.comment = r.comment.zip(read.comment).map(|(a, b)| a.max(b));
                    r.skipped &= read.skipped;
                })
                .or_insert(read);
        }
    }

    /// The non-alphabetic characters that are letters at `file:line:col`.
    pub fn letters_at(&self, file: crate::engine::tex::FileId, line: u32, col: u32) -> Vec<char> {
        let mut out: Vec<char> = Vec::new();
        for (_, c, letter) in self.letters.iter().filter(|(s, ..)| s.file == file && (s.line, s.col) < (line, col)) {
            out.retain(|x| x != c);
            if *letter {
                out.push(*c);
            }
        }
        out
    }
}

/// The conditionals a name a user gives may mean, each with how it got
/// there, by LaTeX's naming conventions: `\\name` itself, the switch
/// `\\ifname` that `\\newif` makes for the stem `name`, and the switches the
/// option `name` sets: its handler `\\ds@name` calls `\\stemtrue` or
/// `\\stemfalse`. The handler is read from its definitions, as
/// `\\ProcessOptions` lets it to `\\relax`.
pub fn switches_named(
    interner: &crate::engine::tex::Interner,
    defs: &[Definition],
    wanted: &str,
) -> Vec<(String, String)> {
    let mut out = vec![
        (wanted.to_string(), String::new()),
        (format!("if{wanted}"), format!("the switch `\\newif` makes for `{wanted}`")),
    ];
    let Some(handler) = interner.lookup(&format!("ds@{wanted}")) else { return out };
    let bodies = defs.iter().filter(|d| d.name == handler).filter_map(|d| d.mac.as_deref());
    for t in bodies.flat_map(|m| m.replacement_text.iter()) {
        let Some(name) = t.cs().map(|s| interner.name(s)) else { continue };
        let stem = name.strip_suffix("true").or_else(|| name.strip_suffix("false"));
        if let Some(stem) = stem.filter(|s| !s.is_empty()) {
            let switch = format!("if{stem}");
            if !out.iter().any(|(n, _)| *n == switch) {
                out.push((switch, format!("the switch the option `{wanted}` sets")));
            }
        }
    }
    out
}

/// A `\pdfliteral` the run wrote (pgf's driver draws through them): its
/// PDF content operators, where the document's call stands, and how many
/// characters were typeset before it, so text set between two literals is
/// placed where the drawing stands then.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Literal {
    pub at: Span,
    pub text: String,
    pub chars: u32,
}
