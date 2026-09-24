use std::fmt::Write as _;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};

use satex::cmd::{Command, Context, Format, Output};
use satex::config::Config;
use satex::machine::{Analysis, Machine};
use satex::query;
use satex::render::{self, Links};

#[derive(Parser)]
#[command(name = "satex", about = "Static analyzer for TeX and LaTeX")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// The version, the plugins this build offers, and the configuration in
    /// effect here.
    #[arg(short = 'V', long, global = true)]
    version: bool,

    /// The document to read. Without it, satex looks for one root document
    /// in the current directory, or reads stdin if that is not a terminal.
    #[arg(short, long, global = true, value_name = "FILE", help_heading = "Input")]
    file: Option<PathBuf>,

    /// An extra config file, merged on top of every discovered satex.yaml
    /// (built-in defaults, the global config, the ones from the filesystem
    /// root down to the document's directory).
    #[arg(long, global = true, value_name = "FILE", help_heading = "Input")]
    config: Option<PathBuf>,

    /// Ignore every satex.yaml, discovered or named with --config —
    /// built-in defaults and `--set` only.
    #[arg(long, global = true, help_heading = "Input")]
    no_config: bool,

    /// Which defaults to apply (usually auto-detected): document, package, class,
    /// literate or plain.
    #[arg(long, global = true, value_name = "NAME", help_heading = "Input")]
    profile: Option<String>,

    /// Overrides an option of `satex.yaml` by its dotted path, the value
    /// written as there: `--set limits.memory=8GiB --set load_inputs=false`.
    /// Repeatable; the flags below win over it.
    #[arg(long = "set", global = true, value_name = "PATH=VALUE", help_heading = "Input")]
    set: Vec<String>,

    /// Do not load packages; analyze the document and the kernel only.
    #[arg(long, global = true, help_heading = "Input")]
    no_packages: bool,

    /// Do not load the document class.
    #[arg(long, global = true, help_heading = "Input")]
    no_classes: bool,

    /// Stop the interpreter after this many expansion steps.
    #[arg(long, global = true, value_name = "N", help_heading = "Limits")]
    max_steps: Option<u64>,

    /// What this run may hold at once, as written by `ulimit`: 512M, 4GiB, 0
    /// for no limit.
    #[arg(long, global = true, value_name = "SIZE", help_heading = "Limits")]
    max_memory: Option<bytesize::ByteSize>,

    /// Threads used for program discovery
    #[arg(long, global = true, value_name = "N", help_heading = "Limits")]
    threads: Option<usize>,

    /// Report progress: files read (-v), every 100k tokens (-vv), every
    /// expansion (-vvv).
    #[arg(short, long, global = true, action = clap::ArgAction::Count, help_heading = "Output")]
    verbose: u8,

    /// Output format: text, json, csv, markdown, dot, github, sarif or lsp.
    #[arg(long, global = true, value_enum, default_value = "text", help_heading = "Output")]
    format: Format,

    /// When to color the output.
    #[arg(long, global = true, value_enum, default_value = "auto", help_heading = "Output")]
    color: Color,

    /// Turn on terminal hyperlinks from names to their source position, even
    /// when auto-detection would leave them off.
    #[arg(long, global = true, overrides_with = "no_links", help_heading = "Output")]
    links: bool,
    /// Turn off terminal hyperlinks, even when auto-detection would turn
    /// them on.
    #[arg(long, global = true, help_heading = "Output")]
    no_links: bool,

    /// Print one line of resource accounting (files, tokens, memory, time) to
    /// stderr after the run.
    #[arg(long, global = true, help_heading = "Output")]
    stats: bool,

    /// Print a per-phase timing breakdown to stderr after the run.
    #[arg(long, global = true, help_heading = "Output")]
    timings: bool,
}

impl Cli {
    /// The subcommand, once `--version` has taken the command-less form.
    fn command(&self) -> &Command {
        self.command.as_ref().expect("a subcommand, which clap has already checked for")
    }

    fn link_policy(&self, terminal: bool) -> Links {
        if self.no_links {
            return Links(false);
        }
        Links(self.links || (self.color != Color::Never && terminal))
    }
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Color {
    /// Color when stdout is a terminal, off otherwise.
    Auto,
    /// Color even when stdout is not a terminal, for example when piped.
    Always,
    /// Never color the output.
    Never,
}

impl From<Color> for anstream::ColorChoice {
    fn from(choice: Color) -> Self {
        match choice {
            Color::Auto => anstream::ColorChoice::Auto,
            Color::Always => anstream::ColorChoice::Always,
            Color::Never => anstream::ColorChoice::Never,
        }
    }
}

/// satex counts what it allocates so `limits.memory` can stop a run before
/// the machine has to.
#[global_allocator]
static ALLOCATOR: satex::budget::Counted = satex::budget::Counted;

/// What `satex explain` analyzes when it has no document of its own: the
/// smallest one that still loads the kernel and a class, so a name from
/// either has something to be explained in.
const EMPTY_DOCUMENT: &str = "\\documentclass{article}\n\\begin{document}\n\\end{document}\n";

fn main() {
    if let Err(message) = run() {
        eprintln!("satex: {message}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut cli = Cli::parse();
    anstream::ColorChoice::from(cli.color).write_global();

    if cli.version || cli.command.is_none() {
        if !matches!(cli.format, Format::Text | Format::Json) {
            return Err(format!(
                "`--format {}` does not apply to `--version`; try text or json",
                cli.format.as_str()
            ));
        }
        let (cfg, layers) = configure(&cli, None)?;
        let mut out = anstream::stdout().lock();
        let text = match cli.format {
            Format::Json => format!(
                "{}\n",
                serde_json::to_string_pretty(&overview_json(&cfg, &satex::project::Project::discover(&base_dir(cli.file.as_deref())), &layers)).map_err(|e| e.to_string())?
            ),
            _ => overview_text(&cfg, &layers),
        };
        let _ = write!(out, "{text}");
        return Ok(());
    }
    cli.command().check_format(cli.format)?;
    // `explain` and `scope` ask what a name means rather than what a
    // document does, so with nothing to read — no `-f`, nothing piped, no
    // single root found — they still have something to run: the smallest
    // LaTeX document there is, which gives the kernel and, once
    // `\documentclass` loads it, the class something to define `\section`
    // and its like from.
    let explaining =
        matches!(cli.command, Some(satex::cmd::Command::Explain { .. } | satex::cmd::Command::Scope { .. }));
    if cli.file.is_none() && cli.command().needs_source() && io::stdin().is_terminal() {
        cli.file = input_for(&std::env::current_dir().unwrap_or_default(), explaining)?;
    }
    let mut source = if cli.file.is_none() && explaining && io::stdin().is_terminal() {
        // Interactive, nothing named, nothing piped in: reading stdin would
        // just hang waiting for input that is never coming.
        EMPTY_DOCUMENT.to_string()
    } else if cli.command().needs_source() {
        read_source(cli.file.as_deref())?
    } else {
        String::new()
    };
    if cli.file.is_none() && explaining && source.trim().is_empty() {
        note_minimal_document("nothing was piped in");
        source = EMPTY_DOCUMENT.to_string();
    }
    let (cfg, _layers) = configure(&cli, Some(&source))?;
    if let Some(satex::cmd::Command::Lsp { port, host }) = &cli.command {
        return satex::cmd::lsp::run(&cfg, cli.file.as_deref(), *port, host.as_deref());
    }
    if let Some(satex::cmd::Command::Cache { action: Some(action) }) = &cli.command {
        let mut out = anstream::stdout().lock();
        let json = cli.format == Format::Json;
        return match action {
            satex::cmd::CacheAction::Build { packages, refresh, engines, preamble } => {
                let engines = engines
                    .iter()
                    .map(|name| {
                        satex::config::Engine::from_program(name).ok_or_else(|| format!("unknown engine `{name}`"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                satex::cmd::cache::build::run(&cfg, *refresh, packages, &engines, preamble.as_deref(), json, &mut out)
            }
            satex::cmd::CacheAction::Clear => satex::cmd::cache::clear(&cfg, json, &mut out),
            satex::cmd::CacheAction::Prune => satex::cmd::cache::prune(&cfg, json, &mut out),
        };
    }
    if let Some(satex::cmd::Command::Query { request: Some(request), .. }) = &cli.command {
        let mut out = anstream::stdout().lock();
        return satex::cmd::query::run_request(request, &cfg, cli.file.as_deref(), &mut out).map(drop);
    }

    let started = std::time::Instant::now();
    let analysis = if cli.command().needs_analysis() {
        Machine::analyze(&source, cli.file.as_deref(), &cfg)
    } else {
        Analysis::empty()
    };
    if cli.stats {
        anstream::eprintln!("{}", analysis.stats(started.elapsed()));
    }
    if cfg.timings {
        anstream::eprint!("{}", render::timings(&analysis, cli.link_policy(io::stderr().is_terminal())));
    }
    log_gaps(&analysis, &cfg);
    // What the run could not follow reads as a note on the answer, so it goes
    // after it.
    let unfollowed = match cfg.report_gaps && cli.format.is_text() {
        true => render::gaps(&analysis),
        false => String::new(),
    };
    let links = cli.link_policy(io::stdout().is_terminal());
    let context = Context { analysis: &analysis, source: &source, format: cli.format, links };
    let mut out = anstream::stdout().lock();
    let output = cli.command().run(&context, &mut out)?;
    if matches!(cli.command, Some(Command::Summary { .. })) && cli.format.is_text() {
        let project = satex::project::Project::discover(&base_dir(cli.file.as_deref()));
        let _ = write!(out, "{}", environment_text(&cfg, &project));
    }
    // `lint` already says what the analysis could not follow,
    // in a form that fits its own findings; the generic footer would only
    // repeat that in a second, unrelated shape.
    let owns_gaps = (cli.format.is_text() && matches!(output, Output::Lint(_)))
        || cli.command().fixes();
    match output {
        Output::Done => {}
        Output::Records(records) => {
            let _ = write!(out, "{}", rendered(&records, &cli, links));
        }
        Output::Detail(records) => {
            let text = match cli.format.is_text() {
                true => render::fields(&records, links),
                false => rendered(&records, &cli, links),
            };
            let _ = write!(out, "{text}");
        }
        Output::Lint(records) => {
            let text = match cli.format.is_text() {
                true => render::lint(&records, links),
                false => rendered(&records, &cli, links),
            };
            let _ = write!(out, "{text}");
        }
        Output::Partial(records, hint) => {
            let _ = write!(out, "{}", rendered(&records, &cli, links));
            if cli.format.is_text() {
                let _ = writeln!(out, "{}", render::footer(&hint));
            }
        }
        Output::DetailPartial(records, hint) => {
            let text = match cli.format.is_text() {
                true => render::fields(&records, links),
                false => rendered(&records, &cli, links),
            };
            let _ = write!(out, "{text}");
            if cli.format.is_text() {
                let _ = writeln!(out, "{}", render::footer(&hint));
            }
        }
    }
    if !owns_gaps {
        let _ = write!(out, "{unfollowed}");
    }
    Ok(())
}

/// The profile in force for this run: `--profile`, then `profile:` already
/// settled by the discovered `satex.yaml` layers, then
/// [`satex::config::Profile::detect`] on `source` — or, when `configure` is
/// called before there is one (`--version`, with no `-f` read yet), a light
/// read of `cli.file` just for the sniff.
fn resolve_profile(cli: &Cli, cfg: &Config, source: Option<&str>) -> Result<satex::config::Profile, String> {
    use satex::config::Profile;
    if let Some(name) = &cli.profile {
        return Profile::parse(name)
            .ok_or_else(|| format!("unknown profile `{name}`; try document, package, class, literate or plain"));
    }
    if let Some(profile) = cfg.profile {
        return Ok(profile);
    }
    let owned;
    let text = match source {
        Some(text) => text,
        None => {
            owned = cli.file.as_deref().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
            &owned
        }
    };
    Ok(Profile::detect(cli.file.as_deref(), text))
}

/// `cfg`, and the files merged into it, in merge order: built-in defaults,
/// the global user config, every `satex.yaml` from the filesystem root down
/// to the document's directory, then the chosen profile's block, then
/// `--config` on top of all of that. `--no-config` skips every file,
/// `--set` (applied by the caller) is not a file and so is never in this
/// list.
fn configure(cli: &Cli, source: Option<&str>) -> Result<(Config, Vec<PathBuf>), String> {
    if cli.no_config && cli.config.is_some() {
        return Err("`--no-config` and `--config` contradict each other".into());
    }
    let (mut cfg, mut layers) = if cli.no_config {
        (Config::default(), Vec::new())
    } else {
        Config::discover_layers(&base_dir(cli.file.as_deref()))
    };
    let profile = resolve_profile(cli, &cfg, source)?;
    cfg.apply_profile(profile)?;
    if let Some(path) = &cli.config {
        cfg.merge(path)?;
        layers.push(path.clone());
    }
    for assignment in &cli.set {
        let (path, value) = assignment
            .split_once('=')
            .ok_or_else(|| format!("--set {assignment}: expected PATH=VALUE"))?;
        cfg.set(path.trim(), value)?;
    }
    cfg.load_packages &= !cli.no_packages;
    cfg.load_classes &= !cli.no_classes;
    if let Some(memory) = cli.max_memory {
        cfg.limits.memory = memory;
    }
    if let Some(threads) = cli.threads {
        cfg.limits.threads = threads.max(1);
    }
    if let Some(steps) = cli.max_steps {
        cfg.limits.steps = steps;
    }
    cfg.verbose = cli.verbose;
    cfg.trace |= cli.command.as_ref().is_some_and(Command::wants_trace);
    if let Some(Command::Trace { lines, steps, .. }) = &cli.command {
        cfg.trace_lines = lines.as_deref().map(range).transpose()?.map(|(a, b)| (a.min(u32::MAX.into()) as u32, b.min(u32::MAX.into()) as u32));
        cfg.trace_steps = steps.as_deref().map(range).transpose()?;
    }
    cfg.record_arguments |= cli.command.as_ref().is_some_and(Command::wants_arguments);
    cfg.timings |= cli.timings;
    if let Some(command) = &cli.command {
        cfg.opaque_conditionals.extend(command.opaque_conditionals());
    }
    satex::budget::set_limit(cfg.limits.memory.as_u64());
    Ok((cfg, layers))
}

/// Every plugin kind this build offers and what it can detect, shared by the
/// text and json forms of `satex --version`.
fn plugin_rows() -> Vec<(satex::plugin::Kind, Vec<String>)> {
    use satex::plugin::{tool::Tool, Format as OutputFormat, Kind, Output, Platform, Provider};
    vec![
        (Kind::Provider, Provider::ALL.iter().map(|p| p.as_str().to_string()).collect()),
        (Kind::Platform, Platform::ALL.iter().map(|p| p.as_str().to_string()).collect()),
        (
            Kind::Engine,
            satex::config::Engine::ALL.iter().map(|e| e.as_str().to_string()).collect(),
        ),
        (
            Kind::Kernel,
            satex::plugin::Kernel::ALL.iter().map(|k| k.as_str().to_string()).collect(),
        ),
        (Kind::Output, Output::ALL.iter().map(|o| o.as_str().to_string()).collect()),
        (Kind::BuildSystem, {
            let mut systems: Vec<String> =
                satex::project::BUILD_FILES.iter().map(|(_, tool)| tool.to_string()).collect();
            systems.dedup();
            systems.push("arara".into());
            systems.push("l3build".into());
            systems
        }),
        (Kind::Tool, Tool::all().iter().map(|t| t.as_str().to_string()).collect()),
        (
            Kind::Magic,
            ["% !TeX", "% !BIB", "% arara:"].iter().map(|m| m.to_string()).collect(),
        ),
        (Kind::Format, OutputFormat::ALL.iter().map(|f| f.as_str().to_string()).collect()),
        (
            Kind::Preload,
            ["%& line", "-fmt=", "configured"].iter().map(|m| m.to_string()).collect(),
        ),
        (Kind::Depp, vec!["depp".to_string(), "DEPENDS.txt".to_string()]),
    ]
}

fn discovery_list(cfg: &Config) -> Vec<String> {
    if cfg.plugins.discovery {
        satex::plugin::Discovery::honors().iter().map(|s| s.to_string()).collect()
    } else {
        Vec::new()
    }
}

/// TEXINPUTS and the other kpathsea search-path variables, resolved for
/// `cfg`/`project` against the current directory.
fn effective_paths(cfg: &Config, project: &satex::project::Project) -> Vec<satex::paths::Effective> {
    satex::paths::effective_all(project, &cfg.paths, &project.root)
}

/// What TeX installation `cfg` resolves to here, without indexing its files:
/// "no installation" when nothing was found (`use_kpsewhich: false` and
/// `use_fallback_roots: false` simulate that on a machine that has one), or
/// how many trees and by what means otherwise.
fn installation_summary(cfg: &Config, project: &satex::project::Project) -> String {
    let mut request = cfg.distribution_request();
    request.kpse_env = satex::paths::env_vars(&effective_paths(cfg, project));
    let (roots, discovery) = satex::distribution::locate(&request);
    if roots.is_empty() {
        "no installation".to_string()
    } else {
        format!("{} tree{} via {}", roots.len(), if roots.len() == 1 { "" } else { "s" }, discovery.as_str())
    }
}

/// What `satex --version` prints: the build, every plugin it can use, and
/// the configuration this directory gives it.
/// Where the run looked: the profile, the TeX installation and the search paths.
fn environment_text(cfg: &Config, project: &satex::project::Project) -> String {
    let mut out = String::new();
    let row = |out: &mut String, name: &str, value: &str| {
        let _ = writeln!(out, "  {}  {}", render::label(&format!("{name:12}")), value);
    };
    let _ = writeln!(out, "\n{}", render::heading("environment"));
    row(&mut out, "profile", cfg.profile.map(satex::config::Profile::as_str).unwrap_or("document"));
    row(&mut out, "installation", &installation_summary(cfg, project));
    let mut unset = Vec::new();
    for effective in effective_paths(cfg, project) {
        if effective.raw.is_empty() {
            unset.push(effective.variable);
            continue;
        }
        let dirs = effective.dirs.len();
        let value = format!("{} ({}, {dirs} {})", effective.raw, effective.source.as_str(), if dirs == 1 { "dir" } else { "dirs" });
        row(&mut out, effective.variable, &value);
    }
    if !unset.is_empty() {
        row(&mut out, "not set", &unset.join(", "));
    }
    out
}

fn overview_text(cfg: &Config, layers: &[PathBuf]) -> String {
    let names = |items: &[String]| items.join(", ");
    let commit = match env!("SATEX_COMMIT") {
        "" => String::new(),
        commit => format!(" ({commit})"),
    };
    let rows = plugin_rows();
    let total: usize = rows.iter().map(|(_, items)| items.len()).sum();
    let mut out = format!(
        "satex {}{commit}, built {}\n\n{}\n",
        env!("CARGO_PKG_VERSION"),
        env!("SATEX_BUILD_TIME"),
        render::heading(&format!("plugins: {total} in {} kinds", rows.len()))
    );
    let width = rows.iter().map(|(kind, _)| kind.as_str().len()).max().unwrap_or(0);
    for (kind, items) in rows {
        let padded = format!("{:width$}", kind.as_str());
        let _ = writeln!(out, "  {}  {:>2}  {}", render::label(&padded), items.len(), names(&items));
    }
    let discovery = discovery_list(cfg);
    let _ = writeln!(
        out,
        "  {}  {}",
        render::label(&format!("{:width$}", "discovery")),
        if discovery.is_empty() { "off".to_string() } else { discovery.join(", ") }
    );
    let _ = writeln!(out, "\n{}", render::heading("configuration"));
    if layers.is_empty() {
        let _ = writeln!(out, "  built-in defaults; write satex.yaml to change them");
    } else {
        for path in layers {
            let full = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            let _ = writeln!(out, "  {}", full.display());
        }
    }
    out
}

/// The same as [`overview_text`], structured for `--format json`.
fn overview_json(cfg: &Config, project: &satex::project::Project, layers: &[PathBuf]) -> serde_json::Value {
    let commit = env!("SATEX_COMMIT");
    let plugins: serde_json::Map<String, serde_json::Value> = plugin_rows()
        .into_iter()
        .map(|(kind, items)| (kind.as_str().to_string(), serde_json::json!(items)))
        .collect();
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "commit": if commit.is_empty() { None } else { Some(commit) },
        "built": env!("SATEX_BUILD_TIME"),
        "plugin_count": plugins.values().filter_map(|items| items.as_array()).map(Vec::len).sum::<usize>(),
        "plugins": plugins,
        "discovery": discovery_list(cfg),
        "configuration": layers.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "installation": installation_summary(cfg, project),
        "profile": cfg.profile.map(|p| p.as_str()),
        "paths": effective_paths(cfg, project).into_iter().map(|e| serde_json::json!({
            "variable": e.variable,
            "value": e.raw,
            "source": e.source.as_str(),
            "dirs": e.dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

/// Append what this run could not follow to the gap log: one JSON object per
/// cause, with the document it came from, so the limits met across many
/// documents accumulate in one file.  `log_gaps: false` turns it off.
fn log_gaps(analysis: &Analysis, cfg: &Config) {
    if !cfg.log_gaps {
        return;
    }
    let records = query::run(analysis, query::Query::Gaps, &query::Filter::Always);
    if records.is_empty() {
        return;
    }
    let Some(path) = cfg
        .gaps_log
        .clone()
        .or_else(|| satex::format::default_cache_dir().map(|dir| dir.join("gaps.ndjson")))
    else {
        return;
    };
    let document = analysis.file_name(analysis.main_file).to_string();
    let mut text = String::new();
    for mut record in records {
        record.insert("document".into(), serde_json::json!(document));
        if let Ok(line) = serde_json::to_string(&record) {
            text.push_str(&line);
            text.push('\n');
        }
    }
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) =
        std::fs::OpenOptions::new().create(true).append(true).open(&path)
    {
        let _ = file.write_all(text.as_bytes());
    }
}

fn rendered(records: &[query::Record], cli: &Cli, links: Links) -> String {
    cli.format.render(records, links, env!("CARGO_PKG_VERSION"))
}

/// The document to read when none is named: the one root in `dir`; for
/// `explain` and `scope`, `None` (the smallest document) when there is not
/// exactly one.
fn input_for(dir: &Path, explaining: bool) -> Result<Option<PathBuf>, String> {
    match discover_input(dir) {
        Ok(found) => Ok(Some(found)),
        Err(why) if explaining => {
            note_minimal_document(&why);
            Ok(None)
        }
        Err(why) => Err(why),
    }
}

/// `explain`/`scope` fell back to [`EMPTY_DOCUMENT`]: said once, on stderr,
/// so a name that means nothing there is not a silent surprise.
fn note_minimal_document(why: &str) {
    anstream::eprintln!("satex: no document named ({why}); analyzing the smallest LaTeX document instead");
}

/// The one root document in `dir`.  Several candidates are never guessed
/// between, and none is an error: either way, `-f` names the root.
fn discover_input(dir: &Path) -> Result<PathBuf, String> {
    let found = satex::plugin::discovery::scan(dir);
    let rivals = found.rival_roots();
    if rivals.len() > 1 {
        const SHOWN: usize = 5;
        let mut names: Vec<String> = rivals.iter().take(SHOWN).map(|p| p.display().to_string()).collect();
        if rivals.len() > SHOWN {
            names.push(format!("… ({} more)", rivals.len() - SHOWN));
        }
        return Err(format!(
            "{} could each be the root: name one with -f, or add a `% !TeX root` line to the source",
            names.join(", ")
        ));
    }
    found.root_document().ok_or_else(|| format!("no root document in {}: name one with -f", dir.display()))
}

fn read_source(path: Option<&Path>) -> Result<String, String> {
    match path {
        Some(path) => std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display())),
        None => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf).map_err(|e| e.to_string())?;
            Ok(buf)
        }
    }
}

fn base_dir(path: Option<&Path>) -> PathBuf {
    path.and_then(|p| p.parent().map(Path::to_path_buf))
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
}

/// `FROM:TO`, either end left out for open: `10:`, `:20`, `15`.
fn range(text: &str) -> Result<(u64, u64), String> {
    let bad = || format!("`{text}` is no range: use FROM:TO");
    let number = |t: &str, open: u64| if t.trim().is_empty() { Ok(open) } else { t.trim().parse().map_err(|_| bad()) };
    let (from, to) = match text.split_once(':') {
        Some((from, to)) => (number(from, 0)?, number(to, u64::MAX)?),
        None => number(text, 0).map(|n| (n, n))?,
    };
    match from <= to {
        true => Ok((from, to)),
        false => Err(format!("`{text}` is an empty range: FROM is past TO")),
    }
}

#[cfg(test)]
mod tests {
    use super::input_for;

    fn dir(tag: &str, files: &[&str]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("satex-root-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in files {
            std::fs::write(dir.join(name), "\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n").unwrap();
        }
        dir
    }

    #[test]
    fn an_ambiguous_or_missing_root_is_never_guessed() {
        let two = dir("two", &["a.tex", "b.tex"]);
        let why = input_for(&two, false).unwrap_err();
        assert!(why.contains("-f") && why.contains("a.tex") && why.contains("b.tex"), "{why}");
        assert_eq!(input_for(&two, true), Ok(None));
        let none = dir("none", &[]);
        assert!(input_for(&none, false).unwrap_err().contains("-f"));
        assert_eq!(input_for(&none, true), Ok(None));
        let one = dir("one", &["a.tex"]);
        assert_eq!(input_for(&one, false), Ok(Some(one.join("a.tex"))));
        assert_eq!(input_for(&one, true), Ok(Some(one.join("a.tex"))));
    }
}
