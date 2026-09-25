//! What the run observed; `query.rs` renders it.

use std::rc::Rc;

use crate::builtins::{DefMode, LoadKind, OccKind};
use crate::env::NodeId;
use crate::graph::ControlDep;
use crate::tex::{FileId, MacroDef, Span, Sym, Token};

/// One `\def`-family definition or assignment the run observed: what name,
/// by which command, with what meaning. Part of [`Facts::defs`].
#[derive(Clone)]
pub struct Definition {
    pub name: Sym,
    pub by: Sym,
    pub tag: &'static str,
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
#[derive(Clone)]
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
/// exist ([`crate::tex::Meaning::Unknown`]).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MeaningKind {
    Primitive,
    Macro(NodeId),
    Register,
    Char,
    Undefined,
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
            MeaningKind::Unknown => "unknown",
        }
    }
}

/// One control sequence expanded or executed: its [`MeaningKind`], the
/// arguments it took, and how many times the site fired. Part of
/// [`Facts::expansions`].
#[derive(Clone)]
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
    pub mode: crate::mode::Modes,
}

/// How a `\usepackage`-family [`Load`] turned out: read, skipped by
/// configuration, already loaded, or never found.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoadStatus {
    Read,
    NotFollowed,
    Skipped,
    AlreadyLoaded,
    TooDeep,
    NotFound,
    Unreadable,
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
        }
    }
}

/// `\usepackage`, `\documentclass`, `\input`, `\include`, `\RequirePackage`.
/// [`Facts::loads`] holds one per site, with its [`LoadStatus`].
#[derive(Clone)]
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
}

/// Anything named that is not itself a control sequence: labels, citations,
/// environment delimiters, hook names, identification banners, messages.
/// Tagged by [`OccKind`]; part of [`Facts::occurrences`].
#[derive(Clone)]
pub struct Occurrence {
    pub kind: OccKind,
    pub key: String,
    pub detail: Option<String>,
    pub span: Span,
    /// `span` is the call the file made, expanded the full target
    pub expanded: Option<Span>,
    pub package: Option<Sym>,
    pub node: NodeId,
    /// The sectioning unit this stands in, or `None` before the first one.
    pub section: Option<Rc<str>>,
    /// Whether every run reaches it: not on one of several paths, and not
    /// after the mode was widened to several.
    pub certain: bool,
}

/// How sure the interpreter is about what it saw; [`Diagnostic::severity`]
/// carries it, from a plain note up to an imprecision the analysis had to
/// make.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Info,
    Warning,
    /// The analysis had to widen here, and says so.  Widening and an
    /// undecided conditional are how satex stays sound, not places it failed.
    Imprecision,
    /// A construct satex cannot follow yet: an engine primitive it does not
    /// interpret, a scan that ran off its end, a budget it could not keep
    /// within.  These are the analysis's real limits.
    Unsupported,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Unsupported => "unsupported",
            Severity::Warning => "warning",
            Severity::Imprecision => "imprecision",
        }
    }
}

/// One thing the analysis wants the user to know: a message, a [`Severity`],
/// and the [`Span`] it concerns. [`Facts::diagnostics`] is what `satex
/// lint`'s correctness rules and `query diagnostics` read.
#[derive(Clone)]
pub struct Diagnostic {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    /// How many times the run raised it: identical diagnostics at one place
    /// are one row.
    pub count: u32,
}

/// What a run observed: [`Definition`]s, [`Expansion`]s, [`Occurrence`]s and
/// [`Load`]s, which every query and lint reads.
#[derive(Default)]
pub struct Facts {
    pub defs: Vec<Definition>,
    /// The files `\write` handed lines to, as `\openout` named them.
    pub written_files: std::collections::BTreeSet<String>,
    /// What each command was seen to read, as `{}` for a mandatory argument,
    /// `[]` for an optional one and `*` for a star: observed while it ran,
    /// not declared anywhere.
    pub shapes: std::collections::HashMap<Sym, String>,
    pub expansions: Vec<Expansion>,
    pub loads: Vec<Load>,
    pub occurrences: Vec<Occurrence>,
    pub diagnostics: Vec<Diagnostic>,
    /// What each `\csname` site formed its control sequence for, the first
    /// time it ran.
    pub csnames: std::collections::HashMap<Span, CsnameRole>,
    /// `\begingroup … \endgroup` read straight from a file over several
    /// lines: where each opened and where it closed.
    pub groups: Vec<(Span, Span)>,
    /// Real `\errmessage`s the run reached, attributed to every name on the
    /// call stack at the time (`explain` filters by the one it was asked
    /// about); deduplicated by name, text and context.
    pub raised: Vec<RaisedError>,
    /// The modes each space token was read in outside package code
    /// (tex.web § 1043: glue in horizontal mode, nothing otherwise).
    pub spaces: std::collections::HashMap<Span, crate::mode::Modes>,
    /// Every conditional the run evaluated outside package code.
    pub conditionals: Vec<Conditional>,
}

/// A conditional the run evaluated (tex.web §§ 487-510), merged over every
/// time it ran.
#[derive(Clone, Debug)]
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
}

/// What a `\csname…\endcsname` was formed for, read off the command an
/// `\expandafter` held back while the name was formed (tex.web § 368).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
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
