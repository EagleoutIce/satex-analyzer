//! What surrounds the document: provider, platform, engine, output.
//! These are plugins: defaults detected, overridden in `satex.yaml`.

pub mod bib;
pub mod build;
pub mod depp;
pub mod discovery;
pub mod format;
pub mod l3build;
pub mod lua;
pub mod lua_run;
pub mod magic;
pub mod output;
pub mod pdf;
pub mod platform;
pub mod preload;
pub mod provider;
pub mod tool;

pub use discovery::Discovery;
pub use format::Format;
pub use output::Output;
pub use platform::Platform;
pub use preload::Preload;
pub use provider::Provider;

use crate::config::{Config, Engine};
use crate::project::Project;

/// The kind of thing a plugin describes, for reporting: which field of
/// [`Plugins`] it names and how [`Plugins::source`] was decided.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Provider,
    Platform,
    Engine,
    Kernel,
    Output,
    BuildSystem,
    Tool,
    Magic,
    Format,
    Preload,
    Depp,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Provider => "provider",
            Kind::Platform => "platform",
            Kind::Engine => "engine",
            Kind::Kernel => "kernel",
            Kind::Output => "output",
            Kind::BuildSystem => "build system",
            Kind::Tool => "tools",
            Kind::Magic => "magic comments",
            Kind::Format => "output format",
            Kind::Preload => "preloaded format",
            Kind::Depp => "TeX Live packages",
        }
    }
}

/// Which format the run starts from.  satex interprets the LaTeX2e kernel,
/// `latex.ltx`; a ConTeXt run is only recognized, because its kernel is
/// written for ConTeXt's own Lua layer and satex does not interpret it.
/// ConTeXt support is therefore EXPERIMENTAL: the document is identified and
/// its primitives are known, but nothing the ConTeXt kernel defines is.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kernel {
    #[serde(rename = "latex2e")]
    Latex,
    /// `plain.tex`: Knuth's macros, which satex interprets like the LaTeX
    /// kernel because they are ordinary TeX (The TeXbook, appendix B).
    Plain,
    Context,
    /// No kernel at all: the engine's primitives and nothing else.
    None,
}

impl Kernel {
    /// Every kernel satex knows, for `--version`.
    pub const ALL: &'static [Kernel] = &[Kernel::Latex, Kernel::Plain, Kernel::Context, Kernel::None];

    pub fn as_str(self) -> &'static str {
        match self {
            Kernel::Latex => "latex2e",
            Kernel::Plain => "plain",
            Kernel::Context => "context",
            Kernel::None => "none",
        }
    }

    /// Whether satex interprets this kernel at all, or only recognizes it.
    pub fn is_experimental(self) -> bool {
        self == Kernel::Context
    }

    /// Whether the source is a ConTeXt document.  ConTeXt wraps the body in
    /// `\starttext`/`\stoptext` and loads modules with `\usemodule`
    /// (ConTeXt reference, "Document structure"); none of the three is a
    /// LaTeX2e command, so one of them settles it.
    /// The file this kernel is interpreted from, when there is one.
    pub fn source(self) -> Option<&'static str> {
        match self {
            Kernel::Latex => Some("latex.ltx"),
            Kernel::Plain => Some("plain.tex"),
            Kernel::Context | Kernel::None => None,
        }
    }

    pub fn detect(source: &str) -> Option<Kernel> {
        let context = ["\\starttext", "\\stoptext", "\\startcomponent", "\\usemodule"]
            .iter()
            .any(|marker| source.contains(marker));
        if context {
            return Some(Kernel::Context);
        }
        // `\bye` ends a plain TeX document and is no LaTeX2e command
        // (The TeXbook, chapter 23); a LaTeX document says `\documentclass`.
        let plain = source.contains("\\bye") && !source.contains("\\documentclass");
        plain.then_some(Kernel::Plain)
    }
}

/// Everything decided about a run's surroundings before the document is
/// read: [`Provider`], [`Platform`], the [`Engine`], the [`Kernel`], the
/// [`Output`] target, and what magic comments and build files said about
/// them.
#[derive(Clone, Debug)]
pub struct Plugins {
    pub provider: Option<Provider>,
    pub provider_version: Option<String>,
    pub platform: Platform,
    pub output: Output,
    pub engine: Engine,
    /// The format that engine is started with.
    pub kernel: Kernel,
    /// The program that drives that engine: `pdflatex` is pdfTeX started
    /// with the LaTeX format, and it is what a build system runs.
    pub program: String,
    /// The build systems configured beside the document.
    pub build: Vec<String>,
    /// Auxiliary tools the document's packages imply.
    pub tools: Vec<tool::Tool>,
    /// `% !TeX …` and `% arara:` lines in the source.
    pub magic: Vec<magic::Magic>,
    /// The format file the build starts from, when it is not the stock one.
    pub preload: Option<Preload>,
    /// The TeX Live packages the run reads from, as depp names them, and
    /// the dependency file that should list them.
    pub depp: Option<depp::Depp>,
    /// What the project directory holds, as a walk that honors `.gitignore`
    /// found it.
    pub discovery: Discovery,
    /// How each was decided: configured, detected, or the default.
    pub sources: Vec<(Kind, &'static str)>,
}

impl Plugins {
    /// Configuration first, then what the build system says, then what the
    /// machine has; every run therefore states which of the three decided.
    pub fn resolve(cfg: &Config, project: &Project) -> Plugins {
        let mut sources = Vec::new();
        let mut note = |kind, how| sources.push((kind, how));

        let (provider, provider_version) = match cfg.provider {
            Some(provider) => {
                note(Kind::Provider, "configured");
                (Some(provider), None)
            }
            None => {
                let detected = cfg.use_kpsewhich.then(Provider::detect).flatten();
                note(Kind::Provider, if detected.is_some() { "detected" } else { "none found" });
                let cache =
                    cfg.cache.then(|| cfg.cache_dir.clone().or_else(crate::format::default_cache_dir)).flatten();
                let version = detected.and_then(|p| p.version(cache.as_deref()));
                (detected, version)
            }
        };

        let platform = match cfg.platform {
            Some(platform) => {
                note(Kind::Platform, "configured");
                platform
            }
            None => {
                note(Kind::Platform, "this machine");
                Platform::current()
            }
        };

        let engine = match cfg.engine {
            Some(engine) => {
                note(Kind::Engine, "configured");
                engine
            }
            None => match project.engine() {
                Some(engine) => {
                    note(Kind::Engine, "build configuration");
                    engine
                }
                // Tectonic drives XeTeX (Tectonic documentation, "The TeX
                // engine"); everything else defaults to pdfTeX, as latexmk
                // does.
                None if provider == Some(Provider::Tectonic) => {
                    note(Kind::Engine, "provider");
                    Engine::XeTeX
                }
                None => {
                    note(Kind::Engine, "default");
                    Engine::PdfTeX
                }
            },
        };

        let output = match cfg.output {
            Some(output) => {
                note(Kind::Output, "configured");
                output
            }
            None => match project.setting("pdf_mode").and_then(Output::from_pdf_mode) {
                Some(output) => {
                    note(Kind::Output, "build configuration");
                    output
                }
                // Every engine in use writes PDF by default; `tex` itself
                // writes DVI.
                None => {
                    note(Kind::Output, "engine");
                    match engine {
                        Engine::Tex => Output::Dvi,
                        _ => Output::Pdf,
                    }
                }
            },
        };

        // latexmk names the program per engine (`$pdflatex`, `$lualatex`,
        // `$xelatex`, `$latex`); its first word is the binary it runs.
        let program = project
            .setting(engine.program())
            .and_then(|command| command.split_whitespace().next())
            .unwrap_or(engine.program())
            .to_string();
        let build = project.describe();
        if !build.is_empty() {
            note(Kind::BuildSystem, "found beside the document");
        }
        let kernel = match cfg.kernel {
            Some(kernel) => {
                note(Kind::Kernel, "configured");
                kernel
            }
            None => {
                note(Kind::Kernel, "default");
                Kernel::Latex
            }
        };
        Plugins {
            provider,
            provider_version,
            platform,
            output,
            engine,
            kernel,
            program,
            build,
            tools: Vec::new(),
            magic: Vec::new(),
            preload: None,
            depp: None,
            discovery: Discovery::default(),
            sources,
        }
    }

    /// What the source itself says: magic comments, and — since they name the
    /// program — the engine they ask for.
    pub fn read_magic(&mut self, source: &str) {
        self.magic = magic::scan(source);
        let Some(program) =
            self.magic.iter().find(|m| m.key.eq_ignore_ascii_case("TeX program")).map(|m| m.value.clone())
        else {
            return;
        };
        if let Some(engine) = Engine::from_program(&program) {
            self.engine = engine;
            self.program = program;
            self.sources.retain(|(kind, _)| *kind != Kind::Engine);
            self.sources.push((Kind::Engine, "magic comment"));
        }
    }

    /// What the build configuration says about this file in particular:
    /// l3build typesets its documents with `typesetexe` and checks
    /// everything else with `stdengine` (l3build manual, "Variables").  A
    /// configured engine wins, and a magic comment read later does too.
    pub fn read_build(&mut self, cfg: &Config, project: &Project, file: Option<&std::path::Path>) {
        if cfg.engine.is_some() || project.setting("pdf_mode").is_some() {
            return;
        }
        let Some(engine) = project.l3build.as_ref().and_then(|config| config.engine_for(file)) else {
            return;
        };
        self.engine = engine;
        self.program = engine.program().to_string();
        self.sources.retain(|(kind, _)| *kind != Kind::Engine);
        self.sources.push((Kind::Engine, "build configuration"));
    }

    /// Which format the source itself asks for.  `% !TeX program = context`
    /// names it outright; otherwise the ConTeXt commands in the body do.
    /// A configured kernel wins over both.
    pub fn read_kernel(&mut self, cfg: &Config, source: &str) {
        if cfg.kernel.is_some() {
            return;
        }
        let named = self
            .magic
            .iter()
            .find(|m| m.key.eq_ignore_ascii_case("TeX program"))
            .map(|m| m.value.trim().to_ascii_lowercase());
        let (kernel, how) = match named.as_deref() {
            Some("context" | "contextjit" | "luametatex" | "mtxrun") => (Kernel::Context, "magic comment"),
            _ => match Kernel::detect(source) {
                Some(kernel) => (kernel, "read from the source"),
                None => return,
            },
        };
        self.kernel = kernel;
        self.sources.retain(|(kind, _)| *kind != Kind::Kernel);
        self.sources.push((Kind::Kernel, how));
    }

    /// The format file this run starts from: the `%&` line in the source, or
    /// the `-fmt=` the build configuration passes to the engine.
    pub fn read_preload(
        &mut self,
        source: &str,
        project: &Project,
        base: &std::path::Path,
        configured: Option<&std::path::Path>,
    ) {
        if let Some(path) = configured {
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or("format");
            self.preload =
                Some(Preload { name: name.to_string(), how: "configured", source: Some(path.to_path_buf()) });
            return;
        }
        let command = project.setting(self.engine.program()).map(str::to_string);
        self.preload = Preload::detect(source, command.as_deref(), base, &self.discovery);
    }

    /// The auxiliary tools a build runs: what the packages imply, what the
    /// build configuration names, and what the output target needs.
    pub fn read_tools(&mut self, loads: &[(String, Vec<String>)], project: &Project) {
        let add = |tool: tool::Tool, tools: &mut Vec<tool::Tool>| {
            if !tools.contains(&tool) {
                tools.push(tool);
            }
        };
        for (name, options) in loads {
            if let Some(tool) = tool::Tool::implied_by(name, options) {
                add(tool, &mut self.tools);
            }
        }
        // latexmk names the programs it will run (latexmk manual, "List of
        // configuration variables").
        for file in &project.files {
            for (key, value) in &file.settings {
                for tool in tool::Tool::all() {
                    if key == tool.as_str() || value.contains(tool.as_str()) {
                        add(*tool, &mut self.tools);
                    }
                }
            }
        }
        if self.output == Output::PostScript {
            add(tool::Tool::Dvips, &mut self.tools);
            add(tool::Tool::Ps2pdf, &mut self.tools);
        }
    }

    /// How a given plugin was decided.
    pub fn source(&self, kind: Kind) -> &'static str {
        self.sources.iter().find(|(k, _)| *k == kind).map(|(_, how)| *how).unwrap_or("default")
    }

    /// One line per plugin, for `summary` and `query plugins`.
    pub fn rows(&self) -> Vec<(Kind, String, &'static str)> {
        let source = |kind: Kind| self.source(kind);
        let provider = match (&self.provider, &self.provider_version) {
            (Some(provider), Some(version)) => format!("{} ({version})", provider.as_str()),
            (Some(provider), None) => provider.as_str().to_string(),
            (None, _) => "none".to_string(),
        };
        let mut rows = vec![
            (Kind::Provider, provider, source(Kind::Provider)),
            (Kind::Platform, self.platform.as_str().to_string(), source(Kind::Platform)),
            (Kind::Engine, format!("{} ({} primitives)", self.program, self.engine.as_str()), source(Kind::Engine)),
            (
                Kind::Kernel,
                match self.kernel.is_experimental() {
                    true => format!("{} (experimental)", self.kernel.as_str()),
                    false => self.kernel.as_str().to_string(),
                },
                source(Kind::Kernel),
            ),
            (Kind::Output, self.output.as_str().to_string(), source(Kind::Output)),
            (
                Kind::BuildSystem,
                if self.build.is_empty() { "none".into() } else { self.build.join(", ") },
                source(Kind::BuildSystem),
            ),
            (
                Kind::Tool,
                if self.tools.is_empty() {
                    "none".into()
                } else {
                    self.tools.iter().map(|t| t.as_str()).collect::<Vec<_>>().join(", ")
                },
                "implied by the packages",
            ),
            (
                Kind::Preload,
                match &self.preload {
                    Some(preload) => match &preload.source {
                        Some(path) => format!("{} ({})", preload.name, path.display()),
                        None => format!("{} (no source found)", preload.name),
                    },
                    None => "none".into(),
                },
                self.preload.as_ref().map_or("default", |preload| preload.how),
            ),
            (
                Kind::Magic,
                if self.magic.is_empty() {
                    "none".into()
                } else {
                    self.magic.iter().map(|m| format!("{} = {}", m.key, m.value)).collect::<Vec<_>>().join(", ")
                },
                "read from the source",
            ),
        ];
        if let Some(found) = &self.depp {
            let how = if found.loaded.is_some() { "depp is loaded" } else { "dependency file" };
            rows.push((Kind::Depp, found.describe(), how));
        }
        rows
    }
}
