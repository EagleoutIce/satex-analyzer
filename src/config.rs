use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// `opaque_conditionals` entry standing for every switch the document's
/// own files declare with `\newif` and the like.
pub const EVERY_SWITCH: &str = "*";

fn default_true() -> bool {
    true
}

/// What bounds the analysis: reaching one stops that part of the run and
/// records an [`Imprecision`](crate::facts::Severity::Imprecision), so no
/// input can make satex loop or run out of memory.
#[derive(Clone, Debug, Deserialize, Serialize)]
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
    /// Tokens a run may digest without reading from a file before the
    /// expansion it is in is given up as a loop that does not end.
    pub stall_tokens: u64,
    /// Wall-clock seconds a run may take before it stops and says so; 0 for
    /// no limit.  A safety net: no document should come near it.
    pub seconds: u64,
    /// How deep undecided conditions may nest before only one arm is read.
    pub branch_depth: u16,
    /// Tokens the arms of one undecided conditional may read, together,
    /// before they have to meet again in the input.
    pub join_tokens: u64,
    /// How deep in nested paths a name that a join left with one of several
    /// meanings still splits the run into a path per meaning when it is
    /// executed or compared by `\ifx`; deeper, and with 0 everywhere, it is
    /// read as a name of unknown meaning.
    pub meaning_splits: u16,
    /// How many rounds a loop head analyzes its paths again from a grown
    /// joined state before what still grows is widened (SEMANTICS § 14).
    pub widen_after: u16,
    /// How many meanings a join leaves a name with as a set (one of them)
    /// before it has an unknown meaning.
    pub meaning_set: u16,
    /// How many values a join leaves a count or dimension register with as
    /// a set before only their interval is kept.
    pub value_set: u16,
    /// Tokens one expansion, argument or conditional may span.  TeX itself
    /// has no such limit, so these only stop a single pathological read from
    /// taking all of `memory`; what makes a run terminate is `steps`,
    /// `held_tokens` and the recursion widening.  Only tokens an expansion
    /// produces count: source read straight from a file is bounded by the
    /// file, so a document body is never cut short by this.
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
    /// How much the package caches in the cache directory may take together;
    /// the least recently used go first.  Zero lifts the bound.
    pub cache_size: bytesize::ByteSize,
}

impl Limits {
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
            steps: 900_000_000,
            // The real expl3 reads the Unicode data files while the kernel
            // is built; that is done once and then cached, and a runaway is
            // cut short by `stall_tokens` long before this bound.
            format_steps: 1_000_000_000,
            file_depth: 64,
            expansion_depth: 24,
            // Measured: the kernel's own helpers (`\hexnumber@` inside
            // `\DeclareMathSymbol`, the expl3 mapping functions) re-expand one
            // site far more often than that while the source stands still,
            // and widening them loses every math symbol after the first few.
            // pgfkeys runs one helper site for every key of a `\tikzset`
            // list that was read in one piece, so a long option list passes
            // 256 without looping; widening it there cost more tokens than
            // following it (tikzpingus: 12 s at 256, 2 s at 16384).
            site_expansions: 16_384,
            stall_tokens: 50_000_000,
            seconds: 300,
            branch_depth: 8,
            join_tokens: 65_536,
            // Measured on doc/architecture.tex (TikZ), with loop heads and
            // joins by program point: false-error sites 0: 64, 1: 22,
            // 2: 39, 3: 51, 8: 66; time 1: 5s, 8: 31s.  Deeper splits run
            // pgfmath's parser over more unknown text.
            meaning_splits: 1,
            widen_after: 3,
            meaning_set: 5,
            value_set: 5,
            // A `Token` is 16 bytes, so a million of them is 16 MiB: enough
            // for any argument an expansion really writes, and far below
            // `memory`.
            expansion_tokens: 1_048_576,
            conditional_tokens: 1_048_576,
            held_tokens: 4_000_000,
            expansion_stack: 4_000,
            vertices: 400_000,
            facts: 400_000,
            trace: 200_000,
            save_stack: 2_000_000,
            // Enough for the kernel and a large document several times over,
            // and small enough that a runaway run dies before the machine
            // starts swapping.
            memory: bytesize::ByteSize::gib(4),
            threads: 0,
            cache_size: bytesize::ByteSize::gib(1),
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
}

impl Default for PluginSwitches {
    fn default() -> PluginSwitches {
        PluginSwitches { magic: true, tools: true, preload: true, discovery: true }
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

/// `cache_index:` in `satex.yaml`.  A package cache holds the state a
/// package leaves when it is loaded right after `\documentclass{class}`;
/// a document that loads it in the same state starts from the cache.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct CacheIndex {
    /// The packages `satex cache build` prepares.
    pub packages: Vec<String>,
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
            engines: Vec::new(),
            class: "article".into(),
            auto: true,
            refresh: false,
            building: false,
        }
    }
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
    pub load_packages: bool,
    pub load_classes: bool,
    pub load_inputs: bool,
    pub skip_packages: Vec<String>,
    pub search_paths: Vec<PathBuf>,
    /// Fallback kpathsea search-path variables ([`PathsConfig`]); the
    /// environment and the project's build files are tried first.
    pub paths: PathsConfig,
    pub texmf_roots: Vec<PathBuf>,
    pub texlive_root: Option<PathBuf>,
    pub texlive_year: Option<u32>,
    pub use_kpsewhich: bool,
    /// Whether an unresolved installation may fall back to well-known
    /// system paths (`/usr/local/texlive/...`, `$HOME/texmf`, ...). Off,
    /// together with `use_kpsewhich: false`, makes a run behave as if this
    /// machine had no TeX installation at all, engine primitives only —
    /// for testing that path without uninstalling one.
    pub use_fallback_roots: bool,
    pub load_format: bool,
    pub cache: bool,
    #[serde(default)]
    pub rebuild_format: bool,
    pub cache_dir: Option<PathBuf>,
    /// What `satex cache build` prepares, and whether ordinary runs store
    /// the packages they read.
    #[serde(default)]
    pub cache_index: CacheIndex,
    /// A preamble to interpret as this project's format, the way
    /// `mylatexformat` dumps one.  Without it, a `%&` line or the build
    /// configuration names it.
    #[serde(default)]
    pub preload: Option<PathBuf>,
    pub limits: Limits,
    pub plugins: PluginSwitches,
    /// Macros to expand when reporting a value, such as `today` or `jobname`.
    /// Empty means report what the document wrote.
    pub expand_in_reports: Vec<String>,
    /// Primitives the `primitive-tex-command` rule reports, each with the
    /// LaTeX command to suggest instead; an empty value suggests nothing.
    /// Entries add to what the rule already knows, or override one.
    pub latex_alternatives: std::collections::BTreeMap<String, String>,
    /// Whether a run appends what it could not follow to `gaps_log`, so the
    /// limits met over many documents can be read back later.
    pub log_gaps: bool,
    /// Where that log goes; the cache directory's `gaps.ndjson` by default.
    pub gaps_log: Option<PathBuf>,
    /// Whether a run also prints what it could not follow, so a reader sees
    /// the analysis's limits without asking for them.
    pub report_gaps: bool,
    /// What `\write18` may do in this build.
    pub shell_escape: ShellEscape,
    #[serde(default)]
    pub interaction: Interaction,
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
    /// Whether `unused-definition` also reports a name with no `@` and no
    /// expl3-style `module_function:signature` in it: a package or class
    /// turns this off, since such a name is its public interface, which the
    /// caller — not this run — is the one to use it.
    #[serde(default = "default_true")]
    pub report_public_definitions: bool,
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
            load_packages: true,
            load_classes: true,
            load_inputs: true,
            skip_packages: Vec::new(),
            search_paths: Vec::new(),
            paths: PathsConfig::default(),
            texmf_roots: Vec::new(),
            texlive_root: None,
            texlive_year: None,
            use_kpsewhich: true,
            use_fallback_roots: true,
            load_format: true,
            cache: true,
            rebuild_format: false,
            cache_dir: None,
            cache_index: CacheIndex::default(),
            preload: None,
            limits: Limits::default(),
            plugins: PluginSwitches::default(),
            expand_in_reports: Vec::new(),
            latex_alternatives: std::collections::BTreeMap::new(),
            log_gaps: true,
            gaps_log: None,
            report_gaps: true,
            shell_escape: ShellEscape::Restricted,
            interaction: Interaction::Nonstopmode,
            engine: None,
            kernel: None,
            provider: None,
            platform: None,
            output: None,
            at_letter: false,
            lint_off: vec!["analysis-imprecision".to_string()],
            report_public_definitions: true,
            profile: None,
            profiles: Profiles::default(),
            verbose: 0,
            trace: false,
            trace_lines: None,
            trace_col: 0,
            trace_file: None,
            trace_end_col: u32::MAX,
            trace_steps: None,
            timings: false,
            opaque_conditionals: Vec::new(),
            record_arguments: false,
            depp: Default::default(),
            lsp: LspConfig::default(),
        }
    }
}

/// Which TeX engine's primitive set the run assumes: plain TeX, pdfTeX,
/// LuaTeX or XeTeX. [`crate::plugin::Plugins::engine`] and `\ifpdf`-style
/// tests key on it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    Tex,
    PdfTeX,
    LuaTeX,
    XeTeX,
}

impl Engine {
    /// Every engine satex knows, for `--version`.
    pub const ALL: &'static [Engine] = &[Engine::Tex, Engine::PdfTeX, Engine::LuaTeX, Engine::XeTeX];
    pub fn as_str(self) -> &'static str {
        match self {
            Engine::Tex => "tex",
            Engine::PdfTeX => "pdftex",
            Engine::LuaTeX => "luatex",
            Engine::XeTeX => "xetex",
        }
    }
    /// The program that starts this engine with the LaTeX format, which is
    /// also latexmk's setting for it (latexmk manual, "$pdflatex").
    /// The engine a program name drives: `lualatex` is LuaTeX with the LaTeX
    /// format, and a magic comment names the program.
    pub fn from_program(program: &str) -> Option<Engine> {
        // Editors write `XeLaTeX` too; Windows paths end in `.exe`.
        let program = program.trim().rsplit(['/', '\\']).next()?.to_ascii_lowercase();
        let program = program.strip_suffix(".exe").unwrap_or(&program);
        Some(match program {
            "pdflatex" | "pdftex" => Engine::PdfTeX,
            "lualatex" | "luatex" | "lualatex-dev" => Engine::LuaTeX,
            "xelatex" | "xetex" => Engine::XeTeX,
            "latex" | "tex" => Engine::Tex,
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
            Engine::Tex => "latex",
            Engine::PdfTeX => "pdflatex",
            Engine::LuaTeX => "lualatex",
            Engine::XeTeX => "xelatex",
        }
    }
    /// ε-TeX is in every format in use today.
    pub fn has_etex(self) -> bool {
        true
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
}

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
    Document,
    Package,
    Class,
    Literate,
    Plain,
}

impl Profile {
    /// Every profile satex knows, for `--version` and error messages.
    pub const ALL: &'static [Profile] =
        &[Profile::Document, Profile::Package, Profile::Class, Profile::Literate, Profile::Plain];

    pub fn as_str(self) -> &'static str {
        match self {
            Profile::Document => "document",
            Profile::Package => "package",
            Profile::Class => "class",
            Profile::Literate => "literate",
            Profile::Plain => "plain",
        }
    }

    /// The profile named `name` (`--profile`, `profile:`), or `None` for an
    /// unknown one.
    pub fn parse(name: &str) -> Option<Profile> {
        Profile::ALL.iter().copied().find(|profile| profile.as_str() == name)
    }

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
    /// else `document` if `\documentclass` appears anywhere and `plain` if
    /// it never does.
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
        if source.contains("\\documentclass") { Profile::Document } else { Profile::Plain }
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
             report_public_definitions: false\n\
             lint_off: [analysis-imprecision, unused-label, microtype-available, build-shell-escape-missing, \
             build-shell-escape-unneeded, build-engine-mismatch, build-bibliography-disabled, \
             build-bibliography-unneeded, build-missing-custom-dependency, \
             build-unused-custom-dependency, build-command-placeholders, build-engine-options]\n",
        );
        Profiles {
            document: serde_yaml_ng::Value::Null,
            package: package.clone(),
            class: package.clone(),
            literate: package,
            plain: parse("load_packages: false\nload_classes: false\nlint_off: [analysis-imprecision, unused-label, microtype-available]\n"),
        }
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

impl Config {
    pub fn distribution_request(&self) -> crate::distribution::Request {
        crate::distribution::Request {
            roots: self.texmf_roots.clone(),
            install_root: self.texlive_root.clone(),
            year: self.texlive_year,
            use_kpsewhich: self.use_kpsewhich,
            probe_fallback: self.use_fallback_roots,
            provider: self.provider.or_else(|| {
                self.use_kpsewhich.then(crate::plugin::Provider::detect).flatten()
            }),
            platform: self.platform.unwrap_or_else(crate::plugin::Platform::current),
            cache: self.cache,
            cache_dir: self.cache_dir.clone(),
            // Filled in by the caller, which also has the `Project` this
            // needs to resolve `latexmkrc`/Makefile paths against.
            kpse_env: Vec::new(),
        }
    }
}

impl Config {
    pub fn read(path: &Path) -> Option<Config> {
        let text = std::fs::read_to_string(path).ok()?;
        let mut cfg: Config = serde_yaml_ng::from_str(&text).ok()?;
        cfg.source = Some(path.to_path_buf());
        Some(cfg)
    }

    /// Overrides the option at `path`, the dotted key it has in `satex.yaml`
    /// (`limits.memory`, `cache_index.auto`), with `value` read as YAML.
    pub fn set(&mut self, path: &str, value: &str) -> Result<(), String> {
        use serde_yaml_ng::Value;
        let parsed: Value = serde_yaml_ng::from_str(value).map_err(|e| format!("{path}: {e}"))?;
        let mut tree = serde_yaml_ng::to_value(&*self).map_err(|e| e.to_string())?;
        let mut slot = &mut tree;
        for key in path.split('.') {
            slot = slot
                .as_mapping_mut()
                .and_then(|map| map.get_mut(key))
                .ok_or_else(|| format!("no option `{path}` in {FILE_NAME}"))?;
        }
        *slot = parsed;
        let source = self.source.take();
        *self = serde_yaml_ng::from_value(tree).map_err(|e| format!("{path}: {e}"))?;
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
        if let Some(global) = global_config_file() {
            merge_file(&mut value, &global, &mut files);
        }
        let mut ancestors: Vec<PathBuf> = dir.ancestors().map(|a| a.join(FILE_NAME)).collect();
        ancestors.reverse();
        for candidate in ancestors {
            merge_file(&mut value, &candidate, &mut files);
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
        let overlay = self.profiles.get(profile).clone();
        if !matches!(overlay, serde_yaml_ng::Value::Mapping(_)) {
            self.profile = Some(profile);
            return Ok(());
        }
        let mut value = serde_yaml_ng::to_value(&*self).map_err(|e| e.to_string())?;
        merge_value(&mut value, &overlay);
        let source = self.source.take();
        *self =
            serde_yaml_ng::from_value(value).map_err(|e| format!("profiles.{}: {e}", profile.as_str()))?;
        self.source = source;
        self.profile = Some(profile);
        Ok(())
    }

    /// Merges `path` on top of this config, the same rule
    /// [`Config::discover_layers`] merges each layer with.  Unlike a layer
    /// `discover_layers` finds on its own, a file named this way is meant
    /// to exist: `Err` when it cannot be read or does not fit `Config`.
    pub fn merge(&mut self, path: &Path) -> Result<(), String> {
        let text =
            std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let incoming: serde_yaml_ng::Value =
            serde_yaml_ng::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
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
    dirs::config_dir().map(|dir| dir.join("satex").join(FILE_NAME))
}

/// Merges `path` onto `base` with `merge_value`, and records it in
/// `files` — but only when the result still fits `Config`; a file that does
/// not (an unknown key, a value of the wrong type, unreadable, absent) is
/// left out and `base` is unchanged, so it never blanks out an outer layer.
fn merge_file(base: &mut serde_yaml_ng::Value, path: &Path, files: &mut Vec<PathBuf>) {
    let Ok(text) = std::fs::read_to_string(path) else { return };
    let Ok(incoming) = serde_yaml_ng::from_str::<serde_yaml_ng::Value>(&text) else { return };
    let mut candidate = base.clone();
    merge_value(&mut candidate, &incoming);
    if serde_yaml_ng::from_value::<Config>(candidate.clone()).is_ok() {
        *base = candidate;
        files.push(path.to_path_buf());
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
            if let (Some(Value::Sequence(existing)), Value::Sequence(added)) =
                (base_map.get_mut(&target), value)
            {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_option_is_set_by_its_path_in_the_file() {
        let mut cfg = Config::default();
        cfg.set("limits.trace", "10").unwrap();
        cfg.set("load_inputs", "false").unwrap();
        assert_eq!(cfg.limits.trace, 10);
        assert!(!cfg.load_inputs);
        assert!(cfg.set("limits.bogus", "1").is_err());
        assert!(cfg.set("limits.trace", "many").is_err());
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
        write(&dir, "satex.yaml", "load_packages: false\nskip_packages: [a]\n");
        write(&inner, "satex.yaml", "load_classes: false\nskip_packages: [b]\n");
        let (cfg, files) = Config::discover_layers(&inner);
        // The inner file wins where both set a key, and inherits what only
        // the outer one set; a plain list replaces rather than merging.
        assert!(!cfg.load_packages);
        assert!(!cfg.load_classes);
        assert_eq!(cfg.skip_packages, vec!["b".to_string()]);
        assert_eq!(files, vec![dir.join("satex.yaml"), inner.join("satex.yaml")]);
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
        assert_eq!(cfg.skip_packages, vec!["a".to_string(), "b".to_string()]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_layer_that_does_not_fit_config_is_skipped_without_blanking_the_outer_one() {
        let dir = std::env::temp_dir().join("satex-config-merge-invalid-test");
        let inner = dir.join("project");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&inner).unwrap();
        write(&dir, "satex.yaml", "load_packages: false\n");
        write(&inner, "satex.yaml", "not_a_real_key: 1\n");
        let (cfg, files) = Config::discover_layers(&inner);
        assert!(!cfg.load_packages);
        assert_eq!(files, vec![dir.join("satex.yaml")]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Whether `path` is the file `name` names: a trailing part of the path,
/// with or without the `.tex` extension.
pub fn names_file(path: &str, name: &str) -> bool {
    let path = std::path::Path::new(path);
    path.ends_with(name) || path.ends_with(format!("{name}.tex"))
}
