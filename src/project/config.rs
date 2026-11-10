use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// `opaque_conditionals` entry standing for every switch the document's
/// own files declare with `\newif` and the like.
pub const EVERY_SWITCH: &str = "*";

/// What bounds the analysis: reaching one stops that part of the run and
/// records an [`Imprecision`](crate::analysis::facts::Severity::Imprecision), so no
/// input can make satex loop or run out of memory.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    /// Tokens the document may digest, and the kernel on top of that.
    pub steps: u64,
    /// Tokens one trial run of a command may read when a hover learns what it does: a sectioning
    /// command takes a few thousand, hyperref's `\GetTitleString` a million. Eight runs answer one
    /// question at most.
    pub probe_steps: u64,
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
    /// unknown token stands for the rest, and the line reads on. Every later
    /// overrun skips the rest of the line; the run stops only with no line
    /// left to skip to. The termination guard. 0 turns it off.
    pub line_steps: u64,
    /// [`Limits::line_steps`] once the document's body has begun, where a line
    /// typesets rather than loads. 0: the same.
    pub text_line_steps: u64,
    /// `site_forks` inside a value scan, as a factor: its arms join where the scan ends.
    pub scan_site_factor: u32,
    /// How many nested calls are recorded to be learned from or summarized.
    pub call_rec_depth: u16,
    /// Choices in output routines a run parts paths for before
    /// `analysis.assume_page_one_arm` applies.
    pub output_forks: u64,
    /// Overruns of one line's budget before the run stops: the hard bound of the guard.
    pub line_overruns: u32,
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
    /// How many undecided `\if` one command may split into paths. Past them
    /// the command's result is unknown text, reported as an `assumption`.
    /// Higher keeps more combinations apart and costs time.
    pub choices: u16,
    /// How many paths one place in a source line may start, counted over all
    /// paths. Past them an `\if` there is no longer followed both ways: the
    /// run takes the effects either branch may have (`site-forks-widened`).
    /// Lower finishes a `\foreach` with unknown bounds sooner but knows less.
    /// 0 turns it off.
    pub site_forks: u32,
    /// How many digit counts a number of unknown digits is read apart into,
    /// one path each, when TeX reads it digit by digit (`\the\count` of
    /// 0..214: one, two, three digits). Past them the longer counts are
    /// one path whose count stays open. Higher is more precise where code
    /// compares or prints the count, and forks more for wide ranges
    /// (`\int_rand:n {16777215}`).
    pub digit_lengths: u8,
    /// How many alternative paths satex keeps at once. Past them the two
    /// oldest at one place are merged into one. Higher is more precise,
    /// slower and takes more memory.
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
    /// How often one command may split the run per meaning of a name that
    /// has several, such as `\x` defined differently in two branches. Past
    /// them, and with 0, the name reads as unknown.
    pub meaning_splits: u16,
    /// How many rounds a loop is followed exactly before values that still
    /// change become ranges (SEMANTICS § 14). A `\loop` counting to an
    /// unknown bound has its counter as a range after these rounds.
    pub widen_after: u16,
    /// How often calls at one place may enter a loop afresh before every further entry counts as a
    /// pass of it, so the loop is summarized: sibling calls on one line (`\int_to_Hex:n` seven
    /// times in an `\edef`) stay exact up to this.
    pub loop_entries: u16,
    /// How many runs of the build (latexmk's rounds) the analysis takes to
    /// reach the files' fixpoint; past them what still changes is unknown.
    /// 1 runs once, with no file of an earlier run.
    pub build_runs: u16,
    /// How deep a macro that calls itself while an `\if` is undecided is
    /// followed before the recursion is assumed to end there.
    pub recursion_depth: u16,
    /// How many loops one run remembers with their merged states. Past
    /// them the oldest is forgotten, and summarized when it comes back.
    pub loop_heads: u16,
    /// After this many runs of one call at one place, a call with unknown
    /// arguments reuses their merged result instead of running again.
    /// `\pgfmathparse{\x}` with an unknown `\x` is then one step. 0 never does.
    pub call_summary_after: u16,
    /// How often a macro that peels unknown text may split one rest it put
    /// back at a delimiter before the rest counts as holding no more.
    pub rest_splits: u16,
    /// How many runs on different arguments teach what a definition's
    /// calls leave, with `analysis.assume_call_learned_effect`. 0 never.
    pub call_learn_after: u16,
    /// How many undecided choices a run of a definition that was taught
    /// may meet before it ends with the learned effect.
    pub call_learn_forks: u32,
    /// How deep code a join made unknown is followed into the macros it
    /// calls to find what it may assign. Past it the run reports a gap.
    pub may_run_depth: u8,
    /// How many times in a row one call may meet the state it met before,
    /// with the same arguments, input, nesting and bindings, before its
    /// path is ended (the only path is summarized instead): TeX repeats
    /// that state forever; 0 never ends one.
    pub cycle_after: u16,
    /// How many possible meanings a name keeps after branches merge. Past
    /// them its meaning is unknown.
    pub meaning_set: u16,
    /// How many arms a join of paths keeps paired ([`crate::engine::env::Corr`]):
    /// which binding of one name goes with which of another.
    pub join_arms: u16,
    /// How many names a join pairs at most: past them the rest are joined
    /// apart, as unpaired names are.
    pub join_names: u16,
    /// How many control sequences typed unknown text names: a widened
    /// list of names (`\@elt\bx@A…`) keeps which it may hold. Past them
    /// it may hold any control sequence.
    pub text_names: u16,
    /// How many exact values a count or length keeps after branches merge.
    /// Past them only its range is kept, so `{1, 5}` becomes `1..5`. Also
    /// the places an `\openin` stream keeps apart (texts and their lines)
    /// before it is unknown.
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
    /// Dev mode: the most entries each list of a step's state holds.
    pub trace_state: usize,
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
            "text_line_steps" => self.text_line_steps,
            "scan_site_factor" => self.scan_site_factor.into(),
            "call_rec_depth" => self.call_rec_depth.into(),
            "output_forks" => self.output_forks,
            "line_overruns" => self.line_overruns.into(),
            "probe_steps" => self.probe_steps,
            "replay_steps" => self.replay_steps,
            "repeat_failures" => self.repeat_failures.into(),
            "seconds" => self.seconds,
            "choices" => self.choices.into(),
            "site_forks" => self.site_forks.into(),
            "digit_lengths" => self.digit_lengths.into(),
            "paths" => self.paths.into(),
            "alike" => self.alike.into(),
            "heads_apart" => self.heads_apart.into(),
            "widen_after" => self.widen_after.into(),
            "loop_entries" => self.loop_entries.into(),
            "recursion_depth" => self.recursion_depth.into(),
            "loop_heads" => self.loop_heads.into(),
            "call_summary_after" => self.call_summary_after.into(),
            "rest_splits" => self.rest_splits.into(),
            "call_learn_after" => self.call_learn_after.into(),
            "call_learn_forks" => self.call_learn_forks.into(),
            "may_run_depth" => self.may_run_depth.into(),
            "cycle_after" => self.cycle_after.into(),
            "degrade_at_percent" => self.degrade_at_percent.into(),
            "expansion_tokens" => self.expansion_tokens as u64,
            "conditional_tokens" => self.conditional_tokens as u64,
            "held_tokens" => self.held_tokens as u64,
            "expansion_stack" => self.expansion_stack as u64,
            "vertices" => self.vertices as u64,
            "facts" => self.facts as u64,
            "trace" => self.trace as u64,
            "trace_state" => self.trace_state as u64,
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
            probe_steps: 2_000_000,
            // The real expl3 reads the Unicode data files while the kernel
            // is built; that is done once and then cached.
            format_steps: 9_000_000_000,
            file_depth: 64,
            expansion_depth: 512,
            // Measured: kernel helpers (`\hexnumber@`, expl3 mappings) and
            // pgfkeys over a long `\tikzset` list re-expand one site this often
            // without looping.
            site_expansions: 16_384,
            // Measured: the heaviest line of a long thesis takes 34-67M
            // steps (about 30 s); a stuck line costs at most 3 times this.
            line_steps: 100_000_000,
            text_line_steps: 0,
            call_rec_depth: 64,
            scan_site_factor: 16,
            output_forks: 0,
            line_overruns: 16,
            // Measured: a `\newlabel` or `\@writefile` line takes a few
            // thousand steps; one of unknown text may take any number.
            replay_steps: 50_000,
            // Measured: the kernel's own `\if` chains report a handful of
            // the same gap while they are still making progress, and no
            // document needed more than a few before the path repeated
            // itself.
            repeat_failures: 8,
            seconds: 300,
            choices: 8,
            site_forks: 64,
            // Measured: `\the` of a count below 1000 reads apart into one,
            // two and three digits (pgfmath's octal path, a false error
            // when merged); a 24-bit random number into eight would fork
            // past `site_forks`.
            digit_lengths: 3,
            paths: 16,
            alike: 8,
            heads_apart: 32,
            meaning_splits: 4,
            widen_after: 3,
            loop_entries: 256,
            build_runs: 5,
            recursion_depth: 100,
            loop_heads: 512,
            call_summary_after: 2,
            rest_splits: 8,
            call_learn_after: 2,
            call_learn_forks: 2,
            may_run_depth: 3,
            cycle_after: 2,
            meaning_set: 8,
            join_arms: 32,
            join_names: 64,
            text_names: 64,
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
            trace_state: 32,
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
/// output that make up [`crate::plugins::Plugins`] are always resolved; these
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
    /// Reading the packages' own manuals for what their commands do, in the
    /// background, for hovers.
    pub docs: bool,
}

impl Default for PluginSwitches {
    fn default() -> PluginSwitches {
        PluginSwitches { magic: true, tools: true, preload: true, discovery: true, packages: false, docs: true }
    }
}

/// `lints.style-prompt:` in `satex.yaml`: when an editor asks whether a rule
/// that fires often in the project is its style, to turn that rule off.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct StylePrompt {
    pub enabled: bool,
    /// Findings of one rule in the project's own files before it is asked about.
    pub min_findings: usize,
    /// The rule categories a project may call its style.
    pub categories: Vec<String>,
    /// Rules the project keeps and is never asked about again.
    pub keep: Vec<String>,
}

impl Default for StylePrompt {
    fn default() -> StylePrompt {
        StylePrompt { enabled: true, min_findings: 10, categories: vec!["style".into()], keep: Vec::new() }
    }
}

/// What a project answered when asked whether a rule is its style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StyleChoice {
    /// The rule goes into `lint_off`.
    Off,
    /// The rule goes into `lints.style-prompt.keep`.
    Keep,
}

/// Records `choice` for `code` in `dir`'s `satex.yaml`, made if missing.
/// The text is edited in place, so the file's comments stay.
pub fn record_style_choice(dir: &Path, code: &str, choice: StyleChoice) -> Result<PathBuf, String> {
    let path = dir.join(FILE_NAME);
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let text = match choice {
        StyleChoice::Off => with_listed(&text, &[], "lint_off", code),
        StyleChoice::Keep => with_listed(&text, &["lints", "style-prompt"], "keep", code),
    }
    .ok_or_else(|| format!("{}: no flow list to add `{code}` to", path.display()))?;
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// `text` with `item` in the flow list of `key` (or `key+`), under the
/// nested mappings `path` (outermost first, maybe empty). A missing
/// mapping or list is added, the list as `key+`, so it extends what other
/// layers set. `None` when the key is there but not as a one-line `[…]`
/// list, or a mapping on the path is written inline.
fn with_listed(text: &str, path: &[&str], key: &str, item: &str) -> Option<String> {
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let (mut from, mut end, mut indent) = (0, lines.len(), String::new());
    // `name:` at `indent`, its value empty or a comment.
    let header = |l: &str, name: &str, indent: &str| {
        l.strip_prefix(indent).and_then(|r| r.strip_prefix(name)).and_then(|r| r.strip_prefix(':')).map(|r| {
            let r = r.trim();
            r.is_empty() || r.starts_with('#')
        })
    };
    let deeper = |l: &str, indent: &str| l.trim().is_empty() || l.starts_with(&format!("{indent} "));
    for name in path {
        match (from..end).find(|&i| lines[i].strip_prefix(&indent).is_some_and(|r| r.starts_with(&format!("{name}:"))))
        {
            Some(at) if header(&lines[at], name, &indent) == Some(true) => {
                let sub_end = (at + 1..end).find(|&i| !deeper(&lines[i], &indent)).unwrap_or(end);
                let child =
                    lines[at + 1..sub_end].iter().find(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'));
                let child = child.map_or(format!("{indent}  "), |l| l[..l.len() - l.trim_start().len()].to_string());
                (from, end, indent) = (at + 1, sub_end, child);
            }
            Some(_) => return None,
            None => {
                lines.insert(end, format!("{indent}{name}:"));
                (from, end, indent) = (end + 1, end + 1, format!("{indent}  "));
            }
        }
    }
    while end > from && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    let found = (from..end).find(|&i| {
        let rest = lines[i].strip_prefix(&indent).unwrap_or_default();
        !rest.starts_with(' ') && (rest.starts_with(&format!("{key}:")) || rest.starts_with(&format!("{key}+:")))
    });
    match found {
        Some(i) => {
            let line = &lines[i];
            let (open, close) = (line.find('[')?, line.find(']')?);
            let listed = line[open + 1..close].split(',').map(str::trim).any(|c| c == item);
            if !listed {
                let sep = if line[open + 1..close].trim().is_empty() { "" } else { ", " };
                lines[i] = format!("{}{sep}{item}{}", line[..close].trim_end(), &line[close..]);
            }
        }
        None => lines.insert(end, format!("{indent}{key}+: [{item}]")),
    }
    Some(lines.join("\n") + "\n")
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

/// `type_plugins:` in `satex.yaml`: the editor features each type plugin
/// ([`crate::lsp::plugins`]) adds where text of its type stands.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TypePlugins {
    pub keys: KeysPlugin,
    pub units: UnitsPlugin,
    pub color: ColorPlugin,
    pub language: LanguagePlugin,
    pub listing: Switch,
    pub environment: Switch,
    pub table: Switch,
    pub image: Switch,
    pub float: Switch,
    pub tikz: TikzPlugin,
    pub forest: Switch,
    pub glossary: Switch,
    pub bibliography: Switch,
    pub index: Switch,
    pub theorem: Switch,
    pub savebox: Switch,
    pub history: HistoryPlugin,
    pub pretty: PrettyPlugin,
    pub overlay: Switch,
    pub frame: Switch,
    pub page: Switch,
    pub comment_edit: Switch,
    pub whitespace_edit: Switch,
    pub typing: TypingPlugin,
}

/// `type_plugins.typing:` letters typed into a control word the run had undefined.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TypingPlugin {
    pub enabled: bool,
    /// How long typing pauses before the run follows the provisional results.
    pub pause_ms: u64,
}

impl Default for TypingPlugin {
    fn default() -> Self {
        Self { enabled: true, pause_ms: 1500 }
    }
}

/// `l3build:` in `satex.yaml`: how an l3build bundle's configuration is read
/// ([`crate::plugins::l3build`]). Empty values keep what the bundle's own files say.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct L3buildConfig {
    pub enabled: bool,
    /// The build files looked for, in this order, in the directory and those above.
    pub files: Vec<String>,
    /// Lua files read after the build file, as `l3build --config` names them.
    pub config: Vec<String>,
    /// The module's name, instead of the build file's `module`.
    pub module: Option<String>,
    /// The test files in `testfiledir`, as patterns.
    pub tests: Vec<String>,
    /// The engines that check, instead of the build file's `checkengines`.
    pub engines: Vec<String>,
    /// Analyze the regression tests as documents of the bundle too.
    pub checks: bool,
}

impl Default for L3buildConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            files: vec!["build.lua".into()],
            config: Vec::new(),
            module: None,
            tests: vec!["*.lvt".into(), "*.pvt".into()],
            engines: Vec::new(),
            checks: false,
        }
    }
}

/// `type_plugins.history:` a command's definition history.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct HistoryPlugin {
    pub enabled: bool,
    /// The events a history lists, newest first, and the calls of its final body it opens.
    pub entries: usize,
}

impl Default for HistoryPlugin {
    fn default() -> Self {
        Self { enabled: true, entries: 50 }
    }
}

/// `type_plugins.pretty:` macro bodies on hover, one statement per line.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PrettyPlugin {
    pub enabled: bool,
    /// The characters a line holds before a statement or group breaks over lines.
    pub width: usize,
    /// The lines a hover shows of a body before it stops.
    pub lines: usize,
}

impl Default for PrettyPlugin {
    fn default() -> Self {
        Self { enabled: true, width: 60, lines: 40 }
    }
}

/// A plugin with nothing to tune: `enabled` only.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Switch {
    pub enabled: bool,
}

impl Default for Switch {
    fn default() -> Self {
        Self { enabled: true }
    }
}

/// `type_plugins.keys:` key lists (pgfkeys, l3keys, kvoptions, package options).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct KeysPlugin {
    pub enabled: bool,
    /// After a family's name, the completion goes on with its keys.
    pub family_continue: bool,
}

impl Default for KeysPlugin {
    fn default() -> Self {
        Self { enabled: true, family_continue: true }
    }
}

/// `type_plugins.units:` dimensions (`12pt`, `\textwidth`).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct UnitsPlugin {
    pub enabled: bool,
    /// The units a hover shows a length in, and the conversions offered.
    pub units: Vec<String>,
    /// Decimals a converted length is written with.
    pub decimals: u8,
}

impl Default for UnitsPlugin {
    fn default() -> Self {
        Self { enabled: true, units: ["pt", "mm", "cm", "in", "bp"].map(String::from).to_vec(), decimals: 2 }
    }
}

/// `type_plugins.color:` color names and color expressions.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ColorPlugin {
    pub enabled: bool,
    /// A swatch next to each color a color command uses.
    pub swatches: bool,
    /// The name a color the picker defines starts with.
    pub name: String,
}

impl Default for ColorPlugin {
    fn default() -> Self {
        Self { enabled: true, swatches: true, name: "mycolor".into() }
    }
}

/// `type_plugins.language:` code languages (listings' `language=`, minted's lexers).
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct LanguagePlugin {
    pub enabled: bool,
    /// The program whose `-L lexers` lists minted's languages, read once; empty: the seed list.
    pub pygmentize: String,
}

impl Default for LanguagePlugin {
    fn default() -> Self {
        Self { enabled: true, pygmentize: "pygmentize".into() }
    }
}

/// `type_plugins.tikz:` the drawing dialog of a `tikzpicture`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TikzPlugin {
    pub enabled: bool,
    /// The grid a dragged or added node snaps to, in cm.
    pub grid: f64,
    /// The distance between the ranks of an imported Mermaid flowchart, in cm.
    pub spacing: f64,
    /// Whether a moved or new node is written relative to a neighbour (`right=1cm of a`) rather
    /// than `at (x,y)`; each viewer can change it on the canvas.
    pub relative: bool,
}

impl Default for TikzPlugin {
    fn default() -> Self {
        Self { enabled: true, grid: 0.5, spacing: 2.0, relative: false }
    }
}

/// `slice:` in `satex.yaml`: what a reconstructed slice leaves out.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct SliceConfig {
    /// Keeps only what the criterion depends on: what the document's frame
    /// (`\begin{document}`, its hooks, the `.aux` file) alone reads stays
    /// out, so unused packages, libraries and numbering go.
    pub trim: bool,
    /// Makes the slice one file that compiles in an empty directory: local
    /// files it reads go in as `filecontents*`, graphics as `example-image`.
    pub standalone: bool,
}

impl Default for SliceConfig {
    fn default() -> Self {
        Self { trim: true, standalone: true }
    }
}

/// `playground:` in `satex.yaml`: what only the web playground reads.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PlaygroundConfig {
    /// A button line above each widget's target besides its gutter mark.
    pub lenses: bool,
}

/// `docs:` in `satex.yaml`: where the documentation of a package is looked for,
/// and which conventions of it are read ([`crate::plugins::docs`]).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct DocsConfig {
    /// The folder of a TDS tree that holds the documented sources.
    pub source_root: String,
    /// The folder of a TDS tree that holds the manuals.
    pub doc_root: String,
    /// The extensions of the files mined.
    pub extensions: Vec<String>,
    /// The commands (with their backslash) with which a manual reads another file.
    pub reads: Vec<String>,
    /// How deep a manual's reads are followed.
    pub depth: usize,
    /// How many lines after an `ltxdoc` anchor its usage line may stand.
    pub usage_lines: usize,
    /// The commands (without their backslash) that announce the documentation of a command or environment, as `\DescribeMacro{\x}` does.
    pub describe: Vec<String>,
    /// The `ltxdoc` environments that document a command's code, `macro` in `\begin{macro}{\x}`.
    pub ltxdoc_environments: Vec<String>,
    /// The environments whose argument is a command or environment and its usage, `\begin{command}{\x\marg{a}}` (beamer's manual).
    pub usage_environments: Vec<String>,
    /// The `l3doc` environments that document a function or variable.
    pub l3doc_environments: Vec<String>,
    /// Also read the package's own `.sty` comments. Not supported yet: it changes nothing.
    pub inline: bool,
}

impl Default for DocsConfig {
    fn default() -> DocsConfig {
        let list = |items: &[&str]| items.iter().map(|s| s.to_string()).collect();
        DocsConfig {
            source_root: "source".into(),
            doc_root: "doc".into(),
            extensions: list(&["dtx", "tex", "satex-docs"]),
            reads: list(&["\\DocInput", "\\DocInclude", "\\input", "\\include"]),
            depth: 3,
            usage_lines: 12,
            describe: list(&["DescribeMacro", "DescribeEnv", "cmditem"]),
            usage_environments: list(&["command", "environment", "key"]),
            ltxdoc_environments: list(&["macro"]),
            l3doc_environments: list(&["function", "variable"]),
            inline: false,
        }
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

named_enum! {
    /// How the engine was started with respect to `\write18` (TeX Live manual).
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize, Default)]
    #[serde(rename_all = "lowercase")]
    pub enum ShellEscape {
        /// `--no-shell-escape`: nothing runs.
        None => "none",
        /// `--shell-restricted`: only the programs the installation's
        /// `shell_escape_commands` names (`kpsewhich -var-value`).
        #[default]
        Restricted => "restricted",
        /// `--shell-escape`: anything runs.
        Full => "full",
    }
}

impl ShellEscape {
    /// pdfTeX's `\shellescape` and LuaTeX's `status.shell_escape`.
    pub fn level(self) -> i64 {
        match self {
            ShellEscape::None => 0,
            ShellEscape::Full => 1,
            ShellEscape::Restricted => 2,
        }
    }
}

named_enum! {
    /// The engine's `-interaction` option (tex.web § 73): `\batchmode` = 0 …
    /// `\errorstopmode` = 3.  Builds run unattended, so nothing is typed at the
    /// terminal. The default is `nonstopmode`.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize, Default)]
    #[serde(rename_all = "lowercase")]
    pub enum Interaction {
        Batchmode => "batchmode",
        #[default]
        Nonstopmode => "nonstopmode",
        Scrollmode => "scrollmode",
        Errorstopmode => "errorstopmode",
    }
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
    /// What each letter after the `:` of an expl3 name says of its argument (interface3,
    /// "Function and variable names"): offered when such a name is typed.
    pub signatures: std::collections::BTreeMap<String, String>,
    /// The character the typeset view shows for a math font slot, keyed `family * 256 + slot`
    /// (0 OT1, 1 OML, 2 OMS, 3 OMX): what `fontmath.ltx`'s `\DeclareMathSymbol`s draw there, as Unicode.
    pub math_symbols: std::collections::BTreeMap<u32, char>,
    pub ties: Ties,
    pub quotes: Quotes,
    pub units: Units,
    pub tables: Tables,
    pub math: MathConventions,
    pub csname: Csname,
    pub fonts: Fonts,
    /// Primitives LaTeX has a command for (chktex's `Primitives`), with it.
    pub alternatives: BTreeMap<String, String>,
    pub completion: Completion,
}

type BTreeMap<K, V> = std::collections::BTreeMap<K, V>;

fn strs(all: &[&str]) -> Vec<String> {
    all.iter().map(|s| s.to_string()).collect()
}

fn pairs<V: Clone>(all: &[(&str, V)]) -> BTreeMap<String, V> {
    all.iter().map(|(k, v)| (k.to_string(), v.clone())).collect()
}

/// Declares a `conventions:` area: a struct whose fields all have defaults.
macro_rules! area {
    ($(#[$m:meta])* $name:ident { $($(#[$fm:meta])* $field:ident: $ty:ty = $default:expr),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
        #[serde(default, deny_unknown_fields)]
        pub struct $name { $($(#[$fm])* pub $field: $ty),+ }
        impl Default for $name {
            fn default() -> $name {
                $name { $($field: $default),+ }
            }
        }
    };
}

area! {
    /// Where a tie belongs (chktex's `Linker` and the warnings beside it).
    Ties {
        /// Commands a tie belongs in front of.
        before: Vec<String> = strs(&[r"\ref", r"\pageref", r"\eqref", r"\cite"]),
        /// References that set the name before the number themselves, and
        /// citations that print words of the sentence: an author, a title, a year.
        naming: Vec<String> = strs(&[
            r"\cref", r"\Cref", r"\crefrange", r"\Crefrange", r"\cpageref", r"\Cpageref", r"\autoref",
            r"\vref", r"\Vref", r"\vrefrange", r"\nameref", r"\Nameref",
            r"\citeauthor", r"\Citeauthor", r"\textcite", r"\Textcite", r"\citetitle", r"\Citetitle",
            r"\citeyear", r"\citedate", r"\citet", r"\Citet", r"\citealt", r"\Citealt", r"\citealp", r"\Citealp",
        ]),
        /// Commands no space goes in front of, beside `\label`.
        unspaced: Vec<String> = strs(&[r"\index"]),
        /// Footnote commands, which no space goes in front of either.
        footnote: Vec<String> = strs(&[r"\footnote", r"\footnotemark"]),
        /// What sets a hard space.
        hard: Vec<String> = strs(&["~", r"\nobreakspace"]),
        /// Abbreviations whose period ends no sentence, whatever follows.
        abbreviations: Vec<String> = Vec::new(),
    }
}

area! {
    Quotes {
        /// babel's names of the languages that quote with `` and ''.
        english: Vec<String> = strs(&["english", "american", "USenglish", "canadian", "british", "UKenglish", "australian", "newzealand"]),
        /// Packages that set the language in force.
        languages: Vec<String> = strs(&["babel", "polyglossia"]),
        /// Quote characters in a row that make a ligature worth a thin space.
        ligature: usize = 3,
    }
}

area! {
    Units {
        /// The siunitx command a quantity is written with.
        quantity: String = "qty".into(),
        /// Its name before siunitx 3, used where only that is defined.
        legacy: String = "SI".into(),
        /// SI prefixes with siunitx's macro (SI brochure, table 7).
        prefixes: BTreeMap<String, String> = pairs(&[
            ("da", r"\deca"), ("q", r"\quecto"), ("r", r"\ronto"), ("y", r"\yocto"), ("z", r"\zepto"),
            ("a", r"\atto"), ("f", r"\femto"), ("p", r"\pico"), ("n", r"\nano"), ("u", r"\micro"),
            ("\u{b5}", r"\micro"), ("\u{3bc}", r"\micro"), ("m", r"\milli"), ("c", r"\centi"), ("d", r"\deci"),
            ("h", r"\hecto"), ("k", r"\kilo"), ("M", r"\mega"), ("G", r"\giga"), ("T", r"\tera"), ("P", r"\peta"),
            ("E", r"\exa"), ("Z", r"\zetta"), ("Y", r"\yotta"), ("R", r"\ronna"), ("Q", r"\quetta"),
        ].map(|(k, v)| (k, v.to_string()))),
        /// Unit symbols with siunitx's macro (SI brochure, tables 1-4, § 4.1).
        symbols: BTreeMap<String, String> = pairs(&[
            ("m", r"\metre"), ("g", r"\gram"), ("s", r"\second"), ("A", r"\ampere"), ("K", r"\kelvin"),
            ("mol", r"\mole"), ("cd", r"\candela"), ("Hz", r"\hertz"), ("N", r"\newton"), ("Pa", r"\pascal"),
            ("J", r"\joule"), ("W", r"\watt"), ("C", r"\coulomb"), ("V", r"\volt"), ("F", r"\farad"),
            ("S", r"\siemens"), ("Wb", r"\weber"), ("T", r"\tesla"), ("H", r"\henry"), ("lm", r"\lumen"),
            ("lx", r"\lux"), ("Bq", r"\becquerel"), ("Gy", r"\gray"), ("Sv", r"\sievert"), ("kat", r"\katal"),
            ("rad", r"\radian"), ("sr", r"\steradian"), ("min", r"\minute"), ("h", r"\hour"), ("d", r"\day"),
            ("t", r"\tonne"), ("L", r"\litre"), ("l", r"\litre"), ("eV", r"\electronvolt"), ("Da", r"\dalton"),
            ("Np", r"\neper"), ("B", r"\bel"), ("ha", r"\hectare"), ("bar", r"\bar"), ("\u{2126}", r"\ohm"),
            ("\u{3a9}", r"\ohm"), ("%", r"\percent"), ("\u{b0}", r"\degree"), ("\u{b0}C", r"\degreeCelsius"),
        ].map(|(k, v)| (k, v.to_string()))),
        /// Symbols no prefix goes in front of.
        unprefixed: Vec<String> = strs(&["min", "h", "d", "ha", "%", "\u{b0}", "\u{b0}C"]),
        /// Digits from which a number before `s` is a plural (`1980s`), not seconds.
        plural: usize = 3,
    }
}

area! {
    Tables {
        /// Environments read as tables, with the arguments before the column specification.
        environments: BTreeMap<String, usize> =
            pairs(&[("tabular", 0), ("tabular*", 1), ("tabularx", 1), ("longtable", 0), ("array", 0)]),
        /// Longest unit after a number in a cell.
        unit_length: usize = 6,
        /// Numbers a column needs to count as one of numbers.
        numbers: usize = 2,
    }
}

area! {
    MathConventions {
        /// Commands that say whether a display line is numbered.
        tags: Vec<String> = strs(&["notag", "nonumber", "tag"]),
        /// Commands that set a math accent.
        accents: Vec<String> = strs(&["mathaccent", "Umathaccent"]),
        /// Source tokens one file is lexed for.
        lexemes: usize = 400_000,
        /// How far from a formula its delimiter is looked for.
        near: usize = 400,
        /// How far past a display the sentence's punctuation is looked for.
        after: usize = 24,
    }
}

area! {
    /// What replaces a `\csname` doing each job, the first one defined.
    Csname {
        r#use: Vec<String> = strs(&["csuse", "@nameuse"]),
        def: Vec<String> = strs(&["csdef", "@namedef"]),
        gdef: Vec<String> = strs(&["csgdef"]),
        edef: Vec<String> = strs(&["csedef"]),
        xdef: Vec<String> = strs(&["csxdef"]),
        r#let: Vec<String> = strs(&["cslet"]),
        let_to: Vec<String> = strs(&["letcs"]),
        test: Vec<String> = strs(&["ifcsundef", "@ifundefined"]),
    }
}

area! {
    Fonts {
        /// The kernel's roman family, which a font package replaces.
        roman: String = "cmr".into(),
        /// Macros naming the shapes LaTeX gives a slanted face.
        slanted: Vec<String> = strs(&["itdefault", "sldefault"]),
    }
}

area! {
    Completion {
        /// Commands one answer lists.
        shown: usize = 300,
        /// Items, best first, whose signature the probe reads where the listing gives none.
        signatures: usize = 4,
        /// Levels below a directory key that are listed.
        key_depth: usize = 3,
        /// Installed files whose identification is read.
        identified: usize = 400,
        /// Calls of a command its hover shows.
        calls: usize = 4,
        /// Characters of an undocumented body its hover shows as what it expands to.
        body: usize = 80,
        /// Commands whose argument is a name to pick from a list.
        pick: Vec<String> = strs(&["begin", "end", "usepackage", "documentclass", "ref", "cref", "cite"]),
        /// Environments whose `\begin` reads arguments through an inner macro, with how many.
        environment_arguments: BTreeMap<String, u8> = pairs(&[
            ("array", 1), ("minipage", 1), ("multicols", 1), ("tabular", 1), ("tabular*", 2), ("tabularx", 2),
            ("thebibliography", 1), ("wrapfigure", 2),
        ]),
        /// Rank of each kind of command, lowest first.
        tiers: Tiers = Tiers::default(),
    }
}

area! {
    Tiers {
        document: u8 = 0,
        package: u8 = 1,
        primitive: u8 = 2,
        internal: u8 = 3,
    }
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
        let signatures = [
            ("N", "one token, usually a control sequence"),
            ("n", "a braced token list, taken as it is"),
            ("V", "the value of a variable, given by its name"),
            ("v", "the value of a variable whose name is built as with \\csname"),
            ("o", "a token list expanded once"),
            ("e", "a token list expanded fully, # kept"),
            ("x", "a token list expanded fully, as \\edef does"),
            ("f", "a token list expanded until the first unexpandable token"),
            ("c", "a control sequence name, built as with \\csname"),
            ("p", "a parameter text, such as #1#2"),
            ("T", "code run when the test is true"),
            ("F", "code run when the test is false"),
            ("w", "an argument read to a delimiter of its own"),
            ("D", "do not use: a primitive's wrapper"),
        ];
        Conventions {
            internal: vec!["@".into(), "^__".into(), "_.*:".into()],
            definers: Vec::new(),
            math_symbols: [
                (0x000, 'Γ'),
                (0x001, 'Δ'),
                (0x002, 'Θ'),
                (0x003, 'Λ'),
                (0x004, 'Ξ'),
                (0x005, 'Π'),
                (0x006, 'Σ'),
                (0x007, 'Υ'),
                (0x008, 'Φ'),
                (0x009, 'Ψ'),
                (0x00a, 'Ω'),
                (0x10b, 'α'),
                (0x10c, 'β'),
                (0x10d, 'γ'),
                (0x10e, 'δ'),
                (0x10f, 'ϵ'),
                (0x110, 'ζ'),
                (0x111, 'η'),
                (0x112, 'θ'),
                (0x113, 'ι'),
                (0x114, 'κ'),
                (0x115, 'λ'),
                (0x116, 'μ'),
                (0x117, 'ν'),
                (0x118, 'ξ'),
                (0x119, 'π'),
                (0x11a, 'ρ'),
                (0x11b, 'σ'),
                (0x11c, 'τ'),
                (0x11d, 'υ'),
                (0x11e, 'ϕ'),
                (0x11f, 'χ'),
                (0x120, 'ψ'),
                (0x121, 'ω'),
                (0x122, 'ε'),
                (0x123, 'ϑ'),
                (0x124, 'ϖ'),
                (0x125, 'ϱ'),
                (0x126, 'ς'),
                (0x127, 'φ'),
                (0x200, '−'),
                (0x201, '·'),
                (0x202, '×'),
                (0x203, '∗'),
                (0x204, '÷'),
                (0x206, '±'),
                (0x207, '∓'),
                (0x208, '⊕'),
                (0x209, '⊖'),
                (0x20a, '⊗'),
                (0x20b, '⊘'),
                (0x20c, '⊙'),
                (0x20e, '∘'),
                (0x20f, '•'),
                (0x211, '≡'),
                (0x212, '⊆'),
                (0x213, '⊇'),
                (0x214, '≤'),
                (0x215, '≥'),
                (0x218, '∼'),
                (0x219, '≈'),
                (0x21a, '⊂'),
                (0x21b, '⊃'),
                (0x21c, '≪'),
                (0x21d, '≫'),
                (0x220, '←'),
                (0x221, '→'),
                (0x222, '↑'),
                (0x223, '↓'),
                (0x224, '↔'),
                (0x230, '′'),
                (0x231, '∞'),
                (0x232, '∈'),
                (0x233, '∋'),
                (0x234, '△'),
                (0x238, '∀'),
                (0x239, '∃'),
                (0x23a, '¬'),
                (0x23b, '∅'),
                (0x240, 'ℵ'),
                (0x25b, '∪'),
                (0x25c, '∩'),
                (0x25e, '∧'),
                (0x25f, '∨'),
                (0x266, '{'),
                (0x267, '}'),
                (0x268, '⟨'),
                (0x269, '⟩'),
                (0x26a, '|'),
                (0x26b, '‖'),
                (0x272, '∇'),
                (0x348, '∮'),
                (0x349, '∮'),
                (0x34a, '⨀'),
                (0x34b, '⨀'),
                (0x34c, '⨁'),
                (0x34d, '⨁'),
                (0x34e, '⨂'),
                (0x34f, '⨂'),
                (0x350, '∑'),
                (0x351, '∏'),
                (0x352, '∫'),
                (0x353, '⋃'),
                (0x354, '⋂'),
                (0x355, '⨄'),
                (0x356, '⋀'),
                (0x357, '⋁'),
                (0x358, '∑'),
                (0x359, '∏'),
                (0x35a, '∫'),
                (0x35b, '⋃'),
                (0x35c, '⋂'),
                (0x35d, '⨄'),
                (0x35e, '⋀'),
                (0x35f, '⋁'),
                (0x360, '∐'),
                (0x361, '∐'),
            ]
            .into_iter()
            // OMS's capitals are calligraphic (`\mathcal`)
            .chain(('A'..='Z').zip("𝒜ℬ𝒞𝒟ℰℱ𝒢ℋℐ𝒥𝒦ℒℳ𝒩𝒪𝒫𝒬ℛ𝒮𝒯𝒰𝒱𝒲𝒳𝒴𝒵".chars()).map(|(l, c)| (0x200 + l as u32, c)))
            .collect(),
            signatures: signatures.into_iter().map(|(k, v)| (k.into(), v.into())).collect(),
            ties: Ties::default(),
            quotes: Quotes::default(),
            units: Units::default(),
            tables: Tables::default(),
            math: MathConventions::default(),
            csname: Csname::default(),
            fonts: Fonts::default(),
            alternatives: pairs(
                &[
                    ("above", r"`\frac`"),
                    ("advance", r"`\addtocounter` or `\addtolength`"),
                    ("chardef", r"`\newcommand`"),
                    ("copy", r"`\usebox`"),
                    ("count", r"`\newcounter` and `\value`"),
                    ("countdef", r"`\newcounter`"),
                    ("cr", r"`\\`"),
                    ("crcr", r"`\\`"),
                    ("csname", r"`\@nameuse`"),
                    ("dimen", r"`\newlength`"),
                    ("dimendef", r"`\newlength`"),
                    ("divide", r"`\setlength` with calc"),
                    ("font", r"`\newfont` or `\selectfont`"),
                    ("hskip", r"`\hspace`"),
                    ("vskip", r"`\vspace`"),
                    ("openout", r"`\newwrite`"),
                ]
                .map(|(k, v)| (k, v.to_string())),
            ),
            completion: Completion::default(),
        }
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
                tex.is_file() || crate::project::overlay::holds(&tex)
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
    /// Characters before a package's end within which every line start is
    /// a checkpoint: its last command may read past the end (`\color` ends
    /// in `\ignorespaces`), and the line it began on is then the end a
    /// replay can resume from.
    pub tail: usize,
    /// Keep each document's whole analysis: a rerun of a document whose
    /// text, settings and every file it read are unchanged executes nothing.
    pub document: bool,
}

impl Default for Cache {
    fn default() -> Cache {
        Cache { enabled: true, dir: None, rebuild: false, index: CacheIndex::default(), tail: 256, document: true }
    }
}

/// `analysis:` in `satex.yaml`: where paths join, and what the run assumes.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct AnalysisConfig {
    /// Places where all paths still apart are merged into one. A `command`
    /// matches by meaning, so `\document` merges at `\begin{document}`
    /// however the run gets there. A `hook` is an lthooks name. Values the
    /// paths disagree on are merged, no path is dropped, and each merge is
    /// reported once as `paths-joined-at`.
    pub join_paths_at: Vec<JoinPoint>,
    /// Assumes the document compiles. A path where a package or the kernel
    /// raises a certain TeX error cannot be a real run, so it is dropped.
    /// Errors in your own text are always kept and reported. Each drop is
    /// reported once as an `assumption`, the last path is never dropped,
    /// and `false` keeps every path.
    pub assume_compiles: bool,
    /// Assumes a package closes every group it opens, so paths that differ
    /// only in open groups merge. `false` keeps them apart.
    pub assume_packages_close_groups: bool,
    /// Assumes the build sees this machine's environment variables, as
    /// Lua's `os.getenv` reads them. `false` makes every variable unknown.
    pub assume_environment: bool,
    /// Assumes pages break only where something forces it (`\clearpage`,
    /// `\newpage`, `\end`). `false` also follows a page break wherever the
    /// page may have filled, one path each way.
    pub assume_forced_page_breaks: bool,
    /// A choice undecided while a page is made takes one arm ([`crate::analysis::machine::Machine`]'s `page_arm`).
    pub assume_page_one_arm: bool,
    /// Assumes formatting a message for the terminal or log (expl3's
    /// `\iow_wrap:nnnN`) changes nothing but the text it writes. A message over
    /// unknown text then writes unknown text, on one path, reported as an
    /// `assumption`. `false` wraps it for real, a path per way it may wrap.
    pub assume_messages_output_only: bool,
    /// Assumes a `\catcode` through a character satex could not pin down
    /// (a summarized loop's counter) never names a structural character,
    /// one of category neither letter nor other (`\ { } % $ & # ^ _`,
    /// space, end of line, active). Reported as `unknown-char-catcode`.
    /// `false` lets it reach them: the reader splits a path per category.
    pub assume_unknown_char_catcode: bool,
    /// Assumes a call whose run keeps forking on unknown values leaves what
    /// runs of its definition on other values left, each value widened to
    /// its type, and raises nothing. Reported as `call-learned-effect`.
    /// `false` walks every such call.
    pub assume_call_learned_effect: bool,
}

impl Default for AnalysisConfig {
    fn default() -> AnalysisConfig {
        AnalysisConfig {
            join_paths_at: vec![JoinPoint { command: Some("\\document".into()), hook: None }],
            assume_compiles: true,
            assume_packages_close_groups: true,
            assume_environment: true,
            assume_forced_page_breaks: true,
            assume_page_one_arm: true,
            assume_messages_output_only: true,
            assume_unknown_char_catcode: true,
            assume_call_learned_effect: true,
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
    /// TeX code read before the document's first line, as latexmk's
    /// `-usepretex` hands the engine: `\def\draft{}` and the like.
    #[serde(default)]
    pub pretex: String,
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
    pub kernel: Option<crate::plugins::Kernel>,
    pub provider: Option<crate::plugins::Provider>,
    pub platform: Option<crate::plugins::Platform>,
    pub output: Option<crate::plugins::Output>,
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
    /// Dev mode: each trace event carries the abstract state of its step.
    #[serde(default)]
    pub trace_state: bool,
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
    /// Stop the run at this trace event, so queries see the state there.
    #[serde(default)]
    pub trace_stop: Option<u32>,
    pub timings: bool,
    pub opaque_conditionals: Vec<String>,
    /// Arms of a choice inside an expanding scan (`\edef`, `\csname`) run
    /// as paths and join where they meet in the scan (SEMANTICS.md § 14).
    /// Off, the command is re-run once per sequence of arms. On by default.
    pub arm_joins: bool,
    pub record_arguments: bool,
    /// How the files a run reads map to TeX Live packages, for the
    /// dependency file depp writes.
    pub depp: crate::plugins::depp::DeppConfig,
    /// Which comments are magic: `% !TeX`, `% arara:` and the rest.
    pub magic: crate::plugins::magic::MagicConfig,
    /// The bibliography backend assumed where the document names none.
    pub bib: crate::plugins::bib::BibConfig,
    /// Whether and how far `\directlua` code runs.
    pub lua: crate::plugins::lua::run::LuaConfig,
    /// How `satex lsp` listens and re-analyzes.
    #[serde(default)]
    pub lsp: LspConfig,
    /// Where package documentation is looked for and which conventions are read.
    #[serde(default)]
    pub docs: DocsConfig,
    /// What `satex slice` leaves out.
    #[serde(default)]
    pub slice: SliceConfig,
    #[serde(default)]
    pub playground: PlaygroundConfig,
    /// The editor features each type plugin adds.
    #[serde(default)]
    pub type_plugins: TypePlugins,
    pub l3build: L3buildConfig,
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
            pretex: String::new(),
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
            lint_off: vec!["analysis-imprecision".to_string(), "assertion-proved".to_string()],
            project: Project::default(),
            lints: Lints::default(),
            conventions: Conventions::default(),
            profile: None,
            profiles: Profiles::default(),
            verbose: 0,
            trace: false,
            trace_state: false,
            trace_document_only: false,
            trace_lines: None,
            trace_col: 0,
            trace_file: None,
            trace_stop: None,
            trace_end_col: u32::MAX,
            trace_steps: None,
            timings: false,
            opaque_conditionals: Vec::new(),
            arm_joins: true,
            record_arguments: false,
            depp: Default::default(),
            magic: Default::default(),
            bib: Default::default(),
            lua: Default::default(),
            lsp: LspConfig::default(),
            docs: DocsConfig::default(),
            slice: SliceConfig::default(),
            playground: PlaygroundConfig::default(),
            type_plugins: TypePlugins::default(),
            l3build: L3buildConfig::default(),
        }
    }
}

named_enum! {
    /// Which TeX engine's primitive set the run assumes: plain TeX, pdfTeX,
    /// LuaTeX or XeTeX. [`crate::plugins::Plugins::engine`] and `\ifpdf`-style
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
        let name = program_name(program);
        Self::PROGRAMS.iter().find(|(p, _)| *p == name).map(|(_, e)| *e)
    }

    /// The programs a magic comment or a build may name, with the engine
    /// each drives. `latex` and `etex` run pdfTeX writing DVI (TeX Live's
    /// `fmtutil.cnf`); `tex` is Knuth's TeX, which has no format for LaTeX.
    /// ConTeXt MkIV runs on LuaTeX and MkXL on LuaMetaTeX, a LuaTeX
    /// derivative (ConTeXt "LMTX" documentation); satex models the LuaTeX
    /// primitives for both, which is why ConTeXt is experimental. The
    /// ConTeXt kernel itself is not interpreted; see [`crate::plugins::Kernel`].
    pub const PROGRAMS: [(&'static str, Engine); 16] = [
        ("pdflatex", Engine::PdfTeX),
        ("lualatex", Engine::LuaTeX),
        ("xelatex", Engine::XeTeX),
        ("latex", Engine::PdfTeX),
        ("pdftex", Engine::PdfTeX),
        ("luatex", Engine::LuaTeX),
        ("xetex", Engine::XeTeX),
        ("tex", Engine::Tex),
        ("etex", Engine::PdfTeX),
        ("lualatex-dev", Engine::LuaTeX),
        ("dvilualatex", Engine::LuaTeX),
        ("dviluatex", Engine::LuaTeX),
        ("context", Engine::LuaTeX),
        ("contextjit", Engine::LuaTeX),
        ("luametatex", Engine::LuaTeX),
        ("mtxrun", Engine::LuaTeX),
    ];

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
/// [`Engine::encoding`] picks it, and [`crate::engine::source`] holds the choice for
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
    /// [`crate::analysis::machine::Machine::analyze`] reads a main file by this too.
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
        let dtx =
            path.and_then(crate::project::literate::Literate::of) == Some(crate::project::literate::Literate::Dtx);
        let code =
            dtx.then(|| crate::project::literate::code_view(source, &crate::project::literate::Guards::AllButDriver));
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
    pub table_rules: TableRules,
    pub itemize_numbered: ItemizeNumbered,
    pub itemize_described: ItemizeDescribed,
    pub package_successor: PackageSuccessor,
    pub clones: Clones,
    /// When an editor asks whether a frequent finding is the project's style.
    pub style_prompt: StylePrompt,
}

/// `lints.clones:` in `satex.yaml`: which repeated blocks of the source are worth a macro.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Clones {
    /// Whether the rule looks for clones at all.
    pub enabled: bool,
    /// Tokens a clone must share, its differing values not counted.
    pub min_tokens: usize,
    /// Source lines a clone must span.
    pub min_lines: usize,
    /// The share of matching tokens two near-identical blocks need. 1 finds
    /// only blocks that differ in their values.
    pub similarity: f64,
    /// The most holes a macro the fix writes may take. TeX allows 9.
    pub max_parameters: usize,
    /// The share of a clone its differing values may take, so paragraphs and
    /// lists of items are no clones.
    pub max_value_share: f64,
    /// Regexes on environment names whose bodies are never read for clones.
    pub ignore: Vec<String>,
    /// Where to look: `body`, `preamble` or `both`.
    pub scope: CloneScope,
    /// How many places a block must stand at.
    pub min_occurrences: usize,
    /// Tokens of a window whose hash finds candidates: smaller finds shorter clones.
    pub seed: usize,
    /// How many later places with the same window one place is compared with.
    pub partners: usize,
    /// Characters a macro must save to be worth it: the shared text each
    /// further place drops, less `parameter-cost` per argument at every call.
    pub min_saved: usize,
    /// Characters one argument costs at a call: its braces, its value's
    /// reading, the reader's lookup of what it means.
    pub parameter_cost: usize,
}

/// Which part of the document [`Clones`] reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CloneScope {
    Body,
    Preamble,
    Both,
}

impl Default for Clones {
    fn default() -> Clones {
        Clones {
            enabled: true,
            min_tokens: 40,
            min_lines: 1,
            similarity: 0.85,
            max_parameters: 9,
            max_value_share: 0.5,
            ignore: Vec::new(),
            scope: CloneScope::Both,
            min_occurrences: 2,
            seed: 8,
            partners: 32,
            min_saved: 20,
            parameter_cost: 12,
        }
    }
}

/// `lints.table-rules:` in `satex.yaml`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct TableRules {
    /// Rows with text, header included, a table needs for its rules to matter.
    pub rows: usize,
}

impl Default for TableRules {
    fn default() -> TableRules {
        TableRules { rows: 3 }
    }
}

/// `lints.itemize-numbered:` in `satex.yaml`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ItemizeNumbered {
    /// Items a list needs for its hand-written numbers to matter.
    pub items: usize,
}

impl Default for ItemizeNumbered {
    fn default() -> ItemizeNumbered {
        ItemizeNumbered { items: 2 }
    }
}

/// `lints.itemize-described:` in `satex.yaml`.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct ItemizeDescribed {
    /// Items a list needs to read as a description.
    pub items: usize,
    /// Letters every label shows for it to count as words, not a symbol.
    pub letters: usize,
}

impl Default for ItemizeDescribed {
    fn default() -> ItemizeDescribed {
        ItemizeDescribed { items: 2, letters: 4 }
    }
}

/// `lints.package-successor:` in `satex.yaml`: packages a successor continues.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct PackageSuccessor {
    pub successors: Vec<Successor>,
}

/// One package, the package that continues it, and what moving changes.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Successor {
    pub package: String,
    pub successor: String,
    pub changes: String,
}

impl Default for PackageSuccessor {
    fn default() -> PackageSuccessor {
        // wrapfig2's manual, § 2 and § 4 (v7.0.2, 2025-03-01).
        let wrapfig = Successor {
            package: "wrapfig".into(),
            successor: "wrapfig2".into(),
            changes: "`wrapfigure` and `wraptable` keep their arguments; a final `*` makes the line count a correction, and `wraptext` wraps framed text. wrapfig2 needs a LaTeX kernel from 2019 on, loads xparse, xfp, etoolbox, float, xcolor, pict2e and xkeyval, and redefines `\\setfontsize`".into(),
        };
        PackageSuccessor { successors: vec![wrapfig] }
    }
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
    /// How many belong between spaces, as chktex's `WordDash`. Two is
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
/// variable ([`crate::project::paths::VARIABLES`]), used when neither the process
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
    /// The root document, relative to the project directory: what a run starts
    /// from when none is named, over a `% !TeX root` line and every guess.
    pub root: Option<PathBuf>,
}

impl Default for Project {
    fn default() -> Self {
        Project { frozen: Vec::new(), freeze_other_repositories: true, ignore_other_configs: false, root: None }
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
    ("style_prompt", &["lints", "style-prompt"]),
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
        warn(format!("{FILE_NAME}: {context}: cache is now cache.enabled"));
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
            warn(format!("{FILE_NAME}: {context}: {key_name} is now {}", path.join(".")));
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
    pub fn distribution_request(&self) -> crate::caches::distribution::Request {
        crate::caches::distribution::Request {
            roots: self.texmf.roots.clone(),
            bundle: self.texmf.bundle.clone(),
            install_root: self.texlive.root.clone(),
            year: self.texlive.year,
            use_kpsewhich: self.texmf.kpsewhich,
            probe_fallback: self.texmf.fallback,
            provider: self.provider.or_else(|| self.texmf.kpsewhich.then(crate::plugins::Provider::detect).flatten()),
            platform: self.platform.unwrap_or_else(crate::plugins::Platform::current),
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
            warn(format!("{FILE_NAME}: --set {path}: this is now --set {migrated}"));
        }
        let parsed: Value = serde_yaml_ng::from_str(value).map_err(|e| format!("{migrated}: {e}"))?;
        let mut tree = serde_yaml_ng::to_value(&*self).map_err(|e| e.to_string())?;
        let mut slot = &mut tree;
        // `key+` appends to a list, as in a satex.yaml (`merge_value`)
        let (keys, append) = migrated.strip_suffix('+').map_or((migrated.as_str(), false), |k| (k, true));
        for key in keys.split('.') {
            slot = slot
                .as_mapping_mut()
                .and_then(|map| map.get_mut(key))
                .ok_or_else(|| format!("no option `{migrated}` in {FILE_NAME}"))?;
        }
        match (append, slot, parsed) {
            (true, Value::Sequence(list), Value::Sequence(more)) => list.extend(more),
            (true, ..) => return Err(format!("{migrated}: only a list takes `+`")),
            (false, slot, parsed) => *slot = parsed,
        }
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
    /// ([`crate::project::config`] module docs), so either of those still wins over
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
    crate::project::paths::config_dir().map(|dir| dir.join("satex").join(FILE_NAME))
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

/// A notice about a config file, said once per process: each run reads the files again.
fn warn(notice: String) {
    static SAID: std::sync::Mutex<Option<std::collections::HashSet<String>>> = std::sync::Mutex::new(None);
    let mut said = SAID.lock().unwrap_or_else(|e| e.into_inner());
    if said.get_or_insert_default().insert(notice.clone()) {
        eprintln!("{notice}");
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
        Err(e) => return warn(format!("satex: {}: ignored: {e}", path.display())),
    };
    // A project, maybe someone else's, does not say where satex writes its
    // caches or whether a lint reaches the network: only the user's own
    // config, `--config`, `--set` and `SATEX_CACHE` do.
    if project {
        for (section, key) in USER_KEYS {
            if let Some(serde_yaml_ng::Value::Mapping(map)) = incoming.get_mut(section)
                && map.remove(key).is_some()
            {
                warn(format!("satex: {}: {section}.{key} ignored: only the user's own config sets it", path.display()));
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
        let Some((without, key)) = dropped else { return warn(format!("satex: {}: ignored: {e}", path.display())) };
        warn(format!("satex: {}: warning: unknown key `{key}` ignored, the rest of the file applies", path.display()));
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

/// What `documented`, the repository's own `satex.yaml`, says about each
/// dotted key: its description (`key: value  # text`, a comment alone on a
/// line goes on) and the `#! affects:`, `#! example:` and `#! range:` lines
/// under it, and `#! plugin:`, the plugin whose work the key steers.
#[derive(Default)]
struct Documented {
    description: String,
    affects: Vec<String>,
    example: Vec<String>,
    range: Option<(f64, f64)>,
    plugin: Option<String>,
    /// `#! category:`, the group a type plugin's settings belong to.
    category: Option<String>,
    /// `#! option: value: text` lines: the values a setting takes and what each means.
    options: Vec<(String, String)>,
}

fn documented_keys(documented: &str) -> std::collections::HashMap<String, Documented> {
    let mut about: std::collections::HashMap<String, Documented> = Default::default();
    let (mut path, mut last): (Vec<(usize, String)>, Option<String>) = (Vec::new(), None);
    for line in documented.lines() {
        let indent = line.len() - line.trim_start().len();
        let text = line.trim_start();
        if let Some(meta) = text.strip_prefix("#!") {
            let Some(entry) = last.as_ref().and_then(|key| about.get_mut(key)) else { continue };
            match meta.trim().split_once(':').map(|(k, v)| (k.trim(), v.trim())) {
                Some(("affects", v)) => entry.affects.extend(v.split(',').map(|a| a.trim().to_string())),
                Some(("example", v)) => entry.example.push(v.to_string()),
                Some(("plugin", v)) => entry.plugin = Some(v.to_string()),
                Some(("category", v)) => entry.category = Some(v.to_string()),
                Some(("option", v)) => {
                    if let Some((value, text)) = v.split_once(':') {
                        entry.options.push((value.trim().to_string(), text.trim().to_string()));
                    }
                }
                Some(("range", v)) => {
                    entry.range = v.split_once("..").and_then(|(lo, hi)| Some((lo.parse().ok()?, hi.parse().ok()?)))
                }
                _ => {}
            }
            continue;
        }
        // A commented-out setting (`# engine: pdftex  # …`) documents that key.
        let commented = text.strip_prefix('#').map(str::trim_start).filter(|c| setting_line(c));
        let text = commented.unwrap_or(text);
        if let Some(more) = text.strip_prefix('#').filter(|_| indent > 0) {
            if let Some(entry) =
                last.as_ref().and_then(|key| about.get_mut(key)).filter(|_| !more.trim_start().starts_with('-'))
            {
                entry.description = format!("{} {}", entry.description, more.trim());
            }
            continue;
        }
        let Some((key, rest)) = text.split_once(':').filter(|(k, _)| !k.is_empty() && !k.starts_with(['#', '-']))
        else {
            continue;
        };
        path.retain(|(i, _)| *i < indent);
        path.push((indent, key.trim().to_string()));
        let dotted = path.iter().map(|(_, k)| k.as_str()).collect::<Vec<_>>().join(".");
        let description = rest.split_once(" # ").map(|(_, c)| c.trim().to_string()).unwrap_or_default();
        about.insert(dotted.clone(), Documented { description, ..Documented::default() });
        last = Some(dotted);
    }
    about
}

/// Whether a comment's text is a setting, `key: value` with a plain key.
fn setting_line(text: &str) -> bool {
    text.split_once(':').is_some_and(|(key, value)| {
        !key.is_empty()
            && key.chars().all(|c| c.is_ascii_alphanumeric() || "_.".contains(c))
            && !value.trim().is_empty()
    })
}

/// The values a setting whose default is unset may take, from its type's
/// own list of spellings.
fn choices(path: &str) -> Option<Vec<&'static str>> {
    let names = |all: &[&'static str]| Some(all.to_vec());
    match path {
        "engine" => names(&Engine::ALL.iter().map(|e| e.as_str()).collect::<Vec<_>>()),
        "kernel" => names(&crate::plugins::Kernel::ALL.iter().map(|k| k.as_str()).collect::<Vec<_>>()),
        "provider" => names(&crate::plugins::Provider::ALL.iter().map(|p| p.as_str()).collect::<Vec<_>>()),
        "platform" => names(&crate::plugins::Platform::ALL.iter().map(|p| p.as_str()).collect::<Vec<_>>()),
        "output" => names(&crate::plugins::Output::ALL.iter().map(|o| o.as_str()).collect::<Vec<_>>()),
        "profile" => names(&Profile::ALL.iter().map(|p| p.as_str()).collect::<Vec<_>>()),
        "shell_escape" => names(&ShellEscape::ALL.iter().map(|s| s.as_str()).collect::<Vec<_>>()),
        "bib.backend" => names(&crate::plugins::bib::Backend::ALL.iter().map(|b| b.as_str()).collect::<Vec<_>>()),
        "interaction" => names(&Interaction::ALL.iter().map(|i| i.as_str()).collect::<Vec<_>>()),
        _ => None,
    }
}

/// The JSON schema of `satex.yaml` (`satex.schema.json`): each key with the
/// type and value [`Config::default`] gives it, described by
/// [`documented_keys`]. Enumerated settings list their values, lists their
/// items' type, and counts their bounds: never below 0 unless a range says so.
pub fn schema(documented: &str) -> serde_json::Value {
    use serde_json::{Value, json};
    let about = documented_keys(documented);
    fn item(v: Option<&Value>) -> Value {
        match v {
            Some(Value::Object(m)) => json!({
                "type": "object",
                "properties": m.keys().map(|k| (k.clone(), json!({ "type": "string" }))).collect::<serde_json::Map<_, _>>(),
                "minProperties": 1,
                "maxProperties": 1,
            }),
            Some(Value::Number(n)) if n.is_f64() => json!({ "type": "number" }),
            Some(Value::Number(_)) => json!({ "type": "integer", "minimum": 0 }),
            Some(Value::Bool(_)) => json!({ "type": "boolean" }),
            _ => json!({ "type": "string" }),
        }
    }
    fn walk(path: &str, v: &Value, about: &std::collections::HashMap<String, Documented>) -> Value {
        let mut out = match v {
            Value::Object(m) => json!({
                "type": "object",
                "properties": m.iter().map(|(k, v)| {
                    let p = if path.is_empty() { k.clone() } else { format!("{path}.{k}") };
                    (k.clone(), walk(&p, v, about))
                }).collect::<serde_json::Map<_, _>>(),
            }),
            Value::Bool(_) => json!({ "type": "boolean" }),
            Value::Number(n) if n.is_f64() => json!({ "type": "number", "minimum": 0 }),
            Value::Number(_) => json!({ "type": "integer", "minimum": 0 }),
            Value::String(_) => json!({ "type": "string" }),
            Value::Array(a) => json!({ "type": "array", "items": item(a.first()) }),
            Value::Null => json!({}),
        };
        if let Some(values) = choices(path) {
            out["type"] = json!("string");
            out["enum"] = json!(values);
        }
        // `#! option:` lines name the values and say what each means (VS Code's `enumDescriptions`). A list takes them as its items.
        if let Some(doc) = about.get(path).filter(|d| !d.options.is_empty()) {
            let names: Vec<String> = doc.options.iter().map(|(o, _)| o.clone()).collect();
            let values: Vec<String> = match out["enum"].as_array().or_else(|| out["items"]["enum"].as_array()) {
                Some(values) => values.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
                None => names.clone(),
            };
            let descriptions: Vec<String> = values
                .iter()
                .map(|v| doc.options.iter().find(|(o, _)| o == v).map(|(_, t)| t.clone()).unwrap_or_default())
                .collect();
            if out["type"] == "array" {
                if out["items"]["enum"].is_null() {
                    out["items"] = json!({ "type": "string", "enum": names });
                }
                out["items"]["enumDescriptions"] = json!(descriptions);
            } else {
                out["enumDescriptions"] = json!(descriptions);
            }
        }
        if path == "clock" {
            out = json!({ "oneOf": [
                { "type": "string", "enum": ["now", "unknown"] },
                { "type": "string", "pattern": r"^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}(:\d{2})?(Z|[+-]\d{2}:\d{2})?)?$" },
            ] });
        }
        if !matches!(v, Value::Object(_) | Value::Null) {
            out["default"] = v.clone();
        }
        if let Some(doc) = about.get(path) {
            if !doc.description.is_empty() {
                out["description"] = json!(doc.description.trim());
            }
            if !doc.affects.is_empty() {
                out["x-affects"] = json!(doc.affects);
            }
            if !doc.example.is_empty() {
                out["x-example"] = json!(doc.example.join("\n"));
            }
            if let Some(plugin) = &doc.plugin {
                out["x-plugin"] = json!(plugin);
            }
            if let Some(category) = &doc.category {
                out["x-category"] = json!(category);
            }
            if let Some((lo, hi)) = doc.range {
                let bound = |x: f64| if x.fract() == 0.0 { json!(x as i64) } else { json!(x) };
                out["minimum"] = bound(lo);
                out["maximum"] = bound(hi);
            }
        }
        out
    }
    let mut out = walk("", &serde_json::to_value(Config::default()).unwrap_or_default(), &about);
    out["$schema"] = json!("http://json-schema.org/draft-07/schema#");
    out["title"] = json!("satex.yaml");
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
        assert_eq!(cfg.conventions, Conventions::default());
    }

    /// Every `conventions:` key the defaults have is described in the repo's `satex.yaml`.
    #[test]
    fn every_convention_is_documented() {
        let documented = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(FILE_NAME)).unwrap();
        let about = documented_keys(&documented);
        let schema = schema(&documented);
        let missing: Vec<String> = keys()
            .keys()
            .filter(|k| k.starts_with("conventions.") && !k.starts_with("conventions.signatures."))
            .filter(|k| {
                // a map's entries are documented by the map
                let parts: Vec<&str> = k.split('.').collect();
                !(2..=parts.len()).any(|n| about.get(&parts[..n].join(".")).is_some_and(|d| !d.description.is_empty()))
            })
            .cloned()
            .collect();
        assert!(missing.is_empty(), "undocumented: {missing:?}");
        assert!(schema["properties"]["magic"]["properties"]["arara"]["x-plugin"] == "magic");
    }

    #[test]
    fn an_option_is_set_by_its_path_in_the_file() {
        let mut cfg = Config::default();
        cfg.set("limits.trace", "10").unwrap();
        cfg.set("load.inputs", "false").unwrap();
        assert_eq!(cfg.limits.trace, 10);
        assert!(!cfg.load.inputs);
        assert!(cfg.set("limits.bogus", "1").is_err());
        let before = cfg.lint_off.len();
        cfg.set("lint_off+", "[unused-label]").unwrap();
        assert_eq!((cfg.lint_off.len(), cfg.lint_off.last().map(String::as_str)), (before + 1, Some("unused-label")));
        assert!(cfg.set("limits.trace+", "[1]").is_err());
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
