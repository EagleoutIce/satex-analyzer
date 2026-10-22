use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// `opaque_conditionals` entry standing for every switch the document's
/// own files declare with `\newif` and the like.
pub const EVERY_SWITCH: &str = "*";

/// What bounds the analysis: reaching one stops that part of the run and
/// records an [`Imprecision`](crate::facts::Severity::Imprecision), so no
/// input can make satex loop or run out of memory.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    /// Tokens the document may digest, and the kernel on top of that.
    pub steps: u64,
    pub format_steps: u64,
    /// How deep `\input` may nest.
    pub file_depth: u16,
    /// How often a macro may re-enter itself, and how often one call site may
    /// expand without the source moving on — TeX loops are tail recursive.
    pub expansion_depth: u16,
    pub site_expansions: u32,
    /// Steps the run may spend while it reads one source line (the
    /// innermost file's line; a cache replay is charged to no line) before
    /// the expansion above that line is cut: what it wrote is widened, an
    /// unknown token stands for the rest, and the line reads on. A second overrun skips the rest of the line, a
    /// third stops the run. The termination guard: the run spends at most
    /// 3 times this per line read. 0 turns it off.
    pub line_steps: u64,
    /// Steps a line a run wrote may take where satex replays it to see
    /// what it names (labels, keys). Past them that line names nothing.
    pub replay_steps: u64,
    /// How often one construct satex cannot follow may be met at one place
    /// before the expansion it is in is given up the same way.  A path that
    /// fails at a site once fails there again on every round, so counting
    /// the failure into the millions says nothing the first one did not.
    pub repeat_failures: u32,
    /// Wall-clock seconds a run may take before it stops and says so; 0 for
    /// no limit.  A safety net: no document should come near it.  Like
    /// `memory`, it never changes where a path is cut before that stop.
    pub seconds: u64,
    /// Undecided choices one command may make on one path.  Past them a
    /// conditional is taken as unknown text, reported as an `assumption`.
    pub choices: u16,
    /// Undecided choices one site (a conditional, a name of several
    /// meanings) may make, over all paths and replays, while the run reads
    /// one source line. Past them the expansion above that line is cut as
    /// `line_steps` cuts it (`site-forks-cut`): what it assigned is unknown,
    /// and the line reads on after an unknown token. Many paths through
    /// one place say little the join of a few did not. 0 turns it off.
    pub site_forks: u32,
    /// Paths one drive keeps waiting before the two furthest back at one
    /// source position are cut to the input levels they share and joined.
    pub paths: u16,
    /// Paths one drive keeps waiting before it joins those that stand
    /// alike (one source position, the same conditionals, groups and
    /// scans): what their token lists differ in becomes one token that
    /// stands for either text.
    pub alike: u16,
    /// Loop heads at one place kept apart because the paths differ in code
    /// they will still run (an operation stacked or not) before they are
    /// joined after all. Halved under time pressure.
    pub heads_apart: u16,
    /// Choices one command may have made on one path for a name that a
    /// join left with one of several meanings to still split the run into a
    /// path per meaning; past them, and with 0 everywhere, it is read as a
    /// name of unknown meaning.
    pub meaning_splits: u16,
    /// How many rounds a loop head analyzes its paths again from a grown
    /// joined state before what still grows is widened (SEMANTICS § 14).
    pub widen_after: u16,
    /// How many runs of the build (latexmk's rounds) the analysis takes to
    /// reach the files' fixpoint; past them what still changes is unknown.
    /// 1 runs once, with no file of an earlier run.
    pub build_runs: u16,
    /// How many levels deep a recursion whose continuation is undecided is
    /// followed before it is assumed to end within them.
    pub recursion_depth: u16,
    /// How many loop heads and loop entries one drive keeps with their
    /// joined states; past them the oldest is forgotten. Its count stays
    /// in the load's loop summaries, so it is summarized when met again.
    pub loop_heads: u16,
    /// How many runs of one call, at one site with the same names handed
    /// to it, are recorded before a later call with unknown operands applies
    /// their join instead of running; 0 never summarizes a call.
    pub call_summary_after: u16,
    /// How many times in a row one call may meet the state it met before,
    /// with the same arguments, input, nesting and bindings, before its
    /// path is ended (the only path is summarized instead): TeX repeats
    /// that state forever; 0 never ends one.
    pub cycle_after: u16,
    /// How many meanings a join leaves a name with as a set (one of them)
    /// before it has an unknown meaning.
    pub meaning_set: u16,
    /// How many arms a join of paths keeps paired ([`crate::env::Corr`]):
    /// which binding of one name goes with which of another.
    pub join_arms: u16,
    /// How many names a join pairs at most: past them the rest are joined
    /// apart, as unpaired names are.
    pub join_names: u16,
    /// How many values a join leaves a count or dimension register with as
    /// a set before only their interval is kept.
    pub value_set: u16,
    /// How many members a join leaves a number with as a set once
    /// intervals are among them (`{[0,10], [100,120]}`) before only their
    /// hull is kept.
    pub interval_set: u16,
    /// A fallback, off by default: past this percentage of `steps`, and
    /// again at each further third of what is left, the limits that only
    /// cost time (paths, sets, choices, loop and recursion widening, call
    /// summaries, `site_forks`, `line_steps`) are halved, so the run
    /// degrades its precision and finishes (`precision-degraded`); 100
    /// never degrades. Steps are counted the same on every machine, so the
    /// result does not depend on its speed. Seconds and memory never steer
    /// the run: they only stop it.
    #[serde(deserialize_with = "percent")]
    pub degrade_at_percent: u8,
    /// Tokens one expansion may add, and tokens one undecided conditional may
    /// copy into its arms.  TeX itself has no such limit, so these only stop
    /// a single pathological read from taking all of `memory`; what makes a
    /// run terminate is `steps`, `held_tokens` and the recursion widening.
    /// Only tokens an expansion produces count: an argument scan or a skipped
    /// arm expands nothing, so it reads only what is already held or a file,
    /// and a long argument is shared, not copied, into a replacement text.
    pub expansion_tokens: usize,
    pub conditional_tokens: usize,
    /// Tokens the input stack may hold at once.
    pub held_tokens: usize,
    /// How many token lists may be open at once: one per macro being
    /// expanded, argument being read or file being input.
    pub expansion_stack: usize,
    /// Ceilings on what the run records.
    pub vertices: usize,
    pub facts: usize,
    pub trace: usize,
    /// Entries on the save stack: one per assignment a group may have to undo
    /// (tex.web § 268).
    pub save_stack: usize,
    /// What this run may hold at once, written the way `ulimit` takes it
    /// (`512M`, `4GiB`); zero lifts the limit.  Only the `satex` binary counts
    /// allocations, so a library user gets no limit.
    pub memory: bytesize::ByteSize,
    /// Threads any parallel work may use; 0 lets satex choose.  The
    /// interpreter runs on one thread because TeX's state is one machine, so
    /// this bounds the file walks around it.
    pub threads: usize,
    /// How much the caches in the cache directory may take together;
    /// the least recently used go first.  Zero lifts the bound.
    pub cache_size: bytesize::ByteSize,
}

/// A percentage: at most 100.
fn percent<'de, D: serde::Deserializer<'de>>(d: D) -> Result<u8, D::Error> {
    let n = u8::deserialize(d)?;
    if n > 100 {
        return Err(serde::de::Error::custom(format!("{n} is not a percentage (at most 100)")));
    }
    Ok(n)
}

impl Limits {
    /// The value of the limit `limits.<key>` names, as a number.
    pub fn value(&self, key: &str) -> Option<u64> {
        Some(match key {
            "steps" => self.steps,
            "format_steps" => self.format_steps,
            "file_depth" => self.file_depth.into(),
            "expansion_depth" => self.expansion_depth.into(),
            "site_expansions" => self.site_expansions.into(),
            "line_steps" => self.line_steps,
            "replay_steps" => self.replay_steps,
            "repeat_failures" => self.repeat_failures.into(),
            "seconds" => self.seconds,
            "choices" => self.choices.into(),
            "site_forks" => self.site_forks.into(),
            "paths" => self.paths.into(),
            "alike" => self.alike.into(),
            "heads_apart" => self.heads_apart.into(),
            "widen_after" => self.widen_after.into(),
            "recursion_depth" => self.recursion_depth.into(),
            "loop_heads" => self.loop_heads.into(),
            "call_summary_after" => self.call_summary_after.into(),
            "cycle_after" => self.cycle_after.into(),
            "degrade_at_percent" => self.degrade_at_percent.into(),
            "expansion_tokens" => self.expansion_tokens as u64,
            "conditional_tokens" => self.conditional_tokens as u64,
            "held_tokens" => self.held_tokens as u64,
            "expansion_stack" => self.expansion_stack as u64,
            "vertices" => self.vertices as u64,
            "facts" => self.facts as u64,
            "trace" => self.trace as u64,
            "save_stack" => self.save_stack as u64,
            "memory" => self.memory.as_u64(),
            _ => return None,
        })
    }

    /// How many threads the file walks may use.  Zero means satex chooses:
    /// enough to overlap the reads, few enough to leave the machine usable.
    pub fn thread_count(&self) -> usize {
        /// Walking is bound by the file system, not the processor, so more
        /// threads than this buy nothing.
        const ENOUGH: usize = 4;
        match self.threads {
            0 => std::thread::available_parallelism().map_or(1, |n| n.get()).min(ENOUGH),
            n => n,
        }
    }
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            steps: 4_000_000_000,
            // The real expl3 reads the Unicode data files while the kernel
            // is built; that is done once and then cached.
            format_steps: 9_000_000_000,
            file_depth: 64,
            expansion_depth: 512,
            // Measured: the kernel's own helpers (`\hexnumber@` inside
            // `\DeclareMathSymbol`, the expl3 mapping functions) re-expand one
            // site far more often than that while the source stands still,
            // and widening them loses every math symbol after the first few.
            // pgfkeys runs one helper site for every key of a `\tikzset`
            // list read in one piece, so a long option list passes 256
            // without looping, and widening it there costs more tokens
            // than following it.
            site_expansions: 16_384,
            // Measured: the heaviest line of a long thesis takes 34-67M
            // steps (about 30 s); a stuck line costs at most 3 times this.
            line_steps: 100_000_000,
            // Measured: a `\newlabel` or `\@writefile` line takes a few
            // thousand steps; one of unknown text may take any number.
            replay_steps: 1_000_000,
            // Measured: the kernel's own `\if` chains report a handful of
            // the same gap while they are still making progress, and no
            // document needed more than a few before the path repeated
            // itself.
            repeat_failures: 8,
            seconds: 300,
            choices: 8,
            site_forks: 4_096,
            paths: 16,
            alike: 8,
            heads_apart: 32,
            meaning_splits: 4,
            widen_after: 3,
            build_runs: 5,
            recursion_depth: 100,
            loop_heads: 512,
            call_summary_after: 2,
            cycle_after: 2,
            meaning_set: 8,
            join_arms: 32,
            join_names: 64,
            value_set: 8,
            interval_set: 4,
            degrade_at_percent: 100,
            // A `Token` is 16 bytes, so a million of them is 16 MiB: enough
            // for any argument an expansion really writes, and far below
            // `memory`.
            expansion_tokens: 1_048_576,
            conditional_tokens: 1_048_576,
            held_tokens: 4_000_000,
            expansion_stack: 4_000,
            vertices: 400_000,
            facts: 2_000_000,
            trace: 200_000,
            save_stack: 2_000_000,
            // Enough for the kernel and a large document several times over,
            // and small enough that a runaway run dies before the machine
            // starts swapping.
            memory: bytesize::ByteSize::gib(8),
            threads: 0,
            cache_size: bytesize::ByteSize::gib(2),
        }
    }
}

/// Which plugin kinds a run consults.  The provider, platform, engine and
/// output that make up [`crate::plugin::Plugins`] are always resolved; these
/// two read the document itself.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PluginSwitches {
    /// `% !TeX program`, `% !TeX root`, `% arara:` and the rest.
    pub magic: bool,
    /// biber, makeindex and the other tools the packages imply.
    pub tools: bool,
    /// `%&format` lines and `-fmt=`: the format file a build starts from.
    pub preload: bool,
    /// Walking the project directory for its documents and build files,
    /// skipping what `.gitignore` excludes.
    pub discovery: bool,
    /// Asking the distribution's package manager which package ships a file
    /// the search could not find, and whether it could be installed.  Off:
    /// `tlmgr info` and `tlmgr search --global` read the repository's
    /// database, so a lint run would reach the network (TeX Live manual,
    /// `tlmgr`).  `satex install` asks regardless, being the request itself.
    pub packages: bool,
}

impl Default for PluginSwitches {
    fn default() -> PluginSwitches {
        PluginSwitches { magic: true, tools: true, preload: true, discovery: true, packages: false }
    }
}

/// `lsp:` in `satex.yaml`: how `satex lsp` listens and re-analyzes.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct LspConfig {
    /// TCP port to listen on instead of stdio; unset (the default) keeps
    /// stdio, which every editor client speaks without extra setup.
    pub port: Option<u16>,
    /// Interface `port` binds to.
    pub host: String,
    /// How long an edit waits with no further edits before it is
    /// re-analyzed, so fast typing coalesces into one run.
    pub debounce_ms: u64,
}

impl Default for LspConfig {
    fn default() -> LspConfig {
        LspConfig { port: None, host: "127.0.0.1".into(), debounce_ms: 300 }
    }
}

/// `clock:` in `satex.yaml`: what `\year`, `\month`, `\day`, `\time` and
/// `\pdfcreationdate` hold (tex.web § 241).  `now` is the analysis time,
/// `unknown` leaves the date unknown, and an ISO date-time
/// (`2026-09-29`, `2026-09-29T14:30`, `2026-09-29T14:30:00+02:00`) pins it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Clock {
    #[default]
    Now,
    Unknown,
    At(time::OffsetDateTime),
}

impl Clock {
    /// The date-time the run compiles at, `None` when unknown.
    pub fn resolve(self) -> Option<time::OffsetDateTime> {
        match self {
            Clock::Now => Some(time::OffsetDateTime::now_local().unwrap_or_else(|_| time::OffsetDateTime::now_utc())),
            Clock::Unknown => None,
            Clock::At(at) => Some(at),
        }
    }

    /// `YYYY-MM-DD`, then an optional `THH:MM[:SS]`, then an optional `Z`
    /// or `±HH:MM`; the run's own zone otherwise.
    fn parse(text: &str) -> Option<time::OffsetDateTime> {
        let (date, rest) = text.split_at(text.len().min(10));
        let mut fields = date.split('-');
        let mut int = |width: usize| -> Option<i32> {
            let f = fields.next()?;
            (f.len() == width && f.bytes().all(|b| b.is_ascii_digit())).then(|| f.parse().ok()).flatten()
        };
        let (year, month, day) = (int(4)?, int(2)?, int(2)?);
        if fields.next().is_some() {
            return None;
        }
        let month = time::Month::try_from(u8::try_from(month).ok()?).ok()?;
        let date = time::Date::from_calendar_date(year, month, u8::try_from(day).ok()?).ok()?;
        let (mut hour, mut minute, mut second) = (0u8, 0u8, 0u8);
        let mut rest = rest;
        if let Some(clock) = rest.strip_prefix('T').or_else(|| rest.strip_prefix(' ')) {
            let end = clock.find(['Z', '+', '-']).unwrap_or(clock.len());
            let mut parts = clock[..end].split(':');
            let mut part = || parts.next().and_then(|p| (p.len() == 2).then(|| p.parse::<u8>().ok()).flatten());
            hour = part()?;
            minute = part()?;
            second = part().unwrap_or(0);
            rest = &clock[end..];
        }
        let time = time::Time::from_hms(hour, minute, second).ok()?;
        let offset = match rest {
            "" => time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC),
            "Z" => time::UtcOffset::UTC,
            zone => {
                let sign = if zone.starts_with('-') { -1 } else { 1 };
                let (h, m) = zone[1..].split_once(':')?;
                time::UtcOffset::from_hms(sign * h.parse::<i8>().ok()?, sign * m.parse::<i8>().ok()?, 0).ok()?
            }
        };
        Some(date.with_time(time).assume_offset(offset))
    }
}

impl std::fmt::Display for Clock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Clock::Now => f.write_str("now"),
            Clock::Unknown => f.write_str("unknown"),
            Clock::At(at) => {
                let (h, m, _) = at.offset().as_hms();
                write!(
                    f,
                    "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{:+03}:{:02}",
                    at.year(),
                    u8::from(at.month()),
                    at.day(),
                    at.hour(),
                    at.minute(),
                    at.second(),
                    h,
                    m.abs()
                )
            }
        }
    }
}

impl Serialize for Clock {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl std::str::FromStr for Clock {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, String> {
        match text.trim() {
            "now" => Ok(Clock::Now),
            "unknown" => Ok(Clock::Unknown),
            at => Clock::parse(at)
                .map(Clock::At)
                .ok_or_else(|| format!("clock: `{at}` is not `now`, `unknown` or an ISO date-time")),
        }
    }
}

impl<'de> Deserialize<'de> for Clock {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?.parse().map_err(serde::de::Error::custom)
    }
}

/// How the engine was started with respect to `\write18` (TeX Live manual).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ShellEscape {
    /// `--no-shell-escape`: nothing runs.
    None,
    /// `--shell-restricted`: only the programs the installation's
    /// `shell_escape_commands` names (`kpsewhich -var-value`).
    #[default]
    Restricted,
    /// `--shell-escape`: anything runs.
    Full,
}

/// The engine's `-interaction` option (tex.web § 73): `\batchmode` = 0 …
/// `\errorstopmode` = 3.  Builds run unattended, so nothing is typed at the
/// terminal; the default is `nonstopmode`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Interaction {
    Batchmode,
    #[default]
    Nonstopmode,
    Scrollmode,
    Errorstopmode,
}

/// `cache.index:` in `satex.yaml`.  A package cache holds the state a
/// package leaves when it is loaded right after `\documentclass{class}`;
/// a document that loads it in the same state starts from the cache.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CacheIndex {
    /// The packages `satex cache build` prepares. Empty means it discovers
    /// them instead, from what ordinary runs already found expensive to
    /// interpret in the cache directory, or with nothing measured yet, the
    /// packages installed packages load most.
    pub packages: Vec<String>,
    /// Packages it prepares first when `packages` is empty, as far as they
    /// are installed.
    pub common: Vec<String>,
    /// The engines it prepares them for; empty means the one in effect.
    pub engines: Vec<Engine>,
    /// The class the prepared packages are loaded after.
    pub class: String,
    /// Whether every run stores the packages it reads, so the next run
    /// with the same preamble reuses them.
    pub auto: bool,
    /// Set by `satex cache build --refresh`: rebuild instead of reading.
    #[serde(skip)]
    pub refresh: bool,
    /// Set while `satex cache build` runs, which stores even when `auto`
    /// is off.
    #[serde(skip)]
    pub building: bool,
}

impl Default for CacheIndex {
    fn default() -> Self {
        Self {
            packages: Vec::new(),
            common: [
                "microtype",
                "cleveref",
                "zref-clever",
                "savetrees",
                "tabularray",
                "xurl",
                "glossaries",
                "enumitem",
                "nowidow",
                "silence",
                "adjustbox",
                "pdfpages",
                "pgfplotstable",
                "memoize",
                "classicthesis",
            ]
            .map(String::from)
            .to_vec(),
            engines: Vec::new(),
            class: "article".into(),
            auto: true,
            refresh: false,
            building: false,
        }
    }
}

impl Config {
    /// `analysis.assume_compiles`.
    pub fn compiles(&self) -> bool {
        self.analysis.assume_compiles
    }
}

/// `conventions:` in `satex.yaml`: what SaTeX takes for granted about how
/// TeX is written, each a choice a project can change.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Conventions {
    /// What makes a name internal, a package's own and not its interface:
    /// regexes on the name without its backslash. Internal names are left
    /// out of listings, completion and the public-name lints.
    pub internal: Vec<String>,
    /// Commands besides those satex finds in the kernel whose next control
    /// sequence is the name they define, as the package files on disk are
    /// read for what they define.
    pub definers: Vec<String>,
}

impl Conventions {
    /// Whether `name` (without its backslash) is internal by `internal`.
    pub fn is_internal(&self, name: &str) -> bool {
        static COMPILED: std::sync::Mutex<Vec<(String, Option<regex::Regex>)>> = std::sync::Mutex::new(Vec::new());
        let mut compiled = COMPILED.lock().unwrap_or_else(|e| e.into_inner());
        self.internal.iter().any(|pattern| {
            let at = match compiled.iter().position(|(p, _)| p == pattern) {
                Some(at) => at,
                None => {
                    compiled.push((pattern.clone(), regex::Regex::new(pattern).ok()));
                    compiled.len() - 1
                }
            };
            compiled[at].1.as_ref().is_some_and(|r| r.is_match(name))
        })
    }
}

impl Default for Conventions {
    fn default() -> Conventions {
        Conventions { internal: vec!["@".into(), "^__".into(), "_.*:".into()], definers: Vec::new() }
    }
}

/// `load:` in `satex.yaml`: what a run follows from the document.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Load {
    pub packages: bool,
    pub classes: bool,
    pub inputs: bool,
    /// Interpret the installation's `latex.ltx`.
    pub format: bool,
    /// Packages to leave unread.
    pub skip: Vec<String>,
    /// Classes and packages the file expects loaded before it, as a beamer
    /// theme expects beamer: read first, as a document would load them.
    pub expect: Vec<String>,
    /// Find what a package or class file expects loaded from the names it
    /// uses and the packages on disk that define them.
    pub expect_detect: bool,
    /// Read the files an earlier build wrote (`build_extensions`) as they
    /// are on disk. Off, they count as absent: the run over-approximates
    /// them by what it writes itself.
    pub build_files: bool,
    pub build_extensions: Vec<String>,
}

impl Load {
    /// Whether `path` is a file a build writes: one of `build_extensions`
    /// named after a `.tex` file the build ran or `\include`d
    /// (`\jobname.aux`, `chapter.aux`). A `.gls` without one is a source.
    pub fn is_build_file(&self, path: &std::path::Path) -> bool {
        let name = path.to_string_lossy();
        self.build_extensions.iter().any(|e| {
            name.strip_suffix(e.as_str()).and_then(|stem| stem.strip_suffix('.')).is_some_and(|stem| {
                let tex = std::path::PathBuf::from(format!("{stem}.tex"));
                tex.is_file() || crate::overlay::holds(&tex)
            })
        })
    }
}

impl Default for Load {
    fn default() -> Load {
        Load {
            packages: true,
            classes: true,
            inputs: true,
            format: true,
            skip: Vec::new(),
            expect: Vec::new(),
            expect_detect: true,
            build_files: false,
            build_extensions: [
                "aux",
                "toc",
                "lof",
                "lot",
                "loa",
                "lol",
                "out",
                "bbl",
                "bcf",
                "run.xml",
                "blg",
                "glo",
                "gls",
                "glg",
                "acn",
                "acr",
                "alg",
                "ist",
                "idx",
                "ind",
                "ilg",
                "nlo",
                "nls",
                "slnc",
                "brf",
                "nav",
                "snm",
                "vrb",
                "fls",
                "fdb_latexmk",
                "xdy",
                "log",
            ]
            .map(String::from)
            .to_vec(),
        }
    }
}

/// `texlive:` in `satex.yaml`: the installation to use instead of detecting
/// one.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Texlive {
    pub root: Option<PathBuf>,
    pub year: Option<u32>,
}

/// `texmf:` in `satex.yaml`: where a run looks for the installation besides
/// `texlive:`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Texmf {
    pub roots: Vec<PathBuf>,
    /// A directory of TeX sources laid out like `texmf-dist`, searched after
    /// `roots`. Without `roots` it is the whole installation: no
    /// `kpsewhich`, no fallback paths. Its cache archive is
    /// `satex-cache.bin` (`satex cache import <bundle>`).
    pub bundle: Option<PathBuf>,
    /// May run `kpsewhich` to find them.
    pub kpsewhich: bool,
    /// Whether an unresolved installation may fall back to well-known
    /// system paths (`/usr/local/texlive/...`, `$HOME/texmf`, ...). Off,
    /// together with `kpsewhich: false`, makes a run behave as if this
    /// machine had no TeX installation at all, engine primitives only —
    /// for testing that path without uninstalling one.
    pub fallback: bool,
}

impl Default for Texmf {
    fn default() -> Texmf {
        Texmf { roots: Vec::new(), bundle: None, kpsewhich: true, fallback: true }
    }
}

/// `cache:` in `satex.yaml`: the interpreted kernel and packages kept
/// between runs.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Cache {
    /// Keep the interpreted kernel and packages between runs.
    pub enabled: bool,
    pub dir: Option<PathBuf>,
    #[serde(default)]
    pub rebuild: bool,
    /// What `satex cache build` prepares, and whether ordinary runs store
    /// the packages they read.
    #[serde(default)]
    pub index: CacheIndex,
}

impl Default for Cache {
    fn default() -> Cache {
        Cache { enabled: true, dir: None, rebuild: false, index: CacheIndex::default() }
    }
}

/// `analysis:` in `satex.yaml`: where paths join, and what the run assumes.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnalysisConfig {
    /// Points where every path still apart is joined, whatever else
    /// differs. A `command` is matched by its meaning: `\document`
    /// joins wherever the run starts that meaning, from
    /// `\begin{document}`, an alias or an `\expandafter`. A `hook` is
    /// an lthooks name and joins when the hook is used. A slot the
    /// paths disagree on becomes the join of their values, widened when
    /// needed, and no path is dropped. Each join is reported once as
    /// the `imprecision` diagnostic `paths-joined-at`, with how many
    /// paths arrived and what kept them apart.
    pub join_paths_at: Vec<JoinPoint>,
    /// A soundiness assumption: the document compiles without errors. A
    /// path that raises a certain TeX error in library code (a package,
    /// the kernel) is then no run of the document, and is dropped. An
    /// error is certain when it arises on state the run knows exactly,
    /// before any approximation. An error in the document's own text is
    /// always kept, to be reported and fixed. Each drop is reported once
    /// as an `assumption` diagnostic, and the last path is never dropped.
    /// `false` keeps every path.
    pub assume_compiles: bool,
    /// A soundiness assumption: a package closes the groups it opens, so
    /// two paths at the same line of a file, one in more groups, join with
    /// those closed. `false` keeps such paths apart.
    pub assume_packages_close_groups: bool,
    /// A soundiness assumption: the build runs in the environment SaTeX
    /// runs in, which Lua's `os.getenv` reads. `false` makes every
    /// variable unknown.
    pub assume_environment: bool,
    /// A soundiness assumption: a page breaks only where a penalty forces
    /// it (`\clearpage`, `\newpage`, `\end`). `false` also fires the
    /// output routine wherever the page may have filled, one path each way.
    pub assume_forced_page_breaks: bool,
}

impl Default for AnalysisConfig {
    fn default() -> AnalysisConfig {
        AnalysisConfig {
            join_paths_at: vec![JoinPoint { command: Some("\\document".into()), hook: None }],
            assume_compiles: true,
            assume_packages_close_groups: true,
            assume_environment: true,
            assume_forced_page_breaks: true,
        }
    }
}

/// One entry of `analysis.join_paths_at`: a command, matched by the meaning
/// it has when the run starts it, or an lthooks hook, when it is used.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct JoinPoint {
    pub command: Option<String>,
    pub hook: Option<String>,
}

/// `gaps:` in `satex.yaml`: what SaTeX could not follow.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Gaps {
    /// Append what a run could not follow to `file`.
    pub log: bool,
    /// Also print it.
    pub report: bool,
    /// Where the log goes; the cache directory's `gaps.ndjson` by default.
    pub file: Option<PathBuf>,
}

impl Default for Gaps {
    fn default() -> Gaps {
        Gaps { log: true, report: true, file: None }
    }
}

/// What a stubbed macro leaves behind (`stubs.<name>.result`, and the
/// values of `sets`). Each names the narrowest element of the lattice of
/// unknown text that is sound for the macro.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StubResult {
    /// `nothing`, `box`, `number`, `dimen` or `text`.
    Plain(StubKind),
    /// `{ number: [lo, hi] }`: typed digits in that range.
    Number { number: [i64; 2] },
    /// `{ text: <class> }`: unknown text of a `TextClass`, one of `digits`,
    /// `number`, `dimension`, `cs-free`, `any`.
    Text { text: String },
    /// `{ box: { width: [lo, hi], height: [lo, hi], depth: [lo, hi] } }`:
    /// a box of unknown size within bounds. A bound is a number of `pt`, a
    /// dimension such as `2cm`, or a length register such as `\linewidth`.
    Box {
        #[serde(rename = "box")]
        bounds: BoxBounds,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StubKind {
    Nothing,
    /// A box of unknown size.
    Box,
    /// Typed digits over the whole range of a TeX integer.
    Number,
    /// A dimension as text: a number and unit letters.
    Dimen,
    /// Unknown text of any class.
    Text,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BoxBounds {
    #[serde(default)]
    pub width: Option<[String; 2]>,
    #[serde(default)]
    pub height: Option<[String; 2]>,
    #[serde(default)]
    pub depth: Option<[String; 2]>,
}

/// `sets` of a stub: names that become unknown, or, with a type, names that
/// become a macro holding that unknown text.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StubSets {
    Names(Vec<String>),
    /// `null` makes the name unknown.
    Typed(std::collections::BTreeMap<String, Option<StubResult>>),
}

impl Default for StubSets {
    fn default() -> Self {
        StubSets::Names(Vec::new())
    }
}

/// `stubs.<name>` in `satex.yaml`: an effect summary standing in for a macro
/// whose internals satex does not follow. Reads its arguments, then leaves
/// `result` and makes the names in `sets` unknown. Said once per name as an
/// `assumption`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Stub {
    /// The arguments, one letter each: `s` a star, `o` an optional `[..]`,
    /// `m` a mandatory argument.
    #[serde(default)]
    pub args: Option<String>,
    /// Instead of `args`: read up to this delimiter, which is consumed.
    #[serde(default)]
    pub until: Option<String>,
    pub result: StubResult,
    /// Names the macro assigns.
    #[serde(default)]
    pub sets: StubSets,
}

/// Settings read from `satex.yaml` ([`Config::discover`]), or the defaults
/// when none exists: what to load, where to look, and the [`Limits`] that
/// bound the run.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// The `satex.yaml` this came from, when one was read.
    #[serde(skip)]
    pub source: Option<PathBuf>,
    /// What a run follows from the document.
    pub load: Load,
    pub search_paths: Vec<PathBuf>,
    /// Fallback kpathsea search-path variables ([`PathsConfig`]); the
    /// environment and the project's build files are tried first.
    pub paths: PathsConfig,
    /// The installation to use instead of detecting one.
    pub texlive: Texlive,
    /// Where a run looks for the installation besides `texlive`.
    pub texmf: Texmf,
    /// The interpreted kernel and packages kept between runs.
    pub cache: Cache,
    /// A preamble to interpret as this project's format, the way
    /// `mylatexformat` dumps one.  Without it, a `%&` line or the build
    /// configuration names it.
    #[serde(default)]
    pub preload: Option<PathBuf>,
    pub limits: Limits,
    /// Effect summaries for macros satex should not follow ([`Stub`]), by name.
    pub stubs: std::collections::BTreeMap<String, Stub>,
    pub plugins: PluginSwitches,
    /// Macros to expand when reporting a value, such as `today` or `jobname`.
    /// Empty means report what the document wrote.
    pub expand_in_reports: Vec<String>,
    /// Primitives the `primitive-tex-command` rule reports, each with the
    /// LaTeX command to suggest instead; an empty value suggests nothing.
    /// Entries add to what the rule already knows, or override one.
    pub latex_alternatives: std::collections::BTreeMap<String, String>,
    /// What SaTeX could not follow.
    pub gaps: Gaps,
    /// Where paths join, and what the run assumes.
    pub analysis: AnalysisConfig,
    /// What `\write18` may do in this build; unset, what the build files
    /// ask for (a latexmk engine command, an arara `shell: yes`), else
    /// TeX Live's `restricted`.
    pub shell_escape: Option<ShellEscape>,
    #[serde(default)]
    pub interaction: Interaction,
    /// What the clock reads when the run starts ([`Clock`]).
    #[serde(default)]
    pub clock: Clock,
    pub engine: Option<Engine>,
    /// Which format the run starts from; detected from the source when unset.
    pub kernel: Option<crate::plugin::Kernel>,
    pub provider: Option<crate::plugin::Provider>,
    pub platform: Option<crate::plugin::Platform>,
    pub output: Option<crate::plugin::Output>,
    pub at_letter: bool,
    /// Lint rule codes that never fire here, regardless of category; a
    /// profile sets this to the rules that do not apply to its kind of
    /// input (`unused-label` for a package, the `build-*` rules for
    /// anything with no build of its own to check).
    #[serde(default)]
    pub lint_off: Vec<String>,
    /// `project:` in `satex.yaml`: which files are this project's own.
    #[serde(default)]
    pub project: Project,
    /// `lints:` in `satex.yaml`: what a single rule reads, under its code.
    #[serde(default)]
    pub lints: Lints,
    /// `conventions:` in `satex.yaml`: how TeX is usually written, where
    /// the text alone does not say.
    #[serde(default)]
    pub conventions: Conventions,
    /// The profile in force: `--profile`, `profile:` here, or
    /// [`Profile::detect`], in that order — [`Config::apply_profile`]
    /// settles one for every run, so this is `Some` by the time analysis
    /// starts. Kept so `satex --version` and `satex summary` can say which
    /// one it was.
    #[serde(default)]
    pub profile: Option<Profile>,
    /// `profiles.document`/`.package`/`.class`/`.literate`/`.plain` in
    /// `satex.yaml`: the partial config each profile merges over the
    /// general settings, the way [`Config::apply_profile`] applies one.
    #[serde(default)]
    pub profiles: Profiles,
    /// Progress reporting on stderr: 1 files, 2 token counts, 3 expansions.
    #[serde(default)]
    pub verbose: u8,
    pub trace: bool,
    /// Record no events while a package or kernel file is read.
    #[serde(default)]
    pub trace_document_only: bool,
    /// Record only these lines of the main file, and what their calls do.
    pub trace_lines: Option<(u32, u32)>,
    /// Within the first traced line, skip what stands left of this column.
    pub trace_col: u32,
    /// The file `trace_lines` and `trace_col` refer to, by a trailing part of
    /// its path (`sub/a` or `a.tex`); the main file when unset.
    pub trace_file: Option<String>,
    /// Within the last traced line, skip what stands right of this column.
    pub trace_end_col: u32,
    /// Record only these steps of the run.
    pub trace_steps: Option<(u64, u64)>,
    pub timings: bool,
    pub opaque_conditionals: Vec<String>,
    /// Arms of a choice inside an expanding scan (`\edef`, `\csname`) run
    /// as paths and join where they meet in the scan (SEMANTICS.md § 14).
    /// Off, the command is re-run once per sequence of arms. On by default.
    pub arm_joins: bool,
    pub record_arguments: bool,
    /// How the files a run reads map to TeX Live packages, for the
    /// dependency file depp writes.
    pub depp: crate::plugin::depp::DeppConfig,
    /// How `satex lsp` listens and re-analyzes.
    #[serde(default)]
    pub lsp: LspConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            source: None,
            load: Load::default(),
            search_paths: Vec::new(),
            paths: PathsConfig::default(),
            texlive: Texlive::default(),
            texmf: Texmf::default(),
            cache: Cache::default(),
            preload: None,
            limits: Limits::default(),
            stubs: std::collections::BTreeMap::new(),
            plugins: PluginSwitches::default(),
            expand_in_reports: Vec::new(),
            latex_alternatives: std::collections::BTreeMap::new(),
            gaps: Gaps::default(),
            analysis: AnalysisConfig::default(),
            shell_escape: None,
            interaction: Interaction::Nonstopmode,
            clock: Clock::Now,
            engine: None,
            kernel: None,
            provider: None,
            platform: None,
            output: None,
            at_letter: false,
            lint_off: vec!["analysis-imprecision".to_string()],
            project: Project::default(),
            lints: Lints::default(),
            conventions: Conventions::default(),
            profile: None,
            profiles: Profiles::default(),
            verbose: 0,
            trace: false,
            trace_document_only: false,
            trace_lines: None,
            trace_col: 0,
            trace_file: None,
            trace_end_col: u32::MAX,
            trace_steps: None,
            timings: false,
            opaque_conditionals: Vec::new(),
            arm_joins: true,
            record_arguments: false,
            depp: Default::default(),
            lsp: LspConfig::default(),
        }
    }
}

named_enum! {
    /// Which TeX engine's primitive set the run assumes: plain TeX, pdfTeX,
    /// LuaTeX or XeTeX. [`crate::plugin::Plugins::engine`] and `\ifpdf`-style
    /// tests key on it.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize)]
    #[serde(rename_all = "lowercase")]
    pub enum Engine {
        Tex => "tex",
        PdfTeX => "pdftex",
        LuaTeX => "luatex",
        XeTeX => "xetex",
    }
}

/// A program as a command names it: editors write `XeLaTeX` too, and
/// Windows paths end in `.exe`.
fn program_name(program: &str) -> String {
    let name = program.trim().rsplit(['/', '\\']).next().unwrap_or_default().to_ascii_lowercase();
    name.strip_suffix(".exe").map(str::to_string).unwrap_or(name)
}

impl Engine {
    /// The program that starts this engine with the LaTeX format, which is
    /// also latexmk's setting for it (latexmk manual, "$pdflatex").
    /// The engine a program name drives: `lualatex` is LuaTeX with the LaTeX
    /// format, and a magic comment names the program.
    pub fn from_program(program: &str) -> Option<Engine> {
        Some(match program_name(program).as_str() {
            // `latex` and `etex` run pdfTeX writing DVI (TeX Live's
            // `fmtutil.cnf`); `tex` is Knuth's TeX, which has no format
            // for LaTeX.
            "pdflatex" | "pdftex" | "latex" | "etex" => Engine::PdfTeX,
            "lualatex" | "luatex" | "lualatex-dev" | "dvilualatex" | "dviluatex" => Engine::LuaTeX,
            "xelatex" | "xetex" => Engine::XeTeX,
            "tex" => Engine::Tex,
            // ConTeXt MkIV runs on LuaTeX and MkXL on LuaMetaTeX, a LuaTeX
            // derivative (ConTeXt "LMTX" documentation); satex models the
            // LuaTeX primitives for both, which is why ConTeXt is
            // experimental.  The ConTeXt kernel itself is not interpreted;
            // see [`crate::plugin::Kernel`].
            "context" | "contextjit" | "luametatex" | "mtxrun" => Engine::LuaTeX,
            _ => return None,
        })
    }

    pub fn program(self) -> &'static str {
        match self {
            Engine::Tex => "tex",
            Engine::PdfTeX => "pdflatex",
            Engine::LuaTeX => "lualatex",
            Engine::XeTeX => "xelatex",
        }
    }
    /// Whether a program writes DVI rather than PDF: `latex`, `etex` and
    /// `dvilualatex` run pdfTeX or LuaTeX with `\pdfoutput` (`\outputmode`)
    /// zero, and Knuth's `tex` knows no other output.
    pub fn writes_dvi(program: &str) -> bool {
        matches!(program_name(program).as_str(), "tex" | "latex" | "etex" | "dvilualatex" | "dviluatex")
    }
    /// ε-TeX's extensions: in every engine but Knuth's TeX, which is
    /// `tex` (doc/primitive-facts.md).
    pub fn has_etex(self) -> bool {
        self != Engine::Tex
    }
    /// pdfTeX's own `\pdf…` primitives.  LuaTeX dropped them for
    /// `\pdfextension` and friends (LuaTeX manual, "Changes from pdfTeX"),
    /// and XeTeX only ever took the position and page-size ones.
    pub fn has_pdftex(self) -> bool {
        self == Engine::PdfTeX
    }
    /// The unprefixed extensions pdfTeX added that XeTeX and LuaTeX kept:
    /// `\expanded`, `\ifincsname`, `\partokenname`, the protrusion codes.
    pub fn has_pdftex_extensions(self) -> bool {
        matches!(self, Engine::PdfTeX | Engine::XeTeX | Engine::LuaTeX)
    }
    /// The Unicode-math `\U…` primitives and the random-number and
    /// `\primitive` extensions XeTeX and LuaTeX share.
    pub fn has_unicode_math(self) -> bool {
        matches!(self, Engine::XeTeX | Engine::LuaTeX)
    }
    pub fn has_luatex(self) -> bool {
        self == Engine::LuaTeX
    }
    pub fn has_xetex(self) -> bool {
        self == Engine::XeTeX
    }
    /// How this engine reads an input file.  `tex` and `pdftex` are 8-bit:
    /// they read bytes (tex.web § 232), so a two-byte UTF-8 character is two
    /// character tokens.  XeTeX and LuaTeX read UTF-8 characters.
    pub fn encoding(self) -> Encoding {
        match self {
            Engine::Tex | Engine::PdfTeX => Encoding::Bytes,
            Engine::LuaTeX | Engine::XeTeX => Encoding::Utf8,
        }
    }
}

/// What one input character is: one byte, or one UTF-8 character.
/// [`Engine::encoding`] picks it, and [`crate::source`] holds the choice for
/// the run.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Encoding {
    /// Every byte is its own character, code 0..=255.
    Bytes,
    #[default]
    Utf8,
}

named_enum! {
    /// Which kind of input a run analyzes, and so which `profiles.*` block in
    /// `satex.yaml` its sensible defaults come from: `document` (the default),
    /// `package` (`.sty`), `class` (`.cls`), `literate` (`.dtx`/`.ins`) or
    /// `plain` (plain TeX, no LaTeX). Chosen from the input's extension and,
    /// where that says nothing, what it starts with ([`Profile::detect`]);
    /// `--profile NAME` and `profile:` in `satex.yaml` override the automatic
    /// choice.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize)]
    #[serde(rename_all = "lowercase")]
    pub enum Profile {
        Document => "document",
        Package => "package",
        Class => "class",
        Literate => "literate",
        Plain => "plain",
    }
}

impl Profile {
    /// What `path`'s extension makes a file, `None` where it says nothing:
    /// a `.dtx`/`.fdd` is a class or a package by the `\Provides…` line of
    /// `code`, the text docstrip extracts (doc.dtx, "The driver").
    /// [`crate::machine::Machine::analyze`] reads a main file by this too.
    pub fn of_name(path: Option<&Path>, code: &str) -> Option<Profile> {
        match path?.extension()?.to_str()? {
            "sty" => Some(Profile::Package),
            "cls" => Some(Profile::Class),
            "ins" => Some(Profile::Literate),
            "dtx" | "fdd" if code.contains("\\ProvidesClass") => Some(Profile::Class),
            "dtx" | "fdd" => Some(Profile::Package),
            _ => None,
        }
    }

    /// The profile `path`/`source` implies: by [`Self::of_name`], else by
    /// what the source starts with — `\ProvidesClass`/`\ProvidesPackage` or
    /// `\documentclass`/`\documentstyle` at the first substantial line,
    /// else `document` if `\documentclass` or `\begin{document}` appears
    /// anywhere and `plain` if neither ever does.  `\begin` counts because
    /// plain TeX has no environments, so a file that opens one is LaTeX
    /// even when an `\input` file holds its `\documentclass`.
    pub fn detect(path: Option<&Path>, source: &str) -> Profile {
        let dtx = path.and_then(crate::literate::Literate::of) == Some(crate::literate::Literate::Dtx);
        let code = dtx.then(|| crate::literate::code_view(source, &crate::literate::Guards::AllButDriver));
        if let Some(profile) = Self::of_name(path, code.as_deref().unwrap_or(source)) {
            return profile;
        }
        for raw in source.lines().take(200) {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('%') {
                continue;
            }
            if line.contains("\\ProvidesClass") {
                return Profile::Class;
            }
            if line.contains("\\ProvidesPackage") {
                return Profile::Package;
            }
            if line.contains("\\documentclass") || line.contains("\\documentstyle") {
                return Profile::Document;
            }
        }
        if source.contains("\\documentclass") || source.contains("\\begin{document}") {
            Profile::Document
        } else {
            Profile::Plain
        }
    }
}

/// `profiles:` in `satex.yaml`: the partial config each [`Profile`] merges
/// over the general settings, the same way `merge_value` merges a layer.
/// An empty (or absent) block changes nothing.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Profiles {
    pub document: serde_yaml_ng::Value,
    pub package: serde_yaml_ng::Value,
    pub class: serde_yaml_ng::Value,
    pub literate: serde_yaml_ng::Value,
    pub plain: serde_yaml_ng::Value,
}

impl Profiles {
    pub fn get(&self, profile: Profile) -> &serde_yaml_ng::Value {
        match profile {
            Profile::Document => &self.document,
            Profile::Package => &self.package,
            Profile::Class => &self.class,
            Profile::Literate => &self.literate,
            Profile::Plain => &self.plain,
        }
    }
}

impl Default for Profiles {
    /// The built-in profile defaults, in force even with no `satex.yaml` at
    /// all: a package or class is read as its author sees it — `@` a
    /// letter, its own definitions not flagged for missing a use outside
    /// what it defines, and the lints that only make sense for a document
    /// with a build of its own to check turned off. A `.dtx`/`.ins` unpacks
    /// to one of those, so it gets the same defaults. Plain TeX has no
    /// LaTeX packages or classes to follow, and none of its own labels or
    /// microtype to suggest.
    fn default() -> Profiles {
        fn parse(yaml: &str) -> serde_yaml_ng::Value {
            serde_yaml_ng::from_str(yaml).expect("built-in profile default is valid YAML")
        }
        let package = parse(
            "at_letter: true\n\
             lints: {unused-definition: {report_public: false}}\n\
             lint_off: [analysis-imprecision, unused-label, microtype-available, preamble-cost, \
               comment-marker, \
               user-pattern, unreferenced-numbered-display, build-shell-escape-missing, \
             build-shell-escape-unneeded, build-engine-mismatch, build-bibliography-disabled, \
             build-bibliography-unneeded, build-missing-custom-dependency, \
             build-unused-custom-dependency, build-command-placeholders, build-engine-options]\n",
        );
        Profiles {
            document: serde_yaml_ng::Value::Null,
            package: package.clone(),
            class: package.clone(),
            literate: package,
            plain: parse(
                "load: {packages: false, classes: false}\nlint_off: [analysis-imprecision, unused-label, microtype-available, hand-set-quantity]\n",
            ),
        }
    }
}

/// `lints:` in `satex.yaml`: what one rule reads, under its own code.
/// `lint_off`, which every rule obeys, stays a general setting.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Lints {
    pub unused_definition: UnusedDefinition,
    pub wrong_dash: WrongDash,
    pub user_pattern: UserPattern,
    pub comment_marker: CommentMarker,
    pub build_cost: BuildCost,
    pub microtype_available: MicrotypeAvailable,
    pub ellipsis_dots: EllipsisDots,
}

/// `lints.build-cost:` in `satex.yaml`: when a part of the build is worth
/// a format (`preamble-cost`) or `\includeonly` (`includeonly-available`).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct BuildCost {
    /// Tokens below which a part reads in well under a second.
    pub tokens: u64,
    /// The share of the run's tokens the part must take.
    pub share: f64,
    /// Paragraph text the body must set for `\includeonly` to be worth a
    /// draft build's bother: a book, not a thesis that builds in seconds.
    pub characters: u64,
}

impl Default for BuildCost {
    fn default() -> BuildCost {
        BuildCost { tokens: 2_000_000, share: 0.5, characters: 500_000 }
    }
}

impl BuildCost {
    /// Whether `part` of `total` tokens is worth saving.
    pub fn costly(&self, part: u64, total: u64) -> bool {
        part >= self.tokens && part as f64 >= self.share * total as f64
    }
}

/// `lints.microtype-available:` in `satex.yaml`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct MicrotypeAvailable {
    /// Characters of justified paragraph text the body must set: about a page.
    pub characters: u64,
}

impl Default for MicrotypeAvailable {
    fn default() -> MicrotypeAvailable {
        MicrotypeAvailable { characters: 2500 }
    }
}

/// `lints.ellipsis-dots:` in `satex.yaml`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct EllipsisDots {
    /// chktex's `CenterDots`: what an ellipsis stands between when `\cdots` belongs.
    pub center: String,
}

impl Default for EllipsisDots {
    fn default() -> EllipsisDots {
        EllipsisDots { center: "=+-<>".into() }
    }
}

/// `lints.user-pattern:` in `satex.yaml`: chktex's `UserWarn` (warning 20)
/// and `UserWarnRegex` (warning 44).  Both empty, so the rule is silent
/// until the project fills one in.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UserPattern {
    /// Literal strings reported wherever the document's own text has them, as chktex's `UserWarn`.
    pub patterns: Vec<String>,
    /// Regular expressions reported the same way, as chktex's `UserWarnRegex`.
    pub regexes: Vec<String>,
}

/// `lints.comment-marker:` in `satex.yaml`: the markers in a comment worth
/// a finding.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CommentMarker {
    /// The markers in a comment worth a finding. This is satex's
    /// reading of chktex's warning 22, which prints every comment.
    pub markers: Vec<String>,
}

impl Default for CommentMarker {
    fn default() -> CommentMarker {
        CommentMarker { markers: ["TODO", "FIXME", "XXX", "HACK"].iter().map(|m| (*m).to_string()).collect() }
    }
}

/// `lints.wrong-dash:` in `satex.yaml`: how many hyphens in a row are
/// right in each context, as chktex's `HyphDash`, `NumDash` and `WordDash`
/// set them, and the spellings to leave alone (`DashExcpt`).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct WrongDash {
    /// How many hyphens in a row belong between letters, as chktex's
    /// `HyphDash`. The first entry is what a fix writes.
    pub letters: Vec<usize>,
    /// How many belong between numbers, as chktex's `NumDash`. Hyphens
    /// inside an identifier such as an ISBN or a date (another hyphen,
    /// a letter or a `/` in the same word) are left alone.
    pub digits: Vec<usize>,
    /// How many belong between spaces, as chktex's `WordDash`; two is
    /// Bringhurst's spaced en-dash.
    pub words: Vec<usize>,
    /// Whole spellings to leave alone, as chktex's `DashExcpt`.
    pub except: Vec<String>,
}

impl Default for WrongDash {
    fn default() -> WrongDash {
        WrongDash {
            letters: vec![1, 3],
            digits: vec![2],
            words: vec![3, 2],
            except: vec!["Birch--Swinnerton-Dyer".to_string()],
        }
    }
}

/// `lints.unused-definition:` in `satex.yaml`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UnusedDefinition {
    /// Whether the rule also reports a name with no `@` and no
    /// expl3-style `module_function:signature`. The package and class
    /// profiles turn it off, since such a name is the package's public
    /// interface, and the caller, not this run, is the one to use it.
    pub report_public: bool,
    /// Regexes on the name as written (`\\my@hook`). A match is never reported.
    pub ignore: Vec<String>,
    /// Whether the rule also reports a pgf key nothing uses. Off by
    /// default, since the user sets keys.
    pub pgf_keys: bool,
}

impl Default for UnusedDefinition {
    fn default() -> UnusedDefinition {
        UnusedDefinition { report_public: true, ignore: Vec::new(), pgf_keys: false }
    }
}

/// `paths:` in `satex.yaml`: the fallback value of each kpathsea search-path
/// variable ([`crate::paths::VARIABLES`]), used when neither the process
/// environment nor the project's `latexmkrc`/Makefile sets it.  Each is a
/// list of directories in kpathsea path syntax (a trailing `//` recurses),
/// overridable with `--set paths.texinputs=[…]`.
#[derive(Clone, Debug, Deserialize, Serialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct PathsConfig {
    pub texinputs: Vec<String>,
    pub bibinputs: Vec<String>,
    pub bstinputs: Vec<String>,
    pub tfmfonts: Vec<String>,
    pub encfonts: Vec<String>,
    /// Where the build writes its output (`-output-directory`): TeX opens a
    /// relative name there before searching.  Read from the build files first.
    pub output_directory: String,
}

impl PathsConfig {
    /// This config's fallback list for the kpathsea variable named
    /// `variable` (`"TEXINPUTS"`, …), or an empty slice when it is not one
    /// satex tracks.
    pub fn get(&self, variable: &str) -> &[String] {
        match variable {
            "TEXINPUTS" => &self.texinputs,
            "BIBINPUTS" => &self.bibinputs,
            "BSTINPUTS" => &self.bstinputs,
            "TFMFONTS" => &self.tfmfonts,
            "ENCFONTS" => &self.encfonts,
            _ => &[],
        }
    }
}

pub const FILE_NAME: &str = "satex.yaml";

/// `project:` in `satex.yaml`: where the project ends, and which of the
/// files it reads are someone else's.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Project {
    /// Globs of files satex still reads and analyzes, so their macros,
    /// counters and labels count, but reports nothing in and offers no
    /// fixes for, such as `["_config/**", "*.cls"]`. Each glob is
    /// relative to the `satex.yaml` that sets it, and `*` crosses
    /// directories.
    pub frozen: Vec<String>,
    /// Whether the files of every git repository other than the main
    /// file's, such as a submodule or a nested clone, are frozen too. A
    /// file belongs to the repository of its nearest directory with a
    /// `.git` in it. Such code is someone else's, and the document only
    /// uses it.
    pub freeze_other_repositories: bool,
    /// Whether the `satex.yaml` files above this one and the global one
    /// are ignored, like `root = true` in `.editorconfig`. `--config` and
    /// `--set` still apply.
    pub ignore_other_configs: bool,
}

impl Default for Project {
    fn default() -> Self {
        Project { frozen: Vec::new(), freeze_other_repositories: true, ignore_other_configs: false }
    }
}

/// Flat keys `satex.yaml` used before they were grouped into a block, and
/// where each now lives.  Read for one release still, with a warning on
/// stderr; `--set` accepts the old dotted name the same way
/// ([`migrate_set_path`]).
const LEGACY_KEYS: &[(&str, &[&str])] = &[
    ("load_packages", &["load", "packages"]),
    ("load_classes", &["load", "classes"]),
    ("load_inputs", &["load", "inputs"]),
    ("load_format", &["load", "format"]),
    ("skip_packages", &["load", "skip"]),
    ("texmf_roots", &["texmf", "roots"]),
    ("use_kpsewhich", &["texmf", "kpsewhich"]),
    ("use_fallback_roots", &["texmf", "fallback"]),
    ("texlive_root", &["texlive", "root"]),
    ("texlive_year", &["texlive", "year"]),
    ("cache_dir", &["cache", "dir"]),
    ("cache_index", &["cache", "index"]),
    ("rebuild_format", &["cache", "rebuild"]),
    ("log_gaps", &["gaps", "log"]),
    ("report_gaps", &["gaps", "report"]),
    ("gaps_log", &["gaps", "file"]),
];

/// Rewrites every legacy key [`LEGACY_KEYS`] names, and the old `cache:
/// <bool>` switch (now `cache.enabled`, since `cache:` itself became a
/// block), into its new nested place, in a raw YAML mapping read from
/// `context` (a file path, or `--set`) before it is deserialized into
/// [`Config`].  A key already spelled the new way is left alone.
fn migrate_legacy(value: &mut serde_yaml_ng::Value, context: &str) {
    use serde_yaml_ng::Value;
    let Value::Mapping(map) = value else { return };
    let cache_key = Value::String("cache".to_string());
    if matches!(map.get(&cache_key), Some(v) if !matches!(v, Value::Mapping(_))) {
        let old = map.remove(&cache_key).expect("just matched");
        eprintln!("{FILE_NAME}: {context}: cache is now cache.enabled");
        set_nested(map, &["cache", "enabled"], old);
    }
    // Both the plain key and its list-appending spelling (`skip_packages+:
    // […]`, `merge_value`'s module docs) move the same way, the `+` kept.
    for (old_key, new_path) in LEGACY_KEYS {
        for suffix in ["", "+"] {
            let key_name = format!("{old_key}{suffix}");
            let Some(old) = map.remove(Value::String(key_name.clone())) else { continue };
            let mut path: Vec<String> = new_path.iter().map(|s| (*s).to_string()).collect();
            *path.last_mut().expect("non-empty path") += suffix;
            eprintln!("{FILE_NAME}: {context}: {key_name} is now {}", path.join("."));
            let borrowed: Vec<&str> = path.iter().map(String::as_str).collect();
            set_nested(map, &borrowed, old);
        }
    }
}

/// Inserts `value` at `path` inside `map`, creating any mapping the path
/// passes through.  Where `path` already holds a mapping and `value` is one
/// too (`cache_index:` onto an already-present `cache.index:`), they merge
/// key by key rather than one replacing the other.
fn set_nested(map: &mut serde_yaml_ng::Mapping, path: &[&str], value: serde_yaml_ng::Value) {
    use serde_yaml_ng::Value;
    let (last, init) = path.split_last().expect("non-empty path");
    let mut slot = map;
    for key in init {
        let key = Value::String((*key).to_string());
        if !matches!(slot.get(&key), Some(Value::Mapping(_))) {
            slot.insert(key.clone(), Value::Mapping(Default::default()));
        }
        slot = slot.get_mut(&key).and_then(Value::as_mapping_mut).expect("just inserted a mapping");
    }
    let key = Value::String((*last).to_string());
    match (slot.get_mut(&key), &value) {
        (Some(existing @ Value::Mapping(_)), Value::Mapping(_)) => merge_value(existing, &value),
        _ => {
            slot.insert(key, value);
        }
    }
}

/// The dotted path `--set` should use for `path`, translating a leading
/// legacy key ([`LEGACY_KEYS`], and bare `cache` for `cache.enabled`) to
/// where it now lives; any other path is returned unchanged.
fn migrate_set_path(path: &str) -> String {
    let (head, rest) = path.split_once('.').map_or((path, None), |(h, r)| (h, Some(r)));
    if head == "cache" && rest.is_none() {
        return "cache.enabled".to_string();
    }
    match LEGACY_KEYS.iter().find(|(old, _)| *old == head) {
        Some((_, new_path)) => new_path.iter().copied().chain(rest).collect::<Vec<_>>().join("."),
        None => path.to_string(),
    }
}

/// Reads `path` as YAML, resolves its `<<:` merge keys, and rewrites any
/// legacy keys ([`migrate_legacy`]) it uses — the common first step of
/// [`Config::read`], [`Config::merge`] and [`merge_file`].
fn read_config_value(path: &Path) -> Result<serde_yaml_ng::Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut value: serde_yaml_ng::Value =
        serde_yaml_ng::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    value.apply_merge().map_err(|e| format!("{}: {e}", path.display()))?;
    migrate_legacy(&mut value, &path.display().to_string());
    Ok(value)
}

impl Config {
    pub fn distribution_request(&self) -> crate::distribution::Request {
        crate::distribution::Request {
            roots: self.texmf.roots.clone(),
            bundle: self.texmf.bundle.clone(),
            install_root: self.texlive.root.clone(),
            year: self.texlive.year,
            use_kpsewhich: self.texmf.kpsewhich,
            probe_fallback: self.texmf.fallback,
            provider: self.provider.or_else(|| self.texmf.kpsewhich.then(crate::plugin::Provider::detect).flatten()),
            platform: self.platform.unwrap_or_else(crate::plugin::Platform::current),
            cache: self.cache.enabled,
            cache_dir: self.cache.dir.clone(),
            // Filled in by the caller, which also has the `Project` this
            // needs to resolve `latexmkrc`/Makefile paths against.
            kpse_env: Vec::new(),
        }
    }
}

impl Config {
    pub fn read(path: &Path) -> Option<Config> {
        let mut cfg: Config = serde_yaml_ng::from_value(read_config_value(path).ok()?).ok()?;
        cfg.source = Some(path.to_path_buf());
        Some(cfg)
    }

    /// Overrides the option at `path`, the dotted key it has in `satex.yaml`
    /// (`limits.memory`, `cache.index.auto`), with `value` read as YAML.
    /// `path` may still be spelled the old, flat way (`cache_index.auto`),
    /// which is translated first, with a warning on stderr.
    pub fn set(&mut self, path: &str, value: &str) -> Result<(), String> {
        use serde_yaml_ng::Value;
        let migrated = migrate_set_path(path);
        if migrated != path {
            eprintln!("{FILE_NAME}: --set {path}: this is now --set {migrated}");
        }
        let parsed: Value = serde_yaml_ng::from_str(value).map_err(|e| format!("{migrated}: {e}"))?;
        let mut tree = serde_yaml_ng::to_value(&*self).map_err(|e| e.to_string())?;
        let mut slot = &mut tree;
        for key in migrated.split('.') {
            slot = slot
                .as_mapping_mut()
                .and_then(|map| map.get_mut(key))
                .ok_or_else(|| format!("no option `{migrated}` in {FILE_NAME}"))?;
        }
        *slot = parsed;
        let source = self.source.take();
        *self = serde_yaml_ng::from_value(tree).map_err(|e| format!("{migrated}: {e}"))?;
        self.source = source;
        Ok(())
    }

    /// [`Config::discover_layers`]'s config alone.
    pub fn discover(dir: &Path) -> Config {
        Self::discover_layers(dir).0
    }

    /// Every `satex.yaml` in effect for `dir`, merged in this order: the
    /// built-in defaults; the user's global config
    /// (`$XDG_CONFIG_HOME/satex/satex.yaml`, falling back to
    /// `~/.config/satex/satex.yaml`; `%APPDATA%\satex\satex.yaml` on
    /// Windows); then every `satex.yaml` from the filesystem root down to
    /// `dir`, outermost first, so a directory's own file wins over an
    /// ancestor's.  `--config` and `--set` (`main.rs`) merge on top of this.
    ///
    /// A map merges key by key, recursively; a list replaces the outer
    /// value entirely, unless the inner file spells the key with a
    /// trailing `+` (`skip_packages+: […]`), which appends to the outer
    /// list instead.  A file whose merged result does not fit `Config` (an
    /// unknown key, a value of the wrong type) is left out, the same as a
    /// single bad file was simply unreadable before — so one bad layer does
    /// not blank out the ones under it.
    ///
    /// Returns the merged config and the files that contributed to it, in
    /// merge order (`satex --version` lists them).
    pub fn discover_layers(dir: &Path) -> (Config, Vec<PathBuf>) {
        let mut value = serde_yaml_ng::to_value(Config::default()).unwrap_or(serde_yaml_ng::Value::Null);
        let mut files = Vec::new();
        // Innermost first, up to the first file that ends the cascade
        // (`project.ignore_other_configs: true`); the global file only when none does.
        let mut layers: Vec<PathBuf> = Vec::new();
        let mut rooted = false;
        for candidate in dir.ancestors().map(|a| a.join(FILE_NAME)).filter(|p| p.is_file()) {
            let stops = read_config_value(&candidate)
                .ok()
                .and_then(|v| v.get("project")?.get("ignore_other_configs")?.as_bool())
                == Some(true);
            layers.push(candidate);
            if stops {
                rooted = true;
                break;
            }
        }
        if !rooted && let Some(global) = global_config_file() {
            merge_file(&mut value, &global, &mut files, false);
        }
        for candidate in layers.into_iter().rev() {
            merge_file(&mut value, &candidate, &mut files, true);
        }
        let mut cfg: Config = serde_yaml_ng::from_value(value).unwrap_or_default();
        cfg.source = files.last().cloned();
        (cfg, files)
    }

    /// Merges `profiles.<profile>` from this config onto itself, the same
    /// rule `merge_value` merges a layer with, and records `profile` as
    /// the one in force. A profile with no block (`document`'s, by
    /// default) changes nothing but that.  Meant to run after the
    /// discovered `satex.yaml` layers and before `--config`/`--set`
    /// ([`crate::config`] module docs), so either of those still wins over
    /// what the profile set.
    pub fn apply_profile(&mut self, profile: Profile) -> Result<(), String> {
        let mut overlay = self.profiles.get(profile).clone();
        if !matches!(overlay, serde_yaml_ng::Value::Mapping(_)) {
            self.profile = Some(profile);
            return Ok(());
        }
        migrate_legacy(&mut overlay, &format!("profiles.{}", profile.as_str()));
        let mut value = serde_yaml_ng::to_value(&*self).map_err(|e| e.to_string())?;
        merge_value(&mut value, &overlay);
        let source = self.source.take();
        *self = serde_yaml_ng::from_value(value).map_err(|e| format!("profiles.{}: {e}", profile.as_str()))?;
        self.source = source;
        self.profile = Some(profile);
        Ok(())
    }

    /// Merges `path` on top of this config, the same rule
    /// [`Config::discover_layers`] merges each layer with.  Unlike a layer
    /// `discover_layers` finds on its own, a file named this way is meant
    /// to exist: `Err` when it cannot be read or does not fit `Config`.
    pub fn merge(&mut self, path: &Path) -> Result<(), String> {
        let incoming = read_config_value(path)?;
        let mut value = serde_yaml_ng::to_value(&*self).map_err(|e| e.to_string())?;
        merge_value(&mut value, &incoming);
        let source = self.source.take();
        *self = serde_yaml_ng::from_value(value).map_err(|e| format!("{}: {e}", path.display()))?;
        self.source = source.or_else(|| Some(path.to_path_buf()));
        Ok(())
    }
}

/// The user's own config, read after the built-in defaults and before any
/// project `satex.yaml` (`dirs::config_dir`: `$XDG_CONFIG_HOME` or
/// `~/.config` on Linux, `~/Library/Application Support` on macOS,
/// `%APPDATA%` on Windows).
fn global_config_file() -> Option<PathBuf> {
    crate::paths::config_dir().map(|dir| dir.join("satex").join(FILE_NAME))
}

/// Makes the `project.frozen` globs of the file at `path` absolute
/// against its directory.
fn anchor_frozen(value: &mut serde_yaml_ng::Value, path: &Path) {
    use serde_yaml_ng::Value;
    let dir = path.parent().map(|d| d.canonicalize().unwrap_or_else(|_| d.to_path_buf())).unwrap_or_default();
    let Some(Value::Mapping(map)) = value.get_mut("project") else { return };
    for key in ["frozen", "frozen+"] {
        if let Some(Value::Sequence(globs)) = map.get_mut(Value::String(key.to_string())) {
            for glob in globs {
                if let Value::String(text) = glob {
                    *text = dir.join(text.trim_start_matches("./")).to_string_lossy().into_owned();
                }
            }
        }
    }
}

/// What only the user's own configuration sets, never a project's.
const USER_KEYS: [(&str, &str); 2] = [("cache", "dir"), ("plugins", "packages")];

/// Merges `path` onto `base` with `merge_value`, and records it in
/// `files` — but only when the result still fits `Config`; a file that does
/// not (an unknown key, a value of the wrong type, unreadable, absent) is
/// left out and `base` is unchanged, so it never blanks out an outer layer.
fn merge_file(base: &mut serde_yaml_ng::Value, path: &Path, files: &mut Vec<PathBuf>, project: bool) {
    if !path.is_file() {
        return;
    }
    // A layer that does not read is left out, but never silently.
    let mut incoming = match read_config_value(path) {
        Ok(incoming) => incoming,
        Err(e) => return eprintln!("satex: {}: ignored: {e}", path.display()),
    };
    // A project, maybe someone else's, does not say where satex writes its
    // caches or whether a lint reaches the network: only the user's own
    // config, `--config`, `--set` and `SATEX_CACHE` do.
    if project {
        for (section, key) in USER_KEYS {
            if let Some(serde_yaml_ng::Value::Mapping(map)) = incoming.get_mut(section)
                && map.remove(key).is_some()
            {
                eprintln!("satex: {}: {section}.{key} ignored: only the user's own config sets it", path.display());
            }
        }
    }
    anchor_frozen(&mut incoming, path);
    let fits = |incoming: &serde_yaml_ng::Value| {
        let mut candidate = base.clone();
        merge_value(&mut candidate, incoming);
        serde_yaml_ng::from_value::<Config>(candidate.clone()).map(|_| candidate).map_err(|e| e.to_string())
    };
    // An unknown (or removed) key is dropped with a warning naming it; the
    // rest of the file applies.
    loop {
        let e = match fits(&incoming) {
            Ok(candidate) => {
                *base = candidate;
                return files.push(path.to_path_buf());
            }
            Err(e) => e,
        };
        let name = e.strip_prefix("unknown field `").and_then(|r| r.split('`').next()).map(str::to_string);
        let dropped = name.and_then(|name| {
            key_paths(&incoming, &name, &mut Vec::new()).into_iter().find_map(|at| {
                let mut without = incoming.clone();
                remove_key(&mut without, &at, &name);
                (fits(&without).err().as_ref() != Some(&e))
                    .then(|| (without, [at, vec![name.clone()]].concat().join(".")))
            })
        });
        let Some((without, key)) = dropped else { return eprintln!("satex: {}: ignored: {e}", path.display()) };
        eprintln!("satex: {}: warning: unknown key `{key}` ignored; the rest of the file applies", path.display());
        incoming = without;
    }
}

/// The paths of the mappings in `value` that hold the key `name`.
fn key_paths(value: &serde_yaml_ng::Value, name: &str, at: &mut Vec<String>) -> Vec<Vec<String>> {
    let serde_yaml_ng::Value::Mapping(map) = value else { return Vec::new() };
    let mut found = Vec::new();
    for (k, v) in map {
        let Some(k) = k.as_str() else { continue };
        if k == name {
            found.push(at.clone());
        }
        at.push(k.to_string());
        found.extend(key_paths(v, name, at));
        at.pop();
    }
    found
}

fn remove_key(value: &mut serde_yaml_ng::Value, at: &[String], name: &str) {
    let mut v = value;
    for k in at {
        let Some(next) = v.get_mut(k.as_str()) else { return };
        v = next;
    }
    if let serde_yaml_ng::Value::Mapping(map) = v {
        map.remove(name);
    }
}

/// Merges `incoming` onto `base`: a mapping merges key by key, recursively;
/// anything else (a scalar, a sequence) replaces `base` outright.  A
/// mapping key spelled with a trailing `+` appends its (sequence) value to
/// the same-named sequence already in `base`, instead of replacing it.
fn merge_value(base: &mut serde_yaml_ng::Value, incoming: &serde_yaml_ng::Value) {
    use serde_yaml_ng::Value;
    let (Value::Mapping(base_map), Value::Mapping(incoming_map)) = (&mut *base, incoming) else {
        *base = incoming.clone();
        return;
    };
    for (key, value) in incoming_map {
        if let Value::String(name) = key
            && let Some(stripped) = name.strip_suffix('+')
        {
            let target = Value::String(stripped.to_string());
            if let (Some(Value::Sequence(existing)), Value::Sequence(added)) = (base_map.get_mut(&target), value) {
                existing.extend(added.clone());
            } else {
                base_map.insert(target, value.clone());
            }
            continue;
        }
        match base_map.get_mut(key) {
            Some(existing) => merge_value(existing, value),
            None => {
                base_map.insert(key.clone(), value.clone());
            }
        }
    }
}

/// Whether `path` is the file `name` names: a trailing part of the path,
/// with or without the `.tex` extension.
pub fn names_file(path: &str, name: &str) -> bool {
    let path = std::path::Path::new(path);
    path.ends_with(name) || path.ends_with(format!("{name}.tex"))
}

/// Every dotted key path `satex.yaml` takes, with its default, read off the defaults.
pub fn keys() -> serde_json::Map<String, serde_json::Value> {
    fn walk(prefix: &str, v: &serde_json::Value, out: &mut serde_json::Map<String, serde_json::Value>) {
        match v {
            serde_json::Value::Object(m) if !m.is_empty() => {
                for (k, v) in m {
                    walk(&if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") }, v, out);
                }
            }
            _ => drop(out.insert(prefix.to_string(), v.clone())),
        }
    }
    let mut out = serde_json::Map::new();
    walk("", &serde_json::to_value(Config::default()).unwrap_or_default(), &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The repo's own `satex.yaml` lists every `limits:` value as documentation,
    /// but the number a run actually gets when none is set is
    /// [`Limits::default`]; this checks the two never drift apart, so there is
    /// one source of truth for what a limit defaults to.
    #[test]
    fn the_repo_satex_yaml_lists_the_same_limits_as_limits_default() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(FILE_NAME);
        let cfg = Config::read(&path).expect("repo satex.yaml reads as a Config");
        assert_eq!(cfg.limits, Limits::default());
    }

    #[test]
    fn an_option_is_set_by_its_path_in_the_file() {
        let mut cfg = Config::default();
        cfg.set("limits.trace", "10").unwrap();
        cfg.set("load.inputs", "false").unwrap();
        assert_eq!(cfg.limits.trace, 10);
        assert!(!cfg.load.inputs);
        assert!(cfg.set("limits.bogus", "1").is_err());
        assert!(cfg.set("limits.trace", "many").is_err());
    }

    #[test]
    fn degrade_at_percent_is_a_percentage() {
        let mut cfg = Config::default();
        assert_eq!(cfg.limits.degrade_at_percent, 100);
        cfg.set("limits.degrade_at_percent", "70").unwrap();
        assert_eq!(cfg.limits.degrade_at_percent, 70);
        assert!(cfg.set("limits.degrade_at_percent", "120").is_err());
        assert_eq!(cfg.limits.degrade_at_percent, 70);
    }

    #[test]
    fn set_still_accepts_the_old_flat_path() {
        let mut cfg = Config::default();
        cfg.set("load_packages", "false").unwrap();
        cfg.set("cache_index.auto", "false").unwrap();
        cfg.set("cache", "false").unwrap();
        assert!(!cfg.load.packages);
        assert!(!cfg.cache.index.auto);
        assert!(!cfg.cache.enabled);
    }

    fn write(dir: &Path, name: &str, text: &str) {
        std::fs::write(dir.join(name), text).unwrap();
    }

    #[test]
    fn ancestor_layers_merge_outer_first_and_a_list_replaces() {
        let dir = std::env::temp_dir().join("satex-config-merge-test");
        let inner = dir.join("project");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&inner).unwrap();
        // Old-style flat keys, to check they still load with the same effect.
        write(&dir, "satex.yaml", "load_packages: false\nskip_packages: [a]\n");
        write(&inner, "satex.yaml", "load_classes: false\nskip_packages: [b]\n");
        let (cfg, files) = Config::discover_layers(&inner);
        // The inner file wins where both set a key, and inherits what only
        // the outer one set; a plain list replaces rather than merging.
        assert!(!cfg.load.packages);
        assert!(!cfg.load.classes);
        assert_eq!(cfg.load.skip, vec!["b".to_string()]);
        assert!(files.ends_with(&[dir.join("satex.yaml"), inner.join("satex.yaml")]));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_project_does_not_choose_the_cache_or_reach_the_network() {
        let dir = std::env::temp_dir().join(format!("satex-config-user-keys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        write(
            &dir,
            "satex.yaml",
            "project: {ignore_other_configs: true}\ncache: {dir: ./c, enabled: false}\nplugins: {packages: true}\n",
        );
        let (cfg, _) = Config::discover_layers(&dir);
        assert_eq!(cfg.cache.dir, None);
        assert!(!cfg.plugins.packages);
        // The rest of the file applies.
        assert!(!cfg.cache.enabled);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unknown_key_is_dropped_and_the_rest_applies() {
        let dir = std::env::temp_dir().join("satex-config-unknown-key-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        write(
            &dir,
            "satex.yaml",
            "limits: {stall_tokens: 5, widen_after: 7, region_share: 3}\nload: {classes: false}\n",
        );
        let (cfg, files) = Config::discover_layers(&dir);
        assert_eq!(cfg.limits.widen_after, 7);
        assert!(!cfg.load.classes);
        assert_eq!(files.last(), Some(&dir.join("satex.yaml")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn new_style_nested_keys_load_the_same_way() {
        let dir = std::env::temp_dir().join("satex-config-merge-new-style-test");
        let inner = dir.join("project");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&inner).unwrap();
        write(&dir, "satex.yaml", "load: {packages: false}\ntexlive: {root: /opt/texlive, year: 2026}\n");
        write(&inner, "satex.yaml", "load: {classes: false}\n");
        let (cfg, _) = Config::discover_layers(&inner);
        assert!(!cfg.load.packages);
        assert!(!cfg.load.classes);
        assert_eq!(cfg.texlive.root, Some(PathBuf::from("/opt/texlive")));
        assert_eq!(cfg.texlive.year, Some(2026));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_trailing_plus_appends_to_the_outer_list() {
        let dir = std::env::temp_dir().join("satex-config-merge-append-test");
        let inner = dir.join("project");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&inner).unwrap();
        write(&dir, "satex.yaml", "skip_packages: [a]\n");
        write(&inner, "satex.yaml", "skip_packages+: [b]\n");
        let (cfg, _) = Config::discover_layers(&inner);
        assert_eq!(cfg.load.skip, vec!["a".to_string(), "b".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_profile_can_extend_another_with_merge_keys() {
        let dir = std::env::temp_dir().join("satex-config-profile-extends-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        write(
            &dir,
            "satex.yaml",
            "profiles:\n  document: &library\n    at_letter: true\n    lint_off: [unused-label]\n  package:\n    <<: *library\n  class:\n    <<: *library\n    lint_off: [unused-label, microtype-available]\n",
        );
        let (mut cfg, _) = Config::discover_layers(&dir);
        cfg.apply_profile(Profile::Package).unwrap();
        assert!(cfg.at_letter);
        assert_eq!(cfg.lint_off, vec!["unused-label".to_string()]);
        let (mut cfg, _) = Config::discover_layers(&dir);
        cfg.apply_profile(Profile::Class).unwrap();
        assert!(cfg.at_letter);
        assert_eq!(cfg.lint_off, vec!["unused-label".to_string(), "microtype-available".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_layer_that_does_not_fit_config_is_skipped_without_blanking_the_outer_one() {
        let dir = std::env::temp_dir().join("satex-config-merge-invalid-test");
        let inner = dir.join("project");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&inner).unwrap();
        write(&dir, "satex.yaml", "load_packages: false\n");
        write(&inner, "satex.yaml", "load: {classes: [1]}\n");
        let (cfg, files) = Config::discover_layers(&inner);
        assert!(!cfg.load.packages);
        assert!(files.ends_with(&[dir.join("satex.yaml")]));
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn ignore_other_configs_makes_a_file_the_root() {
        let dir = std::env::temp_dir().join(format!("satex-cascade-{}", std::process::id()));
        let inner = dir.join("inner");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(dir.join(FILE_NAME), "load: {packages: false}\n").unwrap();
        std::fs::write(inner.join(FILE_NAME), "project: {ignore_other_configs: true}\nload: {classes: false}\n")
            .unwrap();
        let (cfg, files) = Config::discover_layers(&inner);
        assert_eq!(files, vec![inner.join(FILE_NAME)]);
        assert!(cfg.load.packages, "the file above the root is not read");
        assert!(!cfg.load.classes);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
