//! The mouth, the gullet and the stomach.
//!
//! `Machine` reads tokens from an input stack, expands macros, executes the
//! primitives `builtins` declares, and maintains the save stack.  Where TeX
//! would need runtime information the interpreter abstracts: an undecided
//! conditional is analyzed branch by branch and the resulting environments are
//! merged, and a macro that re-enters itself more often than
//! `Config::max_expansions` is widened instead of unfolded.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::builtins::{
    category, initial_meanings, kernel_meanings, pdf_string_skip, DefMode, LoadKind,
    OccKind, Primitive, Typeset,
};
use crate::config::Config;
use crate::env::{Binding, Captured, Env, GroupKind, NodeId};
use crate::facts::{
    Definition, Diagnostic, Expansion, Facts, Load, LoadStatus, MeaningKind, Occurrence, Severity,
};
use crate::graph::{CallGraph, ControlDep, DependencyGraph, EdgeKind, VertexTag};
use crate::format::{self, Format};
use crate::loader::Resolver;
use crate::plugin::Plugins;
use crate::project::Project;
use crate::timing::{Phase, Timings};
use crate::tex::{
    EndLineChar,
    text_of, ArgSpec, Catcode, CatcodeTable, FileId, Interner, MacroDef, Meaning, Mouth,
    ParamItem, ParameterText, Span, Sym, Tok, Token,
};

#[path = "package_cache.rs"]
mod package_cache;
pub use package_cache::prune as prune_caches;

/// One kind of thing the interpreter did, recorded as an [`Event`] in the
/// trace: expand a macro, execute a primitive, open or close a group, and so
/// on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    Expand,
    Execute,
    Define,
    Assign,
    OpenGroup,
    CloseGroup,
    Condition,
    Branch,
    OpenFile,
    CloseFile,
    Widen,
    Undefined,
}

impl Step {
    pub const ALL: [Step; 12] = [
        Step::Expand,
        Step::Execute,
        Step::Define,
        Step::Assign,
        Step::OpenGroup,
        Step::CloseGroup,
        Step::Condition,
        Step::Branch,
        Step::OpenFile,
        Step::CloseFile,
        Step::Widen,
        Step::Undefined,
    ];

    /// The step a record's `step` field names.
    pub fn named(name: &str) -> Option<Step> {
        Step::ALL.into_iter().find(|s| s.as_str() == name)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Step::Expand => "expand",
            Step::Execute => "execute",
            Step::Define => "define",
            Step::Assign => "assign",
            Step::OpenGroup => "open-group",
            Step::CloseGroup => "close-group",
            Step::Condition => "condition",
            Step::Branch => "branch",
            Step::OpenFile => "open-file",
            Step::CloseFile => "close-file",
            Step::Widen => "widen",
            Step::Undefined => "undefined",
        }
    }
}


/// One entry of the execution trace: what [`Step`] happened, to which name,
/// at which [`Span`]. [`Analysis::trace`] holds the sequence `satex trace`
/// prints.
pub struct Event {
    pub index: u32,
    pub depth: u16,
    pub kind: Step,
    pub name: Sym,
    pub span: Span,
    pub detail: Option<Box<str>>,
}

/// A loaded file: its path and its [`LoadKind`] (input, package, class).
/// [`Analysis::files`] is indexed by [`FileId`].
#[derive(Clone)]
pub struct FileInfo {
    pub path: String,
    pub kind: LoadKind,
}

/// Where the LaTeX2e kernel came from for this run: the `latex.ltx` path,
/// whether it was read from a [`Format`] cache, and how far interpretation of
/// it got.
pub struct FormatInfo {
    pub source: String,
    pub definitions: usize,
    pub cached: bool,
    /// The cache file this format was read from or written to.
    pub cache: Option<String>,
    /// The last line of the format that produced a definition, and how many
    /// lines it has.  A large gap means the kernel was only partly read, and
    /// names defined after that point will look undefined.
    pub reached: u32,
    pub lines: u32,
}

impl FormatInfo {
    pub fn complete(&self) -> bool {
        self.lines == 0 || self.reached * 10 >= self.lines * 9
    }
}

/// One of the document's properties as hyperref writes it into the PDF.
/// `text` is what `\pdfstringdef` makes of the value, `source` the token
/// list it was made from, which the rendering drops parts of.
#[derive(Clone, Debug)]
pub struct Metadata {
    /// `title`, `author` or `date`.
    pub field: &'static str,
    pub text: String,
    pub source: String,
}

/// Per name, the probes run in each context it was called from.
pub type ContextProbes = HashMap<Sym, Vec<(Option<Sym>, Option<String>)>>;

/// Everything a run produced: the [`Env`] and [`CatcodeTable`] it ended with,
/// the [`Facts`] and [`DependencyGraph`] it recorded, and the bookkeeping every
/// `satex` command reads instead of the live [`Machine`].
pub struct Analysis {
    pub interner: Interner,
    pub graph: DependencyGraph,
    pub calls: CallGraph,
    /// Which macro's replacement text each expansion was written in, as the
    /// run met them: the relation recursion is read from.
    pub observed: CallGraph,
    pub files: Vec<FileInfo>,
    /// The file the user asked about, as opposed to everything it loads.
    pub main_file: FileId,
    /// Whether that file is a literate source, and which kind: what satex
    /// read is then docstrip's view of it, not the file itself.
    pub literate: Option<crate::literate::Literate>,
    /// The build-system files that surround the document.
    pub project: Project,
    /// The provider, platform and output target this run assumed.
    pub plugins: Plugins,
    /// The TeX installation the run resolved files against.
    pub distribution: crate::distribution::Distribution,
    /// Group depth at `\begin{document}`, the baseline for "this definition
    /// is local to a group".
    pub document_depth: Option<u16>,
    /// For each file, the position in the main file at which its contents
    /// become visible.  A definition in a loaded file is in scope from there.
    pub entry: Vec<Option<Span>>,
    /// Where the LaTeX2e kernel came from, and whether it was cached.
    pub format: Option<FormatInfo>,
    /// The `satex.yaml` in force, when one was read, and the settings it made.
    pub config: Option<PathBuf>,
    pub settings: Config,
    /// `\title`, `\author` and `\date` as the PDF's document properties
    /// show them, each with the token list it was made from.
    pub metadata: Vec<Metadata>,
    /// Where the kernel defined each name it brought in, so a name that has
    /// no definition site in this run still points at `latex.ltx`.
    pub kernel_sites: HashMap<Sym, Span>,
    /// The meaning of every control sequence at the end of the run.
    pub env: Env,
    /// The category codes in force at the end of the run.
    pub catcodes: CatcodeTable,
    pub timings: Timings,
    pub facts: Facts,
    pub trace: Vec<Event>,
    /// The characters typeset from the lines a filtered trace covers.
    pub typeset: String,
    pub steps: u64,
    pub exhausted: bool,
    /// Final sizes of the interpreter's working sets.
    pub journal_size: usize,
    pub held_tokens_left: usize,
    /// The package caches this run read (`true`) or wrote: package, file.
    /// The package caches this run met: package, cache file, and whether it
    /// was `cached` (read), `stored`, or `stale: …` and read again.
    pub package_caches: Vec<(String, String, String)>,
    /// Files whose frame was, at least in part, served from a package cache
    /// replay rather than read token by token: what `summary --timings`
    /// marks as cached instead of pricing at zero.
    pub cached_files: std::collections::HashSet<FileId>,
    /// The bindings and category codes at `\begin{document}`, for probing
    /// what a command takes in the preamble.
    pub preamble: Option<Box<(Env, CatcodeTable)>>,
    /// What [`crate::probe`] found per name and view, computed on demand.
    pub probes: std::sync::Mutex<HashMap<(Sym, bool), Option<crate::probe::Probed>>>,
    /// The environments open right now, outermost first, as `\@currenvir`
    /// tracks the innermost one (`document` excluded: `document_depth`
    /// already answers that axis).  What `explain` reads to tell a
    /// definition made inside `enumerate` from one made outside any
    /// environment.
    pub env_stack: Vec<Sym>,
    /// Environments worth probing a name inside, and what that probe found
    /// there, computed once per name: [`crate::probe::probe_contexts`]'s
    /// cache, for a run with no real use to read the difference off of.
    pub context_probes: std::sync::Mutex<ContextProbes>,
}

/// Peak resident set size in kibibytes, where the platform reports it.
pub fn peak_memory() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|l| l.strip_prefix("VmHWM:"))
        .and_then(|v| v.split_whitespace().next()?.parse().ok())
}

/// A macro found to reach itself.
pub struct Recursion {
    pub name: Sym,
    /// The names of the cycle, `name` among them.
    pub cycle: Vec<Sym>,
    /// The run expanded the cycle.  Otherwise it is read off the
    /// replacement texts of macros nothing expanded, which only says the
    /// names are there, not that expansion reaches them.
    pub observed: bool,
}

impl Analysis {
    /// Macros whose expansion was seen to reach themselves, and, for macros
    /// the run never expanded, the cycles their replacement texts name.
    pub fn recursion(&self) -> Vec<Recursion> {
        let expanded: std::collections::HashSet<Sym> = self.facts.expansions.iter().map(|e| e.name).collect();
        let in_document: std::collections::HashSet<Sym> =
            self.facts.defs.iter().filter(|d| d.span.file == self.main_file).map(|d| d.name).collect();
        let mut out: Vec<Recursion> = self
            .observed
            .recursive()
            .into_iter()
            .map(|(name, cycle)| Recursion { name, cycle, observed: true })
            .collect();
        for (name, cycle) in self.calls.recursive() {
            if cycle.iter().all(|s| !expanded.contains(s) && in_document.contains(s)) {
                out.push(Recursion { name, cycle, observed: false });
            }
        }
        out
    }

    /// For commands that read the source without interpreting it.
    pub fn empty() -> Analysis {
        let cfg = Config { load_format: false, use_kpsewhich: false, ..Config::default() };
        Machine::new(&cfg, PathBuf::new()).out
    }

    /// Is a definition made at `span` in scope at `line:col` of the main file?
    pub fn visible_at(&self, span: Span, line: u32, col: u32) -> bool {
        self.visible_in(span, self.main_file, line, col)
    }

    /// Is a definition made at `span` in scope at `line:col` of `file`?  Both
    /// climb to the file they share: a position in a subfile sees what its
    /// `\input` call saw, and a definition in a file read from a call sits
    /// where that call does.
    pub fn visible_in(&self, span: Span, file: FileId, line: u32, col: u32) -> bool {
        let entry = |id: FileId| self.entry.get(id as usize).copied().flatten();
        let (mut file, mut line, mut col) = (file, line, col);
        loop {
            if span.file == file {
                return span.before(line, col);
            }
            match entry(file) {
                Some(call) if file != self.main_file => (file, line, col) = (call.file, call.line, call.col),
                _ => break,
            }
        }
        // The definition's file is no ancestor of the position: it stands
        // where the call that read it does.
        match entry(span.file) {
            Some(call) if span.file != self.main_file => self.visible_in(call, file, line, col),
            _ => false,
        }
    }

    /// One line of resource accounting, for benchmarking and for the budgets.
    pub fn stats(&self, elapsed: std::time::Duration) -> String {
        fn compact(n: u64) -> String {
            match n {
                0..1_000 => n.to_string(),
                1_000..1_000_000 => format!("{:.1}k", n as f64 / 1e3),
                1_000_000..1_000_000_000 => format!("{:.1}M", n as f64 / 1e6),
                _ => format!("{:.1}G", n as f64 / 1e9),
            }
            .replace(".0k", "k")
            .replace(".0M", "M")
            .replace(".0G", "G")
        }
        let facts = &self.facts;
        let c = |n: usize| compact(n as u64);
        let peak = peak_memory().map_or(String::new(), |kb| format!("  {} MiB peak", kb / 1024));
        let exhausted = if self.exhausted { "  |  budget exhausted" } else { "" };
        [
            format!("{:.3}s{peak}{exhausted}", elapsed.as_secs_f64()),
            format!("{} files  {} tokens", c(self.files.len()), compact(self.steps)),
            format!(
                "{} definitions  {} expansions  {} occurrences  {} diagnostics",
                c(facts.defs.len()),
                c(facts.expansions.len()),
                c(facts.occurrences.len()),
                c(facts.diagnostics.len())
            ),
            format!("{} vertices  {} edges", c(self.graph.len()), c(self.graph.edge_count())),
            format!("{} names  {} save-stack  {} held", c(self.interner.len()), c(self.journal_size), c(self.held_tokens_left)),
        ]
        .join("  |  ")
    }

    pub fn file_name(&self, id: FileId) -> &str {
        self.files.get(id as usize).map_or("<input>", |f| f.path.as_str())
    }
    pub fn short_name(&self, id: FileId) -> &str {
        let path = self.file_name(id);
        Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path)
    }
    pub fn file_names(&self) -> Vec<String> {
        self.files.iter().map(|f| f.path.clone()).collect()
    }
}

#[derive(Clone)]
enum Frame {
    /// A file being read.  `catcodes` is the table to restore when it ends:
    /// `\usepackage` makes `@` a letter for the package only, and it does so
    /// without opening a TeX group, because a package has to be able to
    /// define things.  `depth` is the group nesting the file started at, so
    /// that a group it leaves open can be reported and given up.
    File {
        mouth: Mouth,
        package: Option<Sym>,
        catcodes: Option<Box<CatcodeTable>>,
        depth: usize,
        /// The conditional nesting the file started at, so that one it leaves
        /// open does not govern the rest of the run.
        conds: usize,
        /// Whether `\everyeof` was inserted at its end (etex.ch `eof_seen`).
        eof_seen: bool,
    },
    Tokens { toks: Rc<[Token]>, pos: usize, expanding: Option<Sym> },
    /// A `\scantokens` pseudo file.  Its characters are tokenized as they
    /// are read, so a catcode change inside it governs the rest of it
    /// (etex.ch `pseudo_input`); `eof` is the `\everyeof` read at its end.
    Pseudo { mouth: Box<Mouth>, at: Span, eof: Rc<[Token]>, pos: usize },
    /// Tokens a lookahead read and gave back, newest last.  It lives on the
    /// input stack so that a file opened after the lookahead is still read
    /// before them, which is where TeX inserts it.
    Returned(Vec<Token>),
    /// Nothing is read across this: it ends a token list that is being run to
    /// completion on its own, such as an option body or a conditional arm.
    Boundary,
}

/// The `\global`, `\long`, `\outer` and `\protected` prefixes waiting to
/// apply to the next definition or assignment; [`Machine::define`] consumes
/// and resets them.
#[derive(Default, Clone, Copy)]
pub struct Prefixes {
    pub global: bool,
    pub long: bool,
    pub outer: bool,
    pub protected: bool,
    /// `\immediate`: the next `\write`, `\openout` or `\closeout` acts now
    /// (tex.web § 1375).
    pub immediate: bool,
}

/// How a conditional level ends (tex.web § 489: `if_limit`); [`CondFrame::limit`]
/// holds it while the level is open.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CondLimit {
    /// The `\else` of this level is still to come.
    Else,
    /// An `\or` or the `\else` of an `\ifcase`.
    Or,
    /// Only `\fi` may still turn up.
    Fi,
}

/// One entry of the condition stack ([`Machine::conds`]), tracking whether
/// `\else` is still to come and whether the branch taken is decided yet.
#[derive(Clone, Copy, Debug)]
pub struct CondFrame {
    pub limit: CondLimit,
    /// An undecided condition runs every arm in turn, so `\else` falls
    /// through instead of skipping to `\fi`.
    pub undecided: bool,
    /// Whether this level pushed a control dependency.
    pub dep: bool,
    /// Where the `\if…` stood, so that its `\fi` can close the construct.
    pub at: Span,
    /// e-TeX's `\currentiftype`, `None` where satex cannot tell.
    pub kind: Option<i8>,
    /// For an undecided condition whose arms run in turn: the modes each
    /// arm starts in and those the arms left.
    pub modes: Option<crate::mode::ArmModes>,
}

/// tex.web § 305 `scanner_status`: what a scan that meets the end of a
/// file is in the middle of.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scanner {
    Normal,
    Skipping,
    Defining,
    Matching,
    Absorbing,
}

/// How deeply expansion may nest inside scans before the run stops.
const MAX_NESTING: u32 = 300;

/// How much of the Rust stack nested steps may take before the run stops,
/// whatever each level costs: half the 2 MiB a spawned thread gets.
const MAX_STACK_BYTES: usize = 1 << 20;

/// Prefix for the names under which registers addressed by number are kept.
const REGISTER: char = '\u{2}';

/// The interpreter itself: it reads [`Token`]s from the input stack, expands
/// macros and executes primitives, changes [`Env`] and [`CatcodeTable`], and
/// records what it saw into [`Analysis`].
/// A text copied through into an `\edef` body ([`Machine::copy_through`]):
/// whose, where in the body and how long; how much longer it could have been
/// within the limits it met; and whether it came from before the load being
/// recorded.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CopyEvent {
    pub sym: Sym,
    pub at: usize,
    pub len: usize,
    pub room: i64,
    pub fresh: bool,
}

/// `\openin` streams: the lines each still has to give, by stream number.
type Streams = Rc<HashMap<i64, (Rc<Vec<String>>, usize)>>;

pub struct Machine<'a> {
    pub cfg: &'a Config,
    pub out: Analysis,
    pub env: Env,
    pub catcodes: CatcodeTable,
    /// LuaTeX's catcode tables and what the Lua chunks read.
    pub lua: crate::plugin::lua::Lua,
    /// `\endlinechar`, which expl3 changes.
    pub(crate) end_line: EndLineChar,
    input: Vec<Frame>,
    pub prefixes: Prefixes,
    /// tex.web § 1269 (`after_token`): the one token `\afterassignment`
    /// holds until the next assignment is complete.  A second
    /// `\afterassignment` before that assignment overwrites it.
    pub after_assignment: Option<Token>,
    /// tex.web § 389 (`long_state`): whether the macro whose arguments are
    /// being read was declared `\long`, and so may have `\par` in them.
    /// Outside a macro call nothing is restricted.
    long_argument: bool,
    /// Set when a `\par` aborted the argument scan of a macro that is not
    /// `\long` (tex.web § 396), which abandons the call.
    runaway: bool,
    pub cds: Vec<ControlDep>,
    /// The conditionals being read, innermost last (tex.web § 489).
    pub conds: Vec<CondFrame>,
    /// For each conditional whose test is still being read, how many levels
    /// stood below it: its level's `if_limit` is `if_code` (tex.web § 489).
    pub testing: Vec<usize>,
    /// The `\currentiftype` of each conditional whose test is being read.
    pub testing_kind: Vec<Option<i8>>,
    /// Environments declared verbatim while the run went on, beside the ones
    /// the configuration names: `\newtcblisting` makes one.
    pub verbatim_environments: std::collections::HashSet<String>,
    /// The innermost sectioning unit seen so far.  The interpreter runs in
    /// document order, so the last `\section`-family occurrence is the one
    /// everything recorded after it stands in.
    section: Option<Rc<str>>,
    /// `\openin` streams: the lines each one still has to give.
    streams: Streams,
    /// The `\usedir` of a docstrip batch file: where what it generates next
    /// belongs in a distribution's tree.
    pub docstrip_dir: Option<String>,
    /// Whether the docstrip commands are the ones satex models.
    docstrip: bool,
    /// Names whose modeled meaning stands whatever a file defines: interfaces
    /// satex implements itself because their own definition is written in a
    /// dialect it would have to typeset to follow.
    pinned: std::collections::HashSet<Sym>,
    /// The macro whose replacement text is currently being read.
    within: Vec<(Sym, NodeId)>,
    pub(crate) quantity: crate::observe::Quantities,
    /// The macro whose expansion the last token read came from; a list
    /// pushed or given back while it is processed keeps it.  `None`: a file.
    source: Option<Sym>,
    /// Package context: the name and the options it was given.
    packages: Vec<(Sym, Vec<String>)>,
    /// How often each control sequence is currently being expanded.
    pub(crate) expanding: Vec<u16>,
    pub loaded: Vec<String>,
    /// Options given to `\documentclass`, which packages also see.
    pub class_options: Vec<String>,
    /// The load whose `\Provides…` line has not been seen yet.
    identifying: Option<usize>,
    base: PathBuf,
    resolver: Resolver,
    pub(crate) file_depth: u16,
    branch_depth: u16,
    /// The undecided conditionals whose paths are running, innermost last,
    /// with the arm the running path entered and the source progress at
    /// the split.
    pub(crate) split_at: Vec<(Span, usize, u64)>,
    /// The loop heads whose paths are running, outermost first: see
    /// [`LoopHead`].
    heads: Vec<LoopHead>,
    /// The abstract number a scan just read from a register whose value
    /// is not known (a [`crate::value::Value::Range`]): what a test compares
    /// by interval, and an assignment stores.  With it, the register read
    /// as the whole value and a test on it narrows it on each arm.
    pub(crate) abs: Option<crate::value::Num>,
    /// The register that interval was read from, and whether it is a
    /// dimension register.
    pub(crate) abs_from: Option<(Sym, bool)>,
    pub(crate) abs_whole: bool,
    /// An undecided `\ifnum`/`\ifdim` of such a register against a known
    /// number: the register, whether a dimension, its interval, the
    /// relation and the bound, from which each arm learns.
    pub(crate) narrowing: Option<(Sym, bool, crate::value::Num, char, i64)>,
    /// Where each diagnostic was first raised, by code, place and message
    /// hash, so that a repeat is counted and not stored again.
    diag_seen: HashMap<(&'static str, Span, u64), usize>,
    /// Diagnostics before this index belong to a finished cache segment and
    /// are not counted into.
    pub(crate) diag_floor: usize,
    pub(crate) halted: bool,
    /// Replaces `Config::max_steps` while the format is being read.
    budget: Option<u64>,
    /// Tokens the input stack currently holds, so that deep nesting of large
    /// replacement texts cannot grow without bound.
    held_tokens: usize,
    /// tex.web § 358: the token the last read took from behind a
    /// `\noexpand` marker (`frozen_dont_expand`), which the gullet then treats
    /// as `\relax` with `no_expand_flag`, however it is defined.
    pub(crate) noexpanded: Option<Token>,
    /// LuaTeX's `\lastnamedcs`: the name the last `\csname`,
    /// `\begincsname` or `\ifcsname` built.
    pub(crate) last_named_cs: Option<Sym>,
    /// The marker `\noexpand` puts in front of the token it guards.
    pub(crate) dont_expand: Sym,
    /// A token standing for text satex cannot know, such as what
    /// `\pdfuniformdeviate` expands to: it has an unknown meaning, so a
    /// value scanned from it is unknown, and so is text built with it.
    pub(crate) unknown: Sym,
    /// The rest of unknown text a delimited argument was taken to end in:
    /// unknown too, but a scan meeting it takes it as holding no delimiter,
    /// so that splitting unknown text happens once and a loop over it ends.
    pub(crate) unknown_rest: Sym,
    /// Digits satex does not know, such as `\the\count0` of an unknown
    /// count: unknown text to a scan, but character tokens of category 12
    /// to `\ifx`, `\if`, `\ifcat`, `\let` and `\futurelet` (their
    /// meaning is [`UNKNOWN_CHAR`](crate::tex::UNKNOWN_CHAR)).
    pub(crate) unknown_digits: Sym,
    /// Unknown digits that may be none: what is left of unknown digits
    /// once one of them was taken as a token.
    pub(crate) unknown_more: Sym,
    /// The `edef_depth` of the expansion that makes a definition's
    /// replacement text: what `\the` and `\unexpanded` give it is stored as
    /// it stands (tex.web § 478), so a `#` among it is no parameter.
    def_body_depth: Option<usize>,
    /// Fact index by source site, so that re-analysing a file does not
    /// duplicate what it observed.
    definition_sites: HashMap<(Span, Sym), usize>,
    /// The switches the document's own files declare (`\newif` and the
    /// like): the ones `satex controls` leaves undecided without a name.
    pub(crate) own_switches: HashSet<Sym>,
    expansion_sites: HashMap<(Span, Sym), Site>,
    /// Each evaluated conditional's fact, by its `\if…`.
    cond_sites: HashMap<Span, usize>,
    /// Tokens taken from a file so far: the measure of progress through the
    /// source that tells a loop from a helper called again further down.
    source_progress: u64,
    /// The call being described right now: a control sequence taken straight
    /// from a file, where it stood, and the arguments it has since read from
    /// that same file.  Arguments a macro's body reads from itself are not
    /// part of the call, so only reads from the file count, which is what
    /// makes `\section*[short]{title}` come out of `\@ifstar` and friends
    /// without naming them.
    call_shape: Option<(Sym, Span, String, usize)>,
    /// How deep the run is inside an `\edef` body: a conditional there makes
    /// text, not definitions, so it is resolved to one arm rather than
    /// analyzed arm by arm, which would reach past the body.
    pub edef_depth: usize,
    /// The name an `\edef` body being scanned defines, and the depth the
    /// body is scanned at: its own text met there is copied through.
    pub(crate) copy_target: Option<(Sym, usize)>,
    /// Texts copied through into the bodies being scanned.
    pub(crate) copies: Vec<CopyEvent>,
    /// How many value scans (⟨number⟩, ⟨dimen⟩, ⟨glue⟩) are running, and
    /// whether one of them met a condition it could not decide: the arms of
    /// such a condition are operands, so one is read and the value is
    /// unknown, rather than the others running as commands.
    pub scanning: usize,
    pub scan_undecided: bool,
    /// Gaps already reported, so one unmodeled name is named once.
    gaps: HashSet<(&'static str, Sym)>,
    /// Whether the current expansion is building a PDF string, where even a
    /// robust command is expanded: that is why hyperref needs
    /// `\texorpdfstring` at all (hyperref manual, "Special characters").
    pdf_string: bool,
    /// The file the token just read came from, or `None` when it came from a
    /// macro body.
    pub(crate) last_file: Option<FileId>,
    /// Where the file tokens given back and not yet read again stood.
    recent_file_spans: Vec<Span>,
    /// The last control sequence taken straight from a file, and where it
    /// stood: the call everything its expansion does is part of.
    pub file_call: Option<(Sym, Span)>,
    /// The last control sequence run whose token stands in a project file,
    /// a macro body of the project's included: where its own text put what
    /// the call `file_call` does.
    project_site: Option<Span>,
    /// The file call each open file was pushed under: when the file ends,
    /// the call that read it is the file call again.
    file_calls: Vec<(FileId, Option<(Sym, Span)>)>,
    /// The lines `\write`s handed files and the names body calls read,
    /// related once the run ends (`observe::relate`).
    pub written_lines: Vec<crate::observe::WrittenLine>,
    pub name_reads: Vec<crate::observe::NameRead>,
    /// A copy of the kernel state at `\begin{document}` that written lines
    /// run in; `Some(None)` once none could be made.
    pub line_sandbox: Option<Option<Box<Machine<'a>>>>,
    /// In that copy: the names the running line defined.
    pub line_defined: Option<Vec<Sym>>,
    /// The call whose argument parts [`crate::observe::defined_name`] last
    /// computed, and those parts.
    pub call_parts: Option<(Span, Span, std::rc::Rc<[String]>)>,
    /// Names document calls defined from their arguments: the name, the key,
    /// the calling command and the call.
    pub key_names: Vec<(String, String, Sym, Span, crate::observe::OccContext)>,
    /// The file each open `\openout` stream writes.
    pub stream_files: HashMap<i64, String>,
    /// The prefix of LaTeX's counter registers, once learnt.
    counter_namespace: Option<String>,
    /// What each `\expandafter` being carried out holds back, innermost
    /// last, with the token it expands meanwhile (tex.web § 368).
    pub held: Vec<(Token, Option<Token>)>,
    /// Set while the main loop, not a reader, expands the token in hand: only
    /// a conditional met there can run its arms as paths of their own.
    pub command_level: bool,
    /// The cacheable load whose file is being read, and the state it began in.
    package_capture: Option<Box<package_cache::Recorder>>,
    /// The file nesting the document itself is read at: loads it makes
    /// directly are the ones a package cache serves.
    document_file_depth: u16,
    /// Set while a read pops the frame it ran out of: that read has counted
    /// the token it goes on to return from the frame below.
    popping_in_read: bool,
    /// Set while the main loop reads the token it will act on: a package
    /// file that ends there ends at rest.
    reading_command: bool,
    /// The token count when the run last read from a file, which tells a
    /// loop that makes no progress from a long one that does.
    /// The trace reached `limits.trace` and its truncation was reported.
    trace_full: bool,
    progress_step: u64,
    started: std::time::Instant,
    /// The modes the run may be in, and those of the list the current
    /// paragraph belongs to (tex.web § 211).
    pub(crate) mode: crate::mode::Modes,
    pub(crate) list: crate::mode::Modes,
    /// Whether anything may have been put on a list: until then e-TeX's
    /// `\lastnodetype` of the empty main vertical list is -1.
    pub(crate) material: bool,
    /// The natural size of the current list, where known.
    pub(crate) natural: Option<crate::mode::Natural>,
    /// The names a definition of a name built from unknown text may have
    /// made: one that seems undefined and matches may be defined.
    pub(crate) wild: Wild,
    /// How deeply `step` is nested on the Rust stack.
    step_depth: u32,
    /// Where on the stack the outermost `step` runs.
    stack_base: usize,
    /// Work done without reading a token — copying, comparing and joining
    /// the paths of undecided conditionals — in tokens' worth: it counts
    /// against `limits.steps` too, so no loop outside the reader escapes it.
    pub(crate) work: u64,
    pub(crate) scanner: Scanner,
    /// What a `\scantokens` pseudo file is expanded as, so that its end
    /// can be told (see [`Machine::bump_expanding`]).
    pub(crate) pseudo_file: Sym,
    /// Set in a sandbox run by [`crate::probe`]: what it watches for.
    pub(crate) probe: Option<Box<crate::probe::Watch>>,
}

/// Everything besides the bindings that a path through an undecided
/// conditional changes, so that each arm starts where the test was.  What
/// the run records — facts, the graph, hooks, loaded files — is shared: the
/// analysis reports the union of the paths.
#[derive(Clone)]
struct PathState {
    input: Vec<Frame>,
    catcodes: CatcodeTable,
    end_line: EndLineChar,
    prefixes: Prefixes,
    after_assignment: Option<Token>,
    long_argument: bool,
    runaway: bool,
    cds: Vec<ControlDep>,
    conds: Vec<CondFrame>,
    within: Vec<(Sym, NodeId)>,
    packages: Vec<(Sym, Vec<String>)>,
    held_tokens: usize,
    section: Option<Rc<str>>,
    call_shape: Option<(Sym, Span, String, usize)>,
    edef_depth: usize,
    pdf_string: bool,
    last_file: Option<FileId>,
    file_call: Option<(Sym, Span)>,
    source_progress: u64,
    file_depth: u16,
    pub(crate) halted: bool,
    streams: Streams,
    loaded: Vec<String>,
    mode: crate::mode::Modes,
    list: crate::mode::Modes,
    material: bool,
    natural: Option<crate::mode::Natural>,
    wild: Wild,
}

/// A split — an undecided conditional, a name of several meanings, a mode
/// test — that a path it began may meet again with nothing read from the
/// source since: a loop whose exit is undecided.  It keeps the join of
/// every abstract state the loop reached there, so that the loop is
/// analyzed to a fixpoint (§ 14).
struct LoopHead {
    span: Span,
    progress: u64,
    /// The env trail where the head split: what changed since is what the
    /// loop did.
    trail: usize,
    /// Where the head stands in the input, and the shape of the
    /// conditional and group stacks there: a path that meets the head
    /// elsewhere (a recursion that is not a tail call, a loop that reads
    /// on in its input) has something else to do after the loop.
    places: Vec<Place>,
    conds: usize,
    groups: usize,
    catcodes: CatcodeTable,
    /// Every slot the loop changed: what it held at the head, and the join
    /// of what it held at every visit.
    joined: HashMap<u32, (Option<crate::env::Binding>, Option<crate::env::Binding>)>,
    modes: crate::mode::Modes,
    /// A path met the head with a state that grew `joined`.
    grown: bool,
    /// How often the head's paths were analyzed again.
    rounds: u32,
}

/// One path through an undecided conditional.
struct Arm {
    state: PathState,
    env: crate::env::PathEnv,
    /// It reached `\end`, or the end of the input it may read.
    ended: bool,
    /// Which arm of the conditional the path entered.
    arm: usize,
    /// The source progress where the paths split.
    progress: u64,
}

/// Where a path stands in the input, for telling when two paths meet: a
/// token list by identity and position, a file by position, tokens given
/// back by content.  Exhausted lists are left out, since reading on pops them.
#[derive(PartialEq, Debug)]
enum Place {
    File(FileId, usize, bool),
    Tokens(usize, usize),
    Returned(Vec<Token>),
    Boundary,
}

/// What one call site has done so far.
struct Site {
    fact: usize,
    /// Expansions since the last one that read something new from a file.
    repeats: u32,
    progress: u64,
}

impl<'a> Machine<'a> {
    pub fn new(cfg: &'a Config, base: PathBuf) -> Self {
        let project = Project::discover(&base);
        let plugins = Plugins::resolve(cfg, &project);
        // TEXINPUTS etc., in kpathsea's own precedence: environment, then
        // the project's `latexmkrc`/Makefile, then `satex.yaml`'s `paths.*`.
        let kpse_paths = crate::paths::effective_all(&project, &cfg.paths, &project.root);
        let texinputs = kpse_paths
            .iter()
            .find(|e| e.variable == "TEXINPUTS")
            .map(|e| e.dirs.clone())
            .unwrap_or_default();
        let search = cfg
            .search_paths
            .iter()
            .cloned()
            .chain(project.search_paths())
            .chain(texinputs)
            .collect();
        let mut request = cfg.distribution_request();
        request.kpse_env = crate::paths::env_vars(&kpse_paths);
        let resolver = Resolver::new(search, request).with_unpacked(project.unpacked());
        let engine = plugins.engine;
        let mut interner = Interner::default();
        // Sym(0) is the empty name, used where an event has no subject.
        interner.intern("");
        // No file can spell this name with the catcodes a run starts with.
        let dont_expand = interner.intern("\u{2}notexpanded:");
        let unknown = interner.intern("\u{2}unknown:");
        let unknown_rest = interner.intern("\u{2}unknown rest:");
        let unknown_digits = interner.intern("\u{2}unknown digits:");
        let unknown_more = interner.intern("\u{2}unknown more digits:");
        let pseudo_file = interner.intern("\u{2}pseudo file:");
        let table = kernel_meanings(&mut interner, engine);
        let mut env = Env::with_capacity(interner.len());
        for (sym, meaning) in table {
            env.set(sym, Binding::builtin(meaning), true);
        }
        env.set(unknown, Binding::builtin(Meaning::Unknown), true);
        env.set(unknown_rest, Binding::builtin(Meaning::Unknown), true);
        env.set(unknown_digits, Binding::builtin(Meaning::Char(crate::tex::UNKNOWN_CHAR, Catcode::Other)), true);
        env.set(unknown_more, Binding::builtin(Meaning::Char(crate::tex::UNKNOWN_CHAR, Catcode::Other)), true);
        // No format yet: INITEX's table; a format installs its own.
        let mut catcodes = CatcodeTable::initex();
        catcodes.at_letter(cfg.at_letter);
        Self {
            cfg,
            out: Analysis {
                config: cfg.source.clone(),
                settings: cfg.clone(),
                metadata: Vec::new(),
                kernel_sites: HashMap::new(),
                interner,
                graph: DependencyGraph::default(),
                calls: CallGraph::default(),
                observed: CallGraph::default(),
                files: Vec::new(),
                literate: None,
                main_file: 0,
                project,
                plugins,
                distribution: crate::distribution::Distribution {
                    index_cache: None,
                    roots: Vec::new(),
                    discovery: crate::distribution::Discovery::None,
                    year: None,
                    format_version: None,
                    indexed: 0,
                    index_cached: false,
                },
                document_depth: None,
                entry: Vec::new(),
                format: None,
                env: Env::default(),
                catcodes: CatcodeTable::latex(),
                timings: Timings::new(cfg.timings),
                facts: Facts::default(),
                trace: Vec::new(),
                typeset: String::new(),
                steps: 0,
                exhausted: false,
                journal_size: 0,
                held_tokens_left: 0,
                package_caches: Vec::new(),
                cached_files: Default::default(),
                preamble: None,
                probes: Default::default(),
                env_stack: Vec::new(),
                context_probes: Default::default(),
            },
            env,
            catcodes,
            end_line: EndLineChar::default(),
            input: Vec::new(),
            prefixes: Prefixes::default(),
            after_assignment: None,
            long_argument: true,
            runaway: false,
            cds: Vec::new(),
            within: Vec::new(),
            quantity: crate::observe::Quantities::default(),
            source: None,
            packages: Vec::new(),
            expanding: Vec::new(),
            loaded: Vec::new(),
            class_options: Vec::new(),
            identifying: None,
            resolver,
            base,
            file_depth: 0,
            branch_depth: 0,
            split_at: Vec::new(),
            heads: Vec::new(),
            abs: None,
            abs_from: None,
            abs_whole: false,
            narrowing: None,
            diag_seen: HashMap::new(),
            diag_floor: 0,
            conds: Vec::new(),
            testing: Vec::new(),
            testing_kind: Vec::new(),
            verbatim_environments: std::collections::HashSet::new(),
            section: None,
            streams: Rc::default(),
            docstrip_dir: None,
            docstrip: false,
            lua: Default::default(),
            pinned: std::collections::HashSet::new(),
            halted: false,
            budget: None,
            held_tokens: 0,
            noexpanded: None,
            last_named_cs: None,
            dont_expand,
            unknown,
            unknown_rest,
            unknown_digits,
            unknown_more,
            def_body_depth: None,
            definition_sites: HashMap::new(),
            own_switches: HashSet::new(),
            expansion_sites: HashMap::new(),
            cond_sites: HashMap::new(),
            source_progress: 0,
            call_shape: None,
            edef_depth: 0,
            copy_target: None,
            copies: Vec::new(),
            scanning: 0,
            scan_undecided: false,
            gaps: HashSet::new(),
            pdf_string: false,
            last_file: None,
            recent_file_spans: Vec::new(),
            file_call: None,
            project_site: None,
            file_calls: Vec::new(),
            written_lines: Vec::new(),
            name_reads: Vec::new(),
            line_sandbox: None,
            line_defined: None,
            stream_files: HashMap::new(),
            call_parts: None,
            key_names: Vec::new(),
            counter_namespace: None,
            held: Vec::new(),
            command_level: false,
            package_capture: None,
            document_file_depth: 0,
            popping_in_read: false,
            reading_command: false,
            progress_step: 0,
            started: std::time::Instant::now(),
            mode: crate::mode::Modes::VERTICAL,
            list: crate::mode::Modes::VERTICAL,
            step_depth: 0,
            stack_base: 0,
            work: 0,
            scanner: Scanner::Normal,
            pseudo_file,
            probe: None,
            material: false,
            natural: None,
            wild: Wild::default(),
            trace_full: false,
        }
    }

    pub fn analyze(source: &str, path: Option<&Path>, cfg: &'a Config) -> Analysis {
        let base = path
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let mut m = Machine::new(cfg, base);
        let engine = m.out.plugins.engine;
        // A `.dtx` is documentation with code in it, and docstrip decides
        // which is which; what satex interprets is the code, line for line
        // where it stands in the `.dtx`.  A `.ins` is a batch file and runs
        // as it is written.
        let literate = path.and_then(crate::literate::Literate::of);
        let stripped = (literate == Some(crate::literate::Literate::Dtx))
            .then(|| crate::literate::code_view(source, &crate::literate::Guards::AllButDriver));
        // The magic comments and what the plugins read stay with the file as
        // it was written: they live in comment lines, which is where a `.dtx`
        // keeps its documentation.
        let code = stripped.as_deref().unwrap_or(source);
        m.out.timings.start(Phase::Discovery);
        m.out.distribution = m.resolver.distribution().clone();
        m.out.timings.start(Phase::Format);
        let kind = match crate::config::Profile::of_name(path, code) {
            Some(crate::config::Profile::Package) => LoadKind::Package,
            Some(crate::config::Profile::Class) => LoadKind::Class,
            _ => LoadKind::Input,
        };
        let name = path.map_or_else(|| "<input>".to_string(), |p| p.display().to_string());
        let id = m.register_file(name, kind);
        m.out.main_file = id;
        m.out.literate = literate;
        if kind.is_package() {
            m.catcodes.at_letter(true);
        }
        if m.cfg.plugins.discovery {
            m.out.plugins.discovery =
                crate::plugin::discovery::scan_with(&m.base, m.cfg.limits.threads);
        }
        m.out.plugins.read_build(m.cfg, &m.out.project, path);
        if m.cfg.plugins.magic {
            m.out.plugins.read_magic(source);
        }
        m.out.plugins.read_kernel(m.cfg, source);
        // `Plugins::resolve` ran before the source was read, so a magic
        // comment may have named a different engine than the table installed
        // in `Machine::new` assumed.
        if m.out.plugins.engine != engine {
            let table = kernel_meanings(&mut m.out.interner, m.out.plugins.engine);
            // The other engine's own primitives are not there at all:
            // XeTeX has no `\pdftexversion`.
            for sym in kernel_meanings(&mut m.out.interner, engine).into_keys() {
                if !table.contains_key(&sym) {
                    m.env.set(sym, Binding::builtin(Meaning::Undefined), true);
                }
            }
            for (sym, meaning) in table {
                m.env.set(sym, Binding::builtin(meaning), true);
            }
        }
        if m.cfg.plugins.preload {
            let project = m.out.project.clone();
            let base = m.base.clone();
            let configured = m.cfg.preload.clone();
            m.out.plugins.read_preload(source, &project, &base, configured.as_deref());
        }
        m.install_parameters();
        m.install_clock();
        m.install_format();
        // The engine's `-interaction` overrides what the format dumped
        // (tex.web § 1337).
        let mode = m.intern("interactionmode");
        m.env.set_value(mode, crate::value::Value::Int(m.cfg.interaction as i64), true);
        m.install_output();
        if literate == Some(crate::literate::Literate::Ins) {
            m.install_docstrip();
        }
        m.out.timings.start(Phase::Document);
        m.document_file_depth = m.file_depth;
        m.out.timings.enter_file(id);
        m.input.push(Frame::File {
            mouth: Mouth::file(code, id),
            package: None,
            catcodes: None,
            depth: 0,
            conds: 0,
            eof_seen: false,
        });
        m.run();
        crate::observe::text_break(&mut m);
        m.out.timings.enter_file(id);
        m.out.timings.leave_file();
        m.out.timings.start(Phase::Hooks);
        m.out.journal_size = m.env.journal_len();
        m.out.held_tokens_left = m.held_tokens;
        if m.cfg.plugins.tools {
            let loads: Vec<(String, Vec<String>)> = m
                .out
                .facts
                .loads
                .iter()
                .filter(|load| load.kind.is_package())
                .map(|load| (load.name.clone(), load.options.clone()))
                .collect();
            let project = m.out.project.clone();
            m.out.plugins.read_tools(&loads, &project);
        }
        m.out.plugins.depp = crate::plugin::depp::Depp::read(&m.out);
        m.collect_metadata();
        m.out.timings.finish();
        m.out.calls.finish();
        m.out.observed.finish();
        crate::observe::relate(&mut m);
        m.link_cross_references();
        m.out.env = std::mem::take(&mut m.env);
        m.out.catcodes = m.catcodes.clone();
        m.out
    }

    /// Expand a value for reporting.  With no list, everything the gullet can
    /// expand is expanded, which is the text the engine would write; a list
    /// narrows that to the macros it names.
    fn expand_reported(&mut self, tokens: &[Token]) -> Vec<Token> {
        let allowed = self.cfg.expand_in_reports.clone();
        if allowed.is_empty() {
            return self.expand_tokens(Rc::from(tokens.to_vec()));
        }
        let mut out = Vec::with_capacity(tokens.len());
        for token in tokens {
            let Tok::Cs(sym) = token.tok else {
                out.push(*token);
                continue;
            };
            if !allowed.iter().any(|name| name == self.name(sym)) {
                out.push(*token);
                continue;
            }
            let body = Rc::from(vec![*token]);
            out.extend(self.expand_tokens(body));
        }
        out
    }

    /// What the document says about itself, as the PDF's document properties
    /// show it.  The values are the replacement texts of `\@title` and
    /// friends put through what hyperref's `\pdfstringdef` does to them: the
    /// logo table is in force, `\texorpdfstring` gives the string meant for
    /// the PDF, the gullet expands what it can, and what only sets type is
    /// dropped.  `Metadata::source` keeps the token list itself.
    /// The three names `\title`, `\author` and `\date` store their argument
    /// in.  `\maketitle` reads them inside the document environment, so satex
    /// renders them as each is set: a macro the document defines in its body
    /// is still defined at that moment, and gone once the environment closes.
    const METADATA: [(&'static str, &'static str); 3] =
        [("title", "@title"), ("author", "@author"), ("date", "@date")];

    /// Render one metadata field, if this name is one.
    fn record_metadata(&mut self, sym: Sym) {
        let Some((field, _)) = Self::METADATA.iter().find(|(_, name)| self.name(sym) == *name)
        else {
            return;
        };
        self.render_metadata(field, sym);
    }

    fn collect_metadata(&mut self) {
        for (field, name) in Self::METADATA {
            if self.out.metadata.iter().any(|entry| entry.field == field) {
                continue;
            }
            let Some(sym) = self.out.interner.lookup(name) else { continue };
            self.render_metadata(field, sym);
        }
    }

    fn render_metadata(&mut self, field: &'static str, sym: Sym) {
        {
            let Some(macro_def) = self.env.meaning(sym).as_macro().cloned() else { return };
            let source = crate::tex::detokenize(&macro_def.replacement_text, &self.out.interner);
            let source = squeeze(&source);
            // `\@title` starts out as the kernel's "no \title given" warning.
            if source.contains("@latex@") {
                return;
            }
            let tokens = macro_def.replacement_text.to_vec();
            let expanded = self.expand_as_pdf_string(&tokens);
            let text = squeeze(&self.pdf_string_text(&expanded));
            // `\maketitle` empties `\@title` once it has set it; the title
            // is what it held before.
            if text.is_empty() {
                return;
            }
            self.out.metadata.retain(|entry| entry.field != field);
            self.out.metadata.push(Metadata { field, text, source });
        }
    }

    /// The document-properties form of a token list.  With hyperref loaded
    /// its own `\pdfstringdef\⟨cs⟩{⟨text⟩}` makes it (hyperref manual,
    /// "PDF strings"), and `\⟨cs⟩` is read back.  Without it the list is
    /// expanded in a group, and what is left of it as characters is the text.
    fn expand_as_pdf_string(&mut self, tokens: &[Token]) -> Vec<Token> {
        self.env.push_group(GroupKind::SemiSimple, Span::default());
        if let Some(define) = self.out.interner.lookup("pdfstringdef")
            && self.env.meaning(define).as_macro().is_some()
        {
            let target = self.intern("satex@pdfstring");
            let span = Span::default();
            let mut run = vec![Token::new(Tok::Cs(define), span), Token::new(Tok::Cs(target), span)];
            run.push(Token::new(Tok::Chr('{', Catcode::Begin), span));
            run.extend_from_slice(tokens);
            run.push(Token::new(Tok::Chr('}', Catcode::End), span));
            self.run_tokens(Rc::from(run));
            let written = self
                .env
                .meaning(target)
                .as_macro()
                .map(|m| self.text_of(&m.replacement_text))
                .unwrap_or_default();
            self.env.pop_group(&mut self.catcodes);
            return decode_pdf_string(&written)
                .chars()
                .map(|c| Token::new(Tok::Chr(c, text_catcode(c)), span))
                .collect();
        }
        self.pdf_string = true;
        let expanded = self.expand_reported(tokens);
        self.pdf_string = false;
        self.env.pop_group(&mut self.catcodes);
        expanded
    }

    /// The text an expanded token list contributes to a PDF string.
    ///
    /// A PDF string holds characters, so that is what survives: letters,
    /// other characters and spaces.  Everything else sets type rather than
    /// text, and hyperref removes it after expanding what it can
    /// (hyperref.sty, `\HyPsd@CheckCatcodes`), which is what a command left
    /// standing here is.  Math is dropped with its delimiters, since a
    /// formula is not text either.
    fn pdf_string_text(&self, tokens: &[Token]) -> String {
        let mut out = String::new();
        let mut i = 0;
        while i < tokens.len() {
            let token = tokens[i];
            i += 1;
            match token.tok {
                Tok::Chr(c, Catcode::Letter | Catcode::Other | Catcode::Active) => out.push(c),
                Tok::Chr(_, Catcode::Space | Catcode::Eol) => out.push(' '),
                Tok::Chr(_, Catcode::Math) => {
                    while let Some(token) = tokens.get(i) {
                        i += 1;
                        if token.is_cat(Catcode::Math) {
                            break;
                        }
                    }
                }
                // A command that survived expansion, a parameter, a brace, a
                // sub- or superscript: none of them is a character.
                Tok::Cs(sym) => {
                    let scan = self.env.meaning(sym).prim().and_then(pdf_string_skip);
                    if let Some(scan) = scan {
                        skip_scanned(tokens, &mut i, scan);
                    }
                }
                Tok::Chr(..) | Tok::Param(_) => {}
            }
        }
        out
    }

    /// A `.ins` batch file is written in docstrip's commands.  satex models
    /// them and keeps its models pinned: `\input docstrip` reads a program
    /// whose own definitions write files.
    fn install_docstrip(&mut self) {
        self.docstrip = true;
        for (sym, meaning) in crate::builtins::docstrip_meanings(&mut self.out.interner) {
            self.pinned.insert(sym);
            self.env.set(sym, Binding::builtin(meaning), true);
        }
    }

    /// tex.web §§ 222-240: INITEX starts every parameter at zero, glue at
    /// zero glue and token lists empty, and then sets the few that are not.
    fn install_parameters(&mut self) {
        use crate::builtins::Primitive as P;
        use crate::value::Value;
        let table = initial_meanings(&mut self.out.interner, self.out.plugins.engine);
        for (sym, meaning) in table {
            let zero = match meaning {
                Meaning::Primitive(P::IntegerParameter) => Value::Int(0),
                Meaning::Primitive(P::DimenParameter) => Value::Dimen(0),
                Meaning::Primitive(P::GlueParameter { mu: false }) => Value::Glue(Default::default()),
                Meaning::Primitive(P::GlueParameter { mu: true }) => Value::MuGlue(Default::default()),
                Meaning::Primitive(P::TokensParameter) => Value::Toks(Rc::from(Vec::new())),
                _ => continue,
            };
            if matches!(self.env.value(sym), Value::Unknown) {
                self.env.set_value(sym, zero, true);
            }
        }
        for (name, value) in [
            // tex.web § 74: interaction starts in `\errorstopmode`.
            ("interactionmode", crate::commands::ERROR_STOP_MODE),
            ("escapechar", i64::from(b'\\')),
            ("endlinechar", 13),
            ("tolerance", 10_000),
            ("hangafter", 1),
            ("maxdeadcycles", 25),
            ("mag", 1000),
            ("errorcontextlines", -1),
        ] {
            let sym = self.intern(name);
            self.env.set_value(sym, crate::value::Value::Int(value), true);
        }
        if self.out.plugins.engine.has_luatex()
            && let Some(version) = crate::plugin::lua::engine_version(self.cfg)
        {
            let sym = self.intern("luatexversion");
            self.env.set_value(sym, crate::value::Value::Int(version), true);
        }
    }

    /// tex.web § 241: a job starts with `\year`, `\month`, `\day` and
    /// `\time` set from the clock, which is what `\today` prints.
    fn install_clock(&mut self) {
        let now = time::OffsetDateTime::now_local()
            .unwrap_or_else(|_| time::OffsetDateTime::now_utc());
        let minutes = i64::from(now.hour()) * 60 + i64::from(now.minute());
        for (name, value) in [
            ("year", i64::from(now.year())),
            ("month", i64::from(u8::from(now.month()))),
            ("day", i64::from(now.day())),
            ("time", minutes),
        ] {
            let sym = self.intern(name);
            self.env.set_value(sym, crate::value::Value::Int(value), true);
        }
    }

    /// What the output target decides before the document is read: the value
    /// of `\pdfoutput`, which every `\ifpdf` and driver test is built on.
    fn install_output(&mut self) {
        let output = self.out.plugins.output;
        let value = crate::value::Value::Int(output.pdfoutput());
        for name in ["pdfoutput", "outputmode"] {
            let sym = self.intern(name);
            if self.env.is_defined(sym) {
                self.env.set_value(sym, value.clone(), true);
            }
        }
    }

    fn install_format(&mut self) {
        // A ConTeXt run never reads a kernel file: its format is built from
        // `cont-en` on top of ConTeXt's own Lua layer, which satex does not
        // interpret.  `none` asks for the same thing deliberately.
        let kernel = self.out.plugins.kernel;
        let Some(kernel_file) = kernel.source() else {
            if kernel == crate::plugin::Kernel::Context {
                self.diagnose(
                    Severity::Info,
                    "context-kernel",
                    Span::default(),
                    "ConTeXt support is experimental: its kernel is not interpreted, so only \
                     the engine primitives are known here"
                        .into(),
                );
            }
            self.run_without_format();
            return;
        };
        let source = self
            .cfg
            .load_format
            .then(|| {
                let base = self.base.clone();
                self.resolver.resolve(kernel_file, LoadKind::Input, &base)
            })
            .flatten();
        let Some(source) = source else {
            self.run_without_format();
            return;
        };
        let cache_dir = self.cache_dir();

        // A missing or stale cache is rebuilt below; it says nothing about
        // the document, so it is no finding, only a notice on stderr.
        // One build per process: runs that miss together wait for it and
        // read what it stored.
        let mut _building = None;
        if let (Some(directory), false) = (&cache_dir, self.cfg.rebuild_format) {
            let load = || Format::load(directory, &source, self.out.plugins.engine.as_str(), self.preload_key());
            let mut cached = load();
            if cached.is_err() {
                _building = Some(crate::format::BUILDING.lock().unwrap_or_else(|e| e.into_inner()));
                cached = load();
            }
            match cached {
                Err(reason) => {
                    let name = source.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
                    if self.cfg.verbose >= 1 {
                        eprintln!("satex: building kernel cache from {name}: {reason} (this may take a couple of minutes)");
                    } else {
                        eprintln!("satex: building kernel cache from {name} (this may take a couple of minutes)");
                    }
                }
                Ok(cached) => {
                let definitions = cached.definitions();
                for (path, kind) in cached.files().to_vec() {
                    self.register_file(path, kind);
                }
                self.out.kernel_sites = cached
                    .sites()
                    .iter()
                    .map(|(sym, file, line)| (Sym(*sym), Span::new(*file, *line, 0)))
                    .collect();
                self.wild = cached.wild().clone();
                let deltas = cached.install(&mut self.out.interner, &mut self.env);
                self.catcodes = self.format_catcodes();
                self.catcodes.apply(&deltas);
                self.replay_calls();
                self.out.format = Some(FormatInfo {
                    source: source.display().to_string(),
                    definitions,
                    cached: true,
                    cache: Some(
                        crate::format::cache_file(
                            directory,
                            &source,
                            self.out.plugins.engine.as_str(),
                            self.preload_key(),
                        )
                        .display()
                        .to_string(),
                    ),
                    reached: 0,
                    lines: 0,
                });
                return;
                }
            }
        }

        let Ok(text) = std::fs::read_to_string(&source) else {
            self.run_without_format();
            return;
        };
        let plain = self.format_catcodes();
        // The kernel is read under the table it hands on (see below).
        self.catcodes = plain.clone();
        self.catcodes.at_letter(true);
        let id = self.register_file(source.display().to_string(), LoadKind::Input);
        self.input.push(Frame::File {
            mouth: Mouth::file(&text, id),
            package: None,
            catcodes: None,
            depth: 0,
            conds: 0,
            eof_seen: false,
        });
        self.budget = Some(self.cfg.limits.format_steps);
        self.run();
        self.budget = None;

        let truncated = self.halted;
        let reached = self
            .out
            .facts
            .defs
            .iter()
            .filter(|def| def.span.file == id)
            .map(|def| def.span.line)
            .max()
            .unwrap_or(0);
        let lines = text.lines().count() as u32;
        // The table a format hands to a document is the LaTeX one; latex.ltx
        // makes the specials `other` while it bootstraps and restores them in
        // groups this interpreter only approximates.
        self.catcodes = plain;
        let preload_truncated = self.run_preload();
        let sites: Vec<(u32, u16, u32)> = {
            let mut seen: HashMap<Sym, Span> = HashMap::new();
            for def in &self.out.facts.defs {
                seen.insert(def.name, def.span);
            }
            self.out.kernel_sites = seen.clone();
            seen.into_iter().map(|(sym, span)| (sym.0, span.file, span.line)).collect()
        };
        self.forget_facts();
        let preload_key = self.preload_key();

        let format = FormatInfo {
            source: source.display().to_string(),
            definitions: self.env.meaning_count(),
            cached: false,
            cache: cache_dir.as_deref().map(|dir| {
                crate::format::cache_file(
                    dir,
                    &source,
                    self.out.plugins.engine.as_str(),
                    preload_key,
                )
                .display()
                .to_string()
            }),
            reached,
            lines,
        };
        if truncated {
            self.diagnose(
                Severity::Unsupported,
                "format-truncated",
                Span::default(),
                format!("{} was not read to the end", source.display()),
            );
        } else if !format.complete() {
            self.diagnose(
                Severity::Unsupported,
                "format-incomplete",
                Span::default(),
                format!(
                    "{} was interpreted to line {reached} of {lines}; names defined after that \
will look undefined",
                    source.display()
                ),
            );
        }
        // A kernel read only partly is not kept: a later run, perhaps of a
        // build that reads it to the end, would install it as the format.
        let complete = format.complete();
        self.out.format = Some(format);
        self.pinned.clear();

        // The cache carries bindings, not the Lua state the kernel built
        // (LuaTeX's `ltluatex.lua`, expl3's `\luadef`ed functions such as
        // `\tex_Ucharcat:D`): a replayed kernel would call functions its
        // fresh Lua state does not have.  Such a kernel is read every run.
        let cacheable = self.cfg.cache && !truncated && !preload_truncated && complete && !self.lua.ran();
        // Every file the kernel was read from, in order and without the
        // document itself, so that installing this cache hands out the same
        // file numbers the cached spans carry.
        let files: Vec<(String, LoadKind)> = self
            .out
            .files
            .iter()
            .skip(1)
            .map(|file| (file.path.clone(), file.kind))
            .collect();
        if let (true, Some(directory), Some(captured)) = (
            cacheable,
            &cache_dir,
            Format::capture(&source, &self.out.interner, &self.env, &[], files, sites, &self.wild),
        ) {
            let stored = captured.store(
                directory,
                &source,
                self.out.plugins.engine.as_str(),
                preload_key,
            );
            if let (Err(e), true) = (stored, self.cfg.verbose >= 1) {
                eprintln!("satex: the kernel cache could not be written: {e}");
            }
            // Go on from what was cached, as a run that reads the cache does:
            // a cache never changes an answer.
            let deltas = captured.install(&mut self.out.interner, &mut self.env);
            self.catcodes.apply(&deltas);
            self.out.graph = DependencyGraph::default();
            self.out.calls = CallGraph::default();
            self.out.observed = CallGraph::default();
            self.replay_calls();
        } else if self.cfg.verbose >= 1 && self.cfg.cache && !cacheable {
            eprintln!(
                "satex: the kernel is not cached: {}",
                if truncated { "latex.ltx was not read to the end" } else { "the preloaded format was cut short" }
            );
        }
    }

    /// What the preloaded format contributes to the cache key: a document
    /// that starts from its own format cannot share a cache with one that
    /// starts from the stock kernel.
    fn preload_key(&self) -> u64 {
        let Some(source) = self.out.plugins.preload.as_ref().and_then(|p| p.source.as_ref()) else {
            return 0;
        };
        let mut hash = crate::format::digest(source.as_os_str().as_encoded_bytes());
        if let Some((size, modified)) = crate::format::stamp(source) {
            hash ^= crate::format::digest(&size.to_le_bytes());
            hash ^= crate::format::digest(&modified.to_le_bytes());
        }
        hash
    }

    /// Interpret the preamble a project dumps into its own format, so that
    /// what it defines is in place before the document is read.  This is what
    /// a `.fmt` built by `mylatexformat` holds.  Returns whether it was cut
    /// short, in which case the format it leaves is incomplete.
    fn run_preload(&mut self) -> bool {
        let Some(source) = self.out.plugins.preload.as_ref().and_then(|p| p.source.clone()) else {
            return false;
        };
        let Ok(text) = std::fs::read_to_string(&source) else {
            self.diagnose(
                Severity::Warning,
                "preload-unreadable",
                Span::default(),
                format!("cannot read {}", source.display()),
            );
            return false;
        };
        let text = crate::plugin::preload::dumped_part(&text).to_string();
        let id = self.register_file(source.display().to_string(), LoadKind::Input);
        // A run of its own: the preamble neither inherits what the kernel
        // already spent nor leaves anything on the input stack, which the
        // document would otherwise read as its own continuation.
        self.out.steps = 0;
        self.halted = false;
        self.input.push(Frame::Boundary);
        let base = self.input.len();
        self.input.push(Frame::File {
            mouth: Mouth::file(&text, id),
            package: None,
            catcodes: None,
            depth: 0,
            conds: 0,
            eof_seen: false,
        });
        self.budget = Some(self.cfg.limits.format_steps);
        self.run();
        self.budget = None;
        while self.input.len() >= base {
            self.pop_frame();
        }
        if !self.halted {
            return false;
        }
        self.diagnose(
            Severity::Unsupported,
            "preload-truncated",
            Span::default(),
            format!("{} was not read to the end", source.display()),
        );
        true
    }

    fn cache_dir(&self) -> Option<PathBuf> {
        if !self.cfg.cache {
            return None;
        }
        self.cfg.cache_dir.clone().or_else(format::default_cache_dir)
    }

    /// The table a LaTeX format hands to a document; a package or class
    /// read as the main file has `@` a letter, as `\usepackage` gives it.
    fn format_catcodes(&self) -> CatcodeTable {
        let mut table = CatcodeTable::latex();
        let package = self.out.files.get(self.out.main_file as usize).is_some_and(|f| f.kind.is_package());
        table.at_letter(self.cfg.at_letter || package);
        table
    }

    /// No format: the run starts from the engine's primitives, as `initex`
    /// does.  Only a format that was asked for and not found is reported.
    fn run_without_format(&mut self) {
        if self.cfg.load_format {
            self.diagnose(
                Severity::Warning,
                "no-format",
                Span::default(),
                "no latex.ltx was found; only the engine's primitives are known".into(),
            );
        }
    }

    /// The kernel is not the user's code, so it contributes meanings but no
    /// findings.
    fn forget_facts(&mut self) {
        // Definitions and uses belong to the kernel, not the document; the
        // diagnostics about how well it was read do not.
        // The budget starts again for the document itself.
        self.halted = false;
        // A run that installs the kernel from its cache has not learnt the
        // counter namespace yet either; both learn it the same way.
        self.counter_namespace = None;
        self.out.exhausted = false;
        self.out.steps = 0;
        self.out.facts.defs.clear();
        self.out.facts.expansions.clear();
        self.out.facts.occurrences.clear();
        self.out.facts.loads.clear();
        self.out.facts.shapes.clear();
        self.call_shape = None;
        self.definition_sites.clear();
        self.expansion_sites.clear();
        self.out.facts.spaces.clear();
        self.out.facts.conditionals.clear();
        self.cond_sites.clear();
    }

    /// `\count5` and the rest: a register addressed by number keeps its value
    /// where a named one does, so a group undoes it the same way.
    pub fn register_sym(&mut self, kind: crate::tex::RegKind, index: i64) -> Sym {
        self.intern(&format!("{REGISTER}{}{index}", kind.as_str()))
    }

    /// Where the value of a register name lives: `\countdef\x=20` makes
    /// `\x` another name for `\count20` (tex.web § 1224), so both read and
    /// write the one register.  A `\chardef` constant, a box or stream
    /// number, and a register satex could not number keep their value on
    /// their own name.
    pub fn storage(&mut self, sym: Sym) -> Sym {
        match self.env.meaning(sym) {
            Meaning::Register(
                kind @ (crate::tex::RegKind::Count
                | crate::tex::RegKind::Dimen
                | crate::tex::RegKind::Skip
                | crate::tex::RegKind::MuSkip
                | crate::tex::RegKind::Toks),
                index,
            ) if index != crate::tex::UNNUMBERED => {
                self.register_sym(kind, i64::from(index))
            }
            _ => sym,
        }
    }

    pub fn set_stream(&mut self, stream: i64, lines: Option<Vec<String>>) {
        match lines {
            Some(lines) => {
                Rc::make_mut(&mut self.streams).insert(stream, (Rc::new(lines), 0));
            }
            None => {
                Rc::make_mut(&mut self.streams).remove(&stream);
            }
        }
    }

    pub fn close_in(&mut self, stream: i64) {
        Rc::make_mut(&mut self.streams).remove(&stream);
    }

    /// A stream that was never opened, or whose file was not found, is at
    /// end; an open one only once a read has found no line left (tex.web
    /// § 485 closes it then), so the last line is read with `\ifeof` false.
    pub fn stream_at_end(&self, stream: i64) -> bool {
        !self.streams.contains_key(&stream)
    }

    pub fn take_stream_line(&mut self, stream: i64) -> Option<String> {
        // The lines stay shared; only the position moves, so a snapshot of
        // the streams costs nothing however long the file is.
        let (lines, next) = Rc::make_mut(&mut self.streams).get_mut(&stream)?;
        let Some(line) = lines.get(*next).cloned() else {
            self.close_in(stream);
            return None;
        };
        *next += 1;
        // A line read from a file is input read, as one the mouth reads is:
        // a loop over a data file (expl3's `\ior_map_variable:NNn` over
        // UnicodeData.txt) makes progress with every line.
        self.source_progress += 1;
        self.progress_step = self.out.steps;
        self.env.clear_recent();
        // tex.web § 31 `input_ln`: trailing spaces are dropped.
        Some(line.trim_end_matches(' ').to_string())
    }

    /// Whether the kernel is being read, as an `-ini` run reads it.
    pub(crate) fn in_format(&self) -> bool {
        self.budget.is_some()
    }

    pub fn intern(&mut self, s: &str) -> Sym {
        self.out.interner.intern(s)
    }

    pub fn name(&self, s: Sym) -> &str {
        self.out.interner.name(s)
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    /// The line of the innermost file being read (`\inputlineno`).
    pub fn input_line(&self) -> Option<u32> {
        self.input.iter().rev().find_map(|frame| match frame {
            Frame::File { mouth, .. } => Some(mouth.here().line),
            _ => None,
        })
    }

    pub fn resolver_mut(&mut self) -> &mut Resolver {
        &mut self.resolver
    }

    pub fn package(&self) -> Option<Sym> {
        self.packages.last().map(|(name, _)| *name)
    }

    /// The next token, stopping at the end of the file it is read from.
    ///
    /// Only the main loop crosses a file boundary; a reader grabbing an
    /// argument must not, or a package that ends mid-construct would consume
    /// the file that loaded it.
    pub fn next_token(&mut self) -> Option<Token> {
        self.read(false)
    }

    /// A token from the current file, with no expansion and no budget for the
    /// tokens an expansion would have produced.
    pub fn read_raw(&mut self) -> Option<Token> {
        self.read(false)
    }

    /// Every token read counts against the budget, so that a runaway loop in
    /// any reader terminates and not just the main one.
    fn read(&mut self, across_files: bool) -> Option<Token> {
        let token = self.read_token(across_files)?;
        if token.tok != Tok::Cs(self.dont_expand) {
            self.noexpanded = None;
            return Some(token);
        }
        // tex.web § 358: the marker goes, and the token after it is read as
        // `\relax` would be, whatever its meaning.
        let token = self.read_token(across_files)?;
        self.noexpanded = Some(token);
        Some(token)
    }

    /// Whether the token the last read returned stood behind a `\noexpand`
    /// marker and would otherwise have been expanded.
    pub fn read_noexpanded(&self, token: Token) -> bool {
        self.noexpanded.is_some_and(|t| t.tok == token.tok && t.span == token.span)
    }

    fn read_token(&mut self, across_files: bool) -> Option<Token> {
        if self.over_budget() {
            return None;
        }
        self.out.steps += 1;
        if self.out.timings.is_enabled() {
            self.out.timings.count_token();
        }
        loop {
            let Self { input, catcodes, end_line, out, .. } = self;
            match input.last_mut() {
                None => return None,
                Some(Frame::Boundary) => return None,
                Some(Frame::Returned(tokens)) => {
                    if let Some(t) = tokens.pop() {
                        self.held_tokens = self.held_tokens.saturating_sub(1);
                        // Given back by a lookahead: it came from the file
                        // being read when that file is where it stands.
                        let given = self.recent_file_spans.iter().rposition(|s| *s == t.span);
                        let from_file = given.map(|i| self.recent_file_spans.swap_remove(i)).is_some();
                        self.last_file = from_file.then_some(t.span.file);
                        return Some(t);
                    }
                }
                Some(Frame::Tokens { toks, pos, expanding }) => {
                    if let Some(t) = toks.get(*pos) {
                        *pos += 1;
                        let t = *t;
                        if expanding.is_some() {
                            self.source = *expanding;
                        }
                        self.last_file = None;
                        return Some(t);
                    }
                    // A `\scantokens` pseudo file ends like a file.
                    if *expanding == Some(self.pseudo_file) && !across_files && self.scanner != Scanner::Normal {
                        self.file_ended_while_scanning(None);
                        return None;
                    }
                }
                Some(Frame::Pseudo { mouth, at, eof, pos }) => {
                    let next = match mouth.next_with(catcodes, &mut out.interner, *end_line) {
                        Some(t) => Some(Token::new(t.tok, *at)),
                        None => eof.get(*pos).copied().inspect(|_| *pos += 1),
                    };
                    if let Some(t) = next {
                        self.source = Some(self.pseudo_file);
                        self.last_file = None;
                        return Some(t);
                    }
                    // A pseudo file ends like a file.
                    if !across_files && self.scanner != Scanner::Normal {
                        self.file_ended_while_scanning(None);
                        return None;
                    }
                }
                Some(Frame::File { mouth, eof_seen, .. }) => {
                    if let Some(t) = mouth.next_with(catcodes, &mut out.interner, *end_line) {
                        self.source_progress += 1;
                        self.progress_step = self.out.steps;
                        self.env.clear_recent();
                        self.last_file = Some(t.span.file);
                        self.source = None;
                        return Some(t);
                    }
                    // etex.ch § 362: `\everyeof` is read before the file
                    // ends, so a scan may find its delimiter there.
                    if !std::mem::replace(eof_seen, true) {
                        let eof = self.intern("everyeof");
                        if let crate::value::Value::Toks(toks) = self.env.value(eof)
                            && !toks.is_empty()
                        {
                            let toks = toks.clone();
                            self.push_tokens(toks, None);
                            continue;
                        }
                    }
                    let Some(Frame::File { mouth, .. }) = self.input.last() else { continue };
                    if !across_files {
                        let file = mouth.file;
                        if self.scanner != Scanner::Normal {
                            self.file_ended_while_scanning(Some(file));
                        }
                        return None;
                    }
                }
            }
            if self.reading_command && self.package_capture.is_some() {
                self.checkpoint_at_end();
            }
            let outer = std::mem::replace(&mut self.popping_in_read, true);
            self.pop_frame();
            self.popping_in_read = outer;
        }
    }

    /// tex.web §§ 336-339 `check_outer_validity`: a file that ends in the
    /// middle of a scan ends it — the definition or text gets its `}`, the
    /// skipped conditional its `\fi`, and a macro call is abandoned, as the
    /// `\par` TeX inserts with `long_state = outer_call` abandons it.
    fn file_ended_while_scanning(&mut self, file: Option<FileId>) {
        let (what, recovery) = match self.scanner {
            Scanner::Normal => return,
            Scanner::Skipping => ("conditional text", "\\fi"),
            Scanner::Defining => ("definition", "}"),
            Scanner::Matching => ("use", "\\par"),
            Scanner::Absorbing => ("text", "}"),
        };
        if self.scanner == Scanner::Matching {
            self.runaway = true;
        }
        let span = file.map_or_else(|| self.input_span(), |f| Span::new(f, 0, 0));
        self.diagnose(
            Severity::Warning,
            "file-ended",
            span,
            format!("file ended while scanning {what}; {recovery} inserted"),
        );
    }

    fn input_span(&self) -> Span {
        self.input.iter().rev().find_map(|frame| match frame {
            Frame::File { mouth, .. } => Some(mouth.here()),
            _ => None,
        }).unwrap_or_default()
    }

    fn pop_frame(&mut self) {
        match self.input.pop() {
            Some(Frame::Boundary) => {}
            Some(Frame::Returned(tokens)) => {
                self.held_tokens = self.held_tokens.saturating_sub(tokens.len());
            }
            Some(Frame::Tokens { toks, expanding, .. }) => {
                self.held_tokens = self.held_tokens.saturating_sub(toks.len());
                if let Some(sym) = expanding {
                    if let Some(n) = self.expanding.get_mut(sym.0 as usize) {
                        *n = n.saturating_sub(1);
                    }
                    self.within.pop();
                    // The expansion the call started is over, so the call has
                    // read everything it reads.
                    if self.call_shape.as_ref().is_some_and(|(.., depth)| self.within.len() < *depth) {
                        self.finish_shape();
                    }
                }
            }
            Some(Frame::Pseudo { .. }) => self.bump_expanding(self.pseudo_file, -1),
            Some(Frame::File { mouth, package, catcodes, depth, conds, .. }) => {
                self.out.timings.leave_file();
                self.balance_file(mouth.file, depth);
                self.balance_conditionals(mouth.file, conds);
                if let Some(table) = catcodes {
                    self.catcodes = *table;
                }
                if package.is_some() {
                    self.packages.pop();
                }
                let name = package.unwrap_or(Sym(0));
                self.record(Step::CloseFile, name, Span::new(mouth.file, 0, 0));
                if self.file_calls.last().is_some_and(|(file, _)| *file == mouth.file) {
                    self.file_call = self.file_calls.pop().and_then(|(_, call)| call);
                }
                self.file_depth = self.file_depth.saturating_sub(1);
                if self.package_capture.as_ref().is_some_and(|c| c.file == mouth.file) {
                    self.finish_recording();
                }
            }
            _ => {}
        }
    }

    /// TeX reports the groups still open when a job ends (tex.web § 1335).
    /// satex applies that check at every file boundary: a file that returns
    /// at a deeper nesting than it began at has lost a `}` or an `\endgroup`
    /// to an approximation — one hidden in a token register, or dropped with
    /// a widened expansion — and leaving the level open would make a later,
    /// unrelated `}` undo every definition made since.  The level is given up
    /// without restoring anything, so those definitions survive where they
    /// really belong.
    /// A file that ends inside a conditional leaves it open, which TeX
    /// reports as an incomplete `\if` (tex.web § 336).  The levels it opened
    /// go with it: what follows the file is not governed by a test the file
    /// never finished, and an undecided one would otherwise hang a control
    /// dependency on every vertex left in the run.
    fn balance_conditionals(&mut self, file: FileId, depth: usize) {
        if self.conds.len() <= depth {
            return;
        }
        let open = self.conds.len() - depth;
        let opened = self.conds[depth].at;
        while self.conds.len() > depth {
            if let Some(frame) = self.conds.pop()
                && frame.dep
            {
                self.cds.pop();
            }
        }
        let name = self.out.short_name(file).to_string();
        let at = format!("{}:{}", self.out.short_name(opened.file), opened.line);
        self.diagnose(
            Severity::Unsupported,
            "unbalanced-conditional",
            Span::new(file, 0, 0),
            format!("end of {name} occurred inside {open} unfinished conditional(s), the outermost begun at {at}"),
        );
    }

    fn balance_file(&mut self, file: FileId, depth: usize) {
        let spans = self.env.open_group_spans();
        let mut open = 0;
        while self.env.depth() > depth && self.env.discard_group() {
            open += 1;
        }
        if open == 0 {
            return;
        }
        let name = self.out.short_name(file).to_string();
        let where_from: Vec<String> = spans
            .iter()
            .rev()
            .take(3)
            .map(|(kind, span)| {
                format!("{} at {}:{}", opened_by(*kind), self.out.short_name(span.file), span.line)
            })
            .collect();
        self.diagnose(
            Severity::Unsupported,
            "unbalanced-file",
            Span::new(file, 0, 0),
            format!(
                "end of {name} occurred inside a group at level {open}; opened by {}",
                where_from.join(", ")
            ),
        );
    }

    /// tex.web § 325: before anything goes back into the input, token lists
    /// that are used up are ended, so that a macro whose expansion is over
    /// no longer counts as being expanded — a loop that re-expands its
    /// result with `\expandafter` does not nest.
    fn end_used_up_lists(&mut self) {
        // A pseudo file stays until a read meets its end (tex.web § 362).
        while let Some(Frame::Tokens { toks, pos, expanding }) = self.input.last()
            && *pos >= toks.len()
            && *expanding != Some(self.pseudo_file)
        {
            self.pop_frame();
        }
    }

    pub fn unread(&mut self, token: Token) {
        // A token given back right after it was read from behind a
        // `\noexpand` marker keeps the marker, so that reading it again
        // treats it the same way.
        if self.read_noexpanded(token) {
            self.noexpanded = None;
            self.unread_plain(token);
            let marker = Token::new(Tok::Cs(self.dont_expand), token.span);
            self.unread_plain(marker);
            return;
        }
        self.unread_plain(token);
    }

    fn unread_plain(&mut self, token: Token) {
        if self.last_file == Some(token.span.file) {
            if self.recent_file_spans.len() == 64 {
                self.recent_file_spans.remove(0);
            }
            self.recent_file_spans.push(token.span);
        }
        self.end_used_up_lists();
        self.held_tokens += 1;
        match self.input.last_mut() {
            Some(Frame::Returned(tokens)) => tokens.push(token),
            _ => self.input.push(Frame::Returned(vec![token])),
        }
    }

    pub fn unread_all(&mut self, tokens: &[Token]) {
        if tokens.is_empty() {
            return;
        }
        // Given back by a keyword scan (tex.web § 407): still the file's.
        for t in tokens.iter().filter(|t| self.last_file == Some(t.span.file)) {
            if self.recent_file_spans.len() == 64 {
                self.recent_file_spans.remove(0);
            }
            self.recent_file_spans.push(t.span);
        }
        self.end_used_up_lists();
        self.held_tokens += tokens.len();
        match self.input.last_mut() {
            Some(Frame::Returned(returned)) => returned.extend(tokens.iter().rev().copied()),
            _ => self.input.push(Frame::Returned(tokens.iter().rev().copied().collect())),
        }
    }

    pub fn peek(&mut self) -> Option<Token> {
        let t = self.next_token()?;
        self.unread(t);
        Some(t)
    }

    pub fn push_tokens(&mut self, toks: Rc<[Token]>, expanding: Option<Sym>) {
        if toks.is_empty() || self.held_tokens + toks.len() > self.cfg.limits.held_tokens {
            if let Some(sym) = expanding {
                self.bump_expanding(sym, -1);
            }
            return;
        }
        self.held_tokens += toks.len();
        if expanding.is_none() {
            self.end_used_up_lists();
        }
        self.input.push(Frame::Tokens { toks, pos: 0, expanding });
    }

    pub(crate) fn push_pseudo(&mut self, mouth: Box<Mouth>, at: Span, eof: Rc<[Token]>) {
        self.end_used_up_lists();
        self.input.push(Frame::Pseudo { mouth, at, eof, pos: 0 });
    }

    pub(crate) fn bump_expanding(&mut self, sym: Sym, delta: i32) {
        let i = sym.0 as usize;
        if self.expanding.len() <= i {
            self.expanding.resize(i + 1, 0);
        }
        self.expanding[i] = if delta < 0 {
            self.expanding[i].saturating_sub(1)
        } else {
            self.expanding[i] + 1
        };
    }

    pub fn open_group(&mut self, kind: GroupKind, span: Span) {
        self.env.push_group(kind, span);
    }

    /// `}`, `\endgroup` or a closing `$`.  Each ends only the kind of group
    /// its opener made (tex.web § 269), so a `}` that meets a `\begingroup`
    /// group or an `\endgroup` that meets a `{` is reported the way TeX
    /// reports it (tex.web § 1069, § 1064).  A closer with no group to end,
    /// or one that would close a group opened outside the region being
    /// analyzed, is ignored: there is nothing to restore.
    pub fn close_group(&mut self, closer: GroupKind, span: Span) {
        let Some(kind) = self.env.group_kind() else { return };
        if closer == GroupKind::SemiSimple
            && self.within.is_empty()
            && let Some((_, opened)) = self.env.open_group_spans().last().copied()
            && opened.file == span.file
            && opened.line < span.line
        {
            self.out.facts.groups.push((opened, span));
        }
        if !closer.closes(kind) {
            self.diagnose(
                Severity::Warning,
                "mismatched-group",
                span,
                format!("extra {}, or forgotten {}", name_of(closer), name_of(kind)),
            );
        }
        // tex.web § 1064 `off_save` repairs the nesting by inserting the token
        // the open group is waiting for; the group therefore ends here either
        // way, which is also what keeps the depth of an analyzed file right
        // when the mismatch is this interpreter's own approximation.
        let inner = crate::observe::current_environment(self);
        let nest = self.env.nest();
        if let Some(after) = self.env.pop_group(&mut self.catcodes) {
            self.unread_all(&after);
            self.group_closed(nest, span);
        }
        if closer == GroupKind::SemiSimple {
            crate::observe::environment_closed(self, inner, span);
        }
        self.sync_end_line();
    }

    /// The next token if it is a math shift character, explicit or implicit.
    /// tex.web § 1138 reads the token after the first `$` and starts display
    /// math when it is another one, and tex.web § 1197 asks for the second
    /// `$` again at the end, so `$$ … $$` is one group and not two.
    pub(crate) fn take_math_shift(&mut self) -> bool {
        let Some(token) = self.next_token() else { return false };
        if self.category_of(token) == Some(Catcode::Math) {
            return true;
        }
        self.unread(token);
        false
    }

    /// The category a token acts with.  A control sequence `\let` to a
    /// character carries that character's category and behaves exactly like
    /// it (tex.web § 1063, The TeXbook ch. 24: `\bgroup` is an implicit `{`).
    pub fn category_of(&self, token: Token) -> Option<Catcode> {
        match token.tok {
            Tok::Chr(_, catcode) => Some(catcode),
            Tok::Cs(sym) => match self.env.meaning(sym) {
                Meaning::Char(_, catcode) => Some(catcode),
                _ => None,
            },
            Tok::Param(_) => None,
        }
    }

    /// `\aftergroup⟨token⟩` (tex.web § 326).
    pub fn defer_after_group(&mut self) {
        let Some(token) = self.next_token() else { return };
        self.env.save_after(token);
    }

    /// Give a register a value: a definition of that register at this point.
    /// The value does not depend on the one it held, so a scratch register
    /// written here does not drag in whatever wrote it last.
    pub fn assign(&mut self, name: Sym, value: crate::value::Value, global: bool, span: Span) {
        self.assign_through(name, name, value, global, span);
    }

    /// An assignment written as `via` that stores into `name`: `\gap=1pt`
    /// after `\skipdef\gap=44` sets `\skip44`, and the vertex keeps the
    /// name the source used.
    pub fn assign_through(&mut self, via: Sym, name: Sym, value: crate::value::Value, global: bool, span: Span) {
        self.assign_with(via, name, value, global, span, false);
    }

    /// An assignment built out of the old value: `\advance`, `\multiply`,
    /// `\divide`.  It reads the definitions in force.
    pub fn assign_from_old(&mut self, via: Sym, name: Sym, value: crate::value::Value, global: bool, span: Span) {
        self.assign_with(via, name, value, global, span, true);
    }

    fn assign_with(
        &mut self,
        via: Sym,
        name: Sym,
        value: crate::value::Value,
        global: bool,
        span: Span,
        derived: bool,
    ) {
        // An assignment a macro makes on behalf of a register the file named
        // (`\setlength{\gap}` running `\gap#2`) happens at that call.
        let span = match self.file_call {
            Some((_, call)) if via != name && call.file != span.file => call,
            _ => span,
        };
        if let Some(line) = self.source_extent(span) {
            self.out.graph.note_extent(span, line);
        }
        let within = self.within.last().map(|(_, node)| *node);
        let node =
            self.out.graph.push(VertexTag::VariableDefinition, via, span, within, self.cds.clone());
        if via != name {
            let defs = self.env.link_defs(via);
            self.link_reads(node, via, &defs);
        }
        // Assigning a register reads nothing of the value it held; a package
        // cache counts it as a read only when the name means something else.
        let tracking = self.env.track_reads(false);
        let mut binding = match self.env.get(name) {
            Some(b) => b.clone(),
            None => Binding::builtin(Meaning::Unknown),
        };
        self.env.track_reads(tracking);
        self.env.note_read(name, crate::env::ReadKind::Kept);
        if derived {
            let defs = binding.defs.clone();
            self.link_reads(node, name, &defs);
        }
        // A global assignment made while expanding outlives the expansion.
        if let (true, Some(caller)) = (global, within) {
            self.out.graph.edge(node, caller, EdgeKind::SIDE_EFFECT_ON_CALL);
        }
        if let Some(call) = self.out.graph.reader {
            self.out.graph.made_by(node, call);
        }
        binding.value = value;
        binding.defs = self.def_sites(name, node);
        binding.certain = self.cds.is_empty();
        self.env.set(name, binding, global);
        self.record(Step::Assign, name, span);
    }

    /// The graph side of an assignment whose value is already in place: a
    /// definition of `name` here that reads the ones it replaces, as
    /// [`Machine::assign`] records one.
    pub fn note_assignment(&mut self, name: Sym, global: bool, span: Span) {
        let within = self.within.last().map(|(_, node)| *node);
        let node =
            self.out.graph.push(VertexTag::VariableDefinition, name, span, within, self.cds.clone());
        let defs = self.env.link_defs(name);
        self.link_reads(node, name, &defs);
        if let Some(call) = self.out.graph.reader {
            self.out.graph.made_by(node, call);
        }
        let sites = self.def_sites(name, node);
        if let Some(mut binding) = self.env.get(name).cloned() {
            binding.defs = sites;
            self.env.set(name, binding, global);
        }
    }

    /// `\catcode` is a local assignment, so the old value goes on the save
    /// stack and the enclosing group restores it.
    pub fn set_catcode(&mut self, character: char, catcode: Catcode) {
        self.env.save_catcode(character, self.catcodes.get(character));
        self.catcodes.set(character, catcode);
    }

    pub fn set_end_line(&mut self, value: i32) {
        self.end_line = EndLineChar(value);
    }

    /// `\endlinechar` is an integer parameter like any other, so the end of
    /// a group restores it (tex.web § 236); the mouth follows the table.
    pub(crate) fn sync_end_line(&mut self) {
        let sym = self.intern("endlinechar");
        // A copy kept in step with the parameter, not a use of it: a package
        // cache keys on the copy itself.
        if let Some(value) = self.env.slot_value(sym).and_then(|v| i32::try_from(v).ok()) {
            self.end_line = EndLineChar(value);
        }
    }

    /// `\makeatletter` and `\ExplSyntaxOn` change several at once.
    pub fn with_catcodes(&mut self, change: impl FnOnce(&mut CatcodeTable)) {
        let before = self.catcodes.clone();
        change(&mut self.catcodes);
        for (character, _) in self.catcodes.diff(&before) {
            self.env.save_catcode(character, before.get(character));
        }
    }

    /// [`Machine::with_catcodes`] for a command at `span`: each change is an
    /// occurrence there, as `\catcode` makes one.
    pub fn with_catcodes_at(&mut self, span: Span, change: impl FnOnce(&mut CatcodeTable)) {
        let before = self.catcodes.clone();
        self.with_catcodes(change);
        for (character, code) in self.catcodes.diff(&before) {
            self.occurrence(OccKind::Catcode, character.to_string(), Some(code.to_string()), span);
        }
    }

    /// Appends to the execution trace when `trace` is on.  The kernel is not
    /// traced: a cached one brings no events, so the trace is the document's
    /// whatever state the cache was in.
    pub fn record(&mut self, kind: Step, name: Sym, span: Span) {
        self.record_with(kind, name, span, || None::<&str>);
    }

    /// The file a trace's line and position range refers to.
    fn is_trace_file(&self, file: FileId) -> bool {
        match &self.cfg.trace_file {
            None => file == self.out.main_file,
            Some(name) => self.out.files.get(file as usize).is_some_and(|f| crate::config::names_file(&f.path, name)),
        }
    }

    /// Whether `--lines`/`--steps` keep an event at `span`: a line range
    /// keeps the main file's tokens on those lines and everything the calls
    /// there do, in macro bodies and in the files they read.
    fn in_trace_range(&self, span: Span) -> bool {
        let steps = self.cfg.trace_steps.is_none_or(|(a, b)| (a..=b).contains(&self.out.steps));
        let lines = self.cfg.trace_lines.is_none_or(|(a, b)| {
            let main = |s: &Span| self.is_trace_file(s.file);
            let anchor = match self.last_file == Some(span.file) && main(&span) {
                true => Some(span),
                false => self.file_calls.iter().filter_map(|(_, call)| *call).chain(self.file_call).map(|(_, s)| s).find(main),
            };
            anchor.is_some_and(|s| (a..=b).contains(&s.line) && (s.line > a || s.col >= self.cfg.trace_col) && (s.line < b || s.col <= self.cfg.trace_end_col))
        });
        steps && lines
    }

    /// A character typeset at `span`, kept when a trace range asks what its
    /// lines produce.
    pub(crate) fn note_text(&mut self, c: char, span: Span) {
        const LIMIT: usize = 1 << 16;
        crate::observe::typeset(self, c, span);
        if self.cfg.trace && self.cfg.trace_lines.is_some() && self.out.typeset.len() < LIMIT && !self.reading_format() && self.in_trace_range(span) {
            self.out.typeset.push(c);
        }
    }

    /// The files the macro bodies now being read live in.
    pub(crate) fn within_files(&self) -> Rc<[FileId]> {
        /// Macro bodies looked through for them.
        const DEPTH: usize = 16;
        let mut files: Vec<FileId> = Vec::new();
        for (sym, _) in self.within.iter().rev().take(DEPTH) {
            let Meaning::Macro(m) = self.env.meaning(*sym) else { continue };
            if let Some(file) = m.replacement_text.first().map(|t| t.span.file)
                && !files.contains(&file)
            {
                files.push(file);
            }
        }
        files.into()
    }

    /// [`Machine::record`] with a detail, only built when the trace is kept so
    /// a run without one does not pay for its text.
    pub fn record_with<D: Into<Box<str>>>(&mut self, kind: Step, name: Sym, span: Span, detail: impl FnOnce() -> Option<D>) {
        if !self.cfg.trace || self.trace_full || self.reading_format() || !self.in_trace_range(span) {
            return;
        }
        if self.out.trace.len() >= self.cfg.limits.trace {
            self.trace_full = true;
            self.diagnose(
                Severity::Unsupported,
                "trace-truncated",
                span,
                format!("trace stopped after {} events (limits.trace)", self.cfg.limits.trace),
            );
            return;
        }
        let index = self.out.trace.len() as u32;
        self.out.trace.push(Event {
            index,
            depth: self.within.len() as u16,
            kind,
            name,
            span,
            detail: detail().map(Into::into),
        });
    }

    /// Record a construct satex cannot follow yet, once per name and code:
    /// the run reports what it could not do instead of quietly doing nothing.
    pub fn note_gap(&mut self, code: &'static str, name: Sym, span: Span) {
        if !self.gaps.insert((code, name)) {
            if let Some(capture) = &mut self.package_capture {
                capture.log.gaps_seen.push((code, name));
            }
            return;
        }
        if let Some(capture) = &mut self.package_capture {
            capture.log.gaps_new.push((code, name, self.out.facts.diagnostics.len()));
            // The gap names this index: the diagnostic must be a new row.
            self.diag_floor = self.out.facts.diagnostics.len();
        }
        let cs = self.out.interner.cs(name);
        self.diagnose(
            Severity::Imprecision,
            code,
            span,
            format!("{cs} is not interpreted here, so its effect is not in this analysis"),
        );
    }

    pub fn diagnose(&mut self, severity: Severity, code: &'static str, span: Span, message: String) {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        message.hash(&mut hasher);
        let key = (code, span, hasher.finish());
        let diagnostics = &mut self.out.facts.diagnostics;
        if let Some(&i) = self.diag_seen.get(&key)
            && i >= self.diag_floor
            && let Some(d) = diagnostics.get_mut(i)
            && d.severity == severity
            && d.message == message
        {
            d.count = d.count.saturating_add(1);
            return;
        }
        self.diag_seen.insert(key, diagnostics.len());
        diagnostics.push(Diagnostic { severity, code, message, span, count: 1 });
    }

    /// Named things get a vertex too, so that a label, a citation or a
    /// section heading can be a slicing criterion.  The vertex is returned so
    /// that the caller can wire it to what the name stands for.
    pub fn occurrence(
        &mut self,
        kind: OccKind,
        key: String,
        detail: Option<String>,
        span: Span,
    ) -> Option<NodeId> {
        // A command that has acted has read its arguments.
        self.finish_shape();
        // A call that records the same key twice made one occurrence — but
        // every literal a call hands the PDF writer is content of its own:
        // `q … q … Q … Q` from one `\tcolorbox` must stay balanced.
        let seen = self
            .out
            .facts
            .occurrences
            .iter()
            .rev()
            .take(32)
            .filter(|_| kind != OccKind::Pdf)
            .find(|o| o.span == span && o.kind == kind && o.key == key)
            .map(|o| o.node);
        if seen.is_some() {
            return seen;
        }
        let context = self.occurrence_context();
        let node = self.occurrence_in(kind, key, detail, span, &context);
        Some(node)
    }

    /// What an occurrence recorded now would be attributed to; kept for one
    /// recorded once the run has ended.
    pub fn occurrence_context(&self) -> crate::observe::OccContext {
        crate::observe::OccContext {
            package: self.package(),
            expanded: self.project_site,
            within: self.within.last().map(|(_, node)| *node),
            cds: self.cds.clone(),
            section: self.section.clone(),
            certain: self.exact(),
        }
    }

    /// An occurrence in `context`.
    pub fn occurrence_in(
        &mut self,
        kind: OccKind,
        key: String,
        detail: Option<String>,
        span: Span,
        context: &crate::observe::OccContext,
    ) -> NodeId {
        let name = self.intern(&key);
        // A `\label` declares a key; a `\ref` or `\cite` uses one.
        let tag = match kind {
            OccKind::Label | OccKind::BibItem => VertexTag::VariableDefinition,
            _ => VertexTag::Use,
        };
        let node = self.out.graph.push_key(tag, name, span, context.within, context.cds.clone());
        if kind == OccKind::Section {
            self.section = Some(Rc::from(key.as_str()));
        }
        self.out.facts.occurrences.push(Occurrence {
            kind,
            key,
            detail,
            span,
            expanded: context.expanded.filter(|e| *e != span),
            package: context.package,
            node,
            section: context.section.clone(),
            certain: context.certain,
        });
        node
    }

    /// Whether what happens now happens in every run: no undecided
    /// conditional or split path encloses it, and the mode is one mode.
    pub(crate) fn exact(&self) -> bool {
        self.cds.is_empty() && self.split_at.is_empty() && self.mode.is_known()
    }

    /// The macros currently expanding, outermost first: what `\errmessage`
    /// reached during any of these ran while they were on the way.
    pub(crate) fn call_stack(&self) -> impl Iterator<Item = Sym> + '_ {
        self.within.iter().map(|(sym, _)| *sym)
    }

    /// A `use` vertex for `sym`, wired to whatever currently defines it.
    ///
    /// Names that carry no definition site — the primitives the format
    /// provides, and undefined names — get no vertex: they would dominate the
    /// graph without adding an edge to it.
    pub fn reference(&mut self, sym: Sym, span: Span) -> Option<NodeId> {
        let defs = self.env.defs(sym);
        if defs.is_empty() {
            return None;
        }
        let node = self.force_reference(sym, span);
        self.link_reads(node, sym, &defs);
        Some(node)
    }

    /// Make `node` depend on whatever currently defines `sym`.
    pub fn reads_definition_of(&mut self, node: NodeId, sym: Sym) {
        let defs = self.env.link_defs(sym);
        self.link_reads(node, sym, &defs);
    }

    /// A `use` vertex even for a name with no definition site, for the places
    /// that need something to hang a control dependency on.
    pub fn force_reference(&mut self, sym: Sym, span: Span) -> NodeId {
        let within = self.within.last().map(|(_, n)| *n);
        self.out.graph.push(VertexTag::Use, sym, span, within, self.cds.clone())
    }

    fn take_prefixes(&mut self) -> Prefixes {
        std::mem::take(&mut self.prefixes)
    }

    /// The definition sites a binding carries once `node` has defined `name`:
    /// just `node`, or `node` added to the sites already there when the
    /// definition is made under an undecided condition and the analysis has
    /// to keep the arms it has already walked as possible.
    fn def_sites(&self, name: Sym, node: NodeId) -> Vec<NodeId> {
        if self.cds.is_empty() {
            return vec![node];
        }
        let mut defs = self.env.link_defs(name);
        if !defs.contains(&node) {
            defs.push(node);
        }
        defs
    }

    /// `\⟨x⟩` and `\end⟨x⟩` defined at one place are an environment
    /// (ltdefns, `\newenvironment`): the half defined first is retagged
    /// when the second arrives.
    fn environment_half(&mut self, spelled: &str, span: Span) -> Option<Sym> {
        let (base, partner) = match spelled.strip_prefix("end") {
            Some(base) if !base.is_empty() => (base.to_string(), base.to_string()),
            _ => (spelled.to_string(), format!("end{spelled}")),
        };
        let partner = self.out.interner.lookup(&partner)?;
        let defs = &mut self.out.facts.defs;
        let from = defs.len().saturating_sub(64);
        let mut found = false;
        for def in defs[from..].iter_mut().filter(|d| d.name == partner && d.span == span) {
            if matches!(def.tag, "macro" | "alias" | "environment") {
                def.tag = "environment";
                found = true;
            }
        }
        found.then(|| self.intern(&base))
    }

    /// What `\newif\if⟨x⟩` makes (ltdefns): `\if⟨x⟩`, a name `\let` to
    /// `\iftrue` or `\iffalse`, and `\⟨x⟩true` and `\⟨x⟩false`, macros
    /// whose whole body is such a `\let`.  The switch is named by `⟨x⟩`.
    fn switch_of(&mut self, meaning: &Meaning, spelled: &str) -> Option<Sym> {
        use crate::builtins::Cond;
        let base = match meaning {
            Meaning::Primitive(Primitive::If(Cond::True | Cond::False)) => {
                spelled.strip_prefix("if").filter(|b| !matches!(*b, "true" | "false"))?.to_string()
            }
            Meaning::Macro(m) if m.parameter_text.items.is_empty() => {
                let [a, b, c] = &m.replacement_text[..] else { return None };
                let (Tok::Cs(a), Tok::Cs(b), Tok::Cs(c)) = (a.tok, b.tok, c.tok) else { return None };
                let target = self.name(b).strip_prefix("if")?.to_string();
                let value = self.name(c);
                let ok = self.name(a) == "let"
                    && matches!(value, "iftrue" | "iffalse")
                    && spelled == format!("{target}{}", &value[2..]);
                ok.then_some(target)?
            }
            _ => return None,
        };
        Some(self.intern(&base))
    }

    /// Bind `name`, record the definition and wire up the graph.
    /// The names the kernel keeps counters under, learnt by running its
    /// own `\value{⟨counter⟩}` (ltcounts: `\csname c@#1\endcsname`) on a
    /// probe; none while `\value` is not a macro.
    fn counter_namespace(&mut self) -> Option<String> {
        const PROBE: &str = "satex@probe";
        let value = self.out.interner.lookup("value")?;
        self.env.meaning(value).as_macro()?;
        if let Some(known) = &self.counter_namespace {
            return Some(known.clone());
        }
        let span = Span::default();
        let mut tokens = vec![Token::new(Tok::Cs(value), span), Token::new(Tok::Chr('{', Catcode::Begin), span)];
        tokens.extend(PROBE.chars().map(|c| Token::new(Tok::Chr(c, Catcode::Letter), span)));
        tokens.push(Token::new(Tok::Chr('}', Catcode::End), span));
        let expanded = self.expand_tokens(Rc::from(tokens));
        let [only] = expanded.as_slice() else { return None };
        let name = self.name(only.cs()?).to_string();
        let prefix = name.strip_suffix(PROBE)?.to_string();
        self.counter_namespace = Some(prefix.clone());
        Some(prefix)
    }

    pub fn define(
        &mut self,
        name: Sym,
        meaning: Meaning,
        by: Sym,
        mode: DefMode,
        span: Span,
        subject: Option<Sym>,
    ) -> NodeId {
        // A command that has defined something has read its arguments.
        self.finish_shape();
        if let Meaning::Register(kind, index) = &meaning {
            self.note_register_origin(*kind, *index);
        }
        crate::observe::defined_name(self, name);
        let spelled = self.name(name).to_string();
        crate::observe::version_record(self, &spelled, &meaning);
        if let Some(line) = self.source_extent(span) {
            self.out.graph.note_extent(span, line);
        }
        let prefixes = self.take_prefixes();
        let global = prefixes.global;
        let (redefines, relative) = self.redefines(name);
        let register = match &meaning {
            Meaning::Register(kind, _) => Some(*kind),
            _ => None,
        };
        let within = self.within.last().map(|(_, n)| *n);
        let node = self.out.graph.push(VertexTag::VariableDefinition, name, span, within, self.cds.clone());

        let switch = self.switch_of(&meaning, &spelled);
        let mac = meaning.as_macro().cloned();
        if let Some(m) = &mac {
            let body =
                self.out.graph.push(VertexTag::MacroDefinition, name, span, within, self.cds.clone());
            self.out.graph.edge(node, body, EdgeKind::DEFINED_BY);
            let replacement = m.replacement_text.clone();
            self.record_calls(name, &replacement);
            // What the body names, it depends on, whether or not it is ever
            // expanded.
            for token in replacement.iter() {
                let Tok::Cs(callee) = token.tok else { continue };
                let defs = self.env.link_defs(callee);
                self.link_mentions(body, callee, &defs);
            }
        }

        // `\renewcommand` and friends test that the name is already there
        // (latex.ltx, `\@ifundefined`), so they read the definition they
        // replace; `\def` and `\newcommand` do not.
        if mode == DefMode::Renew {
            let defs = self.env.link_defs(name);
            self.link_reads(node, name, &defs);
        }

        // A replacement text with unknown text in it is one of many.
        // So is one made on a path before it has met the others again.
        let certain = self.cds.is_empty() && self.split_at.is_empty() && !mac.as_ref().is_some_and(|m| self.has_unknown(&m.replacement_text));
        let environment = (spelled == "@currenvir").then(|| crate::observe::current_environment(self));
        let pattern = if name == self.unknown || name == self.unknown_rest || name == self.unknown_digits || name == self.unknown_more {
            Some((String::new(), String::new()))
        } else {
            self.name_pattern(name)
        };
        if let Some((prefix, suffix)) = pattern {
            self.wild.add(prefix, suffix);
            self.diagnose(
                Severity::Imprecision,
                "unknown-name",
                span,
                "a name built from unknown text was defined; which one is not known".into(),
            );
        } else if !self.pinned.contains(&name) {
            let mut binding = Binding::new(meaning, node);
            // An engine parameter's value stays with the parameter: latex.ltx
            // makes `\everypar` a `\newtoks` and sets the primitive through
            // `\tex_everypar:D` from then on.
            if let Some(old) = self.env.slot(name)
                && matches!(
                    old.meaning.prim(),
                    Some(
                        Primitive::IntegerParameter
                            | Primitive::DimenParameter
                            | Primitive::GlueParameter { .. }
                            | Primitive::TokensParameter
                    )
                )
            {
                binding.value = old.value.clone();
            }
            binding.certain = certain;
            binding.defs = self.def_sites(name, node);
            self.env.set(name, binding, global);
        }
        if let Some(outer) = environment {
            crate::observe::environment_opened(self, outer, span);
        }
        // A global definition made while expanding outlives the expansion,
        // which is the effect worth tracking.
        if let (true, Some(caller)) = (global, within) {
            self.out.graph.edge(node, caller, EdgeKind::SIDE_EFFECT_ON_CALL);
        }
        if let Some(call) = self.out.graph.reader {
            self.out.graph.made_by(node, call);
            // Placed at the file's call (see `register_def`), the definition
            // is that call's effect.
            if self.out.graph.vertex(call).is_some_and(|v| v.span.file == span.file && v.span != span) {
                self.out.graph.edge(node, call, EdgeKind::SIDE_EFFECT_ON_CALL);
            }
        }

        self.record(Step::Define, name, span);
        let existing = self.definition_sites.get(&(span, name)).copied();
        // Kernel code re-run in a new environment (`\list`'s own
        // `\def\@itemlabel{#1}`, once per `\begin{itemize}`/`\begin{enumerate}`)
        // hits the same site every time; the first context seen there would
        // otherwise be the only one `explain` ever finds. A context not
        // already on record for this site, however recently, is its own
        // meaning and gets its own fact.
        let current_context: Rc<[Sym]> = Rc::from(self.out.env_stack.clone());
        let context_is_new = existing.is_some_and(|i| {
            self.out.facts.defs[i].context.as_ref() != current_context.as_ref()
                && !self.out.facts.defs.iter().rev().take(256).any(|d| {
                    d.name == name && d.span == span && d.context.as_ref() == current_context.as_ref()
                })
        });
        // A package being cached records the fact it would have made even
        // where this run already has one for the site: a run that replays
        // it may not.
        if existing.is_none() || self.package_capture.is_some() || context_is_new {
            {
                // What LaTeX calls a counter is a count register in the
                // namespace `\value` reads (ltcounts), and a length is a
                // skip register (`\newlength` is `\newskip`).
                let counter = match register {
                    Some(crate::tex::RegKind::Count) => {
                        let prefix = self.counter_namespace();
                        prefix.and_then(|p| self.name(name).strip_prefix(p.as_str()).map(str::to_string))
                    }
                    _ => None,
                };
                let (tag, subject) = match (register, counter) {
                    (_, Some(rest)) => ("counter", subject.or_else(|| Some(self.intern(&rest)))),
                    _ if switch.is_some() => ("switch", subject.or(switch)),
                    (Some(crate::tex::RegKind::Skip), None) => ("length", subject),
                    // clsguide § 4.5: `\DeclareOption{⟨option⟩}` stores the
                    // option's code as `\ds@⟨option⟩`, which `\ProcessOptions`
                    // runs.
                    _ if mac.is_some() && spelled.starts_with("ds@") && spelled.len() > 3 => {
                        let option = spelled[3..].to_string();
                        ("option", subject.or_else(|| Some(self.intern(&option))))
                    }
                    _ => (self.env.meaning(by).prim().map_or("macro", category), subject),
                };
                let (tag, subject) = match self.environment_half(&spelled, span) {
                    Some(base) if matches!(tag, "macro" | "alias") => ("environment", subject.or(Some(base))),
                    _ => (tag, subject),
                };
                // Made by the command the file called: `\newcommand` runs
                // `\def` and `\newbool` runs `\newif`.
                let by = match self.file_call {
                    Some((call, at))
                        if call != name && at.file == span.file && self.env.meaning(call).as_macro().is_some() =>
                    {
                        call
                    }
                    _ => by,
                };
                let fact = Definition {
                    name,
                    by,
                    tag,
                    subject,
                    mode,
                    span,
                    package: self.package(),
                    node,
                    mac: mac.clone(),
                    depth: self.env.depth() as u16,
                    global,
                    redefines,
                    certain,
                    cds: self.cds.clone(),
                    context: current_context,
                    via: self.within.last().map(|(sym, _)| *sym),
                };
                if let Some(capture) = &mut self.package_capture {
                    capture.log.definitions.push((fact.clone(), relative));
                }
                let declared = self.name(by);
                if fact.mac.is_none()
                    && self.name(name).len() > 2
                    && self.name(name).starts_with("if")
                    && (declared.starts_with("new") || declared.starts_with("provide"))
                    && self.project_file(span.file)
                {
                    self.own_switches.insert(name);
                }
                if existing.is_none() {
                    self.definition_sites.insert((span, name), self.out.facts.defs.len());
                    self.out.facts.defs.push(fact);
                } else if context_is_new {
                    self.out.facts.defs.push(fact);
                }
            }
        }
        // A new context got its own fact above; the site's original one
        // still describes *its* context and must keep its own `mac`, not
        // this run's (a later environment's local meaning would otherwise
        // overwrite what the record for the first one shows).
        if let Some(i) = existing
            && !context_is_new
        {
            let previous = &mut self.out.facts.defs[i];
            previous.certain &= certain;
            previous.mac = mac;
        }
        // The kernel's own run defines nothing a document reports.
        if self.budget.is_none() {
            self.record_metadata(name);
        }
        node
    }

    /// `\ref{k}` depends on `\label{k}`, wherever it is.
    fn link_cross_references(&mut self) {
        // `\cite{k}` depends on the `\bibitem{k}` that declares it.
        let declares = |kind| match kind {
            OccKind::Ref => Some(OccKind::Label),
            OccKind::Cite => Some(OccKind::BibItem),
            _ => None,
        };
        let mut labels: HashMap<(OccKind, &str), NodeId> = HashMap::new();
        for occurrence in &self.out.facts.occurrences {
            if matches!(occurrence.kind, OccKind::Label | OccKind::BibItem) {
                labels.entry((occurrence.kind, &occurrence.key)).or_insert(occurrence.node);
            }
        }
        let links: Vec<(NodeId, NodeId)> = self
            .out
            .facts
            .occurrences
            .iter()
            .filter_map(|o| {
                let target = labels.get(&(declares(o.kind)?, o.key.as_str()))?;
                Some((o.node, *target))
            })
            .collect();
        for (from, to) in links {
            self.out.graph.edge(from, to, EdgeKind::READS);
        }
    }

    /// The expansion was written in the replacement text of the macro the
    /// token came from, not handed to it as an argument: that macro's
    /// expansion reached this one, directly or as its tail.
    fn observe_call(&mut self, sym: Sym, span: Span) {
        let Some(source) = self.source.filter(|_| span != Span::default()) else { return };
        let written = match self.env.meaning(source) {
            Meaning::Macro(m) => m.replacement_text.iter().any(|t| t.span == span && t.cs() == Some(sym)),
            _ => false,
        };
        if written {
            self.out.observed.add(source, sym);
        }
    }

    fn record_calls(&mut self, name: Sym, body: &[Token]) {
        // A redefinition replaces what the name calls; a scratch name the
        // kernel reuses for unrelated purposes throughout the run
        // (`\reserved@a`, `\@gtempa`, ...) would otherwise keep every
        // callee any of its bodies ever had, turning `analysis.calls` into
        // the union of everything it was ever defined to do instead of what
        // it currently does — which invents cycles `unguarded-recursion`
        // and the `recursion` query then report as real.
        self.out.calls.node(name);
        self.out.calls.clear(name);
        // What an earlier definition was seen to reach is not this one's.
        self.out.observed.clear(name);
        for token in body {
            if let Tok::Cs(callee) = token.tok {
                self.out.calls.add(name, callee);
            }
        }
    }

    /// A machine that starts where `analysis` ended, or at its
    /// `\begin{document}` with `preamble`, to run [`crate::probe`]s in:
    /// what it finds is not recorded anywhere, and `cfg` bounds it.
    pub fn sandbox(cfg: &'a Config, analysis: &Analysis, preamble: bool) -> Option<Self> {
        let (env, catcodes) = match (preamble, &analysis.preamble) {
            (true, Some(state)) => (state.0.snapshot(), state.1.clone()),
            (true, None) => return None,
            (false, _) => (analysis.env.snapshot(), analysis.catcodes.clone()),
        };
        let mut m = Machine::new(cfg, PathBuf::new());
        m.out.interner = analysis.interner.clone();
        m.out.files = analysis.files.clone();
        m.out.entry = analysis.entry.clone();
        m.out.document_depth = analysis.document_depth;
        m.env = env;
        m.catcodes = catcodes;
        m.dont_expand = m.out.interner.intern("\u{2}notexpanded:");
        m.unknown = m.out.interner.intern("\u{2}unknown:");
        m.unknown_rest = m.out.interner.intern("\u{2}unknown rest:");
        m.unknown_digits = m.out.interner.intern("\u{2}unknown digits:");
        m.unknown_more = m.out.interner.intern("\u{2}unknown more digits:");
        m.pseudo_file = m.out.interner.intern("\u{2}pseudo file:");
        m.halted = false;
        m.out.steps = 0;
        // Nobody is there to answer, but a read from the terminal (a file
        // the probe named that does not exist) must not end the run.
        let mode = m.intern("interactionmode");
        m.env.set_value(mode, crate::value::Value::Unknown, true);
        Some(m)
    }

    /// Run `text` as a file of its own, `@` a letter as it is while
    /// `\document` reads the `.aux` file: the names it defined that are
    /// still defined.
    pub fn run_line(&mut self, text: &str) -> Vec<String> {
        let file = self.register_file("\u{2}line".into(), LoadKind::Input);
        self.catcodes.set('@', Catcode::Letter);
        self.line_defined = Some(Vec::new());
        self.input.push(Frame::File { mouth: Mouth::new(text, file), package: None, catcodes: None, depth: 0, conds: 0, eof_seen: false });
        self.halted = false;
        self.out.exhausted = false;
        self.out.steps = 0;
        self.run();
        let defined = self.line_defined.take().unwrap_or_default();
        defined
            .into_iter()
            .filter(|&sym| self.env.is_defined(sym))
            .map(|sym| self.out.interner.name(sym).to_string())
            .collect()
    }

    /// Run `sym` on `text` read from a file of its own, until the main loop
    /// takes one of those characters itself or the input or budget ends.
    pub fn run_probe(&mut self, sym: Sym, text: &str) -> crate::probe::Watch {
        let file = self.register_file("\u{2}probe".into(), LoadKind::Input);
        self.probe = Some(Box::new(crate::probe::Watch::new(file)));
        self.input.push(Frame::File { mouth: Mouth::new(text, file), package: None, catcodes: None, depth: 0, conds: 0, eof_seen: false });
        let call = Token { tok: Tok::Cs(sym), span: Span::default() };
        self.push_tokens(Rc::from(vec![call]), None);
        self.run();
        let mut watch = *self.probe.take().expect("set above");
        watch.exhausted = watch.reached.is_none() && (self.halted || self.out.exhausted);
        let interner = &self.out.interner;
        let catcodes = &self.catcodes;
        let typable = |t: &Token| match t.tok {
            Tok::Chr(c, cat) => cat == Catcode::Space || catcodes.get(c) == cat,
            _ => true,
        };
        watch.render(|toks| toks.iter().all(typable).then(|| crate::tex::detokenize(toks, interner)));
        watch
    }

    /// Whether the main loop took a probe character straight from its file:
    /// the command being probed did not consume it.
    fn probe_reached(&mut self, token: Token) -> bool {
        let boxed = self.env.nest().is_some_and(|n| n.level);
        let Some(watch) = &mut self.probe else { return false };
        if token.span.file != watch.file || self.last_file != Some(watch.file) {
            return false;
        }
        watch.reached = Some(token.span.col.saturating_sub(1) as usize);
        watch.boxed = boxed;
        self.halted = true;
        true
    }

    /// A format installed from its bindings brings no definitions with it:
    /// what each kernel macro's replacement text names is read off again.
    fn replay_calls(&mut self) {
        for i in 0..self.out.interner.len() as u32 {
            if let Some(m) = self.env.meaning(Sym(i)).as_macro() {
                let body = m.replacement_text.clone();
                self.record_calls(Sym(i), &body);
            }
        }
    }

    pub fn run(&mut self) {
        while self.command() {
            self.report_progress();
        }
    }

    /// `-vv` and `-vvv`: say where the run is, so a long one does not look
    /// like a hang.
    /// What a call site's repeat count is measured against: input read, or
    /// any assignment.  expl3's `\clist_count:N` inside a loop that fills
    /// a table expands `\__clist_count:n` at one site far more often than a
    /// fixed bound, and each round of the loop assigns; `\def\a{\a}`
    /// never does.
    /// Whether the kernel format is being read: what it observes is not
    /// kept (only its meanings are), so per-token records can be skipped.
    pub(crate) fn reading_format(&self) -> bool {
        self.budget.is_some()
    }

    fn site_progress(&self) -> u64 {
        self.source_progress ^ self.env.assignments.rotate_left(32)
    }

    fn report_progress(&mut self) {
        let every = match self.cfg.verbose {
            0 | 1 => return,
            2 => 100_000,
            _ => 1_000,
        };
        if !self.out.steps.is_multiple_of(every) {
            return;
        }
        let (file, line) = self.position();
        let within: Vec<&str> = self.within.iter().rev().take(4).map(|(sym, _)| self.name(*sym)).collect();
        eprintln!("satex: {} tokens, at {file}:{line} in {}", self.out.steps, within.join(" < "));
    }

    /// The file the run is reading, innermost first.
    /// How far into the file a construct that began at `span` has read.  The
    /// mouth's position is only the construct's own when the tokens really
    /// came from that file: arguments taken from an expansion say nothing
    /// about the source.
    pub(crate) fn source_extent(&self, span: Span) -> Option<u32> {
        if self.last_file != Some(span.file) {
            return None;
        }
        self.input.iter().rev().find_map(|frame| match frame {
            Frame::File { mouth, .. } => Some(mouth.here()),
            _ => None,
        })
        .filter(|here| here.file == span.file && here.line > span.line)
        .map(|here| here.line)
    }

    /// Whether `file` is the document's own: the main file, or one beside
    /// it or below its directory.
    pub fn project_file(&self, file: FileId) -> bool {
        if file == self.out.main_file {
            return true;
        }
        let main = self.out.files.get(usize::from(self.out.main_file)).map(|f| f.path.as_str()).unwrap_or_default();
        let Some(dir) = Path::new(main).parent().filter(|d| !d.as_os_str().is_empty()) else { return false };
        self.out.files.get(usize::from(file)).is_some_and(|f| Path::new(&f.path).starts_with(dir))
    }

    /// Where the file of `span` is being read, and the text from `span` to
    /// there: what the call at `span` has read of it so far.
    pub fn call_extent(&self, span: Span) -> Option<(Span, String)> {
        self.input.iter().rev().find_map(|frame| match frame {
            Frame::File { mouth, .. } if mouth.here().file == span.file => Some((mouth.here(), mouth.since(span.line, span.col))),
            _ => None,
        })
    }

    /// The file and line the run is reading.
    fn position(&self) -> (String, u32) {
        for frame in self.input.iter().rev() {
            if let Frame::File { mouth, .. } = frame {
                let span = mouth.here();
                return (self.out.short_name(span.file).to_string(), span.line);
            }
        }
        (String::new(), 0)
    }

    /// One token as the main loop reads it, where a conditional may split the
    /// run into paths.
    pub fn command(&mut self) -> bool {
        self.command_level = true;
        self.step()
    }

    /// One token from the input.  Returns `false` at end of input or budget.
    pub fn step(&mut self) -> bool {
        // A scan that expands what makes it scan again (`\def\a{\number\a}`)
        // nests without end; TeX runs out of input stack there (tex.web
        // § 321, "TeX capacity exceeded"), and so does the run.
        // The stack grows down; a level's cost varies with the path to it,
        // so the stack taken is bounded as well as the count.
        let marker = 0u8;
        let here = std::hint::black_box(std::ptr::addr_of!(marker)) as usize;
        if self.step_depth == 0 {
            self.stack_base = here;
        }
        if self.step_depth >= MAX_NESTING || self.stack_base.saturating_sub(here) > MAX_STACK_BYTES {
            self.halt_capacity("expansion nested too deeply");
            return false;
        }
        self.step_depth += 1;
        let stepped = self.step_once();
        self.step_depth -= 1;
        stepped
    }

    /// tex.web § 94 `overflow`: "TeX capacity exceeded", which ends the run.
    pub(crate) fn halt_capacity(&mut self, what: &str) {
        if !self.halted {
            self.halted = true;
            self.out.exhausted = true;
            self.diagnose(
                Severity::Unsupported,
                "budget-exhausted",
                Span::default(),
                format!("analysis stopped: TeX capacity exceeded ({what})"),
            );
        }
    }

    fn step_once(&mut self) -> bool {
        if self.over_budget() {
            return false;
        }
        let command = std::mem::take(&mut self.command_level);
        if command && self.package_capture.is_some() {
            self.maybe_checkpoint();
        }
        self.reading_command = command;
        let token = self.read(true);
        self.reading_command = false;
        let Some(token) = token else { return false };
        if self.probe_reached(token) {
            return false;
        }
        // tex.web § 358: behind a `\noexpand` marker an expandable token
        // acts as `\relax`, which does nothing.
        if self.read_noexpanded(token)
            && token.cs().or_else(|| self.active_cs(token)).is_some_and(|sym| self.expandable(sym))
        {
            return true;
        }
        // A command the main loop takes from a file is read on no call's
        // behalf.
        if command && self.within.is_empty() && self.last_file == Some(token.span.file) {
            self.out.graph.reader = None;
        }
        match token.tok {
            Tok::Cs(sym) if command && self.split_meaning(sym, token) => {}
            Tok::Cs(sym) => {
                self.command_level = command && matches!(self.env.meaning(sym).prim(), Some(Primitive::If(_) | Primitive::SkipCrossing { .. }));
                self.do_control_sequence(sym, token.span);
                self.command_level = false;
            }
            // tex.web § 344: an active character acts as the control sequence
            // of that single character, while staying a character token.
            Tok::Chr(c, Catcode::Active) => {
                let sym = self.active_sym(c);
                if !(command && self.split_meaning(sym, token)) {
                    self.do_control_sequence(sym, token.span);
                }
            }
            _ => self.do_character(token),
        }
        true
    }

    /// A character token that is not typeset material: the ones that open and
    /// close a group.  A control sequence `\let` to such a character comes
    /// here too (tex.web § 1063).
    fn do_character(&mut self, token: Token) {
        match self.category_of(token) {
            Some(Catcode::Begin) => {
                self.end_word();
                self.open_group(GroupKind::Simple, token.span);
                self.record(Step::OpenGroup, Sym(0), token.span);
                self.simple_group_opened();
            }
            Some(Catcode::End) => {
                self.end_word();
                crate::observe::text_break(self);
                self.close_group(GroupKind::Simple, token.span);
                self.record(Step::CloseGroup, Sym(0), token.span);
            }
            Some(Catcode::Math) => {
                crate::observe::text_break(self);
                self.math_shift_token(token)
            }
            // tex.web § 1043: a space in horizontal mode is glue.
            Some(Catcode::Space) => {
                self.note_text(' ', token.span);
                if self.packages.is_empty() {
                    let modes = self.out.facts.spaces.entry(token.span).or_default();
                    *modes = modes.union(self.mode);
                }
                if self.mode.meets(crate::mode::Modes::ANY_HORIZONTAL) {
                    let (width, spec) = self.space_glue(false);
                    self.append_item(crate::mode::Item::Glue(width, spec));
                }
            }
            // tex.web § 1090: a letter or other character is horizontal
            // material.
            Some(Catcode::Letter | Catcode::Other) => {
                if !self.horizontal_material(token) {
                    if let Some(c) = self.typeset_code(token).and_then(char::from_u32) {
                        self.note_text(c, token.span);
                    }
                    match self.typeset_code(token) {
                        Some(c) => self.append_char(c),
                        None => self.append_item(crate::mode::Item::Unknown),
                    }
                }
            }
            // A command whose meaning is unknown may do anything to the mode.
            None if token.cs().is_some_and(|sym| matches!(self.env.meaning(sym), Meaning::Unknown)) => {
                self.widen_mode()
            }
            _ => {
                crate::observe::text_break(self);
                self.end_word()
            }
        }
    }

    /// The code of a character token, or of the character a control
    /// sequence is `\let` to.
    fn typeset_code(&self, token: Token) -> Option<u32> {
        match token.tok {
            Tok::Chr(c, _) => Some(u32::from(c)),
            Tok::Cs(sym) => match self.env.meaning(sym) {
                Meaning::Char(c, _) => Some(u32::from(c)),
                _ => None,
            },
            _ => None,
        }
    }

    /// Every collection the run appends to has a ceiling; the first one that
    /// is reached stops the analysis instead of growing the process.
    fn over_budget(&mut self) -> bool {
        /// Collection sizes change slowly; checking them on every token would
        /// cost more than it saves.
        const CHECK_INTERVAL: u64 = 256;
        if self.halted {
            return true;
        }
        if self.out.steps < self.cfg.limits.steps && !self.out.steps.is_multiple_of(CHECK_INTERVAL) {
            return false;
        }
        if self.out.steps.saturating_sub(self.progress_step) > self.cfg.limits.stall_tokens {
            self.abandon_stall();
        }
        let facts = &self.out.facts;
        let used = facts.defs.len() + facts.expansions.len() + facts.occurrences.len();
        let max_steps = self.budget.unwrap_or(self.cfg.limits.steps);
        let reason = if self.out.steps.saturating_add(self.work) >= max_steps {
            "token budget"
        } else if self.out.graph.len() >= self.cfg.limits.vertices {
            "dependency graph size"
        } else if used >= self.cfg.limits.facts {
            "fact budget"
        } else if self.input.len() >= self.cfg.limits.expansion_stack {
            "input stack depth"
        } else if self.cfg.limits.seconds > 0 && self.started.elapsed().as_secs() >= self.cfg.limits.seconds {
            "time limit (limits.seconds)"
        } else if self.held_tokens >= self.cfg.limits.held_tokens {
            "held token count"
        } else if self.env.journal_len() >= self.cfg.limits.save_stack {
            "save stack size"
        } else if self.cfg.limits.memory.as_u64() > 0
            && crate::budget::in_use() as u64 >= self.cfg.limits.memory.as_u64()
        {
            "memory limit"
        } else {
            return false;
        };
        self.halted = true;
        self.out.exhausted = true;
        self.diagnose(
            Severity::Unsupported,
            "budget-exhausted",
            Span::default(),
            format!("analysis stopped: {reason} exceeded"),
        );
        true
    }

    /// A loop that has read nothing from a file for `limits.stall_tokens`
    /// tokens is one satex's approximations keep from ending.  It is widened
    /// to every state it could leave: whatever it assigned becomes unknown,
    /// what it has pending is dropped and the file is read on.
    fn abandon_stall(&mut self) {
        self.env.widen_recent();
        // The macros the loop was running, innermost first: what to look at.
        let running: Vec<String> =
            self.within.iter().rev().take(4).map(|(sym, _)| self.out.interner.cs(*sym)).collect();
        while matches!(self.input.last(), Some(Frame::Tokens { .. } | Frame::Pseudo { .. } | Frame::Returned(_))) {
            self.pop_frame();
        }
        self.progress_step = self.out.steps;
        let (file, line) = self.position();
        let running = if running.is_empty() { String::new() } else { format!(" in {}", running.join(" < ")) };
        self.diagnose(
            Severity::Unsupported,
            "stalled-loop",
            Span::default(),
            format!(
                "no input read for {} tokens at {file}:{line}{running}; the loop was widened: what it assigned is unknown",
                self.cfg.limits.stall_tokens
            ),
        );
    }

    /// Run a token list to completion, then return.
    pub fn run_tokens(&mut self, toks: Rc<[Token]>) {
        if toks.is_empty() {
            return;
        }
        self.input.push(Frame::Boundary);
        let base = self.input.len();
        self.push_tokens(toks, None);
        while self.input.len() > base && self.command() {}
        while self.input.len() >= base {
            self.pop_frame();
        }
    }

    /// Whether the gullet expands this control sequence: TeX calls `expand`
    /// only for the commands above `max_command` (tex.web § 366).
    pub fn expandable(&self, sym: Sym) -> bool {
        self.expandable_meaning(&self.env.meaning(sym))
    }

    /// The meaning an active character carries (tex.web § 344).  Active
    /// characters have their own region of the table of equivalents
    /// (tex.web § 222, `active_base`), apart from the control symbol of the
    /// same character: `~` and `\~` are two meanings, and `\csname ~\endcsname`
    /// is the control symbol.
    pub fn active_sym(&mut self, c: char) -> Sym {
        self.intern(&format!("{}{c}", crate::tex::ACTIVE))
    }

    pub fn active_cs(&mut self, token: Token) -> Option<Sym> {
        match token.tok {
            Tok::Chr(c, Catcode::Active) => Some(self.active_sym(c)),
            _ => None,
        }
    }

    /// The same test on a meaning already in hand.
    pub fn expandable_meaning(&self, meaning: &Meaning) -> bool {
        match meaning {
            Meaning::Macro(_) => true,
            Meaning::Primitive(p) => crate::builtins::expandable(*p),
            _ => false,
        }
    }

    /// The replacement text `\edef` stores: tex.web § 477 reads the body with
    /// the gullet, so every expandable token is expanded until none is left,
    /// what the gullet cannot expand is kept as it stands, `\noexpand` passes
    /// the token after it through untouched (tex.web § 369), and e-TeX's
    /// `\protected` macros are copied rather than expanded.
    /// Append tokens tex.web § 478 stores without looking at them.  In a
    /// replacement text still to be scanned for parameters, a `#` among them
    /// is doubled, which that scan reads back as the one character.
    fn store_literally(&mut self, out: &mut Vec<Token>, tokens: Vec<Token>) {
        if self.def_body_depth != Some(self.edef_depth) {
            out.extend(tokens);
            return;
        }
        for t in tokens {
            if t.is_cat(Catcode::Param) {
                out.push(t);
            }
            out.push(t);
        }
    }

    pub fn expand_tokens(&mut self, toks: Rc<[Token]>) -> Vec<Token> {
        let mut out = Vec::new();
        if toks.is_empty() {
            return out;
        }
        // A conditional met here makes text, which cannot split into paths.
        self.edef_depth += 1;
        self.input.push(Frame::Boundary);
        let base = self.input.len();
        self.push_tokens(toks, None);
        while out.len() < self.cfg.limits.expansion_tokens {
            let Some(token) = self.read(false) else { break };
            if !self.expand_into(token, &mut out) {
                break;
            }
        }
        while self.input.len() >= base {
            self.pop_frame();
        }
        self.edef_depth -= 1;
        out
    }

    /// One token of an expanding scan (tex.web § 477): an expandable one is
    /// expanded, anything else is appended to `out` as it stands.  `false`
    /// when the run cannot go on.
    fn expand_into(&mut self, token: Token, out: &mut Vec<Token>) -> bool {
        let cs = match token.tok {
            Tok::Cs(sym) => Some(sym),
            _ => self.active_cs(token),
        };
        let Some(sym) = cs else {
            out.push(token);
            return true;
        };
        if self.read_noexpanded(token) {
            out.push(token);
            return true;
        }
        if self.copy_target == Some((sym, self.edef_depth))
            && self.def_body_depth == Some(self.edef_depth)
            && let Some(going) = self.copy_through(sym, token, out)
        {
            return going;
        }
        match self.env.meaning(sym) {
            // tex.web § 367: only a control sequence or active character is
            // marked; any other token goes back unchanged.
            Meaning::Primitive(Primitive::NoExpand) => {
                // tex.web § 367: read with the scanner normal.
                let outer = std::mem::replace(&mut self.scanner, Scanner::Normal);
                let next = self.read(false);
                self.scanner = outer;
                if let Some(next) = next {
                    if next.cs().is_some() || self.active_cs(next).is_some() {
                        out.push(next);
                    } else {
                        self.unread(next);
                    }
                }
            }
            Meaning::Macro(m) if m.protected && !self.pdf_string => out.push(token),
            // A name whose meaning is unknown may expand to anything.
            // A name of unknown meaning expands to unknown text; one a join
            // left one of several macros to one of their texts.
            Meaning::Unknown if self.is_unknown_marker(sym) => out.push(self.unknown_token(token.span)),
            Meaning::Unknown => {
                let text = self.one_of_texts(sym, token.span);
                out.push(text);
            }
            Meaning::Undefined if self.may_be_defined(sym) => out.push(self.unknown_token(token.span)),
            // `\unexpanded{…}` contributes its group to the result with
            // nothing expanded, which is what `\edef` stores (eTeX manual
            // § 3.4).
            Meaning::Primitive(Primitive::Reinject) => {
                let body = self.read_general_text();
                self.store_literally(out, body);
            }
            // tex.web § 478: what `\the` yields is stored, not expanded
            // again.
            Meaning::Primitive(Primitive::Text(crate::builtins::TextOf::The)) => {
                let tokens = self.the_tokens(token.span);
                self.store_literally(out, tokens);
            }
            meaning if self.expandable_meaning(&meaning) => {
                self.unread(token);
                return self.step();
            }
            _ => out.push(token),
        }
        true
    }

    /// `\edef\x{…\x…}` where `\x`'s text is a copied text
    /// ([`crate::env::copied_text`]): the call is made as any is, and its
    /// text appended at once, which is what reading it token by token does,
    /// since no token of it does anything.  Its meaning is read as a
    /// [`ReadKind::Copy`](crate::env::ReadKind::Copy).  `None` when the text
    /// is no copied text or does not fit the limits, and it is read as usual.
    fn copy_through(&mut self, sym: Sym, token: Token, out: &mut Vec<Token>) -> Option<bool> {
        if self.env.forking() {
            return None;
        }
        let text = self.env.slot(sym).and_then(|b| crate::env::copied_text(&b.meaning)).cloned()?;
        let limits = &self.cfg.limits;
        let len = text.len() as i64;
        let room = (limits.expansion_tokens as i64 - len).min(limits.held_tokens as i64 - self.held_tokens as i64 - len);
        if out.len() + text.len() >= limits.expansion_tokens || room < 0 {
            return None;
        }
        let fresh = self.env.meaning_fresh(sym);
        self.env.set_copying(Some(sym));
        self.unread(token);
        let going = self.step();
        self.env.set_copying(None);
        let pushed = matches!(
            self.input.last(),
            Some(Frame::Tokens { toks, pos: 0, expanding: Some(s) })
                if *s == sym && toks.len() == text.len()
                    && toks.iter().zip(text.iter()).all(|(a, b)| a.tok == b.tok && a.span == b.span)
        );
        if !pushed {
            // Widened or abandoned: its tokens are read as they come.
            self.env.note_read(sym, crate::env::ReadKind::Meaning);
            return Some(going);
        }
        self.pop_frame();
        self.copies.push(CopyEvent { sym, at: out.len(), len: text.len(), room, fresh });
        out.extend(text.iter().copied());
        Some(going)
    }

    /// The replacement text of an `\edef` or `\xdef` after its `{`, scanned
    /// while it is expanded (tex.web § 477): the text ends at the `}` that
    /// balances the braces expansion leaves, so `{\iffalse}\fi … }` spans
    /// what follows, which expl3's `\tex_edef:D … { \if_false: } \fi:`
    /// relies on.
    pub fn scan_expanded_def_body(&mut self) -> Vec<Token> {
        self.scan_expanded_body(true)
    }

    /// The ⟨balanced text⟩ after a `{` that `\expanded`, `\message` and
    /// their relatives read while expanding it (tex.web § 473, `scan_toks`
    /// with `xpand`): a replacement text when `def`, whose `#`s from `\the`
    /// and `\unexpanded` stay characters.
    pub fn scan_expanded_body(&mut self, def: bool) -> Vec<Token> {
        let scanner = std::mem::replace(&mut self.scanner, if def { Scanner::Defining } else { Scanner::Absorbing });
        let body = self.scan_expanded_body_in(def);
        self.scanner = scanner;
        body
    }

    fn scan_expanded_body_in(&mut self, def: bool) -> Vec<Token> {
        let saved = if def { self.def_body_depth.replace(self.edef_depth + 1) } else { self.def_body_depth.take() };
        self.edef_depth += 1;
        let mut out = Vec::new();
        let mut depth = 0usize;
        while out.len() < self.cfg.limits.expansion_tokens {
            let Some(token) = self.read(false) else { break };
            match token.tok {
                Tok::Chr(_, Catcode::Begin) => depth += 1,
                Tok::Chr(_, Catcode::End) if depth == 0 => break,
                Tok::Chr(_, Catcode::End) => depth -= 1,
                _ => {
                    if !self.expand_into(token, &mut out) {
                        break;
                    }
                    continue;
                }
            }
            out.push(token);
        }
        self.edef_depth -= 1;
        self.def_body_depth = saved;
        out
    }

    fn do_control_sequence(&mut self, sym: Sym, span: Span) {
        if self.project_file(span.file) {
            self.project_site = Some(span);
        }
        // A control sequence read straight from a file starts a new call; one
        // that came out of a macro body is part of the call already running.
        if self.last_file == Some(span.file) {
            self.file_call = Some((sym, span));
            self.finish_shape();
            self.call_shape = Some((sym, span, String::new(), self.within.len()));
        }
        match self.env.meaning(sym) {
            Meaning::Primitive(p) => {
                self.record(Step::Execute, sym, span);
                let node = self.reference(sym, span);
                self.note_expansion(sym, span, MeaningKind::Primitive, node, Vec::new());
                // tex.web § 1038: a command that is neither a character nor
                // expands ends the word being set.
                if !crate::builtins::expandable(p) && p != Primitive::Mode(crate::builtins::ModeCmd::Horizontal(crate::builtins::Material::Char)) {
                    self.end_word();
                }
                self.execute(p, sym, span);
                // A primitive reads everything it takes while it runs.
                self.finish_shape();
            }
            Meaning::Macro(m) => self.expand_macro(sym, m, span),
            Meaning::Undefined if self.may_be_defined(sym) => self.widen_mode(),
            Meaning::Undefined => {
                // An expl3 name carries its own arity in its signature, so a
                // kernel function this build does not model can still be
                // stepped over correctly instead of reported as undefined.
                match crate::tex::expl3_arity(self.name(sym)) {
                    Some(arity) => {
                        let node = self.reference(sym, span);
                        self.note_expansion(sym, span, MeaningKind::Unknown, node, Vec::new());
                        for _ in 0..arity {
                            self.read_undelimited();
                        }
                    }
                    None => {
                        let node = self.reference(sym, span);
                        self.note_expansion(sym, span, MeaningKind::Undefined, node, Vec::new());
                        self.record(Step::Undefined, sym, span);
                    }
                }
            }
            Meaning::Register(kind, _) => {
                let node = self.reference(sym, span);
                self.note_expansion(sym, span, MeaningKind::Register, node, Vec::new());
                // A `\chardef` constant in the stomach typesets its character
                // (tex.web § 1038); only a register takes an assignment.
                // `\newbox` makes a `\chardef` (plain.tex `\alloc@`), which
                // typesets its character.
                if matches!(kind, crate::tex::RegKind::Char | crate::tex::RegKind::Box) {
                    if !self.horizontal_material(Token::new(Tok::Cs(sym), span)) {
                        match self.env.value(sym).as_int().and_then(|c| u32::try_from(c).ok()) {
                            Some(c) => {
                                if let Some(ch) = char::from_u32(c) {
                                    self.note_text(ch, span);
                                }
                                self.append_char(c)
                            }
                            None => self.append_item(crate::mode::Item::Unknown),
                        }
                    }
                } else if kind != crate::tex::RegKind::MathChar {
                    self.end_word();
                    self.apply_global_defs();
                    self.assign_register(sym, kind, span);
                    // tex.web § 1269: a register assignment is a prefixed
                    // command too, and hands back the `\afterassignment`
                    // token once it is done (`\@defaultunits`).
                    if let Some(token) = self.after_assignment.take() {
                        self.unread(token);
                    }
                }
            }
            other => {
                // tex.web § 1211: whatever the command is, a prefix before it
                // is spent.
                self.prefixes = Default::default();
                let node = self.reference(sym, span);
                let kind = match other {
                    Meaning::Char(..) => MeaningKind::Char,
                    _ => MeaningKind::Unknown,
                };
                self.note_expansion(sym, span, kind, node, Vec::new());
                // An implicit `{`, `}` or `$` acts like the character it was
                // `\let` to (tex.web § 1063): `\bgroup` opens a group.
                self.do_character(Token::new(Tok::Cs(sym), span));
            }
        }
    }

    pub(crate) fn note_expansion(
        &mut self,
        name: Sym,
        span: Span,
        meaning: MeaningKind,
        node: Option<NodeId>,
        arguments: Vec<Box<[Token]>>,
    ) -> u32 {
        self.note_site(span, name);
        let progress = self.site_progress();
        // A site this run already had before a package being cached reached
        // it: the package records the fact it would have made, for a run
        // that does not have the site.
        if let Some(capture) = &mut self.package_capture
            && self.expansion_sites.get(&(span, name)).is_some_and(|site| site.fact < capture.expansions_start)
            && !capture.log.touched.contains_key(&(span, name))
        {
            let fact = Expansion {
                name,
                span,
                package: self.packages.last().map(|(sym, _)| *sym),
                node,
                meaning,
                within: self.within.last().map(|(sym, _)| *sym),
                arguments: arguments.clone(),
                cds: self.cds.clone(),
                count: 0,
                mode: self.mode,
            };
            let count = self.expansion_sites.get(&(span, name)).map_or(0, |site| self.out.facts.expansions[site.fact].count);
            capture.log.touched.insert((span, name), (fact, count));
        }
        if let Some(site) = self.expansion_sites.get_mut(&(span, name)) {
            // A loop is repetition that reads nothing new: `\loop … \repeat`
            // and expl3's mapping functions re-expand one site while the
            // source stands still.  A helper called again from a later line
            // has made progress, so its count starts over and the per-site
            // bound only ever bites a loop.
            site.repeats =
                if site.progress == progress { site.repeats.saturating_add(1) } else { 1 };
            site.progress = progress;
            let (repeats, fact) = (site.repeats, site.fact);
            let mode = self.mode;
            let previous = &mut self.out.facts.expansions[fact];
            previous.count = previous.count.saturating_add(1);
            previous.mode = previous.mode.union(mode);
            return repeats;
        }
        let package = self.package();
        let within = self.within.last().map(|(sym, _)| *sym);
        let site = Site { fact: self.out.facts.expansions.len(), repeats: 1, progress };
        self.expansion_sites.insert((span, name), site);
        self.out.facts.expansions.push(Expansion {
            name,
            span,
            package,
            node,
            meaning,
            within,
            arguments,
            cds: self.cds.clone(),
            count: 1,
            mode: self.mode,
        });
        1
    }

    /// A conditional at `at` was evaluated: `taken` is the arm its test
    /// selected, `None` when undecided.  Package code is left out, as a
    /// cached package's run would not see it.
    pub(crate) fn note_conditional(&mut self, name: Sym, at: Span, taken: Option<Option<u32>>) {
        if !self.packages.is_empty() {
            return;
        }
        let facts = &mut self.out.facts.conditionals;
        let index = *self.cond_sites.entry(at).or_insert_with(|| {
            facts.push(crate::facts::Conditional { name, at, arms: Vec::new(), fi: None, taken: Vec::new(), undecided: false });
            facts.len() - 1
        });
        let fact = &mut facts[index];
        match taken {
            Some(arm) if !fact.taken.contains(&arm) => fact.taken.push(arm),
            Some(_) => {}
            None => fact.undecided = true,
        }
    }

    /// The `\else`, `\or` or (`fi`) `\fi` at `mark` of the conditional at
    /// `at`; one in another file does not make the two one construct.
    pub(crate) fn note_cond_mark(&mut self, at: Span, mark: Span, fi: bool) {
        let Some(&index) = self.cond_sites.get(&at).filter(|_| mark.file == at.file) else { return };
        let fact = &mut self.out.facts.conditionals[index];
        if fi {
            fact.fi = Some(mark);
        } else if !fact.arms.contains(&mark) {
            fact.arms.push(mark);
        }
    }

    fn expand_macro(&mut self, sym: Sym, m: Rc<MacroDef>, span: Span) {
        let node = self.out.graph.push(
            VertexTag::MacroCall,
            sym,
            span,
            self.within.last().map(|(_, n)| *n),
            self.cds.clone(),
        );
        let defs = self.env.defs(sym);
        let definition = defs.first().copied();
        self.link_reads(node, sym, &defs);
        if let Some(caller) = self.within.last().map(|(name, _)| *name) {
            self.out.calls.add(caller, sym);
        }
        self.observe_call(sym, span);

        // Widening decided before the arguments are read: a tail-recursive
        // loop would otherwise have its next iteration's tokens eaten by a
        // call that is then abandoned, and the rest of the file with them.
        self.end_used_up_lists();
        let depth = self.expanding.get(sym.0 as usize).copied().unwrap_or(0);
        // The count only stands while the source stands still, the way
        // `record_expansion` keeps it: widening returns before the site is
        // recorded again, so a site that once tripped the bound would stay
        // tripped for the rest of the run even though later calls read new
        // source.  That is what stopped `\DeclareMathSymbol` after 32 symbols
        // and left every math symbol past `\Lambda` undefined.
        let site = self
            .expansion_sites
            .get(&(span, sym))
            .filter(|site| site.progress == self.site_progress())
            .map_or(0, |site| site.repeats);
        if depth >= self.cfg.limits.expansion_depth
            || site > self.cfg.limits.site_expansions
        {
            self.record_with(Step::Widen, sym, span, || Some("recursion"));
            self.diagnose(
                Severity::Imprecision,
                "recursion-widened",
                span,
                format!(
                    "\\{}: re-entered {depth} times, expanded {site} times at this call; \
expansion stopped",
                    self.name(sym)
                ),
            );
            return;
        }

        let arguments = self.read_arguments(&m);
        if let Some(watch) = &mut self.probe {
            let text = &m.parameter_text;
            let delimited: Vec<bool> = (1..=arguments.len())
                .map(|n| {
                    let at = text.items.iter().position(|i| matches!(i, ParamItem::Param(k) if *k as usize == n));
                    at.is_some_and(|at| match text.items.get(at + 1) {
                        Some(ParamItem::Lit(_)) => true,
                        None => text.brace_end,
                        _ => false,
                    })
                })
                .collect();
            watch.bound(sym, &arguments, &delimited);
        }
        if std::mem::take(&mut self.runaway) {
            self.diagnose(
                Severity::Imprecision,
                "runaway-argument",
                span,
                format!(
                    "\\{}: a \\par ended an argument of a macro that is not \\long; the call is abandoned",
                    self.name(sym)
                ),
            );
            return;
        }
        if self.cfg.record_arguments {
            for argument in arguments.iter().filter(|a| !a.is_empty()) {
                let first = argument[0];
                let named = argument.iter().find_map(Token::cs).unwrap_or(sym);
                let value = self.out.graph.push(
                    VertexTag::Value,
                    named,
                    first.span,
                    Some(node),
                    self.cds.clone(),
                );
                self.out.graph.edge(node, value, EdgeKind::ARGUMENT);
                // An argument depends on whatever its tokens name.
                for token in argument.iter() {
                    let Tok::Cs(referenced) = token.tok else { continue };
                    let defs = self.env.link_defs(referenced);
                    self.link_reads(value, referenced, &defs);
                }
            }
        }
        let recorded = if self.cfg.record_arguments {
            arguments.iter().map(|a| a.clone().into_boxed_slice()).collect()
        } else {
            Vec::new()
        };
        let resolved = MeaningKind::Macro(definition.unwrap_or(node));
        let visits = self.note_expansion(sym, span, resolved, Some(node), recorded);
        self.note_repeated_load(sym, &arguments);

        let depth = self.expanding.get(sym.0 as usize).copied().unwrap_or(0);
        if depth >= self.cfg.limits.expansion_depth || visits > self.cfg.limits.site_expansions {
            self.record_with(Step::Widen, sym, span, || Some("recursion"));
            if visits == self.cfg.limits.site_expansions + 1 || depth >= self.cfg.limits.expansion_depth {
                let reason = if depth >= self.cfg.limits.expansion_depth {
                    format!("re-entered {depth} times")
                } else {
                    format!("expanded {visits} times without reading on")
                };
                self.diagnose(
                    Severity::Imprecision,
                    "recursion-widened",
                    span,
                    format!("\\{}: {reason}; expansion stopped", self.name(sym)),
                );
            }
            return;
        }

        self.record(Step::Expand, sym, span);
        // The budget bounds what an expansion adds; arguments it was handed
        // were already there (a `filecontents` line read from the file is
        // one argument, however long), and each `#n` copies one.
        let handed: usize = arguments.iter().map(Vec::len).max().unwrap_or(0);
        let copies = m.replacement_text.iter().filter(|t| matches!(t.tok, Tok::Param(_))).count();
        let limit = self.cfg.limits.expansion_tokens.saturating_add(handed.saturating_mul(copies));
        let Some(body) = substitute(&m, &arguments, limit) else {
            self.diagnose(
                Severity::Unsupported,
                "expansion-too-large",
                span,
                format!("\\{} would expand beyond the token limit", self.name(sym)),
            );
            return;
        };
        self.end_used_up_lists();
        self.bump_expanding(sym, 1);
        // A call is read on behalf of the last call read from the file until
        // the file is read again: what an expansion leaves behind for the
        // main loop is still that call's doing.
        if self.out.graph.reader.is_none() {
            self.out.graph.reader = Some(node);
        }
        self.within.push((sym, node));
        let before = self.input.len();
        self.push_tokens(body, Some(sym));
        if self.input.len() == before {
            self.within.pop();
        }
    }

    fn read_arguments(&mut self, m: &MacroDef) -> Vec<Vec<Token>> {
        let outer = std::mem::replace(&mut self.long_argument, m.long);
        let interrupted = std::mem::take(&mut self.runaway);
        let scanner = std::mem::replace(&mut self.scanner, Scanner::Matching);
        let arguments = match &m.arg_spec {
            Some(spec) => self.read_signature(spec),
            None => self.match_parameter_text(&m.parameter_text),
        };
        self.scanner = scanner;
        self.long_argument = outer;
        if !self.runaway {
            self.runaway = interrupted;
        }
        arguments
    }

    /// tex.web § 392/396: a macro not declared `\long` may not have `\par`
    /// in an argument.  TeX puts the `\par` back, reports "Paragraph ended
    /// before … was complete", and abandons the call, so nothing is
    /// substituted and the tokens read so far are dropped.
    fn runaway_argument(&mut self, t: Token) -> bool {
        if self.long_argument {
            return false;
        }
        let Tok::Cs(sym) = t.tok else { return false };
        if self.name(sym) != "par" {
            return false;
        }
        self.unread(t);
        self.runaway = true;
        true
    }

    /// tex.web § 443: a constant is followed by one optional space.
    pub fn skip_optional_space(&mut self) {
        if let Some(t) = self.peek()
            && t.is_space() {
                self.next_token();
            }
    }

    pub fn skip_spaces(&mut self) {
        while let Some(t) = self.next_token() {
            if !t.is_space() {
                self.unread(t);
                return;
            }
        }
    }

    /// Whether a read that has taken `expanded` tokens out of expansions may
    /// go on.  Tokens read straight from a file do not count against
    /// `limits.expansion_tokens`: a file is finite, so a read walking source
    /// cannot run away, and the document body is source rather than an
    /// argument.  What the budget bounds is an argument an expansion keeps
    /// filling, which has no end of its own.
    fn reading_on(&self, expanded: usize) -> bool {
        expanded < self.cfg.limits.expansion_tokens
    }

    /// Whether the token [`Machine::next_token`] just returned counts against
    /// [`Machine::reading_on`].
    fn expansion_delta(&self) -> usize {
        usize::from(self.last_file.is_none())
    }

    /// The contents of a `{…}` group, braces stripped, nesting preserved.
    ///
    /// A group that does not close before the expansion budget runs out is
    /// not a group: TeX's conditionals may leave braces unbalanced across the
    /// arms this interpreter analyzes separately.  The tokens go back and the
    /// caller gets nothing, which keeps one bad read from swallowing a file.
    pub fn read_group_body(&mut self) -> Vec<Token> {
        let mut out = Vec::new();
        let mut depth = 1usize;
        let mut expanded = 0;
        while self.reading_on(expanded) {
            let Some(t) = self.next_token() else { break };
            expanded += self.expansion_delta();
            if self.runaway_argument(t) {
                return Vec::new();
            }
            match t.tok {
                Tok::Chr(_, Catcode::Begin) => depth += 1,
                Tok::Chr(_, Catcode::End) => {
                    depth -= 1;
                    if depth == 0 {
                        return out;
                    }
                }
                _ => {}
            }
            out.push(t);
        }
        let at = out.first().map(|t| t.span).unwrap_or_default();
        self.diagnose(Severity::Unsupported, "unbalanced-group", at, "group never closes".into());
        self.unread_all(&out);
        Vec::new()
    }

    /// TeX's undelimited argument: one token, or a braced group with the outer
    /// braces removed (The TeXbook, ch. 20).
    /// Note an argument the call being described just read, if it read it
    /// from the file the call itself stands in and after the call.
    fn note_shape(&mut self, at: Span, item: &str) {
        let from_file = self.last_file;
        let Some((_, call, shape, _)) = &mut self.call_shape else { return };
        if from_file != Some(call.file) || at.file != call.file {
            return;
        }
        if (at.line, at.col) < (call.line, call.col) {
            return;
        }
        shape.push_str(item);
    }

    /// Keep the fullest shape seen for a command: a call that leaves an
    /// optional argument out reads fewer items than one that gives it, and
    /// the fuller call is the one that describes the command.
    fn finish_shape(&mut self) {
        let Some((sym, span, shape, _)) = self.call_shape.take() else { return };
        self.note_call_finished(sym, span);
        if shape.is_empty() {
            return;
        }
        if let Some(capture) = &mut self.package_capture
            && capture.seg_call != Some((sym, span))
        {
            capture.log.shapes.push((sym, shape.clone()));
        }
        let slot = self.out.facts.shapes.entry(sym).or_default();
        if shape.len() > slot.len() {
            *slot = shape;
        }
    }

    pub fn read_undelimited(&mut self) -> Vec<Token> {
        self.skip_spaces();
        match self.next_token() {
            None => Vec::new(),
            Some(t) if t.is_cat(Catcode::Begin) => {
                self.note_shape(t.span, "{}");
                self.read_group_body()
            }
            // An unmatched `}` can never become an argument: TeX puts it back
            // and takes an empty argument (tex.web § 395, "Argument of \x has
            // an extra }").  Letting it through would replay one source brace
            // once per occurrence of the parameter and close groups that were
            // opened before the call.
            Some(t) if t.is_cat(Catcode::End) => {
                self.unread(t);
                self.diagnose(
                    Severity::Unsupported,
                    "extra-right-brace",
                    t.span,
                    "an argument ran into a `}`".into(),
                );
                Vec::new()
            }
            Some(t) if self.runaway_argument(t) => Vec::new(),
            Some(t) => {
                self.note_shape(t.span, "{}");
                vec![self.one_token(t)]
            }
        }
    }

    /// Unknown digits taken as one token (an undelimited argument, the
    /// token `\futurelet` looks at) are their first digit, one character
    /// of category 12; the rest, perhaps none, is read next (tex.web § 465
    /// prints a number as characters).
    pub(crate) fn one_token(&mut self, t: Token) -> Token {
        if t.tok != Tok::Cs(self.unknown_digits) {
            return t;
        }
        self.unread(Token::new(Tok::Cs(self.unknown_more), t.span));
        Token::new(Tok::Chr(crate::tex::UNKNOWN_DIGIT, Catcode::Other), t.span)
    }

    /// Everything up to the next occurrence of `delimiter` at brace level 0.
    fn read_delimited(&mut self, delimiter: &[Tok]) -> Vec<Token> {
        let mut out: Vec<Token> = Vec::new();
        let mut depth = 0usize;
        let mut expanded = 0;
        while self.reading_on(expanded) {
            let Some(t) = self.next_token() else { break };
            expanded += self.expansion_delta();
            if self.runaway_argument(t) {
                return Vec::new();
            }
            // Unknown text may hold the delimiter, with anything after it:
            // the argument ends in it and the rest of it is read next.  That
            // rest is taken to hold no delimiter, so a split is made once.
            if depth == 0 && t.tok == Tok::Cs(self.unknown) {
                out.push(t);
                let rest = self.unknown_rest_token(t.span);
                self.unread(rest);
                return strip_braces(out);
            }
            match t.tok {
                Tok::Chr(_, Catcode::Begin) => depth += 1,
                // A `}` that closes a group the argument did not open ends the
                // call: TeX reports "Argument of \x has an extra }" and
                // abandons it (tex.web § 395).  Reading on would swallow the
                // rest of the file looking for a delimiter that is not there.
                Tok::Chr(_, Catcode::End) if depth == 0 => {
                    self.unread(t);
                    self.diagnose(
                        Severity::Unsupported,
                        "unbalanced-argument",
                        t.span,
                        "a delimited argument ran into the end of its group".into(),
                    );
                    return Vec::new();
                }
                Tok::Chr(_, Catcode::End) => depth -= 1,
                _ => {}
            }
            out.push(t);
            if depth == 0 && out.len() >= delimiter.len() {
                let tail = &out[out.len() - delimiter.len()..];
                if tail.iter().zip(delimiter).all(|(t, d)| t.tok == *d) {
                    out.truncate(out.len() - delimiter.len());
                    return strip_braces(out);
                }
            }
        }
        self.probe_wants(&out, delimiter, crate::probe::Want::Until);
        self.unread_all(&out);
        Vec::new()
    }

    /// `\def\a#1#{…}`: the final argument stops at the begin-group character,
    /// which the replacement text puts back (tex.web § 476).
    fn read_before_group(&mut self) -> Vec<Token> {
        let mut out: Vec<Token> = Vec::new();
        let mut expanded = 0;
        while self.reading_on(expanded) {
            let Some(t) = self.next_token() else { break };
            expanded += self.expansion_delta();
            if self.runaway_argument(t) {
                return Vec::new();
            }
            match t.tok {
                Tok::Chr(_, Catcode::Begin) => {
                    // An empty argument leaves no trace in what a probe
                    // reads: it is told that text up to a `{` goes here.
                    if out.is_empty() {
                        self.probe_wants(&[t], &[t.tok], crate::probe::Want::Until);
                    }
                    return strip_braces(out);
                }
                Tok::Chr(_, Catcode::End) => {
                    self.unread(t);
                    self.diagnose(
                        Severity::Unsupported,
                        "unbalanced-argument",
                        t.span,
                        "a delimited argument ran into the end of its group".into(),
                    );
                    return Vec::new();
                }
                _ => out.push(t),
            }
        }
        let open = [Tok::Chr('{', Catcode::Begin)];
        self.probe_wants(&out, &open, crate::probe::Want::Until);
        self.unread_all(&out);
        Vec::new()
    }

    fn match_parameter_text(&mut self, text: &ParameterText) -> Vec<Vec<Token>> {
        let mut args: Vec<Vec<Token>> = vec![Vec::new(); text.arity as usize];
        let mut i = 0;
        while i < text.items.len() {
            match &text.items[i] {
                ParamItem::Lit(expected) => {
                    let t = self.next_token();
                    // Unknown text may begin with the delimiter: it is
                    // matched, and the rest of that text is read next.
                    if let Some(t) = t
                        && t.tok == Tok::Cs(self.unknown)
                    {
                        while let Some(ParamItem::Lit(_)) = text.items.get(i) {
                            i += 1;
                        }
                        let rest = self.unknown_rest_token(t.span);
                        self.unread(rest);
                        continue;
                    }
                    if let Some(t) = t
                        && t.tok != *expected {
                            // tex.web § 398: "Use of \x doesn't match its
                            // definition"; a probe learns what was wanted.
                            let wanted: Vec<Tok> = text.items[i..]
                                .iter()
                                .map_while(|item| match item {
                                    ParamItem::Lit(tok) => Some(*tok),
                                    ParamItem::Param(_) => None,
                                })
                                .collect();
                            self.probe_wants(&[t], &wanted, crate::probe::Want::Literal);
                            self.unread(t);
                            return args;
                        }
                    i += 1;
                }
                ParamItem::Param(n) => {
                    let mut delimiter = Vec::new();
                    let mut j = i + 1;
                    while let Some(ParamItem::Lit(tok)) = text.items.get(j) {
                        delimiter.push(*tok);
                        j += 1;
                    }
                    let last = j == text.items.len();
                    let arg = if !delimiter.is_empty() {
                        self.read_delimited(&delimiter)
                    } else if last && text.brace_end {
                        self.read_before_group()
                    } else {
                        self.read_undelimited()
                    };
                    if let Some(slot) = args.get_mut(*n as usize - 1) {
                        *slot = arg;
                    }
                    i = j;
                }
            }
        }
        // `\def\a#{…}`, `\def\a x#{…}`: the `{` ending the parameter text is
        // a delimiter matched like any other, and the replacement text puts
        // it back (tex.web § 476, § 392).
        if text.brace_end && !matches!(text.items.last(), Some(ParamItem::Param(_)))
            && let Some(t) = self.next_token()
            && !t.is_cat(Catcode::Begin)
        {
            self.probe_wants(&[t], &[], crate::probe::Want::Group);
            self.unread(t);
        }
        args
    }

    /// A scanner read `token` looking for `what`: a keyword, or a quantity
    /// (`number`, `dimen`, `glue`, …), which a probe learns a primitive's
    /// grammar from (The TeXbook, chapter 24).
    pub(crate) fn probe_scans(&mut self, token: Token, what: &str) {
        if let Some(watch) = &mut self.probe
            && token.span.file == watch.file
        {
            let entry = (token.span.col.saturating_sub(1) as usize, what.to_string());
            if !watch.scanned.contains(&entry) {
                watch.scanned.push(entry);
            }
        }
    }

    /// A probe run offered `found` where a parameter text wanted `wanted`
    /// (tex.web §§ 392, 398): the first such place is what the prober
    /// learns the command's input from.  Only the probe's own characters
    /// count; the text the macros hand each other is not the caller's.
    fn probe_wants(&mut self, found: &[Token], wanted: &[Tok], want: crate::probe::Want) {
        let Some(watch) = &self.probe else { return };
        let file = watch.file;
        if watch.demand.is_some() {
            return;
        }
        let Some(at) = found.iter().find(|t| t.span.file == file).map(|t| t.span.col.saturating_sub(1) as usize) else {
            return;
        };
        let toks: Vec<Token> = wanted.iter().map(|t| Token::new(*t, Span::default())).collect();
        let text = crate::tex::detokenize(&toks, &self.out.interner);
        if let Some(watch) = &mut self.probe {
            watch.demand = Some(crate::probe::Demand { at, text, want });
        }
    }

    pub fn read_optional(&mut self, open: char, close: char) -> Option<Vec<Token>> {
        self.skip_spaces();
        let t = self.next_token()?;
        // Noted whether or not it is there: a call that looks for `[` accepts
        // one at this place, which is what the shape records.
        self.note_shape(t.span, &format!("{open}{close}"));
        if !t.is_char(open) {
            self.unread(t);
            return None;
        }
        let mut out = Vec::new();
        let mut depth = 0usize;
        let mut expanded = 0;
        while self.reading_on(expanded) {
            let Some(t) = self.next_token() else { break };
            expanded += self.expansion_delta();
            match t.tok {
                Tok::Chr(_, Catcode::Begin) => depth += 1,
                Tok::Chr(_, Catcode::End) => depth = depth.saturating_sub(1),
                Tok::Chr(c, _) if c == close && depth == 0 => return Some(out),
                _ => {}
            }
            out.push(t);
        }
        self.unread_all(&out);
        None
    }

    pub fn read_star(&mut self) -> bool {
        self.skip_spaces();
        match self.next_token() {
            Some(t) if t.is_char('*') => {
                self.note_shape(t.span, "*");
                true
            }
            Some(t) => {
                self.note_shape(t.span, "*");
                self.unread(t);
                false
            }
            None => false,
        }
    }

    /// What ltcmd hands the body for each argument: `\BooleanTrue` or
    /// `\BooleanFalse` for `s` and `t` (usrguide, "Boolean arguments"), and
    /// the marker `-NoValue-` for an optional argument that was left out
    /// and has no default (`\c_novalue_tl`, whose first `-` is a letter).
    fn read_signature(&mut self, spec: &ArgSpec) -> Vec<Vec<Token>> {
        use crate::tex::ArgType;
        let boolean = |m: &mut Self, seen: bool| {
            let sym = m.intern(if seen { "BooleanTrue" } else { "BooleanFalse" });
            vec![Token::new(Tok::Cs(sym), Span::default())]
        };
        let no_value = |m: &mut Self, default: Option<&str>| match default {
            Some(text) => m.tokenize(text),
            None => "-NoValue-"
                .chars()
                .enumerate()
                .map(|(i, c)| {
                    let cat = if i == 0 || c != '-' { Catcode::Letter } else { Catcode::Other };
                    Token::new(Tok::Chr(c, cat), Span::default())
                })
                .collect(),
        };
        let mut out = Vec::with_capacity(spec.items.len());
        for item in &spec.items {
            let arg = match item {
                ArgType::Mandatory => self.read_undelimited(),
                // Only a `\def`'s own parameter text has these, and a macro
                // with a parameter text is matched by it, not by this reader.
                ArgType::Until(_) | ArgType::Literal(_) | ArgType::Keyword { .. } | ArgType::Quantity(_) => Vec::new(),
                ArgType::Star => {
                    let seen = self.read_star();
                    boolean(self, seen)
                }
                ArgType::TokenFlag(c) => {
                    self.skip_spaces();
                    let seen = match self.next_token() {
                        Some(t) if t.is_char(*c) => true,
                        Some(t) => {
                            self.unread(t);
                            false
                        }
                        None => false,
                    };
                    boolean(self, seen)
                }
                ArgType::Optional(default) => match self.read_optional('[', ']') {
                    Some(given) => given,
                    None => no_value(self, default.as_deref()),
                },
                ArgType::Delimited { open, close, default, .. } => match self.read_optional(*open, *close) {
                    Some(given) => given,
                    None => no_value(self, default.as_deref()),
                },
                ArgType::Embellishment(_) => Vec::new(),
            };
            out.push(arg);
        }
        out
    }

    /// Tokenize a string under the current catcodes, attributing every token
    /// to `at` so that synthesized material keeps a real source position.
    pub fn tokenize_at(&mut self, text: &str, at: Span) -> Vec<Token> {
        let mut tokens = self.tokenize(text);
        for token in &mut tokens {
            token.span = at;
        }
        tokens
    }

    /// One line as the engine reads it from a file: the current
    /// `\endlinechar` is appended, and the line starts in state N.
    pub fn tokenize_line_at(&mut self, text: &str, at: Span) -> Vec<Token> {
        let text = format!("{text}\n");
        let mut mouth = Mouth::new(&text, 0);
        let mut out = Vec::new();
        let Self { catcodes, out: analysis, end_line, .. } = self;
        while let Some(mut t) = mouth.next_with(catcodes, &mut analysis.interner, *end_line) {
            t.span = at;
            out.push(t);
        }
        out
    }

    /// Tokenize a string under the current catcodes.
    pub fn tokenize(&mut self, text: &str) -> Vec<Token> {
        let mut mouth = Mouth::new(text, 0);
        let mut out = Vec::new();
        let Self { catcodes, out: analysis, .. } = self;
        while let Some(t) = mouth.next(catcodes, &mut analysis.interner) {
            out.push(t);
        }
        out
    }

    /// The ⟨control sequence⟩ a TeX assignment defines.  `get_r_token`
    /// (tex.web § 1215) skips spaces and takes one token, which has to be a
    /// control sequence; unlike a LaTeX macro argument it never looks inside
    /// a group, so `\edef~{…}` on an active character defines nothing here
    /// rather than the first name the group happens to contain.
    pub fn read_r_token(&mut self) -> Option<Sym> {
        self.skip_spaces();
        let t = self.next_token()?;
        match t.tok {
            Tok::Cs(sym) => Some(sym),
            Tok::Chr(c, Catcode::Active) => Some(self.active_sym(c)),
            _ => {
                self.probe_scans(t, "cs");
                self.unread(t);
                None
            }
        }
    }

    /// A control sequence argument: `\foo`, or `{\foo}` as LaTeX writes it.
    pub fn read_cs(&mut self) -> Option<Sym> {
        self.skip_spaces();
        let t = self.next_token()?;
        // A control sequence argument: `\newcommand\foo` and
        // `\newcommand{\foo}` are the same argument, written two ways.
        self.note_shape(t.span, "{}");
        match t.tok {
            Tok::Cs(sym) => Some(sym),
            Tok::Chr(c, Catcode::Active) => Some(self.active_sym(c)),
            Tok::Chr(_, Catcode::Begin) => {
                let body = self.read_group_body();
                body.iter().find_map(Token::cs)
            }
            _ => {
                self.unread(t);
                None
            }
        }
    }

    /// An argument read for its text value: names, options, file names.
    pub fn read_text(&mut self) -> String {
        let raw = self.read_undelimited();
        let expanded = self.expand_for_value(&raw);
        text_of(&expanded, &self.out.interner)
    }

    pub fn text_of(&self, toks: &[Token]) -> String {
        text_of(toks, &self.out.interner)
    }

    /// An argument read for its structure rather than its text value.
    pub fn read_structured(&mut self) -> String {
        let raw = self.read_undelimited();
        crate::tex::text_with_groups(&raw, &self.out.interner)
    }

    /// Expand parameterless macros so that `\usepackage{\mypackages}` and
    /// `\csname\@currname\endcsname` yield their text, and so that `\edef`
    /// stores what it would really store.
    ///
    /// Iterative, with one budget shared by the whole expansion: nesting
    /// multiplies, so a per-level cap would not bound anything.
    pub fn expand_for_value(&self, toks: &[Token]) -> Vec<Token> {
        /// Deep enough for the `\@currname`-style indirections packages use.
        const MAX_VALUE_DEPTH: u16 = 16;
        let budget = self.cfg.limits.expansion_tokens;
        let mut out = Vec::with_capacity(toks.len());
        let mut stack: Vec<(Rc<[Token]>, usize, u16)> = vec![(Rc::from(toks), 0, 0)];
        while let Some((source, position, depth)) = stack.last_mut() {
            let Some(token) = source.get(*position).copied() else {
                stack.pop();
                continue;
            };
            *position += 1;
            if out.len() >= budget {
                break;
            }
            match token.tok {
                Tok::Cs(sym) if *depth < MAX_VALUE_DEPTH => match self.env.meaning(sym) {
                    Meaning::Macro(m) if m.arity() == 0 && !m.replacement_text.is_empty() => {
                        let next = *depth + 1;
                        stack.push((m.replacement_text.clone(), 0, next));
                    }
                    _ => out.push(token),
                },
                _ => out.push(token),
            }
        }
        out
    }

    /// Whether the token being executed was read from a file rather than
    /// produced by an expansion.
    pub fn at_top_level(&self) -> bool {
        self.within.is_empty()
    }

    /// Make `node` depend on every name the expansion of `toks` meets, the
    /// ones [`Machine::expand_for_value`] expands and the ones it leaves in
    /// place: `\message{\thefoo}` reads `\thefoo` and the counter under it.
    pub fn reads_value(&mut self, node: NodeId, toks: &[Token]) {
        const MAX_VALUE_DEPTH: u16 = 16;
        let mut seen: Vec<Sym> = Vec::new();
        let mut stack: Vec<(Rc<[Token]>, u16)> = vec![(Rc::from(toks), 0)];
        while let Some((source, depth)) = stack.pop() {
            for (at, token) in source.iter().enumerate() {
                let sym = match token.tok {
                    Tok::Cs(sym) => sym,
                    Tok::Chr(c, Catcode::Active) => self.active_sym(c),
                    _ => continue,
                };
                // A name formed from characters: `\csname c@foo\endcsname`.
                let formed = match self.env.meaning(sym).prim() {
                    Some(Primitive::Csname) => {
                        let rest = &source[at + 1..];
                        let end = rest.iter().position(|t| {
                            matches!(t.tok, Tok::Cs(s) if self.env.meaning(s).prim() == Some(Primitive::Endcsname))
                        });
                        end.map(|end| self.text_of(&rest[..end]))
                    }
                    _ => None,
                };
                if let Some(name) = formed.filter(|n| !n.is_empty()) {
                    let formed = self.intern(&name);
                    if !seen.contains(&formed) {
                        seen.push(formed);
                    }
                }
                if seen.contains(&sym) || seen.len() >= self.cfg.limits.expansion_tokens {
                    continue;
                }
                seen.push(sym);
                if let Meaning::Macro(m) = self.env.meaning(sym)
                    && depth < MAX_VALUE_DEPTH
                {
                    stack.push((m.replacement_text.clone(), depth + 1));
                }
            }
        }
        for sym in seen {
            self.reads_definition_of(node, sym);
            // `\the\mycount` reads the register `\mycount` names.
            let storage = self.storage(sym);
            if storage != sym {
                self.reads_definition_of(node, storage);
            }
        }
    }

    /// tex.web § 405: blanks and an `=` read with expansion.
    pub fn read_equals(&mut self) {
        self.scan_optional_equals();
    }

    /// Split the material up to the matching `\fi` into its alternatives.
    ///
    /// Returns `None` when no matching `\fi` turns up within
    /// `Config::max_conditional_tokens`; the tokens are then put back and the
    /// conditional is left alone, which is what happens in practice when a
    /// package hides its `\fi` behind `\expandafter`.
    /// Returns the alternatives and whether an `\else` closed the last one.
    /// Without one the conditional has a further outcome — that none of the
    /// arms runs — which the merge has to account for.
    /// Skip the text of an arm that is not taken, as `pass_text` does
    /// (tex.web § 494): nested conditionals are counted, nothing is executed.
    /// Stops at `\fi`, and at `\else`/`\or` unless `to_fi`.  Returns the
    /// primitive that ended the skip.
    pub fn pass_text(&mut self, to_fi: bool, at: Span) -> Option<(Primitive, Span)> {
        let scanner = std::mem::replace(&mut self.scanner, Scanner::Skipping);
        let found = self.pass_text_in(to_fi, at);
        self.scanner = scanner;
        found
    }

    /// [`Self::pass_text`] for the conditional at `at` itself, noting where
    /// the skip stopped.
    pub(crate) fn pass_arm(&mut self, to_fi: bool, at: Span) -> Option<Primitive> {
        let (kind, mark) = self.pass_text(to_fi, at)?;
        self.note_cond_mark(at, mark, kind == Primitive::Fi);
        Some(kind)
    }

    fn pass_text_in(&mut self, to_fi: bool, at: Span) -> Option<(Primitive, Span)> {
        let mut nesting = 0usize;
        let mut read = 0usize;
        while read < self.cfg.limits.conditional_tokens {
            let Some(t) = self.next_token() else { break };
            read += 1;
            // Skipping for a conditional begun in another file has run out of
            // that file's text into this one's: a package conditional whose
            // test or `\fi` satex got wrong would otherwise swallow the
            // document.  TeX inserts `\fi` where a file ends inside a skip
            // (tex.web § 336); satex ends the skip here as though it had, and
            // leaves a marker that splits off the path skipping on as TeX does.
            if self.last_file.is_some_and(|f| f != at.file) {
                self.unread(t);
                let name = self.out.short_name(at.file).to_string();
                self.diagnose(
                    Severity::Imprecision,
                    "conditional-crosses-file",
                    t.span,
                    format!("skipping a conditional begun at {name}:{} reached another file's text; both its end there and the skip on are analyzed", at.line),
                );
                let nested = u8::try_from(nesting).unwrap_or(u8::MAX);
                let marker = self.intern(&format!("\u{2}skip crossing:{nested}:{to_fi}"));
                let meaning = Meaning::Primitive(Primitive::SkipCrossing { nested, to_fi });
                self.env.set(marker, crate::env::Binding::builtin(meaning), true);
                self.unread(Token::new(Tok::Cs(marker), t.span));
                return Some((Primitive::Fi, t.span));
            }
            let Tok::Cs(sym) = t.tok else { continue };
            match self.env.skip_meaning(sym).prim() {
                Some(Primitive::If(_)) => nesting += 1,
                Some(Primitive::Fi) => {
                    if nesting == 0 {
                        return Some((Primitive::Fi, t.span));
                    }
                    nesting -= 1;
                }
                Some(kind @ (Primitive::Else | Primitive::Or)) if nesting == 0 && !to_fi => {
                    return Some((kind, t.span));
                }
                _ => {}
            }
        }
        self.diagnose(
            Severity::Unsupported,
            "unterminated-conditional",
            at,
            "no matching \\fi within the conditional budget".into(),
        );
        None
    }

    pub fn read_alternatives(&mut self, at: Span) -> Option<(Vec<Rc<[Token]>>, bool)> {
        let mut body: Vec<Token> = Vec::new();
        let mut cuts: Vec<usize> = Vec::new();
        let mut nesting = 0usize;
        let mut closed = false;
        let mut has_else = false;
        while body.len() < self.cfg.limits.conditional_tokens {
            let Some(t) = self.next_token() else { break };
            if let Tok::Cs(sym) = t.tok {
                match self.env.skip_meaning(sym).prim() {
                    Some(Primitive::If(_)) => nesting += 1,
                    Some(Primitive::Fi) => {
                        if nesting == 0 {
                            closed = true;
                            break;
                        }
                        nesting -= 1;
                    }
                    Some(kind @ (Primitive::Else | Primitive::Or)) if nesting == 0 => {
                        has_else = kind == Primitive::Else;
                        cuts.push(body.len());
                    }
                    _ => {}
                }
            }
            body.push(t);
        }
        if !closed {
            self.unread_all(&body);
            self.diagnose(
                Severity::Unsupported,
                "unterminated-conditional",
                at,
                "no matching \\fi within the conditional budget".into(),
            );
            return None;
        }
        let mut alternatives = Vec::with_capacity(cuts.len() + 1);
        let mut start = 0;
        for cut in cuts.into_iter().chain(std::iter::once(body.len())) {
            alternatives.push(Rc::from(&body[start..cut]));
            // Step over the `\else` or `\or` that ended this alternative.
            start = (cut + 1).min(body.len());
        }
        Some((alternatives, has_else))
    }

    fn save_path(&self) -> PathState {
        PathState {
            input: self.input.clone(),
            catcodes: self.catcodes.clone(),
            end_line: self.end_line,
            prefixes: self.prefixes,
            after_assignment: self.after_assignment,
            long_argument: self.long_argument,
            runaway: self.runaway,
            cds: self.cds.clone(),
            conds: self.conds.clone(),
            within: self.within.clone(),
            packages: self.packages.clone(),
            held_tokens: self.held_tokens,
            section: self.section.clone(),
            call_shape: self.call_shape.clone(),
            edef_depth: self.edef_depth,
            pdf_string: self.pdf_string,
            last_file: self.last_file,
            file_call: self.file_call,
            source_progress: self.source_progress,
            file_depth: self.file_depth,
            halted: self.halted,
            streams: self.streams.clone(),
            loaded: self.loaded.clone(),
            mode: self.mode,
            list: self.list,
            material: self.material,
            natural: self.natural,
            wild: self.wild.clone(),
        }
    }

    fn restore_path(&mut self, state: PathState) {
        // How often a name is being expanded is the number of its levels on
        // the input stack, so it follows the input instead of being copied.
        for (frames, delta) in [(std::mem::take(&mut self.input), -1), (state.input.clone(), 1)] {
            for frame in &frames {
                match frame {
                    Frame::Tokens { expanding: Some(sym), .. } => self.bump_expanding(*sym, delta),
                    Frame::Pseudo { .. } => self.bump_expanding(self.pseudo_file, delta),
                    _ => {}
                }
            }
        }
        self.input = state.input;
        self.catcodes = state.catcodes;
        self.end_line = state.end_line;
        self.prefixes = state.prefixes;
        self.after_assignment = state.after_assignment;
        self.long_argument = state.long_argument;
        self.runaway = state.runaway;
        self.cds = state.cds;
        self.conds = state.conds;
        self.within = state.within;
        self.packages = state.packages;
        self.quantity = crate::observe::Quantities::default();
        self.held_tokens = state.held_tokens;
        self.section = state.section;
        self.call_shape = state.call_shape;
        self.edef_depth = state.edef_depth;
        self.pdf_string = state.pdf_string;
        self.last_file = state.last_file;
        self.file_call = state.file_call;
        self.source_progress = state.source_progress;
        self.file_depth = state.file_depth;
        self.halted = state.halted;
        self.streams = state.streams;
        self.loaded = state.loaded;
        self.mode = state.mode;
        self.list = state.list;
        self.material = state.material;
        self.natural = state.natural;
        self.wild = state.wild;
    }

    /// How many levels of [`Machine::places`] there are, without building them.
    fn live_depth(&self) -> usize {
        self.input
            .iter()
            .filter(|frame| match frame {
                Frame::Tokens { toks, pos, .. } => *pos < toks.len(),
                Frame::Returned(tokens) => !tokens.is_empty(),
                _ => true,
            })
            .count()
    }

    /// The input stack as places, bottom first, without exhausted levels.
    fn places(&self) -> Vec<Place> {
        self.input
            .iter()
            .filter_map(|frame| match frame {
                Frame::File { mouth, .. } => Some(Place::File(mouth.file, mouth.offset(), mouth.ending())),
                Frame::Tokens { toks, pos, .. } => (*pos < toks.len())
                    .then(|| Place::Tokens(toks.as_ptr() as usize, *pos)),
                Frame::Pseudo { mouth, pos, .. } => Some(Place::Tokens(mouth.source_id(), mouth.offset() + pos)),
                Frame::Returned(tokens) => (!tokens.is_empty()).then(|| Place::Returned(tokens.clone())),
                Frame::Boundary => Some(Place::Boundary),
            })
            .collect()
    }

    /// Run the current path until it is back at or below `depth` levels of
    /// input with the conditionals down to `conds`, which is where it can
    /// meet another path; `false` when it ends first.
    fn run_to_meeting(&mut self, depth: usize, conds: usize, budget: &mut u64, moved: bool, target: u64) -> bool {
        let start = self.out.steps;
        let mut moved = moved;
        loop {
            if !moved && self.source_progress >= target && self.conds.len() <= conds && self.live_depth() <= depth {
                break;
            }
            moved = false;
            if self.halted || *budget == 0 || !self.command() {
                *budget = budget.saturating_sub(self.out.steps - start);
                return false;
            }
            if self.out.steps - start >= *budget {
                *budget = 0;
            }
        }
        *budget = budget.saturating_sub(self.out.steps - start);
        true
    }

    /// An undecided conditional read by the main loop: every arm runs as a
    /// path of its own from the state the test left, until the paths are
    /// back at the same place in the input; there the bindings are joined —
    /// a name the arms left with different meanings has an unknown one from
    /// then on, so tests on it are undecided in turn.  A path that reaches
    /// `\end` drops out.  `enter` puts the path at the start of arm `i` and
    /// says whether there is such an arm.  Returns `false`, having done
    /// nothing, when the nesting of such conditionals is at its limit.
    pub fn run_paths(&mut self, by: Sym, span: Span, enter: impl Fn(&mut Self, usize) -> bool) -> bool {
        self.split_paths(by, span, false, enter)
    }

    /// [`Machine::run_paths`]; with `moved`, each path reads at least one
    /// command before it can meet the others, for paths that begin with
    /// the same token given back.
    fn split_paths(&mut self, by: Sym, span: Span, moved: bool, enter: impl Fn(&mut Self, usize) -> bool) -> bool {
        // The same split again on a path one of its own arms began, with
        // nothing read from the source since: a loop whose exit is
        // undecided came round (§ 14).
        let progress = self.source_progress;
        let matching: Vec<usize> = (0..self.heads.len()).filter(|&h| self.heads[h].span == span && self.heads[h].progress == progress).collect();
        if let Some(&h) = matching.iter().rev().find(|&&h| self.same_shape(h)) {
            self.revisit(h);
            // Its state is joined into the head's, which the head analyzes
            // on; the path adds nothing of its own.
            self.halted = true;
            return true;
        }
        // Met in another shape (a recursion that is not a tail call): this
        // path takes the other arm, so it leaves after one more pass.  A
        // split on a meaning, a mode or unknown digits reads on in its input
        // (pgfmath's parser taking a number apart): a split of its own,
        // bounded by `branch_depth`.
        if !moved && !matching.is_empty() {
            let taken = self.split_at.iter().rev().find(|(at, _, read)| *at == span && *read == progress).map_or(0, |e| e.1);
            self.diagnose(
                Severity::Imprecision,
                "undecided-loop",
                span,
                format!("\\{} decides a recursion and could not be decided; one more pass is analyzed", self.name(by)),
            );
            return enter(self, usize::from(taken == 0));
        }
        if self.branch_depth >= self.cfg.limits.branch_depth || self.cfg.limits.join_tokens == 0 {
            return false;
        }
        self.branch_depth += 1;
        self.env.sets = crate::env::SetLimits {
            meanings: usize::from(self.cfg.limits.meaning_set),
            values: usize::from(self.cfg.limits.value_set),
        };
        let base = self.save_path();
        let fork = self.env.fork();
        self.heads.push(LoopHead {
            span,
            progress,
            trail: self.env.trail_mark(),
            places: self.places(),
            conds: self.conds.len(),
            groups: self.env.depth(),
            catcodes: self.catcodes.clone(),
            joined: HashMap::new(),
            modes: self.mode,
            grown: false,
            rounds: 0,
        });
        let depth = self.live_depth();
        let conds = self.conds.len();
        let widening = u32::from(self.cfg.limits.widen_after);
        let live = loop {
            let mut budget = self.cfg.limits.join_tokens;
            let mut paths: Vec<Arm> = Vec::new();
            for arm in 0.. {
                let round = self.heads.last().map_or(0, |h| h.rounds);
                if arm > 0 || round > 0 {
                    self.restore_path(base.clone());
                    self.env.rewind(&fork);
                    // A later round starts from the join of every state
                    // the loop reached at the head.
                    let head = self.heads.last().expect("pushed above");
                    let joined: Vec<_> = head.joined.iter().map(|(&sym, (_, j))| (sym, j.clone())).collect();
                    self.mode = head.modes;
                    self.work += joined.len() as u64;
                    for (sym, slot) in joined {
                        self.env.assume(sym, slot);
                    }
                }
                if !enter(self, arm) {
                    break;
                }
                // Each arm copies the input stack and the save stack's top.
                self.work += 1 + self.input.len() as u64;
                self.split_at.push((span, arm, progress));
                let ended = !self.run_to_meeting(depth, conds, &mut budget, moved, 0);
                self.split_at.pop();
                paths.push(Arm { state: self.save_path(), env: self.env.path(&fork), ended, arm, progress });
            }
            let live = self.converge(paths, &fork, depth, conds, &mut budget, by, span);
            // A path met the head again with a state it had not seen: the
            // paths from the head are analyzed again from the joined
            // state, which covers every earlier round's, until nothing
            // grows it (the fixpoint).  After `widen_after` rounds what
            // still grows is widened to unknown, which ends it.
            let head = self.heads.last_mut().expect("pushed above");
            if !head.grown {
                break live;
            }
            head.grown = false;
            head.rounds += 1;
            if head.rounds > widening + 1 {
                self.diagnose(
                    Severity::Imprecision,
                    "undecided-loop",
                    span,
                    format!(
                        "\\{} decides a loop and could not be decided; its state still grew after {} widened passes",
                        self.name(by),
                        widening + 1
                    ),
                );
                break live;
            }
        };
        let head = self.heads.pop().expect("pushed above");
        if head.rounds > 0 && head.rounds <= widening + 1 {
            self.diagnose(
                Severity::Imprecision,
                "undecided-loop",
                span,
                format!(
                    "\\{} decides a loop and could not be decided; every number of passes is joined{}",
                    self.name(by),
                    if head.rounds > widening { ", widened" } else { "" }
                ),
            );
        }
        match live {
            Some(path) => {
                self.restore_path(path.state);
                self.env.resume(&fork, &path.env);
            }
            None => {
                self.restore_path(base);
                self.env.rewind(&fork);
                self.halted = true;
            }
        }
        self.env.unfork(fork);
        self.branch_depth -= 1;
        true
    }

    /// A path meets loop head `h` again in the same shape: its state is
    /// joined into the head's — widened after `widen_after` rounds, so that
    /// a slot that still grows holds anything from then on — and the head
    /// is told to run again when that grew it.
    fn revisit(&mut self, h: usize) {
        let head = &self.heads[h];
        let widen = head.rounds >= u32::from(self.cfg.limits.widen_after);
        let sets = self.env.sets;
        let changed = self.env.changed_since(head.trail);
        let head = &mut self.heads[h];
        let mut grown = false;
        for (sym, base, now) in changed {
            let entry = head.joined.entry(sym).or_insert_with(|| (base.clone(), base));
            let joined = crate::env::join_slot(entry.1.clone(), now, sets);
            let next = if widen { crate::env::widen_slot(&entry.1, joined, sets) } else { joined };
            if !crate::env::same_slot(&entry.1, &next) {
                grown = true;
                entry.1 = next;
            }
        }
        let modes = head.modes.union(self.mode);
        grown |= modes != head.modes;
        head.modes = modes;
        head.grown |= grown;
        self.work += 1;
    }

    /// Whether the run stands where loop head `h` split, in the same
    /// input and with the same conditional and group stacks and category
    /// codes: a pass of the loop, not a recursion or a read further on.
    fn same_shape(&self, h: usize) -> bool {
        let head = &self.heads[h];
        self.conds.len() == head.conds
            && self.env.depth() == head.groups
            && self.catcodes == head.catcodes
            && self.places() == head.places
    }

    /// Bring the paths together: while two stand at different places, the
    /// one that has read less of the source runs on to its next meeting
    /// point.  Paths at the same place are joined.  When the budget runs out
    /// the paths that are left are joined where they stand, which keeps the
    /// first path's input, and the run says so.
    #[allow(clippy::too_many_arguments)]
    fn converge(
        &mut self,
        mut paths: Vec<Arm>,
        fork: &crate::env::Fork,
        depth: usize,
        conds: usize,
        budget: &mut u64,
        by: Sym,
        span: Span,
    ) -> Option<Arm> {
        /// Rounds in which no path reads further in the source before the
        /// paths count as diverged: stepping command by command at one place
        /// in the file only repeats work.
        const STILL_ROUNDS: u32 = 64;
        let mut still = 0;
        let mut furthest = 0;
        loop {
            // A path that reached `\end`, or met a loop head with a state
            // the head holds already, adds nothing to the join.
            paths.retain(|path| !path.state.halted);
            let reached = paths.iter().map(|p| p.state.source_progress).max().unwrap_or(0);
            if reached > furthest {
                furthest = reached;
                still = 0;
            } else {
                still += 1;
                if still > STILL_ROUNDS {
                    *budget = 0;
                }
            }
            if paths.len() <= 1 {
                return paths.pop();
            }
            let places: Vec<Vec<Place>> = paths
                .iter_mut()
                .map(|path| {
                    std::mem::swap(&mut self.input, &mut path.state.input);
                    let places = self.places();
                    std::mem::swap(&mut self.input, &mut path.state.input);
                    places
                })
                .collect();
            // Every round compares every path's place and state.
            self.work += places.iter().map(|p| 1 + p.len() as u64).sum::<u64>();
            let met = places.iter().all(|p| *p == places[0]);
            // State a join cannot combine — category codes, a waiting
            // `\afterassignment` or `\aftergroup` token — keeps the paths
            // apart until it agrees again.
            let joinable = met && paths.iter().all(|p| self.joinable(&paths[0], p));
            if joinable || *budget == 0 || paths.iter().all(|p| p.ended) {
                if *budget == 0 || !joinable {
                    self.diagnose(
                        Severity::Imprecision,
                        "paths-diverged",
                        span,
                        format!(
                            "the arms of \\{} did not meet again within {} tokens; the run goes on after the first",
                            self.name(by),
                            self.cfg.limits.join_tokens
                        ),
                    );
                }
                let envs: Vec<_> = paths.iter().map(|p| p.env.clone()).collect();
                let (mode, list) = paths
                    .iter()
                    .fold((crate::mode::Modes::NONE, crate::mode::Modes::NONE), |(m, l), p| {
                        (m.union(p.state.mode), l.union(p.state.list))
                    });
                let material = paths.iter().any(|p| p.state.material);
                let natural = paths.iter().map(|p| p.state.natural).reduce(crate::mode::join_natural).flatten();
                let mut wild = Wild::default();
                for p in paths.iter() {
                    wild.union(&p.state.wild);
                }
                let mut first = paths.swap_remove(0);
                first.state.wild = wild;
                first.state.material = material;
                first.state.natural = natural;
                first.state.mode = mode;
                first.state.list = list;
                if envs.iter().all(|e| crate::env::Env::same_groups(&envs[0], e)) {
                    self.env.join_paths(fork, &envs);
                    first.env = self.env.path(fork);
                }
                return Some(first);
            }
            // Paths that stand at the same place are joined there, whether
            // or not the others have come: from one program point on, one
            // path with their join covers them, so a dispatch that passes
            // its continuation along does not multiply the paths.
            if let Some((i, group)) = (0..paths.len()).find_map(|i| {
                let group: Vec<usize> = (i + 1..paths.len())
                    .filter(|&j| !paths[i].ended && !paths[j].ended && places[i] == places[j] && self.joinable(&paths[i], &paths[j]))
                    .collect();
                (!group.is_empty()).then_some((i, group))
            }) {
                let envs: Vec<_> =
                    std::iter::once(i).chain(group.iter().copied()).map(|k| paths[k].env.clone()).collect();
                if envs.iter().all(|e| crate::env::Env::same_groups(&envs[0], e)) {
                    self.work += envs.len() as u64;
                    let mut merged = std::mem::replace(&mut paths[i].state.wild, Wild::default());
                    let (mut mode, mut list, mut material) = (paths[i].state.mode, paths[i].state.list, paths[i].state.material);
                    let mut natural = paths[i].state.natural;
                    for &k in &group {
                        merged.union(&paths[k].state.wild);
                        mode = mode.union(paths[k].state.mode);
                        list = list.union(paths[k].state.list);
                        material |= paths[k].state.material;
                        natural = crate::mode::join_natural(natural, paths[k].state.natural);
                    }
                    self.env.join_paths(fork, &envs);
                    let path = &mut paths[i];
                    path.env = self.env.path(fork);
                    path.state.wild = merged;
                    path.state.mode = mode;
                    path.state.list = list;
                    path.state.material = material;
                    path.state.natural = natural;
                    for &k in group.iter().rev() {
                        paths.remove(k);
                    }
                    continue;
                }
            }
            // The path that is behind runs on; ties go to the deeper one.
            let behind = (0..paths.len())
                .filter(|&i| !paths[i].ended)
                .min_by_key(|&i| (paths[i].state.source_progress, std::cmp::Reverse(places[i].len())))
                .expect("not every path has ended");
            // It runs until it has read as far as the furthest path, so the
            // paths are compared where they can meet, not at every command.
            let target = paths.iter().map(|p| p.state.source_progress).max().unwrap_or(0) + u64::from(met);
            let path = &mut paths[behind];
            self.restore_path(path.state.clone());
            self.env.resume(fork, &path.env);
            self.split_at.push((span, path.arm, path.progress));
            let ended = !self.run_to_meeting(depth, conds, budget, true, target);
            self.split_at.pop();
            let (arm, progress) = (path.arm, path.progress);
            *path = Arm { state: self.save_path(), env: self.env.path(fork), ended, arm, progress };
        }
    }

    /// A name that holds one of several meanings (a join left it so), read
    /// by the main loop: every meaning is a path of its own, on which the
    /// name has that meaning and is read again.  The paths meet and are
    /// joined like the arms of an undecided conditional.  `false`, having
    /// done nothing, when the name has one meaning or no path may split.
    pub fn split_meaning(&mut self, sym: Sym, token: Token) -> bool {
        let Some(may) = self.env.slot(sym).and_then(|b| b.may.clone()) else { return false };
        if self.edef_depth > 0 || !self.may_split_meaning() {
            return false;
        }
        self.split_paths(sym, token.span, true, |m, arm| {
            let Some(meaning) = may.get(arm) else { return false };
            m.env.refine(sym, meaning.clone());
            m.unread(token);
            true
        })
    }

    /// Whether a name that holds one of several meanings may split the run
    /// here; past `meaning_splits` nested paths it is read as a name of
    /// unknown meaning instead.
    pub(crate) fn may_split_meaning(&self) -> bool {
        self.branch_depth < self.cfg.limits.meaning_splits
    }

    /// [`Machine::run_paths`] for paths that each begin with a token given
    /// back, which they read before they can meet.
    pub fn split_moving(&mut self, by: Sym, span: Span, enter: impl Fn(&mut Self, usize) -> bool) -> bool {
        self.split_paths(by, span, true, enter)
    }

    /// Whether two paths that stand at the same place can be joined.
    fn joinable(&self, a: &Arm, b: &Arm) -> bool {
        a.state.catcodes == b.state.catcodes
            && a.state.end_line == b.state.end_line
            && a.state.after_assignment.map(|t| t.tok) == b.state.after_assignment.map(|t| t.tok)
            && crate::env::Env::same_after(&a.env, &b.env)
    }

    /// Analyze mutually exclusive alternatives and merge their effects.
    pub fn analyze_alternatives(&mut self, branches: &[Rc<[Token]>], on: NodeId) {
        if self.branch_depth >= self.cfg.limits.branch_depth {
            return;
        }
        self.branch_depth += 1;
        // Category codes are rolled back with the bindings: a `\catcode` made
        // on a path that may not be taken must not change how the rest of the
        // file is tokenized.
        let mut captured: Vec<Captured> = Vec::with_capacity(branches.len());
        let entry = self.natural;
        let mut natural = None;
        for (i, branch) in branches.iter().enumerate() {
            self.natural = entry;
            let depth = self.env.depth();
            let mark = self.env.mark();
            self.cds.push(ControlDep { on, taken: i == 0 });
            self.run_tokens(branch.clone());
            self.cds.pop();
            let mut catcodes = std::mem::take(&mut self.catcodes);
            captured.push(self.env.capture_and_rollback(mark, depth, &mut catcodes));
            self.catcodes = catcodes;
            natural = if i == 0 { self.natural } else { crate::mode::join_natural(natural, self.natural) };
        }
        self.natural = natural;
        self.env.merge(captured);
        self.branch_depth -= 1;
    }

    /// Take one alternative when the condition is decidable, analyze them all
    /// when it is not.
    pub fn choose(&mut self, taken: Option<usize>, mut branches: Vec<Rc<[Token]>>, sym: Sym, span: Span) {
        match taken {
            Some(i) => {
                self.record_with(Step::Branch, sym, span, || Some(format!("branch {i}")));
                if let Some(branch) = branches.into_iter().nth(i) {
                    self.push_tokens(branch, None);
                }
            }
            None => {
                let node = self.force_reference(sym, span);
                self.record_with(Step::Condition, sym, span, || Some("undecided"));
                if branches.len() < 2 {
                    branches.push(Rc::from(Vec::new()));
                }
                self.diagnose(
                    Severity::Imprecision,
                    "undecided-condition",
                    span,
                    format!("\\{} could not be decided; all branches analyzed", self.name(sym)),
                );
                self.analyze_alternatives(&branches, node);
            }
        }
    }

    pub fn register_file(&mut self, path: String, kind: LoadKind) -> FileId {
        if let Some(i) = self.out.files.iter().position(|f| f.path == path) {
            return i as FileId;
        }
        if self.cfg.verbose >= 1 {
            eprintln!("satex: reading {path}");
        }
        self.out.files.push(FileInfo { path, kind });
        self.out.entry.push(None);
        (self.out.files.len() - 1) as FileId
    }

    /// `\input` has named a file: read it next.  latex.ltx reads packages
    /// and classes this way too, in `\@onefilewithoptions`, with `\@currname`
    /// and `\@currext` naming the file and `\opt@⟨name⟩.⟨ext⟩` holding its
    /// options (ltclass.dtx), which is what makes the file a package or a
    /// class here.
    pub fn load(&mut self, name: &str, kind: LoadKind, span: Span, options: Vec<String>, required: Option<String>) {
        let (name, kind, options) = self.kernel_file(name.trim(), kind, options);
        let name = name.as_str();
        // The load stands at the call from a file whose own text names the
        // file; a file the code of some other call opens stands at the
        // `\input` that opened it.
        let span = match self.file_call {
            Some((_, call)) if self.call_names_file(call, name) => call,
            _ => self.line_naming_file(name).unwrap_or(span),
        };
        let follow = match kind {
            LoadKind::Package | LoadKind::Inherited => self.cfg.load_packages,
            LoadKind::Class => self.cfg.load_classes,
            _ => self.cfg.load_inputs || self.reading_format(),
        };
        let by = self.package();
        // `\input docstrip` reads the program whose batch-file commands
        // satex models; its own code reads the `.dtx` and writes the files,
        // which an interpreter that does neither cannot follow.
        let modeled = self.docstrip && matches!(name, "docstrip" | "docstrip.tex");
        let mut status = if !follow || modeled {
            LoadStatus::NotFollowed
        } else if self.cfg.skip_packages.iter().any(|p| p == name) {
            LoadStatus::Skipped
        } else if self.file_depth >= self.cfg.limits.file_depth {
            LoadStatus::TooDeep
        } else {
            LoadStatus::NotFound
        };
        let resolved = if status == LoadStatus::NotFound {
            let base = self.base.clone();
            self.resolver.resolve(name, kind, &base)
        } else {
            None
        };
        // A file this run has opened for writing is there to read, with what
        // satex, which writes nothing, does not have.
        if resolved.is_none() && status == LoadStatus::NotFound && self.engine_int(&format!("written.{name}")) == Some(1) {
            status = LoadStatus::NotFollowed;
        }
        let source = resolved.as_ref().and_then(|path| match crate::overlay::read_to_string(path) {
            // A `.dtx` read as an input is read the way docstrip reads it.
            Ok(text) => Some(match crate::literate::Literate::of(path) {
                Some(crate::literate::Literate::Dtx) => {
                    crate::literate::code_view(&text, &self.resolver.guards(name, kind))
                }
                _ => text,
            }),
            Err(_) => {
                status = LoadStatus::Unreadable;
                None
            }
        });
        if source.is_some() {
            status = LoadStatus::Read;
        }
        if kind.is_package() {
            self.loaded.push(name.to_string());
            for option in &options {
                let key = option.split('=').next().unwrap_or(option).trim().to_string();
                self.occurrence(OccKind::PassedOption, key, Some(name.to_string()), span);
            }
        }
        let file = resolved.as_ref().map(|path| {
            let display = path.display().to_string();
            self.register_file(display, kind)
        });
        self.out.facts.loads.push(Load {
            name: name.to_string(),
            kind,
            options: options.clone(),
            span,
            by,
            file,
            path: resolved.as_ref().map(|p| p.display().to_string()),
            status,
            required,
            provided: None,
            depth: self.file_depth,
        });
        let load = self.out.facts.loads.len() - 1;
        let (Some(source), Some(id)) = (source, file) else { return };
        // Where this file's definitions become visible, expressed in the main
        // file: the load site, or whatever made the loading file visible.
        let entry = if span.file == self.out.main_file {
            Some(span)
        } else {
            self.out.entry.get(span.file as usize).copied().flatten()
        };
        if let Some(slot) = self.out.entry.get_mut(id as usize) {
            *slot = entry;
        }
        if kind == LoadKind::Class && self.class_options.is_empty() {
            self.class_options = options.clone();
        }
        let package = kind.is_package().then(|| self.intern(name));
        if let Some(sym) = package {
            self.packages.push((sym, options));
        }
        self.identifying = Some(load);
        self.file_depth += 1;
        self.out.timings.enter_file(id);
        let file = self.cfg.trace.then(|| self.out.file_name(id).to_string());
        self.record_with(Step::OpenFile, package.unwrap_or(Sym(0)), span, || file);
        let depth = self.env.depth();
        let conds = self.conds.len();
        if let Some(path) = resolved.as_ref() {
            self.note_opened(&path.display().to_string());
        }
        self.file_calls.push((id, self.file_call));
        let mouth = self.open_package(&source, id, name, kind, span);
        self.input.push(Frame::File {
            mouth,
            package,
            catcodes: None,
            depth,
            conds,
            eof_seen: false,
        });
    }

    /// What latex.ltx is reading through `\input`: the package or class
    /// `\@currname` and `\@currext` name, with the options
    /// `\opt@⟨name⟩.⟨ext⟩` holds, when the file is theirs; the file itself
    /// otherwise.  Which extensions are a package's and a class's is the
    /// kernel's own `\@pkgextension` and `\@clsextension`.
    fn kernel_file(&self, name: &str, kind: LoadKind, options: Vec<String>) -> (String, LoadKind, Vec<String>) {
        let text = |cs: &str| -> Option<String> {
            let sym = self.out.interner.lookup(cs)?;
            let binding = self.env.slot(sym)?;
            let m = binding.meaning.as_macro()?;
            Some(self.text_of(&m.replacement_text))
        };
        let file = name.trim_matches('"');
        let (Some(current), Some(ext)) = (text("@currname"), text("@currext")) else {
            return (file.to_string(), kind, options);
        };
        let leaf = std::path::Path::new(file).file_name().map_or(file.to_string(), |f| f.to_string_lossy().into_owned());
        if current.is_empty() || leaf != format!("{current}.{ext}") {
            return (file.to_string(), kind, options);
        }
        let kind = if text("@clsextension").as_deref() == Some(ext.as_str()) {
            LoadKind::Class
        } else if text("@pkgextension").as_deref() == Some(ext.as_str()) {
            LoadKind::Package
        } else {
            return (file.to_string(), kind, options);
        };
        let options = text(&format!("opt@{current}.{ext}"))
            .map(|list| crate::tex::comma_split(&list))
            .unwrap_or_default();
        (current, kind, options)
    }

    /// Whether the source text of the call at `call`, up to where its file
    /// is read now, names the file `name`.
    fn call_names_file(&self, call: Span, name: &str) -> bool {
        let base = std::path::Path::new(name.trim_matches('"'));
        let Some(stem) = base.file_stem().map(|s| s.to_string_lossy().into_owned()) else { return false };
        let text = self.input.iter().rev().find_map(|frame| match frame {
            Frame::File { mouth, .. } if mouth.file == call.file => Some(mouth.text_since(call.line, call.col)),
            _ => None,
        });
        text.is_some_and(|text| text.contains(stem.as_str()))
    }

    /// The line the innermost file is reading, when its text so far names
    /// the file `name`: a call whose arguments a lookahead took from the
    /// file before it was read as a call.
    fn line_naming_file(&self, name: &str) -> Option<Span> {
        let stem = std::path::Path::new(name.trim_matches('"')).file_stem()?.to_string_lossy().into_owned();
        let mouth = self.input.iter().rev().find_map(|frame| match frame {
            Frame::File { mouth, .. } => Some(mouth),
            _ => None,
        })?;
        let (line, _) = mouth.position();
        mouth.text_since(line, 1).contains(stem.as_str()).then_some(Span { file: mouth.file, line, col: 1 })
    }

    /// A load request for a file the kernel has already loaded: latex.ltx's
    /// `\@onefilewithoptions` asks `\@ifl@aded\@currext\@currname` and, for
    /// a file it has, compares the options with `\@onefilewithoptions@clashchk`
    /// instead of reading it.  The request is a load all the same.
    fn note_repeated_load(&mut self, sym: Sym, arguments: &[Vec<Token>]) {
        let is = |args: &[Token], name: &str| {
            matches!(args, [t] if t.cs().is_some_and(|s| self.out.interner.name(s) == name))
        };
        match self.name(sym) {
            "@ifl@aded" if arguments.len() == 2 && is(&arguments[0], "@currext") && is(&arguments[1], "@currname") => {
                let text = |m: &Self, cs: &str| {
                    let sym = m.out.interner.lookup(cs)?;
                    let macro_ = m.env.slot(sym)?.meaning.as_macro()?;
                    Some(m.text_of(&macro_.replacement_text))
                };
                let (Some(current), Some(ext)) = (text(self, "@currname"), text(self, "@currext")) else { return };
                let loaded = self
                    .out
                    .interner
                    .lookup(&format!("ver@{current}.{ext}"))
                    .is_some_and(|v| {
                        let meaning = self.env.meaning(v);
                        meaning != Meaning::Undefined && meaning.prim() != Some(crate::builtins::Primitive::Relax)
                    });
                if !loaded {
                    return;
                }
                let kind = if text(self, "@clsextension").as_deref() == Some(ext.as_str()) {
                    LoadKind::Class
                } else if text(self, "@pkgextension").as_deref() == Some(ext.as_str()) {
                    LoadKind::Package
                } else {
                    return;
                };
                // Only a request some file's text makes: one the code of a
                // hook or a package makes has no place in the document.
                let span = match self.file_call {
                    Some((_, call)) if self.call_names_file(call, &current) => Some(call),
                    _ => self.line_naming_file(&current),
                };
                let Some(span) = span else { return };
                let by = self.package();
                self.out.facts.loads.push(Load {
                    name: current,
                    kind,
                    options: Vec::new(),
                    span,
                    by,
                    file: None,
                    path: None,
                    status: LoadStatus::AlreadyLoaded,
                    required: None,
                    provided: None,
                    depth: self.file_depth,
                });
            }
            "@onefilewithoptions@clashchk" if arguments.len() == 1 => {
                let options = crate::tex::comma_split(&self.text_of(&arguments[0]));
                if let Some(load) = self.out.facts.loads.last_mut()
                    && load.status == LoadStatus::AlreadyLoaded
                {
                    load.options = options;
                }
            }
            _ => {}
        }
    }

    /// Attach a `\Provides…` banner to the load it identifies.
    pub fn record_identification(&mut self, info: &str) {
        let Some(load) = self.identifying.take() else { return };
        let date = info.split_whitespace().next().unwrap_or_default();
        let provided = (!date.is_empty()).then(|| date.to_string());
        self.note_identification(load, provided.clone());
        if let Some(entry) = self.out.facts.loads.get_mut(load) {
            entry.provided = provided;
        }
    }

    /// `\endinput` does not unwind the input stack: tex.web § 362 only sets
    /// `force_eof` for the file being read, so a token list or an argument
    /// scan that is still running finishes first, and the file ends when the
    /// reader comes back to it.
    pub fn end_input(&mut self) {
        for frame in self.input.iter_mut().rev() {
            if let Frame::File { mouth, .. } = frame {
                mouth.end_input();
                return;
            }
        }
    }

    /// The engine's `\end`: nothing after it is read, whatever file or
    /// token list it stood in (tex.web § 1335).
    pub fn end_job(&mut self) {
        // `\end{document}` ends the job inside the environment it names.
        if let Some((_, at)) = self.file_call
            && let Some(name) = crate::observe::current_environment(self).filter(|n| !n.is_empty())
        {
            self.occurrence(OccKind::EndEnvironment, name, None, at);
        }
        while !self.input.is_empty() {
            self.pop_frame();
        }
    }

}

/// How TeX names a group kind in its complaints (tex.web § 1069).
/// The token that opens a group of this kind, for reporting where an
/// unclosed one began.
fn opened_by(kind: GroupKind) -> &'static str {
    match kind {
        GroupKind::Simple => "{",
        GroupKind::SemiSimple => "\\begingroup",
        GroupKind::Math { .. } => "$",
    }
}

fn name_of(kind: GroupKind) -> &'static str {
    match kind {
        GroupKind::Simple => "}",
        GroupKind::SemiSimple => "\\endgroup",
        GroupKind::Math { .. } => "$",
    }
}

/// TeX removes one level of braces around an argument that is wholly grouped.
fn strip_braces(mut toks: Vec<Token>) -> Vec<Token> {
    if toks.len() < 2 || !toks[0].is_cat(Catcode::Begin) || !toks[toks.len() - 1].is_cat(Catcode::End) {
        return toks;
    }
    let mut depth = 0usize;
    for (i, t) in toks.iter().enumerate() {
        match t.tok {
            Tok::Chr(_, Catcode::Begin) => depth += 1,
            Tok::Chr(_, Catcode::End) => {
                depth -= 1;
                if depth == 0 && i + 1 != toks.len() {
                    return toks;
                }
            }
            _ => {}
        }
    }
    toks.pop();
    toks.remove(0);
    toks
}

/// Replace `#n` in a replacement text by the matched arguments.  Returns
/// `None` when the result would exceed `limit`, which is how nested
/// argument-duplicating macros are kept from exhausting memory.
pub fn substitute(m: &MacroDef, args: &[Vec<Token>], limit: usize) -> Option<Rc<[Token]>> {
    if args.is_empty() || !m.replacement_text.iter().any(|t| matches!(t.tok, Tok::Param(_))) {
        return Some(m.replacement_text.clone());
    }
    let size: usize = m
        .replacement_text
        .iter()
        .map(|t| match t.tok {
            Tok::Param(n) => args.get(n as usize - 1).map_or(1, Vec::len),
            _ => 1,
        })
        .sum();
    if size > limit {
        return None;
    }
    let mut out = Vec::with_capacity(size);
    for t in m.replacement_text.iter() {
        match t.tok {
            Tok::Param(n) => match args.get(n as usize - 1) {
                Some(arg) => out.extend_from_slice(arg),
                None => out.push(*t),
            },
            _ => out.push(*t),
        }
    }
    Some(Rc::from(out))
}

/// Collapse the runs of white space a token list detokenizes to, which is
/// how a document property reads on one line.
fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The category a character of a configured replacement text is read with.
fn text_catcode(c: char) -> Catcode {
    match c {
        ' ' => Catcode::Space,
        c if c.is_alphabetic() => Catcode::Letter,
        _ => Catcode::Other,
    }
}

/// Step over the number, dimension or glue a typesetting primitive scans
/// (tex.web §§ 440-461), which leaves the PDF string along with it
/// (hyperref.sty, `\HyPsd@AfterDimenRemove`).
fn skip_scanned(tokens: &[Token], i: &mut usize, scan: Typeset) {
    match scan {
        Typeset::Plain => {}
        Typeset::Number => skip_value(tokens, i, false),
        Typeset::Dimen => skip_value(tokens, i, true),
        Typeset::Glue => {
            skip_value(tokens, i, true);
            for keyword in ["plus", "minus"] {
                if skip_keyword(tokens, i, keyword) {
                    skip_value(tokens, i, true);
                }
            }
        }
        // The delimiter is one token, and it sets type rather than text.
        Typeset::Delimiter => {
            *i += usize::from(i < &mut tokens.len());
        }
    }
}

/// One ⟨number⟩ or ⟨dimen⟩: optional signs, digits, and for a dimension the
/// unit that follows.  An internal quantity stands in for the whole of it,
/// as `\kern\p@` does.
fn skip_value(tokens: &[Token], i: &mut usize, dimen: bool) {
    skip_spaces(tokens, i);
    while tokens.get(*i).is_some_and(|t| t.is_char('+') || t.is_char('-')) {
        *i += 1;
        skip_spaces(tokens, i);
    }
    if matches!(tokens.get(*i).map(|t| t.tok), Some(Tok::Cs(_))) {
        *i += 1;
        return;
    }
    while let Some(token) = tokens.get(*i) {
        match token.tok {
            Tok::Chr(c, _) if c.is_ascii_digit() || (dimen && (c == '.' || c == ',')) => *i += 1,
            _ => break,
        }
    }
    if !dimen {
        skip_spaces(tokens, i);
        return;
    }
    skip_spaces(tokens, i);
    skip_keyword(tokens, i, "true");
    if matches!(tokens.get(*i).map(|t| t.tok), Some(Tok::Cs(_))) {
        *i += 1;
        return;
    }
    // A unit is two letters: `pt`, `em`, `sp`, `fil` and its longer forms.
    for _ in 0..2 {
        if tokens.get(*i).is_some_and(|t| matches!(t.tok, Tok::Chr(c, _) if c.is_alphabetic())) {
            *i += 1;
        }
    }
    while tokens.get(*i).is_some_and(|t| t.is_char('l')) {
        *i += 1;
    }
    skip_spaces(tokens, i);
}

/// A PDF string as `\pdfstringdef` writes it (PDF reference, "String
/// Objects"): `\ddd` octal and `\n`-style escapes, and UTF-16BE after the
/// byte-order mark `\376\377`, PDFDocEncoding (Latin-1 here) otherwise.
fn decode_pdf_string(text: &str) -> String {
    let mut bytes = Vec::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            let mut buffer = [0u8; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buffer).as_bytes());
            continue;
        }
        match chars.next() {
            Some(d @ '0'..='7') => {
                let mut value = d.to_digit(8).unwrap_or(0);
                for _ in 0..2 {
                    match chars.peek() {
                        Some(&e @ '0'..='7') => {
                            value = value * 8 + e.to_digit(8).unwrap_or(0);
                            chars.next();
                        }
                        _ => break,
                    }
                }
                bytes.push(value as u8);
            }
            Some('n') => bytes.push(b'\n'),
            Some('r') => bytes.push(b'\r'),
            Some('t') => bytes.push(b'\t'),
            Some('b') => bytes.push(8),
            Some('f') => bytes.push(12),
            Some(other) => {
                let mut buffer = [0u8; 4];
                bytes.extend_from_slice(other.encode_utf8(&mut buffer).as_bytes());
            }
            None => {}
        }
    }
    if let Some(rest) = bytes.strip_prefix(&[0xfe, 0xff]) {
        let units: Vec<u16> = rest.chunks(2).map(|p| u16::from_be_bytes([p[0], *p.get(1).unwrap_or(&0)])).collect();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8(bytes.clone()).unwrap_or_else(|_| bytes.iter().map(|&b| b as char).collect())
}

fn skip_spaces(tokens: &[Token], i: &mut usize) {
    while tokens.get(*i).is_some_and(Token::is_space) {
        *i += 1;
    }
}

/// A keyword of a value's syntax, matched without regard to case as TeX
/// matches it (tex.web § 407).
fn skip_keyword(tokens: &[Token], i: &mut usize, keyword: &str) -> bool {
    skip_spaces(tokens, i);
    let mut at = *i;
    for wanted in keyword.chars() {
        let matched = tokens
            .get(at)
            .is_some_and(|t| matches!(t.tok, Tok::Chr(c, _) if c.eq_ignore_ascii_case(&wanted)));
        if !matched {
            return false;
        }
        at += 1;
    }
    *i = at;
    skip_spaces(tokens, i);
    true
}


/// The names definitions from partly unknown text may have made, each as the
/// known text before and after the unknown part: `\csname MT@inh@\x\endcsname`
/// may be `\MT@inh@…` but never `\@nil`.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Wild(Vec<(String, String)>);

impl Wild {
    /// Beyond this many patterns they are merged into what they share.
    const MAX: usize = 64;

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Names beginning with `prefix` and ending with `suffix` may be defined.
    pub(crate) fn add(&mut self, prefix: String, suffix: String) {
        let covered = |(p, s): &(String, String)| prefix.starts_with(p.as_str()) && suffix.ends_with(s.as_str());
        if self.0.iter().any(covered) {
            return;
        }
        self.0.retain(|(p, s)| !(p.starts_with(prefix.as_str()) && s.ends_with(suffix.as_str())));
        self.0.push((prefix, suffix));
        if self.0.len() > Self::MAX {
            // What every pattern shares still covers every one of them.
            let (mut p, mut s) = self.0[0].clone();
            for (q, t) in &self.0[1..] {
                p.truncate(p.chars().zip(q.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum());
                let keep: usize = s.chars().rev().zip(t.chars().rev()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum();
                s = s[s.len() - keep..].to_string();
            }
            self.0 = vec![(p, s)];
        }
    }

    pub(crate) fn union(&mut self, other: &Wild) {
        for (p, s) in &other.0 {
            self.add(p.clone(), s.clone());
        }
    }

    pub(crate) fn matches(&self, name: &str) -> bool {
        self.0.iter().any(|(p, s)| name.len() >= p.len() + s.len() && name.starts_with(p.as_str()) && name.ends_with(s.as_str()))
    }
}

/// The name standing for a `\csname` built from partly unknown text: its
/// known prefix and suffix around a separator no real name has.
pub(crate) const UNKNOWN_NAME: &str = "\u{2}unknown name:";
pub(crate) const UNKNOWN_NAME_GAP: char = '\u{3}';

impl Machine<'_> {
    /// The known prefix and suffix of a name built from partly unknown text.
    pub(crate) fn name_pattern(&self, sym: Sym) -> Option<(String, String)> {
        let rest = self.out.interner.name(sym).strip_prefix(UNKNOWN_NAME)?;
        let (prefix, suffix) = rest.split_once(UNKNOWN_NAME_GAP)?;
        Some((prefix.to_string(), suffix.to_string()))
    }

    /// Whether `sym` stands for a name built from partly unknown text.
    pub(crate) fn is_unknown_name(&self, sym: Sym) -> bool {
        self.out.interner.name(sym).starts_with(UNKNOWN_NAME)
    }

    /// The one control sequence `\csname` builds from partly unknown text.
    pub(crate) fn unknown_name_token(&mut self, prefix: &str, suffix: &str, span: Span) -> Token {
        let sym = self.intern(&format!("{UNKNOWN_NAME}{prefix}{UNKNOWN_NAME_GAP}{suffix}"));
        if !matches!(self.env.meaning(sym), Meaning::Unknown) {
            self.env.set(sym, Binding::builtin(Meaning::Unknown), true);
        }
        Token::new(Tok::Cs(sym), span)
    }

    /// Whether an undefined `sym` may have been defined by a name built from
    /// unknown text.
    pub(crate) fn may_be_defined(&self, sym: Sym) -> bool {
        !self.wild.is_empty() && self.wild.matches(self.out.interner.name(sym))
    }
}
