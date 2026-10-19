//! The mouth, the gullet and the stomach.
//!
//! `Machine` reads tokens from an input stack, expands macros, executes the
//! primitives `builtins` declares, and maintains the save stack. Where TeX
//! needs runtime information the interpreter abstracts: an undecided
//! conditional is analyzed branch by branch and the environments merged, and
//! a macro re-entering itself past `Config::max_expansions` is widened
//! instead of unfolded.

use crate::fixed::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

thread_local! {
    /// Where a panic inside [`Machine::run_guarded`] happened; `None` outside it.
    static PANIC_AT: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Panics inside a guarded run are reported as a gap, not printed.
fn guard_panics() {
    static HOOK: std::sync::Once = std::sync::Once::new();
    HOOK.call_once(|| {
        let printed = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let guarded = PANIC_AT.with(|cell| {
                let mut cell = cell.borrow_mut();
                let guarded = cell.is_some();
                if guarded {
                    *cell = Some(info.location().map(|l| l.to_string()).unwrap_or_default());
                }
                guarded
            });
            if !guarded || std::env::var_os("RUST_BACKTRACE").is_some() {
                printed(info);
            }
        }));
    });
}

use crate::builtins::{
    DefMode, LoadKind, OccKind, Primitive, Typeset, category, initial_meanings, kernel_meanings, pdf_string_skip,
};
use crate::config::Config;
use crate::env::{Binding, Env, GroupKind, NodeId};
use crate::facts::{Definition, Diagnostic, Expansion, Facts, Load, LoadStatus, MeaningKind, Occurrence, Severity};
use crate::format::{self, Format};
use crate::graph::{CallGraph, ControlDep, DependencyGraph, EdgeKind, VertexTag};
use crate::loader::Resolver;
use crate::plugin::Plugins;
use crate::project::Project;
use crate::tex::{
    ArgSpec, Catcode, CatcodeTable, EndLineChar, FileId, Interner, MacroDef, Meaning, Mouth, ParamItem, ParameterText,
    Span, Sym, TextClass, Tok, Token, text_of,
};
use crate::timing::{Phase, Timings};
use explore::Choice;

pub(crate) mod classes;
pub(crate) use classes::decide;
mod calls;
mod checkpoint;
pub use checkpoint::Checkpoints;
pub(crate) mod explore;
#[path = "package_cache.rs"]
mod package_cache;
use package_cache::Delta;
pub use package_cache::prune as prune_caches;

/// One kind of thing the interpreter did, recorded as an [`Event`] in the
/// trace.
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
/// at which [`Span`]. Held by [`Analysis::trace`], the sequence `satex trace` prints.
#[derive(Clone)]
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
    /// The name a diagnostic gives this file: its base name, or base name
    /// under its directory when another file of the run shares the base
    /// name (else two same-named files read as one that cannot balance).
    pub short: String,
}

/// A limit (`limits.<key>` in the configuration) that cut the run.
#[derive(Clone, Debug)]
pub struct LimitHit {
    pub key: &'static str,
    pub value: u64,
    pub at: Span,
    pub count: u32,
}

/// Where the LaTeX2e kernel came from for this run: the `latex.ltx` path,
/// whether it was read from a [`Format`] cache, and how far interpretation of
/// it got.
#[derive(Clone)]
pub struct FormatInfo {
    pub source: String,
    pub definitions: usize,
    pub cached: bool,
    /// The cache file this format was read from or written to.
    pub cache: Option<String>,
    /// The last line of the format that produced a definition, and how many
    /// lines it has. A large gap means the kernel was only partly read.
    pub reached: u32,
    pub lines: u32,
    /// Tokens the kernel build took (0 when read from a cache).
    pub steps: u64,
    /// Why the kernel was built and not read from its cache, when it was.
    pub rebuilt: Option<String>,
    /// The commands the kernel wrote its definitions with ([`crate::exports::written`]).
    pub definers: Vec<String>,
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

/// Cached per-character font state: the five NFSS names, the bodies they
/// had when last resolved, and the resolved row. Costs a few pointer
/// comparisons instead of five macro lookups.
#[derive(Clone, Default)]
pub(crate) struct FontWatch {
    pub(crate) names: Option<[Option<Sym>; 5]>,
    /// Held, not just pointed at, so a freed body cannot be mistaken for
    /// an unchanged one through a reused address.
    pub(crate) bodies: [Option<std::rc::Rc<[Token]>>; 5],
    /// The same bodies as bare addresses, which is what the per-character
    /// comparison reads.
    pub(crate) at: [Option<*const u8>; 5],
    pub(crate) state: u16,
    /// `\pickup@font` and `\font@name`.
    pub(crate) pick: Option<(Sym, Sym, Sym)>,
}

/// A font `\font` loaded: the TFM name it was given and the size it was
/// loaded at (tex.web § 1257), which together locate its metrics.
#[derive(Clone, Debug)]
pub struct FontUse {
    pub name: String,
    pub size: crate::value::Scaled,
}

/// Set by `--stats`: the run then counts its work per source line.
pub static HOT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Work per source line, keyed by file and line.
#[derive(Clone, Default)]
pub struct Hot {
    pub steps: HashMap<(FileId, u32), u64>,
    pub forks: HashMap<(FileId, u32), u64>,
    pub joins: HashMap<(FileId, u32), u64>,
    pub heads: HashMap<(FileId, u32), u64>,
    /// Untyped unknown text made, per place in satex's source that made it.
    pub untyped: HashMap<(&'static str, u32), u64>,
}

impl Analysis {
    /// Counts one unit of work at `span` in a hotspot table, when `--stats` is on.
    pub(crate) fn heat(&mut self, table: fn(&mut Hot) -> &mut HashMap<(FileId, u32), u64>, span: Span) {
        if let Some(hot) = &mut self.hot {
            *table(hot).entry((span.file, span.line)).or_default() += 1;
        }
    }

    /// The `n` hottest lines of each table, one line per table.
    pub fn hotspots(&self, n: usize) -> Vec<String> {
        let Some(hot) = &self.hot else { return Vec::new() };
        let forks: u64 = hot.forks.values().sum();
        let (r, slots) = (
            crate::env::RESUMES.load(std::sync::atomic::Ordering::Relaxed),
            crate::env::RESUME_SLOTS.load(std::sync::atomic::Ordering::Relaxed),
        );
        let switches = format!(
            "  paths: {} forks, {} resumes carrying {} slots",
            compact_count(forks),
            compact_count(r),
            compact_count(slots)
        );
        let tables = [("steps", &hot.steps), ("forks", &hot.forks), ("joins", &hot.joins), ("heads", &hot.heads)];
        tables
            .iter()
            .filter(|(_, t)| !t.is_empty())
            .map(|(name, t)| {
                let mut rows: Vec<_> = t.iter().collect();
                rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
                let at = rows.iter().take(n).map(|((f, l), c)| {
                    let file = self.file_name(*f);
                    format!("{} {}:{l}", compact_count(**c), file.rsplit('/').next().unwrap_or(file))
                });
                format!("  hot {name} {}", at.collect::<Vec<_>>().join("  "))
            })
            .chain(std::iter::once(switches))
            .chain({
                let mut rows: Vec<_> = hot.untyped.iter().collect();
                rows.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
                rows.into_iter()
                    .map(|(at, n)| format!("  untyped-unknown made at {}:{}: {n}", at.0, at.1))
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

fn compact_count(n: u64) -> String {
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

/// Everything a run produced: the ending [`Env`] and [`CatcodeTable`], the
/// recorded [`Facts`] and [`DependencyGraph`], and the bookkeeping every
/// `satex` command reads instead of the live [`Machine`].
#[derive(Clone)]
pub struct Analysis {
    pub interner: Interner,
    pub graph: DependencyGraph,
    pub calls: CallGraph,
    /// Which macro's replacement text each expansion was written in, in the
    /// order met: what recursion is read from.
    pub observed: CallGraph,
    pub files: Vec<FileInfo>,
    /// The file the user asked about, as opposed to everything it loads.
    pub main_file: FileId,
    /// Whether that file is a literate source, and which kind. What satex
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
    /// Files read and tokens taken at `\begin{document}`: the preamble's
    /// share of the build.
    pub preamble_cost: Option<(usize, u64)>,
    /// Characters set in the body's justified paragraphs.
    pub justified_text: u64,
    /// For each file, the position in the main file where its contents
    /// become visible: a definition in it is in scope from there.
    pub entry: Vec<Option<Span>>,
    /// Where the LaTeX2e kernel came from, and whether it was cached.
    pub format: Option<FormatInfo>,
    /// The `satex.yaml` in force, when one was read, and the settings it made.
    pub config: Option<PathBuf>,
    pub settings: Config,
    /// `\title`, `\author` and `\date` as PDF document properties, each
    /// with the token list it was made from.
    pub metadata: Vec<Metadata>,
    /// Where the kernel defined each name it brought in, so a name with no
    /// definition site in this run still points at `latex.ltx`.
    pub kernel_sites: HashMap<Sym, Span>,
    /// The fonts the run loaded, indexed by the number `\the\font` gives
    /// them (what [`crate::facts::FontState::font`] indexes).
    pub fonts: Vec<Option<FontUse>>,
    /// The distinct font states passed through, row 0 the unknown one:
    /// what [`crate::facts::Spot::font_state`] indexes.
    pub font_states: Vec<crate::facts::FontState>,
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
    /// The part of `steps` spent in outermost class and package loads, read or replayed.
    pub load_steps: u64,
    /// The limits that cut the run somewhere, in the order first reached.
    pub limits: Vec<LimitHit>,
    /// Source lines read from files, each counted to where it stopped.
    pub lines: u64,
    pub exhausted: bool,
    /// Final sizes of the interpreter's working sets.
    pub journal_size: usize,
    pub held_tokens_left: usize,
    /// The most loop heads one drive kept at once.
    pub loop_heads: usize,
    /// The most paths one drive held at once, the running one included.
    pub peak_paths: usize,
    /// Undecided choices made, each replay of one included: the work of
    /// the paths, which `steps` counts only in part.
    pub forks: u64,
    /// Where the run spent itself, counted only under `--stats`.
    pub hot: Option<Box<Hot>>,
    /// Slots the joins of paths compared: their work, which `steps` leaves out.
    pub joined_slots: u64,
    /// The last 256 MiB step the memory report named.
    pub memory_reported: usize,
    /// Untyped unknown text made: text no typed text covers.
    pub untyped: u64,
    /// The package caches this run met: package, cache file, and whether it
    /// was `cached` (read), `stored`, or `stale: …` and read again.
    pub package_caches: Vec<(String, String, String)>,
    /// The packages among them that were classes.
    pub class_caches: std::collections::BTreeSet<String>,
    /// Files served at least in part from a package cache replay rather
    /// than read token by token: what `summary --timings` marks as cached
    /// instead of pricing at zero.
    pub cached_files: crate::fixed::HashSet<FileId>,
    /// The bindings and category codes at `\begin{document}`, for probing
    /// what a command takes in the preamble.
    pub preamble: Option<Box<(Env, CatcodeTable)>>,
    /// The bindings the document started with, the format's: what a name
    /// means where the document uses it before anything redefines it.
    pub kernel: Option<Box<Env>>,
    /// What [`crate::probe`] found per name and view, computed on demand.
    pub probes: crate::fixed::Mutex<HashMap<(Sym, crate::probe::State), Option<crate::probe::Probed>>>,
    /// Environments open right now, outermost first (`document` excluded:
    /// `document_depth` already answers that axis). What `explain` reads to
    /// tell a definition made inside `enumerate` from one made outside.
    pub env_stack: Vec<Sym>,
    /// Environments worth probing a name inside, and what that found: cache
    /// for [`crate::probe::probe_contexts`].
    pub context_probes: crate::fixed::Mutex<ContextProbes>,
    /// The fewest arguments each defined name takes, for completion: built
    /// once, from the definitions query.
    pub takes: std::sync::OnceLock<crate::fixed::HashMap<String, u8>>,
    /// The entries of the bibliography databases the document names, for
    /// completion: read once.
    pub bib: std::sync::OnceLock<Vec<crate::plugin::bib::Entry>>,
}

/// Collection sizes change slowly. Checking them on every token would
/// cost more than it saves.
const CHECK_INTERVAL: u64 = 256;

/// How many tokens of an input list a call's shape hashes: an identity
/// key, not a limit on what runs.
const SHAPE_TOKENS: usize = 64;
/// Slots of [`Machine::shape_memo`].
const SHAPE_MEMO: usize = 1024;
/// One slot of [`Machine::shape_memo`]: the list, the position in it, and
/// the hash of what is pending there.
type ShapeSlot = Option<(Rc<[Token]>, usize, u64)>;

/// Peak resident set size in kibibytes, where the platform reports it.
pub fn peak_memory() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|l| l.strip_prefix("VmHWM:")).and_then(|v| v.split_whitespace().next()?.parse().ok())
}

/// A macro found to reach itself.
pub struct Recursion {
    pub name: Sym,
    /// The names of the cycle, `name` among them.
    pub cycle: Vec<Sym>,
    /// The run expanded the cycle. Otherwise it is read off replacement
    /// texts of macros nothing expanded, which only says the names are
    /// there, not that expansion reaches them.
    pub observed: bool,
}

impl Analysis {
    /// Macros whose expansion was seen to reach themselves, and, for macros
    /// the run never expanded, the cycles their replacement texts name.
    pub fn recursion(&self) -> Vec<Recursion> {
        let expanded: crate::fixed::HashSet<Sym> = self.facts.expansions.iter().map(|e| e.name).collect();
        let in_document: crate::fixed::HashSet<Sym> =
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
        let cfg = Config {
            load: crate::config::Load { format: false, ..Default::default() },
            texmf: crate::config::Texmf { kpsewhich: false, ..Default::default() },
            ..Config::default()
        };
        Machine::new(&cfg, PathBuf::new()).out
    }

    /// Is a definition made at `span` in scope at `line:col` of `file`? Both
    /// climb to the file they share: a subfile position sees what its
    /// `\input` call saw, and a definition sits where its call does.
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

    /// One line per limit that cut the run: which, where first, and how to
    /// raise it.
    pub fn limit_notes(&self) -> Vec<String> {
        self.limits
            .iter()
            .map(|hit| {
                format!(
                    "limits.{} = {} was reached {}× (first at {}:{}); raise it with --set limits.{}=<larger>",
                    hit.key,
                    hit.value,
                    hit.count,
                    self.short_name(hit.at.file),
                    hit.at.line,
                    hit.key
                )
            })
            .collect()
    }

    /// One line of resource accounting, for benchmarking and for the budgets.
    pub fn stats(&self, elapsed: std::time::Duration) -> String {
        let facts = &self.facts;
        let c = |n: usize| compact_count(n as u64);
        let peak = peak_memory().map_or(String::new(), |kb| format!("  {} MiB peak", kb / 1024));
        let exhausted = if self.exhausted { "  |  budget exhausted" } else { "" };
        [
            format!("{:.3}s{peak}{exhausted}", elapsed.as_secs_f64()),
            format!(
                "{} files  {} lines  {} tokens ({} loading)",
                c(self.files.len()),
                compact_count(self.lines),
                compact_count(self.steps),
                compact_count(self.load_steps)
            ),
            format!(
                "{} definitions  {} expansions  {} occurrences  {} diagnostics",
                c(facts.defs.len()),
                c(facts.expansions.len()),
                c(facts.occurrences.len()),
                c(facts.diagnostics.len())
            ),
            format!("{} vertices  {} edges", c(self.graph.len()), c(self.graph.edge_count())),
            format!(
                "{} names  {} save-stack  {} held  {} loop heads",
                c(self.interner.len()),
                c(self.journal_size),
                c(self.held_tokens_left),
                c(self.loop_heads)
            ),
        ]
        .join("  |  ")
    }

    /// One line per cache this run met: kernel, class, hooks and package
    /// segments, whether reused or rebuilt and why.  Reused packages are counted.
    pub fn cache_report(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(f) = &self.format {
            let engine = self.plugins.engine.as_str();
            lines.push(match &f.rebuilt {
                None => format!("cache kernel {engine}: reused"),
                Some(why) => format!("cache kernel {engine}: rebuilt ({why})"),
            });
        }
        let mut last: Vec<(&str, &str)> = Vec::new();
        for (name, _, state) in &self.package_caches {
            match last.iter_mut().find(|(n, _)| n == name) {
                // A stale cache read again and stored: the reason is why it was read.
                Some(entry) if state == "stored" && entry.1.starts_with("stale: ") => {}
                Some(entry) => entry.1 = state,
                None => last.push((name, state)),
            }
        }
        let mut reused = 0;
        for (name, state) in last {
            let kind = if self.class_caches.contains(name) { "class" } else { "package" };
            let verdict = match state {
                "cached" => None,
                "stored" => Some("missing".to_string()),
                s if s.starts_with("stale: ") => Some(s["stale: ".len()..].to_string()),
                s if s.starts_with("not stored: ") => Some(format!("not stored, {}", &s["not stored: ".len()..])),
                s => Some(match s.split_once(" cached (") {
                    Some((pct, why)) => format!("{pct} reused, {}", why.split(')').next().unwrap_or(why)),
                    None => s.replace(", rest stored", ""),
                }),
            };
            match verdict {
                None if kind == "package" => reused += 1,
                None => lines.push(format!("cache {kind} {name}: reused")),
                Some(why) => lines.push(format!("cache {kind} {name}: rebuilt ({why})")),
            }
        }
        if reused > 0 {
            lines.push(format!("cache package segments: {reused} reused"));
        }
        lines
    }

    pub fn file_name(&self, id: FileId) -> &str {
        self.files.get(id as usize).map_or("<input>", |f| f.path.as_str())
    }
    pub fn short_name(&self, id: FileId) -> &str {
        self.files.get(id as usize).map_or("<input>", |f| f.short.as_str())
    }
    pub fn file_names(&self) -> Vec<String> {
        self.files.iter().map(|f| f.path.clone()).collect()
    }
}

/// How many positions of tokens given back are remembered: the window a
/// lookahead's token has to be read again within to still count as the
/// file's.
pub(crate) const RECENT_FILE_SPANS: usize = 64;

/// Records which primitive each of the engine's second spellings copies,
/// so that `\meaning` names the primitive, as it does for a `\let` copy.
fn set_primitive_aliases(env: &mut Env, interner: &mut Interner, engine: crate::config::Engine) {
    for (alias, primitive) in crate::builtins::primitive_aliases(engine) {
        env.aliases.insert(interner.intern(alias), interner.intern(primitive));
    }
}

/// A loop head without its content: call site, deciding command, source
/// progress.
pub(crate) type LoopId = (Span, Sym, u64);

#[derive(Clone)]
pub(crate) enum Frame {
    /// A file being read. `depth` is the group nesting the file started
    /// at, so a group it leaves open can be reported and given up.
    File {
        mouth: Mouth,
        package: Option<Sym>,
        depth: usize,
        /// The conditional nesting the file started at, so that one it leaves
        /// open does not govern the rest of the run.
        conds: usize,
        /// Whether `\everyeof` was inserted at its end (etex.ch `eof_seen`).
        eof_seen: bool,
    },
    /// A token list. One expansion may stand on several: a long argument
    /// is shared, not copied, into its replacement text
    /// ([`substitute_parts`]). Its lowest list is the expansion's own, and
    /// the ones above it are `upper`: their tokens are its tokens, but the
    /// expansion ends with the lowest. `held` is what the list counts in
    /// `held_tokens`: a shared argument counts at its first use only.
    Tokens {
        toks: Rc<[Token]>,
        pos: usize,
        expanding: Option<Sym>,
        held: usize,
        upper: bool,
        /// The conditionals, the innermost group and the assignment count
        /// when it was pushed: what its text opened and wrote lies above.
        at: (usize, u64, u64),
    },
    /// A `\scantokens` pseudo file.  Its characters are tokenized as they
    /// are read, so a catcode change inside it governs the rest of it
    /// (etex.ch `pseudo_input`). `eof` is the `\everyeof` read at its end.
    Pseudo { mouth: Box<Mouth>, at: Span, eof: Rc<[Token]>, pos: usize },
    /// Tokens a lookahead read and gave back, newest last.  It lives on the
    /// input stack so that a file opened after the lookahead is still read
    /// before them, which is where TeX inserts it.
    Returned(Vec<Token>),
    /// Nothing is read across this: it ends a token list that is being run to
    /// completion on its own, such as an option body or a conditional arm.
    Boundary,
}

/// The input stack (tex.web § 300), its frames shared with the path
/// snapshots that saw them: a snapshot is taken before every command, and
/// copies pointers. A frame is copied only when a command reads on in it
/// while a snapshot still holds it.
#[derive(Clone, Default)]
pub(crate) struct Input(Vec<Rc<Frame>>);

impl Input {
    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &Frame> + ExactSizeIterator {
        self.0.iter().map(|f| &**f)
    }

    pub(crate) fn iter_mut(&mut self) -> impl DoubleEndedIterator<Item = &mut Frame> {
        self.0.iter_mut().map(Rc::make_mut)
    }

    pub(crate) fn get(&self, i: usize) -> Option<&Frame> {
        self.0.get(i).map(|f| &**f)
    }

    pub(crate) fn last(&self) -> Option<&Frame> {
        self.0.last().map(|f| &**f)
    }

    pub(crate) fn last_mut(&mut self) -> Option<&mut Frame> {
        self.0.last_mut().map(Rc::make_mut)
    }

    pub(crate) fn push(&mut self, frame: Frame) {
        self.0.push(Rc::new(frame));
    }

    pub(crate) fn pop(&mut self) -> Option<Frame> {
        self.0.pop().map(Rc::unwrap_or_clone)
    }
}

impl<'a> IntoIterator for &'a Input {
    type Item = &'a Frame;
    type IntoIter = std::iter::Map<std::slice::Iter<'a, Rc<Frame>>, fn(&Rc<Frame>) -> &Frame>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter().map(|f| &**f)
    }
}

impl FromIterator<Frame> for Input {
    fn from_iter<I: IntoIterator<Item = Frame>>(frames: I) -> Input {
        Input(frames.into_iter().map(Rc::new).collect())
    }
}

impl std::ops::Index<usize> for Input {
    type Output = Frame;
    fn index(&self, i: usize) -> &Frame {
        &self.0[i]
    }
}

impl std::ops::IndexMut<usize> for Input {
    fn index_mut(&mut self, i: usize) -> &mut Frame {
        Rc::make_mut(&mut self.0[i])
    }
}

/// The `\global`, `\long`, `\outer` and `\protected` prefixes waiting to
/// apply to the next definition or assignment. [`Machine::define`] consumes
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

/// How a conditional level ends (tex.web § 489: `if_limit`). [`CondFrame::limit`]
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
#[derive(Clone, Debug)]
pub struct CondFrame {
    pub limit: CondLimit,
    /// Whether this level pushed a control dependency.
    pub dep: bool,
    /// Where the `\if…` stood, so that its `\fi` can close the construct.
    pub at: Span,
    /// e-TeX's `\currentiftype`, `None` where satex cannot tell.
    pub kind: Option<i8>,
    /// It stands for any number of such conditionals, none too (a widened
    /// stack).
    pub many: bool,
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

/// tex.web § 392 `long_state`: why an argument scan ran away. Only `\par`
/// in a macro that is not `\long` prints "Paragraph ended" (§ 396); an
/// `\outer` token or the file's end set `outer_call` and report their own
/// error (§§ 336-338). `Cut` is satex's own: an argument met a `}` in
/// text a cut left after its `\unknown` ([`Machine::after_cut`]), whose `{`
/// the summarized code may have taken. The call is dropped as TeX's
/// recovery would drop it, but no error is TeX's there.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, serde::Serialize)]
pub enum Runaway {
    #[default]
    No,
    Par,
    Outer,
    FileEnded,
    Cut,
}

/// One run of [`Machine::drive`]: a main-loop command, or one token of
/// the expanding scan on top of [`Machine::scans`].
#[derive(Clone, Copy)]
pub(crate) enum Unit {
    /// A main-loop command.
    Command,
    /// One token of a body read while expanding (tex.web § 477), which ends
    /// at its `}`. `opened_in` and `gaps` are for `body_lost_its_end`.
    Body { opened_in: Option<FileId>, gaps: u64 },
    /// One token of a list expanded to its end.
    Expand,
    /// One token of the name `\csname` builds (tex.web § 372).
    Name,
    /// A whole ⟨number⟩ scan in one run (`\number`, `\romannumeral`). Arms
    /// that end it in one place join, their values one hull.
    Number,
}

/// The name a `\csname` has built so far ([`Machine::scan_csname_chars`]).
#[derive(Clone)]
pub(crate) struct NameAccum {
    /// The names the joined paths built, each the text before its first
    /// unknown part (once it has one) and the text after its last. The
    /// name is one of them, split again once it is complete.
    pub(crate) names: Vec<(Option<String>, String)>,
    pub(crate) at: Option<Span>,
    pub(crate) at_written: bool,
    pub(crate) files: Vec<FileId>,
    pub(crate) closed: bool,
    pub(crate) stopped: Option<Sym>,
    /// Whether every unknown part, and what the names differ in, is
    /// digits: the name is then the text around a number.
    pub(crate) gap_digits: bool,
    /// The `\csname` that reads it: where its choices are made.
    pub(crate) site: Span,
}

impl Default for NameAccum {
    fn default() -> NameAccum {
        NameAccum {
            names: vec![(None, String::new())],
            at: None,
            at_written: false,
            files: Vec::new(),
            closed: false,
            stopped: None,
            gap_digits: true,
            site: Span::default(),
        }
    }
}

impl NameAccum {
    pub(crate) fn push(&mut self, c: char) {
        for (_, name) in &mut self.names {
            name.push(c);
        }
    }

    /// An unknown part, digits or not: what came before it is the
    /// prefix, and the name goes on after it. Text between two unknown
    /// parts becomes part of the gap.
    pub(crate) fn unknown_part(&mut self, digits: bool) {
        self.gap_digits &= digits;
        for (prefix, name) in &mut self.names {
            if prefix.is_some() && !name.chars().all(|c| c.is_ascii_digit()) {
                self.gap_digits = false;
            }
            prefix.get_or_insert_with(|| name.clone());
            name.clear();
        }
    }

    /// The names as one: what they all begin with, an unknown part, and
    /// what they all end with after it.
    pub(crate) fn collapse(&mut self) {
        let names = std::mem::take(&mut self.names);
        let mut all = names.iter();
        let mut one = all.next().expect("a name").clone();
        for other in all {
            let (p, n) = explore::join_name_parts((&one.0, &one.1), (&other.0, &other.1));
            one = (Some(p), n);
        }
        // What a name has between the shared ends is the gap, too.
        if let (Some(p), n) = &one {
            for (prefix, name) in &names {
                let text = prefix.as_deref().map_or_else(|| name.clone(), |prefix| format!("{prefix}{name}"));
                let middle = text.strip_prefix(p.as_str()).and_then(|t| t.strip_suffix(n.as_str()));
                self.gap_digits &= middle.is_some_and(|m| m.chars().all(|c| c.is_ascii_digit()));
            }
        }
        self.names = vec![one];
    }
}

/// What an expanding scan has made so far. It goes with the path, so the
/// arms of a choice inside the scan join where they meet again. Older
/// tokens are frozen in shared chunks, so saving a path costs little.
#[derive(Clone, Default)]
pub(crate) struct Accum {
    chunks: Vec<Rc<[Token]>>,
    tail: Vec<Token>,
    /// Braces open inside the body.
    depth: usize,
    /// The scan reached its end.
    done: bool,
    /// Set for a name scan, which holds text instead of tokens.
    pub(crate) name: Option<Box<NameAccum>>,
    /// What a [`Unit::Number`] scan read: its value, else its hull.
    pub(crate) number: Option<(Option<i64>, Option<crate::value::Num>)>,
}

impl Accum {
    /// Tokens the tail holds before it is frozen into a chunk. Small, since
    /// every path snapshot clones the tail.
    const CHUNK: usize = 32;

    pub(crate) fn len(&self) -> usize {
        self.chunks.iter().map(|c| c.len()).sum::<usize>() + self.tail.len()
    }

    fn freeze(&mut self) {
        if self.tail.len() >= Self::CHUNK {
            self.chunks.push(Rc::from(std::mem::take(&mut self.tail)));
        }
    }

    pub(crate) fn to_vec(&self) -> Vec<Token> {
        let mut out = Vec::with_capacity(self.len());
        for chunk in &self.chunks {
            out.extend_from_slice(chunk);
        }
        out.extend_from_slice(&self.tail);
        out
    }

    /// An empty name scan.
    pub(crate) fn for_name() -> Accum {
        Accum { name: Some(Box::default()), ..Default::default() }
    }

    /// One built from `toks`, standing as this one does.
    pub(crate) fn with(&self, toks: Vec<Token>) -> Accum {
        Accum { chunks: Vec::new(), tail: toks, depth: self.depth, done: self.done, name: None, number: self.number }
    }

    /// The name a name scan built: what stands around its unknown part
    /// when it has one.
    /// The name built, or the text around its unknown part and whether
    /// that part is digits.
    pub(crate) fn built(&self) -> Result<String, (String, String, bool)> {
        let mut n = self.name.as_deref().cloned().unwrap_or_default();
        n.collapse();
        match n.names.remove(0) {
            (None, name) => Ok(name),
            (Some(prefix), name) => Err((prefix, name, n.gap_digits)),
        }
    }

    /// What a join keeps apart: how deep in braces, and whether it is over.
    pub(crate) fn shape(&self) -> (usize, bool) {
        (self.depth, self.done)
    }
}

/// How deeply expansion may nest inside scans before the run stops.
const MAX_NESTING: u32 = 300;

/// How much of the Rust stack nested steps may take before the run stops,
/// whatever each level costs: half the 2 MiB a spawned thread gets.
/// Only where the thread's own stack is not known.
const MAX_STACK_BYTES: usize = 1 << 20;

/// The stack nested steps leave free where the thread's stack is known.
const STACK_MARGIN: usize = 1 << 20;

/// The lowest address of this thread's stack, where the platform tells.
/// A level costs about 13 KiB, so the 8 MiB main stack holds ~500 levels.
fn stack_low() -> Option<usize> {
    thread_local! {
        static LOW: std::cell::Cell<Option<Option<usize>>> = const { std::cell::Cell::new(None) };
    }
    LOW.with(|low| {
        if let Some(known) = low.get() {
            return known;
        }
        let found = stack_low_os();
        low.set(Some(found));
        found
    })
}

#[cfg(target_os = "linux")]
fn stack_low_os() -> Option<usize> {
    // SAFETY: the attribute is initialised by pthread_getattr_np and destroyed once read.
    unsafe {
        let mut attr: libc::pthread_attr_t = std::mem::zeroed();
        if libc::pthread_getattr_np(libc::pthread_self(), &mut attr) != 0 {
            return None;
        }
        let (mut addr, mut size) = (std::ptr::null_mut(), 0usize);
        let ok = libc::pthread_attr_getstack(&attr, &mut addr, &mut size) == 0;
        libc::pthread_attr_destroy(&mut attr);
        (ok && size > 2 * STACK_MARGIN).then_some(addr as usize)
    }
}

#[cfg(not(target_os = "linux"))]
fn stack_low_os() -> Option<usize> {
    None
}

/// Prefix for the names under which registers addressed by number are kept.
const REGISTER: char = '\u{2}';
/// What a register's own name starts with: `\u{2}count3`.
pub const REGISTER_PREFIX: char = REGISTER;

/// A text copied through into an `\edef` body ([`Machine::copy_through`]):
/// whose, where in the body and how long, how much longer it could have
/// been within the limits met, and whether it predates the load recorded.
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

/// The files this run opened with `\openout`, by name: the text written so
/// far, or `None` once what they hold is unknown (a deferred `\write`, an
/// unpinned stream).
type Outputs = Rc<HashMap<String, Option<String>>>;

/// The stream a number satex could not pin down names.  `i64::MIN` is past
/// what `scan_int` can give (tex.web § 445), so no source reaches it.
const ANY_STREAM: i64 = i64::MIN;

/// Where a stream known to be closed stands: past lines it does not have,
/// which no open stream is.  Absence says the same, except after a
/// widening, where it only means nothing is known (tex.web § 1275).
const CLOSED: usize = usize::MAX;

/// Whether two paths know the same of the `\openin` streams (tex.web
/// § 480).  The tables are shared, so the pointers usually settle it.
fn same_streams(a: &Streams, b: &Streams) -> bool {
    Rc::ptr_eq(a, b) || a == b
}

/// The join of two stream states (tex.web § 480): a stream is known only
/// where both sides know it in the same state, open on the same lines at
/// the same place or closed.  Every other one is unknown.
fn join_streams(a: &Streams, b: &Streams) -> Streams {
    if same_streams(a, b) {
        return a.clone();
    }
    // A stream absent from a table that was never widened is closed.
    let state = |t: &Streams, s: i64| match t.get(&s) {
        None if !t.contains_key(&ANY_STREAM) => Some((Rc::new(Vec::new()), CLOSED)),
        known => known.cloned(),
    };
    let mut joined: HashMap<_, _> =
        (0..16).filter_map(|s| state(a, s).filter(|x| state(b, s).as_ref() == Some(x)).map(|x| (s, x))).collect();
    joined.insert(ANY_STREAM, (Rc::new(Vec::new()), 0));
    Rc::new(joined)
}

/// One struct of per-load fields and its [`Delta`]: what a load changed in
/// each, so a cache stores the change and a replay applies it.
macro_rules! carried {
    ($($(#[$doc:meta])* pub(crate) $field:ident: $ty:ty,)*) => {
        #[derive(Default, Clone)]
        pub(crate) struct Carried { $($(#[$doc])* pub(crate) $field: $ty,)* }

        #[derive(Default, Debug, serde::Serialize, serde::Deserialize)]
        pub(crate) struct CarriedDelta { $($field: <$ty as Delta>::Change,)* }

        impl Carried {
            pub(crate) fn delta(&self, before: &Self) -> CarriedDelta {
                CarriedDelta { $($field: self.$field.delta(&before.$field),)* }
            }

            pub(crate) fn apply(&mut self, change: &CarriedDelta) {
                $(self.$field.apply(&change.$field);)*
            }
        }
    };
}

// Per-load state a package cache stores as part of a load's effect:
// what the load changed is kept and a replay applies it.
carried! {
    /// The switches the document's own files declare (`\newif` and the
    /// like): the ones `satex controls` leaves undecided without a name.
    pub(crate) own_switches: HashSet<Sym>,
    /// Names `\csname` built where the document wrote only part of them.
    pub(crate) foreign_names: HashSet<(Sym, Span)>,
    /// The files the `\read` streams are open on, by stream number.
    pub(crate) stream_paths: HashMap<i64, String>,
    /// The furthest line the progress bar saw read by `\read` in each file.
    pub(crate) stream_lines: HashMap<String, u64>,
    /// The stream a `\read` last took a line from, its line, its length and
    /// the step it was read at: shown under the progress bar while it is read.
    pub(crate) stream_read: Option<(i64, usize, usize, u64)>,
    /// The furthest line the progress bar saw read in each file.
    pub(crate) lines_seen: HashMap<FileId, u32>,
    /// The serial of the last rest made ([`Machine::unknown_text`]).
    pub(crate) rest_serial: u32,
    /// The last control sequence a call took from a project file as an
    /// argument delimiter (tex.web § 392): read, but never run, so it is
    /// not a `file_call` of its own.
    pub(crate) absorbed: Option<Span>,
}

/// Per-load state every cacheable load forgets at its end, read or
/// replayed alike ([`Machine::end_cacheable_load`]).
#[derive(Clone, Default)]
pub(crate) struct Scratch {
    /// Where a region's effect summary stands for the unknown text it left
    /// (a recursion cut: its widened fixpoint holds every deeper level's
    /// writes), so running that text adds nothing. Elsewhere unknown text
    /// runs under a reported assumption.
    pub(crate) summarized: HashSet<Span>,
    /// Per loop identity, across drives: how often it was summarized, and
    /// how often it was entered afresh (capped in explore.rs).
    pub(crate) loop_summaries: HashMap<LoopId, (u32, u32)>,
    /// The loops this path continues the summary of: never dropped as
    /// their re-entry.
    pub(crate) summary_of: Rc<Vec<LoopId>>,
    /// Per loop, the bounded text this path's last pass began with
    /// ([`Machine::input_bound`]).
    pub(crate) loop_bounds: Rc<Vec<(LoopId, u32)>>,
    /// The last loop cut with no path waiting: its head, the source
    /// progress, and how often it was met there since ([`Machine::end_covered`]).
    pub(crate) recut: Option<(Span, u64, u32)>,
    /// The gap that has now been met `limits.repeat_failures` times at one
    /// place: said at the next step.
    pub(crate) repeat_cut: Option<(&'static str, Span)>,
    /// `gaps_raised` when each conditional's test was read, by where it stands.
    pub(crate) cond_gaps: HashMap<Span, u64>,
    /// How often satex approximated so far: a limitation reported, a
    /// widening, unknown text made. Only monotone.
    pub(crate) approximations: u64,
    /// [`Scratch::approximations`] when the main loop took its command.
    pub(crate) approximations_at_command: u64,
}

/// The interpreter itself: reads [`Token`]s from the input stack, expands
/// macros, executes primitives, changes [`Env`] and [`CatcodeTable`], and
/// records what it saw into [`Analysis`].
#[derive(Clone)]
pub struct Machine<'a> {
    pub cfg: &'a Config,
    /// What a load changes that its cache carries.
    pub(crate) carried: Carried,
    /// What every cacheable load forgets at its end.
    pub(crate) scratch: Scratch,
    /// The precision limits in force: `cfg.limits` until [`Machine::degrade`]
    /// lowers them.
    pub(crate) limits: crate::config::Limits,
    /// Where and why the precision limits were first lowered, reported at
    /// the end.
    degraded_at: Option<(&'static str, Span)>,
    /// How often the precision limits were halved (`precision-degraded`):
    /// 0 for full precision.
    pub(crate) pressure: u8,
    /// The last language's patterns [`crate::hyph`] parsed, by their text.
    pub(crate) hyph_cache: Option<(Rc<[Token]>, Rc<crate::hyph::Patterns>)>,
    /// The paragraphs' recorded item lists, for the line breaker.
    pub(crate) kp: crate::linebreak::Arena,
    pub out: Analysis,
    pub env: Env,
    pub catcodes: CatcodeTable,
    /// LuaTeX's catcode tables and what the Lua chunks read.
    pub lua: crate::plugin::lua::Lua,
    /// `\endlinechar`, which expl3 changes.
    pub(crate) end_line: EndLineChar,
    input: Input,
    pub prefixes: Prefixes,
    /// tex.web § 1269 (`after_token`): the one token `\afterassignment`
    /// holds until the next assignment is complete.  A second
    /// `\afterassignment` before that assignment overwrites it.
    pub after_assignment: Option<Token>,
    /// tex.web § 389 (`long_state`): whether the macro whose arguments are
    /// being read was declared `\long`, and so may have `\par` in them.
    /// Outside a macro call nothing is restricted.
    long_argument: bool,
    /// Set when the argument scan ran away (tex.web § 396), which abandons the call.
    runaway: Runaway,
    /// Set when an argument scan was abandoned: the call did not get the
    /// argument it was written for, so a loop around it is not converging.
    scan_abandoned: bool,
    /// Where an unknown-text token stood for one character, and which.
    unknown_chars: HashMap<Span, char>,
    pub cds: Rc<Vec<ControlDep>>,
    /// The conditionals being read, innermost last (tex.web § 489).
    pub conds: Rc<Vec<CondFrame>>,
    /// For each conditional whose test is still being read, how many levels
    /// stood below it: its level's `if_limit` is `if_code` (tex.web § 489).
    pub testing: Vec<usize>,
    /// The `\currentiftype` of each conditional whose test is being read.
    pub testing_kind: Vec<Option<i8>>,
    /// Environments declared verbatim while the run went on, beside the ones
    /// the configuration names: `\newtcblisting` makes one.
    pub verbatim_environments: crate::fixed::HashSet<String>,
    /// The innermost sectioning unit seen so far.  The interpreter runs in
    /// document order, so the last `\section`-family occurrence is the one
    /// everything recorded after it stands in.
    section: Option<Rc<str>>,
    /// `\openin` streams: the lines each one still has to give.
    streams: Streams,
    /// What this run has written to its `\openout` files.
    pub(crate) outputs: Outputs,
    /// The `\usedir` of a docstrip batch file: where what it generates next
    /// belongs in a distribution's tree.
    pub docstrip_dir: Option<String>,
    /// Whether the docstrip commands are the ones satex models.
    docstrip: bool,
    /// Names whose modeled meaning stands whatever a file defines: interfaces
    /// satex implements itself because their own definition is written in a
    /// dialect it would have to typeset to follow.
    pinned: crate::fixed::HashSet<Sym>,
    /// The macro whose replacement text is currently being read.
    within: Rc<Vec<(Sym, NodeId)>>,
    pub(crate) quantity: crate::observe::Quantities,
    /// Whether something that is not a character was digested since the
    /// last one, so the next breaks the word (tex.web § 1034).
    pub(crate) text_broken: bool,
    /// The character ranges of the text each box register holds.
    pub(crate) box_chars: crate::fixed::HashMap<i64, Vec<(u32, u32)>>,
    /// The ranges of the box the last fetch took, until it is used.
    pub(crate) fetched: Vec<(u32, u32)>,
    /// Ranges of boxes used inside a box that is not placed yet.
    pub(crate) carried_chars: Vec<(u32, u32)>,
    /// What a shorthand definition's name meant before § 1224 made it
    /// `\relax` for the number scan, for [`Machine::define`].
    pub(crate) redefines_hint: Option<(Sym, (bool, bool))>,
    /// See [`crate::facts::Spot::apart`].
    pub(crate) text_apart: bool,
    /// The macro whose expansion the last token read came from. A list
    /// pushed or given back while it is processed keeps it.  `None`: a file.
    source: Option<Sym>,
    /// Package context: the name and the options it was given.
    packages: Rc<Vec<(Sym, Vec<String>)>>,
    /// `out.steps` when the outermost package load began.
    load_start: Option<u64>,
    /// How often each control sequence is currently being expanded.
    pub(crate) expanding: Vec<u16>,
    pub loaded: Rc<Vec<String>>,
    /// Options given to `\documentclass`, which packages also see.
    pub class_options: Vec<String>,
    /// The load whose `\Provides…` line has not been seen yet.
    identifying: Option<usize>,
    base: PathBuf,
    resolver: Resolver,
    pub(crate) file_depth: u16,
    /// Arms the running command takes at its choices: a run again
    /// ([`Machine::branch`]).
    script: Vec<usize>,
    /// The choices the running command met.
    choices: Vec<Choice>,
    /// Runs of the running command so far.
    runs: usize,
    /// Commands run under [`Machine::drive`], which can run them again.
    replayable: bool,
    /// What one run of [`Machine::drive`] is.
    pub(crate) unit: Unit,
    /// The innermost drive runs one path: a scan steps on through its
    /// tokens until one of them makes a choice.
    pub(crate) lone: bool,
    /// Expanding scans in flight, innermost last.
    pub(crate) scans: Vec<Accum>,
    /// The running command took an arm that does not exist.
    infeasible: bool,
    /// What the running command changed of the counters in [`Undo`].
    undo: Vec<Undo>,
    /// Paths waiting in the innermost drive, for progress reports.
    waiting: usize,
    /// A path ended while others went on: what the run does from then on
    /// happens only in some runs.
    parted: bool,
    /// The abstract number a scan just read from a register whose value
    /// is not known (a [`crate::value::Value::Num`]): what a test compares
    /// by interval, and an assignment stores.  With it, the register read
    /// as the whole value and a test on it narrows it on each arm.
    pub(crate) abs: Option<crate::value::Num>,
    /// [`Machine::abs`]'s exact members, while a join left it few of them
    /// (the set lift): `\advance`/`\multiply`/`\divide`, `\ifnum`/`\ifodd`
    /// and `\numexpr` apply the operation member by member before falling
    /// back to the interval. Always set or cleared together with `abs`.
    pub(crate) abs_exact: Option<Vec<i64>>,
    /// Whether the constant the last digit scan read is odd, when its
    /// value is unknown but its last digit is not (`?14` is even).
    pub(crate) abs_odd: Option<bool>,
    /// Whether the run said the dependency graph is full.
    graph_full_noted: bool,
    /// tex.web § 989 `output_active`: the output routine runs.
    pub(crate) output_active: bool,
    /// [`Machine::abs`] as the few intervals it is the hull of, when an
    /// expression left a set of them (`\numexpr` past its range is 0, not
    /// every number down to 0). Only the number scan that read it may
    /// leave it: [`Machine::scan_int`] clears it first.
    pub(crate) abs_parts: Option<Vec<crate::value::Num>>,
    /// The register that interval was read from, and what quantity it is.
    pub(crate) abs_from: Option<(Sym, crate::value::Quantity)>,
    pub(crate) abs_whole: bool,
    /// A unit's value over an interval of numbers (see `scan_units`).
    pub(crate) ranged: Option<crate::value::Num>,
    /// An undecided `\ifnum`/`\ifdim` of such a register against a known
    /// number: the register, its quantity, its interval, the relation and
    /// the bound, from which each arm learns.
    #[allow(clippy::type_complexity)]
    pub(crate) narrowing: Option<(Sym, crate::value::Quantity, crate::value::Num, Option<Vec<i64>>, char, (i64, i64))>,
    /// Where each diagnostic was first raised, by code, place and message
    /// hash, so that a repeat is counted and not stored again.
    diag_seen: crate::fixed::LookupMap<(&'static str, Span, u64), usize>,
    /// Diagnostics before this index belong to a finished cache segment and
    /// are not counted into.
    pub(crate) diag_floor: usize,
    pub(crate) halted: bool,
    /// Replaces `Config::max_steps` while the format is being read.
    budget: Option<u64>,
    /// Lines in `latex.ltx`, while it is being interpreted: the denominator
    /// for the progress bar `report_progress` draws. 0 outside a build.
    /// The step of the last undecided choice: only a loop that made one since
    /// it last read input can be stalled. Code that decides everything runs
    /// as TeX runs it, however long.
    /// Where the run last read further into the source than before.
    region: Region,
    guard: LineGuard,
    /// Rust-level scans in flight: the live input depth each began at, and
    /// whether a cut since dropped the text it began in.
    scan_starts: Vec<(usize, bool)>,
    /// A cut dropped the text a scan in flight began in: every reader but
    /// the main loop finds the input ended until it reads the file again.
    scan_cut: bool,
    /// What the conditional being widened skipped, to do what its arms
    /// may do ([`Machine::may_run`]).
    pub(crate) skip_log: Option<Vec<Token>>,
    /// Lines the kernel and package caches stood for instead of reading.
    pub(crate) cached_lines: u64,
    /// While the format builds: its file's input frame.
    format_progress: Option<usize>,
    /// Tokens the input stack currently holds, so that deep nesting of large
    /// replacement texts cannot grow without bound.
    held_tokens: usize,
    /// tex.web § 309 `align_state`: the explicit `{` read less the `}`,
    /// whatever reads them (§§ 347, 357) and less what goes back (§ 325).
    /// A cell's `&` or `\cr` read where it is as at the cell's start ends
    /// the cell there (§ 342): [`Machine::template_end_read`].
    pub(crate) align_state: i64,
    /// The `align_state` the cell being read began with, while its `v`
    /// part is still to come: where a read may end the cell.
    pub(crate) cell_base: Option<i64>,
    /// tex.web § 358: the token the last read took from behind a
    /// `\noexpand` marker (`frozen_dont_expand`), which the gullet then treats
    /// as `\relax` with `no_expand_flag`, however it is defined.
    pub(crate) noexpanded: Option<Token>,
    /// LuaTeX's `\lastnamedcs`: the name the last `\csname`,
    /// `\begincsname` or `\ifcsname` built.
    pub(crate) last_named_cs: Option<Sym>,
    /// What `observed_font_state` needs to stamp every character with its
    /// font without resolving the state again.
    pub(crate) font_watch: FontWatch,
    /// The marker `\noexpand` puts in front of the token it guards.
    pub(crate) dont_expand: Sym,
    /// A token standing for text satex cannot know, such as what
    /// `\pdfuniformdeviate` expands to: it has an unknown meaning, so a
    /// value scanned from it is unknown, and so is text built with it.
    pub(crate) unknown: Sym,
    /// Where an undelimited parameter read unknown text: the scan's place
    /// in the input is not known from there on, so the next delimited scan
    /// starts fuzzy (see [`Machine::read_delimited`]).
    scan_fuzzy: Option<Span>,
    /// The last delimited scan ended at its delimiter, read from the input.
    delimiter_met: bool,
    /// A scan of this call's arguments took the arm where unknown text
    /// holds no delimiter: a runaway after it refutes that arm.
    arm_assumed: bool,
    /// Recorded runs per call ([`calls`]).
    call_runs: HashMap<calls::CallKey, calls::CallRuns>,
    /// Calls being recorded, innermost last.
    call_recs: Rc<Vec<calls::CallRec>>,
    guard_seen: GuardSeen,
    /// This path ran `\end` (tex.web § 1054), not off the end of its input.
    pub(crate) job_ended: bool,
    /// The step count the budget was last checked at. A distance, not a
    /// multiple: a read can step onto a multiple the charge then passes.
    checked_at: u64,
    /// The cacheable package or class being loaded: its end forgets what a
    /// replay of it would not bring ([`Machine::end_cacheable_load`]).
    cacheable_load: Option<FileId>,
    /// Digits satex does not know, such as `\the\count0` of an unknown
    /// count: unknown text to a scan, but character tokens of category 12
    /// to `\ifx`, `\if`, `\ifcat`, `\let` and `\futurelet` (their
    /// meaning is [`UNKNOWN_CHAR`](crate::tex::UNKNOWN_CHAR)).
    pub(crate) unknown_digits: Sym,
    /// Unknown digits that may be none: what is left of unknown digits
    /// once one of them was taken as a token.
    pub(crate) unknown_more: Sym,
    /// The names of typed unknown text ([`Machine::unknown_text`]), one
    /// per class and lower bound, once made: widening at a loop head names
    /// them without the interner.
    typed_syms: Option<Rc<[Sym]>>,
    /// The `edef_depth` of the expansion that makes a definition's
    /// replacement text: what `\the` and `\unexpanded` give it is stored as
    /// it stands (tex.web § 478), so a `#` among it is no parameter.
    def_body_depth: Option<usize>,
    /// Fact index by source site, so that re-analyzing a file does not
    /// duplicate what it observed.
    definition_sites: crate::fixed::LookupMap<(Span, Sym), usize>,
    expansion_sites: crate::fixed::LookupMap<(Span, Sym), Site>,
    /// Hashes of input lists' pending heads, by list and position: a deep
    /// input below a call is hashed once, not at every call.
    shape_memo: std::cell::RefCell<Vec<ShapeSlot>>,
    /// Each evaluated conditional's fact, by its `\if…`.
    cond_sites: crate::fixed::LookupMap<Span, usize>,
    /// Tokens taken from a file so far: the measure of progress through the
    /// source that tells a loop from a helper called again further down.
    source_progress: u64,
    /// Of those, lines read by `\read`: a path's place in the source
    /// leaves them out, as it does the stream position ([`Streams`]).
    lines_read: u64,
    /// The call being described right now: a control sequence taken
    /// straight from a file, where it stood, and the arguments read since
    /// from that same file. Arguments a macro's body reads from itself
    /// don't count, so `\section*[short]{title}` comes out of `\@ifstar`
    /// and friends without naming them.
    call_shape: Option<Rc<(Sym, Span, String, usize)>>,
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
    pub(crate) gaps: HashSet<(&'static str, Sym)>,
    /// Whether the current expansion is building a PDF string, where even a
    /// robust command is expanded: that is why hyperref needs
    /// `\texorpdfstring` at all (hyperref manual, "Special characters").
    pdf_string: bool,
    /// The file the token just read came from, or `None` when it came from a
    /// macro body.
    pub(crate) last_file: Option<FileId>,
    /// Tokens the document's run has read: what `--steps` counts, the
    /// first token being step 0.
    read: u64,
    /// Files code that may have run (a widened conditional's arms) could
    /// have ended with `\endinput`: what they hold after is not certain.
    may_have_ended: HashSet<FileId>,
    /// Where the file tokens given back and not yet read again stood.
    recent_file_spans: Vec<Span>,
    /// The last control sequence taken straight from a file, and where it
    /// stood: the call everything its expansion does is part of.
    pub file_call: Option<(Sym, Span)>,
    /// The last control sequence run whose token stands in a project file,
    /// a macro body of the project's included: where its own text put what
    /// the call `file_call` does.
    project_site: Option<Span>,
    /// The name `\csname` built last, at the text its name came from: no
    /// command the document wrote there.
    pub(crate) built_name: Option<(Sym, Span)>,
    /// The file call each open file was pushed under: when the file ends,
    /// the call that read it is the file call again.
    file_calls: Vec<(FileId, Option<(Sym, Span)>)>,
    /// The lines `\write`s handed files and the names body calls read,
    /// related once the run ends (`observe::relate`).
    pub written_lines: Vec<crate::observe::WrittenLine>,
    pub name_reads: Vec<crate::observe::NameRead>,
    /// A copy of the kernel state at `\begin{document}` that written lines
    /// run in. `Some(None)` once none could be made.
    pub line_sandbox: Option<Option<Box<Machine<'a>>>>,
    /// In that copy: the names the running line defined.
    pub line_defined: Option<Vec<Sym>>,
    /// The call whose argument parts [`crate::observe::defined_name`] last
    /// computed, and those parts.
    pub call_parts: Option<(Span, Span, std::rc::Rc<[String]>)>,
    /// Where [`Machine::read_group_body`] collects a body, kept between
    /// calls so a long body is not grown anew each time.
    body: Vec<Token>,
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
    /// The token the last `get_r_token` read: the name a definition makes.
    name_token: Option<Token>,
    /// [`Machine::project_file`] per file: 0 not asked, 1 no, 2 yes.
    own_files: Vec<u8>,
    /// The group depth and the open conditionals when the run last read from
    /// a file: the state a cut rolls back to.
    /// How many gaps (`Severity::Unsupported`) the run has raised, and the
    /// shallowest `edef_depth` one was raised at since an expansion gave up
    /// over it. An expansion that hit a gap cannot stand behind the text it
    /// assembled, since that's only what satex got past the gap, not what
    /// the engine would make, so unknown text goes back in its place.
    pub(crate) gaps_raised: u64,
    /// tex.web § 82 `error_count`: the engine's errors since the last
    /// paragraph ended. At [`MAX_ERRORS`] the run stops as TeX does.
    pub(crate) tex_errors: u32,
    /// `analysis.join_paths_at`, resolved: commands, whose meaning joins
    /// when it runs, and the code macros of hooks.
    join_commands: Vec<Sym>,
    join_hooks: Vec<Sym>,
    /// The running path started a join point ([`Machine::join_commands`]):
    /// it waits there for the others.
    at_join: bool,
    /// The conditional the running path closed last, by its `\if…` and the
    /// stack depth below it: paths that closed the same one and stand at
    /// one place join there whatever else tells them apart.
    pub(crate) fi_mark: Option<(Span, usize)>,
    /// A certain TeX error the running command raised: under
    /// `analysis.assume_compiles` its path is no run of the document.
    pub(crate) dropping: Option<String>,
    /// `dropping` is an error reached only past an approximation, which
    /// may not be TeX's ([`Machine::certain_error`]).
    pub(crate) dropping_possible: bool,
    /// `dropping` is a possible engine error whose recovery breaks the
    /// code's brackets ([`BREAKING`]): its path is contained, not run on.
    pub(crate) dropping_breaks: bool,
    /// Paths with certain errors run on: no path compiles
    /// ([`Machine::settle`]).
    pub(crate) keep_errors: bool,
    /// The running path approximated since the paths parted: its errors
    /// are not certain.
    pub(crate) approximated: bool,
    /// Set by a cut ([`crate::machine::explore`]): the input depth of the
    /// file under the token lists the cut left after its `\unknown`. Those
    /// lists were arguments the dropped code may have taken, so what TeX
    /// does with them is not known until the file is read again.
    pub(crate) cut_taint: Option<usize>,
    gap_depth: Option<usize>,
    started: std::time::Instant,
    /// The date-time the run compiles at (`clock:` in `satex.yaml`), or
    /// `None` when the date is left unknown.
    pub(crate) clock: Option<time::OffsetDateTime>,
    /// Where the clock was first read, if it was: said once as an
    /// assumption when the run ends ([`Machine::say_clock`]).
    clock_read_at: Option<Span>,
    /// A replayed package cache that read the clock, named in that
    /// assumption: its date is the one of the run that made the cache.
    pub(crate) clock_cached: Option<String>,
    stubs: Option<Rc<HashMap<Sym, crate::config::Stub>>>,
    stub_said: HashSet<Sym>,
    /// The modes the run may be in, and those of the list the current
    /// paragraph belongs to (tex.web § 211).
    pub(crate) mode: crate::mode::Modes,
    /// The vertical command a `\par` was last put in front of (tex.web
    /// § 1094), while horizontal mode lasts.
    pub(crate) par_inserted: Option<Span>,
    pub(crate) list: crate::mode::Modes,
    /// Whether anything may have been put on a list: until then e-TeX's
    /// `\lastnodetype` of the empty main vertical list is -1.
    pub(crate) material: bool,
    /// Whether a box or a rule may have reached the main vertical list, so
    /// that the page builder has frozen the page dimensions (tex.web
    /// §§ 987, 1000).  Until then `\pagegoal` reads `\maxdimen` and the
    /// other seven page dimensions read zero.
    pub(crate) page_touched: bool,
    /// Whether material reached the page since the page builder last looked.
    pub(crate) page_grew: bool,
    /// The natural size of the current list, where known.
    pub(crate) natural: Option<crate::mode::Natural>,
    /// The last item put on the main vertical list, where known: what
    /// `\lastskip` and `\lastkern` read there, whether the page builder has
    /// taken it or not (tex.web §§ 424, 996: `last_glue` and `last_kern` are
    /// the last item it moved, and an output routine leaves the items from
    /// the break on, the last one among them, after its own).
    pub(crate) main_tail: crate::mode::Last,
    /// The names a definition of a name built from unknown text may have
    /// made: one that seems undefined and matches may be defined.
    pub(crate) wild: Wild,
    /// How deeply `step` is nested on the Rust stack.
    step_depth: u32,
    /// Where on the stack the outermost `step` runs.
    stack_base: usize,
    pub(crate) scanner: Scanner,
    /// What a `\scantokens` pseudo file is expanded as, so that its end
    /// can be told (see [`Machine::bump_expanding`]).
    pub(crate) pseudo_file: Sym,
    /// Set in a sandbox run by [`crate::probe`]: what it watches for.
    pub(crate) probe: Option<Box<crate::probe::Watch>>,
    /// Where this run keeps its checkpoints, if anywhere ([`Checkpoints`]).
    checkpoints: checkpoint::Taker<'a>,
}

/// Everything besides the bindings that a path through an undecided
/// conditional changes, so each arm starts where the test was. What the run
/// records (facts, graph, hooks, loaded files) is shared: analysis reports
/// the union of the paths.
#[derive(Clone)]
struct PathState {
    input: Input,
    catcodes: CatcodeTable,
    end_line: EndLineChar,
    prefixes: Prefixes,
    after_assignment: Option<Token>,
    long_argument: bool,
    runaway: Runaway,
    cds: Rc<Vec<ControlDep>>,
    conds: Rc<Vec<CondFrame>>,
    within: Rc<Vec<(Sym, NodeId)>>,
    packages: Rc<Vec<(Sym, Vec<String>)>>,
    held_tokens: usize,
    align_state: i64,
    cell_base: Option<i64>,
    section: Option<Rc<str>>,
    call_shape: Option<Rc<(Sym, Span, String, usize)>>,
    /// The loops this path continues the summary of: never dropped as
    /// their re-entry.
    pub(crate) summary_of: Rc<Vec<LoopId>>,
    /// Per loop, the bounded text this path's last pass began with
    /// ([`Machine::input_bound`]).
    pub(crate) loop_bounds: Rc<Vec<(LoopId, u32)>>,
    edef_depth: usize,
    pdf_string: bool,
    last_file: Option<FileId>,
    file_call: Option<(Sym, Span)>,
    source_progress: u64,
    lines_read: u64,
    file_depth: u16,
    pub(crate) halted: bool,
    streams: Streams,
    outputs: Outputs,
    loaded: Rc<Vec<String>>,
    mode: crate::mode::Modes,
    list: crate::mode::Modes,
    material: bool,
    page_touched: bool,
    page_grew: bool,
    output_active: bool,
    natural: Option<crate::mode::Natural>,
    main_tail: crate::mode::Last,
    wild: Wild,
    quantity: crate::observe::Quantities,
    noexpanded: Option<Token>,
    last_named_cs: Option<Sym>,
    source: Option<Sym>,
    identifying: Option<usize>,
    recent_file_spans: Vec<Span>,
    file_calls: Vec<(FileId, Option<(Sym, Span)>)>,
    reader: Option<NodeId>,
    scans: Vec<Accum>,
    at_join: bool,
    fi_mark: Option<(Span, usize)>,
    approximated: bool,
    cut_taint: Option<usize>,
    call_recs: Rc<Vec<calls::CallRec>>,
    guard_seen: GuardSeen,
    job_ended: bool,
}

/// Where a path stands in the input, for telling when two paths meet: a
/// file by position, a token list by what is left to read in it. Two
/// expansions of one macro are separate lists but the same continuation.
#[derive(Debug, Clone)]
enum Place {
    File(FileId, usize, bool),
    Tokens(Rc<[Token]>, usize),
    Pseudo(Span, Rc<[char]>, usize),
    Returned(Vec<Token>),
    Boundary,
}

impl PartialEq for Place {
    fn eq(&self, other: &Place) -> bool {
        match (self, other) {
            (Place::File(f, i, e), Place::File(g, j, d)) => (f, i, e) == (g, j, d),
            (Place::Tokens(a, i), Place::Tokens(b, j)) => (Rc::ptr_eq(a, b) && i == j) || a[*i..] == b[*j..],
            (Place::Pseudo(_, a, i), Place::Pseudo(_, b, j)) => Rc::ptr_eq(a, b) && i == j,
            (Place::Returned(a), Place::Returned(b)) => a == b,
            (Place::Boundary, Place::Boundary) => true,
            _ => false,
        }
    }
}

/// What one call site has done so far.
#[derive(Clone, Copy)]
struct Site {
    fact: usize,
    /// Expansions since the last one that read something new from a file.
    repeats: u32,
    progress: u64,
    /// How many tokens the last expansion here was handed.  A recursion
    /// that does not make this smaller has no measure that is running out.
    size: usize,
    /// The source position of the last expansion here, and how many have
    /// happened since it moved.  Unlike `repeats` an assignment does not
    /// reset this, so a recursion that only assigns cannot hide from it.
    source: u64,
    unread: u32,
    /// The arguments and the input the last expansion here had, hashed,
    /// with how many input levels that input stood on and its conditional
    /// and group depth.
    shape: u64,
    levels: usize,
    nest: (usize, usize),
    /// How many scans the last expansion here was nested in: one met by
    /// a scan the last one began (`\def\a{\number\a}`) is a level deeper
    /// in TeX's own recursion (tex.web § 366 `expand`), never the state
    /// that one was in.
    scans: u32,
    /// Expansions in a row that had that shape, each nested deeper: in
    /// conditionals or groups, or on the input stack, with that input
    /// still below.
    deeper: u32,
    /// [`Env::effective`] at the last expansion here, and how many in a
    /// row met the same shape, nesting and bindings.
    effective: u64,
    cycled: u32,
    /// [`Env::assignments`] when the recursion through this site began:
    /// a cut widens only what was written after it.
    recent: u64,
}

impl Site {
    /// The site as a first expansion finds it.
    fn fresh(fact: usize) -> Site {
        Site {
            fact,
            repeats: 0,
            progress: u64::MAX,
            size: usize::MAX,
            source: u64::MAX,
            unread: 0,
            shape: 0,
            levels: usize::MAX,
            nest: (0, 0),
            scans: 0,
            deeper: 0,
            effective: u64::MAX,
            cycled: 0,
            recent: 0,
        }
    }
}

/// A counter that decides when the run gives up, as the running command
/// found it: running the command again starts from it ([`Machine::branch`]).
#[derive(Clone)]
enum Undo {
    Site((Span, Sym), Site),
    Count(usize, u32),
}

/// The most categories the reader splits a path on, one per path; past
/// them the character stands for unknown text.
const SPLIT_CATS: u32 = 4;

/// Gaps the repeated-failure cut leaves alone: the ones it would raise
/// itself, the ones that already end the run, unknown text that ran, and
/// a summary's own assumption that a loop closed its levels. A path always
/// gets past those, and a loop around them is bounded by its summary, so
/// cutting there only drops the callers' code that follows.
const CUT_EXEMPT: [&str; 8] = [
    "repeated-failure",
    "budget-exhausted",
    "limit-reached",
    "unknown-text-run",
    "unbalanced-path",
    "line-budget-cut",
    "region-skipped",
    "region-catcodes",
];

/// [`LineGuard`]'s line and fork charges, held over a cacheable load.
type HeldCharges = (HashMap<GuardKey, (u64, u8)>, HashMap<(GuardKey, Span), u32>);

/// The termination guard: the steps spent while the run reads each source
/// line, and how often that line overran `limits.line_steps`.
#[derive(Clone, Default)]
struct LineGuard {
    /// The step the last check charged up to.
    step: u64,
    lines: HashMap<GuardKey, (u64, u8)>,
    /// The undecided choices made at each site while the run read each
    /// source line, over all paths ([`Machine::hot_site`]).
    forks: HashMap<(GuardKey, Span), u32>,
    /// `lines` and `forks` before the cacheable load under way: a replay
    /// charges none of its lines, so a read forgets them too
    /// ([`Machine::end_cacheable_load`]).
    held: HeldCharges,
    /// The most one line took, for `-vvvv`.
    peak: u64,
}

/// A path's view of the guard: the line it stands on, the overruns of it
/// it has taken (a path parked before a cut takes it when it runs), and
/// whether its cut is still unwinding to the main loop.
/// The line a step is charged to: the source line, and the line `\read`
/// last took (stream, line), so a loop over a data file gets a budget per
/// data line; a finite file still gives finitely many keys.
type GuardKey = (FileId, u32, i64, usize);

#[derive(Clone, Copy, Default)]
struct GuardSeen {
    line: GuardKey,
    level: u8,
    cut: bool,
}

/// The stretch since the run last read source further than before: where
/// a guard cut reads on, and the categories it puts back.
#[derive(Clone, Default)]
struct Region {
    progress: u64,
    span: Span,
    /// Characters whose category changed since the last source token, with
    /// the category they had then.
    catcodes: Vec<(char, Catcode, u16)>,
}

/// Gaps the repeated-failure cut takes although they are satex's own
/// imprecision, not a fault of the document (`argument-ends-in-unknown`: a
/// scan gave up where unknown text may have opened the group). A path that
/// meets it over and over is as stuck as one meeting a real error, so it is
/// given up the same way.
const CUT_IMPRECISE: [&str; 1] = ["argument-ends-in-unknown"];

/// Gaps meaning satex lost the thread of the text it was assembling, as
/// against the engine's own error faithfully reproduced. A `\csname` hitting
/// an unexpandable token is the engine's error with defined recovery
/// (tex.web § 372: `\endcsname` is inserted), so satex knows what follows
/// and the assembled text matches the engine's. A scan that gave up, a file
/// ending inside a group, a widened loop: there satex doesn't know what the
/// engine would have read, so the fragment assembled is its own and unknown
/// text goes back instead.
/// The engine's own errors, each with the recovery tex.web defines: the
/// run goes on exactly as TeX does, and never widens for them. Each is
/// raised as the error TeX prints (`raised-error`) and counts toward
/// [`MAX_ERRORS`] (tex.web § 82).
pub(crate) const TEX_ERRORS: [(&str, Option<&str>); 14] = [
    ("missing-endcsname", Some("Missing \\endcsname inserted")),
    ("missing-sharp", Some("Missing # inserted in alignment preamble")),
    ("extra-sharp", Some("Only one # is allowed per tab")),
    ("extra-alignment-tab", Some("Extra alignment tab has been changed to \\cr")),
    ("misplaced-tab", Some("Misplaced alignment tab character &")),
    ("extra-endcsname", Some("Extra \\endcsname")),
    ("extra-else", Some("Extra \\else")),
    ("extra-fi", Some("Extra \\fi")),
    ("too-many-braces", Some("Too many }'s")),
    ("script-outside-math", Some("Missing $ inserted")),
    ("illegal-unit", None),
    ("missing-number", Some("Missing number, treated as zero")),
    // Named after the macro in the message itself.
    ("extra-right-brace", None),
    ("runaway-argument", None),
];

/// The files of the tokens a token list still holds, `None` for a file.
fn list_files(frame: &Frame) -> Option<Vec<FileId>> {
    match frame {
        Frame::Tokens { toks, pos, .. } => Some(toks[(*pos).min(toks.len())..].iter().map(|t| t.span.file).collect()),
        Frame::Returned(t) => Some(t.iter().map(|t| t.span.file).collect()),
        _ => None,
    }
}

/// Engine errors whose recovery drops or inserts brackets (§§ 395-396,
/// 1069, 510, 1135): the code after one reads text it never meant to.
/// So does a misread number ([`Machine::misread`]).
pub(crate) const BREAKING: [&str; 6] =
    ["extra-right-brace", "runaway-argument", "too-many-braces", "extra-fi", "extra-else", "extra-endcsname"];

/// Errors and stops of the engine that satex reports as warnings of its
/// own, not through [`TEX_ERRORS`].
const ENGINE_WARNINGS: [&str; 5] =
    ["mismatched-group", "file-ended", "wrong-mode", "arithmetic-overflow", "endless-loop"];

/// tex.web § 82: after this many errors in one paragraph TeX gives up
/// ("That makes 100 errors; please try again").
const MAX_ERRORS: u32 = 100;

const WIDEN_ON: [&str; 6] = [
    "argument-ends-in-unknown",
    "unbalanced-conditional",
    "unbalanced-file",
    "unbalanced-group",
    "unterminated-conditional",
    "repeated-failure",
];

impl<'a> Machine<'a> {
    pub fn new(cfg: &'a Config, base: PathBuf) -> Self {
        let project = Project::discover(&base);
        let plugins = Plugins::resolve(cfg, &project);
        // TEXINPUTS etc., in kpathsea's own precedence: environment, then
        // the project's `latexmkrc`/Makefile, then `satex.yaml`'s `paths.*`.
        let kpse_paths = crate::paths::effective_all(&project, &cfg.paths, &project.root);
        let texinputs =
            kpse_paths.iter().find(|e| e.variable == "TEXINPUTS").map(|e| e.dirs.clone()).unwrap_or_default();
        let search = cfg.search_paths.iter().cloned().chain(project.search_paths()).chain(texinputs).collect();
        let mut request = cfg.distribution_request();
        request.kpse_env = crate::paths::env_vars(&kpse_paths);
        let output = crate::paths::output_dir(&project, &cfg.paths.output_directory, &project.root).map(|o| o.dir);
        let resolver = Resolver::new(search, request).with_output(output).with_unpacked(project.unpacked());
        let engine = plugins.engine;
        let mut interner = Interner::default();
        // Sym(0) is the empty name, used where an event has no subject.
        interner.intern("");
        // No file can spell this name with the catcodes a run starts with.
        let dont_expand = interner.intern("\u{2}notexpanded:");
        let unknown = interner.intern(crate::tex::UNKNOWN_CS);
        let unknown_digits = interner.intern("\u{2}unknown digits:");
        let unknown_more = interner.intern("\u{2}unknown more digits:");
        let pseudo_file = interner.intern("\u{2}pseudo file:");
        let table = kernel_meanings(&mut interner, engine);
        let mut env = Env::with_capacity(interner.len());
        for (sym, meaning) in table {
            env.set(sym, Binding::builtin(meaning), true);
        }
        set_primitive_aliases(&mut env, &mut interner, engine);
        env.set(unknown, Binding::builtin(Meaning::Unknown), true);
        env.set(unknown_digits, Binding::builtin(Meaning::Char(crate::tex::UNKNOWN_CHAR, Catcode::Other)), true);
        env.set(unknown_more, Binding::builtin(Meaning::Char(crate::tex::UNKNOWN_CHAR, Catcode::Other)), true);
        // No format yet: INITEX's table. A format installs its own.
        let mut catcodes = CatcodeTable::initex();
        catcodes.at_letter(cfg.at_letter);
        Self {
            cfg,
            carried: Carried::default(),
            scratch: Scratch::default(),
            limits: cfg.limits.clone(),
            pressure: 0,
            hyph_cache: None,
            kp: Default::default(),
            degraded_at: None,
            out: Analysis {
                config: cfg.source.clone(),
                settings: cfg.clone(),
                metadata: Vec::new(),
                kernel_sites: HashMap::default(),
                interner,
                graph: DependencyGraph::with_cap(cfg.limits.vertices),
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
                    bundle: None,
                },
                document_depth: None,
                preamble_cost: None,
                justified_text: 0,
                entry: Vec::new(),
                format: None,
                fonts: Vec::new(),
                font_states: vec![crate::facts::FontState::default()],
                env: Env::default(),
                catcodes: CatcodeTable::latex(),
                timings: Timings::new(cfg.timings),
                facts: Facts::default(),
                trace: Vec::new(),
                typeset: String::new(),
                steps: 0,
                load_steps: 0,
                limits: Vec::new(),
                lines: 0,
                exhausted: false,
                journal_size: 0,
                held_tokens_left: 0,
                loop_heads: 0,
                peak_paths: 0,
                forks: 0,
                hot: HOT.load(std::sync::atomic::Ordering::Relaxed).then(Default::default),
                joined_slots: 0,
                memory_reported: 0,
                untyped: 0,
                package_caches: Vec::new(),
                class_caches: Default::default(),
                cached_files: Default::default(),
                preamble: None,
                kernel: None,
                probes: Default::default(),
                env_stack: Vec::new(),
                context_probes: Default::default(),
                takes: Default::default(),
                bib: Default::default(),
            },
            env,
            catcodes,
            end_line: EndLineChar::default(),
            input: Input::default(),
            prefixes: Prefixes::default(),
            after_assignment: None,
            long_argument: true,
            runaway: Runaway::No,
            scan_abandoned: false,
            scan_fuzzy: None,
            delimiter_met: false,
            arm_assumed: false,
            unknown_chars: HashMap::default(),
            cds: Rc::default(),
            within: Rc::default(),
            quantity: crate::observe::Quantities::default(),
            text_broken: true,
            box_chars: Default::default(),
            fetched: Vec::new(),
            carried_chars: Vec::new(),
            redefines_hint: None,
            text_apart: false,
            source: None,
            packages: Rc::default(),
            load_start: None,
            expanding: Vec::new(),
            loaded: Rc::default(),
            class_options: Vec::new(),
            identifying: None,
            resolver,
            base,
            file_depth: 0,
            script: Vec::new(),
            choices: Vec::new(),
            runs: 0,
            replayable: false,
            unit: Unit::Command,
            lone: false,
            scans: Vec::new(),
            infeasible: false,
            undo: Vec::new(),
            waiting: 0,
            parted: false,
            abs: None,
            abs_exact: None,
            abs_odd: None,
            graph_full_noted: false,
            output_active: false,
            abs_parts: None,
            abs_from: None,
            abs_whole: false,
            ranged: None,
            narrowing: None,
            diag_seen: Default::default(),
            diag_floor: 0,
            conds: Rc::default(),
            testing: Vec::new(),
            testing_kind: Vec::new(),
            verbatim_environments: crate::fixed::HashSet::default(),
            section: None,
            streams: Rc::default(),
            outputs: Rc::default(),
            docstrip_dir: None,
            docstrip: false,
            lua: Default::default(),
            pinned: crate::fixed::HashSet::default(),
            halted: false,
            budget: None,
            format_progress: None,
            cached_lines: 0,
            held_tokens: 0,
            align_state: 0,
            cell_base: None,
            noexpanded: None,
            last_named_cs: None,
            font_watch: FontWatch::default(),
            dont_expand,
            unknown,
            call_runs: HashMap::default(),
            call_recs: Rc::default(),
            guard_seen: GuardSeen::default(),
            job_ended: false,
            checked_at: 0,
            unknown_digits,
            unknown_more,
            typed_syms: None,
            def_body_depth: None,
            definition_sites: Default::default(),
            expansion_sites: Default::default(),
            shape_memo: std::cell::RefCell::new(vec![None; SHAPE_MEMO]),
            cond_sites: Default::default(),
            source_progress: 0,
            lines_read: 0,
            call_shape: None,
            edef_depth: 0,
            copy_target: None,
            copies: Vec::new(),
            scanning: 0,
            scan_undecided: false,
            cacheable_load: None,
            gaps: HashSet::default(),
            pdf_string: false,
            last_file: None,
            read: 0,
            may_have_ended: HashSet::default(),
            recent_file_spans: Vec::new(),
            file_call: None,
            project_site: None,
            built_name: None,
            file_calls: Vec::new(),
            written_lines: Vec::new(),
            name_reads: Vec::new(),
            line_sandbox: None,
            line_defined: None,
            stream_files: HashMap::default(),
            call_parts: None,
            body: Vec::new(),
            key_names: Vec::new(),
            counter_namespace: None,
            held: Vec::new(),
            command_level: false,
            package_capture: None,
            document_file_depth: 0,
            popping_in_read: false,
            reading_command: false,
            name_token: None,
            own_files: Vec::new(),
            gaps_raised: 0,
            tex_errors: 0,
            join_commands: Vec::new(),
            join_hooks: Vec::new(),
            at_join: false,
            fi_mark: None,
            dropping: None,
            dropping_possible: false,
            dropping_breaks: false,
            keep_errors: false,
            approximated: false,
            cut_taint: None,
            gap_depth: None,
            started: std::time::Instant::now(),
            region: Region::default(),
            guard: LineGuard::default(),
            scan_starts: Vec::new(),
            scan_cut: false,
            skip_log: None,
            clock: None,
            clock_read_at: None,
            clock_cached: None,
            stubs: None,
            stub_said: HashSet::default(),
            mode: crate::mode::Modes::VERTICAL,
            par_inserted: None,
            list: crate::mode::Modes::VERTICAL,
            step_depth: 0,
            stack_base: 0,
            scanner: Scanner::Normal,
            pseudo_file,
            probe: None,
            material: false,
            page_touched: false,
            page_grew: false,
            natural: None,
            main_tail: crate::mode::Last::UNKNOWN,
            wild: Wild::default(),
            trace_full: false,
            checkpoints: Default::default(),
        }
    }

    /// [`Machine::run`], where a bug in satex ends the run with a gap
    /// instead of a crash: what was read so far is still reported.
    fn run_guarded(&mut self) {
        guard_panics();
        // Nested runs (package caches) restore the outer run's guard.
        let outer = PANIC_AT.with(|cell| cell.replace(Some(String::new())));
        let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.run()));
        let at = PANIC_AT.with(|cell| cell.replace(outer)).unwrap_or_default();
        let Err(payload) = run else { return };
        let what = payload
            .downcast_ref::<&str>()
            .map(|s| s.to_string())
            .or_else(|| payload.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        self.out.exhausted = true;
        let span = self.position_span();
        self.diagnose(
            Severity::Unsupported,
            "internal-error",
            span,
            format!("satex hit a bug here and stopped ({what} at {at}); please report it"),
        );
    }

    /// [`Machine::analyze_once`], and for a package or class that uses
    /// names it never defines, once more after what the packages on disk
    /// say it expects loaded (`load.expect_detect`), kept when that run
    /// defines more of them.
    pub fn analyze(source: &str, path: Option<&Path>, cfg: &'a Config) -> Analysis {
        let first = Machine::analyze_once(source, path, cfg);
        let own = matches!(
            crate::config::Profile::of_name(path, source),
            Some(crate::config::Profile::Package | crate::config::Profile::Class)
        );
        if !own || !cfg.load.expect.is_empty() || !cfg.load.expect_detect {
            return first;
        }
        let missing = |a: &Analysis| -> Vec<String> {
            let mut names: Vec<String> = a
                .facts
                .expansions
                .iter()
                .filter(|e| e.meaning == MeaningKind::Undefined && e.cds.is_empty() && e.span.file == a.main_file)
                .map(|e| a.interner.name(e.name).to_string())
                .collect();
            names.sort();
            names.dedup();
            names
        };
        let names = missing(&first);
        if names.is_empty() {
            return first;
        }
        let cache = cfg.cache.enabled.then(|| cfg.cache.dir.clone().or_else(format::default_cache_dir)).flatten();
        let exports = crate::exports::Exports::of(
            &first.distribution.roots,
            cache.as_deref(),
            &crate::exports::definers(&first, &cfg.conventions.definers),
        );
        for (file, _) in exports.providers(&names).into_iter().take(3) {
            let name = file.trim_end_matches(".sty").trim_end_matches(".cls").to_string();
            let mut with = cfg.clone();
            with.load.expect = vec![name.clone()];
            let mut second = Machine::analyze_once(source, path, &with);
            let left = missing(&second).len();
            if left >= names.len() {
                continue;
            }
            let message = format!(
                "read after {name}, which defines {} of the {} names this file uses without defining them; \
                 set `load.expect: [{name}]` to say so, or `load.expect_detect: false`",
                names.len() - left,
                names.len()
            );
            second.facts.diagnostics.push(crate::facts::Diagnostic {
                severity: Severity::Assumption,
                code: "expected-load",
                message,
                span: Span { file: second.main_file, line: 1, col: 1 },
                count: 1,
                origin: None,
                after: 0,
            });
            return second;
        }
        first
    }

    pub fn analyze_once(source: &str, path: Option<&Path>, cfg: &'a Config) -> Analysis {
        // Memory is counted per run, apart from runs beside it in the process.
        let (_run, _entered) = crate::budget::Run::start();
        let (mut m, id) = Machine::begin(source, path, cfg);
        m.run_guarded();
        m.conclude(id)
    }

    /// A run of `source` set up to read it: the format installed, the
    /// file's frame pushed, `\everyjob` run.
    fn begin(source: &str, path: Option<&Path>, cfg: &'a Config) -> (Self, FileId) {
        // `-f doc.tex` has an empty parent, not a missing one, so the
        // filter is what keeps a bare name rooted at the working directory.
        let base = path
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let mut m = Machine::new(cfg, base);
        let engine = m.out.plugins.engine;
        // A `.dtx` is documentation with code in it; docstrip decides which
        // is which. satex interprets the code, line for line where it
        // stands in the `.dtx`. A `.ins` is a batch file and runs as written.
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
            m.out.plugins.discovery = crate::plugin::discovery::scan_with(&m.base, m.cfg.limits.thread_count());
        }
        m.out.plugins.read_build(m.cfg, &m.out.project, path);
        if m.cfg.plugins.magic {
            m.out.plugins.read_magic(source);
        }
        m.out.plugins.read_shell_escape(m.cfg, &m.out.project);
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
            set_primitive_aliases(&mut m.env, &mut m.out.interner, m.out.plugins.engine);
        }
        // The engine is settled: every source read from here on, the kernel
        // and the document included, is decoded the way it reads a file.
        crate::source::set_encoding(m.out.plugins.engine.encoding());
        let code = crate::source::recode(code);
        let code = code.as_ref();
        if m.cfg.plugins.preload {
            let project = m.out.project.clone();
            let base = m.base.clone();
            let configured = m.cfg.preload.clone();
            m.out.plugins.read_preload(source, &project, &base, configured.as_deref());
        }
        m.install_parameters();
        m.install_clock();
        m.install_format();
        m.install_clock();
        let limits = &m.cfg.limits;
        m.env.sets = crate::env::SetLimits {
            meanings: usize::from(limits.meaning_set).max(1),
            arms: usize::from(limits.join_arms),
            values: usize::from(limits.value_set).max(1),
            intervals: usize::from(limits.interval_set).max(1),
        };
        // What the kernel wrote (latex.ltx's `texsys.aux` holds the clock) is the
        // format build's, not the document's: kept, it made every package cache
        // root differ from one minute to the next.
        m.outputs = Rc::default();
        // Building the kernel is its own phase and happens once, so the
        // document's time budget starts once the format is in place. A cold
        // run charging it to the document reported a `budget-exhausted` gap
        // a warm run never had.
        m.started = std::time::Instant::now();
        // The engine's `-interaction` overrides what the format dumped
        // (tex.web § 1337).
        let mode = m.intern("interactionmode");
        m.env.set_value(mode, crate::value::Value::Int(m.cfg.interaction as i64), true);
        m.install_output();
        m.out.kernel = Some(Box::new(m.env.snapshot()));
        if literate == Some(crate::literate::Literate::Ins) {
            m.install_docstrip();
        }
        m.out.timings.start(Phase::Document);
        m.document_file_depth = m.file_depth;
        m.out.timings.enter_file(id);
        m.input.push(Frame::File { mouth: Mouth::file(code, id), package: None, depth: 0, conds: 0, eof_seen: false });
        m.push_expected(kind);
        // tex.web § 1337: `\everyjob` is read first, after the format is in
        // place: what it sets from the clock (expl3's `\c_sys_year_int`) is
        // the run's, not the kernel cache's.
        let job = m.intern("everyjob");
        if let crate::value::Value::Toks(toks) = m.env.value(job)
            && !toks.is_empty()
        {
            m.run_tokens(toks);
            m.out.steps = 0;
        }
        // `--steps` counts the document's tokens, not the job's nor what
        // installing the format took.
        m.read = 0;
        (m, id)
    }

    /// What the document's run leaves, once it ran: the hooks after it,
    /// and the facts drawn from all it recorded.
    fn conclude(self, id: FileId) -> Analysis {
        let mut m = self;
        // The document's own run only: nested runs read part of it.
        m.report_unread_inputs();
        crate::observe::text_break(&mut m);
        m.out.timings.enter_file(id);
        m.out.timings.leave_file();
        m.out.timings.start(Phase::Hooks);
        m.out.journal_size = m.env.journal_len();
        m.out.held_tokens_left = m.held_tokens;
        if let Some(start) = m.load_start.take() {
            m.out.load_steps += m.out.steps.saturating_sub(start);
        }
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
        crate::progress::clear();
        if let Some((key, at)) = m.degraded_at {
            let (percent, times) = (m.limits.degrade_at_percent, m.pressure);
            m.diagnose(
                Severity::Imprecision,
                "precision-degraded",
                at,
                format!(
                    "{percent}% of limits.{key} used: the limits that only cost time (paths, sets, choices, \
                     loop and recursion widening, fork and line budgets) were halved {times} time(s) from here \
                     so the run finishes"
                ),
            );
        }
        m.out.plugins.depp = crate::plugin::depp::Depp::read(&m.out);
        m.collect_metadata();
        m.say_clock();
        m.out.timings.finish();
        m.out.calls.finish();
        m.out.observed.finish();
        crate::observe::relate(&mut m);
        m.link_cross_references();
        m.collect_fonts();
        m.out.env = std::mem::take(&mut m.env);
        m.out.catcodes = m.catcodes.clone();
        m.out
    }

    /// Expand a value for reporting. With no list, everything the gullet can
    /// expand is expanded. A list narrows that to the macros it names.
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

    /// The field name and the internal macro `\title`, `\author` and `\date`
    /// store their argument in. Rendered through hyperref's
    /// `\pdfstringdef` when loaded, which expands what it can and drops
    /// what only sets type; `Metadata::source` keeps the raw token list.
    /// `\maketitle` reads them inside the document body, so satex renders
    /// each as it is set: a macro the body defines is still defined then,
    /// gone once the environment closes.
    const METADATA: [(&'static str, &'static str); 3] = [("title", "@title"), ("author", "@author"), ("date", "@date")];

    /// Render one metadata field, if this name is one.
    fn record_metadata(&mut self, sym: Sym) {
        let Some((field, _)) = Self::METADATA.iter().find(|(_, name)| self.name(sym) == *name) else {
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
            // satex's own rendering, which no TeX run does: what it raises
            // (siunitx's `\day` is `d` in a PDF string) is not the document's.
            let facts = &self.out.facts;
            let kept = (facts.diagnostics.clone(), facts.occurrences.len(), facts.raised.len(), self.tex_errors);
            let expanded = self.expand_as_pdf_string(&tokens);
            let facts = &mut self.out.facts;
            facts.diagnostics = kept.0;
            facts.occurrences.truncate(kept.1);
            facts.raised.truncate(kept.2);
            self.tex_errors = kept.3;
            let text = squeeze(&self.pdf_string_text(&expanded));
            // `\maketitle` empties `\@title` once it has set it. The title
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
            let written =
                self.env.meaning(target).as_macro().map(|m| self.text_of(&m.replacement_text)).unwrap_or_default();
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
    /// other characters and spaces. Everything else sets type rather than
    /// text and hyperref removes it after expanding what it can
    /// (hyperref.sty, `\HyPsd@CheckCatcodes`). Math is dropped with its
    /// delimiters, since a formula is not text either.
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
        let engine = self.out.plugins.engine;
        let table = initial_meanings(&mut self.out.interner, engine);
        for (sym, meaning) in table {
            let zero = match meaning {
                // A parameter the engine range-checks starts where the
                // engine starts it, which is not always zero.
                Meaning::Primitive(P::IntegerParameter) => {
                    let name = self.out.interner.name(sym);
                    Value::Int(
                        crate::builtins::limit(name, engine)
                            .and_then(|l| l.start())
                            .or_else(|| crate::builtins::initial(name, engine))
                            .unwrap_or(0),
                    )
                }
                // tex.web § 215: the main vertical list also starts with
                // `\prevdepth` at `ignore_depth`.
                Meaning::Primitive(P::DimenParameter) => {
                    let name = self.out.interner.name(sym);
                    Value::Dimen(match crate::builtins::owned(name) {
                        Some(crate::builtins::Owned::List { fresh, .. }) => fresh,
                        _ => crate::builtins::initial(name, engine).unwrap_or(0),
                    })
                }
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
            ("escapechar", i64::from(b'\\')),
            ("endlinechar", 13),
            ("tolerance", 10_000),
            ("hangafter", 1),
            ("maxdeadcycles", 25),
            ("mag", 1000),
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
    /// `clock: unknown` leaves them unknown: a document may never rely on
    /// the date it is compiled at. Installed again once the format is in
    /// place: what the kernel build read of the clock is the kernel's, the
    /// document's first read is its own.
    fn install_clock(&mut self) {
        let clock = self.cfg.clock.resolve();
        self.clock = clock;
        self.clock_read_at = None;
        self.clock_cached = None;
        // Unknown, each still lies in its calendar range: a test on it
        // (expl3's `\int_const:Nn`) decides alike for every date. Years
        // from the Unix epoch (`SOURCE_DATE_EPOCH` too) to four digits.
        let value = |part: fn(&time::OffsetDateTime) -> i64, lo: i64, hi: i64| match &clock {
            Some(at) => crate::value::Value::Int(part(at)),
            None => crate::value::Value::Num {
                quantity: crate::value::Quantity::Int,
                num: crate::value::Num::range(lo, hi),
            },
        };
        for (name, value) in [
            ("year", value(|at| i64::from(at.year()), 1970, 9999)),
            ("month", value(|at| i64::from(u8::from(at.month())), 1, 12)),
            ("day", value(|at| i64::from(at.day()), 1, 31)),
            ("time", value(|at| i64::from(at.hour()) * 60 + i64::from(at.minute()), 0, 1439)),
        ] {
            let sym = self.intern(name);
            self.env.set_value(sym, value, true);
        }
    }

    /// The clock was read: the document is analyzed as if compiled now (or
    /// at the configured date), which [`Machine::say_clock`] says once.
    pub(crate) fn clock_read(&mut self, span: Span) {
        self.clock_read_at.get_or_insert(span);
    }

    /// The clock assumption, once, where the run ends: not while the kernel
    /// or a package cache is built, whose diagnostics are kept and replayed.
    fn say_clock(&mut self) {
        let Some(span) = self.clock_read_at.take() else { return };
        let at = match self.cfg.clock {
            crate::config::Clock::At(at) => crate::config::Clock::At(at).to_string(),
            _ => self.clock.map(|at| crate::config::Clock::At(at).to_string()).unwrap_or_default(),
        };
        if at.is_empty() {
            return;
        }
        // A pinned clock keys the cache, so only `now` can replay another date.
        let cached = self.clock_cached.take().filter(|_| matches!(self.cfg.clock, crate::config::Clock::Now));
        let cached =
            cached.map(|name| format!("; {name} keeps the date of the run that cached it")).unwrap_or_default();
        self.diagnose(
            Severity::Assumption,
            "clock-assumed",
            span,
            format!("the document is assumed to be compiled at {at}{cached} (`clock:` in satex.yaml sets or unsets the date)"),
        );
    }

    /// pdfTeX manual: `\pdfcreationdate` is `D:YYYYMMDDHHmmSS` followed by
    /// `Z` or `±HH'mm'`. Its digits are the clock's ([`Machine::install_clock`]),
    /// read through the registers so that a package cache sees the read.
    pub(crate) fn creation_date(&mut self, span: Span) -> Vec<Token> {
        self.clock_read(span);
        let mut known = true;
        let mut read = |m: &mut Self, name: &str| {
            let sym = m.intern(name);
            let value = m.env.value(sym).as_int();
            known &= value.is_some();
            value
        };
        let (year, month, day, time) = (read(self, "year"), read(self, "month"), read(self, "day"), read(self, "time"));
        match (known, self.clock, year, month, day, time) {
            (true, Some(at), Some(year), Some(month), Some(day), Some(time)) => {
                let (h, m, _) = at.offset().as_hms();
                let zone = if (h, m) == (0, 0) {
                    "Z".to_string()
                } else {
                    format!("{}{:02}'{:02}'", if h < 0 || m < 0 { '-' } else { '+' }, h.abs(), m.abs())
                };
                let text =
                    format!("D:{year:04}{month:02}{day:02}{:02}{:02}{:02}{zone}", time / 60, time % 60, at.second());
                self.text_tokens(&text, span)
            }
            _ => self.unknown_pdf_date(span),
        }
    }

    /// A date of the form `\pdfcreationdate` gives, its digits unknown:
    /// fourteen digits, then one of the three zone forms.
    pub(crate) fn unknown_pdf_date(&mut self, span: Span) -> Vec<Token> {
        let unknown_digits =
            |n: usize| -> Vec<Token> { vec![Token::new(Tok::Chr(crate::tex::UNKNOWN_DIGIT, Catcode::Other), span); n] };
        let mut out = self.text_tokens("D:", span);
        out.extend(unknown_digits(14));
        let mut plus = self.text_tokens("+", span);
        plus.extend(unknown_digits(2));
        plus.extend(self.text_tokens("'", span));
        plus.extend(unknown_digits(2));
        plus.extend(self.text_tokens("'", span));
        let mut minus = plus.clone();
        minus[0] = Token::new(Tok::Chr('-', Catcode::Other), span);
        let zone = self.text_tokens("Z", span);
        let zone = self.one_of(vec![Rc::from(zone), Rc::from(plus), Rc::from(minus)], span);
        out.push(zone);
        out
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
            .load
            .format
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
        let roots = self.cache_roots();

        // A missing or stale cache is rebuilt below. It says nothing about
        // the document, so it is no finding, only a notice on stderr.
        // One build per process: runs that miss together wait for it and
        // read what it stored.
        let mut _building = None;
        let mut rebuilt: Option<String> = if cache_dir.is_none() {
            Some("cache disabled".into())
        } else {
            self.cfg.cache.rebuild.then(|| "asked for (cache.rebuild)".into())
        };
        if let (Some(directory), false) = (&cache_dir, self.cfg.cache.rebuild) {
            let load = || {
                Format::load(
                    directory,
                    &source,
                    self.out.plugins.engine.as_str(),
                    self.preload_key(),
                    self.cfg.limits.cache_size.as_u64(),
                    &roots,
                )
            };
            let mut cached = load();
            if cached.is_err() {
                _building = Some(crate::format::BUILDING.lock().unwrap_or_else(|e| e.into_inner()));
                cached = load();
            }
            match cached {
                Err(reason) => {
                    // The reason is said at every verbosity: a rebuild costs
                    // minutes, so a run that asks for one says why.
                    let name = source.file_name().map_or(String::new(), |n| n.to_string_lossy().into_owned());
                    rebuilt = Some(reason.clone());
                    crate::progress::step(
                        &format!("building kernel cache from {name}"),
                        &reason,
                        "this may take a couple of minutes",
                    );
                }
                Ok(cached) => {
                    let definitions = cached.definitions();
                    for (path, kind) in cached.files().to_vec() {
                        self.cached_lines +=
                            std::fs::read(&path).map_or(0, |b| b.iter().filter(|&&c| c == b'\n').count() as u64);
                        self.register_file(path, kind);
                    }
                    self.out.kernel_sites = cached
                        .sites()
                        .iter()
                        .map(|(sym, file, line)| (Sym(*sym), Span::new(*file, *line, 0)))
                        .collect();
                    self.wild = cached.wild().clone();
                    // The cache carries what the kernel build diagnosed.
                    self.restore_format_diagnostics(&cached);
                    let definers = cached.definers().to_vec();
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
                                &roots,
                            )
                            .display()
                            .to_string(),
                        ),
                        reached: 0,
                        lines: 0,
                        steps: 0,
                        rebuilt: None,
                        definers,
                    });
                    return;
                }
            }
        }

        let Ok(text) = crate::overlay::read_bytes(&source).map(|b| crate::source::decode(&b)) else {
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
            depth: 0,
            conds: 0,
            eof_seen: false,
        });
        self.budget = Some(self.cfg.limits.format_steps);
        let lines = text.lines().count() as u32;
        self.format_progress = Some(self.input.len() - 1);
        self.run();
        let kernel_steps = self.out.steps;
        self.budget = None;
        let engine = self.out.plugins.engine.as_str();
        crate::progress::done(&format!("built {engine} kernel cache"), self.started.elapsed());
        // The kernel build has a clock of its own; the document's starts after it.
        self.started = std::time::Instant::now();
        self.format_progress = None;
        crate::progress::clear();

        let truncated = self.halted;
        let reached =
            self.out.facts.defs.iter().filter(|def| def.span.file == id).map(|def| def.span.line).max().unwrap_or(0);
        // latex.ltx makes the specials `other` while bootstrapping and restores them
        // in groups; this interpreter only approximates that.
        self.catcodes = plain;
        let preload_truncated = self.run_preload();
        let sites: Vec<(u32, u16, u32)> = {
            let mut seen: HashMap<Sym, Span> = HashMap::default();
            for def in &self.out.facts.defs {
                seen.insert(def.name, def.span);
            }
            self.out.kernel_sites = seen.clone();
            seen.into_iter().map(|(sym, span)| (sym.0, span.file, span.line)).collect()
        };
        let definers = {
            let files = &self.out.files;
            crate::exports::written(&self.out.facts.defs, &self.out.interner, |f| {
                files.get(usize::from(f)).map(|f| f.path.clone())
            })
        };
        self.forget_facts();
        let preload_key = self.preload_key();

        let format = FormatInfo {
            source: source.display().to_string(),
            definitions: self.env.meaning_count(),
            cached: false,
            cache: cache_dir.as_deref().map(|dir| {
                crate::format::cache_file(dir, &source, self.out.plugins.engine.as_str(), preload_key, &roots)
                    .display()
                    .to_string()
            }),
            reached,
            lines,
            steps: kernel_steps,
            rebuilt,
            definers: definers.clone(),
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
        // A format carries no open files or streams (tex.web § 1299): the run
        // that built it goes on as one that loads it, else cold and warm
        // runs key their package caches apart.
        self.streams = Rc::default();
        self.outputs = Rc::default();
        self.carried.stream_paths.clear();
        self.carried.stream_read = None;
        self.stream_files.clear();
        self.section = None;
        self.docstrip_dir = None;
        // The kernel stops at `\dump` inside its own `\input` call: leaving
        // it set would date `\everyjob`'s messages to latex.ltx.
        self.file_call = None;
        self.file_calls.clear();

        // LuaTeX dumps no Lua state: a job starts with a fresh one, and `\everyjob`
        // runs `ltluatex`, luaotfload and expl3's Lua again (LuaTeX manual, "Lua
        // general"). So the cache, which carries no Lua state, loses nothing.
        self.lua = Default::default();
        let cacheable = self.cfg.cache.enabled && !truncated && !preload_truncated && complete;
        // Every file the kernel was read from, in order, minus the document itself,
        // so installing this cache hands out the same file numbers the cached spans carry.
        let files: Vec<(String, LoadKind)> =
            self.out.files.iter().skip(1).map(|file| (file.path.clone(), file.kind)).collect();
        if let (true, Some(directory), Some(captured)) = (
            cacheable,
            &cache_dir,
            Format::capture(&source, &self.out.interner, &self.env, files, sites, &self.wild, &roots)
                .map(|format| format.diagnosed(&self.out.facts.diagnostics, &self.gaps).with_definers(definers)),
        ) {
            let stored = captured.store(
                directory,
                &source,
                self.out.plugins.engine.as_str(),
                preload_key,
                self.cfg.limits.cache_size.as_u64(),
                &roots,
            );
            if let (Err(e), true) = (stored, self.cfg.verbose >= 1) {
                crate::progress::note!("satex: the kernel cache could not be written: {e}");
            }
            // Go on from what was cached, as a run that reads the cache does:
            // a cache never changes an answer.
            let deltas = captured.install(&mut self.out.interner, &mut self.env);
            self.catcodes.apply(&deltas);
            self.out.graph = DependencyGraph::default();
            self.out.calls = CallGraph::default();
            self.out.observed = CallGraph::default();
            self.replay_calls();
        } else if self.cfg.verbose >= 1 && self.cfg.cache.enabled && !cacheable {
            crate::progress::note!(
                "satex: the kernel is not cached: {}",
                if truncated {
                    "latex.ltx was not read to the end"
                } else if preload_truncated {
                    "the preloaded format was cut short"
                } else {
                    "latex.ltx was interpreted only in part"
                }
            );
        }
    }

    /// What the preloaded format contributes to the cache key: a document starting
    /// from its own format cannot share a cache with one starting from the stock kernel.
    fn preload_key(&self) -> u64 {
        let Some(source) = self.out.plugins.preload.as_ref().and_then(|p| p.source.as_ref()) else {
            return 0;
        };
        let encoded = self.cache_roots().encode(&source.display().to_string());
        let mut hash = crate::format::digest(encoded.as_bytes());
        if let Some(content) = crate::format::content_hash(source) {
            hash ^= crate::format::digest(&content.to_le_bytes());
        }
        hash
    }

    /// Interpret the preamble a project dumps into its own format (what a `.fmt`
    /// built by `mylatexformat` holds), so its definitions are in place before the
    /// document is read. Returns whether it was cut short, leaving the format incomplete.
    /// What the file expects loaded before it (`load.expect`), read first:
    /// a class by `\documentclass` where the file is no document itself,
    /// the rest by `\RequirePackage`.
    fn push_expected(&mut self, kind: LoadKind) {
        let expect = self.cfg.load.expect.clone();
        if expect.is_empty() {
            return;
        }
        let base = self.base.clone();
        let mut text = String::new();
        for name in &expect {
            let class = kind != LoadKind::Input
                && text.is_empty()
                && self.resolver.resolve(name, LoadKind::Class, &base).is_some();
            let command = if class { "documentclass" } else { "RequirePackage" };
            text.push_str(&format!("\\{command}{{{name}}}\n"));
        }
        let id = self.register_file("<load.expect>".to_string(), LoadKind::Input);
        self.input.push(Frame::File {
            mouth: Mouth::file(&text, id),
            package: None,
            depth: 0,
            conds: 0,
            eof_seen: false,
        });
    }

    fn run_preload(&mut self) -> bool {
        let Some(source) = self.out.plugins.preload.as_ref().and_then(|p| p.source.clone()) else {
            return false;
        };
        let Ok(text) = crate::overlay::read_bytes(&source).map(|b| crate::source::decode(&b)) else {
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
        // A run of its own: the preamble neither inherits the kernel's spent steps
        // nor leaves anything on the input stack for the document to read as its own.
        (self.out.steps, self.out.load_steps) = (0, 0);
        self.halted = false;
        self.input.push(Frame::Boundary);
        let base = self.input.len();
        self.input.push(Frame::File {
            mouth: Mouth::file(&text, id),
            package: None,
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
        if !self.cfg.cache.enabled {
            return None;
        }
        self.cfg.cache.dir.clone().or_else(format::default_cache_dir)
    }

    /// The table a LaTeX format hands to a document. A package or class
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
        if self.cfg.load.format {
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
        // Definitions and uses belong to the kernel, not the document. Diagnostics
        // about how well it was read do not. The budget starts again for the document.
        self.halted = false;
        // A run that installs the kernel from its cache has not learnt the
        // counter namespace yet either. Both learn it the same way.
        self.counter_namespace = None;
        self.out.exhausted = false;
        (self.out.steps, self.out.load_steps) = (0, 0);
        // Steps of the kernel's: a build would otherwise leave the document's
        // loops open to the stall check a cached kernel leaves them closed to.
        self.region = Region::default();
        self.out.facts.defs.clear();
        self.out.facts.expansions.clear();
        self.out.facts.occurrences.clear();
        self.out.facts.loads.clear();
        self.out.facts.shapes.clear();
        self.call_shape = None;
        self.definition_sites.clear();
        self.expansion_sites.clear();
        self.out.facts.spaces.clear();
        self.out.facts.text = crate::facts::Text::default();
        self.out.font_states.truncate(1);
        self.font_watch = FontWatch::default();
        self.out.facts.conditionals.clear();
        self.cond_sites.clear();
        // What the kernel run left that a cached kernel does not bring:
        // visit counts, summaries, counters and the typed-name table (kept,
        // a built run forked 50x more than a cached one on the same document).
        self.call_runs.clear();
        self.call_recs = Rc::default();
        self.scratch = Scratch::default();
        self.carried = Carried::default();
        self.unknown_chars.clear();
        (self.source_progress, self.lines_read) = (0, 0);
        self.own_files.clear();
        self.last_file = None;
        self.join_commands.clear();
        self.join_hooks.clear();
        // Bound before any path parts, so every path has them.
        self.typed_syms = None;
        self.typed_syms();
    }

    /// A package read and the same package replayed from its cache must
    /// leave the same state. The cache carries meanings, not the
    /// termination tables or site counts, so both forget those here, as
    /// the document start does for the kernel's ([`Machine::forget_facts`]).
    pub(crate) fn end_cacheable_load(&mut self) {
        self.cacheable_load = None;
        (self.guard.lines, self.guard.forks) = std::mem::take(&mut self.guard.held);
        self.forget_settled_calls();
        self.scratch = Scratch::default();
        #[cfg(test)]
        package_cache::note_load_end(&self.carried, self.guard.lines.len());
        for site in self.expansion_sites.values_mut() {
            *site = Site::fresh(site.fact);
        }
    }

    /// `\count5` and the rest: a register addressed by number keeps its value
    /// where a named one does, so a group undoes it the same way.
    pub fn register_sym(&mut self, kind: crate::tex::RegKind, index: i64) -> Sym {
        self.out.interner.register(REGISTER, kind.as_str(), index)
    }

    /// Where a register name's value lives: `\countdef\x=20` makes `\x` another name
    /// for `\count20` (tex.web § 1224), so both read and write the same register.
    /// A `\chardef` constant and anything satex could not number keep their value on their own name.
    pub fn storage(&mut self, sym: Sym) -> Sym {
        match self.env.meaning(sym) {
            Meaning::Register(
                kind @ (crate::tex::RegKind::Count
                | crate::tex::RegKind::Dimen
                | crate::tex::RegKind::Skip
                | crate::tex::RegKind::MuSkip
                | crate::tex::RegKind::Toks),
                index,
            ) if index != crate::tex::UNNUMBERED => self.register_sym(kind, i64::from(index)),
            _ => sym,
        }
    }

    /// The name `\openout` writes: `.tex` added when it has none (tex.web
    /// § 1374).
    fn output_name(name: &str) -> String {
        let name = name.trim();
        let name = name.strip_prefix("./").unwrap_or(name);
        let file = name.rsplit('/').next().unwrap_or(name);
        if file.contains('.') { name.to_string() } else { format!("{name}.tex") }
    }

    /// `\immediate\openout`: the file is there, empty, from here on. With
    /// the stream unpinned, what it gets is unknown.
    pub(crate) fn open_output(&mut self, name: &str, known: bool) {
        let name = Self::output_name(name);
        for k in ["w", "h"] {
            let sym = self.engine_sym(&format!("produced.{name}.{k}"));
            if self.env.get(sym).is_some() {
                self.set_engine(&format!("produced.{name}.{k}"), crate::value::Value::Unknown, true);
            }
        }
        Rc::make_mut(&mut self.outputs).insert(name, known.then(String::new));
    }

    /// `\immediate\write18{command}`: a program the engine may run makes the
    /// files its `--outfile=` or `-o` options name, with content unknown,
    /// when every other argument names a file there (a converter such as
    /// `repstopdf` given its input). Under `-shell-restricted` only the
    /// programs of TeX Live's `shell_escape_commands` run.
    pub(crate) fn run_shell(&mut self, command: &str) {
        use crate::config::ShellEscape;
        const RESTRICTED: &str = "bibtex bibtex8 extractbb gregorio kpsewhich l3sys-query latexminted makeindex \
            memoize-extract.pl memoize-extract.py repstopdf r-mpost texosquery-jre8";
        let mut words = command.split_whitespace().map(|w| w.trim_matches(|c| c == '"' || c == '\''));
        let Some(program) = words.next() else { return };
        let runs = match self.out.plugins.shell_escape {
            ShellEscape::None => false,
            ShellEscape::Restricted => RESTRICTED.split_whitespace().any(|p| p == program),
            ShellEscape::Full => true,
        };
        if !runs || command.chars().any(crate::tex::is_unknown_char) {
            return;
        }
        let (mut outputs, mut inputs, mut options) = (Vec::new(), Vec::new(), false);
        while let Some(word) = words.next() {
            match word.split_once('=') {
                Some(("--outfile" | "-outfile", file)) => outputs.push(file.to_string()),
                _ if word == "-o" => outputs.extend(words.next().map(str::to_string)),
                _ if word.starts_with('-') => options = true,
                _ => inputs.push(word.to_string()),
            }
        }
        let bytes: Vec<_> = inputs.iter().map(|name| self.input_bytes(name)).collect();
        if bytes.contains(&Some(None)) {
            return;
        }
        // epstopdf's page is the EPS bounding box (`Self::produced_page`);
        // `repstopdf` writes only where kpathsea's `openout_any=p` lets it.
        let page = match (&bytes[..], &outputs[..]) {
            ([Some(Some(eps))], [out]) if !options && matches!(program, "epstopdf" | "repstopdf") => {
                let safe = program == "epstopdf" || !(out.starts_with('/') || out.split('/').any(|c| c == ".."));
                crate::pdfbox::eps_page_size(eps).filter(|_| safe)
            }
            _ => None,
        };
        for name in outputs {
            let name = name.strip_prefix("./").unwrap_or(&name).to_string();
            for (k, sp) in [("w", page.map(|p| p.0)), ("h", page.map(|p| p.1))] {
                self.set_engine(
                    &format!("produced.{name}.{k}"),
                    sp.map_or(crate::value::Value::Unknown, crate::value::Value::Int),
                    true,
                );
            }
            Rc::make_mut(&mut self.outputs).insert(name, None);
        }
    }

    /// The page size of the PDF file `name` this run made through
    /// `\write18` (`Self::run_shell`), where it is known: `Some(None)` when
    /// the run made that file with its size unknown, `None` when the run
    /// did not make it that way.
    pub(crate) fn produced_page(&mut self, name: &str) -> Option<Option<(i64, i64)>> {
        let name = name.trim();
        let name = name.strip_prefix("./").unwrap_or(name);
        if self.written_text(name)?.is_some() {
            return None;
        }
        // A stream still open on it may have written there since.
        if self.stream_files.values().any(|n| Self::output_name(n) == Self::output_name(name)) {
            return Some(None);
        }
        let w = self.engine_int(&format!("produced.{name}.w"));
        let h = self.engine_int(&format!("produced.{name}.h"));
        Some(w.zip(h))
    }

    /// A `\write` to `stream`: `text` ends up in its file when known, and
    /// `None` makes the file's content unknown. An unpinned stream may be
    /// any file.
    pub(crate) fn write_output(&mut self, stream: Option<i64>, text: Option<&str>) {
        let Some(stream) = stream else {
            Rc::make_mut(&mut self.outputs).values_mut().for_each(|t| *t = None);
            return;
        };
        let Some(name) = self.stream_files.get(&stream).map(|n| Self::output_name(n)) else { return };
        let open = self.engine_int(&format!("write_open.{stream}"));
        if let Some(slot) = Rc::make_mut(&mut self.outputs).get_mut(&name) {
            match (open, text, slot.as_mut()) {
                (Some(0), ..) => {}
                (Some(1), Some(text), Some(file)) => {
                    file.push_str(text);
                    file.push('\n');
                }
                _ => *slot = None,
            }
        }
    }

    /// What this run wrote to the file `\input` or `\openin` names: `None`
    /// when it wrote no such file, `Some(None)` when what it holds is unknown.
    pub(crate) fn written_text(&self, name: &str) -> Option<Option<String>> {
        let name = name.trim();
        let name = name.strip_prefix("./").unwrap_or(name);
        [Self::output_name(name), name.to_string()].into_iter().find_map(|n| self.outputs.get(&n).cloned())
    }

    pub fn set_stream(&mut self, stream: Option<i64>, lines: Option<Vec<String>>) {
        let Some(stream) = stream else { return self.forget_streams() };
        match lines {
            Some(lines) => {
                Rc::make_mut(&mut self.streams).insert(stream, (Rc::new(lines), 0));
            }
            // tex.web § 1275: a file that is not there leaves the stream
            // closed, which `\ifeof` reads as true.
            None => self.mark_closed(stream),
        }
    }

    /// What stream `stream` reads is not known; the others stay as known.
    pub fn unknown_stream(&mut self, stream: Option<i64>) {
        let Some(stream) = stream else { return self.forget_streams() };
        let streams = Rc::make_mut(&mut self.streams);
        // Widened, an absent stream is unknown: a closed one says so.
        if !streams.contains_key(&ANY_STREAM) {
            for s in 0..16 {
                streams.entry(s).or_insert_with(|| (Rc::new(Vec::new()), CLOSED));
            }
            streams.insert(ANY_STREAM, (Rc::new(Vec::new()), 0));
        }
        streams.remove(&stream);
    }

    pub fn close_in(&mut self, stream: Option<i64>) {
        let Some(stream) = stream else { return self.forget_streams() };
        self.mark_closed(stream);
    }

    /// A stream is closed. Dropping it says so, except after a widening, where the
    /// table has to say it outright, keeping it unchanged when no widening happened.
    fn mark_closed(&mut self, stream: i64) {
        let widened = self.streams.contains_key(&ANY_STREAM);
        let streams = Rc::make_mut(&mut self.streams);
        match widened {
            true => {
                streams.insert(stream, (Rc::new(Vec::new()), CLOSED));
            }
            false => {
                streams.remove(&stream);
            }
        }
    }

    /// `\openin` or `\closein` through a number satex could not pin down: it may have
    /// reached any stream, so no stream's state is known from here on (tex.web § 1275).
    pub fn forget_streams(&mut self) {
        let streams = Rc::make_mut(&mut self.streams);
        streams.clear();
        streams.insert(ANY_STREAM, (Rc::new(Vec::new()), 0));
    }

    /// A stream that was never opened, or whose file was not found, is at
    /// end. An open one is at end only once a read has found no line left
    /// (tex.web § 485 closes it then), so the last line is read with
    /// `\ifeof` false.
    /// `None` once a number satex could not pin down opened or closed one.
    pub fn stream_at_end(&self, stream: i64) -> Option<bool> {
        // What the table holds was learned after the last widening.
        if let Some((_, at)) = self.streams.get(&stream) {
            return Some(*at == CLOSED);
        }
        if self.streams.contains_key(&ANY_STREAM) {
            return None;
        }
        Some(true)
    }

    /// Whether `\read` and `\readline` take a line from the stream rather
    /// than from the terminal (tex.web § 484). `None` where the number or
    /// the stream's state is not known.
    pub(crate) fn stream_open(&mut self, stream: Option<i64>) -> Option<bool> {
        let n = stream?;
        if !(0..16).contains(&n) {
            return Some(false);
        }
        self.stream_at_end(n).map(|at_end| !at_end)
    }

    pub fn take_stream_line(&mut self, stream: i64) -> Option<String> {
        // The lines stay shared. Only the position moves, so a snapshot of
        // the streams costs nothing however long the file is.
        let (lines, next) = Rc::make_mut(&mut self.streams).get_mut(&stream)?;
        let Some(line) = lines.get(*next).cloned() else {
            self.close_in(Some(stream));
            return None;
        };
        *next += 1;
        self.carried.stream_read = Some((stream, *next, lines.len(), self.out.steps));
        // A line read from a file is input read, as one the mouth reads is:
        // a loop over a data file (expl3's `\ior_map_variable:NNn` over
        // UnicodeData.txt) makes progress with every line.
        self.source_progress += 1;
        self.lines_read += 1;
        self.env.clear_recent();
        self.read_on(self.region.span, true);
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
            Frame::Pseudo { mouth, .. } => Some(mouth.here().line),
            _ => None,
        })
    }

    pub fn resolver_mut(&mut self) -> &mut Resolver {
        &mut self.resolver
    }

    pub fn package(&self) -> Option<Sym> {
        self.packages.last().map(|(name, _)| *name)
    }

    /// The next token, ending a scan that the end of a file interrupts.
    ///
    /// tex.web §§ 336-339: a file ending mid-scan (definition, argument, skipped
    /// conditional) ends that scan and returns nothing. A read outside a scan
    /// goes on in the file that opened this one, so `\futurelet` in `\@ifnextchar`
    /// sees past the end of an `\input` file.
    pub fn next_token(&mut self) -> Option<Token> {
        self.read(false)
    }

    /// Every token read counts against the budget, so that a runaway loop in
    /// any reader terminates and not just the main one.
    fn read(&mut self, across_files: bool) -> Option<Token> {
        let token = self.read_token(across_files)?;
        self.count_brace(&token, 1);
        if self.template_end_read(token) {
            return self.read(across_files);
        }
        // tex.web § 336: `check_outer_validity` runs on every token `get_next`
        // reads, not only where a file ends.
        if let Some(space) = self.outer_while_scanning(token) {
            return Some(space);
        }
        if token.tok != Tok::Cs(self.dont_expand) {
            self.noexpanded = None;
            return Some(token);
        }
        // tex.web § 358: the marker goes, and the token after it is read as
        // `\relax` would be, whatever its meaning.
        let token = self.read_token(across_files)?;
        self.count_brace(&token, 1);
        self.noexpanded = Some(token);
        Some(token)
    }

    /// Whether the token the last read returned stood behind a `\noexpand`
    /// marker and would otherwise have been expanded.
    pub fn read_noexpanded(&self, token: Token) -> bool {
        self.noexpanded.is_some_and(|t| t.tok == token.tok && t.span == token.span)
    }

    fn read_token(&mut self, across_files: bool) -> Option<Token> {
        let t = self.take_token(across_files);
        if self.out.timings.is_enabled()
            && let Some(t) = t
            && self.last_file.is_some()
            && self.own_file(t.span.file)
        {
            self.out.timings.at_site(t.span.file, t.span.line);
        }
        t
    }

    fn take_token(&mut self, across_files: bool) -> Option<Token> {
        if self.over_budget() {
            return None;
        }
        self.out.steps += 1;
        self.read += 1;
        if self.out.timings.is_enabled() {
            self.out.timings.count_token();
        }
        // A scan that began in text a cut dropped ends there: it never
        // reads on into the file below ([`Machine::scan_end`]).
        if self.scan_cut {
            if !(self.reading_command && self.step_depth == 1) {
                return None;
            }
            self.scan_cut = false;
            self.scan_starts.clear();
        }
        loop {
            // A skipped region's readers find the input ended, as at the
            // budget: only the main loop reads on.
            if self.guard_seen.cut {
                if !(self.reading_command && self.step_depth == 1) {
                    return None;
                }
                self.resync();
            }
            if !self.call_recs.is_empty() {
                self.end_call_recs();
                if matches!(self.input.last(), Some(Frame::File { .. } | Frame::Pseudo { .. })) {
                    self.leak_call_recs();
                }
            }
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
                Some(Frame::Tokens { toks, pos, expanding, .. }) => {
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
                    let mut load = false;
                    let next = match mouth.next_with(catcodes, &mut out.interner, *end_line) {
                        Some(t) => Some(Token::new(t.tok, *at)),
                        None => {
                            load = *pos == 0 && eof.is_empty();
                            eof.get(*pos).copied().inspect(|_| *pos += 1)
                        }
                    };
                    if let Some(t) = next {
                        self.source = Some(self.pseudo_file);
                        self.last_file = None;
                        return Some(t);
                    }
                    // etex.ch: `\everyeof` is read as the pseudo file ends, so the
                    // value its own text set counts.
                    if load {
                        let sym = self.intern("everyeof");
                        if let crate::value::Value::Toks(toks) = self.env.value(sym)
                            && !toks.is_empty()
                        {
                            let toks = toks.clone();
                            if let Some(Frame::Pseudo { eof, .. }) = self.input.last_mut() {
                                *eof = toks;
                            }
                            continue;
                        }
                    }
                    // A pseudo file ends like a file.
                    if !across_files && self.scanner != Scanner::Normal {
                        self.file_ended_while_scanning(None);
                        return None;
                    }
                }
                Some(Frame::File { mouth, eof_seen, .. }) => {
                    let before = mouth.clone();
                    if let Some(t) = mouth.next_with(catcodes, &mut out.interner, *end_line) {
                        // A character that may be structural: a path for each
                        // category, on which it has that one, reads it again.
                        if let Some(c) = mouth.undecided.take() {
                            let cats = catcodes.cats(c);
                            let plain = (1 << Catcode::Letter as u8) | (1 << Catcode::Other as u8);
                            if cats.count_ones() <= SPLIT_CATS && cats & !plain != 0 {
                                let after = std::mem::replace(mouth, before);
                                if self.split_catcode(c, cats, t.span) {
                                    continue;
                                }
                                if let Some(Frame::File { mouth, .. }) = self.input.last_mut() {
                                    *mouth = after;
                                }
                            }
                        }
                        let Some(Frame::File { mouth, .. }) = self.input.last_mut() else { return Some(t) };
                        let here = mouth.here();
                        // The character an unknown-text token stood for, so
                        // that executing it later can be bounded by what
                        // that character could mean (tex.web §§ 232, 344).
                        let unknown = mouth.last_unknown.take();
                        if let Some(c) = unknown {
                            self.unknown_chars.insert(t.span, c);
                        }
                        // A letter or an other character either way: one of the two tokens.
                        let plain = (1 << Catcode::Letter as u8) | (1 << Catcode::Other as u8);
                        let chars = unknown.filter(|&c| self.catcodes.cats(c) & !plain == 0);
                        // A path takes the guard's cuts made on its line before it came.
                        let key = self.guard_key(here);
                        if self.guard_seen.line != key {
                            self.guard_seen.line = key;
                            self.guard_seen.level = self.guard.lines.get(&key).map_or(0, |e| e.1);
                        }
                        self.source_progress += 1;
                        self.env.clear_recent();
                        let main = self.reading_command && self.step_depth == 1;
                        self.read_on(t.span, main);
                        self.last_file = Some(t.span.file);
                        self.source = None;
                        if let Some(c) = chars {
                            let texts = [Catcode::Letter, Catcode::Other]
                                .map(|cat| Rc::from([Token::new(Tok::Chr(c, cat), t.span)]) as Rc<[Token]>);
                            return Some(self.one_of(texts.into(), t.span));
                        }
                        return Some(t);
                    }
                    // etex.ch § 362: `\everyeof` is read before the file ends, so a scan may
                    // find its delimiter there. Only on running out; `\endinput` skips it.
                    if !mouth.ending() && !std::mem::replace(eof_seen, true) {
                        let eof = self.intern("everyeof");
                        if let crate::value::Value::Toks(toks) = self.env.value(eof)
                            && !toks.is_empty()
                        {
                            let toks = toks.clone();
                            self.push_tokens(toks);
                            continue;
                        }
                    }
                    let Some(Frame::File { mouth, .. }) = self.input.last() else { continue };
                    // tex.web §§ 336-339: the file ends only a scan in progress. A read
                    // outside one goes on in the file that opened this one, so `\usepackage`
                    // on the last line of an `\input` file still loads.
                    if !across_files && self.scanner != Scanner::Normal {
                        self.file_ended_while_scanning(Some(mouth.file));
                        return None;
                    }
                }
            }
            // The end of a captured package's file is a checkpoint regardless of
            // who found it: a look-ahead past the end pops the frame like the main
            // loop does, and the segment needs an end to be replayed to.
            if self.package_capture.is_some() {
                self.checkpoint_at_end();
            }
            let outer = std::mem::replace(&mut self.popping_in_read, true);
            self.pop_frame();
            self.popping_in_read = outer;
        }
    }

    /// tex.web §§ 336-339 `check_outer_validity`: an `\outer` control sequence met while
    /// scanning a definition, use, conditional text or token list is forbidden there.
    /// The scan is abandoned with the token that would have ended it, and the control
    /// sequence is read again as a command. A space takes its place.
    fn outer_while_scanning(&mut self, token: Token) -> Option<Token> {
        if self.scanner == Scanner::Normal {
            return None;
        }
        let sym = token.cs()?;
        if !self.env.is_outer(sym) {
            return None;
        }
        let (what, recovery) = match self.scanner {
            Scanner::Normal => return None,
            Scanner::Skipping => ("conditional text", Tok::Cs(self.intern("fi"))),
            Scanner::Defining => ("definition", Tok::Chr('}', Catcode::End)),
            Scanner::Matching => ("use", Tok::Cs(self.intern("par"))),
            Scanner::Absorbing => ("text", Tok::Chr('}', Catcode::End)),
        };
        if self.scanner == Scanner::Matching {
            self.runaway = Runaway::Outer;
        }
        let name = self.name(sym).to_string();
        self.diagnose(
            Severity::Warning,
            "forbidden-outer",
            token.span,
            format!("\\{name} is \\outer, so scanning a {what} stops where it was met"),
        );
        self.unread(token);
        self.unread(Token::new(recovery, token.span));
        Some(Token::new(Tok::Chr(' ', Catcode::Space), token.span))
    }

    /// tex.web §§ 336-339 `check_outer_validity`: a file ending mid-scan ends it.
    /// The definition or text gets its `}`, the skipped conditional its `\fi`, and a
    /// macro call is abandoned, as `\par` with `long_state = outer_call` abandons it.
    fn file_ended_while_scanning(&mut self, file: Option<FileId>) {
        let (what, recovery) = match self.scanner {
            Scanner::Normal => return,
            Scanner::Skipping => ("conditional text", "\\fi"),
            Scanner::Defining => ("definition", "}"),
            Scanner::Matching => ("use", "\\par"),
            Scanner::Absorbing => ("text", "}"),
        };
        // A macro call reports it at the call, named (tex.web § 338).
        if self.scanner == Scanner::Matching {
            self.runaway = Runaway::FileEnded;
            return;
        }
        let span = file.map_or_else(|| self.position_span(), |f| Span::new(f, 0, 0));
        self.diagnose(
            Severity::Warning,
            "file-ended",
            span,
            format!("file ended while scanning {what}; {recovery} inserted"),
        );
    }

    fn pop_frame(&mut self) {
        match self.input.pop() {
            Some(Frame::Boundary) => {}
            Some(Frame::Returned(tokens)) => {
                self.held_tokens = self.held_tokens.saturating_sub(tokens.len());
            }
            Some(Frame::Tokens { expanding, held, upper, .. }) => {
                self.held_tokens = self.held_tokens.saturating_sub(held);
                if let Some(sym) = expanding.filter(|_| !upper) {
                    if let Some(n) = self.expanding.get_mut(sym.0 as usize) {
                        *n = n.saturating_sub(1);
                    }
                    Rc::make_mut(&mut self.within).pop();
                    // The expansion the call started is over, so the call has
                    // read everything it reads.
                    if self.call_shape.as_deref().is_some_and(|(.., depth)| self.within.len() < *depth) {
                        self.finish_shape();
                    }
                }
            }
            Some(Frame::Pseudo { .. }) => self.bump_expanding(self.pseudo_file, -1),
            Some(Frame::File { mouth, package, depth, conds, .. }) => {
                self.out.timings.leave_file();
                self.out.lines += u64::from(mouth.here().line);
                self.balance_file(mouth.file, depth);
                self.balance_conditionals(mouth.file, conds);
                if package.is_some() {
                    Rc::make_mut(&mut self.packages).pop();
                    if self.packages.is_empty()
                        && let Some(start) = self.load_start.take()
                    {
                        self.out.load_steps += self.out.steps.saturating_sub(start);
                    }
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
                if self.cacheable_load == Some(mouth.file) {
                    self.end_cacheable_load();
                }
            }
            _ => {}
        }
    }

    /// TeX reports groups still open when a job ends (tex.web § 1335). satex checks
    /// this at every file boundary: a file returning at deeper nesting than it began
    /// has lost a `}` or `\endgroup` to an approximation, and leaving the level open
    /// would make a later unrelated `}` undo definitions made since. The level is
    /// given up without restoring, so those definitions survive.
    /// A file ending inside a conditional leaves it open, reported as an incomplete
    /// `\if` (tex.web § 336). Its levels go with it: what follows is not governed by
    /// a test the file never finished.
    fn balance_conditionals(&mut self, file: FileId, depth: usize) {
        if self.conds.len() <= depth {
            return;
        }
        let open = self.conds.len() - depth;
        let opened = self.conds[depth].at;
        self.drop_conditionals(depth);
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
            .map(|(kind, span)| format!("{} at {}:{}", opened_by(*kind), self.out.short_name(span.file), span.line))
            .collect();
        // Assumed, as of a package that is done: its groups end with it.
        self.diagnose(
            Severity::Assumption,
            "unbalanced-file",
            Span::new(file, 0, 0),
            format!("end of {name} occurred inside a group at level {open}; opened by {}", where_from.join(", ")),
        );
    }

    /// tex.web § 325: before anything goes back into the input, used-up token lists
    /// are ended, so a macro whose expansion is over no longer counts as expanding.
    /// A loop that re-expands its result with `\expandafter` does not nest.
    fn end_used_up_lists(&mut self) {
        // A pseudo file stays until a read meets its end (tex.web § 362).
        while let Some(Frame::Tokens { toks, pos, expanding, .. }) = self.input.last()
            && *pos >= toks.len()
            && *expanding != Some(self.pseudo_file)
        {
            self.pop_frame();
        }
    }

    /// `\noexpand`'s give-back (tex.web § 369): `token` goes back behind the
    /// `dont_expand` marker, so its next read acts as `\relax` (§ 358). A
    /// look-ahead TeX does not make uses it to leave a marked token as it was.
    pub fn unread_noexpanded(&mut self, token: Token) {
        self.unread(token);
        self.unread(Token::new(Tok::Cs(self.dont_expand), token.span));
    }

    /// `back_input` (tex.web § 325). A token read from behind `\noexpand`'s
    /// marker goes back without it: the marker is consumed by the read, so
    /// `\count1=1\noexpand\x` expands `\x` afterwards.
    pub fn unread(&mut self, token: Token) {
        self.count_brace(&token, -1);
        if self.read_noexpanded(token) {
            self.noexpanded = None;
        }
        if self.last_file == Some(token.span.file) {
            self.note_given_back(token.span);
        }
        self.end_used_up_lists();
        self.held_tokens += 1;
        match self.input.last_mut() {
            Some(Frame::Returned(tokens)) => tokens.push(token),
            _ => self.input.push(Frame::Returned(vec![token])),
        }
    }

    /// Note where a token given back stood. One entry per position: text a primitive
    /// produces stands where its operand did (tex.web § 465, `\string`), so `\string`
    /// gives back a whole name at one span; counting those would push out the peeked positions.
    fn note_given_back(&mut self, span: Span) {
        if self.recent_file_spans.contains(&span) {
            return;
        }
        if self.recent_file_spans.len() == RECENT_FILE_SPANS {
            self.recent_file_spans.remove(0);
        }
        self.recent_file_spans.push(span);
    }

    pub fn unread_all(&mut self, tokens: &[Token]) {
        if tokens.is_empty() {
            return;
        }
        for t in tokens {
            self.count_brace(t, -1);
        }
        // Given back by a keyword scan (tex.web § 407): still the file's.
        let last_file = self.last_file;
        for t in tokens.iter().filter(|t| last_file == Some(t.span.file)) {
            self.note_given_back(t.span);
        }
        self.end_used_up_lists();
        self.held_tokens += tokens.len();
        match self.input.last_mut() {
            Some(Frame::Returned(returned)) => returned.extend(tokens.iter().rev().copied()),
            _ => self.input.push(Frame::Returned(tokens.iter().rev().copied().collect())),
        }
    }

    /// tex.web §§ 325, 347, 357: an explicit brace read counts `by` toward
    /// `align_state`, one given back `-by`.
    pub(crate) fn count_brace(&mut self, token: &Token, by: i64) {
        match token.tok {
            Tok::Chr(_, Catcode::Begin) => self.align_state += by,
            Tok::Chr(_, Catcode::End) => self.align_state -= by,
            _ => {}
        }
    }

    pub fn peek(&mut self) -> Option<Token> {
        let t = self.next_token()?;
        self.unread(t);
        Some(t)
    }

    pub fn push_tokens(&mut self, toks: Rc<[Token]>) {
        if toks.is_empty() || self.held_tokens + toks.len() > self.cfg.limits.held_tokens {
            if !toks.is_empty() {
                let at = toks[0].span;
                self.limit_reached("held_tokens", at);
            }
            return;
        }
        self.held_tokens += toks.len();
        self.end_used_up_lists();
        let held = toks.len();
        let at = (self.conds.len(), self.env.group_id(), self.env.assignments);
        self.input.push(Frame::Tokens { toks, pos: 0, expanding: None, held, upper: false, at });
    }

    /// A token list TeX inserts on its own, such as `\everypar` (tex.web
    /// § 323 `begin_token_list`): what it expands is its doing, not the
    /// doing of the macro whose text set it off.
    pub(crate) fn push_inserted(&mut self, sym: Sym, toks: Rc<[Token]>, span: Span) {
        let node = self.force_reference(sym, span);
        self.end_used_up_lists();
        self.bump_expanding(sym, 1);
        crate::push_shared(&mut self.within, (sym, node));
        let before = self.input.len();
        let held = toks.len();
        self.push_parts(vec![(toks, held)], sym);
        if self.input.len() == before {
            Rc::make_mut(&mut self.within).pop();
        }
    }

    /// [`Machine::push_tokens`] for an expansion standing on several lists,
    /// in reading order: held as one, or not at all.
    fn push_parts(&mut self, mut parts: Vec<(Rc<[Token]>, usize)>, sym: Sym) {
        parts.retain(|(toks, _)| !toks.is_empty());
        let held: usize = parts.iter().map(|(_, n)| n).sum();
        if parts.is_empty() || self.held_tokens + held > self.cfg.limits.held_tokens {
            self.bump_expanding(sym, -1);
            if let Some((toks, _)) = parts.first() {
                self.limit_reached("held_tokens", toks[0].span);
            }
            return;
        }
        self.held_tokens += held;
        let last = parts.len() - 1;
        let at = (self.conds.len(), self.env.group_id(), self.env.assignments);
        for (i, (toks, held)) in parts.into_iter().enumerate().rev() {
            self.input.push(Frame::Tokens { toks, pos: 0, expanding: Some(sym), held, upper: i < last, at });
        }
    }

    pub(crate) fn push_pseudo(&mut self, mut mouth: Box<Mouth>, at: Span, eof: Rc<[Token]>) {
        self.end_used_up_lists();
        // A pseudo file asks for its first line, so a pending `\endinput`
        // takes that line away and the pseudo file is empty (etex.ch § 362).
        if self.take_pending_eof() {
            mouth.inherit_eof(true);
        }
        self.input.push(Frame::Pseudo { mouth, at, eof, pos: 0 });
    }

    pub(crate) fn bump_expanding(&mut self, sym: Sym, delta: i32) {
        let i = sym.0 as usize;
        if self.expanding.len() <= i {
            self.expanding.resize(i + 1, 0);
        }
        self.expanding[i] = if delta < 0 { self.expanding[i].saturating_sub(1) } else { self.expanding[i] + 1 };
    }

    pub fn open_group(&mut self, kind: GroupKind, span: Span) {
        self.env.push_group(kind, span);
    }

    /// A closer meets a group that stands for any number of groups: a path
    /// where one of them closes, and one where there are none. Where no
    /// more paths may part, the group below joins it, so one path covers
    /// both.
    fn unfold_group(&mut self, token: Token) {
        let syms = self.typed_syms();
        let by = token.cs().unwrap_or(self.unknown);
        if self.split(by, token.span, 2, |m, arm| {
            let typed = crate::env::typed!(m, syms);
            m.env.unfold(arm == 0, &mut m.catcodes, &typed);
            true
        }) {
            return;
        }
        // The group below joins it, when of its kind: one path covers both.
        let kinds: Vec<GroupKind> = self.env.groups().iter().rev().take(2).map(|g| g.0).collect();
        let typed = crate::env::typed!(self, syms);
        if let [top, below] = kinds[..]
            && top == below
        {
            self.env.unfold(false, &mut self.catcodes, &typed);
            self.env.set_top_many();
        } else {
            self.diagnose(
                Severity::Assumption,
                "unbalanced-path",
                token.span,
                "a closer met any number of open groups and no more paths could part; one of them is assumed to close"
                    .into(),
            );
        }
        let typed = crate::env::typed!(self, syms);
        self.env.unfold(true, &mut self.catcodes, &typed);
    }

    /// `}`, `\endgroup` or a closing `$`. Each ends only the kind of group its opener
    /// made (tex.web § 269). A closer with no group to end, or one that would
    /// close a group opened outside the analyzed region, is ignored: nothing to restore.
    pub fn close_group(&mut self, closer: GroupKind, token: Token) {
        let span = token.span;
        if self.env.top_many() {
            self.unfold_group(token);
        }
        let Some(kind) = self.env.group_kind() else {
            // tex.web § 1068: a `}` at the bottom level is deleted.
            if closer == GroupKind::Simple {
                // The brace is the document's command: the error is raised at it.
                if self.own_file(span.file) {
                    self.project_site = Some(span);
                }
                self.diagnose(Severity::Warning, "too-many-braces", span, "`}` closes no group".into());
            }
            return;
        };
        if !closer.closes(kind) {
            if closer == GroupKind::Simple {
                // tex.web § 1069: a `}` that meets `\begingroup` or `$` is deleted.
                let msg = format!("extra }}, or forgotten {}", name_of(kind));
                self.diagnose(Severity::Warning, "mismatched-group", span, msg);
                return;
            }
            // tex.web § 1064 `off_save`: the closer the open group waits for is
            // inserted before this one, which is read again.
            let msg = format!("missing {} inserted before {}", name_of(kind), name_of(closer));
            self.diagnose(Severity::Warning, "mismatched-group", span, msg);
            self.unread(token);
            return self.close_group(kind, token);
        }
        if closer == GroupKind::SemiSimple
            && self.within.is_empty()
            && let Some((_, opened)) = self.env.open_group_spans().last().copied()
            && opened.file == span.file
            && opened.line < span.line
        {
            self.out.facts.groups.push((opened, span));
        }
        let inner = crate::observe::current_environment(self);
        let nest = self.env.nest();
        // tex.web §§ 1100, 1133: a box's paragraph ends before `unsave`,
        // so it is broken with the box's own `\hsize`.
        if nest.is_some_and(|n| n.level) && self.mode.meets(crate::mode::Modes::HORIZONTAL) {
            self.end_graf();
        }
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
    /// tex.web § 1138 reads the token after the first `$` and starts display math
    /// when it is another; § 1197 asks for the second `$` again at the end, so
    /// `$$ … $$` is one group, not two.
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
        let node = self.out.graph.push(VertexTag::VariableDefinition, via, span, within, self.cds.to_vec());
        if via != name {
            let defs = self.env.link_defs(via);
            self.link_reads(node, via, &defs);
        }
        // Assigning a register reads nothing of the value it held. A package
        // cache counts it as a read only when the name means something else.
        let tracking = self.env.track_reads(false);
        let mut binding = self.env.get(name).cloned().unwrap_or_else(|| Binding::builtin(Meaning::Unknown));
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
        let node = self.out.graph.push(VertexTag::VariableDefinition, name, span, within, self.cds.to_vec());
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
        self.leak_call_recs();
        self.region_catcode(character);
        self.env.save_catcode_state(character, self.catcodes.get(character), self.catcodes.may(character));
        self.catcodes.set(character, catcode);
    }

    /// `\catcode` through a character number satex could not pin down
    /// (tex.web § 1232): each character `abs` spans (all for `None`) keeps
    /// its old category or takes one of `cats`. The save stack cannot put
    /// back what was never one character's, so the enclosing group does not end it.
    pub fn set_catcodes_reach(&mut self, abs: Option<crate::value::Num>, cats: u16) {
        let top = i64::from(char::MAX as u32);
        let (lo, hi) = abs.map_or((0, top), |n| (n.lo.clamp(0, top), n.hi.clamp(0, top)));
        self.leak_call_recs();
        self.catcodes.set_reach(lo as u32, hi as u32, cats);
    }

    /// A cut unwinds to the main loop: readers find the input ended.
    pub(crate) fn cutting(&self) -> bool {
        self.guard_seen.cut
    }

    /// The reader met `c`, whose category is one of `cats`: a path for
    /// each, on which `c` has that one. `false` when no more paths may part here.
    fn split_catcode(&mut self, c: char, cats: u16, span: Span) -> bool {
        let arms: Vec<Catcode> = (0..16).filter(|k| cats & 1 << k != 0).filter_map(Catcode::from_u8).collect();
        let mut taken = None;
        if !self.split(self.unknown, span, arms.len(), |_, arm| {
            taken = Some(arms[arm]);
            true
        }) {
            return false;
        }
        if let Some(cat) = taken {
            self.env.save_catcode_state(c, self.catcodes.get(c), self.catcodes.may(c));
            self.catcodes.set(c, cat);
        }
        true
    }

    /// The character's category is one of `cats`, one bit each.
    pub fn set_catcode_may(&mut self, character: char, cats: u16) {
        if self.catcodes.cats(character) == cats {
            return;
        }
        self.leak_call_recs();
        self.region_catcode(character);
        self.env.save_catcode_state(character, self.catcodes.get(character), self.catcodes.may(character));
        self.catcodes.set_may(character, cats);
    }

    /// Steps a cached segment replayed at once are not spent in the region,
    /// nor on the line that asked for it: they were the segment's own lines.
    pub(crate) fn skip_replayed_steps(&mut self, steps: u64) {
        self.guard.step = self.guard.step.saturating_add(steps).min(self.out.steps);
    }

    fn region_catcode(&mut self, c: char) {
        if !self.region.catcodes.iter().any(|r| r.0 == c) {
            self.region.catcodes.push((c, self.catcodes.get(c), self.catcodes.may(c)));
        }
    }

    /// A source token was read. The main loop reading a command from a file
    /// further than any path read before begins a region: paths behind it
    /// and replays go on in the region it began. A lone path is the furthest.
    fn read_on(&mut self, span: Span, main: bool) {
        self.region.catcodes.clear();
        let progress = self.source_progress;
        if main && (progress > self.region.progress || (self.waiting == 0 && progress != self.region.progress)) {
            self.region = Region { progress: self.source_progress, span, ..Region::default() };
        }
    }

    pub fn set_end_line(&mut self, value: i32) {
        self.end_line = EndLineChar(value);
    }

    /// `\endlinechar` is an integer parameter like any other, so the end of
    /// a group restores it (tex.web § 236). The mouth follows the table.
    pub(crate) fn sync_end_line(&mut self) {
        let sym = self.intern("endlinechar");
        // A copy kept in step with the parameter, not a use of it: a package
        // cache keys on the copy itself.
        if let Some(value) = self.env.slot_value(sym).and_then(|v| i32::try_from(v).ok()) {
            self.end_line = EndLineChar(value);
        }
    }

    /// Appends to the execution trace when `trace` is on.  The kernel is not
    /// traced: a cached one brings no events, so the trace is the document's
    /// whatever state the cache was in.
    pub fn record(&mut self, kind: Step, name: Sym, span: Span) {
        self.record_with(kind, name, span, || None::<&str>);
    }

    /// Whether the innermost file being read lies outside the main file's
    /// directory: a package or kernel file.
    fn reading_library(&self) -> bool {
        let file = self.input.iter().rev().find_map(|f| match f {
            Frame::File { mouth, .. } => Some(mouth.here().file),
            _ => None,
        });
        let path = |id: FileId| self.out.files.get(id as usize).map(|f| std::path::Path::new(&f.path));
        file.is_some_and(|file| {
            file != self.out.main_file
                && !matches!((path(file), path(self.out.main_file)),
                    (Some(f), Some(main)) if crate::paths::within(&crate::paths::dir_of(main), f))
        })
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
        let steps = self.cfg.trace_steps.is_none_or(|(a, b)| (a..=b).contains(&self.read.saturating_sub(1)));
        let lines = self.cfg.trace_lines.is_none_or(|(a, b)| {
            let main = |s: &Span| self.is_trace_file(s.file);
            let anchor = match self.last_file == Some(span.file) && main(&span) {
                true => Some(span),
                false => self
                    .file_calls
                    .iter()
                    .filter_map(|(_, call)| *call)
                    .chain(self.file_call)
                    .map(|(_, s)| s)
                    .find(main),
            };
            anchor.is_some_and(|s| {
                (a..=b).contains(&s.line)
                    && (s.line > a || s.col >= self.cfg.trace_col)
                    && (s.line < b || s.col <= self.cfg.trace_end_col)
            })
        });
        steps && lines
    }

    /// A character typeset at `span`, kept when a trace range asks what its
    /// lines produce.
    pub(crate) fn note_text(&mut self, c: char, span: Span) {
        const LIMIT: usize = 1 << 16;
        // Heads and feet the output routine sets are no text of the document.
        if self.output_active {
            return;
        }
        crate::observe::typeset(self, c, span);
        if self.cfg.trace
            && self.cfg.trace_lines.is_some()
            && self.out.typeset.len() < LIMIT
            && !self.reading_format()
            && self.in_trace_range(span)
        {
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
    pub fn record_with<D: Into<Box<str>>>(
        &mut self,
        kind: Step,
        name: Sym,
        span: Span,
        detail: impl FnOnce() -> Option<D>,
    ) {
        if !self.cfg.trace || self.trace_full || self.reading_format() || !self.in_trace_range(span) {
            return;
        }
        if self.cfg.trace_document_only && self.reading_library() {
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
            self.limit_reached("trace", span);
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
        let cs = self.out.interner.cs(name).to_string();
        let message = format!("{cs} is not interpreted here, so its effect is not in this analysis");
        self.note_gap_with(code, name, span, Severity::Imprecision, message);
    }

    /// A gap whose subject is not a control sequence, so it brings its own
    /// message and severity.  `name` still keys it, so one subject is
    /// recorded once.
    pub fn note_gap_with(&mut self, code: &'static str, name: Sym, span: Span, severity: Severity, message: String) {
        if !self.gaps.insert((code, name)) {
            self.raise_gap(severity, code);
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
        self.diagnose(severity, code, span, message);
    }

    /// A gap the constructs around it widen for, whether or not it was
    /// reported before.
    fn raise_gap(&mut self, severity: Severity, code: &'static str) {
        if matches!(severity, Severity::Unsupported | Severity::Assumption) && WIDEN_ON.contains(&code) {
            self.gaps_raised = self.gaps_raised.saturating_add(1);
            let depth = self.edef_depth;
            self.gap_depth = Some(self.gap_depth.map_or(depth, |at| at.min(depth)));
        }
    }

    pub fn diagnose(&mut self, mut severity: Severity, code: &'static str, span: Span, message: String) {
        // What TeX would say about text a cut left after its `\unknown` is
        // not known: that text may have been taken as arguments.
        if severity == Severity::Warning && self.after_cut() {
            severity = Severity::Imprecision;
        }
        // The engine's errors satex reports itself, met on a path that
        // approximated since the paths parted: the approximation may be
        // what led there, so they are not TeX's for certain.
        if severity == Severity::Warning && self.approximated && ENGINE_WARNINGS.contains(&code) {
            severity = Severity::Imprecision;
        }
        self.raise_gap(severity, code);
        match TEX_ERRORS.iter().find(|(c, _)| *c == code) {
            // An engine error on a command satex approximated: the run may
            // not be TeX's there, so it is satex's gap, cut when it repeats.
            // On a path that approximated since the paths parted the error
            // may be the approximation's, as [`Machine::certain_error`]
            // judges it: a possible error, not TeX's.
            Some((_, text))
                if self.scratch.approximations != self.scratch.approximations_at_command
                    || self.after_cut()
                    || self.approximated =>
            {
                severity = Severity::Unsupported;
                let text = text.map_or_else(|| message.clone(), str::to_string);
                self.certain_error(&text, false, span);
                self.dropping_breaks |= self.dropping_possible && BREAKING.contains(&code);
            }
            Some((_, text)) => {
                let text = text.map_or_else(|| message.clone(), str::to_string);
                self.certain_error(&text, false, span);
                self.tex_error(&text, span);
            }
            None if matches!(severity, Severity::Unsupported | Severity::Imprecision) => {
                self.scratch.approximations += 1;
                self.approximated = true;
            }
            None => {}
        }
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        message.hash(&mut hasher);
        // A runaway has read its file to where it gave up: the command in
        // the user's files that started it is the place, not that end.
        let origin = match code {
            "runaway-argument" if !self.own_file(span.file) => self.project_site.or_else(|| self.user_origin(span)),
            _ => self.user_origin(span),
        };
        // An engine error is one row per place in the user's files it came
        // from, so each cause is shown.
        if TEX_ERRORS.iter().any(|(c, _)| *c == code) {
            origin.hash(&mut hasher);
        }
        let key = (code, span, hasher.finish());
        let diagnostics = &mut self.out.facts.diagnostics;
        if let Some(&i) = self.diag_seen.get(&key)
            && i >= self.diag_floor
            && let Some(d) = diagnostics.get_mut(i)
            && d.severity == severity
            && d.message == message
        {
            // The same gap at the same place over and over is one path
            // repeating itself, not new information: it is counted up to
            // `limits.repeat_failures`, and then the path is given up.
            let bound = self.cfg.limits.repeat_failures;
            let cuts =
                matches!(severity, Severity::Unsupported | Severity::Assumption) || CUT_IMPRECISE.contains(&code);
            self.undo.push(Undo::Count(i, d.count));
            if cuts && bound > 0 && !CUT_EXEMPT.contains(&code) {
                if d.count >= bound {
                    return;
                }
                d.count += 1;
                if d.count == bound {
                    self.scratch.repeat_cut = Some((code, span));
                }
                return;
            }
            d.count = d.count.saturating_add(1);
            return;
        }
        self.diag_seen.insert(key, diagnostics.len());
        let after = self.out.facts.occurrences.len();
        let diagnostics = &mut self.out.facts.diagnostics;
        diagnostics.push(Diagnostic { severity, code, message, span, count: 1, origin, after });
    }

    /// The innermost place in the user's own files on the input stack:
    /// `span` itself when it is one. A package being loaded ends the
    /// search, as its errors are its own.
    fn user_origin(&mut self, span: Span) -> Option<Span> {
        /// Frames looked through.
        const DEPTH: usize = 64;
        if self.own_file(span.file) {
            return Some(span);
        }
        let mut places: Vec<(Span, bool)> = Vec::new();
        for frame in self.input.iter().rev().take(DEPTH) {
            match frame {
                Frame::Tokens { toks, pos, .. } => {
                    if let Some(t) = toks.get(pos.saturating_sub(1)) {
                        places.push((t.span, false));
                    }
                }
                Frame::File { mouth, package, .. } => places.push((mouth.here(), package.is_some())),
                Frame::Pseudo { at, .. } => places.push((*at, false)),
                _ => {}
            }
        }
        for (place, package) in places {
            if self.own_file(place.file) {
                return Some(place);
            }
            if package {
                return None;
            }
        }
        None
    }

    /// The place in the user's own files being read right now, the
    /// innermost such file frame: where the call that led to `span` stands.
    /// `span` itself where no such frame is open.
    pub(crate) fn call_site(&mut self, span: Span) -> Span {
        /// Frames looked through.
        const DEPTH: usize = 256;
        let mut places: Vec<Span> = Vec::new();
        for frame in self.input.iter().rev().take(DEPTH) {
            if let Frame::File { mouth, .. } = frame {
                places.push(mouth.here());
            }
        }
        places.into_iter().find(|p| self.own_file(p.file)).unwrap_or(span)
    }

    /// Named things get a vertex too, so that a label, a citation or a
    /// section heading can be a slicing criterion.  The vertex is returned so
    /// that the caller can wire it to what the name stands for.
    pub fn occurrence(&mut self, kind: OccKind, key: String, detail: Option<String>, span: Span) -> Option<NodeId> {
        // A command that has acted has read its arguments.
        self.finish_shape();
        // A call that records the same key twice made one occurrence, but
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
            .find(|o| o.span == span && o.kind == kind && o.key == key && o.call == self.file_call)
            .map(|o| o.node);
        if seen.is_some() {
            return seen;
        }
        let context = self.occurrence_context();
        let node = self.occurrence_in(kind, key, detail, span, &context);
        Some(node)
    }

    /// What an occurrence recorded now would be attributed to. Kept for one
    /// recorded once the run has ended.
    pub fn occurrence_context(&self) -> crate::observe::OccContext {
        crate::observe::OccContext {
            package: self.package(),
            expanded: self.project_site,
            call: self.file_call,
            within: self.within.last().map(|(_, node)| *node),
            cds: self.cds.to_vec(),
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
        // A `\label` declares a key. A `\ref` or `\cite` uses one.
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
            call: context.call,
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
        self.cds.is_empty() && !self.on_paths() && self.mode.is_known() && !self.after_maybe_end()
    }

    /// The file being read may have ended before here ([`Machine::may_run`]).
    fn after_maybe_end(&self) -> bool {
        !self.may_have_ended.is_empty() && self.innermost_file().is_some_and(|f| self.may_have_ended.contains(&f))
    }

    fn innermost_file(&self) -> Option<FileId> {
        self.input.iter().rev().find_map(|f| match f {
            Frame::File { mouth, .. } => Some(mouth.here().file),
            _ => None,
        })
    }

    /// The macros currently expanding, outermost first: what `\errmessage`
    /// reached during any of these ran while they were on the way.
    pub(crate) fn call_stack(&self) -> impl Iterator<Item = Sym> + '_ {
        self.within.iter().map(|(sym, _)| *sym)
    }

    /// A `use` vertex for `sym`, wired to whatever currently defines it.
    ///
    /// Names that carry no definition site (the primitives the format
    /// provides, and undefined names) get no vertex: they would dominate the
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
        self.out.graph.push(VertexTag::Use, sym, span, within, self.cds.to_vec())
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
        /// How far back the half defined first may stand.
        const WINDOW: usize = 64;
        let defs = &mut self.out.facts.defs;
        let from = defs.len().saturating_sub(WINDOW);
        let mut found = false;
        for def in defs[from..].iter_mut().filter(|d| d.name == partner && d.span == span) {
            if matches!(def.tag, "macro" | "alias" | "environment") {
                def.tag = "environment";
                found = true;
            }
        }
        // A capture logs a definition as it is made, so the retag above
        // would not reach it: a run that loaded the cache would call the
        // half the macro it was first taken for.
        if let Some(capture) = &mut self.package_capture {
            let log = &mut capture.log.definitions;
            let from = log.len().saturating_sub(WINDOW);
            for (def, _) in log[from..].iter_mut().filter(|(d, _)| d.name == partner && d.span == span) {
                if matches!(def.tag, "macro" | "alias" | "environment") {
                    def.tag = "environment";
                }
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

    /// The names the kernel keeps counters under, learnt by running its
    /// own `\value{⟨counter⟩}` (ltcounts: `\csname c@#1\endcsname`) on a
    /// probe. None while `\value` is not a macro.
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

    pub fn define(&mut self, name: Sym, meaning: Meaning, by: Sym, mode: DefMode, span: Span) -> NodeId {
        // A command that has defined something has read its arguments.
        self.finish_shape();
        if let Meaning::Register(kind, index) = &meaning {
            self.note_register_origin(*kind, *index);
        }
        // A name made `\relax` reads as undefined (`\@ifundefined`): a
        // reference to a missing label leaves its `\r@⟨key⟩` so (hyperref's
        // `\NR@@setref`), which declares no key.
        if meaning.prim() != Some(Primitive::Relax) {
            crate::observe::defined_name(self, name);
        }
        let spelled = self.name(name).to_string();
        crate::observe::version_record(self, &spelled, &meaning);
        crate::observe::color_def(self, &spelled, &meaning, span);
        if let Some(line) = self.source_extent(span) {
            self.out.graph.note_extent(span, line);
        }
        let prefixes = self.take_prefixes();
        let global = prefixes.global;
        let own_name = self.name_token.take().is_some_and(|t| {
            let named = match t.tok {
                Tok::Cs(sym) => sym == name,
                Tok::Chr(c, Catcode::Active) => self.active_sym(c) == name,
                _ => false,
            };
            named && self.own_file(t.span.file) && !self.carried.foreign_names.contains(&(name, t.span))
        });
        let (redefines, relative) = match self.redefines_hint.take() {
            Some((hinted, before)) if hinted == name => before,
            _ => self.redefines(name),
        };
        let register = match &meaning {
            Meaning::Register(kind, _) => Some(*kind),
            _ => None,
        };
        let within = self.within.last().map(|(_, n)| *n);
        let node = self.out.graph.push(VertexTag::VariableDefinition, name, span, within, self.cds.to_vec());

        let switch = self.switch_of(&meaning, &spelled);
        let mac = meaning.as_macro().cloned();
        if let Some(m) = &mac {
            let body = self.out.graph.push(VertexTag::MacroDefinition, name, span, within, self.cds.to_vec());
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
        // replace. `\def` and `\newcommand` do not.
        if mode == DefMode::Renew {
            let defs = self.env.link_defs(name);
            self.link_reads(node, name, &defs);
        }

        // A replacement text with unknown text in it is one of many.
        // So is one made on a path before it has met the others again.
        let certain = self.cds.is_empty()
            && !self.on_paths()
            && !self.after_maybe_end()
            && !mac.as_ref().is_some_and(|m| self.has_unknown(&m.replacement_text));
        let environment = (spelled == "@currenvir").then(|| crate::observe::current_environment(self));
        let pattern = if name == self.unknown
            || name == self.unknown_digits
            || self.is_more_digits(name)
            || self.unknown_text_name(name)
        {
            Some((String::new(), String::new()))
        } else {
            self.name_pattern(name)
        };
        if pattern.as_ref().is_some_and(|(p, s)| p.is_empty() && s.is_empty()) {
            // Assumption: a name of wholly unknown text is a fresh one. It
            // redefines no name that has a meaning and no name spelled out
            // elsewhere; a lookup of unknown text stays unknown.
            self.diagnose(
                Severity::Imprecision,
                "unknown-name-defines-fresh",
                span,
                "a name built from unknown text was defined; it is assumed to be a new name".into(),
            );
        } else if let Some((prefix, suffix)) = pattern {
            self.wild.add(prefix, suffix, self.name_gap_is_digits(name), Some(meaning.clone()));
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
        // Kernel code re-run in a new environment (`\list`'s own `\def\@itemlabel{#1}`,
        // once per `\begin{itemize}`/`\begin{enumerate}`) hits the same site every time.
        // Without this, the first context seen there would be the only one `explain`
        // ever finds, so a context not already on record gets its own fact.
        let current_context: Rc<[Sym]> = Rc::from(self.out.env_stack.clone());
        let context_is_new = existing.is_some_and(|i| {
            self.out.facts.defs[i].context.as_ref() != current_context.as_ref()
                && !self
                    .out
                    .facts
                    .defs
                    .iter()
                    .rev()
                    .take(256)
                    .any(|d| d.name == name && d.span == span && d.context.as_ref() == current_context.as_ref())
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
                    (_, Some(rest)) => ("counter", Some(self.intern(&rest))),
                    _ if switch.is_some() => ("switch", switch),
                    (Some(crate::tex::RegKind::Skip), None) => ("length", None),
                    // clsguide § 4.5: `\DeclareOption{⟨option⟩}` stores the
                    // option's code as `\ds@⟨option⟩`, which `\ProcessOptions`
                    // runs.
                    _ if mac.is_some() && spelled.starts_with("ds@") && spelled.len() > 3 => {
                        ("option", Some(self.intern(&spelled[3..])))
                    }
                    _ => (self.env.meaning(by).prim().map_or("macro", category), None),
                };
                let (tag, subject) = match self.environment_half(&spelled, span) {
                    Some(base) if matches!(tag, "macro" | "alias") => ("environment", Some(base)),
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
                    depth: self.env.group_level() as u16,
                    global,
                    redefines,
                    certain,
                    cds: self.cds.to_vec(),
                    context: current_context,
                    via: self.within.last().map(|(sym, _)| *sym),
                    spelled: own_name,
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
                    self.carried.own_switches.insert(name);
                }
                if existing.is_none() {
                    self.definition_sites.insert((span, name), self.out.facts.defs.len());
                    self.out.facts.defs.push(fact);
                } else if context_is_new {
                    self.out.facts.defs.push(fact);
                }
            }
        }
        // A new context got its own fact above. The site's original fact
        // keeps its own `mac`, not this run's: a later local meaning must
        // not overwrite what the first one recorded.
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
        let mut labels: HashMap<(OccKind, &str), NodeId> = HashMap::default();
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

    /// Whether the expansion was written in the source macro's replacement
    /// text, not handed to it as an argument: its expansion reached this one.
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
        // A redefinition replaces what the name calls. Without clearing, a
        // scratch name the kernel reuses (`\reserved@a`, `\@gtempa`, ...)
        // would keep every callee any of its bodies ever had, inventing
        // cycles that `unguarded-recursion` and `recursion` report as real.
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
    pub fn sandbox(cfg: &'a Config, analysis: &Analysis, at: crate::probe::State) -> Option<Self> {
        use crate::probe::State;
        let (env, catcodes) = match (at, &analysis.preamble, &analysis.kernel) {
            (State::Preamble, Some(state), _) => (state.0.snapshot(), state.1.clone()),
            (State::Start, Some(state), Some(kernel)) => (kernel.snapshot(), state.1.clone()),
            (State::Preamble | State::Start, ..) => return None,
            (State::End, ..) => (analysis.env.snapshot(), analysis.catcodes.clone()),
        };
        let mut m = Machine::new(cfg, PathBuf::new());
        m.out.interner = analysis.interner.clone();
        m.out.files = analysis.files.clone();
        m.out.entry = analysis.entry.clone();
        m.out.document_depth = analysis.document_depth;
        m.env = env;
        m.catcodes = catcodes;
        m.dont_expand = m.out.interner.intern("\u{2}notexpanded:");
        m.unknown = m.out.interner.intern(crate::tex::UNKNOWN_CS);
        m.unknown_digits = m.out.interner.intern("\u{2}unknown digits:");
        m.unknown_more = m.out.interner.intern("\u{2}unknown more digits:");
        m.typed_syms = None;
        m.typed_syms();
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
        self.input.push(Frame::File {
            mouth: Mouth::new(text, file),
            package: None,
            depth: 0,
            conds: 0,
            eof_seen: false,
        });
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
        self.probe = Some(Box::new(crate::probe::Watch::new(file, sym)));
        self.input.push(Frame::File {
            mouth: Mouth::new(text, file),
            package: None,
            depth: 0,
            conds: 0,
            eof_seen: false,
        });
        let call = Token { tok: Tok::Cs(sym), span: Span::default() };
        self.push_tokens(Rc::from(vec![call]));
        self.run();
        let mut watch = *self.probe.take().expect("set above");
        watch.exhausted = watch.reached.is_none() && (self.halted || self.out.exhausted);
        watch.steps = self.out.steps;
        let interner = &self.out.interner;
        let catcodes = &self.catcodes;
        let typable = |t: &Token| match t.tok {
            Tok::Chr(c, cat) => cat == Catcode::Space || catcodes.get(c) == cat,
            _ => true,
        };
        watch.render(|toks| toks.iter().all(typable).then(|| crate::tex::detokenize(toks, interner)));
        watch
    }

    /// A probe run reached a prompt: the command read what its file has
    /// read so far, less what a lookahead gave back (§ 325), and the run
    /// ends there.
    pub(crate) fn probe_waits(&mut self) {
        let Some(file) = self.probe.as_ref().map(|w| w.file) else { return };
        let given = self.input.iter().filter_map(|f| if let Frame::Returned(ts) = f { Some(ts) } else { None });
        let given = given.flatten().filter(|t| t.span.file == file && self.recent_file_spans.contains(&t.span));
        let read = given
            .map(|t| t.span.col.saturating_sub(1) as usize)
            .min()
            .or_else(|| self.reading(file).map(|mouth| mouth.here().col.saturating_sub(1) as usize));
        if let Some(watch) = &mut self.probe {
            watch.reached = watch.reached.or(read);
        }
        self.halted = true;
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
        // Resolved afresh: a sandbox runs once per written line.
        self.join_commands.clear();
        self.join_hooks.clear();
        let points = self.cfg.analysis.join_paths_at.clone();
        for point in points {
            if let Some(name) = point.command {
                let sym = self.intern(name.strip_prefix('\\').unwrap_or(&name));
                self.join_commands.push(sym);
            }
            if let Some(hook) = point.hook {
                let sym = self.intern(&format!("__hook {hook}"));
                self.join_hooks.push(sym);
            }
        }
        self.drive(Unit::Command, |_| false);
        // Never silent: every path ended while a file was still being read,
        // with no fatal end and no budget hit.
        let open = self.input.iter().rev().find_map(|f| match f {
            Frame::File { mouth, .. } if !mouth.ending() => Some(mouth.here()),
            _ => None,
        });
        if self.cfg.verbose >= 4 {
            crate::progress::note!(
                "satex: run ended: halted {} exhausted {} levels {} open file {:?}",
                self.halted,
                self.out.exhausted,
                self.input.len(),
                open.map(|o| (o.file, o.line))
            );
        }
        if let Some(at) = open.filter(|_| !self.halted && !self.out.exhausted) {
            let span = Span { file: at.file, line: at.line, col: at.col };
            self.diagnose(
                Severity::Unsupported,
                "analysis-ended-early",
                span,
                "analysis ended early here: all paths were dropped before the input ended".into(),
            );
        }
    }

    /// Never silent: a project file an `\input`, `\include` or `\subfile`
    /// of the document's own files names, which exists but was never
    /// opened, is said where it is named.
    fn report_unread_inputs(&mut self) {
        let real = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        let read: HashSet<PathBuf> = self.out.files.iter().map(|f| real(Path::new(&f.path))).collect();
        let main =
            self.out.files.get(usize::from(self.out.main_file)).map(|f| crate::paths::dir_of(Path::new(&f.path)));
        let Some(main) = main else { return };
        let mut unread = Vec::new();
        let mut only = false;
        for id in 0..self.out.files.len() {
            let Ok(file) = FileId::try_from(id) else { break };
            if !self.project_file(file) {
                continue;
            }
            let path = PathBuf::from(&self.out.files[id].path);
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            only |= text.lines().any(|l| l.split('%').next().is_some_and(|c| c.contains("\\includeonly")));
            let dirs = [crate::paths::dir_of(&path), main.clone()];
            for (n, line) in text.lines().enumerate() {
                let code = line
                    .char_indices()
                    .find(|&(i, c)| c == '%' && !line[..i].ends_with('\\'))
                    .map_or(line, |(i, _)| &line[..i]);
                for cmd in ["\\input{", "\\include{", "\\subfile{"] {
                    for (at, _) in code.match_indices(cmd) {
                        let Some(name) = code[at + cmd.len()..].split('}').next().map(str::trim) else { continue };
                        // `\\includeonly` leaves the others out on purpose.
                        if name.is_empty() || name.contains(['#', '\\']) || (cmd == "\\include{" && only) {
                            continue;
                        }
                        let names = [format!("{name}.tex"), name.to_string()];
                        let found =
                            dirs.iter().flat_map(|d| names.iter().map(move |n| d.join(n))).find(|p| p.is_file());
                        let Some(found) = found else { continue };
                        if !read.contains(&real(&found)) {
                            let span = Span { file, line: n as u32 + 1, col: at as u32 + 1 };
                            unread.push((span, name.to_string()));
                        }
                    }
                }
            }
        }
        if self.cfg.verbose >= 4 {
            crate::progress::note!("satex: {} project inputs never read", unread.len());
        }
        for (span, name) in unread {
            self.diagnose(
                Severity::Unsupported,
                "file-not-read",
                span,
                format!("{name} exists but the analysis never read it: what it defines and typesets is missing"),
            );
        }
    }

    /// Whether the kernel format is being read: what it observes is not
    /// kept (only its meanings are), so per-token records can be skipped.
    pub(crate) fn reading_format(&self) -> bool {
        self.budget.is_some()
    }

    /// What a call site's repeat count is measured against: input read,
    /// or any assignment.  Only reading on is progress (tex.web § 366),
    /// but an assignment is what tells apart the many calls one site sees
    /// between two tokens of a file, which are not one loop.
    fn site_progress(&self) -> u64 {
        self.source_progress ^ self.env.assignments.rotate_left(32)
    }

    /// `-vv` and `-vvv`: say where the run is, so a long one does not look
    /// like a hang.
    fn report_progress(&mut self) {
        // Progress shows regardless of `--verbose`: of the one-time caching
        // work, or else of the document.
        if self.out.steps.is_multiple_of(2048) {
            let probing = self.probe.as_ref().map(|w| w.sym);
            let (label, frame, phase) = match (self.format_progress, self.recording_package()) {
                (Some(frame), _) => (format!("caching {} kernel", self.out.plugins.engine.as_str()), frame, "format"),
                (None, Some((name, frame))) => (format!("caching {name}"), frame, "package"),
                (None, None) => {
                    let Some(frame) = self.input.iter().position(|f| matches!(f, Frame::File { .. })) else { return };
                    let phase = match probing {
                        Some(_) => "probe",
                        None if self.out.document_depth.is_some() => "document",
                        None => "preamble",
                    };
                    (String::new(), frame, phase)
                }
            };
            // The file read now, when it is another one than the bar's.
            let (file, line) = self.position();
            // A package's frame may be gone while its recording ends.
            let own = match self.input.get(frame) {
                Some(Frame::File { mouth, .. }) => self.out.short_name(mouth.here().file).to_string(),
                _ => String::new(),
            };
            let label = match (label.is_empty(), probing) {
                (false, _) => label,
                (true, Some(sym)) => format!("probing \\{}", self.name(sym)),
                (true, None) => format!("analyzing {own} ({})", self.phase()),
            };
            let (read, all) = self.frame_read(frame).unwrap_or((1, 1));
            let limit = self.cfg.limits.seconds;
            let clock = (limit > 0).then(|| (self.started.elapsed().as_secs(), limit));
            // Lines read so far: the furthest line seen in every file.
            for f in &self.input {
                if let Frame::File { mouth, .. } = f {
                    let here = mouth.here();
                    let seen = self.carried.lines_seen.entry(here.file).or_default();
                    *seen = (*seen).max(here.line);
                }
            }
            // Data files read with `\read` count as read lines too.
            if let Some((stream, at, _, _)) = self.carried.stream_read
                && let Some(path) = self.carried.stream_paths.get(&stream)
            {
                let seen = self.carried.stream_lines.entry(path.clone()).or_default();
                *seen = (*seen).max(at as u64);
            }
            let lines: u64 = self.carried.lines_seen.values().map(|&l| u64::from(l)).sum::<u64>()
                + self.carried.stream_lines.values().sum::<u64>()
                + self.cached_lines;
            let lines = (lines, self.cached_lines);
            // The innermost file, when it is another than the job's own.
            let inner = self.input.iter().rposition(|f| matches!(f, Frame::File { .. }));
            let elsewhere = !(file == own || file.is_empty()) && inner.is_some_and(|i| i != frame);
            let path = match inner.and_then(|i| self.input.get(i)) {
                Some(Frame::File { mouth, .. }) => {
                    self.out.files.get(mouth.here().file as usize).map(|f| f.path.clone())
                }
                _ => None,
            }
            .unwrap_or_default();
            let mut subs = Vec::new();
            if let Some((reached, total)) = inner.filter(|_| elsewhere).and_then(|i| self.frame_read(i)) {
                subs.push(crate::progress::Sub { file: file.clone(), path: path.clone(), line, reached, total });
            }
            // A data file read line by line with `\read`, while it is being read.
            if let Some((stream, at, total, step)) = self.carried.stream_read
                && self.out.steps.saturating_sub(step) < 1_000_000
                && let Some(full) = self.carried.stream_paths.get(&stream)
            {
                let name =
                    std::path::Path::new(full).file_name().map_or(full.clone(), |n| n.to_string_lossy().into_owned());
                subs.push(crate::progress::Sub { file: name, path: full.clone(), line: at as u32, reached: at, total });
            }
            crate::progress::tell(crate::progress::Status {
                text: label.clone(),
                phase: phase.into(),
                file: if file.starts_with('\u{2}') { String::new() } else { file.clone() },
                line: line.into(),
                files: (self.carried.lines_seen.len() + self.carried.stream_lines.len()) as u64,
                lines: lines.0,
                cached: lines.1,
                ms: self.started.elapsed().as_millis() as u64,
                paths: self.waiting as u64 + 1,
                probe: probing.map(|sym| format!("\\{}", self.name(sym))).unwrap_or_default(),
                why: if probing.is_some() { crate::progress::why() } else { String::new() },
            });
            let label = if subs.is_empty() && !file.is_empty() { format!("{label}:{line}") } else { label };
            crate::progress::bar(&label, read, all, lines, clock, &subs);
        }
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
        let paths = if self.waiting > 0 { format!(", {} paths waiting", self.waiting) } else { String::new() };
        crate::progress::note!(
            "satex: {} tokens, {} MB, at {file}:{line} in {}{paths}",
            self.out.steps,
            crate::budget::in_use() >> 20,
            within.join(" < ")
        );
    }

    /// How far into the file a construct that began at `span` has read.
    /// Only meaningful when the tokens really came from that file: arguments
    /// taken from an expansion say nothing about the source.
    pub(crate) fn source_extent(&self, span: Span) -> Option<u32> {
        if self.last_file != Some(span.file) {
            return None;
        }
        self.input
            .iter()
            .rev()
            .find_map(|frame| match frame {
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
        let dir = crate::paths::dir_of(Path::new(main));
        self.out.files.get(usize::from(file)).is_some_and(|f| crate::paths::within(&dir, Path::new(&f.path)))
    }

    /// [`Machine::project_file`], asked once per file.
    pub(crate) fn own_file(&mut self, file: FileId) -> bool {
        let i = usize::from(file);
        if self.own_files.len() <= i {
            self.own_files.resize(i + 1, 0);
        }
        if self.own_files[i] == 0 {
            self.own_files[i] = if self.project_file(file) { 2 } else { 1 };
        }
        self.own_files[i] == 2
    }

    /// Where the file of `span` is being read, and the text from `span` to
    /// there: what the call at `span` has read of it so far.
    pub fn call_extent(&self, span: Span) -> Option<(Span, String)> {
        self.reading(span.file).map(|mouth| (mouth.here(), mouth.since(span.line, span.col)))
    }

    /// The innermost input level reading `file`.
    pub(crate) fn reading(&self, file: FileId) -> Option<&Mouth> {
        self.input.iter().rev().find_map(|frame| match frame {
            Frame::File { mouth, .. } if mouth.here().file == file => Some(mouth),
            _ => None,
        })
    }

    /// Where in the document the run is, for the progress bar: the preamble,
    /// the begin- or end-document hooks, or the body, and the environment
    /// it is in (`document · tikzpicture`).
    fn phase(&mut self) -> String {
        let hook = self.within.iter().rev().find_map(|(sym, _)| {
            let name = self.name(*sym);
            if name.contains("begindocument") || name == "@begindocumenthook" {
                Some("begin-document hooks")
            } else if name.contains("enddocument") || name == "@enddocumenthook" {
                Some("end-document hooks")
            } else {
                None
            }
        });
        let part = hook.unwrap_or(if self.out.document_depth.is_some() { "document" } else { "preamble" });
        match crate::observe::current_environment(self).filter(|e| e != "document" && !e.is_empty()) {
            Some(env) => format!("{part} · {env}"),
            None => part.to_string(),
        }
    }

    /// How much of its file input frame `i` has read, if it reads one.
    fn frame_read(&self, i: usize) -> Option<(usize, usize)> {
        match self.input.get(i)? {
            Frame::File { mouth, .. } => Some(mouth.read()),
            _ => None,
        }
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
        self.scratch.approximations_at_command = self.scratch.approximations;
        self.step()
    }

    /// One token from the input.  Returns `false` at end of input or budget.
    pub fn step(&mut self) -> bool {
        // A scan that expands what makes it scan again (`\def\a{\number\a}`) nests
        // without end; TeX runs out of input stack there (tex.web § 321, "TeX
        // capacity exceeded"), and so does the run. The stack grows down, and a
        // level's cost varies with the path to it, so bound stack bytes too, not just count.
        let marker = 0u8;
        let here = std::hint::black_box(std::ptr::addr_of!(marker)) as usize;
        if self.step_depth == 0 {
            self.stack_base = here;
        }
        let deep = match stack_low() {
            Some(low) => here < low + STACK_MARGIN,
            None => self.stack_base.saturating_sub(here) > MAX_STACK_BYTES,
        };
        if self.step_depth >= MAX_NESTING || deep {
            // satex's own nesting bound, not the document's: the expansion
            // that nests too deeply is given up like a stalled loop, and the
            // run reads on.
            // Unwind: returning `false` ends this nested step, so the caller
            // one level up goes on. Going on at this depth would give up
            // again on every step.
            self.abandon_nesting();
            return false;
        }
        self.step_depth += 1;
        let stepped = self.step_once();
        self.step_depth -= 1;
        stepped
    }

    /// Where the innermost file being read stands.
    pub(crate) fn position_span(&self) -> Span {
        self.input
            .iter()
            .rev()
            .find_map(|frame| match frame {
                Frame::File { mouth, .. } => Some(mouth.here()),
                _ => None,
            })
            .unwrap_or_default()
    }

    /// The limit `limits.<key>` cut the run at `at`: recorded once per key,
    /// with how often, so that a run can say which to raise.
    pub(crate) fn limit_reached(&mut self, key: &'static str, at: Span) {
        if let Some(hit) = self.out.limits.iter_mut().find(|h| h.key == key) {
            hit.count += 1;
            return;
        }
        let value = self.cfg.limits.value(key).unwrap_or_default();
        self.out.limits.push(LimitHit { key, value, at, count: 1 });
        self.diagnose(
            Severity::Unsupported,
            "limit-reached",
            at,
            format!("limits.{key} ({value}) was reached; what it cut is not in this analysis"),
        );
    }

    /// A cut is about to push its `\unknown` over the token lists it kept:
    /// they stay tainted until the file under them is read again.
    pub(crate) fn taint_after_cut(&mut self) {
        self.leak_call_recs();
        let file = self
            .input
            .iter()
            .rposition(|f| matches!(f, Frame::File { .. } | Frame::Pseudo { .. }))
            .map_or(0, |i| i + 1);
        self.cut_taint = Some(self.cut_taint.map_or(file, |t| t.min(file)));
    }

    /// Whether the input still holds token lists a cut left after its
    /// `\unknown` (see [`Machine::cut_taint`]).
    pub(crate) fn after_cut(&mut self) -> bool {
        match self.cut_taint {
            Some(floor)
                if self.input.len() > floor
                    && !matches!(self.input.last(), Some(Frame::File { .. } | Frame::Pseudo { .. })) =>
            {
                true
            }
            Some(_) => {
                self.cut_taint = None;
                false
            }
            None => false,
        }
    }

    /// A TeX error the running path raises on state nothing approximated,
    /// or with a cause that is `exact` whatever was approximated (a name
    /// certainly undefined): under `analysis.assume_compiles` the path is
    /// infeasible and dropped ([`Machine::explore`]).
    ///
    /// An error the path reached only past an approximation may not be
    /// TeX's: it is a possible error, and its path is dropped the same way
    /// where another path approximated nothing ([`Machine::explore`]),
    /// so what it left open (a group, a conditional, an argument scan that
    /// ran to its file's end) does not decide how the paths without one
    /// read on. Where no path goes on without an error, the dropped ones run
    /// on and TeX's recovery applies ([`Machine::settle`]).
    pub(crate) fn certain_error(&mut self, text: &str, exact: bool, span: Span) {
        if self.cfg.compiles()
            && self.probe.is_none()
            && !self.halted
            && !self.reading_format()
            && !self.keep_errors
            && self.dropping.is_none()
        {
            let certain = exact
                || (self.scratch.approximations == self.scratch.approximations_at_command
                    && !self.approximated
                    && !self.after_cut());
            // A message that prints unknown text was made from it.
            let certain = certain && !text.contains('\u{2}');
            // An error the document's own text makes is kept, possible or
            // certain: it is the one to report and fix. So is a certain one
            // library code raises while the document is read. Only library
            // code is assumed to compile.
            if self.own_file(span.file) || (certain && !self.reading_library()) {
                return;
            }
            self.dropping_possible = !certain;
            self.dropping = Some(match certain {
                true => text.to_string(),
                false => {
                    format!("{} (the analysis approximated on the way: may not be TeX's)", text.replace('\u{2}', "?"))
                }
            });
        }
    }

    /// Where an error or message is raised against: the command of the
    /// document's text that reached it, which may stand inside the argument
    /// of the file call (a frame's body), else that call.
    pub(crate) fn raised_at(&self, span: Span) -> Span {
        match (self.file_call, self.project_site) {
            (Some((_, call)), Some(site))
                if site.file == call.file && (site.line, site.col) > (call.line, call.col) =>
            {
                site
            }
            (Some((_, call)), _) => call,
            (None, _) => span,
        }
    }

    /// One of the engine's own errors, `text` as TeX prints it: raised
    /// against the document command that reached it, like `\errmessage`.
    fn tex_error(&mut self, text: &str, span: Span) {
        if self.probe.is_some() || self.halted {
            return;
        }
        let at = self.raised_at(span);
        self.note_raised(text, at);
        // Reported as `\errmessage`'s errors are (`raised-error`).
        self.occurrence(OccKind::Message, text.to_string(), Some("errmessage".to_string()), at);
        self.count_error(at);
    }

    /// tex.web § 82 `error`: one more error. The count is per paragraph
    /// (§ 1096 clears it). TeX ends the run at the hundredth. satex ends only
    /// the expansion it is in, like a stalled loop, and reads on from the
    /// file: errors its own approximations raised must not end the analysis
    /// of a document TeX compiles, and a loop of real errors still ends.
    pub(crate) fn count_error(&mut self, at: Span) {
        if self.probe.is_some() || self.halted {
            return;
        }
        self.tex_errors += 1;
        if self.tex_errors == MAX_ERRORS {
            let text = "That makes 100 errors; please try again";
            self.note_raised(text, at);
            self.occurrence(OccKind::Message, text.to_string(), Some("errmessage".to_string()), at);
            self.diagnose(
                Severity::Warning,
                "too-many-errors",
                at,
                "That makes 100 errors; please try again: TeX would stop here (tex.web § 82); satex gives up the expansion and reads on".into(),
            );
            self.tex_errors = 0;
            self.give_up_lists(false);
        }
    }

    /// tex.web § 395: an argument ran into a `}` that closes a group it did
    /// not open. TeX reports "Argument of \x has an extra }", puts the
    /// brace back and inserts `\par` before it, and makes the call not
    /// `\long` (`long_state:=call`), so the `\par` ends it as a runaway
    /// (§ 396): the call is dropped, and `\par }` is read next.
    fn extra_right_brace(&mut self, t: Token) {
        self.unread(t);
        let par = self.intern("par");
        self.unread(Token::new(Tok::Cs(par), t.span));
        self.long_argument = false;
        let of = self.call_shape.as_ref().map(|cs| format!(" of \\{}", self.name(cs.0))).unwrap_or_default();
        self.diagnose(Severity::Warning, "extra-right-brace", t.span, format!("Argument{of} has an extra }}"));
    }

    /// An unmatched `}` an argument scan meets in text a cut left after its
    /// `\unknown` ([`Machine::after_cut`]): the summarized code may have
    /// taken the `{` it closes as an argument, so this is no engine error.
    /// The call is dropped and `\par }` read next, as TeX's recovery
    /// (§ 395) leaves it, but without the error. The `}` is given back.
    fn cut_right_brace(&mut self, at: Span) -> Vec<Token> {
        let par = self.intern("par");
        self.unread(Token::new(Tok::Cs(par), at));
        self.runaway = Runaway::Cut;
        self.diagnose(
            Severity::Imprecision,
            "argument-ends-in-unknown",
            at,
            "an argument ran into a `}` of text a cut left, which the code cut may have taken; the call is dropped"
                .into(),
        );
        Vec::new()
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
        if let Some((code, at)) = self.scratch.repeat_cut.take() {
            self.abandon_repeat(code, at);
        }
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
        self.out.heat(|h| &mut h.steps, token.span);
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
        // Inline math whose size is followed holds only characters: any
        // other command may add atoms of unknown size.
        if self.natural.is_some() && self.mode.meets(crate::mode::Modes::ANY_MATH) {
            let plain = match token.tok {
                Tok::Cs(sym) => self.adds_no_atom(sym),
                Tok::Chr(c, Catcode::Active) => {
                    let sym = self.active_sym(c);
                    self.adds_no_atom(sym)
                }
                _ => matches!(
                    self.category_of(token),
                    Some(
                        Catcode::Letter
                            | Catcode::Other
                            | Catcode::Space
                            | Catcode::Math
                            | Catcode::Begin
                            | Catcode::End
                    )
                ),
            };
            // A script is one character here: expansion, spaces and
            // `\relax` may come before it (tex.web §§ 404, 1151). A math
            // character given by name is that character (§ 1151).
            let script = self.natural.is_some_and(|n| n.script.is_some())
                && match token.tok {
                    Tok::Cs(sym) => {
                        !self.expandable(sym)
                            && self.env.meaning(sym) != Meaning::Primitive(crate::builtins::Primitive::Relax)
                            && !self.script_transparent(sym)
                            && !self.names_math_char(sym)
                    }
                    // `{` opens a script group (`SCRIPT_GROUP` in `crate::mode`).
                    _ => !matches!(
                        self.category_of(token),
                        Some(Catcode::Letter | Catcode::Other | Catcode::Space | Catcode::Begin)
                    ),
                };
            match self.category_of(token) {
                Some(Catcode::Sup) => self.math_script(true),
                Some(Catcode::Sub) => self.math_script(false),
                _ if !plain || script => self.natural = None,
                _ => {}
            }
        }
        match token.tok {
            Tok::Cs(sym) => {
                // A built name parts per meaning and goes on in this step:
                // given back, its paths would join at once, undoing it.
                if command && !self.split_family(sym, token.span) && self.split_one_of(token) {
                    return true;
                }
                if command {
                    self.split_meaning(sym, token.span);
                }
                self.command_level = command
                    && matches!(self.env.meaning(sym).prim(), Some(Primitive::If(_) | Primitive::SkipCrossing { .. }));
                self.do_control_sequence(sym, token.span);
                self.command_level = false;
            }
            // tex.web § 344: an active character acts as the control sequence
            // of that single character, while staying a character token.
            Tok::Chr(c, Catcode::Active) => {
                let sym = self.active_sym(c);
                if command {
                    self.split_meaning(sym, token.span);
                }
                self.do_control_sequence(sym, token.span);
            }
            _ => self.do_character(token),
        }
        true
    }

    /// A character token that is not typeset material: the ones that open and
    /// close a group.  A control sequence `\let` to such a character comes
    /// here too (tex.web § 1063).
    fn do_character(&mut self, token: Token) {
        // tex.web § 1211 skips spaces after a prefix.
        if self.category_of(token) != Some(Catcode::Space) {
            self.drop_prefixes(None);
        }
        match self.category_of(token) {
            Some(Catcode::Begin) => {
                self.end_word();
                crate::observe::word_break(self);
                // tex.web § 785: a `{` between an `\halign`'s rows begins
                // a row, and its group is in that row's first cell.
                self.align_cell(false);
                self.open_group(GroupKind::Simple, token.span);
                self.record(Step::OpenGroup, Sym(0), token.span);
                self.simple_group_opened();
            }
            Some(Catcode::End) => {
                self.end_word();
                // A plain group in text sets nothing between its characters
                // and the next (`12{,}5`, the `{}` of `~`); the end of a
                // list's group or a math group does (`\sum_{i=1}^{N}`).
                let plain = self.env.group_kind() == Some(GroupKind::Simple)
                    && self.env.nest().is_none()
                    && !self.mode.meets(crate::mode::Modes::ANY_MATH);
                match plain {
                    true => crate::observe::word_break(self),
                    false => crate::observe::text_break(self),
                }
                self.close_group(GroupKind::Simple, token);
                self.record(Step::CloseGroup, Sym(0), token.span);
            }
            Some(Catcode::Math) => {
                crate::observe::text_break(self);
                // tex.web § 785: so does a `$`.
                self.align_cell(false);
                self.math_shift_token(token)
            }
            // tex.web § 1128: an `&` with no alignment open is deleted.
            Some(Catcode::Tab) if !self.mode.is_empty() && !self.env.in_alignment() => {
                self.diagnose(
                    Severity::Warning,
                    "misplaced-tab",
                    token.span,
                    "an `&` stands where no alignment is open".into(),
                );
            }
            // tex.web § 785: an `&` ends a cell, and between rows begins one.
            Some(Catcode::Tab) => {
                if !self.template_cell_end(token, false) {
                    self.end_word();
                    self.align_cell(true);
                    self.template_next_cell(false, token.span);
                }
            }
            Some(Catcode::Sup | Catcode::Sub) if self.insert_dollar(token) => {}
            // tex.web § 785: spaces between rows are skipped.
            Some(Catcode::Space) if self.between_rows() => {}
            // tex.web § 1043: a space in horizontal mode is glue; vertical
            // mode ignores it, so it sets nothing.
            Some(Catcode::Space) => {
                if !self.mode.within(crate::mode::Modes::ANY_VERTICAL) {
                    self.note_text(' ', token.span);
                }
                if self.packages.is_empty() {
                    let modes = self.out.facts.spaces.entry(token.span).or_default();
                    *modes = modes.union(self.mode);
                }
                if self.mode.meets(crate::mode::Modes::ANY_HORIZONTAL) {
                    let (width, spec) = self.space_glue(false);
                    self.append_item(crate::mode::Item::glue(width, spec));
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

    /// Every collection the run appends to has a ceiling. The first one that
    /// is reached stops the analysis instead of growing the process.
    /// Trade precision for finishing, as the budget runs low (off by
    /// default): past `degrade_at_percent` of `limits.steps`, and again
    /// at each further third of what is left, every limit that only costs
    /// time is halved from what `cfg.limits` says (down to 1): paths and
    /// sets kept apart, choices per command, loop and recursion widening,
    /// call summaries and the fork and line budgets. So the work left per
    /// line shrinks as the budget does and the run converges instead of
    /// stopping. Every result stays an over-approximation: a lower limit
    /// only joins, widens or cuts sooner.
    fn degrade(&mut self, key: &'static str, used: u64) {
        let start = u64::from(self.limits.degrade_at_percent);
        if start >= 100 || used < start {
            return;
        }
        let stage = (1 + (used - start) * 3 / (100 - start)).min(3) as u8;
        if stage <= self.pressure {
            return;
        }
        self.pressure = stage;
        let base = &self.cfg.limits;
        let half = |n: u16| (n >> stage).max(1);
        let limits = &mut self.limits;
        limits.paths = half(base.paths);
        limits.alike = half(base.alike);
        limits.heads_apart = half(base.heads_apart);
        limits.choices = half(base.choices);
        limits.meaning_splits = base.meaning_splits >> stage;
        limits.widen_after = half(base.widen_after);
        limits.recursion_depth = half(base.recursion_depth);
        limits.call_summary_after = match base.call_summary_after {
            0 => 0,
            n => half(n),
        };
        limits.meaning_set = half(base.meaning_set);
        limits.value_set = half(base.value_set);
        limits.interval_set = half(base.interval_set);
        limits.join_arms = half(base.join_arms);
        // The budgets halve twice as fast: they bound work, not precision.
        limits.site_forks = match base.site_forks {
            0 => 0,
            n => (n >> (2 * stage)).max(1),
        };
        limits.line_steps = match base.line_steps {
            0 => 0,
            n => (n >> (2 * stage)).max(1),
        };
        self.env.sets = crate::env::SetLimits {
            meanings: usize::from(limits.meaning_set),
            arms: usize::from(limits.join_arms),
            values: usize::from(limits.value_set),
            intervals: usize::from(limits.interval_set),
        };
        // Reported when the run ends: a replayed command rolls back what it
        // diagnosed, and this must not be rolled back with it.
        if self.degraded_at.is_none() {
            self.degraded_at = Some((key, self.position_span()));
        }
    }

    /// The one place work besides reading tokens is paid: a driver
    /// iteration that read no token (a re-meet, a summary pass, a join, a
    /// resume) costs one step. So `limits.steps` bounds every loop.
    pub(super) fn charge(&mut self, mark: &mut u64) {
        if self.out.steps == *mark {
            self.out.steps += 1;
        }
        *mark = self.out.steps;
        if self.out.steps.wrapping_sub(self.checked_at) >= CHECK_INTERVAL || self.out.steps >= self.cfg.limits.steps {
            self.checked_at = self.out.steps;
            self.check_budget();
        }
    }

    fn over_budget(&mut self) -> bool {
        if self.halted {
            return true;
        }
        if self.out.steps < self.cfg.limits.steps && self.out.steps.wrapping_sub(self.checked_at) < CHECK_INTERVAL {
            return false;
        }
        self.checked_at = self.out.steps;
        self.check_budget()
    }

    fn check_budget(&mut self) -> bool {
        if self.halted {
            return true;
        }
        if crate::probe::preempted() {
            self.halted = true;
            self.out.exhausted = true;
            return true;
        }
        if self.guard_line() {
            return true;
        }
        // Degraded at a share of the steps only: seconds and memory differ
        // between runs, so they only stop the run, never steer it.
        if self.budget.is_none() {
            let used = u128::from(self.out.steps) * 100 / u128::from(self.cfg.limits.steps.max(1));
            self.degrade("steps", used.min(100) as u64);
        }
        // Sizes a region can grow and give back: past them the region is
        // cut as the guard cuts a line, and the run reads on.
        let limits = &self.cfg.limits;
        let size = [
            (self.input.len() >= limits.expansion_stack, "expansion_stack"),
            (self.held_tokens >= limits.held_tokens, "held_tokens"),
            (self.env.journal_len() >= limits.save_stack, "save_stack"),
        ]
        .into_iter()
        .find_map(|(over, key)| over.then_some(key));
        if let Some(key) = size {
            self.cut_region(key);
        }
        // A full graph only stops growing: the analysis goes on.
        if self.out.graph.full() && !self.graph_full_noted {
            self.graph_full_noted = true;
            let at = self.input.iter().rev().find_map(|f| match f {
                Frame::File { mouth, .. } => Some(mouth.here()),
                _ => None,
            });
            self.limit_reached("vertices", at.unwrap_or_default());
        }
        let facts = &self.out.facts;
        let used = facts.defs.len() + facts.expansions.len() + facts.occurrences.len();
        let max_steps = self.budget.unwrap_or(self.cfg.limits.steps);
        let (reason, key) = if self.out.steps >= max_steps {
            ("token budget", if self.budget.is_some() { "format_steps" } else { "steps" })
        } else if used >= self.cfg.limits.facts {
            ("fact budget", "facts")
        } else if self.cfg.limits.seconds > 0 && self.started.elapsed().as_secs() >= self.cfg.limits.seconds {
            ("time limit", "seconds")
        } else if self.cfg.limits.memory.as_u64() > 0
            && crate::budget::in_use() as u64 >= self.cfg.limits.memory.as_u64()
        {
            ("memory limit", "memory")
        } else {
            return false;
        };
        self.halted = true;
        self.out.exhausted = true;
        let at = self.position_span();
        self.diagnose(
            Severity::Unsupported,
            "budget-exhausted",
            Span::default(),
            format!("analysis stopped: {reason} (limits.{key}) exceeded"),
        );
        self.limit_reached(key, at);
        true
    }

    /// Give up every token list being read, down to the file, and close
    /// what their text opened ([`Machine::close_since`]): its closers went
    /// with it, so the file's next `}`/`\fi` would close one of its levels
    /// instead of its own (tex.web § 336 balances a file end the same way).
    /// With `own`, down to the first list that still holds the document's
    /// text instead. Returns the groups and conditionals closed.
    fn give_up_lists(&mut self, own: bool) -> (usize, usize) {
        let mut at = None;
        loop {
            let pending = match self.input.last() {
                Some(Frame::Pseudo { .. }) if !own => Vec::new(),
                last => match last.and_then(list_files) {
                    Some(files) => files,
                    None => break,
                },
            };
            if own && pending.into_iter().any(|f| self.own_file(f)) {
                break;
            }
            at = explore::lower(at, self.input.last());
            self.pop_frame();
        }
        self.mark_cut_scans();
        let closed = at.map_or((0, 0), |at| self.close_since(at));
        // What the dropped calls assigned and the closed groups left holds
        // anything of its kind; values the groups gave back stay.
        self.widen_since(at.map_or(0, |at| at.2));
        closed
    }

    /// Close the conditionals above `at.0` and the groups opened after
    /// group `at.1`, as their closers would (tex.web §§ 281-283, 496);
    /// what they put back that was written since assignment `at.2` is
    /// widened ([`crate::env::Env::close_since`]). Returns the groups and conditionals
    /// closed.
    pub(crate) fn close_since(&mut self, at: (usize, u64, u64)) -> (usize, usize) {
        let conds = self.conds.len().saturating_sub(at.0);
        for _ in 0..conds {
            self.pop_conditional();
        }
        // A list's group gives back the modes around it (tex.web § 1085);
        // what the dropped text would have put on the list is unknown.
        let span = self.position_span();
        let mut groups = 0;
        loop {
            let nest = self.env.nest();
            let syms = self.typed_syms();
            let typed = crate::env::typed!(self, syms);
            if !self.env.close_since(at.1, at.2, &mut self.catcodes, &typed) {
                break;
            }
            self.group_dropped(nest, span);
            groups += 1;
        }
        (groups, conds)
    }

    /// A group closed without its closer being read: it gives back the
    /// modes around its list, whose contents are unknown (tex.web § 1085),
    /// and an output routine ends with it, the rest of its list unread
    /// (§ 1100).
    pub(crate) fn group_dropped(&mut self, nest: Option<crate::mode::Nest>, span: Span) {
        let output = nest.as_ref().is_some_and(|n| n.code == Some(crate::mode::group::OUTPUT));
        if nest.is_some() {
            self.natural = None;
            self.group_closed(nest, span);
        }
        if output {
            let sym = self.intern("output");
            let list =
                self.input.iter().rposition(|f| matches!(f, Frame::Tokens { expanding: Some(s), .. } if *s == sym));
            while list.is_some_and(|i| self.input.len() > i) {
                self.pop_frame();
            }
        }
    }

    /// A scan begins: where the input stands, read levels not counted.
    pub(crate) fn scan_begin(&mut self) -> usize {
        let mut n = self.input.len();
        while n > 0
            && match &self.input[n - 1] {
                Frame::Tokens { toks, pos, .. } => *pos >= toks.len(),
                Frame::Returned(t) => t.is_empty(),
                _ => false,
            }
        {
            n -= 1;
        }
        self.scan_starts.push((n, false));
        self.scan_starts.len() - 1
    }

    /// A scan ends: whether a cut dropped the text it began in. Its reads
    /// then found the input ended, as at the budget, so it holds what it
    /// read before the cut, and the rest is unknown.
    pub(crate) fn scan_end(&mut self, id: usize) -> bool {
        let cut = self.scan_starts.get(id).is_some_and(|s| s.1);
        self.scan_starts.truncate(id);
        if cut {
            let at = self.position_span();
            self.diagnose(
                Severity::Imprecision,
                "scan-cut-at-resync",
                at,
                "a scan that began in text the analysis gave up ends there; what it would have read is unknown".into(),
            );
        }
        cut
    }

    /// The scans that began in token lists a cut just dropped.
    pub(super) fn mark_cut_scans(&mut self) {
        let n = self.input.len();
        for s in &mut self.scan_starts {
            s.1 |= s.0 > n;
            self.scan_cut |= s.1;
        }
    }

    /// The note a give-up adds for what [`Machine::give_up_lists`] closed.
    fn closed_note((groups, closed): (usize, usize)) -> String {
        match (groups, closed) {
            (0, 0) => String::new(),
            (g, 0) => format!(", and the {g} group(s) it opened were closed"),
            (0, c) => format!(", and the {c} conditional(s) it opened were closed"),
            (g, c) => format!(", and the {g} group(s) and {c} conditional(s) it opened were closed"),
        }
    }

    /// Where the innermost file being read stands on the input stack.
    pub(crate) fn file_level(&self) -> Option<usize> {
        self.input.iter().rposition(|f| matches!(f, Frame::File { .. } | Frame::Pseudo { .. }))
    }

    /// A possible `error` library code raised on a path that approximated,
    /// with no exact path beside it: TeX's recovery would read on into the
    /// caller's text. The call is summarized instead: its pending lists go,
    /// down to the file or a list holding the document's text, what it
    /// assigned is unknown, and an unknown token stands for the rest.
    pub(crate) fn contain_fault(&mut self, error: &str, file: Option<usize>) {
        // Nothing of the call's code is left to read (its file ended, its
        // lists are done, put-back tokens aside): TeX's recovery is all.
        if self.file_level() < file {
            return;
        }
        let lists = self.input.iter().rev().take_while(|f| list_files(f).is_some());
        let pending =
            lists.filter(|f| matches!(f, Frame::Tokens { .. })).filter_map(list_files).find(|t| !t.is_empty());
        if pending.is_none_or(|files| files.into_iter().any(|f| self.own_file(f))) {
            return;
        }
        let span = self.position_span();
        let balanced = Self::closed_note(self.give_up_lists(true));
        self.diagnose(
            Severity::Imprecision,
            "fault-contained",
            span,
            format!(
                "library code raised `{}`, maybe only on approximated values; the call is summarized: what it assigned is unknown{balanced}",
                error.lines().next().unwrap_or_default()
            ),
        );
        self.approximated = true;
        self.taint_after_cut();
        let unknown = self.unknown_text(Unk::Any, span);
        self.push_tokens(Rc::from(vec![unknown]));
    }

    /// An expansion nested deeper than satex follows: its assignments become
    /// unknown, its pending lists are dropped, and the run reads on.
    fn abandon_nesting(&mut self) {
        let balanced = Self::closed_note(self.give_up_lists(false));
        let (file, line) = self.position();
        self.diagnose(
            Severity::Unsupported,
            "nesting-widened",
            Span::default(),
            format!(
                "an expansion at {file}:{line} nested deeper than satex follows; what it assigned is unknown{balanced}"
            ),
        );
    }

    /// The rest of the line at `at` is dropped, said as a warning: whether
    /// there was one.
    pub(crate) fn skip_line(&mut self, at: Span) -> bool {
        let Some(i) = self.input.iter().rposition(|f| matches!(f, Frame::File { .. })) else { return false };
        let Frame::File { mouth, .. } = &mut self.input[i] else { return false };
        if mouth.file != at.file {
            return false;
        }
        let Some((line, col)) = mouth.skip_line() else { return false };
        let file = self.out.short_name(at.file).to_string();
        self.diagnose(
            Severity::Warning,
            "stalled-line-skipped",
            Span::new(at.file, line, col),
            format!("analysis was cut twice at {file}:{line}; the rest of the line from column {col} was not analyzed"),
        );
        true
    }

    fn guard_key(&self, at: Span) -> GuardKey {
        let (stream, line) = self.carried.stream_read.map_or((0, 0), |(s, l, ..)| (s, l));
        (at.file, at.line, stream, line)
    }

    /// The termination guard, met on every check whatever path, scan or
    /// replay the step is in: the steps since the last check go to the
    /// source line read now (and the line `\read` took last), over all paths. Past `limits.line_steps`
    /// there, the expansion above the line is cut (as a skipped region, read
    /// on with an unknown token); past it again, the rest of the line is
    /// skipped; a third time, or with no line left to skip, the run stops.
    /// A path parked on the line before a cut takes it when it runs again.
    /// Whether the run stopped.
    fn guard_line(&mut self) -> bool {
        let spent = self.out.steps.saturating_sub(self.guard.step);
        self.guard.step = self.out.steps;
        let limit = self.limits.line_steps;
        if limit == 0 || self.budget.is_some() || self.halted {
            return false;
        }
        let at = self.position_span();
        let key = self.guard_key(at);
        let entry = self.guard.lines.entry(key).or_default();
        entry.0 += spent;
        if entry.0 > self.guard.peak {
            let crossed = entry.0.ilog2() > self.guard.peak.max(1).ilog2();
            self.guard.peak = entry.0;
            if self.cfg.verbose >= 4 && crossed {
                crate::progress::note!("satex: line {at:?} took {} steps", entry.0);
            }
        }
        if self.guard_seen.line != key {
            self.guard_seen.line = key;
            self.guard_seen.level = entry.1;
        }
        let behind = self.guard_seen.level < entry.1;
        if !behind && entry.0 <= limit {
            return false;
        }
        if !behind {
            entry.0 = 0;
            entry.1 += 1;
        }
        let level = entry.1;
        self.guard_seen.level = level;
        let file = self.out.short_name(at.file).to_string();
        if !behind {
            self.limit_reached("line_steps", at);
        }
        if level == 1 && !behind {
            self.diagnose(
                Severity::Unsupported,
                "line-budget-cut",
                at,
                format!("analysis spent more than {limit} steps at {file}:{}; the expansion it was in was cut: what it assigned is unknown", at.line),
            );
        }
        let skipped = level >= 2 && self.skip_line(at);
        if !behind && (level >= 3 || (level == 2 && !skipped)) {
            self.halted = true;
            self.out.exhausted = true;
            self.diagnose(
                Severity::Unsupported,
                "budget-exhausted",
                at,
                format!("analysis stopped: {file}:{} overran limits.line_steps after its rest was skipped", at.line),
            );
            return true;
        }
        self.cut();
        false
    }

    /// The choices made at `site` while this source line is read, as
    /// [`Machine::hot_site`] counts them.
    pub(crate) fn site_forked(&self, site: Span) -> u32 {
        self.guard.forks.get(&(self.guard_key(self.position_span()), site)).copied().unwrap_or(0)
    }

    /// A choice at `site`: counted per source line, over all paths and
    /// replays. Past `limits.site_forks` the expansion above the line is
    /// cut as the guard cuts it ([`Machine::cut`]) on every path that
    /// stands on the line, instead of parting again: what it assigned is
    /// unknown, and the line reads on after an unknown token. Whether it
    /// was cut.
    pub(crate) fn hot_site(&mut self, site: Span) -> bool {
        let limit = self.limits.site_forks;
        if limit == 0 || self.budget.is_some() {
            return false;
        }
        let key = (self.guard_key(self.position_span()), site);
        let count = self.guard.forks.entry(key).or_default();
        *count = count.saturating_add(1);
        if *count <= limit {
            return false;
        }
        if !self.guard_seen.cut {
            let file = self.out.short_name(site.file).to_string();
            let at = self.position_span();
            if self.cfg.verbose >= 4 {
                crate::progress::note!(
                    "satex: site {file}:{}:{} cut at {at:?}, {} paths waiting",
                    site.line,
                    site.col,
                    self.waiting
                );
            }
            self.diagnose(
                Severity::Imprecision,
                "site-forks-cut",
                site,
                format!(
                    "more than {limit} undecided choices were made at {file}:{} while line {} was read; the expansion they were in was cut: what it assigned is unknown",
                    site.line, at.line
                ),
            );
            // The line's first overrun, as the guard counts it: every path
            // parked on it takes the cut when it runs again.
            let line = self.guard_key(at);
            let entry = self.guard.lines.entry(line).or_default();
            if entry.1 == 0 {
                *entry = (0, 1);
            }
            (self.guard_seen.line, self.guard_seen.level) = (line, entry.1);
            self.cut();
        }
        true
    }

    /// The region past a size limit is cut ([`Machine::cut`]), said once.
    fn cut_region(&mut self, key: &'static str) {
        if !self.guard_seen.cut {
            let at = self.region.span;
            let file = self.out.short_name(at.file).to_string();
            self.diagnose(
                Severity::Imprecision,
                "region-skipped",
                at,
                format!("analysis skipped the rest of the expansion after {file}:{}: it passed limits.{key}; what it assigned is unknown", at.line),
            );
            self.limit_reached(key, at);
        }
        self.cut();
    }

    /// Unwind to the main loop, which reads on after an unknown token
    /// ([`Machine::resync`]). Met again before that, the unwinding is cut there.
    fn cut(&mut self) {
        if std::mem::replace(&mut self.guard_seen.cut, true) {
            self.resync();
        }
    }

    /// Cut where the run stands and read on at once ([`Machine::resync`]).
    pub(super) fn resync_now(&mut self) {
        self.guard_seen.cut = true;
        self.resync();
    }

    /// The main loop reads on after a skipped region: what was assigned since
    /// the last source token is unknown, the token lists left are dropped
    /// down to the file, and categories changed since are put back, an
    /// assumption.
    fn resync(&mut self) {
        self.leak_call_recs();
        self.give_up_lists(false);
        if std::mem::take(&mut self.guard_seen.cut) {
            let at = self.position_span();
            let unknown = self.unknown_text(Unk::Any, at);
            self.push_tokens(Rc::from(vec![unknown]));
        }
        let changed = std::mem::take(&mut self.region.catcodes);
        for &(c, cat, may) in &changed {
            self.catcodes.restore(c, cat, may);
        }
        if !changed.is_empty() {
            let chars: String = changed.iter().map(|r| r.0).collect();
            self.diagnose(
                Severity::Assumption,
                "region-catcodes",
                self.region.span,
                format!("a skipped expansion changed the category of {chars:?}; they are assumed put back"),
            );
        }
    }

    /// A gap met `limits.repeat_failures` times at one place: said once.
    /// The loop it is in is bounded by the loop summaries, the stall cut
    /// and the region share, which keep a continuation.
    fn abandon_repeat(&mut self, code: &'static str, at: Span) {
        let file = self.out.short_name(at.file).to_string();
        self.diagnose(
            Severity::Unsupported,
            "repeated-failure",
            at,
            format!("{code} at {file}:{} was met {} times", at.line, self.cfg.limits.repeat_failures),
        );
    }

    /// Run a token list to completion, then return.
    pub fn run_tokens(&mut self, toks: Rc<[Token]>) {
        if toks.is_empty() {
            return;
        }
        self.input.push(Frame::Boundary);
        let base = self.input.len();
        self.push_tokens(toks);
        self.drive(Unit::Command, |m| m.input.len() <= base);
        while self.input.len() >= base {
            self.pop_frame();
        }
    }

    /// Whether the gullet expands this control sequence: TeX calls `expand`
    /// only for the commands above `max_command` (tex.web § 366).
    /// A command that puts nothing on a math list: an expansion, an
    /// assignment other than a math character, a prefix, `\relax`, a
    /// semi-simple group, a message or a deferred token.
    /// Before a script: what adds nothing to the list, so the script is
    /// still the next atom (whatsits, assignments, prefixes).
    fn script_transparent(&self, sym: Sym) -> bool {
        use crate::builtins::{PdfOp, Primitive as P, Syntax};
        match self.env.meaning(sym) {
            Meaning::Primitive(
                P::Pdf(PdfOp::Literal | PdfOp::Special)
                | P::Command(Syntax::PdfColorStack | Syntax::Whatsit | Syntax::OpenOut | Syntax::CloseOut)
                | P::Prefix(_)
                | P::IgnoreSpaces
                | P::Write
                | P::Message { .. },
            ) => true,
            Meaning::Primitive(p) => crate::builtins::assignment(p),
            Meaning::Register(kind, _) => {
                !matches!(kind, crate::tex::RegKind::MathChar | crate::tex::RegKind::UMathChar)
            }
            _ => false,
        }
    }

    /// A `\mathchardef` constant, `\mathchar` or `\delimiter`: one math
    /// character, which `math_code_char` sets (tex.web § 1151).
    fn names_math_char(&self, sym: Sym) -> bool {
        matches!(self.env.meaning(sym), Meaning::Register(crate::tex::RegKind::MathChar, _))
            || self.math_char_command(sym).is_some()
    }

    fn adds_no_atom(&self, sym: Sym) -> bool {
        use crate::builtins::Primitive as P;
        match self.env.meaning(sym) {
            Meaning::Primitive(P::Register(crate::tex::RegKind::MathChar | crate::tex::RegKind::UMathChar)) => false,
            // A `\mathchardef` constant is one atom, which `math_code_char`
            // adds, and so are `\mathchar` and `\delimiter` in inline math.
            Meaning::Register(crate::tex::RegKind::MathChar, _) => true,
            Meaning::Primitive(P::Command(crate::builtins::Syntax::Number))
                if self.math_char_command(sym).is_some() =>
            {
                true
            }
            Meaning::Primitive(
                P::Prefix(_)
                | P::BeginGroup
                | P::EndGroup
                | P::Relax
                | P::Message { .. }
                | P::Defer(_)
                | P::IgnoreSpaces
                | P::Write,
            ) => true,
            // tex.web §§ 1275, 1285-1288: `\openin`, `\closein` and a case
            // change act at once and leave no node, as `\pdffontexpand`
            // (pdfTeX manual, "Character spacing") does.
            Meaning::Primitive(
                P::OpenIn | P::CloseIn | P::CaseShift(_) | P::Command(crate::builtins::Syntax::FontExpand),
            ) => true,
            // A type for the next atom, or limits on the last (tex.web §§ 1156, 1159).
            Meaning::Primitive(P::Command(crate::builtins::Syntax::MathComp(_) | crate::builtins::Syntax::Limits)) => {
                true
            }
            // Glue and kerns, `\mskip` and `\mkern` in math units, are no
            // atoms: `append_item` adds their width (tex.web §§ 732, 761).
            // `\unskip`, `\unkern` and `\unpenalty` take an item away or
            // nothing, which `remove_item` follows (tex.web § 1105).
            Meaning::Primitive(
                P::Typeset(
                    crate::builtins::Typeset::Glue | crate::builtins::Typeset::Dimen | crate::builtins::Typeset::Plain,
                )
                | P::Mode(crate::builtins::ModeCmd::Horizontal(crate::builtins::Material::Glue)),
            ) => true,
            // A whatsit is no atom and has no size (tex.web § 730).
            Meaning::Primitive(
                P::Pdf(
                    crate::builtins::PdfOp::Literal
                    | crate::builtins::PdfOp::Special
                    | crate::builtins::PdfOp::Object
                    | crate::builtins::PdfOp::Resources,
                )
                | P::Command(
                    crate::builtins::Syntax::PdfColorStack
                    | crate::builtins::Syntax::PdfStartLink
                    | crate::builtins::Syntax::Whatsit
                    | crate::builtins::Syntax::OpenOut
                    | crate::builtins::Syntax::CloseOut,
                ),
            ) => true,
            // `\leavevmode`'s `\unhbox` of a void box adds nothing; any
            // other unpacking in math loses the size where it runs (§ 1110).
            Meaning::Primitive(P::Mode(crate::builtins::ModeCmd::Horizontal(
                crate::builtins::Material::Unpackage { .. },
            ))) => true,
            // A style node, and a choice of lists the style picks, which
            // `mode_command` follows (tex.web §§ 731, 1171-1174).
            Meaning::Primitive(P::Mode(
                crate::builtins::ModeCmd::MathStyle(_) | crate::builtins::ModeCmd::MathChoice,
            )) => true,
            // A box is an ordinary atom, whose size `append_item` adds.
            Meaning::Primitive(P::Mode(crate::builtins::ModeCmd::Box(
                crate::builtins::BoxCmd::HBox
                | crate::builtins::BoxCmd::VBox
                | crate::builtins::BoxCmd::VTop
                | crate::builtins::BoxCmd::VCenter,
            ))) => true,
            Meaning::Primitive(p) => crate::builtins::expandable(p) || crate::builtins::assignment(p),
            Meaning::Register(kind, _) => {
                !matches!(kind, crate::tex::RegKind::MathChar | crate::tex::RegKind::UMathChar)
            }
            // tex.web § 370: an undefined name is an error and adds nothing.
            Meaning::Undefined => !self.may_be_defined(sym),
            meaning => self.expandable_meaning(&meaning),
        }
    }

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

    /// tex.web § 477: `\edef`'s body is read fully expanded; unexpandable tokens are
    /// kept, `\noexpand` passes the next token through unchanged (§ 369), and
    /// e-TeX's `\protected` macros are copied, not expanded. tex.web § 478: appended
    /// tokens are stored without inspection; a `#` among them is doubled so a later
    /// parameter scan reads it back as one character.
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
        if toks.is_empty() {
            return Vec::new();
        }
        let gaps = self.gaps_raised;
        let at = toks[0].span;
        self.edef_depth += 1;
        let depth = self.edef_depth;
        self.input.push(Frame::Boundary);
        let base = self.input.len();
        self.push_tokens(toks);
        let out = match self.cfg.arm_joins {
            true => self.scan_as_paths(Unit::Expand),
            false => {
                let mut out = Vec::new();
                while out.len() < self.cfg.limits.expansion_tokens {
                    let Some(token) = self.read(false) else { break };
                    if !self.expand_into(token, &mut out) {
                        break;
                    }
                }
                out
            }
        };
        while self.input.len() >= base {
            self.pop_frame();
        }
        self.edef_depth -= 1;
        if self.ran_into_gap(gaps, depth) {
            return vec![self.unknown_text(Unk::Any, at)];
        }
        out
    }

    /// Run an expanding scan as paths: one token per run of
    /// [`Machine::drive`], what it made so far on the path ([`Accum`]), so
    /// the arms of a choice inside it join where they meet again.
    fn scan_as_paths(&mut self, unit: Unit) -> Vec<Token> {
        self.scan_paths(unit, Accum::default()).to_vec()
    }

    /// Run `acc`'s scan as paths ([`Machine::scan_as_paths`]).
    pub(crate) fn scan_paths(&mut self, unit: Unit, acc: Accum) -> Accum {
        let depth = self.scans.len();
        self.scans.push(acc);
        self.drive(unit, |m| m.scans.get(depth).is_none_or(|a| a.done));
        let acc = self.scans.get(depth).cloned().unwrap_or_default();
        self.scans.truncate(depth);
        acc
    }

    /// One run of the scan on top of `scans` ([`Unit`]): one token, or
    /// while the drive runs one path, tokens up to the first that makes a
    /// choice, so a scan that never branches is one run.
    pub(super) fn scan_step(&mut self, unit: Unit) -> bool {
        if matches!(unit, Unit::Number) {
            let value = self.scan_number();
            let abs = if value.is_none() { self.abs.take() } else { None };
            if let Some(acc) = self.scans.last_mut() {
                (acc.number, acc.done) = (Some((value, abs)), true);
            }
            return true;
        }
        loop {
            let done = self.scan_token(unit);
            if done || !self.lone || !self.choices.is_empty() || self.halted || self.out.exhausted {
                return true;
            }
        }
    }

    /// ⟨number⟩ scanned as paths ([`Unit::Number`]): arms of choices inside
    /// it join where it ends, so a test on a bounded value leaves one path
    /// and the value's hull ([`Machine::abs`]) instead of a path per arm.
    /// With `arm_joins` off it is a plain [`Machine::scan_number`].
    pub(crate) fn scan_number_joined(&mut self) -> Option<i64> {
        if !self.cfg.arm_joins {
            return self.scan_number();
        }
        let acc = self.scan_paths(Unit::Number, Accum::default());
        let (value, abs) = acc.number.unwrap_or((None, None));
        self.abs = abs;
        self.abs_exact = None;
        value
    }

    /// One token of the scan on top of `scans`: whether the scan is over.
    fn scan_token(&mut self, unit: Unit) -> bool {
        let Some(acc) = self.scans.last_mut() else { return true };
        if acc.done {
            return true;
        }
        if matches!(unit, Unit::Name) {
            let done = self.name_step();
            if let Some(acc) = self.scans.last_mut() {
                acc.done |= done;
            }
            return done;
        }
        let (mut out, mut depth) = (std::mem::take(&mut acc.tail), acc.depth);
        let frozen = acc.len();
        let done = 'step: {
            if frozen + out.len() >= self.cfg.limits.expansion_tokens {
                break 'step true;
            }
            let Some(token) = self.read(false) else { break 'step true };
            let Unit::Body { opened_in, gaps } = unit else {
                break 'step !self.expand_into(token, &mut out);
            };
            if opened_in.is_none() && self.gaps_raised != gaps && self.reading_document_text(token) {
                self.unread(token);
                self.body_lost_its_end(token.span, &mut out);
                break 'step true;
            }
            match token.tok {
                Tok::Chr(_, Catcode::Begin) => depth += 1,
                Tok::Chr(_, Catcode::End) if depth == 0 => break 'step true,
                Tok::Chr(_, Catcode::End) => depth -= 1,
                _ => break 'step !self.expand_into(token, &mut out),
            }
            out.push(token);
            false
        };
        let acc = self.scans.last_mut().expect("the scan is in flight");
        acc.tail = out;
        acc.depth = depth;
        acc.done |= done;
        acc.freeze();
        done
    }

    /// Whether the expansion that began with `gaps` raised, running at `depth`, ran
    /// into one of its own. Only the innermost expansion gives up over a gap: taking
    /// the mark clears it, so callers keep the unknown text instead of widening again.
    /// A probe run is left alone: it measures satex against the engine.
    fn ran_into_gap(&mut self, gaps: u64, depth: usize) -> bool {
        if self.probe.is_some() || self.gaps_raised == gaps {
            return false;
        }
        match self.gap_depth {
            Some(at) if at >= depth => {
                self.gap_depth = None;
                true
            }
            _ => false,
        }
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
            // marked. Any other token goes back unchanged.
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
            // A name of unknown meaning expands to unknown text. One a join left as
            // one of several macros expands to one of their texts. A token
            // for one of several texts is those texts already: it is stored
            // as `\expandafter` would leave it, not widened to unknown text.
            Meaning::Unknown
                if self.one_of_members(token).is_some()
                    || (self.typed_shape(token).is_some() && !self.typed_text_expands(token)) =>
            {
                out.push(token)
            }
            Meaning::Unknown if self.is_unknown_marker(sym) => out.push(self.unknown_text(Unk::Any, token.span)),
            Meaning::Unknown => {
                self.approximated = true;
                let text = self.one_of_texts(sym, token.span);
                out.push(text);
            }
            Meaning::Undefined if self.may_be_defined(sym) => out.push(self.unknown_text(Unk::Any, token.span)),
            // tex.web § 370: an undefined name is an error and expands to
            // nothing. Kept, `\edef\x{\x}` would make `\x` its own text.
            Meaning::Undefined => {}
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

    /// `\edef\x{…\x…}` where `\x`'s text is a copied text ([`crate::env::copied_text`]):
    /// the call runs as any does and its text is appended at once, which is what
    /// token-by-token reading would do anyway since none of its tokens act. Its
    /// meaning reads as [`ReadKind::Copy`](crate::env::ReadKind::Copy). `None` when
    /// the text is not a copied text or does not fit the limits; then it is read as usual.
    fn copy_through(&mut self, sym: Sym, token: Token, out: &mut Vec<Token>) -> Option<bool> {
        if self.on_paths() {
            return None;
        }
        let text = self.env.slot(sym).and_then(|b| crate::env::copied_text(&b.meaning)).cloned()?;
        let limits = &self.cfg.limits;
        let len = text.len() as i64;
        let room =
            (limits.expansion_tokens as i64 - len).min(limits.held_tokens as i64 - self.held_tokens as i64 - len);
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
            Some(Frame::Tokens { toks, pos: 0, expanding: Some(s), upper: false, .. })
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

    /// The ⟨balanced text⟩ after a `{` that `\expanded`, `\message` and
    /// their relatives read while expanding it (tex.web § 473, `scan_toks`
    /// with `xpand`): a replacement text when `def`, whose `#`s from `\the`
    /// and `\unexpanded` stay characters. The text ends at the `}` balancing
    /// the braces expansion leaves (tex.web § 477), so `{\iffalse}\fi … }`
    /// spans what follows, which expl3's `\tex_edef:D … { \if_false: } \fi:`
    /// relies on.
    pub fn scan_expanded_body(&mut self, def: bool) -> Vec<Token> {
        let scanner = std::mem::replace(&mut self.scanner, if def { Scanner::Defining } else { Scanner::Absorbing });
        let id = self.scan_begin();
        let mut body = self.scan_expanded_body_in(def);
        if self.scan_end(id) {
            body.push(self.unknown_text(Unk::Any, self.position_span()));
        }
        self.scanner = scanner;
        body
    }

    /// Whether `token` is the document's own text rather than a macro's.
    ///
    /// [`Machine::last_file`] answers this for a token straight from the
    /// mouth, but a lookahead token loses that once it falls out of the
    /// window satex keeps, which an argument scan that reads and unreads a
    /// token several times will hit. A token on the line the file's mouth
    /// currently stands on is that file's text either way; a macro body's
    /// span is behind the mouth, except when defined and called on one line.
    fn reading_document_text(&self, token: Token) -> bool {
        let Some(at) = self.input.iter().rev().find_map(|frame| match frame {
            Frame::File { mouth, .. } => Some(mouth.here()),
            _ => None,
        }) else {
            return false;
        };
        if !self.project_file(at.file) || at.file != token.span.file {
            return false;
        }
        self.last_file == Some(token.span.file) || at.line == token.span.line
    }

    /// A body that reached the document's own text although its `{` came from a
    /// macro, which a balanced replacement text never does (tex.web § 476), so the
    /// `}` satex lost is inserted here.
    ///
    /// Cut only when a gap was raised while the scan ran and the document's own
    /// text is about to go in: satex knows it lost the `}` itself, and reading on
    /// would otherwise absorb the rest of the document into the body (as seen with
    /// `\IfFileExists@` swallowing hundreds of lines after a lost argument,
    /// reported only as `file-ended`). A package's own text absorbed into a
    /// package's own definition stays a gap.
    ///
    /// tex.web § 336 inserts the missing `}` where the file ends; satex inserts it
    /// at the boundary instead, so text after it still reads as the document's own.
    /// What the body would have held past this point is unknown, so it ends in
    /// unknown text and the gap is recorded.
    fn body_lost_its_end(&mut self, at: Span, out: &mut Vec<Token>) {
        let name = self.out.short_name(at.file).to_string();
        self.diagnose(
            Severity::Unsupported,
            "body-crosses-file",
            at,
            format!(
                "a definition body that began in a macro reached the text of {name} at line {}, after a gap: \
                 the `}}` it is missing is inserted here rather than at the end of the file, and what it \
                 would have held is unknown",
                at.line
            ),
        );
        out.push(self.unknown_text(Unk::Any, at));
    }

    fn scan_expanded_body_in(&mut self, def: bool) -> Vec<Token> {
        let gaps = self.gaps_raised;
        let saved = if def { self.def_body_depth.replace(self.edef_depth + 1) } else { self.def_body_depth.take() };
        self.edef_depth += 1;
        let scanning = self.edef_depth;
        // Where the `{` came from, so that a body which lost its `}` over a gap
        // is not left to absorb the document (see `body_lost_its_end`).
        let opened_in = self.last_file;
        let out = match self.cfg.arm_joins {
            true => self.scan_as_paths(Unit::Body { opened_in, gaps }),
            false => self.scan_expanded_body_replaying(opened_in, gaps),
        };
        self.edef_depth -= 1;
        self.def_body_depth = saved;
        if self.ran_into_gap(gaps, scanning) {
            let at = out.first().map_or_else(Span::default, |t| t.span);
            return vec![self.unknown_text(Unk::Any, at)];
        }
        out
    }

    /// The body read token by token in one run, a choice inside it re-running
    /// the whole command (`arm_joins` off).
    fn scan_expanded_body_replaying(&mut self, opened_in: Option<FileId>, gaps: u64) -> Vec<Token> {
        let mut out = Vec::new();
        let mut depth = 0usize;
        while out.len() < self.cfg.limits.expansion_tokens {
            let Some(token) = self.read(false) else { break };
            if opened_in.is_none() && self.gaps_raised != gaps && self.reading_document_text(token) {
                self.unread(token);
                self.body_lost_its_end(token.span, &mut out);
                break;
            }
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
        out
    }

    /// tex.web §§ 1046-1047 `insert_dollar_sign`: math material outside
    /// math, or `\\par`, `\\end` and vertical material inside it, puts the
    /// `$` TeX lacks before `token`, which is read again. Entering math only
    /// outside alignments and in the document's own text, whose mode satex
    /// follows surely.
    fn insert_dollar(&mut self, token: Token) -> bool {
        use crate::mode::Modes;
        let math = match (token.tok, token.cs().map(|sym| self.env.meaning(sym))) {
            (Tok::Chr(_, Catcode::Sup | Catcode::Sub), _) => true,
            (_, Some(Meaning::Register(crate::tex::RegKind::MathChar | crate::tex::RegKind::UMathChar, _))) => true,
            (Tok::Cs(sym), Some(Meaning::Primitive(_))) => {
                match crate::builtins::math_material(self.name(self.env.identity(sym))) {
                    Some(math) => math,
                    None => return false,
                }
            }
            _ => return false,
        };
        let insert = match math {
            true => {
                !self.mode.is_empty()
                    && !self.mode.meets(Modes::ANY_MATH)
                    && !self.env.in_alignment()
                    && self.own_file(token.span.file)
            }
            false => self.mode.within(Modes::ANY_MATH) && !self.mode.is_empty(),
        };
        if !insert {
            return false;
        }
        if self.own_file(token.span.file) {
            self.project_site = Some(token.span);
        }
        let what = match token.tok {
            Tok::Cs(sym) => format!("\\{}", self.name(sym)),
            _ => "a script".into(),
        };
        let code = if math { "script-outside-math" } else { "missing-dollar" };
        self.diagnose(Severity::Warning, code, token.span, format!("missing $ inserted before {what}"));
        self.unread(token);
        self.unread(Token::new(Tok::Chr('$', Catcode::Math), token.span));
        true
    }

    fn do_control_sequence(&mut self, sym: Sym, span: Span) {
        if self.own_file(span.file) && self.built_name != Some((sym, span)) {
            self.project_site = Some(span);
        }
        // A control sequence read straight from a file starts a new call. One
        // that came out of a macro body is part of the call already running.
        if self.last_file == Some(span.file) {
            self.file_call = Some((sym, span));
            self.finish_shape();
            self.call_shape = Some(Rc::new((sym, span, String::new(), self.within.len())));
        }
        // Text a join or a gap left: what the path does next is not known exactly.
        if matches!(self.env.meaning(sym), Meaning::Unknown) {
            self.approximated = true;
            if let Some((_, _, named)) = self.typed_shape(Token::new(Tok::Cs(sym), span))
                && !named.is_empty()
            {
                self.may_run(sym, &named, &[], span);
            }
        }
        if self.insert_dollar(Token::new(Tok::Cs(sym), span)) {
            return;
        }
        match self.env.meaning(sym) {
            Meaning::Primitive(p) => {
                self.record(Step::Execute, sym, span);
                let node = self.reference(sym, span);
                self.note_expansion(sym, span, MeaningKind::Primitive, node, Vec::new());
                // tex.web § 1038: a command that is neither a character nor
                // expands ends the word being set.
                if !crate::builtins::expandable(p)
                    && p != Primitive::Mode(crate::builtins::ModeCmd::Horizontal(crate::builtins::Material::Char))
                {
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
                        self.read_call_arguments(sym, arity as usize, span);
                    }
                    None => {
                        let node = self.reference(sym, span);
                        let kind = self.undefined_kind();
                        self.note_expansion(sym, span, kind, node, Vec::new());
                        self.record(Step::Undefined, sym, span);
                        let text = format!("Undefined control sequence \\{}", self.name(sym));
                        self.certain_error(&text, true, span);
                    }
                }
            }
            Meaning::Register(kind, _) => {
                let node = self.reference(sym, span);
                self.note_expansion(sym, span, MeaningKind::Register, node, Vec::new());
                // A `\chardef` constant in the stomach typesets its character (tex.web
                // § 1038); only a register takes an assignment. `\newbox` makes a
                // `\chardef` (plain.tex `\alloc@`), which also typesets its character.
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
                } else if kind == crate::tex::RegKind::MathChar && self.mode == crate::mode::Modes::MATH {
                    let code = self.env.value(sym).as_int();
                    self.math_code_char(code);
                } else if !matches!(kind, crate::tex::RegKind::MathChar | crate::tex::RegKind::UMathChar) {
                    self.end_word();
                    let scope = self.apply_global_defs();
                    let recent = scope.then(|| {
                        let recent = self.env.recent();
                        self.env.clear_recent();
                        recent
                    });
                    self.assign_register(sym, kind, span);
                    if let Some(recent) = recent {
                        self.widen_since(0);
                        self.env.set_recent(recent);
                    }
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
                // tex.web § 232: only category 13 runs a meaning, bound to that
                // character (§ 344). A marker standing for a name has no such bound;
                // `domains.md` records that as a limit.
                if let Some(c) = self.unknown_chars.get(&span).copied() {
                    self.force_reference(sym, span);
                    let bound = self.active_sym(c);
                    let runs = !matches!(self.env.meaning(bound), Meaning::Undefined | Meaning::Unknown);
                    if runs && self.branch(sym, 2, span) == Some(1) {
                        self.push_tokens(Rc::from(vec![Token::new(Tok::Cs(bound), span)]));
                        return;
                    }
                }
                let kind = match other {
                    Meaning::Char(..) => MeaningKind::Char,
                    _ => MeaningKind::Unknown,
                };
                // Unknown text run where no region's summary bounds it: what
                // it does is not known, and assumed to touch nothing.
                if sym == self.unknown && !self.scratch.summarized.contains(&span) {
                    self.diagnose(
                        Severity::Assumption,
                        "unknown-text-run",
                        span,
                        "unknown text runs here; it is assumed to change no state".into(),
                    );
                }
                self.note_expansion(sym, span, kind, node, Vec::new());
                // An implicit `{`, `}` or `$` acts like the character it was
                // `\let` to (tex.web § 1063): `\bgroup` opens a group.
                self.do_character(Token::new(Tok::Cs(sym), span));
            }
        }
    }

    /// An undefined name met here: [`MeaningKind::UndefinedApproximated`]
    /// on a path that approximated since the paths parted, where the
    /// engine's error is a possible one ([`Machine::certain_error`]).
    pub(crate) fn undefined_kind(&mut self) -> MeaningKind {
        match self.approximated || self.after_cut() {
            true => MeaningKind::UndefinedApproximated,
            false => MeaningKind::Undefined,
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
        // The macro whose text held the call, as its vertex was made: a list
        // used up by the call's last token has ended since (tex.web § 390).
        let graph = &self.out.graph;
        let within = match node.and_then(|n| graph.vertex(n)) {
            Some(v) => v.within.and_then(|w| graph.vertex(w)).map(|w| w.name),
            None => self.within.last().map(|(sym, _)| *sym),
        };
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
                within,
                arguments: arguments.clone(),
                cds: self.cds.to_vec(),
                count: 0,
                mode: self.mode,
            };
            let count =
                self.expansion_sites.get(&(span, name)).map_or(0, |site| self.out.facts.expansions[site.fact].count);
            capture.log.touched.insert((span, name), (fact, count));
        }
        let (active, written) = (self.expanding.get(name.0 as usize).is_some_and(|&d| d > 0), self.env.assignments);
        if let Some(site) = self.expansion_sites.get_mut(&(span, name)) {
            self.undo.push(Undo::Site((span, name), *site));
            // A loop re-expands one site while the source stands still
            // (`\loop … \repeat`, expl3's mapping functions). A helper
            // called again from a later line has made progress, so its
            // count starts over — the per-site bound only bites a loop.
            site.repeats = if site.progress == progress { site.repeats.saturating_add(1) } else { 1 };
            // A recursion begins where no expansion of this name is open
            // and the site starts a new run of repeats.
            if !active && site.repeats == 1 {
                site.recent = written;
            }
            site.progress = progress;
            let read = self.source_progress;
            site.unread = if site.source == read { site.unread.saturating_add(1) } else { 1 };
            site.source = read;
            let (repeats, fact) = (site.repeats, site.fact);
            let mode = self.mode;
            let previous = &mut self.out.facts.expansions[fact];
            previous.count = previous.count.saturating_add(1);
            previous.mode = previous.mode.union(mode);
            return repeats;
        }
        let package = self.package();
        let fact = self.out.facts.expansions.len();
        let site = Site {
            repeats: 1,
            progress,
            source: self.source_progress,
            unread: 1,
            recent: self.env.assignments,
            ..Site::fresh(fact)
        };
        self.expansion_sites.insert((span, name), site);
        self.undo.push(Undo::Site((span, name), Site::fresh(fact)));
        self.out.facts.expansions.push(Expansion {
            name,
            span,
            package,
            node,
            meaning,
            within,
            arguments,
            cds: self.cds.to_vec(),
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
            facts.push(crate::facts::Conditional {
                name,
                at,
                arms: Vec::new(),
                fi: None,
                taken: Vec::new(),
                undecided: false,
                test_end: None,
            });
            facts.len() - 1
        });
        let fact = &mut facts[index];
        match taken {
            Some(arm) if !fact.taken.contains(&arm) => fact.taken.push(arm),
            Some(_) => {}
            None => fact.undecided = true,
        }
    }

    /// Where the undecided test of the conditional at `at` ended: the top
    /// frame's next place, looked at without reading. A peek would give
    /// the token back as a frame of its own, and loop keys see it.
    pub(crate) fn note_test_end(&mut self, at: Span) {
        let Some(&index) = self.cond_sites.get(&at) else { return };
        let end = match self.input.last() {
            Some(Frame::Tokens { toks, pos, .. }) => toks.get(*pos).map(|t| t.span),
            Some(Frame::Returned(back)) => back.last().map(|t| t.span),
            Some(Frame::File { mouth, .. }) => Some(mouth.here()),
            _ => None,
        };
        self.out.facts.conditionals[index].test_end.get_or_insert(end.unwrap_or_default());
    }

    /// The `\else`, `\or` or (`fi`) `\fi` at `mark` of the conditional at
    /// `at`. One in another file does not make the two one construct.
    pub(crate) fn note_cond_mark(&mut self, at: Span, mark: Span, fi: bool) {
        let Some(&index) = self.cond_sites.get(&at).filter(|_| mark.file == at.file) else { return };
        let fact = &mut self.out.facts.conditionals[index];
        if fi {
            fact.fi = Some(mark);
        } else if !fact.arms.contains(&mark) {
            fact.arms.push(mark);
        }
    }

    /// `\pickup@font` over a font name with unknown parts (an unknown size,
    /// family or shape): the font is some font of unknown metrics. Loading
    /// `<enc>/<family>/…` would search every file name the unknown text
    /// could spell, on every path.
    fn unknown_font_pick(&mut self, sym: Sym, span: Span) -> bool {
        let (pick, name, load) = match self.font_watch.pick {
            Some(pair) => pair,
            None => {
                let pair = (self.intern("pickup@font"), self.intern("font@name"), self.intern("try@load@fontshape"));
                self.font_watch.pick = Some(pair);
                pair
            }
        };
        if sym != pick && sym != load {
            return false;
        }
        let of = |this: &Self, name: Sym| match &this.env.slot(name).map(|b| &b.meaning) {
            Some(Meaning::Macro(m)) => this.has_unknown(&m.replacement_text),
            Some(Meaning::Unknown) => true,
            _ => false,
        };
        let unknown = if sym == pick {
            of(self, name)
        } else {
            crate::commands::NFSS
                .iter()
                .chain(&["f@encoding"])
                .any(|n| self.out.interner.lookup(n).is_some_and(|n| of(self, n)))
        };
        if !unknown {
            return false;
        }
        if let Some(font) = self.out.interner.lookup("\u{4}font") {
            self.env.set_value(font, crate::value::Value::Unknown, false);
        }
        self.diagnose(
            Severity::Imprecision,
            "unknown-font",
            span,
            "a font selected by an unknown size, family or shape: some font with unknown metrics".to_string(),
        );
        true
    }

    /// A hash of a call's arguments and of the input left after them, the
    /// number of input levels that input stands on, and the same hash of
    /// its lowest `below` levels, if it has that many: what a call met a
    /// level deeper would hash to if the input under it were unchanged.
    /// A level is a list with tokens left, a file or a boundary; a used-up
    /// list holds nothing a later read could meet.
    /// A token list counts by its length and first [`SHAPE_TOKENS`] tokens.
    fn call_shape_hash(&self, arguments: &[Vec<Token>], below: usize) -> (u64, usize, Option<u64>) {
        use std::hash::{BuildHasher, Hash, Hasher};
        let mut h = crate::tex::NameHash.build_hasher();
        for a in arguments {
            a.len().hash(&mut h);
            a.iter().for_each(|t| h.write_u64(t.tok.word()));
        }
        let (mut levels, mut under) = (0, None);
        for frame in &self.input {
            let level = match frame {
                Frame::Tokens { toks, pos, .. } => *pos < toks.len(),
                Frame::Returned(t) => !t.is_empty(),
                _ => true,
            };
            if !level {
                continue;
            }
            if levels == below {
                under = Some(h.finish());
            }
            levels += 1;
            match frame {
                // By content, never by address: a freed list's address is
                // reused, and where depends on allocation order.
                Frame::Tokens { toks, pos, .. } => {
                    // The memo holds the list, so its address names this content.
                    let slot = ((toks.as_ptr() as usize / 8) ^ pos.wrapping_mul(0x9e37)) % SHAPE_MEMO;
                    let mut memo = self.shape_memo.borrow_mut();
                    let sub = match &memo[slot] {
                        Some((list, at, sub)) if *at == *pos && Rc::ptr_eq(list, toks) => *sub,
                        _ => {
                            let mut g = crate::tex::NameHash.build_hasher();
                            let rest = &toks[*pos..];
                            rest.len().hash(&mut g);
                            rest.iter().take(SHAPE_TOKENS).for_each(|t| g.write_u64(t.tok.word()));
                            let sub = g.finish();
                            memo[slot] = Some((toks.clone(), *pos, sub));
                            sub
                        }
                    };
                    sub.hash(&mut h);
                }
                Frame::Returned(t) => t.iter().for_each(|t| h.write_u64(t.tok.word())),
                Frame::File { mouth, .. } => (mouth.file, mouth.offset()).hash(&mut h),
                Frame::Pseudo { mouth, at, pos, .. } => (at, mouth.offset() + pos).hash(&mut h),
                Frame::Boundary => 0u8.hash(&mut h),
            }
        }
        (h.finish(), levels, under)
    }

    /// The one summary of a call: what it would expand to is unknown text,
    /// and what the rounds before it assigned may hold anything of its
    /// kind (tex.web § 366: the call is replaced by its expansion, so
    /// dropping it would claim it expanded to nothing).
    fn cut_call(&mut self, sym: Sym, span: Span, key: &'static str, nested: bool, reason: String) {
        self.record_with(Step::Widen, sym, span, || Some("recursion"));
        if self.cfg.verbose >= 4 {
            crate::progress::note!("satex: call \\{} at {span:?} cut: {reason}", self.name(sym));
        }
        self.diagnose(
            Severity::Imprecision,
            "recursion-widened",
            span,
            format!("\\{}: {reason}; what it would expand to and what it would assign are not known", self.name(sym)),
        );
        // A recursion that nests is cut where it began, never past a
        // file: what its rounds stacked would run only once it returned,
        // which it never does. Nesting past `expansion_depth` is also a
        // limit. A site that repeats is summarized, as a loop is.
        if key == "expansion_depth" {
            self.limit_reached(key, span);
        }
        if nested {
            let file = self.input.iter().rposition(|f| matches!(f, Frame::File { .. })).map_or(0, |i| i + 1);
            let first =
                self.input.iter().position(|f| matches!(f, Frame::Tokens { expanding: Some(s), .. } if *s == sym));
            if let Some(i) = first.map(|i| i.max(file)) {
                let places = self.input.iter().take(i).filter_map(explore::place).count();
                self.unwind((places, self.conds.len(), self.env.depth()), span);
            }
        }
        self.approximated = true;
        self.scratch.summarized.insert(span);
        // The rounds not run would write what the rounds run wrote: only
        // the names written since this recursion began are widened. A
        // write before it, or one a closed group undid, is not the call's.
        let mark = self.expansion_sites.get(&(span, sym)).map_or(0, |site| site.recent);
        self.widen_since(mark);
        self.taint_after_cut();
        let unknown = self.unknown_text(Unk::Any, span);
        self.push_tokens(Rc::from(vec![unknown]));
    }

    fn expand_macro(&mut self, sym: Sym, m: Rc<MacroDef>, span: Span) {
        if self.out.timings.is_enabled() {
            self.out.timings.count_call(sym);
        }
        if self.join_hooks.contains(&sym)
            || self.join_commands.iter().any(|&c| self.env.meaning(c).as_macro().is_some_and(|d| Rc::ptr_eq(d, &m)))
        {
            self.at_join = true;
        }
        let node = self.out.graph.push(
            VertexTag::MacroCall,
            sym,
            span,
            self.within.last().map(|(_, n)| *n),
            self.cds.to_vec(),
        );
        let defs = self.env.defs(sym);
        let definition = defs.first().copied();
        self.link_reads(node, sym, &defs);
        if let Some(caller) = self.within.last().map(|(name, _)| *name) {
            self.out.calls.add(caller, sym);
        }
        self.observe_call(sym, span);
        if self.unknown_font_pick(sym, span) {
            return;
        }
        if self.stubbed(sym, &defs) {
            self.expand_stub(sym, node, span);
            return;
        }

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
        // One call site open more than `recursion_depth` times at once on a
        // path that approximated: a recursion over unknown values that nests
        // rather than reads on, which no round ends. On known values it runs
        // as TeX runs it.
        let rounds = usize::from(self.limits.recursion_depth);
        if usize::from(depth) > rounds && self.approximated {
            let graph = &self.out.graph;
            let here =
                self.within.iter().filter(|(s, n)| *s == sym && graph.vertex(*n).is_some_and(|v| v.span == span));
            let nested = here.count();
            if nested > rounds {
                self.cut_call(sym, span, "recursion_depth", true, format!("open {nested} times at this call"));
                return;
            }
        }
        if depth >= self.limits.expansion_depth || site > self.limits.site_expansions {
            let key = if depth >= self.limits.expansion_depth { "expansion_depth" } else { "site_expansions" };
            let nested = depth >= self.limits.expansion_depth;
            self.cut_call(
                sym,
                span,
                key,
                nested,
                format!("re-entered {depth} times, expanded {site} times at this call"),
            );
            return;
        }

        self.scan_abandoned = false;
        self.arm_assumed = false;
        let arguments = self.read_arguments(&m);
        let abandoned = self.scan_abandoned;
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
        // tex.web §§ 395-396: the document compiles, so a runaway after a
        // scan took unknown text to hold no delimiter refutes that arm; the
        // other arm ended the argument in that text.
        if self.runaway != Runaway::No
            && std::mem::take(&mut self.arm_assumed)
            && self.cfg.compiles()
            && self.probe.is_none()
        {
            self.runaway = Runaway::No;
            self.no_arm();
            return;
        }
        // A runaway on an arm over unknown text: under analysis.assume_compiles the
        // path is dropped while another arm goes on, kept if it is the last.
        if self.runaway != Runaway::No
            && self.cfg.compiles()
            && self.probe.is_none()
            && !self.keep_errors
            && self.took_unknown_arm()
        {
            let text = format!("Paragraph ended before \\{} was complete", self.name(sym));
            self.dropping.get_or_insert(text);
        }
        let runaway = std::mem::take(&mut self.runaway);
        if runaway != Runaway::No {
            if self.cfg.verbose >= 4 {
                let read: Vec<String> = arguments
                    .iter()
                    .map(|a| crate::tex::detokenize(&a[..a.len().min(48)], &self.out.interner))
                    .collect();
                let pending = self.pending_lists();
                let ahead = crate::tex::detokenize(&pending[..pending.len().min(48)], &self.out.interner);
                let unk = self.unknown;
                let from: Vec<String> = arguments
                    .iter()
                    .flatten()
                    .filter(|t| matches!(t.tok, Tok::Cs(c) if c == unk))
                    .map(|t| {
                        let f = self.out.file_name(t.span.file);
                        format!("{}:{}:{}", f.rsplit('/').next().unwrap_or(f), t.span.line, t.span.col)
                    })
                    .collect();
                crate::progress::note!(
                    "satex: runaway in \\{}: arguments {read:?}; unknown from {from:?}; ahead: {ahead}",
                    self.name(sym)
                );
            }
            // tex.web § 396: the call is dropped. Only `\par` in a macro not
            // `\long` says so; `\outer` was reported where it was met.
            let name = self.name(sym).to_string();
            let message = match runaway {
                Runaway::Par => format!("Paragraph ended before \\{name} was complete; the call is abandoned"),
                Runaway::FileEnded => format!("File ended while scanning use of \\{name}: its argument never closes"),
                _ => return,
            };
            self.diagnose(Severity::Warning, "runaway-argument", span, message);
            return;
        }
        self.record_argument_values(sym, node, &arguments);
        let recorded = if self.cfg.record_arguments {
            arguments.iter().map(|a| a.clone().into_boxed_slice()).collect()
        } else {
            Vec::new()
        };
        let resolved = MeaningKind::Macro(definition.unwrap_or(node));
        let visits = self.note_expansion(sym, span, resolved, Some(node), recorded);
        self.note_repeated_load(sym, &arguments);
        let call_key = self.call_key(definition.unwrap_or(node), span, &arguments);
        if self.summarize_call(call_key, &arguments) {
            return;
        }
        let unknown_operands = arguments.iter().any(|a| self.has_unknown(a));

        let depth = self.expanding.get(sym.0 as usize).copied().unwrap_or(0);
        // What this expansion was handed, against what the last one here
        // was: a recursion that keeps re-entering the same site without
        // reading on is only running out if this is getting smaller.
        let size: usize = arguments.iter().map(Vec::len).sum();
        let below = self.expansion_sites.get(&(span, sym)).map_or(usize::MAX, |site| site.levels);
        let (shape, levels, under) = self.call_shape_hash(&arguments, below);
        let nest = (self.conds.len(), self.env.depth());
        let effective = self.env.effective;
        let (shrinking, unread, deeper, cycled) = match self.expansion_sites.get_mut(&(span, sym)) {
            Some(site) => {
                self.undo.push(Undo::Site((span, sym), *site));
                let outside = nest.0 >= site.nest.0 && nest.1 >= site.nest.1;
                // Deeper on the input stack: the last round's input is all
                // still there, unread, under what that round left on top,
                // and nothing was written since, so this round reads what
                // the last did and stacks the same again (tex.web § 366).
                let stacked = under == Some(site.shape) && levels > site.levels && site.effective == effective;
                let grew = outside && (stacked || site.shape == shape && nest != site.nest);
                site.deeper = if grew { site.deeper + 1 } else { 0 };
                let same = site.shape == shape
                    && nest == site.nest
                    && site.effective == effective
                    && site.scans == self.step_depth;
                site.scans = self.step_depth;
                site.cycled = if same { site.cycled + 1 } else { 0 };
                (site.shape, site.levels, site.nest, site.effective) = (shape, levels, nest, effective);
                (size < std::mem::replace(&mut site.size, size), site.unread, site.deeper, site.cycled)
            }
            None => (true, 1, 0, 0),
        };
        // The same arguments, input, nesting and bindings as the last
        // expansion here: TeX is in a state it was in, and repeats it
        // forever (tex.web § 366 expands deterministically). A path ends
        // only while another waits to read on; one no other path covers is
        // summarized, so the run goes on (the repeat may be satex's own
        // imprecision).
        if self.limits.cycle_after > 0 && cycled >= u32::from(self.limits.cycle_after) {
            let alone = !self.on_paths() || self.waiting == 0;
            self.diagnose(
                Severity::Warning,
                "endless-loop",
                span,
                format!("\\{} meets the state it was in before: possible endless loop", self.name(sym)),
            );
            match alone {
                true => self.cut_call(sym, span, "cycle_after", false, "it meets the state it was in before".into()),
                false => self.no_arm(),
            }
            return;
        }
        // The same arguments and input, only deeper in conditionals or
        // groups each time: every round repeats the last, so none ends it.
        let nesting = deeper > u32::from(self.limits.widen_after);
        // A recursion re-entering this site without reading on, with its
        // argument scan abandoned and its argument not shrinking, has
        // nothing left that could end it: the delimiter it seeks is the one
        // the scan just failed to find, and the next round looks in the
        // same place. Running it to the per-site bound only repeats that
        // round, so it is widened sooner, at `recursion_depth`.
        let rounds = u32::from(self.limits.recursion_depth);
        // A quark (`\q_nil`, `\q_stop`, …) is a macro whose whole body is
        // itself: expl3 never expands one, only compares it, but a path
        // satex could not rule out may try to. That is a fixed point by
        // construction, like expanding satex's own unknown marker
        // (`is_unknown_marker`): no argument, nothing can shrink, and no
        // bound but this one would end it. Cut at `rounds`, not
        // `site_expansions`.
        let self_fixed_point = arguments.is_empty()
            && m.replacement_text.len() == 1
            && matches!(m.replacement_text[0].tok, Tok::Cs(s) if s == sym);
        // `abandoned` alone, without `!shrinking`, still cuts: a scan that
        // gave up on the unknown marker guessed where its delimiter was
        // (`read_delimited`'s "a split is made once"), so a shrinking token
        // count next round is not real progress — the guessed remainder can
        // shrink by the same dropped tokens every time and never converge,
        // as a corrupted `array`/`tabularx` column preamble does.
        let stuck = nesting
            || (abandoned && visits > rounds && visits <= self.limits.site_expansions)
            || (self_fixed_point && !shrinking && visits > rounds && visits <= self.limits.site_expansions);
        // `repeats` cannot see a recursion that assigns every round, so
        // this counts reading on alone (tex.web § 366).  The multiple is
        // measured, not derived: 16 cuts no loop that was getting somewhere.
        let never_read = self.limits.site_expansions.saturating_mul(16);
        let spinning = unread > never_read;
        if depth >= self.limits.expansion_depth || visits > self.limits.site_expansions || stuck || spinning {
            let reason = if depth >= self.limits.expansion_depth {
                format!("re-entered {depth} times")
            } else if spinning {
                format!("expanded {unread} times without reading on, whatever it assigned")
            } else if nesting {
                format!("re-entered {deeper} times with the same arguments and input, nested deeper each time")
            } else if self_fixed_point {
                format!("expanded {visits} times")
            } else if stuck {
                format!("expanded {visits} times without finding the delimiter its argument ends at")
            } else {
                format!("expanded {visits} times without reading on")
            };
            let key = match () {
                _ if depth >= self.limits.expansion_depth => "expansion_depth",
                _ if stuck => "recursion_depth",
                _ => "site_expansions",
            };
            self.cut_call(sym, span, key, nesting || depth >= self.limits.expansion_depth, reason);
            return;
        }

        // The detail is what this very definition inserts: later ones under
        // the same name must not stand in for it.
        let interner = &self.out.interner;
        let inserts = (self.cfg.trace)
            .then(|| substitute(&m, &arguments, 512).map(|b| crate::tex::detokenize(&b, interner)))
            .flatten();
        self.record_with(Step::Expand, sym, span, || inserts);
        // The budget bounds what an expansion adds. Arguments it was handed
        // were already there (a `filecontents` line read from the file is
        // one argument, however long), and each `#n` copies one.
        let handed: usize = arguments.iter().map(Vec::len).max().unwrap_or(0);
        let copies = m.replacement_text.iter().filter(|t| matches!(t.tok, Tok::Param(_))).count();
        let limit = self.cfg.limits.expansion_tokens.saturating_add(handed.saturating_mul(copies));
        let Some(body) = substitute_parts(&m, &arguments, limit) else {
            self.diagnose(
                Severity::Unsupported,
                "expansion-too-large",
                span,
                format!("\\{} would expand beyond the token limit", self.name(sym)),
            );
            self.limit_reached("expansion_tokens", span);
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
        crate::push_shared(&mut self.within, (sym, node));
        let before = self.input.len();
        self.record_call(call_key, unknown_operands);
        self.push_parts(body, sym);
        if self.input.len() == before {
            Rc::make_mut(&mut self.within).pop();
        }
    }

    fn record_argument_values(&mut self, sym: Sym, node: NodeId, arguments: &[Vec<Token>]) {
        if self.cfg.record_arguments {
            // Only arguments the document wrote: library calls would fill
            // `limits.vertices` before the document is read.
            for argument in arguments.iter().filter(|a| !a.is_empty()) {
                let first = argument[0];
                if !self.own_file(first.span.file) {
                    continue;
                }
                let named = argument.iter().find_map(Token::cs).unwrap_or(sym);
                let value = self.out.graph.push(VertexTag::Value, named, first.span, Some(node), self.cds.to_vec());
                self.out.graph.edge(node, value, EdgeKind::ARGUMENT);
                // An argument depends on whatever its tokens name.
                for token in argument.iter() {
                    let Tok::Cs(referenced) = token.tok else { continue };
                    let defs = self.env.link_defs(referenced);
                    self.link_reads(value, referenced, &defs);
                }
            }
        }
    }

    /// Whether `sym`'s expansion is a configured stub (`stubs:`): only while
    /// its definition is not the document's own.
    fn stubbed(&mut self, sym: Sym, defs: &[NodeId]) -> bool {
        if self.cfg.stubs.is_empty() {
            return false;
        }
        if self.stubs.is_none() {
            let stubs = self.cfg.stubs.clone();
            let table =
                stubs.into_iter().map(|(name, stub)| (self.intern(name.trim_start_matches('\\')), stub)).collect();
            self.stubs = Some(Rc::new(table));
        }
        if !self.stubs.as_ref().is_some_and(|t| t.contains_key(&sym)) {
            return false;
        }
        let Some(file) = defs.first().and_then(|d| self.out.graph.vertex(*d)).map(|v| v.span.file) else {
            return true;
        };
        let files = &self.out.files;
        let own = |kind| matches!(kind, LoadKind::Input | LoadKind::Include | LoadKind::Conditional);
        let absolute = |p: &std::path::Path| {
            let p = if p.as_os_str().is_empty() { std::path::Path::new(".") } else { p };
            std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf())
        };
        let root = files.first().map(|f| std::path::Path::new(&f.path).parent().map(absolute));
        let info = files.get(file as usize);
        !info.is_some_and(|f| {
            own(f.kind)
                && root
                    .as_ref()
                    .is_some_and(|r| r.as_ref().is_none_or(|r| absolute(std::path::Path::new(&f.path)).starts_with(r)))
        })
    }

    /// A stub (`stubs:`) in place of the expansion: the arguments are read
    /// and recorded, the result left, and `sets` made unknown.
    fn expand_stub(&mut self, sym: Sym, node: NodeId, span: Span) {
        use crate::config::{StubKind, StubResult};
        use crate::tex::ArgType;
        let Some(stub) = self.stubs.as_ref().and_then(|t| t.get(&sym)).cloned() else { return };
        let outer = std::mem::replace(&mut self.long_argument, true);
        let arguments = match (&stub.until, &stub.args) {
            (Some(until), _) => {
                let mut delimiter = Vec::new();
                let mut rest = until.chars().peekable();
                while let Some(c) = rest.next() {
                    if c == '\\' {
                        let mut name = String::new();
                        while let Some(&l) = rest.peek().filter(|l| l.is_alphabetic() || matches!(l, '@' | '_' | ':')) {
                            name.push(l);
                            rest.next();
                        }
                        let sym = self.intern(&name);
                        delimiter.push(Tok::Cs(sym));
                    } else {
                        delimiter.push(Tok::Chr(c, if c.is_alphabetic() { Catcode::Letter } else { Catcode::Other }));
                    }
                }
                vec![self.read_delimited(&delimiter)]
            }
            (None, args) => {
                let items = args
                    .as_deref()
                    .unwrap_or("")
                    .chars()
                    .filter(|c| !c.is_whitespace())
                    .map(|c| match c {
                        's' => ArgType::Star,
                        'o' => ArgType::Optional(None),
                        _ => ArgType::Mandatory,
                    })
                    .collect();
                self.read_signature(&ArgSpec { items, raw: String::new() })
            }
        };
        self.long_argument = outer;
        self.record_argument_values(sym, node, &arguments);
        let recorded = if self.cfg.record_arguments {
            arguments.iter().map(|a| a.clone().into_boxed_slice()).collect()
        } else {
            Vec::new()
        };
        self.note_expansion(sym, span, MeaningKind::Macro(node), Some(node), recorded);
        let sets: Vec<(String, Option<crate::config::StubResult>)> = match &stub.sets {
            crate::config::StubSets::Names(names) => names.iter().map(|n| (n.clone(), None)).collect(),
            crate::config::StubSets::Typed(map) => map.iter().map(|(n, r)| (n.clone(), r.clone())).collect(),
        };
        for (name, typed) in sets {
            let target = self.intern(name.trim_start_matches('\\'));
            let text = typed.as_ref().and_then(|r| self.stub_text(r, span));
            match (self.env.meaning(target), text) {
                // A register keeps its meaning, only what it holds is lost.
                (Meaning::Register(kind, n), _) => {
                    let cell = self.register_sym(kind, i64::from(n));
                    let value = match typed {
                        Some(StubResult::Number { number: [lo, hi] }) => crate::value::Value::Num {
                            quantity: crate::value::Quantity::Int,
                            num: crate::value::Num { lo, hi },
                        },
                        _ => crate::value::Value::Unknown,
                    };
                    self.env.set_value(cell, value, true);
                }
                (_, Some(body)) => {
                    let meaning = self.make_macro(crate::tex::ParameterText::default(), body);
                    self.env.set(target, crate::env::Binding::new(meaning, node), true);
                }
                _ => self.env.set(target, crate::env::Binding::new(Meaning::Unknown, node), true),
            }
        }
        match &stub.result {
            StubResult::Plain(StubKind::Box) => {
                self.touch_page();
                self.append_item(crate::mode::Item::Unknown);
            }
            StubResult::Box { bounds } => {
                self.touch_page();
                let dims = [&bounds.width, &bounds.height, &bounds.depth].map(|b| self.stub_dim(b));
                self.append_item(crate::mode::Item::Box(dims));
            }
            result => {
                if let Some(text) = self.stub_text(result, span).filter(|t| !t.is_empty()) {
                    self.push_tokens(Rc::from(text));
                }
            }
        }
        if self.stub_said.insert(sym) {
            let name = self.name(sym).to_string();
            self.diagnose(
                Severity::Assumption,
                "stubbed",
                span,
                format!("\\{name} was stubbed: its internals were not followed (`stubs:` in satex.yaml)"),
            );
        }
    }

    /// The unknown text a stub result stands for, typed with the narrowest
    /// class the configuration names. `None` for a box.
    fn stub_text(&mut self, result: &crate::config::StubResult, span: Span) -> Option<Vec<Token>> {
        use crate::config::{StubKind, StubResult};
        let dimension = TextClass::NUMBER.union(TextClass::LETTER);
        Some(match result {
            StubResult::Plain(StubKind::Nothing) => Vec::new(),
            StubResult::Plain(StubKind::Number) => {
                self.digits_in(crate::value::Num { lo: -2_147_483_647, hi: 2_147_483_647 }, span)
            }
            StubResult::Number { number: [lo, hi] } => self.digits_in(crate::value::Num { lo: *lo, hi: *hi }, span),
            StubResult::Plain(StubKind::Dimen) => vec![self.unknown_text(Unk::Class(dimension, true, None, &[]), span)],
            StubResult::Plain(StubKind::Text) => vec![self.unknown_text(Unk::Any, span)],
            StubResult::Text { text } => {
                let (class, one) = match text.as_str() {
                    "digits" => (TextClass::DIGIT, true),
                    "number" => (TextClass::NUMBER, true),
                    "dimension" => (dimension, true),
                    "cs-free" => (TextClass(TextClass::ANY.0 & !TextClass::CS.0), false),
                    _ => (TextClass::ANY, false),
                };
                vec![self.unknown_text(Unk::Class(class, one, None, &[]), span)]
            }
            StubResult::Plain(StubKind::Box) | StubResult::Box { .. } => return None,
        })
    }

    /// A box side's interval from its two bounds; unbounded when absent.
    fn stub_dim(&mut self, bounds: &Option<[String; 2]>) -> crate::mode::Dim {
        let Some([lo, hi]) = bounds else { return crate::mode::Dim::ANY };
        let (lo, hi) = (self.stub_bound(lo, true), self.stub_bound(hi, false));
        crate::mode::Dim::range(lo, hi)
    }

    /// One bound: a length register's current interval end, or a dimension
    /// in `pt`, `bp`, `in`, `cm`, `mm` or `sp` (a bare number is `pt`).
    fn stub_bound(&mut self, text: &str, low: bool) -> i64 {
        let max = crate::value::MAX_DIMEN;
        let text = text.trim();
        if let Some(name) = text.strip_prefix('\\') {
            let sym = self.intern(name);
            let cell = match self.env.meaning(sym) {
                Meaning::Register(kind, n) => self.register_sym(kind, i64::from(n)),
                _ => sym,
            };
            return match self.env.num(cell) {
                Some((_, num)) => {
                    if low {
                        num.lo
                    } else {
                        num.hi
                    }
                }
                None => {
                    if low {
                        -max
                    } else {
                        max
                    }
                }
            };
        }
        let split = text.find(|c: char| c.is_alphabetic()).unwrap_or(text.len());
        let (number, unit) = text.split_at(split);
        let per_pt = match unit.trim() {
            "" | "pt" => 65536.0,
            "bp" => 65536.0 * 7227.0 / 7200.0,
            "in" => 65536.0 * 72.27,
            "cm" => 65536.0 * 72.27 / 2.54,
            "mm" => 65536.0 * 7.227 / 2.54,
            "sp" => 1.0,
            _ => return if low { -max } else { max },
        };
        match number.trim().parse::<f64>() {
            Ok(n) if low => (n * per_pt).floor() as i64,
            Ok(n) => (n * per_pt).ceil() as i64,
            Err(_) => {
                if low {
                    -max
                } else {
                    max
                }
            }
        }
    }

    fn read_arguments(&mut self, m: &MacroDef) -> Vec<Vec<Token>> {
        let outer = std::mem::replace(&mut self.long_argument, m.long);
        let interrupted = std::mem::take(&mut self.runaway);
        let scanner = std::mem::replace(&mut self.scanner, Scanner::Matching);
        let id = self.scan_begin();
        let mut arguments = match &m.arg_spec {
            Some(spec) => self.read_signature(spec),
            None => self.match_parameter_text(&m.parameter_text),
        };
        if self.scan_end(id) {
            let unknown = self.unknown_text(Unk::Any, self.position_span());
            if let Some(last) = arguments.last_mut() {
                last.push(unknown);
            }
        }
        self.scanner = scanner;
        self.long_argument = outer;
        if self.runaway == Runaway::No {
            self.runaway = interrupted;
        }
        arguments
    }

    /// The `n` undelimited arguments of a call satex runs without its
    /// macro (an expl3 function it steps over, a modeled `\use:…`), read
    /// as TeX reads a `\long` macro's (tex.web §§ 336-339, 392): a file
    /// ending first ends the scan there, a runaway, and the call is
    /// dropped. `None` then, said as TeX says it.
    pub(crate) fn read_call_arguments(&mut self, sym: Sym, n: usize, span: Span) -> Option<Vec<Vec<Token>>> {
        let outer = std::mem::replace(&mut self.long_argument, true);
        let interrupted = std::mem::take(&mut self.runaway);
        let scanner = std::mem::replace(&mut self.scanner, Scanner::Matching);
        let id = self.scan_begin();
        let mut arguments: Vec<Vec<Token>> = Vec::with_capacity(n);
        for _ in 0..n {
            if self.runaway != Runaway::No {
                break;
            }
            arguments.push(self.read_undelimited());
        }
        if self.scan_end(id) {
            let unknown = self.unknown_text(Unk::Any, self.position_span());
            if let Some(last) = arguments.last_mut() {
                last.push(unknown);
            }
        }
        self.scanner = scanner;
        self.long_argument = outer;
        let runaway = std::mem::replace(&mut self.runaway, interrupted);
        if runaway == Runaway::No {
            return Some(arguments);
        }
        if runaway == Runaway::FileEnded {
            let name = self.name(sym).to_string();
            let message = format!("File ended while scanning use of \\{name}: its argument never closes");
            self.diagnose(Severity::Warning, "runaway-argument", span, message);
        }
        None
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
        self.runaway = Runaway::Par;
        true
    }

    pub fn skip_spaces(&mut self) {
        while let Some(t) = self.next_token() {
            if !t.is_space() {
                self.unread(t);
                return;
            }
        }
    }

    /// The contents of a `{…}` group, braces stripped, nesting preserved.
    ///
    /// tex.web § 392: the group ends at the matching `}`, however far away.
    /// Argument scans expand nothing, so they read only what the input
    /// stack already holds or a file: they cannot run away, and no token
    /// budget cuts them short. Every read still costs a step.
    pub fn read_group_body(&mut self) -> Vec<Token> {
        let mut out = std::mem::take(&mut self.body);
        out.clear();
        let mut depth = 1usize;
        while let Some(t) = self.next_token() {
            if self.runaway_argument(t) {
                let unknown = self.unknown_argument(&out, t.span);
                self.body = out;
                return unknown;
            }
            match t.tok {
                Tok::Chr(_, Catcode::Begin) => depth += 1,
                Tok::Chr(_, Catcode::End) => {
                    depth -= 1;
                    if depth == 0 {
                        let body = out.to_vec();
                        self.body = out;
                        return body;
                    }
                }
                _ => {}
            }
            out.push(t);
        }
        let at = out.first().map(|t| t.span).unwrap_or_default();
        // A file that ended the scan is the call's error, reported there.
        if self.runaway != Runaway::FileEnded {
            self.diagnose(Severity::Unsupported, "unbalanced-group", at, "group never closes".into());
        }
        self.unread_all(&out);
        let unknown = self.unknown_argument(&out, at);
        self.body = out;
        unknown
    }

    /// Note an argument the call being described just read, if it read it
    /// from the file the call itself stands in and after the call.
    fn note_shape(&mut self, at: Span, item: &str) {
        let from_file = self.last_file;
        let Some(cs) = &mut self.call_shape else { return };
        let (_, call, shape, _) = Rc::make_mut(cs);
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
        let Some(cs) = self.call_shape.take() else { return };
        let (sym, span, shape, _) = Rc::try_unwrap(cs).unwrap_or_else(|rc| (*rc).clone());
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

    /// TeX's undelimited argument: one token, or a braced group with the outer
    /// braces removed (The TeXbook, ch. 20).
    pub fn read_undelimited(&mut self) -> Vec<Token> {
        self.skip_spaces();
        match self.next_token() {
            None => Vec::new(),
            Some(t) if t.is_cat(Catcode::Begin) => {
                self.note_shape(t.span, "{}");

                self.read_group_body()
            }
            // In text a cut left, no error ([`Machine::cut_right_brace`]).
            Some(t) if t.is_cat(Catcode::End) && self.after_cut() => {
                self.unread(t);
                self.cut_right_brace(t.span)
            }
            // An unmatched `}` can never become an argument (tex.web § 395):
            // the `\par` inserted before it is read instead.
            Some(t) if t.is_cat(Catcode::End) => {
                self.extra_right_brace(t);
                self.read_undelimited()
            }
            Some(t) if self.runaway_argument(t) => Vec::new(),
            // TeX reads the first digit (or `-`). A slice of several places
            // is its first place and the rest; a number is a path per sign,
            // zero and length, on which it is a first place and the rest,
            // slices of it, so `\__int_div_truncate:NwNw #1#2` puts it back
            // together whole ([`Machine::digit_arms`]).
            Some(t) if let Some(num) = t.cs().and_then(|sym| self.typed_digits_of(sym)) => {
                let sym = t.cs().expect("typed digits are a name");
                if let Some((whole, k, i, j)) = self.typed_slice(sym) {
                    if j - i > 1 {
                        let whole_num = self.typed_digits_of(whole).unwrap_or(num);
                        let first = self.slice_token(whole, whole_num, (k, i, i + 1), t.span);
                        let rest = self.slice_token(whole, whole_num, (k, i + 1, j), t.span);
                        self.unread_all(&[first, rest]);
                        return self.read_undelimited();
                    }
                    return vec![t];
                }
                if (0..=9).contains(&num.lo) && num.hi <= 9 {
                    return vec![t];
                }
                // A test narrowed it to one number: its digits.
                if num.lo == num.hi {
                    let digits = self.text_tokens(&num.lo.to_string(), t.span);
                    self.unread_all(&digits);
                    return self.read_undelimited();
                }
                // Digits of any length (a 32-bit value nothing bounds): a split
                // keeps nothing of their bound and costs a path per length,
                // so they stay one argument, an assumption said once below.
                let any = |lo: i64, hi: i64| lo <= 1 && hi >= 1_000_000_000;
                let unbounded = any(num.lo.max(1), num.hi)
                    && (num.lo >= 0 || any(num.hi.min(-1).saturating_neg(), num.lo.saturating_neg()));
                let arms = if unbounded { Vec::new() } else { self.digit_arms(sym, num, t.span) };
                if !unbounded
                    && self.split(sym, t.span, arms.len(), |m, arm| {
                        let (part, text) = &arms[arm];
                        m.refine_value(sym, crate::value::Value::from_num(crate::value::Quantity::Int, *part));
                        m.unread_all(text);
                        true
                    })
                {
                    return self.read_undelimited();
                }
                self.diagnose(
                    Severity::Assumption,
                    "digits-read-as-one-token",
                    t.span,
                    "a number of unknown digits was read as one argument where TeX reads its first digit; the macro is assumed to put it back together whole".into(),
                );
                vec![t]
            }
            // The rest of unknown text, perhaps none: a path where it is
            // empty, one where it is not ([`Machine::one_token`] reads that).
            Some(t) if let Some((class, false)) = self.typed_text_of(t) => {
                if !self.split_rest(t, class) {
                    return self.unknown_argument(&[t], t.span);
                }
                self.read_undelimited()
            }
            // One of several texts: the argument is that text's first token.
            Some(t) if self.one_of_matters(t) => match self.split_one_of(t) {
                true => self.read_undelimited(),
                // No more paths may part here: the token stands for its
                // texts as one argument, and said so. Its texts stay known
                // (unknown text instead would cut every scan they are in),
                // and the parameters after it are read as not in step.
                false => {
                    self.diagnose(
                        Severity::Assumption,
                        "one-of-read-as-one-token",
                        t.span,
                        "text that is one of several was read as one argument; where its texts have more tokens than one, the rest is assumed not to matter".into(),
                    );
                    vec![t]
                }
            },
            // A printed number's sign or first digit, its digits after it
            // ([`LEAD_MARK`]).
            Some(t) if t.tok == Tok::Chr(crate::tex::UNKNOWN_CHAR, Catcode::Other) => {
                let Some(rest) = self.next_token() else { return vec![t] };
                let first =
                    self.text_len(rest).filter(|_| self.is_lead(rest)).and_then(|n| self.lead_first(rest, true, n));
                if first.is_none() {
                    self.unread(rest);
                }
                vec![Token::new(first.unwrap_or(t.tok), t.span)]
            }
            // Typed text of one token or more is its first character, then
            // a rest of one character or none ([`Machine::one_token`]).
            Some(t) => {
                self.note_shape(t.span, "{}");
                vec![self.one_token(t)]
            }
        }
    }

    /// Unknown digits taken as one token (an undelimited argument, the
    /// token `\futurelet` looks at) are their first digit, one character
    /// of category 12. The rest, perhaps none, is read next (tex.web § 465
    /// prints a number as characters).
    pub(crate) fn one_token(&mut self, t: Token) -> Token {
        let Some((class, true)) = self.typed_text_of(t) else { return t };
        if let Some(c) = self.text_len(t).filter(|_| self.is_lead(t)).and_then(|n| self.lead_first(t, false, n)) {
            return Token::new(c, t.span);
        }
        // One token read, the rest holds one fewer: none at a bound of 1.
        let len = self.text_len(t).map(|n| n - 1);
        let rest = (len != Some(0)).then(|| self.unknown_text(Unk::Class(class, false, len, &[]), t.span));
        if let Some(c) = class.one_char() {
            if let Some(rest) = rest {
                self.unread(rest);
            }
            return Token::new(c, t.span);
        }
        // Text over two categories: a path per category of its first
        // character. Spaces, braces and names stay one token.
        let chars = Self::class_chars(class);
        let Some(sym) = t.cs().filter(|_| chars.len() > 1) else { return t };
        let mut first = None;
        if !self.split(sym, t.span, chars.len(), |_, arm| {
            first = Some(chars[arm]);
            true
        }) {
            return t;
        }
        if let Some(rest) = rest {
            self.unread(rest);
        }
        Token::new(first.expect("an arm was entered"), t.span)
    }

    /// The first character of the integer digits `t` ([`LEAD_MARK`]), at
    /// most `n` of them, after a sign or digit where `sign`: a path for
    /// `-` (every digit still to come), for `0` (nothing after it, no
    /// leading zero) and for 1-9 (at most `n - 1` digits after it), the
    /// rest put back. `None` where no more paths may part.
    fn lead_first(&mut self, t: Token, sign: bool, n: u32) -> Option<Tok> {
        let span = t.span;
        let other = |c| Tok::Chr(c, Catcode::Other);
        let mut first = None;
        // The rest is made on the path that takes it: made before, it would
        // spend a rest serial ([`REST_SERIALS`]) where no path parts.
        let parted = self.split(t.cs()?, span, 2 + usize::from(sign), |m, arm| {
            let rest = match arm {
                0 => None,
                1 => (n > 1).then(|| m.unknown_text(Unk::Class(TextClass::DIGIT, false, Some(n - 1), &[]), span)),
                _ => Some(m.lead_digits(true, n, span)),
            };
            first = Some(other(['0', crate::tex::UNKNOWN_NONZERO, '-'][arm]));
            if let Some(rest) = rest {
                m.unread(rest);
            }
            true
        });
        parted.then_some(first).flatten()
    }

    /// What a scan gives when it cannot stand behind what it read: text
    /// that is not known (tex.web § 392).  Dropping it would say the
    /// argument was empty, which the scan never earned.
    fn unknown_argument(&mut self, read: &[Token], span: Span) -> Vec<Token> {
        self.scan_abandoned = true;
        // A runaway scan drops the call and its arguments (tex.web § 396).
        if read.is_empty() || self.runaway != Runaway::No {
            return Vec::new();
        }
        // Typed by what the scan read: its classes and control sequences.
        vec![self.unknown_text(Unk::Of(&[read]), span)]
    }

    /// Whether `t` is wholly unknown text to a scan for `needle`: the
    /// unknown marker, or an unknown character that may be a token of
    /// `needle`. Not the rest of unknown text a scan split off: that is
    /// taken to hold no delimiter, so a loop that takes text apart ends
    /// (parting there again spends the command's choices on every pass,
    /// and hyperref's space loop then runs out of them).
    fn is_unknown_text(&self, t: Token, needle: &[Tok]) -> bool {
        t.tok == Tok::Cs(self.unknown)
            || matches!(t.tok, Tok::Chr(c, _) if crate::tex::is_unknown_char(c))
                && self.may_hold(t, needle) == Some(true)
    }

    /// Whether the argument a scan reads ends inside the wholly unknown
    /// text `t`: a path each way ([`Machine::branch`]). Where no more
    /// paths may part it does not, so the scan goes on to the real
    /// delimiter instead of leaving it behind.
    fn unknown_ends_argument(&mut self, t: Token) -> bool {
        matches!(self.branch(self.unknown, 2, t.span), Some(0))
    }

    /// What a test taught one arm of a typed rest `from`: its copies in
    /// the lists waiting above the nearest file become `to`, or go where
    /// it was empty. A copy is the same token from the same place.
    pub(crate) fn narrow_pending(&mut self, from: Token, to: Option<Token>) {
        let same = |t: &Token| t.tok == from.tok && t.span == from.span;
        let swap = |toks: &[Token]| -> Vec<Token> {
            toks.iter().filter_map(|t| if same(t) { to } else { Some(*t) }).collect()
        };
        let mut dropped = 0;
        for frame in self.input.iter_mut().rev() {
            match frame {
                Frame::Tokens { toks, pos, .. } => {
                    if toks[*pos..].iter().any(same) {
                        let mut new = toks[..*pos].to_vec();
                        new.extend(swap(&toks[*pos..]));
                        *toks = Rc::from(new);
                    }
                }
                Frame::Returned(tokens) => {
                    let before = tokens.len();
                    *tokens = swap(tokens);
                    dropped += before - tokens.len();
                }
                _ => break,
            }
        }
        self.held_tokens = self.held_tokens.saturating_sub(dropped);
    }

    /// The token lists waiting to be read above the nearest file, in
    /// reading order: what a scan sees without opening a file.
    fn pending_lists(&self) -> Vec<Token> {
        Self::pending_in(&self.input)
    }

    /// The most tokens `input`'s token lists may hold, typed text by its
    /// bound; `None` when none of it is bounded. A pass that begins with
    /// less than the one before ends by construction.
    pub(crate) fn input_bound(&self, input: &Input) -> Option<u32> {
        let lists = input.iter().rev().map_while(|frame| match frame {
            Frame::Tokens { toks, pos, .. } => Some(&toks[*pos..]),
            Frame::Returned(tokens) => Some(&tokens[..]),
            _ => None,
        });
        let (mut sum, mut bounded) = (0, false);
        for t in lists.flatten() {
            match self.text_len(*t) {
                Some(n) => (sum, bounded) = (sum + n, true),
                None if self.typed_shape(*t).is_none() && t.cs() != Some(self.unknown) => sum += 1,
                None => return None,
            }
        }
        bounded.then_some(sum)
    }

    fn pending_in(input: &Input) -> Vec<Token> {
        let mut rest = Vec::new();
        for frame in input.iter().rev() {
            match frame {
                Frame::Tokens { toks, pos, .. } => rest.extend_from_slice(&toks[*pos..]),
                Frame::Returned(tokens) => rest.extend(tokens.iter().rev().copied()),
                _ => break,
            }
        }
        rest
    }

    /// A scan for `delimiter` at level 0 over `text`: where the delimiter
    /// starts, or `None` when a `}` at level 0 comes first or it is not
    /// there.
    fn find_delimiter(text: &[Token], delimiter: &[Tok]) -> Option<usize> {
        let mut depth = 0usize;
        for (i, t) in text.iter().enumerate() {
            match t.tok {
                Tok::Chr(_, Catcode::Begin) => depth += 1,
                Tok::Chr(_, Catcode::End) if depth == 0 => return None,
                Tok::Chr(_, Catcode::End) => depth -= 1,
                _ => {}
            }
            if depth == 0 && text[i..].iter().zip(delimiter).filter(|(t, d)| t.tok == **d).count() == delimiter.len() {
                return Some(i);
            }
        }
        None
    }

    /// A delimited scan that read `out` and now meets `t`, one of several
    /// texts that may hold `delimiter`: each text is looked at in its
    /// place. A text the delimiter lies inside ends the argument there,
    /// with the text's prefix in it and its rest read next. `Unknown`
    /// when a text is not known in full, the delimiter is not in one of
    /// the texts or straddles a text's end, or no more paths may part:
    /// the caller then ends the argument at `t`, read as unknown text.
    fn scan_one_of(&mut self, t: Token, delimiter: &[Tok], out: &[Token], tail_free: bool) -> OneOfScan {
        let Some(texts) = self.one_of_members(t) else { return OneOfScan::Unknown };
        // Unknown text in a member hides the delimiter, unless typed so
        // that it cannot hold it.
        if texts.iter().any(|x| {
            x.iter().enumerate().any(|(i, t)| {
                let prev = match i {
                    0 if tail_free => None,
                    0 => out.last(),
                    _ => x.get(i - 1),
                };
                t.cs().is_some_and(|s| self.is_unknown_marker(s))
                    && self.typed_may_hold(*t, Some(prev.copied()), delimiter)
            })
        }) {
            return OneOfScan::Unknown;
        }
        // What follows `t` in the token lists above the file: as far as
        // the delimiter can be seen. A text ending in a beginning of the
        // delimiter whose end is not in view may straddle it.
        let ahead = self.pending_lists();
        let straddles = |text: &[Token]| {
            (1..delimiter.len()).any(|k| {
                text.len() >= k
                    && text[text.len() - k..].iter().zip(delimiter).all(|(t, d)| t.tok == *d)
                    && ahead.len() < delimiter.len() - k
            })
        };
        let mut inside: Vec<usize> = Vec::new();
        let mut far: Vec<usize> = Vec::new();
        for (i, text) in texts.iter().enumerate() {
            let whole: Vec<Token> = text.iter().chain(&ahead).copied().collect();
            match Self::find_delimiter(&whole, delimiter) {
                Some(at) if at + delimiter.len() <= text.len() => inside.push(i),
                Some(at) if at >= text.len() => far.push(i),
                Some(_) => return OneOfScan::Unknown,
                None if explore::balanced(text) && !straddles(text) => far.push(i),
                None => return OneOfScan::Unknown,
            }
        }
        if !far.is_empty() {
            // No text holds it: the token is one more token of the argument.
            if inside.is_empty() {
                return OneOfScan::Pass;
            }
            // Some do: two paths, each with the token for one group of
            // texts. Read as unknown text instead, the scan would end in
            // it on every pass of a loop over it and never get past.
            let Some(sym) = t.cs() else { return OneOfScan::Unknown };
            let groups: [Vec<Rc<[Token]>>; 2] = [far, inside].map(|g| g.iter().map(|&i| texts[i].clone()).collect());
            let parted = self.split(sym, t.span, 2, |m, arm| {
                match &groups[arm][..] {
                    [one] => m.unread_all(one),
                    texts => {
                        let token = m.one_of(texts.to_vec(), t.span);
                        m.unread(token);
                    }
                }
                true
            });
            return if parted { OneOfScan::Again } else { OneOfScan::Unknown };
        }
        let one = |m: &mut Self, parts: Vec<Rc<[Token]>>| -> Vec<Token> {
            let first = &parts[0];
            match parts.iter().all(|p| p.iter().map(|t| t.tok).eq(first.iter().map(|t| t.tok))) {
                true => first.to_vec(),
                false => vec![m.one_of(parts, t.span)],
            }
        };
        let mut argument = out.to_vec();
        let mut prefixes = Vec::new();
        let mut rests = Vec::new();
        for &i in &inside {
            let at = Self::find_delimiter(&texts[i], delimiter).expect("found above");
            prefixes.push(Rc::from(&texts[i][..at]));
            rests.push(Rc::from(&texts[i][at + delimiter.len()..]));
        }
        let back = one(self, rests);
        self.unread_all(&back);
        argument.extend(one(self, prefixes));
        OneOfScan::Argument(strip_braces(argument))
    }

    /// Whether `t` stands for one of several texts ([`Machine::one_of`])
    /// that a scan for `needle` cannot pass as one token: one of them may
    /// hold a token of the delimiter, or may be empty, so the delimiter
    /// could straddle it.
    fn one_of_hides(&self, t: Token, needle: &[Tok], before: Option<Option<Token>>) -> bool {
        let Some(texts) = self.one_of_members(t) else { return false };
        // A text that may be empty lets the delimiter straddle it, where
        // what came before may begin the delimiter.
        let straddles = match before {
            None => true,
            Some(None) => false,
            Some(Some(p)) => {
                delimiter_prefix(needle).contains(&p.tok) || self.may_hold(p, needle).is_some_and(|may| may)
            }
        };
        if straddles && (before.is_none() || needle.len() > 1) && self.token_class(t, false).empty {
            return true;
        }
        texts.iter().any(|text| {
            text.iter().enumerate().any(|(i, x)| match x.tok {
                _ if self.may_hold(*x, needle).is_some() => {
                    let prev = if i == 0 { before } else { Some(text.get(i - 1).copied()) };
                    self.typed_may_hold(*x, prev, needle)
                }
                _ => {
                    let class = self.token_class(*x, false);
                    class.empty || needle.iter().any(|d| class.may_be(*d))
                }
            })
        })
    }

    /// Everything up to the next occurrence of `delimiter` at brace level 0.
    fn read_delimited(&mut self, delimiter: &[Tok]) -> Vec<Token> {
        let mut out: Vec<Token> = Vec::new();
        let mut depth = 0usize;
        // Set once unknown text is read inside a group: it may hold a `{`
        // or `}`, so `depth` is only a guess from there on and the
        // delimiter found later is not known to be the real one.
        let mut fuzzy: Option<Span> = self.scan_fuzzy.take();
        // Where unknown text at level 0 stands in `out`, on the path that
        // reads on past it: a `}` met before the delimiter then means the
        // argument ended in that text after all.
        let mut unknown_at: Option<usize> = None;
        // How many of the tokens just read stand in a project file.
        let mut from_file = 0usize;
        // Read on past unknown text at level 0 and no delimiter before a
        // `\par` or the end of the file: the argument ended in that text
        // after all (the other way TeX stops with an error), so what was
        // read after it goes back, for the scans it belongs to. The call
        // thus ends in its own text instead of eating the file.
        let ends_in_unknown = |m: &mut Self, out: &mut Vec<Token>, k: usize, span: Span, end: &str| -> Vec<Token> {
            let tail = out.split_off(k + 1);
            m.unread_all(&tail);
            m.unread_rest(out[k], delimiter, span);
            m.scan_abandoned = true;
            m.diagnose(
                Severity::Imprecision,
                "argument-ends-in-unknown",
                span,
                format!("a delimited argument ran on to {end} after unknown text; it is taken to end in that text"),
            );
            strip_braces(std::mem::take(out))
        };
        // Where a token for one of several texts stands in `out`, and how
        // deep the conditionals read into the argument nest: an argument
        // that reads past such a token into the `\fi` of a conditional
        // open around the call takes the closer of a level it is in. Its
        // texts were joined apart from the tests that shaped them, so that
        // is the join's artifact: the argument ends in that text instead.
        let mut joined_at: Option<usize> = None;
        let mut nesting = CondDepth::default();
        // Where short typed text stands in `out` ([`Machine::straddles`]).
        let mut straddled: Vec<usize> = Vec::new();
        // Unknown or one-of tokens of the delimiter match any token they
        // may be, so the argument ends at the earliest possible end.
        let span = self.position_span();
        let vague: Vec<Option<classes::TokenClass>> = (delimiter.iter())
            .map(|&d| Token::new(d, span))
            .map(|d| d.cs().filter(|&s| self.is_unknown_marker(s)).map(|_| self.token_class(d, false)))
            .collect();
        let matches = |i: usize, t: &Token| {
            t.tok == delimiter[i]
                || vague[i]
                    .as_ref()
                    .is_some_and(|c| !matches!(t.tok, Tok::Chr(_, Catcode::Begin | Catcode::End)) && c.may_be(t.tok))
        };
        loop {
            let Some(t) = self.next_token() else {
                if let Some(k) = unknown_at
                    && k < out.len()
                {
                    let span = out[k].span;
                    return ends_in_unknown(self, &mut out, k, span, "the end of the file");
                }
                break;
            };
            // Past unknown text at level 0, source text is never read into
            // the argument: it ends in that text, so an approximation
            // gobbles no file.
            if self.last_file == Some(t.span.file)
                && let Some(k) = unknown_at
                && k < out.len()
            {
                self.unread(t);
                let span = out[k].span;
                return ends_in_unknown(self, &mut out, k, span, "source text");
            }
            from_file = match self.last_file == Some(t.span.file) && self.own_file(t.span.file) {
                true => from_file + 1,
                false => 0,
            };
            if self.runaway_argument(t) {
                if let Some(k) = unknown_at
                    && k < out.len()
                {
                    self.runaway = Runaway::No;
                    return ends_in_unknown(self, &mut out, k, t.span, "a paragraph end");
                }
                return self.unknown_argument(&out, t.span);
            }
            // Unknown text may hold the delimiter followed by more text: cut
            // the argument there and read the rest next, taken to hold no
            // delimiter, so the split happens once. `UNKNOWN_CHAR` is the
            // same case as a character rather than a control sequence: an
            // unbounded run of unknown digits (`\the`/`\number` of an
            // unknown dimen or count) may itself hide the delimiter.
            // Without this a corrupted column preamble with an unknown
            // width is not caught, and the scan hunts character by
            // character until the line guard stops it.
            // Typed unknown text is unknown text to the scan only where its
            // class may hold a brace or the delimiter's first token: the
            // delimiter may begin in it.
            let first = &delimiter[..1.min(delimiter.len())];
            // Inside a group no delimiter matches (tex.web § 392): there
            // unknown text matters only where its class may hold a brace,
            // which may close the group. Typed text of characters is read
            // on exactly, so the argument stays typed by what it read.
            // Typed text shorter than the delimiter and without braces
            // cannot hold all of it: the delimiter may only straddle it,
            // which the tokens read after it decide ([`straddles`]), so the
            // text is read on as it is (an unknown `\escapechar` before a
            // name `\string` printed).
            let short = match (depth, self.typed_text_of(t), self.text_len(t)) {
                // One such text at a time: two near each other may hold
                // the delimiter between them.
                (0, Some(_), Some(n))
                    if (n as usize) < delimiter.len()
                        && self.may_hold(t, &[]) == Some(false)
                        && straddled.last().is_none_or(|&q| out.len() - q >= delimiter.len()) =>
                {
                    Some(n as usize)
                }
                _ => None,
            };
            let unknown_text = short.is_none()
                && (self.is_unknown_text(t, first) || (t.cs().is_some() && self.may_hold(t, first) == Some(true)))
                && (depth == 0 || self.may_hold(t, &[]) != Some(false));
            // The argument read so far ends in the delimiter's first `j`
            // tokens and the text may begin with the rest (tex.web § 392):
            // the delimiter ends in it, a path each way. Text too short for
            // the rest leaves it to what comes after.
            let mut tail_free = true;
            if depth == 0
                && let Some(j) = (1..delimiter.len()).rev().find(|&j| {
                    out.len() >= j
                        && short.is_none_or(|n| delimiter.len() - j <= n)
                        && out[out.len() - j..].iter().zip(delimiter).all(|(o, d)| o.tok == *d)
                        && self.may_begin(t, &delimiter[j..])
                })
            {
                match self.branch(self.unknown, 2, t.span) {
                    Some(0) => {
                        out.truncate(out.len() - j);
                        self.unread_rest(t, &delimiter[j..], t.span);
                        self.scan_abandoned = true;
                        return strip_braces(out);
                    }
                    Some(_) => self.arm_assumed = true,
                    None => {
                        fuzzy = fuzzy.or(Some(t.span));
                        tail_free = false;
                    }
                }
            }
            // Past that test the delimiter cannot begin before `t`.
            let before = match tail_free {
                true => Some(None),
                false => Some(out.last().copied()),
            };
            // A token for one of several texts is balanced, but at level 0
            // the delimiter may be among its texts. A path per text would
            // be exact, but multiplies the paths of every loop over such a
            // text (pgfmath's long division), so it is read as unknown text.
            let mut passes = false;
            if !unknown_text && depth == 0 && self.one_of_hides(t, delimiter, before) {
                match self.scan_one_of(t, delimiter, &out, tail_free) {
                    OneOfScan::Argument(argument) => return argument,
                    OneOfScan::Pass => passes = true,
                    OneOfScan::Again => continue,
                    OneOfScan::Unknown => {}
                }
            }
            if !passes && (unknown_text || (depth == 0 && self.one_of_hides(t, delimiter, before))) {
                // Wholly unknown text at level 0 may end the argument or be
                // part of it, the delimiter after: a path each way, and the
                // second where no more paths may part, since it never leaves
                // the real delimiter behind to run as text. One of several
                // texts ends it: the rest of the text is read next.
                let ends = match depth == 0 && unknown_text {
                    true => self.branch(self.unknown, 2, t.span),
                    false => None,
                };
                if depth == 0 && (!unknown_text || ends == Some(0)) {
                    out.push(t);
                    self.unread_rest(t, delimiter, t.span);
                    // The delimiter may be inside the unknown text just
                    // split off: this scan gave up, not found it. A caller
                    // that keeps re-entering without shrinking is the
                    // unknown marker expanding into itself, caught cheaply
                    // by `abandon_repeat` rather than the full recursion budget.
                    self.scan_abandoned = true;
                    return strip_braces(out);
                }
                // Inside a group the text may hold the `}` that closes it,
                // so where the argument ends is not known from here on. At
                // level 0 the arm that holds no delimiter reads on exactly:
                // the other arm took the split.
                match ends {
                    Some(1) => self.arm_assumed = true,
                    _ => fuzzy = fuzzy.or(Some(t.span)),
                }
                if depth == 0 && unknown_text {
                    unknown_at = unknown_at.or(Some(out.len()));
                }
            }
            if nesting.feed(self.cond_prim(t)).is_some()
                && !self.conds.is_empty()
                && let Some(k) = joined_at.or(unknown_at)
            {
                self.unread(t);
                return ends_in_unknown(self, &mut out, k, t.span, "the end of a conditional open around it");
            }
            if depth == 0 && joined_at.is_none() && self.one_of_members(t).is_some() {
                joined_at = Some(out.len());
            }
            match t.tok {
                Tok::Chr(_, Catcode::Begin) => depth += 1,
                // A `}` that closes a group the argument did not open ends the
                // call: TeX reports "Argument of \x has an extra }" and
                // abandons it (tex.web § 395).  Reading on would swallow the
                // rest of the file looking for a delimiter that is not there.
                Tok::Chr(_, Catcode::End) if depth == 0 => {
                    self.unread(t);
                    // Read on past unknown text at level 0 and no delimiter
                    // before this `}`: the argument ended in that text (the
                    // other way TeX stops with an error), so what was read
                    // after it goes back, for the scans it belongs to.
                    if let Some(k) = unknown_at {
                        let tail = out.split_off(k + 1);
                        self.unread_all(&tail);
                        self.unread_rest(out[k], delimiter, t.span);
                        self.scan_abandoned = true;
                        self.diagnose(
                            Severity::Imprecision,
                            "argument-ends-in-unknown",
                            t.span,
                            "a delimited argument ran into the end of a group after unknown text; it is taken to end in that text".into(),
                        );
                        return strip_braces(out);
                    }
                    // Unknown text was read here, so this `}` may close a
                    // group that text opened. The engine never met this
                    // end; reporting it as the engine's error would be one
                    // satex invented.
                    if fuzzy.is_none() {
                        if self.after_cut() {
                            return self.cut_right_brace(t.span);
                        }
                        self.extra_right_brace(t);
                        continue;
                    }
                    self.diagnose(
                        Severity::Imprecision,
                        "argument-ends-in-unknown",
                        t.span,
                        "a delimited argument ran into the end of a group after unknown text; \
                         where it really ends is not known"
                            .into(),
                    );
                    return self.unknown_argument(&out, t.span);
                }
                Tok::Chr(_, Catcode::End) => depth -= 1,
                _ => {}
            }
            if short.is_some() {
                straddled.push(out.len());
            }
            out.push(t);
            // A delimiter that straddles short typed text read before: a
            // path where it ends here, and one where the text holds other
            // characters.
            if depth == 0
                && let Some((p, cut)) =
                    straddled.iter().rev().find_map(|&p| self.straddles(&out, p, delimiter).map(|c| (p, c)))
            {
                match self.branch(self.unknown, 2, t.span) {
                    Some(0) => {
                        let rest = out[p];
                        out.truncate(p - cut.0);
                        if cut.0 == 0
                            && let Some((class, _)) = self.typed_text_of(rest)
                            && let Some(n) = self.text_len(rest).map(|n| n.saturating_sub(cut.1 as u32))
                            && n > 0
                        {
                            out.push(self.unknown_text(Unk::Class(class, false, Some(n), &[]), rest.span));
                        }
                        self.delimiter_met = true;
                        if let Some(span) = fuzzy {
                            return self.unknown_argument(&out, span);
                        }
                        return strip_braces(out);
                    }
                    Some(_) => self.arm_assumed = true,
                    None => fuzzy = fuzzy.or(Some(t.span)),
                }
            }
            if depth == 0 && out.len() >= delimiter.len() {
                let at = out.len() - delimiter.len();
                if out[at..].iter().enumerate().all(|(i, t)| matches(i, t)) {
                    if out[at..].iter().zip(delimiter).any(|(t, d)| t.tok != *d) {
                        let message =
                            "a delimiter of unknown text is taken to end the argument where it may first match".into();
                        self.diagnose(Severity::Assumption, "unknown-delimiter", out[at].span, message);
                    }
                    if from_file >= delimiter.len() && out[at].cs().is_some() {
                        self.carried.absorbed = Some(out[at].span);
                    }
                    out.truncate(at);
                    self.delimiter_met = true;
                    if let Some(span) = fuzzy {
                        return self.unknown_argument(&out, span);
                    }
                    return strip_braces(out);
                }
            }
        }
        self.probe_wants(&out, delimiter, crate::probe::Want::Until);
        // A file that ended the scan has consumed them (tex.web § 396).
        if self.runaway == Runaway::No {
            self.unread_all(&out);
        }
        Vec::new()
    }

    /// Whether `delimiter` may end `out` straddling the short typed text
    /// at `p`, with at least one token after it: the `j` tokens before it
    /// begin the delimiter, the text holds the next `m` (all of it where
    /// `j > 0`, its end otherwise) and the tokens after it are the rest,
    /// each exactly (tex.web § 392). The tokens before it that belong to
    /// the delimiter and how many the text gives it.
    fn straddles(&self, out: &[Token], p: usize, delimiter: &[Tok]) -> Option<(usize, usize)> {
        let after = out.len().checked_sub(p + 1).filter(|&n| n > 0 && n < delimiter.len())?;
        let len = self.text_len(out[p])? as usize;
        let class = self.token_class(out[p], false);
        if out[p + 1..].iter().zip(&delimiter[delimiter.len() - after..]).any(|(o, d)| o.tok != *d) {
            return None;
        }
        let inside = delimiter.len() - after;
        (0..=inside.min(p)).find_map(|j| {
            let m = inside - j;
            let fits = m <= len && (m > 0 || j > 0);
            let before = out[p - j..p].iter().zip(delimiter).all(|(o, d)| o.tok == *d);
            let held = delimiter[j..inside].iter().all(|d| class.may_be(*d));
            // Empty text: only where it may hold nothing.
            let empty = m > 0 || class.empty;
            (fits && before && held && empty).then_some((j, m))
        })
    }

    /// `\def\a#1#{…}`: the final argument stops at the begin-group character,
    /// which the replacement text puts back (tex.web § 476).
    fn read_before_group(&mut self) -> Vec<Token> {
        let mut out: Vec<Token> = Vec::new();
        while let Some(t) = self.next_token() {
            if self.runaway_argument(t) {
                return self.unknown_argument(&out, t.span);
            }
            // One of several texts, one of which may hold the `{`: a path
            // per text, or the argument ends in it.
            let unknown_text = self.is_unknown_text(t, &[Tok::Chr('{', Catcode::Begin)]);
            if unknown_text && !self.unknown_ends_argument(t) {
                out.push(t);
                continue;
            }
            if !unknown_text && self.one_of_hides(t, &[Tok::Chr('{', Catcode::Begin)], None) {
                match self.scan_one_of(t, &[Tok::Chr('{', Catcode::Begin)], &out, false) {
                    OneOfScan::Argument(argument) => return argument,
                    OneOfScan::Again => continue,
                    OneOfScan::Pass | OneOfScan::Unknown => {}
                }
            }
            if unknown_text || self.one_of_hides(t, &[Tok::Chr('{', Catcode::Begin)], None) {
                out.push(t);
                self.unread_rest(t, &[Tok::Chr('{', Catcode::Begin)], t.span);
                self.scan_abandoned = true;
                return strip_braces(out);
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
                    self.extra_right_brace(t);
                    continue;
                }
                _ => out.push(t),
            }
        }
        let open = [Tok::Chr('{', Catcode::Begin)];
        self.probe_wants(&out, &open, crate::probe::Want::Until);
        // A file that ended the scan has consumed them (tex.web § 396).
        if self.runaway == Runaway::No {
            self.unread_all(&out);
        }
        Vec::new()
    }

    fn match_parameter_text(&mut self, text: &ParameterText) -> Vec<Vec<Token>> {
        let mut args: Vec<Vec<Token>> = vec![Vec::new(); text.arity as usize];
        let mut i = 0;
        // Set once an undelimited parameter read unknown text, which is one
        // token standing for any number: where the call's later parameters
        // begin is not known from there on (tex.web § 392).
        let mut fuzzy: Option<Span> = None;
        // The unknown text that made the scan fuzzy.
        let mut hole: Option<Token> = None;
        while i < text.items.len() {
            match &text.items[i] {
                ParamItem::Lit(expected) => {
                    let t = self.next_token();
                    // Unknown text may begin with the delimiter: it is
                    // matched, and the rest of that text is read next.
                    // One of several texts: a path per text. Unknown text:
                    // it begins with the delimiter (the rest of it is read
                    // next), or it is empty and the delimiter follows.
                    // One of several texts: each text is matched in its
                    // place, and where they differ a path per outcome.
                    if let Some(t) = t
                        && !self.is_unknown_text(t, &[*expected])
                        && self.one_of_hides(t, &[*expected], None)
                    {
                        match self.scan_one_of(t, &[*expected], &[], false) {
                            OneOfScan::Argument(argument) if argument.is_empty() => {
                                i += 1;
                                continue;
                            }
                            // tex.web § 398: text before the delimiter does
                            // not match the definition.
                            OneOfScan::Argument(before) => {
                                self.unread_all(&before);
                                self.probe_wants(&before, &[*expected], crate::probe::Want::Literal);
                                return args;
                            }
                            OneOfScan::Again => continue,
                            OneOfScan::Unknown | OneOfScan::Pass => {}
                        }
                    }
                    // Unknown text: it begins with the delimiter, or it is
                    // empty and the delimiter follows (a path each way).
                    if let Some(t) = t
                        && self.is_unknown_text(t, &[*expected])
                        && !self.unknown_ends_argument(t)
                    {
                        continue;
                    }
                    if let Some(t) = t
                        && (self.is_unknown_text(t, &[*expected]) || self.one_of_hides(t, &[*expected], None))
                    {
                        while let Some(ParamItem::Lit(_)) = text.items.get(i) {
                            i += 1;
                        }
                        self.unread_rest(t, &[*expected], t.span);
                        continue;
                    }
                    // After unknown text the literal may be inside it: what
                    // was read is given back, and the parameters go on.
                    if let Some(t) = t
                        && t.tok != *expected
                        && fuzzy.is_some()
                    {
                        self.unread(t);
                        while let Some(ParamItem::Lit(_)) = text.items.get(i) {
                            i += 1;
                        }
                        self.scan_abandoned = true;
                        continue;
                    }
                    if let Some(t) = t
                        && t.tok != *expected
                    {
                        // tex.web § 398: "Use of \x doesn't match its
                        // definition". A probe learns what was wanted.
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
                        self.scan_fuzzy = fuzzy;
                        self.delimiter_met = false;
                        let arg = self.read_delimited(&delimiter);
                        // Text that holds no token of the delimiter ends
                        // before the one met: the parameters after it
                        // stand where they are read (tex.web § 392).
                        if self.delimiter_met
                            && let Some(h) = hole
                            && !delimiter.iter().any(|d| self.token_class(h, false).may_be(*d))
                        {
                            (fuzzy, hole) = (None, None);
                        }
                        arg
                    } else if last && text.brace_end {
                        self.read_before_group()
                    } else if let Some(at) = fuzzy {
                        // The token may be anywhere in the unknown text.
                        self.scan_abandoned = true;
                        vec![match hole {
                            Some(h) => self.unknown_text(Unk::Of(&[&[h]]), at),
                            None => self.unknown_text(Unk::Any, at),
                        }]
                    } else {
                        // A bare token of unknown text (not `{…}`, which is
                        // one argument whatever it holds).
                        self.skip_spaces();
                        let bare = self
                            .peek()
                            .and_then(|t| t.cs())
                            .is_some_and(|sym| sym == self.unknown || self.is_unknown_name(sym));
                        let arg = self.read_undelimited();
                        if bare && let [t] = arg[..] {
                            fuzzy = Some(t.span);
                            hole = Some(t);
                        }
                        arg
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
        if text.brace_end
            && !matches!(text.items.last(), Some(ParamItem::Param(_)))
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
    /// count. The text the macros hand each other is not the caller's.
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
        while let Some(t) = self.next_token() {
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
    /// control sequence. Unlike a LaTeX macro argument it never looks inside
    /// a group, so `\edef~{…}` on an active character defines nothing here
    /// rather than the first name the group happens to contain.
    pub fn read_r_token(&mut self) -> Option<Sym> {
        self.skip_spaces();
        let mut t = self.next_token()?;
        // One of several names: a path per name.
        if let Some(members) = self.one_of_members(t)
            && let Some(names) = members
                .iter()
                .map(|m| match **m {
                    [n @ Token { tok: Tok::Cs(_) | Tok::Chr(_, Catcode::Active), .. }] => Some(n),
                    _ => None,
                })
                .collect::<Option<Vec<Token>>>()
        {
            let mut arm = None;
            if self.split(t.cs().expect("one-of"), t.span, names.len(), |_, i| {
                arm = Some(i);
                i < names.len()
            }) {
                t = Token { span: t.span, ..*arm.and_then(|i| names.get(i))? };
            }
        }
        self.name_token = Some(t);
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
                        let end = rest.iter().position(
                            |t| matches!(t.tok, Tok::Cs(s) if self.env.meaning(s).prim() == Some(Primitive::Endcsname)),
                        );
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

    /// Skip the text of an arm that is not taken, as `pass_text` does
    /// (tex.web § 494): nested conditionals are counted, nothing is executed.
    /// Stops at `\fi`, and at `\else`/`\or` unless `to_fi`. Returns the
    /// primitive that ended the skip.
    pub fn pass_text(&mut self, to_fi: bool, at: Span) -> Option<(Primitive, Span)> {
        let scanner = std::mem::replace(&mut self.scanner, Scanner::Skipping);
        let id = self.scan_begin();
        let found = self.pass_text_in(to_fi, at);
        self.scan_end(id);
        self.scanner = scanner;
        found
    }

    /// [`Self::pass_text`] for a conditional whose own test left conditionals
    /// open above it (tex.web § 498).  tex.web § 500: while it skips, a `\fi`
    /// of one of those ends that one and the skip goes on, and an `\or` or
    /// `\else` of one is passed over.  `base` is how deep the condition stack
    /// was before the conditional was read.
    pub(crate) fn pass_outer(&mut self, to_fi: bool, at: Span, base: usize) -> Option<(Primitive, Span)> {
        loop {
            match self.pass_text(to_fi, at)? {
                (Primitive::Fi, mark) if self.conds.len() > base => {
                    if let Some(inner) = self.conds.last() {
                        self.note_cond_mark(inner.at, mark, true);
                    }
                    self.pop_conditional();
                }
                (Primitive::Else | Primitive::Or, _) if self.conds.len() > base => {}
                found => return Some(found),
            }
        }
    }

    /// What skipped text sees of `t` (tex.web § 494): only control
    /// sequences, by [`Env::skip_meaning`].
    pub(crate) fn cond_prim(&self, t: Token) -> Option<Primitive> {
        let Tok::Cs(sym) = t.tok else { return None };
        self.env.skip_meaning(sym).prim()
    }

    /// Skipping holds and expands nothing, so it reads only what the input
    /// stack holds or a file, and no token budget cuts it short.
    fn pass_text_in(&mut self, to_fi: bool, at: Span) -> Option<(Primitive, Span)> {
        let mut nesting = CondDepth::default();
        while let Some(t) = self.next_token() {
            if let Some(log) = &mut self.skip_log {
                log.push(t);
            }
            // Skipping crossed into another file's text: a misjudged test or
            // `\fi` would otherwise swallow the document. TeX inserts `\fi`
            // where a file ends inside a skip (tex.web § 336); satex ends the
            // skip here as though it had, and leaves a marker that splits off
            // the path skipping on as TeX does. Only after a gap: without one
            // the skip is TeX's own on purpose (versions.sty, comment.sty),
            // not a lost `\fi`.
            let unsure = self.scratch.cond_gaps.get(&at).is_none_or(|&gaps| gaps != self.gaps_raised);
            if unsure && self.last_file.is_some_and(|f| f != at.file) {
                self.unread(t);
                // A library conditional skipping on into the document's text
                // is no run of it (`analysis.assume_compiles`): it ends here.
                let ends = self.cfg.compiles() && !self.own_file(at.file) && self.own_file(t.span.file);
                let name = self.out.short_name(at.file).to_string();
                let on = if ends { "it ends there" } else { "both its end there and the skip on are analyzed" };
                self.diagnose(
                    Severity::Imprecision,
                    "conditional-crosses-file",
                    t.span,
                    format!("skipping a conditional begun at {name}:{} reached another file's text; {on}", at.line),
                );
                if ends {
                    return Some((Primitive::Fi, t.span));
                }
                let nested = u8::try_from(nesting.0).unwrap_or(u8::MAX);
                let marker = self.intern(&format!("\u{2}skip crossing:{nested}:{to_fi}"));
                let meaning = Meaning::Primitive(Primitive::SkipCrossing { nested, to_fi });
                self.env.set(marker, crate::env::Binding::builtin(meaning), true);
                self.unread(Token::new(Tok::Cs(marker), t.span));
                return Some((Primitive::Fi, t.span));
            }
            match nesting.feed(self.cond_prim(t)) {
                Some(Primitive::Fi) => return Some((Primitive::Fi, t.span)),
                Some(kind) if !to_fi => return Some((kind, t.span)),
                _ => {}
            }
        }
        self.diagnose(
            Severity::Unsupported,
            "unterminated-conditional",
            at,
            "no matching \\fi before the input ends".into(),
        );
        None
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
            align_state: self.align_state,
            cell_base: self.cell_base,
            section: self.section.clone(),
            call_shape: self.call_shape.clone(),
            summary_of: self.scratch.summary_of.clone(),
            loop_bounds: self.scratch.loop_bounds.clone(),
            edef_depth: self.edef_depth,
            pdf_string: self.pdf_string,
            last_file: self.last_file,
            file_call: self.file_call,
            source_progress: self.source_progress,
            lines_read: self.lines_read,
            file_depth: self.file_depth,
            halted: self.halted,
            streams: self.streams.clone(),
            outputs: self.outputs.clone(),
            loaded: self.loaded.clone(),
            mode: self.mode,
            list: self.list,
            material: self.material,
            page_touched: self.page_touched,
            page_grew: self.page_grew,
            output_active: self.output_active,
            natural: self.natural,
            main_tail: self.main_tail,
            wild: self.wild.clone(),
            quantity: self.quantity.clone(),
            noexpanded: self.noexpanded,
            last_named_cs: self.last_named_cs,
            source: self.source,
            identifying: self.identifying,
            recent_file_spans: self.recent_file_spans.clone(),
            file_calls: self.file_calls.clone(),
            reader: self.out.graph.reader,
            scans: self.scans.clone(),
            at_join: self.at_join,
            fi_mark: self.fi_mark,
            approximated: self.approximated,
            cut_taint: self.cut_taint,
            call_recs: self.call_recs.clone(),
            guard_seen: self.guard_seen,
            job_ended: self.job_ended,
        }
    }

    fn restore_path(&mut self, state: PathState) {
        // How often a name is being expanded is the number of its levels on
        // the input stack, so it follows the input instead of being copied.
        for (frames, delta) in [(std::mem::take(&mut self.input), -1), (state.input.clone(), 1)] {
            for frame in &frames {
                match frame {
                    Frame::Tokens { expanding: Some(sym), upper: false, .. } => self.bump_expanding(*sym, delta),
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
        self.quantity = state.quantity;
        self.noexpanded = state.noexpanded;
        self.last_named_cs = state.last_named_cs;
        self.source = state.source;
        self.identifying = state.identifying;
        self.recent_file_spans = state.recent_file_spans;
        self.file_calls = state.file_calls;
        self.out.graph.reader = state.reader;
        self.scans = state.scans;
        self.at_join = state.at_join;
        self.fi_mark = state.fi_mark;
        self.approximated = state.approximated;
        self.cut_taint = state.cut_taint;
        self.call_recs = state.call_recs;
        self.guard_seen = state.guard_seen;
        self.job_ended = state.job_ended;
        if self.call_recs.is_empty() {
            self.env.log_reads(false);
        }
        self.held_tokens = state.held_tokens;
        self.align_state = state.align_state;
        self.cell_base = state.cell_base;
        self.section = state.section;
        self.call_shape = state.call_shape;
        self.scratch.summary_of = state.summary_of;
        self.scratch.loop_bounds = state.loop_bounds;
        self.edef_depth = state.edef_depth;
        self.pdf_string = state.pdf_string;
        self.last_file = state.last_file;
        self.file_call = state.file_call;
        (self.source_progress, self.lines_read) = (state.source_progress, state.lines_read);
        self.file_depth = state.file_depth;
        self.halted = state.halted;
        self.streams = state.streams;
        self.outputs = state.outputs;
        self.loaded = state.loaded;
        self.mode = state.mode;
        self.list = state.list;
        self.material = state.material;
        self.page_touched = state.page_touched;
        self.output_active = state.output_active;
        self.page_grew = state.page_grew;
        self.natural = state.natural;
        self.main_tail = state.main_tail;
        self.wild = state.wild;
    }

    /// Drop the conditionals above `depth`, with the control dependencies
    /// they pushed.
    pub(crate) fn drop_conditionals(&mut self, depth: usize) {
        while self.conds.len() > depth {
            if Rc::make_mut(&mut self.conds).pop().is_some_and(|frame| frame.dep) {
                Rc::make_mut(&mut self.cds).pop();
            }
        }
    }

    /// An undecided choice of `arms` ways (`usize::MAX` when not known):
    /// each is a path of its own ([`Machine::branch`]). `enter` puts the path
    /// at the start of arm `i`. Returns `false`, having done nothing, when no
    /// arm is entered or no more choices may be made here.
    pub fn split(&mut self, by: Sym, span: Span, arms: usize, enter: impl FnOnce(&mut Self, usize) -> bool) -> bool {
        let Some(arm) = self.branch(by, arms, span) else { return false };
        // A replay that meets fewer arms than its script names: that arm
        // is gone, and its siblings cover the ones left.
        if arm >= arms || !enter(self, arm) {
            self.no_arm();
        }
        true
    }

    /// A name that holds one of several meanings (a join left it so), about
    /// to be executed or compared: a path for each meaning, on which the
    /// name has that one.  Each path goes on with the command, so the paths
    /// part where the meanings act differently.
    pub fn split_meaning(&mut self, sym: Sym, span: Span) -> bool {
        self.split_meaning_by(sym, span, |_, _| None)
    }

    /// [`Machine::split_meaning`], a path per class of meanings `class`
    /// puts alike (an `\\ifx` answer): the name holds that class on it.
    /// Unclassed meanings (`None`) each part on their own.
    pub(crate) fn split_meaning_by(
        &mut self,
        sym: Sym,
        span: Span,
        class: impl Fn(&Self, &Meaning) -> Option<bool>,
    ) -> bool {
        // A name with a meaning of its own may only be undefined besides:
        // it is read with that meaning ([`crate::env`]'s one-sided join).
        let Some(may) =
            self.env.slot(sym).filter(|b| matches!(b.meaning, Meaning::Unknown)).and_then(|b| b.may.clone())
        else {
            return false;
        };
        // A one-of token keeps its meanings: refined, `text*` would call
        // itself forever.
        let one_of = self.name(sym).strip_prefix(UNKNOWN_NAME).is_some_and(|n| n.starts_with(explore::ONE_OF));
        let mut classes: Vec<(Option<bool>, Vec<Meaning>)> = Vec::new();
        for m in may.iter() {
            let c = class(self, m);
            match classes.iter_mut().find(|(k, _)| c.is_some() && *k == c) {
                Some((_, ms)) => ms.push(m.clone()),
                None => classes.push((c, vec![m.clone()])),
            }
        }
        !one_of
            && self.may_split_meaning()
            && self.split(sym, span, classes.len(), |m, arm| {
                m.refine_to(sym, &classes[arm].1);
                true
            })
    }

    /// A name built from partly unknown text (`\\csname h⟨digits⟩\\endcsname`),
    /// run: a path per meaning of the names of its family it may spell,
    /// that name in its place, and one where it was never defined and is
    /// `\\relax` (tex.web § 372), which does nothing here. `false` when it
    /// is no such name or the family has more than `limits.meaning_set`
    /// meanings; it then runs as unknown text.
    /// The name keeps that meaning on its path, so each later read of the
    /// same token agrees with it; the caller goes on with it in the same
    /// step, as given back its paths would meet at once and join.
    pub(crate) fn split_family(&mut self, sym: Sym, span: Span) -> bool {
        if !matches!(self.env.meaning(sym), Meaning::Unknown) || self.env.slot(sym).is_some_and(|b| b.may.is_some()) {
            return false;
        }
        let Some((prefix, suffix)) = self.name_pattern(sym) else { return false };
        let digits = self.name_gap_is_digits(sym);
        let limit = usize::from(self.limits.meaning_set);
        let mut members: Vec<(Sym, Meaning)> = Vec::new();
        for s in self.env.bound() {
            let name = self.out.interner.name(s);
            if self.is_unknown_name(s)
                || name.len() < prefix.len() + suffix.len()
                || !name.starts_with(prefix.as_str())
                || !name.ends_with(suffix.as_str())
                || (digits && !name[prefix.len()..name.len() - suffix.len()].chars().all(|c| c.is_ascii_digit()))
            {
                continue;
            }
            let meaning = self.env.meaning(s);
            if matches!(meaning, Meaning::Undefined) || members.iter().any(|(_, m)| *m == meaning) {
                continue;
            }
            if members.len() == limit {
                return false;
            }
            members.push((s, meaning));
        }
        self.split(sym, span, members.len() + 1, |m, arm| {
            let meaning = members.get(arm).map_or(Meaning::Primitive(Primitive::Relax), |(_, meaning)| meaning.clone());
            m.refine(sym, meaning);
            true
        })
    }

    /// A token for one of several texts ([`Machine::one_of`]), met where a
    /// read takes one token as it stands: a path per text, with that text
    /// in the token's place, so the read sees what TeX would. Bounded by
    /// `limits.choices` alone: it parts on text, not on a meaning. `false`
    /// when `t` is no such token or this command may part no further; the
    /// caller then reads it as unknown text.
    pub(crate) fn split_one_of(&mut self, t: Token) -> bool {
        self.split_one_of_at(t, t.span)
    }

    /// [`Machine::split_one_of`], the choice made at `site`: the code that
    /// reads the token, not where its texts were joined, so reads of one
    /// value at several places are no passes of one loop.
    pub(crate) fn split_one_of_at(&mut self, t: Token, site: Span) -> bool {
        let Some(sym) = t.cs() else { return false };
        let Some(texts) = self.one_of_members(t) else { return false };
        self.split(sym, site, texts.len(), |m, arm| {
            m.unread_all(&texts[arm]);
            true
        })
    }

    /// [`Machine::split_one_of`] for a token a parameterless macro's whole
    /// text is, read just now from its expansion: each path also takes the
    /// macro to hold its text alone, as a name a join left several
    /// meanings does ([`Machine::split_meaning`]), so what a use of the
    /// value refines sticks to the macro.
    pub(crate) fn split_one_of_refining(&mut self, t: Token) -> bool {
        let owner = match self.input.last() {
            Some(Frame::Tokens { toks, pos, expanding: Some(sym), .. })
                if *pos == toks.len() && matches!(&toks[..], [only] if only.tok == t.tok) =>
            {
                Some(*sym)
            }
            _ => None,
        };
        let owned = owner.and_then(|sym| match self.env.meaning(sym) {
            Meaning::Macro(m)
                if m.parameter_text.items.is_empty()
                    && matches!(&m.replacement_text[..], [only] if only.tok == t.tok) =>
            {
                Some((sym, m))
            }
            _ => None,
        });
        let (Some((owner, def)), Some(sym)) = (owned, t.cs()) else { return self.split_one_of(t) };
        let Some(texts) = self.one_of_members(t) else { return false };
        self.split(sym, t.span, texts.len(), |m, arm| {
            let text = MacroDef { replacement_text: texts[arm].clone(), ..(*def).clone() };
            m.refine(owner, Meaning::Macro(Rc::new(text)));
            m.unread_all(&texts[arm]);
            true
        })
    }

    /// Whether a token for one of several texts may not stand for one
    /// undelimited argument: a text that is not one plain token.
    fn one_of_matters(&self, t: Token) -> bool {
        self.one_of_members(t).is_some_and(|texts| {
            texts.iter().any(|x| match &**x {
                [one] => {
                    matches!(one.tok, Tok::Chr(_, Catcode::Begin | Catcode::End | Catcode::Space))
                        || one.cs().is_some_and(|sym| self.is_unknown_marker(sym))
                }
                _ => true,
            })
        })
    }

    /// Whether a name that holds one of several meanings may split the run
    /// here. Past `meaning_splits` choices in one command it is read as a
    /// name of unknown meaning instead.
    pub(crate) fn may_split_meaning(&self) -> bool {
        self.choices.len() < usize::from(self.limits.meaning_splits)
    }

    /// A file the run reads without loading it as TeX source (`\openin`, an
    /// image): it is one of the analysis's files all the same.
    pub(crate) fn note_read_file(&mut self, path: &Path) {
        let path = path.display().to_string();
        self.note_opened(&path);
        self.register_file(path, LoadKind::Input);
    }

    pub fn register_file(&mut self, path: String, kind: LoadKind) -> FileId {
        if let Some(i) = self.out.files.iter().position(|f| f.path == path) {
            return i as FileId;
        }
        if self.cfg.verbose >= 1 {
            crate::progress::note!("satex: reading {path}");
        }
        // Two files of one run with the same base name are indistinguishable in
        // every diagnostic that names one, so both are named by their
        // directory as well.
        let mut short = base_name(&path).to_string();
        if self.out.files.iter().any(|f| base_name(&f.path) == short) {
            for other in self.out.files.iter_mut().filter(|f| base_name(&f.path) == short) {
                other.short = qualified_name(&other.path);
            }
            short = qualified_name(&path);
        }
        self.out.files.push(FileInfo { path, kind, short });
        self.out.entry.push(None);
        (self.out.files.len() - 1) as FileId
    }

    /// `\input` has named a file: read it next. latex.ltx reads packages and
    /// classes this way too, in `\@onefilewithoptions`, with `\@currname` and
    /// `\@currext` naming the file and `\opt@⟨name⟩.⟨ext⟩` holding its options
    /// (ltclass.dtx).
    pub fn load(&mut self, name: &str, kind: LoadKind, span: Span, options: Vec<String>, required: Option<String>) {
        let (name, directory, kind, options) = self.kernel_file(name.trim(), kind, options);
        let name = name.as_str();
        // What the search looks for: the name with the directory the kernel
        // keeps in `\@currpath`, which is the only place a relative path
        // like `../template/` survives.
        let search = format!("{directory}{name}");
        // The load stands at the call from a file whose own text names the
        // file. A file the code of some other call opens stands at the
        // `\input` that opened it.
        let span = match self.file_call {
            Some((_, call)) if self.call_names_file(call, name) => call,
            _ => self.line_naming_file(name).unwrap_or(span),
        };
        let follow = match kind {
            LoadKind::Package | LoadKind::Inherited => self.cfg.load.packages,
            LoadKind::Class => self.cfg.load.classes,
            _ => self.cfg.load.inputs || self.reading_format(),
        };
        let by = self.package();
        // `\input docstrip` reads the program whose batch-file commands
        // satex models. Its own code reads the `.dtx` and writes the files,
        // which an interpreter that does neither cannot follow.
        let modeled = self.docstrip && matches!(name, "docstrip" | "docstrip.tex");
        let mut status = if !follow || modeled {
            LoadStatus::NotFollowed
        } else if self.cfg.load.skip.iter().any(|p| p == name) {
            LoadStatus::Skipped
        } else if self.file_depth >= self.cfg.limits.file_depth {
            self.limit_reached("file_depth", span);
            LoadStatus::TooDeep
        } else {
            LoadStatus::NotFound
        };
        let resolved = if status == LoadStatus::NotFound {
            let base = self.base.clone();
            let mut found = self.resolver.resolve(&search, kind, &base);
            // The directory is only where the kernel itself looked, so a
            // name that resolves without it names the same file.
            if found.is_none() && search != name {
                found = self.resolver.resolve(name, kind, &base);
            }
            found
        } else {
            None
        };
        // A file this run wrote is read as written (tex.web § 537).
        let written = if status == LoadStatus::NotFound { self.written_text(&search).flatten() } else { None };
        let resolved = match (&written, resolved) {
            (Some(_), None) => Some(self.base.join(&search)),
            (_, found) => found,
        };
        // Check if a case mismatch was detected when the resolve failed.
        if resolved.is_none()
            && status == LoadStatus::NotFound
            && let Some((_, found_name)) = self.resolver.case_mismatch(&search, kind)
        {
            let on_disk = self.intern(&found_name);
            status = LoadStatus::CaseMismatch { on_disk };
        }
        // A file this run has opened for writing is there to read, with what
        // satex, which writes nothing, does not have.
        if resolved.is_none()
            && status == LoadStatus::NotFound
            && self.engine_int(&format!("written.{name}")) == Some(1)
        {
            status = LoadStatus::NotFollowed;
        }
        let mut invalid = 0;
        let source =
            resolved.as_ref().filter(|_| written.is_none()).and_then(|path| match crate::overlay::read_bytes(path) {
                Ok(bytes) => {
                    let (text, bad) = crate::source::decode_counted(&bytes);
                    invalid = bad;
                    // A `.dtx` read as an input is read the way docstrip reads it.
                    Some(match crate::literate::Literate::of(path) {
                        Some(crate::literate::Literate::Dtx) => {
                            crate::literate::code_view(&text, &self.resolver.guards(name, kind))
                        }
                        _ => text,
                    })
                }
                Err(_) => {
                    status = LoadStatus::Unreadable;
                    None
                }
            });
        let source = source.or(written);
        // XeTeX reads a replacement character there, which satex does too;
        // LuaTeX stops with "String contains an invalid utf-8 sequence",
        // which satex cannot follow.
        if invalid > 0 && self.out.plugins.engine == crate::config::Engine::LuaTeX {
            self.diagnose(
                Severity::Unsupported,
                "invalid-utf8",
                span,
                format!(
                    "{name} holds {invalid} bytes that are no UTF-8, which luatex stops on; read as XeTeX reads them"
                ),
            );
        }
        if source.is_some() {
            status = LoadStatus::Read;
        }
        self.note_unresolved(name, kind, status, None, span);
        if kind.is_package() {
            Rc::make_mut(&mut self.loaded).push(name.to_string());
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
            if self.packages.is_empty() {
                self.load_start.get_or_insert(self.out.steps);
            }
            Rc::make_mut(&mut self.packages).push((sym, options));
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
        // An `\endinput` still waiting on the line that opened this file
        // belongs to this file now (tex.web § 362), so the file ends after
        // its first line.  The package cache replays a file whole and would
        // record a truncated one, so a pending flag reads the file itself.
        let mut mouth = match self.pending_eof() {
            true => Mouth::file(&source, id),
            false => self.open_package(&source, id, name, kind, span),
        };
        if self.take_pending_eof() {
            mouth.inherit_eof(false);
        }
        self.input.push(Frame::File { mouth, package, depth, conds, eof_seen: false });
    }

    /// A `\documentclass`, `\usepackage` or `\RequirePackage` whose file the
    /// search did not find.  Everything the file would have defined is
    /// missing, so the run records the gap instead of carrying on as if the
    /// file defined nothing.
    fn note_unresolved(&mut self, name: &str, kind: LoadKind, status: LoadStatus, _on_disk: Option<&str>, span: Span) {
        if !kind.is_package() {
            return;
        }
        let reason = match status {
            LoadStatus::NotFound => "was not found".to_string(),
            LoadStatus::CaseMismatch { on_disk } => {
                let disk_name = self.out.interner.name(on_disk);
                format!("is spelled `{disk_name}` on disk")
            }
            _ => return,
        };
        let Some(file) = crate::plugin::packages::file_of(name, kind) else { return };
        self.note_missing_file(&file, &reason, span);
    }

    /// `\@missingfileerror` (latex.ltx): the kernel raises it when the
    /// `\IfFileExists` of `\@onefilewithoptions` found nothing, and only
    /// then.  An `\IfFileExists` or `\InputIfFileExists` of the document's
    /// own raises nothing, so an optional file that is absent on purpose
    /// leaves no gap.
    pub(crate) fn note_missing_file_error(&mut self, text: &str, at: Span) {
        const MARKER: &str = "LaTeX Error: File `";
        let Some((_, rest)) = text.split_once(MARKER) else { return };
        let Some((file, _)) = rest.split_once("' not found") else { return };
        if !file.ends_with(".cls") && !file.ends_with(".sty") {
            return;
        }
        let file = file.to_string();
        self.note_missing_file(&file, "was not found", at);
    }

    /// The gap a class or package file that never loaded leaves behind.
    fn note_missing_file(&mut self, file: &str, reason: &str, span: Span) {
        let (code, what) =
            if file.ends_with(".cls") { ("unresolved-class", "class") } else { ("unresolved-package", "package") };
        let sym = self.intern(file);
        let message = format!("{what} file `{file}` {reason}, so nothing it defines is in this analysis");
        self.note_gap_with(code, sym, span, Severity::Unsupported, message);
    }

    /// What latex.ltx is reading through `\input`: the package or class
    /// `\@currname` and `\@currext` name, with the options
    /// `\opt@⟨name⟩.⟨ext⟩` holds, when the file is theirs, else the file
    /// itself. `\@pkgextension`/`\@clsextension` say which extension is which.
    /// The directory comes back beside the name because the kernel keeps it
    /// apart in `\@currpath`: `\documentclass{../template/x}` is the class
    /// `x` read from `../template/`, so the name identifies the class and the
    /// directory only says where to look for it.
    fn kernel_file(&self, name: &str, kind: LoadKind, options: Vec<String>) -> (String, String, LoadKind, Vec<String>) {
        let text = |cs: &str| -> Option<String> {
            let sym = self.out.interner.lookup(cs)?;
            let binding = self.env.slot(sym)?;
            let m = binding.meaning.as_macro()?;
            Some(self.text_of(&m.replacement_text))
        };
        let file = name.trim_matches('"');
        let plain = || (file.to_string(), String::new(), kind, options.clone());
        let (Some(current), Some(ext)) = (text("@currname"), text("@currext")) else {
            return plain();
        };
        let path = std::path::Path::new(file);
        let leaf = path.file_name().map_or(file.to_string(), |f| f.to_string_lossy().into_owned());
        if current.is_empty() || leaf != format!("{current}.{ext}") {
            return plain();
        }
        let kind = if text("@clsextension").as_deref() == Some(ext.as_str()) {
            LoadKind::Class
        } else if text("@pkgextension").as_deref() == Some(ext.as_str()) {
            LoadKind::Package
        } else {
            return plain();
        };
        let directory = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map_or(String::new(), |parent| format!("{}/", parent.display()));
        let options =
            text(&format!("opt@{current}.{ext}")).map(|list| crate::tex::comma_split(&list)).unwrap_or_default();
        (current, directory, kind, options)
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
        let is = |args: &[Token], name: &str| matches!(args, [t] if t.cs().is_some_and(|s| self.out.interner.name(s) == name));
        match self.name(sym) {
            "@ifl@aded" if arguments.len() == 2 && is(&arguments[0], "@currext") && is(&arguments[1], "@currname") => {
                let text = |m: &Self, cs: &str| {
                    let sym = m.out.interner.lookup(cs)?;
                    let macro_ = m.env.slot(sym)?.meaning.as_macro()?;
                    Some(m.text_of(&macro_.replacement_text))
                };
                let (Some(current), Some(ext)) = (text(self, "@currname"), text(self, "@currext")) else { return };
                let loaded = self.out.interner.lookup(&format!("ver@{current}.{ext}")).is_some_and(|v| {
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
    /// `force_eof` for the file being read, so a running token list or
    /// argument scan finishes first, and the file ends when the reader comes
    /// back to it. A pseudo file is read by the same code in eTeX, so
    /// `\scantokens{\endinput}` ends the pseudo file, not the real one
    /// (etex.ch § 362).
    pub fn end_input(&mut self, span: Span) {
        match self.reading_mouth() {
            Some(mouth) => mouth.end_input(),
            // tex.web § 362 asks `force_eof` only where a file is read, so
            // a terminal `\endinput` waits for the next file to open.  satex
            // reads its input as a file, so nothing observable is left over,
            // but the run says the flag went nowhere.
            None => self.diagnose(
                Severity::Unsupported,
                "endinput-without-file",
                span,
                "\\endinput with no file being read; nothing is ended".into(),
            ),
        }
    }

    /// The frame a new line would come from: the innermost file or pseudo
    /// file, whatever token lists are running above it.  A boundary is where
    /// reading stops, so nothing below it can end.
    fn reading_frame(&self) -> Option<usize> {
        let mut at = self.input.len();
        while at > 0 {
            at -= 1;
            match self.input[at] {
                Frame::Boundary => return None,
                Frame::File { .. } | Frame::Pseudo { .. } => return Some(at),
                _ => {}
            }
        }
        None
    }

    fn reading_mouth(&mut self) -> Option<&mut Mouth> {
        let at = self.reading_frame()?;
        match &mut self.input[at] {
            Frame::File { mouth, .. } => Some(mouth),
            Frame::Pseudo { mouth, .. } => Some(&mut **mouth),
            _ => None,
        }
    }

    /// Whether an `\endinput` has set the flag and no file has taken it yet.
    pub(crate) fn pending_eof(&self) -> bool {
        match self.reading_frame().map(|at| &self.input[at]) {
            Some(Frame::File { mouth, .. }) => mouth.pending_eof(),
            Some(Frame::Pseudo { mouth, .. }) => mouth.pending_eof(),
            _ => false,
        }
    }

    /// Take the flag `\endinput` set, if it has not fired yet.  tex.web's
    /// `force_eof` is a single global, so the next file to ask for a line
    /// takes it, even one opened after the `\endinput` on the same line.
    fn take_pending_eof(&mut self) -> bool {
        match self.reading_mouth().filter(|mouth| mouth.pending_eof()) {
            Some(mouth) => {
                mouth.release_eof();
                true
            }
            None => false,
        }
    }

    /// The engine's `\end`: nothing after it is read, whatever file or
    /// token list it stood in (tex.web § 1335).
    pub fn end_job(&mut self) {
        self.job_ended = true;
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

/// The token that opens a group of this kind, for reporting where an
/// unclosed one began.
fn opened_by(kind: GroupKind) -> &'static str {
    match kind {
        GroupKind::Simple => "{",
        GroupKind::SemiSimple => "\\begingroup",
        GroupKind::Math { .. } => "$",
    }
}

/// How TeX names a group kind in its complaints (tex.web § 1069).
/// A path's base name.
fn base_name(path: &str) -> &str {
    Path::new(path).file_name().and_then(|n| n.to_str()).unwrap_or(path)
}

/// A path's base name under the directory holding it, for a base name that
/// another file of the run also has.
fn qualified_name(path: &str) -> String {
    let base = base_name(path);
    match Path::new(path).parent().and_then(Path::file_name).and_then(|n| n.to_str()) {
        Some(dir) if !dir.is_empty() => format!("{dir}/{base}"),
        _ => base.to_string(),
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
    let parts = substitute_parts(m, args, limit)?;
    match &parts[..] {
        [(text, _)] => Some(text.clone()),
        _ => Some(parts.iter().flat_map(|(text, _)| text.iter().copied()).collect()),
    }
}

/// Arguments at least this long are shared by the replacement text, not copied.
const SHARED_ARGUMENT: usize = 4096;

/// [`substitute`] as lists read one after another, each with the tokens
/// it newly holds. An argument of [`SHARED_ARGUMENT`] tokens or more is one
/// shared list wherever it stands: each use costs O(1), and it is held once.
/// `limit` bounds what is held, not the copies a shared argument stands for.
pub fn substitute_parts(m: &MacroDef, args: &[Vec<Token>], limit: usize) -> Option<Vec<(Rc<[Token]>, usize)>> {
    if args.is_empty() || !m.replacement_text.iter().any(|t| matches!(t.tok, Tok::Param(_))) {
        return Some(vec![(m.replacement_text.clone(), m.replacement_text.len())]);
    }
    let mut shared: Vec<Option<Rc<[Token]>>> = vec![None; args.len()];
    let mut parts = Vec::new();
    let copied: usize = args.iter().map(Vec::len).filter(|&n| n < SHARED_ARGUMENT).sum();
    let mut run: Vec<Token> = Vec::with_capacity(m.replacement_text.len() + copied);
    let mut held = 0;
    for t in m.replacement_text.iter() {
        match t.tok {
            Tok::Param(n) => match args.get(n as usize - 1) {
                Some(arg) if arg.len() >= SHARED_ARGUMENT => {
                    if !run.is_empty() {
                        parts.push((Rc::from(std::mem::take(&mut run)), 0));
                    }
                    let slot = &mut shared[n as usize - 1];
                    let first = if slot.is_none() { arg.len() } else { 0 };
                    held += first;
                    parts.push((slot.get_or_insert_with(|| Rc::from(arg.as_slice())).clone(), first));
                }
                Some(arg) => {
                    held += arg.len();
                    run.extend_from_slice(arg);
                }
                None => {
                    held += 1;
                    run.push(*t);
                }
            },
            _ => {
                held += 1;
                run.push(*t);
            }
        }
        if held > limit {
            return None;
        }
    }
    if !run.is_empty() || parts.is_empty() {
        parts.push((Rc::from(run), 0));
    }
    // A copied run holds its own length.
    for (text, n) in &mut parts {
        if !shared.iter().flatten().any(|s| Rc::ptr_eq(s, text)) {
            *n = text.len();
        }
    }
    Some(parts)
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
        let matched =
            tokens.get(at).is_some_and(|t| matches!(t.tok, Tok::Chr(c, _) if c.eq_ignore_ascii_case(&wanted)));
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
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Wild(Vec<Pattern>);

/// Names of a known prefix and suffix around unknown text, digits alone
/// where `digits`. They may be `\relax`, and where `real` one of
/// `meanings` (`None`: any meaning; a cache keeps none).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Pattern {
    prefix: String,
    suffix: String,
    digits: bool,
    real: bool,
    #[serde(skip)]
    meanings: Option<Rc<[Meaning]>>,
}

impl Pattern {
    /// What a name it fits may mean besides `\relax`; `None`: anything.
    fn may(&self) -> Option<&[Meaning]> {
        if self.real { self.meanings.as_deref() } else { Some(&[]) }
    }

    fn fits(&self, name: &str) -> bool {
        let (p, s) = (self.prefix.as_str(), self.suffix.as_str());
        name.len() >= p.len() + s.len()
            && name.starts_with(p)
            && name.ends_with(s)
            && (!self.digits || name[p.len()..name.len() - s.len()].chars().all(|c| c.is_ascii_digit()))
    }

    /// Every name `other` fits, this fits, with its meanings among these.
    fn covers(&self, other: &Pattern) -> bool {
        other.prefix.starts_with(self.prefix.as_str())
            && other.suffix.ends_with(self.suffix.as_str())
            && (!self.digits || other.digits)
            && match (self.may(), other.may()) {
                (None, _) => true,
                (Some(_), None) => false,
                (Some(a), Some(b)) => b.iter().all(|m| a.contains(m)),
            }
    }
}

/// The meanings of both; any past [`Wild::MEANINGS`].
fn join_may(a: Option<&[Meaning]>, b: Option<&[Meaning]>) -> Option<Rc<[Meaning]>> {
    let mut all = a?.to_vec();
    for m in b? {
        if !all.contains(m) {
            all.push(m.clone());
        }
    }
    (all.len() <= Wild::MEANINGS).then(|| all.into())
}

impl Wild {
    /// Beyond this many patterns they are merged into what they share.
    const MAX: usize = 1024;
    /// Beyond this many meanings a pattern may have any.
    const MEANINGS: usize = 8;

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Names beginning with `prefix` and ending with `suffix` may be defined,
    /// with digits alone between where `digits`. `meaning` is what one may
    /// have besides the `\relax` a `\csname` gives (`Meaning::Unknown`:
    /// any), `None` where only that.
    pub(crate) fn add(&mut self, prefix: String, suffix: String, digits: bool, meaning: Option<Meaning>) {
        // A name of wholly unknown text is assumed fresh
        // (`unknown-name-defines-fresh`): it stands for no known name.
        if prefix.is_empty() && suffix.is_empty() && !digits {
            return;
        }
        let meanings: Option<Rc<[Meaning]>> = match meaning {
            None => Some(Rc::from([])),
            Some(Meaning::Unknown) => None,
            Some(m) => Some(Rc::from([m])),
        };
        self.put(Pattern { prefix, suffix, digits, real: meanings.as_ref().is_none_or(|m| !m.is_empty()), meanings });
    }

    fn put(&mut self, new: Pattern) {
        // The same names: their meanings join.
        if let Some(old) =
            self.0.iter_mut().find(|p| (&p.prefix, &p.suffix, p.digits) == (&new.prefix, &new.suffix, new.digits))
        {
            old.meanings = join_may(old.may(), new.may());
            old.real = old.meanings.as_ref().is_none_or(|m| !m.is_empty());
            return;
        }
        if self.0.iter().any(|p| p.covers(&new)) {
            return;
        }
        self.0.retain(|p| !new.covers(p));
        self.0.push(new);
        if self.0.len() > Self::MAX {
            // What every pattern shares still covers every one of them.
            let mut all = self.0[0].clone();
            for q in &self.0[1..] {
                let p = &mut all.prefix;
                p.truncate(
                    p.chars().zip(q.prefix.chars()).take_while(|(a, b)| a == b).map(|(a, _)| a.len_utf8()).sum(),
                );
                let s = &all.suffix;
                let keep: usize = s
                    .chars()
                    .rev()
                    .zip(q.suffix.chars().rev())
                    .take_while(|(a, b)| a == b)
                    .map(|(a, _)| a.len_utf8())
                    .sum();
                all.suffix = s[s.len() - keep..].to_string();
                all.digits &= q.digits;
                all.meanings = join_may(all.may(), q.may());
                all.real |= q.real;
            }
            self.0 = vec![all];
        }
    }

    pub(crate) fn union(&mut self, other: &Wild) {
        for p in &other.0 {
            self.put(p.clone());
        }
    }

    pub(crate) fn matches(&self, name: &str) -> bool {
        self.0.iter().any(|p| p.fits(name))
    }

    /// Whether `name` may have a meaning besides `\relax`.
    pub(crate) fn real(&self, name: &str) -> bool {
        self.0.iter().any(|p| p.real && p.fits(name))
    }

    /// The meanings `name` may have besides `\relax`; `None`: any.
    pub(crate) fn meanings(&self, name: &str) -> Option<Vec<Meaning>> {
        let mut all: Vec<Meaning> = Vec::new();
        for p in self.0.iter().filter(|p| p.fits(name)) {
            all = join_may(Some(&all), p.may())?.to_vec();
        }
        Some(all)
    }

    /// Whether some name of `prefix`, any text and `suffix` may have a
    /// meaning besides `\relax`: the unknown parts may be any length, so
    /// two patterns share a name when their prefixes and their suffixes
    /// agree as far as both go.
    pub(crate) fn overlaps_real(&self, prefix: &str, suffix: &str) -> bool {
        self.0.iter().any(|p| {
            p.real
                && (p.prefix.starts_with(prefix) || prefix.starts_with(p.prefix.as_str()))
                && (p.suffix.ends_with(suffix) || suffix.ends_with(p.suffix.as_str()))
        })
    }
}

/// The name standing for a `\csname` built from partly unknown text: its
/// known prefix and suffix around a separator no real name has.
/// What a delimited scan makes of one of several texts ([`Machine::scan_one_of`]).
enum OneOfScan {
    /// The argument ends inside the text: the whole argument.
    Argument(Vec<Token>),
    /// Not known: the text is read as unknown text.
    Unknown,
    /// No text holds the delimiter or straddles it: an ordinary token.
    Pass,
    /// Some texts hold the delimiter and some do not: a path for each
    /// group put back the token for its texts, to be read again.
    Again,
}

pub(crate) const UNKNOWN_NAME: &str = "\u{2}unknown name:";
pub(crate) const UNKNOWN_NAME_GAP: char = '\u{3}';
/// [`UNKNOWN_NAME_GAP`] where the unknown part is digits.
pub(crate) const UNKNOWN_DIGITS_GAP: char = '\u{7}';
const GAPS: [char; 2] = [UNKNOWN_NAME_GAP, UNKNOWN_DIGITS_GAP];
/// After [`UNKNOWN_NAME`]: the names of typed unknown text, the class as
/// its letters and `+` (at least one token) or `*` (perhaps none).
pub(crate) const TYPED_TEXT: &str = "text of:";
/// The name of unknown digits that may be none, before its serial.
pub(crate) const MORE_DIGITS: &str = "\u{2}unknown more digits:";
/// What parts a rest's serial from its name: a rest is its own text.
pub(crate) const REST_SERIAL: char = '\u{4}';
/// Marks integer digits as `\the` prints them, which have no leading
/// zero (tex.web §§ 103, 104): at least one digit, `0` only where it is
/// alone; after a printed sign or digit, none after a `0`.
pub(crate) const LEAD_MARK: char = '\u{6}';
/// How many serials rests of one class cycle through.
pub(crate) const REST_SERIALS: usize = 32;
/// Where the bounded names begin in the typed names
/// ([`crate::env::bounded_index`]): after the classes and the rests.
pub(crate) const BOUNDED_BASE: usize = 256 + 129 * REST_SERIALS;

/// What unknown text is known to hold ([`Machine::unknown_text`]).
#[derive(Clone, Copy)]
pub(crate) enum Unk<'a> {
    /// Nothing.
    Any,
    /// Tokens of a class or these control sequences, at least one when
    /// set, at most a bound.
    Class(TextClass, bool, Option<u32>, &'a [Sym]),
    /// Any of these texts; unknown text in them stands for its own text.
    Of(&'a [&'a [Token]]),
}

impl Machine<'_> {
    /// The known prefix and suffix of a name built from partly unknown text.
    /// A token standing for unknown text (typed, or one of several texts),
    /// not a name built around a gap: defining it defines some fresh name,
    /// never the token itself, whose expansion would then name itself.
    pub(crate) fn unknown_text_name(&self, sym: Sym) -> bool {
        self.out.interner.name(sym).strip_prefix(UNKNOWN_NAME).is_some_and(|rest| !rest.contains(GAPS))
    }

    pub(crate) fn name_pattern(&self, sym: Sym) -> Option<(String, String)> {
        let rest = self.out.interner.name(sym).strip_prefix(UNKNOWN_NAME)?;
        let (prefix, suffix) = rest.split_once(GAPS)?;
        Some((prefix.to_string(), suffix.to_string()))
    }

    /// Whether a name [`Machine::name_pattern`] reads has digits for its
    /// unknown part.
    pub(crate) fn name_gap_is_digits(&self, sym: Sym) -> bool {
        self.out.interner.name(sym).strip_prefix(UNKNOWN_NAME).is_some_and(|rest| rest.contains(UNKNOWN_DIGITS_GAP))
    }

    /// What was assigned after assignment `mark` (and since the recent list
    /// was cleared) may hold anything of its kind; 0 widens all of it.
    pub(crate) fn widen_since(&mut self, mark: u64) {
        let syms = self.typed_syms();
        let written = self.env.written_after(mark);
        self.env.widen(&written, None, &crate::env::typed!(self, syms));
        // After: widening writes, and those are no news.
        self.env.clear_recent();
    }

    /// The names of typed unknown text, all classes and both lower bounds,
    /// each bound to the one character it may be when its class has one
    /// category (an `\ifx` then sees a character, as it does for unknown
    /// digits), else to an unknown meaning.
    pub(crate) fn typed_syms(&mut self) -> Rc<[Sym]> {
        // A path resumed from before the bindings were made lost them:
        // make them again.
        if let Some(syms) = &self.typed_syms
            && self.env.slot(syms[0]).is_some()
            && self.env.slot(syms[syms.len() - 1]).is_some()
        {
            return syms.clone();
        }
        let mut syms = Vec::with_capacity(256);
        for i in 0..256usize {
            let (class, one) = (TextClass((i / 2) as u8), i % 2 == 1);
            let name = format!("{UNKNOWN_NAME}{TYPED_TEXT}{}{}", class.spell(), if one { '+' } else { '*' });
            let sym = self.intern(&name);
            let meaning = match class.one_char() {
                Some(Tok::Chr(c, cat)) => Meaning::Char(c, cat),
                _ => Meaning::Unknown,
            };
            if self.env.meaning(sym) != meaning {
                self.env.constant(sym, Binding::builtin(meaning));
            }
            syms.push(sym);
        }
        // The rests' names, bound here once: a binding made per rest
        // would be a write every pass of a loop.
        for i in 0..=128usize {
            let base = if i < 128 { syms[i * 2] } else { self.unknown_more };
            let meaning = self.env.meaning(base);
            let spelled = self.name(base).to_string();
            for k in 0..REST_SERIALS {
                let sym = self.intern(&format!("{spelled}{REST_SERIAL}{k}"));
                if self.env.meaning(sym) != meaning {
                    self.env.constant(sym, Binding::builtin(meaning.clone()));
                }
                syms.push(sym);
            }
        }
        // Bounded text, per class, lower bound, bound and serial.
        for class in crate::env::BOUNDED {
            for one in [false, true] {
                let base = self.typed_base(class, one, &syms);
                let meaning = self.env.meaning(base);
                let spelled = self.name(base).to_string();
                for len in 1..=crate::env::MAX_LEN {
                    for k in 0..=REST_SERIALS {
                        let sym = self.intern(&format!("{spelled}{REST_SERIAL}{k}{}{len}", crate::env::LEN_MARK));
                        if self.env.meaning(sym) != meaning {
                            self.env.constant(sym, Binding::builtin(meaning.clone()));
                        }
                        debug_assert_eq!(Some(syms.len()), crate::env::bounded_index(class, one, len, k));
                        syms.push(sym);
                    }
                }
            }
        }
        let syms: Rc<[Sym]> = syms.into();
        self.typed_syms = Some(syms.clone());
        syms
    }

    /// The one constructor of unknown text: typed text of the class, lower
    /// bound, length and named control sequences `of` allows; a class with
    /// control sequences is typed too. The untyped marker only stands for
    /// text naming more control sequences than typed text keeps.
    #[track_caller]
    pub(crate) fn unknown_text(&mut self, of: Unk<'_>, span: Span) -> Token {
        let (class, one, len, cs) = match of {
            Unk::Any => {
                self.scratch.approximations += 1;
                self.approximated = true;
                (TextClass::WIDE, false, None, Vec::new())
            }
            Unk::Class(class, one, len, cs) => (class, one, len, cs.to_vec()),
            Unk::Of(texts) => match self.cover(texts) {
                Some(shape) => shape,
                None => return self.untyped(span),
            },
        };
        // Text of no kind is empty: digits, perhaps none, cover it.
        let (class, one) = match class == TextClass::EMPTY && cs.is_empty() {
            true => (TextClass::DIGIT, false),
            false => (class, one),
        };
        if !cs.is_empty() {
            let mut name = format!("{UNKNOWN_NAME}{TYPED_TEXT}{}{}", class.spell(), if one { '+' } else { '*' });
            for cs in cs.iter().map(|sym| self.name(*sym).to_string()).collect::<std::collections::BTreeSet<_>>() {
                name.push(UNKNOWN_NAME_GAP);
                name.push_str(&cs);
            }
            let sym = self.intern(&name);
            if !matches!(self.env.meaning(sym), Meaning::Unknown) {
                self.env.constant(sym, Binding::builtin(Meaning::Unknown));
            }
            return Token::new(Tok::Cs(sym), span);
        }
        let syms = self.typed_syms();
        // A bounded or rest text is its own text: a test on one teaches
        // nothing about another from the same place
        // ([`Machine::narrow_pending`]).
        let pool: Vec<Sym> = match len.and_then(|len| crate::env::bounded_index(class, one, len, 1)) {
            Some(i) => syms[i..i + REST_SERIALS].to_vec(),
            None if one && class == TextClass::DIGIT => return self.unknown_digits_token(span),
            None if one => return Token::new(Tok::Cs(syms[class.0 as usize * 2 + 1]), span),
            None => {
                let i = 256 + REST_SERIALS * if class == TextClass::DIGIT { 128 } else { class.0 as usize };
                syms[i..i + REST_SERIALS].to_vec()
            }
        };
        // Serials cycle, skipping one still waiting to be read.
        let pending = self.pending_lists();
        let mut rest = pool[0];
        for _ in 0..REST_SERIALS {
            self.carried.rest_serial = (self.carried.rest_serial + 1) % REST_SERIALS as u32;
            rest = pool[self.carried.rest_serial as usize];
            if !pending.iter().any(|t| t.tok == Tok::Cs(rest)) {
                break;
            }
        }
        Token::new(Tok::Cs(rest), span)
    }

    /// Puts back the rest of unknown text `t` a scan took `delimiter` to
    /// end in: text of its class, perhaps none, shorter by the delimiter.
    /// Only the text before the first delimiter is free of it (tex.web
    /// § 392): the rest may hold it again (`macro:#2->#3` of a `\meaning`).
    fn unread_rest(&mut self, t: Token, delimiter: &[Tok], span: Span) {
        let Some((class, _, len, cs)) = self.cover(&[&[t], &[]]) else {
            let rest = self.unknown_text(Unk::Of(&[&[t], &[]]), span);
            return self.unread(rest);
        };
        let len = len.map(|n| n.saturating_sub(delimiter.len() as u32));
        if len != Some(0) && (class != TextClass::EMPTY || !cs.is_empty()) {
            let rest = self.unknown_text(Unk::Class(class, false, len, &cs), span);
            self.unread(rest);
        }
    }

    /// Unknown text `t` detokenized: its class printed
    /// ([`TextClass::printed`]). The first token only where `first`
    /// (`\string`): the rest stays as it was.
    pub(crate) fn printed(&mut self, t: Token, first: bool, span: Span) -> Token {
        let (class, one, len) = match self.typed_shape(t) {
            Some((class, one, cs)) if cs.is_empty() => (class, one, self.text_len(t)),
            Some((class, one, _)) => (class.union(TextClass::CS), one, None),
            None => (TextClass::ANY, false, None),
        };
        let class = match first && len != Some(1) {
            true => class.union(class.printed()),
            false => class.printed(),
        };
        let len = len.filter(|_| !class.meets(TextClass::CS));
        self.unknown_text(Unk::Class(class, one, len, &[]), span)
    }

    /// The untyped marker: text no typed text covers. Counted per caller.
    #[track_caller]
    fn untyped(&mut self, span: Span) -> Token {
        let at = std::panic::Location::caller();
        self.out.untyped += 1;
        if let Some(hot) = &mut self.out.hot {
            *hot.untyped.entry((at.file(), at.line())).or_insert(0) += 1;
        }
        self.scratch.approximations += 1;
        self.approximated = true;
        if !matches!(self.env.meaning(self.unknown), Meaning::Unknown) {
            self.env.set(self.unknown, crate::env::Binding::builtin(Meaning::Unknown), true);
        }
        Token::new(Tok::Cs(self.unknown), span)
    }

    /// The class, lower bound, length and named control sequences of text
    /// covering each of `texts`; any control sequence where they name too
    /// many. Untyped text counts as the widest class. Braces are balanced
    /// ones, as in every argument.
    fn cover(&self, texts: &[&[Token]]) -> Option<(TextClass, bool, Option<u32>, Vec<Sym>)> {
        /// Named control sequences a typed text may hold before it is any text.
        const NAMED: usize = 8;

        let (mut class, mut cs, mut len) = (TextClass::EMPTY, Vec::new(), Some(0u32));
        for text in texts {
            let mut n = Some(0u32);
            for t in *text {
                let (c, k) = match (t.tok, self.typed_shape(*t)) {
                    (_, Some((c, _, named))) => {
                        cs.extend(named);
                        (c, self.text_len(*t))
                    }
                    (Tok::Cs(sym), None) if !self.is_unknown_marker(sym) => {
                        cs.push(sym);
                        (TextClass::EMPTY, Some(1))
                    }
                    (Tok::Cs(_), None) => (TextClass::WIDE, None),
                    (tok, None) => (TextClass::of(tok), Some(1)),
                };
                class = class.union(c);
                n = n.zip(k).map(|(n, k)| n.saturating_add(k));
                cs.sort_unstable();
                cs.dedup();
                // More names than a type spells: any control sequence,
                // still balanced text of the classes read.
                if cs.len() > NAMED || class.meets(TextClass::CS) {
                    class = class.union(TextClass::CS);
                    cs.clear();
                }
            }
            len = len.zip(n).map(|(a, b)| a.max(b));
        }
        let one = texts.iter().all(|text| !text.is_empty());
        Some((class, one, len.filter(|&n| n <= crate::env::MAX_LEN), cs))
    }

    /// Unknown integer digits as `\the` prints them, at most `len`, at
    /// least one where `one` ([`LEAD_MARK`]).
    pub(crate) fn lead_digits(&mut self, one: bool, len: u32, span: Span) -> Token {
        let base = self.unknown_text(Unk::Class(TextClass::DIGIT, one, Some(len), &[]), span);
        let Some(sym) = base.cs() else { return base };
        let Some((head, len)) =
            self.name(sym).rsplit_once(crate::env::LEN_MARK).map(|(h, l)| (h.to_string(), l.to_string()))
        else {
            return base;
        };
        let lead = self.intern(&format!("{head}{LEAD_MARK}{}{len}", crate::env::LEN_MARK));
        if self.env.slot(lead).is_none() {
            let meaning = self.env.meaning(sym);
            self.env.constant(lead, Binding::builtin(meaning));
        }
        Token::new(Tok::Cs(lead), span)
    }

    /// Whether `t` is integer digits ([`LEAD_MARK`]).
    fn is_lead(&self, t: Token) -> bool {
        t.cs().is_some_and(|sym| self.name(sym).contains(LEAD_MARK))
    }

    /// The name typed text of `class` and lower bound `one` is spelled
    /// from: the digits markers for digits.
    fn typed_base(&self, class: TextClass, one: bool, syms: &[Sym]) -> Sym {
        match (class == TextClass::DIGIT, one) {
            (true, true) => self.unknown_digits,
            (true, false) => self.unknown_more,
            _ => syms[class.0 as usize * 2 + usize::from(one)],
        }
    }

    /// The most tokens unknown text `t` may hold, when that is bounded.
    pub(crate) fn text_len(&self, t: Token) -> Option<u32> {
        crate::env::text_len(&self.out.interner, &t)
    }

    /// Whether `sym` is unknown digits that may be none.
    pub(crate) fn is_more_digits(&self, sym: Sym) -> bool {
        sym == self.unknown_more || self.name(sym).starts_with(MORE_DIGITS)
    }

    /// The class of unknown text `t` stands for, whether it holds at least
    /// one token, and the control sequences it may hold besides: the digits
    /// markers, typed digits and typed text.
    pub(crate) fn typed_shape(&self, t: Token) -> Option<(TextClass, bool, Vec<Sym>)> {
        let sym = t.cs()?;
        if self.is_more_digits(sym) {
            return Some((TextClass::DIGIT, false, Vec::new()));
        }
        if sym == self.unknown_digits || self.name(sym).starts_with(crate::scan::TYPED_DIGITS) {
            return Some((TextClass::DIGIT, true, Vec::new()));
        }
        let (class, one, cs) = crate::env::Typed::parse_name(self.name(sym))?;
        let cs = cs.iter().filter_map(|name| self.out.interner.lookup(name)).collect();
        Some((class, one, cs))
    }

    /// [`Machine::typed_shape`] for text that may hold no control sequence.
    pub(crate) fn typed_text_of(&self, t: Token) -> Option<(TextClass, bool)> {
        match self.typed_shape(t)? {
            (class, one, cs) if cs.is_empty() => Some((class, one)),
            _ => None,
        }
    }

    /// Whether typed text `t` after `prev` may hold part of `delimiter`:
    /// its start, or the rest of a start `prev` may end. `Some(None)`:
    /// the scan begins at `t`; `None`: what came before is not known.
    fn typed_may_hold(&self, t: Token, prev: Option<Option<Token>>, delimiter: &[Tok]) -> bool {
        if self.may_hold(t, &delimiter[..1.min(delimiter.len())]) != Some(false) {
            return true;
        }
        let continues = match prev {
            None => true,
            Some(None) => false,
            Some(Some(p)) => delimiter[..delimiter.len() - 1].contains(&p.tok) || self.may_hold(p, delimiter).is_some(),
        };
        delimiter.len() > 1 && continues && self.may_hold(t, &delimiter[1..]) != Some(false)
    }

    /// Whether unknown text `t` (typed text or one unknown character) may
    /// hold a token of `needle` or a brace; `None` for a known token.
    pub(crate) fn may_hold(&self, t: Token, needle: &[Tok]) -> Option<bool> {
        let class = match t.tok {
            // A printed number's sign or a digit: one of the characters
            // [`crate::tex::unknown_number_chars`] names, so never a `.`.
            Tok::Chr(c, Catcode::Other) if crate::tex::unknown_number_chars(c).is_some() => {
                return Some(needle.iter().any(|d| {
                    matches!(*d, Tok::Chr(e, Catcode::Other) if e == c || crate::tex::may_equal_unknown_char(c as u32, e as u32))
                }));
            }
            // One character: never empty, never a control sequence.
            Tok::Chr(c, _) if crate::tex::is_unknown_char(c) => {
                classes::TokenClass::of_text(TextClass::of(t.tok), true, Vec::new())
            }
            Tok::Cs(_) if self.typed_shape(t).is_some() => self.token_class(t, false),
            _ => return None,
        };
        Some(class.may_brace() || needle.iter().any(|d| class.may_be(*d)))
    }

    /// Whether unknown text `t` may begin with `needle`: its class may be
    /// each of its tokens.
    fn may_begin(&self, t: Token, needle: &[Tok]) -> bool {
        let class = match t.tok {
            Tok::Cs(sym) if sym == self.unknown => return true,
            Tok::Chr(c, _) if crate::tex::is_unknown_char(c) => match needle.len() {
                1 => classes::TokenClass::of_text(TextClass::of(t.tok), true, Vec::new()),
                _ => return false,
            },
            Tok::Cs(_) if self.typed_shape(t).is_some() || self.one_of_members(t).is_some() => {
                self.token_class(t, false)
            }
            _ => return false,
        };
        needle.iter().all(|d| class.may_be(*d))
    }

    /// Whether typed text `t` may hold a control sequence that expands.
    pub(crate) fn typed_text_expands(&self, t: Token) -> bool {
        self.typed_shape(t).is_none_or(|(_, _, cs)| cs.iter().any(|sym| self.expandable(*sym)))
    }

    /// The characters one token taken from text of `class` may be, one per
    /// category the class spans: none where it may be a space, a brace or
    /// a control sequence.
    fn class_chars(class: TextClass) -> Vec<Tok> {
        if let Some(c) = class.one_char() {
            return vec![c];
        }
        let letters = TextClass(class.0 & TextClass::LETTER.0);
        let others = TextClass(class.0 & !TextClass::LETTER.0);
        match (letters.one_char(), others.one_char()) {
            (Some(l), Some(o)) => vec![l, o],
            _ => Vec::new(),
        }
    }

    /// Whether `sym` stands for a name built from partly unknown text.
    pub(crate) fn is_unknown_name(&self, sym: Sym) -> bool {
        self.out.interner.name(sym).starts_with(UNKNOWN_NAME)
    }

    /// The one control sequence `\csname` builds from partly unknown text.
    pub(crate) fn unknown_name_token(&mut self, prefix: &str, suffix: &str, digits: bool, span: Span) -> Token {
        let gap = if digits { UNKNOWN_DIGITS_GAP } else { UNKNOWN_NAME_GAP };
        let sym = self.intern(&format!("{UNKNOWN_NAME}{prefix}{gap}{suffix}"));
        // Each undefined name it may spell may now be `\relax` (tex.web § 372).
        // A wholly unknown name is a fresh one, as for `\def` (unknown-name-defines-fresh).
        if !(prefix.is_empty() && suffix.is_empty()) {
            self.wild.add(prefix.to_string(), suffix.to_string(), digits, None);
        }
        // A name that can spell no defined name is undefined, so `\csname`
        // makes it `\relax`, locally as TeX does.
        let meaning = match self.spells_defined(prefix, suffix) {
            false => Meaning::Primitive(Primitive::Relax),
            true => Meaning::Unknown,
        };
        if self.env.slot(sym).is_none_or(|b| b.meaning != meaning || b.may.is_some()) {
            self.env.set(sym, Binding::builtin(meaning), false);
        }
        Token::new(Tok::Cs(sym), span)
    }

    /// Whether a name of `prefix`, unknown text and `suffix` may be one
    /// with a meaning besides `\relax`: a defined name of that shape, or
    /// one a name built from unknown text may have defined.
    fn spells_defined(&self, prefix: &str, suffix: &str) -> bool {
        self.wild.overlaps_real(prefix, suffix)
            || self.env.bound().into_iter().any(|s| {
                let name = self.out.interner.name(s);
                name.len() >= prefix.len() + suffix.len()
                    && name.starts_with(prefix)
                    && name.ends_with(suffix)
                    && !self.is_unknown_name(s)
                    && !matches!(self.env.meaning(s), Meaning::Undefined | Meaning::Primitive(Primitive::Relax))
            })
    }

    /// Unknown text that may hold code runs: what that code may do is done,
    /// over-approximated. A name it, or a macro it reaches, may define that
    /// has no meaning yet may now have one: a fresh name, as for a `\\def`
    /// of an unknown name, so names with a meaning keep theirs. A count or
    /// dimension register it may touch may now hold any value of its kind.
    /// `texts` is code known in order, `named` names known in no order.
    pub(crate) fn may_run(&mut self, by: Sym, named: &[Sym], texts: &[&[Token]], span: Span) {
        use crate::tex::RegKind;
        let mut seen: HashSet<Sym> = named.iter().copied().collect();
        let mut queue = named.to_vec();
        let (mut fresh, mut regs) = (Vec::new(), Vec::new());
        // The code reached is read up to this many tokens.
        let mut left = self.cfg.limits.expansion_tokens;
        // The text's own names come in no order: with a defining command
        // among them, each may be what it defines.
        let defining = |m: &Self, cs: Sym| {
            matches!(
                m.env.meaning(cs).prim(),
                Some(Primitive::Def { .. } | Primitive::Let { .. } | Primitive::RegisterDef(_))
            )
        };
        let defines = named.iter().any(|&cs| defining(self, cs));
        for &cs in named {
            match self.env.meaning(cs) {
                Meaning::Undefined if defines => fresh.push(cs),
                Meaning::Register(kind @ (RegKind::Count | RegKind::Dimen), _) => regs.push((self.storage(cs), kind)),
                _ => {}
            }
        }
        let mut texts: Vec<Rc<[Token]>> = texts.iter().map(|t| Rc::from(*t)).collect();
        loop {
            let text = match texts.pop() {
                Some(text) => text,
                None => match queue.pop() {
                    Some(sym) => match self.env.meaning(sym) {
                        Meaning::Macro(m) => m.replacement_text.clone(),
                        _ => continue,
                    },
                    None => break,
                },
            };
            let (mut defines, mut numbered) = (false, None::<(RegKind, i64)>);
            for t in text.iter() {
                left = left.saturating_sub(1);
                if let Some((kind, n)) = numbered {
                    match t.tok {
                        Tok::Chr(c, Catcode::Other) if c.is_ascii_digit() => {
                            numbered = Some((kind, n.saturating_mul(10) + i64::from(c as u8 - b'0')));
                            continue;
                        }
                        _ => {
                            let reg = self.register_sym(kind, n);
                            regs.push((reg, kind));
                            numbered = None;
                        }
                    }
                }
                let Some(cs) = t.cs() else { continue };
                match self.env.meaning(cs) {
                    Meaning::Undefined if defines => fresh.push(cs),
                    Meaning::Macro(_) if seen.insert(cs) => queue.push(cs),
                    Meaning::Register(kind @ (RegKind::Count | RegKind::Dimen), _) => {
                        regs.push((self.storage(cs), kind))
                    }
                    Meaning::Primitive(Primitive::Register(kind @ (RegKind::Count | RegKind::Dimen))) => {
                        numbered = Some((kind, 0));
                    }
                    Meaning::Primitive(Primitive::Endinput) => {
                        if let Some(file) = self.innermost_file() {
                            self.may_have_ended.insert(file);
                        }
                    }
                    _ => {}
                }
                defines = defining(self, cs);
            }
            if left == 0 {
                break;
            }
        }
        fresh.sort_unstable();
        fresh.dedup();
        fresh.retain(|&name| matches!(self.env.meaning(name), Meaning::Undefined));
        regs.sort_unstable_by_key(|&(reg, _)| reg);
        regs.dedup_by_key(|&mut (reg, _)| reg);
        for &name in &fresh {
            self.prefixes.global = true;
            self.define(name, Meaning::Unknown, by, DefMode::Declare, span);
        }
        for &(reg, kind) in &regs {
            let (quantity, num) = match kind {
                RegKind::Dimen => (crate::value::Quantity::Dimen, crate::value::Num::full(true)),
                _ => (
                    crate::value::Quantity::Int,
                    crate::value::Num::range(crate::value::WORD_MIN, crate::value::WORD_MAX),
                ),
            };
            self.env.set_value(reg, crate::value::Value::Num { quantity, num }, true);
        }
        if fresh.is_empty() && regs.is_empty() && left > 0 {
            return;
        }
        let names: Vec<String> = fresh.iter().map(|&n| format!("\\{}", self.name(n))).collect();
        let more = if left == 0 { "; it reached more code than was read" } else { "" };
        self.diagnose(
            Severity::Imprecision,
            "joined-code-may-define",
            span,
            format!(
                "code a join made unknown may have run: {} may be defined and {} registers may hold any value{more}",
                if names.is_empty() { "no name".to_string() } else { names.join(", ") },
                regs.len()
            ),
        );
    }

    /// Whether an undefined `sym` may have been defined by a name built from
    /// unknown text.
    pub(crate) fn may_be_defined(&self, sym: Sym) -> bool {
        !self.wild.is_empty() && self.wild.matches(self.out.interner.name(sym))
    }

    /// Whether an undefined `sym` may have a meaning besides `\relax`.
    pub(crate) fn may_be_real(&self, sym: Sym) -> bool {
        !self.wild.is_empty() && self.wild.real(self.out.interner.name(sym))
    }
}

/// The tokens of `delimiter` but its last: what may begin a match.
fn delimiter_prefix(delimiter: &[Tok]) -> &[Tok] {
    &delimiter[..delimiter.len().saturating_sub(1)]
}

/// Conditional nesting of text passed over unexpanded (tex.web § 494).
/// A name of unknown meaning counts as no conditional: unknown text is
/// assumed balanced, as a cut assumes closed what it opened
/// (`unbalanced-path`). A caller that counts `\or`s at its own level
/// cannot rule one out there, and gives up (`case_arms`).
#[derive(Default)]
pub(crate) struct CondDepth(pub usize);

impl CondDepth {
    /// What skipping sees of `p`: 1 a conditional, 2 `\fi`, 3 `\else`,
    /// 4 `\or`, 0 anything else.
    pub(crate) fn role(p: Option<Primitive>) -> u8 {
        match p {
            Some(Primitive::If(_)) => 1,
            Some(Primitive::Fi) => 2,
            Some(Primitive::Else) => 3,
            Some(Primitive::Or) => 4,
            _ => 0,
        }
    }

    /// Counts `p`; `\fi`, `\else` or `\or` at level 0 is returned.
    pub(crate) fn feed(&mut self, p: Option<Primitive>) -> Option<Primitive> {
        match (Self::role(p), self.0) {
            (1, _) => self.0 += 1,
            (2, d) if d > 0 => self.0 -= 1,
            (2..=4, 0) => return p,
            _ => {}
        }
        None
    }
}
