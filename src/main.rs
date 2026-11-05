use std::fmt::Write as _;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, ValueEnum};

use satex::analysis::machine::{Analysis, Machine};
use satex::cli::{CacheAction, Command, Context, Format, Output};
use satex::project::config::{Config, Engine};
use satex::queries::query;
use satex::util::render::{self, Links};

#[derive(Parser)]
#[command(name = "satex", about = "Static analyzer for TeX and LaTeX")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// The version, the plugins this build offers, and the configuration in
    /// effect here.
    #[arg(short = 'V', long, global = true)]
    version: bool,

    /// The document to read. Without it, satex reads each root document
    /// in the current directory, or stdin if that is not a terminal.
    #[arg(short, long, global = true, value_name = "FILE", help_heading = "Input")]
    file: Option<PathBuf>,

    /// An extra config file, merged on top of every discovered satex.yaml
    /// (built-in defaults, the global config, the ones from the filesystem
    /// root down to the document's directory).
    #[arg(long, global = true, value_name = "FILE", help_heading = "Input")]
    config: Option<PathBuf>,

    /// Ignore every satex.yaml, discovered or named with --config. Only
    /// built-in defaults and `--set` apply.
    #[arg(long, global = true, help_heading = "Input")]
    no_config: bool,

    /// Which defaults to apply (usually auto-detected): document, package, class,
    /// literate or plain.
    #[arg(long, global = true, value_name = "NAME", help_heading = "Input")]
    profile: Option<String>,

    /// Overrides an option of `satex.yaml` by its dotted path, the value
    /// written as there: `--set limits.memory=8GiB --set load.inputs=false`.
    /// `PATH+` appends to a list: `--set lint_off+=[unused-label]`. Repeatable. The flags below win over it.
    #[arg(long = "set", global = true, value_name = "PATH=VALUE", help_heading = "Input")]
    #[cfg_attr(not(target_os = "emscripten"), arg(add = clap_complete::ArgValueCandidates::new(|| {
        satex::project::config::keys().into_iter().map(|(k, _)| k).map(|k| clap_complete::CompletionCandidate::new(format!("{k}="))).collect::<Vec<_>>()
    })))]
    set: Vec<String>,

    /// Do not load packages. Analyze the document and the kernel only.
    #[arg(long, global = true, help_heading = "Input")]
    no_packages: bool,

    /// Do not load the document class.
    #[arg(long, global = true, help_heading = "Input")]
    no_classes: bool,

    /// Text read at `--at` as if it stood there in the file, such as
    /// `\mymacro{x}`: every command then sees what it does in the state the
    /// document has there. The file on disk is not changed.
    #[arg(long, global = true, value_name = "TEXT", requires = "at", help_heading = "Input")]
    inject: Option<String>,

    /// Where `--inject` puts its text, `[FILE:]LINE[:COL]`. The main file
    /// without FILE, the start of the line without COL.
    #[arg(long, global = true, value_name = "LINE[:COL]", requires = "inject", help_heading = "Input")]
    at: Option<String>,

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

    /// Print the accounting of `--stats` and `--timings` as one JSON object to
    /// stderr after the run.
    #[arg(long, global = true, help_heading = "Output")]
    stats_json: bool,
}

impl Cli {
    /// Whether `--no-group` was given to `lint`.
    fn no_group(&self) -> bool {
        matches!(self.command, Some(Command::Lint { no_group: true, .. }))
    }

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
static ALLOCATOR: satex::analysis::budget::Counted = satex::analysis::budget::Counted;

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

fn completions(shell: &str, target: Option<&str>) -> Result<(), String> {
    let mut cmd = <Cli as clap::CommandFactory>::command();
    cmd.build();
    let mut out = io::stdout().lock();
    if shell == "install" {
        return install_completions(target);
    }
    if let Some(target) = target {
        return Err(format!("`{target}` applies to `satex completions install` only"));
    }
    if shell == "json" {
        let json = command_json(&cmd);
        // The type lattice, for type hovers: each type, the one above it, and what it is.
        let types: Vec<serde_json::Value> = {
            use satex::analysis::roles::Ty;
            let name = |t: Ty| if t == Ty::Top { "any".to_string() } else { t.to_string() };
            Ty::all()
                .into_iter()
                .map(|t| {
                    let about = satex::lsp::knowledge::topic(&name(t))
                        .map(|(_, body)| body.split("\n\n").next().unwrap_or(body));
                    serde_json::json!({ "name": name(t), "parent": name(t.parent()), "family": t.family(), "about": about })
                })
                .collect()
        };
        let text = serde_json::to_string_pretty(&serde_json::json!({
            "command": json,
            "queries": query::Query::ALL.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            "config_keys": satex::project::config::keys(),
            "rules": satex::linter::rules().iter().filter_map(|r| r.get("code").cloned()).collect::<Vec<_>>(),
            "types": types,
        }));
        return writeln!(out, "{}", text.map_err(|e| e.to_string())?).map_err(|e| e.to_string());
    }
    #[cfg(target_os = "emscripten")]
    return Err(format!("no shell completion in this build: `{shell}`"));
    #[cfg(not(target_os = "emscripten"))]
    let shells = clap_complete::env::Shells::builtins();
    #[cfg(not(target_os = "emscripten"))]
    let sh = shells.completer(shell).ok_or_else(|| format!("unknown shell `{shell}`"))?;
    #[cfg(not(target_os = "emscripten"))]
    sh.write_registration("COMPLETE", "satex", "satex", "satex", &mut out).map_err(|e| e.to_string())
}

/// The startup file of `shell` under `home` and the completion hook line.
fn completion_file(home: &std::path::Path, shell: &str) -> Option<(std::path::PathBuf, &'static str)> {
    match shell {
        "bash" => Some((home.join(".bashrc"), "source <(COMPLETE=bash satex)")),
        "zsh" => Some((home.join(".zshrc"), "source <(COMPLETE=zsh satex)")),
        "fish" => Some((home.join(".config/fish/completions/satex.fish"), "COMPLETE=fish satex | source")),
        _ => None,
    }
}

/// The shell that started satex, by its parent's process name.
fn parent_shell() -> Option<String> {
    #[cfg(unix)]
    {
        let comm = std::fs::read_to_string(format!("/proc/{}/comm", std::os::unix::process::parent_id())).ok()?;
        let name = comm.trim().trim_start_matches('-');
        return ["bash", "zsh", "fish"].contains(&name).then(|| name.to_string());
    }
    #[allow(unreachable_code)]
    None
}

/// The `$SHELL` name.
fn env_shell() -> String {
    std::env::var("SHELL").unwrap_or_default().rsplit('/').next().unwrap_or("").to_string()
}

/// Whether `shell`'s startup file under `home` loads the completion.
fn completion_set_up(home: &std::path::Path, shell: &str) -> bool {
    match completion_file(home, shell) {
        Some((file, _)) if shell == "fish" => file.exists(),
        Some((file, _)) => std::fs::read_to_string(file).is_ok_and(|t| t.contains(&format!("COMPLETE={shell} satex"))),
        None => true,
    }
}

/// The shells installed on this machine or in use.
fn known_shells(home: &std::path::Path) -> Vec<&'static str> {
    let used = [env_shell(), parent_shell().unwrap_or_default()];
    ["bash", "zsh", "fish"]
        .into_iter()
        .filter(|shell| used.iter().any(|u| u == shell) || satex::project::paths::which(shell).is_ok())
        .filter(|shell| completion_file(home, shell).is_some())
        .collect()
}

/// After `--version`: one hint on stderr naming the shells without the completion.
fn completion_hints() {
    let Some(home) = std::env::var_os("HOME").map(std::path::PathBuf::from) else { return };
    let missing: Vec<&str> = known_shells(&home).into_iter().filter(|shell| !completion_set_up(&home, shell)).collect();
    if !missing.is_empty() {
        eprintln!("no shell completion for {}: run `satex completions install`", missing.join(", "));
    }
}

/// Adds the completion hook to a shell's startup file, once: `target`, else
/// every shell [`known_shells`] finds.
fn install_completions(target: Option<&str>) -> Result<(), String> {
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).ok_or("HOME is not set")?;
    let shells: Vec<&str> = match target {
        Some(shell) => vec![shell],
        None => known_shells(&home),
    };
    if shells.is_empty() {
        return Err("found no bash, zsh or fish to set up. Name one: `satex completions install zsh`".into());
    }
    for shell in shells {
        let (file, line) = completion_file(&home, shell)
            .ok_or_else(|| format!("no automatic setup for the shell `{shell}`. See `satex completions --help`"))?;
        let text = std::fs::read_to_string(&file).unwrap_or_default();
        if text.lines().any(|l| l.trim() == line) {
            println!("{shell}: already set up in {}", file.display());
            continue;
        }
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let sep = if text.is_empty() || text.ends_with('\n') { "" } else { "\n" };
        std::fs::write(&file, format!("{text}{sep}# satex shell completion\n{line}\n")).map_err(|e| e.to_string())?;
        println!("{shell}: added to {}. Run `source {}` to use it in this shell", file.display(), file.display());
    }
    Ok(())
}

fn command_json(cmd: &clap::Command) -> serde_json::Value {
    use serde_json::json;
    let args: Vec<_> = cmd
        .get_arguments()
        .filter(|a| !a.is_hide_set())
        .map(|a| {
            json!({
                "id": a.get_id().as_str(),
                "long": a.get_long(),
                "short": a.get_short().map(String::from),
                "positional": a.is_positional(),
                "takes_value": a.get_action().takes_values(),
                "value_names": a.get_value_names().map(|v| v.iter().map(|s| s.as_str()).collect::<Vec<_>>()),
                "possible_values": a.get_possible_values().iter().filter(|p| !p.is_hide_set()).map(|p| p.get_name()).collect::<Vec<_>>(),
                "help": a.get_help().map(|h| h.to_string()),
            })
        })
        .collect();
    json!({
        "name": cmd.get_name(),
        "about": cmd.get_about().map(|h| h.to_string()),
        "args": args,
        "subcommands": cmd.get_subcommands().filter(|c| !c.is_hide_set()).map(command_json).collect::<Vec<_>>(),
    })
}

fn run() -> Result<(), String> {
    #[cfg(not(target_os = "emscripten"))]
    clap_complete::CompleteEnv::with_factory(<Cli as clap::CommandFactory>::command).complete();
    let mut cli = Cli::parse();
    if let Some(Command::Completions { shell, target }) = &cli.command {
        return completions(shell, target.as_deref());
    }
    anstream::ColorChoice::from(cli.color).write_global();
    // Progress is for a person reading text, not for a program reading output.
    if !matches!(cli.format, Format::Text) {
        satex::util::progress::disable();
    }

    if cli.version || cli.command.is_none() {
        if !matches!(cli.format, Format::Text | Format::Json) {
            return Err(format!("`--format {}` does not apply to `--version`. Try text or json", cli.format.as_str()));
        }
        let (cfg, layers) = configure(&cli, None)?;
        let mut out = anstream::stdout().lock();
        let text = match cli.format {
            Format::Json => format!(
                "{}\n",
                serde_json::to_string_pretty(&overview_json(
                    &cfg,
                    &satex::project::build_files::Project::discover(&base_dir(cli.file.as_deref())),
                    &layers
                ))
                .map_err(|e| e.to_string())?
            ),
            _ => overview_text(&cfg, &layers),
        };
        let _ = write!(out, "{text}");
        if cli.version {
            completion_hints();
        }
        return Ok(());
    }
    cli.command().check_format(cli.format)?;
    // `explain` and `scope` without a document run the smallest LaTeX
    // document, so the kernel and class still define `\section` and the like.
    let explaining = matches!(cli.command, Some(Command::Explain { .. } | Command::Scope { .. }));
    if cli.file.is_none() && cli.command().needs_source() && satex::util::progress::terminal(io::stdin()) {
        let roots = input_for(&std::env::current_dir().unwrap_or_default(), explaining)?;
        if roots.len() < 2 {
            return run_on(&mut cli, roots.into_iter().next(), explaining);
        }
        // One machine-readable document per run: several roots would
        // concatenate them.
        if !cli.format.is_text() {
            let names: Vec<String> = roots.iter().map(|p| p.display().to_string()).collect();
            return Err(format!("{} are each a root: name one with -f", names.join(", ")));
        }
        // Several roots: each its own run, in turn; one that fails stops none of the others.
        let mut failed = Vec::new();
        for root in roots {
            anstream::eprintln!("{}", render::status(&format!("satex: root {}", root.display())));
            if let Err(why) = run_on(&mut cli, Some(root.clone()), explaining) {
                anstream::eprintln!("satex: {}: {why}", root.display());
                failed.push(root.display().to_string());
            }
        }
        return match failed.is_empty() {
            true => Ok(()),
            false => Err(format!("the run failed on {}", failed.join(", "))),
        };
    }
    run_on(&mut cli, None, explaining)
}

/// One run of the command on `file`, or on what `cli` names.
fn run_on(cli: &mut Cli, file: Option<PathBuf>, explaining: bool) -> Result<(), String> {
    cli.file = file.or(cli.file.take());
    let mut source = if cli.file.is_none() && explaining && satex::util::progress::terminal(io::stdin()) {
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
    let injected = match (&cli.inject, &cli.at) {
        (Some(text), Some(at)) => Some(inject(cli, &mut source, text, at)?),
        _ => None,
    };
    let (mut cfg, _layers) = configure(cli, Some(&source))?;
    if let Some((file, from, to)) = injected {
        cfg.trace_file = file;
        cfg.trace_lines = Some((from.0, to.0));
        (cfg.trace_col, cfg.trace_end_col) = (from.1, to.1);
    }
    // Input decoding follows the engine.  `Machine::analyze` settles it
    // again once the project and the magic comments are read; commands that
    // tokenize without an analysis, such as `tokens`, keep this one.
    satex::engine::source::set_encoding(satex::plugins::Plugins::sniff_engine(&cfg, &source).encoding());
    if let Some(Command::Lsp { port, host, .. }) = &cli.command {
        return satex::cli::lsp::run(&cfg, cli.file.as_deref(), *port, host.as_deref());
    }
    if let Some(Command::Cache { action: Some(action) }) = &cli.command {
        let mut out = anstream::stdout().lock();
        let json = cli.format == Format::Json;
        return match action {
            CacheAction::Build { packages, refresh, engines, preamble } => {
                let engines = engines
                    .iter()
                    .map(|name| {
                        satex::project::config::Engine::from_program(name)
                            .ok_or_else(|| format!("unknown engine `{name}`"))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                satex::cli::cache::build::run(&cfg, *refresh, packages, &engines, preamble.as_deref(), json, &mut out)
            }
            CacheAction::Export { archive, only, without } => {
                satex::cli::cache::export(&cfg, archive, (only, without), json, &mut out)
            }
            CacheAction::Import { archives } => {
                archives.iter().try_for_each(|archive| satex::cli::cache::import(&cfg, archive, json, &mut out))
            }
            CacheAction::Clear => satex::cli::cache::clear(&cfg, json, &mut out),
            CacheAction::Prune => satex::cli::cache::prune(&cfg, json, &mut out),
        };
    }
    if let Some(Command::Query { request: Some(request), .. }) = &cli.command {
        let mut out = anstream::stdout().lock();
        return satex::cli::query::run_request(request, &cfg, cli.file.as_deref(), &mut out).map(drop);
    }

    satex::analysis::machine::HOT.store(cli.stats, std::sync::atomic::Ordering::Relaxed);
    let started = std::time::Instant::now();
    let analysis = if cli.command().needs_analysis() {
        Machine::analyze(&source, cli.file.as_deref(), &cfg)
    } else {
        Analysis::empty()
    };
    if cli.stats || cfg.verbose >= 1 {
        for line in analysis.cache_report() {
            anstream::eprintln!("{}", render::status(&format!("satex: {line}")));
        }
    }
    if cli.stats {
        anstream::eprintln!("{}", render::status(&analysis.stats(started.elapsed())));
        for line in analysis.hotspots(10) {
            anstream::eprintln!("{}", render::status(&line));
        }
    }
    log_gaps(&analysis, &cfg);
    for note in analysis.limit_notes() {
        anstream::eprintln!("{}", render::status(&format!("satex: {note}")));
    }
    // What the run could not follow reads as a note on the answer, so it goes
    // after it.
    let unfollowed = match cfg.gaps.report && cli.format.is_text() {
        true => render::gaps(&analysis),
        false => String::new(),
    };
    let links = cli.link_policy(satex::util::progress::terminal(io::stdout()));
    let context = Context { analysis: &analysis, source: &source, format: cli.format, links };
    let mut out = anstream::stdout().lock();
    let output = cli.command().run(&context, &mut out)?;
    if matches!(cli.command, Some(Command::Summary { .. })) && cli.format.is_text() {
        let project = satex::project::build_files::Project::discover(&base_dir(cli.file.as_deref()));
        let _ = write!(out, "{}", environment_text(&cfg, &project));
    }
    // `lint` already says what the analysis could not follow,
    // in a form that fits its own findings; the generic footer would only
    // repeat that in a second, unrelated shape.
    let owns_gaps = (cli.format.is_text() && matches!(output, Output::Lint(_)))
        || cli.command().fixes()
        || cli.command().reports_gaps();
    let text = cli.format.is_text();
    let (body, hint) = match output {
        Output::Done => (String::new(), None),
        Output::Records(records) => (rendered(&records, cli, links), None),
        Output::Partial(records, hint) => (rendered(&records, cli, links), Some(hint)),
        Output::Detail(records) if text => (render::fields(&records, links), None),
        Output::DetailPartial(records, hint) if text => (render::fields(&records, links), Some(hint)),
        Output::Lint(records) if text => (render::lint(&records, links, !cli.no_group()), None),
        Output::Detail(records) | Output::Lint(records) => (rendered(&records, cli, links), None),
        Output::DetailPartial(records, hint) => (rendered(&records, cli, links), Some(hint)),
    };
    // JSON escapes what text would print raw.
    let safe = |t: &str| if text { render::terminal_safe(t) } else { t.to_string() };
    let _ = write!(out, "{}", safe(&body));
    if let Some(hint) = hint.filter(|_| text) {
        let _ = writeln!(out, "{}", safe(&render::footer(&hint)));
    }
    if !owns_gaps {
        let _ = write!(out, "{}", safe(&unfollowed));
    }
    // After the command, so that the phases it ran are in the table.
    if cfg.timings {
        let _ = out.flush();
        anstream::eprint!(
            "{}",
            render::timings(&analysis, cli.link_policy(satex::util::progress::terminal(io::stderr())))
        );
    }
    if cli.stats_json {
        let _ = out.flush();
        anstream::eprintln!("{}", render::stats_json(&analysis, started.elapsed()));
    }
    Ok(())
}

/// The profile in force for this run: `--profile`, then `profile:` already
/// settled by the discovered `satex.yaml` layers, then
/// [`satex::project::config::Profile::detect`] on `source` — or, when `configure` is
/// called before there is one (`--version`, with no `-f` read yet), a light
/// read of `cli.file` just for the sniff.
fn resolve_profile(cli: &Cli, cfg: &Config, source: Option<&str>) -> Result<satex::project::config::Profile, String> {
    use satex::project::config::Profile;
    if let Some(name) = &cli.profile {
        return Profile::parse(name)
            .ok_or_else(|| format!("unknown profile `{name}`. Try document, package, class, literate or plain"));
    }
    if let Some(profile) = cfg.profile {
        return Ok(profile);
    }
    // `cache build` analyzes the documents it writes itself: a class and
    // its packages, never plain TeX. `lsp` serves LaTeX documents and reads
    // no source up front.
    if cli.file.is_none() && matches!(cli.command, Some(Command::Cache { .. } | Command::Lsp { .. })) {
        return Ok(Profile::Document);
    }
    let owned;
    let text = match source.filter(|s| !s.is_empty()) {
        Some(text) => text,
        None => {
            owned =
                cli.file.as_deref().and_then(|p| satex::project::overlay::read_to_string(p).ok()).unwrap_or_default();
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
        let (path, value) =
            assignment.split_once('=').ok_or_else(|| format!("--set {assignment}: expected PATH=VALUE"))?;
        cfg.set(path.trim(), value)?;
    }
    cfg.load.packages &= !cli.no_packages;
    cfg.load.classes &= !cli.no_classes;
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
    if let Some(Command::Trace { lines, steps, from, to, include_internal, interactive, state, .. }) = &cli.command {
        cfg.trace_document_only = !include_internal && !interactive;
        cfg.trace_state |= state;
        cfg.trace_file = satex::cli::trace_file(&[from.as_deref(), to.as_deref(), lines.as_deref()])?;
        if let Some(from) = from {
            let (_, from) = satex::cli::split_file(from);
            let (line, col) = from.split_once(':').unwrap_or((from, "0"));
            let bad = || format!("`{from}` is no position: use LINE or LINE:COL");
            let line: u32 = line.trim().parse().map_err(|_| bad())?;
            cfg.trace_col = col.trim().parse().map_err(|_| bad())?;
            cfg.trace_lines = Some((line, u32::MAX));
        }
        if let Some(to) = to {
            let (_, to) = satex::cli::split_file(to);
            let (line, col) = to.split_once(':').unwrap_or((to, ""));
            let bad = || format!("`{to}` is no position: use LINE or LINE:COL");
            let line: u32 = line.trim().parse().map_err(|_| bad())?;
            if !col.trim().is_empty() {
                cfg.trace_end_col = col.trim().parse().map_err(|_| bad())?;
            }
            cfg.trace_lines = Some((cfg.trace_lines.map_or(0, |(a, _)| a), line));
        }
        if lines.is_some() {
            cfg.trace_lines = lines
                .as_deref()
                .map(|l| range(satex::cli::split_file(l).1))
                .transpose()?
                .map(|(a, b)| (a.min(u32::MAX.into()) as u32, b.min(u32::MAX.into()) as u32));
        }
        cfg.trace_steps = steps.as_deref().map(range).transpose()?;
    }
    cfg.record_arguments |= cli.command.as_ref().is_some_and(Command::wants_arguments);
    cfg.timings |= cli.timings || cli.stats_json;
    if let Some(command) = &cli.command {
        cfg.opaque_conditionals.extend(command.opaque_conditionals());
    }
    satex::analysis::budget::set_limit(cfg.limits.memory.as_u64());
    Ok((cfg, layers))
}

/// Every plugin kind this build offers and what it can detect, shared by the
/// text and json forms of `satex --version`.
fn plugin_rows() -> Vec<(satex::plugins::Kind, Vec<String>)> {
    use satex::plugins::{Format as OutputFormat, Kind, Output, Platform, Provider, tool::Tool};
    vec![
        (Kind::Provider, Provider::ALL.iter().map(|p| p.as_str().to_string()).collect()),
        (Kind::Platform, Platform::ALL.iter().map(|p| p.as_str().to_string()).collect()),
        (Kind::Engine, satex::project::config::Engine::ALL.iter().map(|e| e.as_str().to_string()).collect()),
        (Kind::Kernel, satex::plugins::Kernel::ALL.iter().map(|k| k.as_str().to_string()).collect()),
        (Kind::Output, Output::ALL.iter().map(|o| o.as_str().to_string()).collect()),
        (Kind::BuildSystem, {
            let mut systems: Vec<String> =
                satex::project::build_files::BUILD_FILES.iter().map(|(_, tool)| tool.to_string()).collect();
            systems.dedup();
            systems.push("arara".into());
            systems.push("l3build".into());
            systems
        }),
        (Kind::Tool, Tool::all().iter().map(|t| t.as_str().to_string()).collect()),
        (Kind::Magic, ["% !TeX", "% !BIB", "% arara:"].iter().map(|m| m.to_string()).collect()),
        (Kind::Format, OutputFormat::ALL.iter().map(|f| f.as_str().to_string()).collect()),
        (Kind::Preload, ["%& line", "-fmt=", "configured"].iter().map(|m| m.to_string()).collect()),
        (Kind::Depp, vec!["depp".to_string(), "DEPENDS.txt".to_string()]),
    ]
}

fn discovery_list(cfg: &Config) -> Vec<String> {
    if cfg.plugins.discovery {
        satex::plugins::Discovery::honors().iter().map(|s| s.to_string()).collect()
    } else {
        Vec::new()
    }
}

/// TEXINPUTS and the other kpathsea search-path variables, resolved for
/// `cfg`/`project` against the current directory.
fn effective_paths(
    cfg: &Config,
    project: &satex::project::build_files::Project,
) -> Vec<satex::project::paths::Effective> {
    satex::project::paths::effective_all(project, &cfg.paths, &project.root)
}

/// What TeX installation `cfg` resolves to here, without indexing its files:
/// "no installation" when nothing was found (`texmf.kpsewhich: false` and
/// `texmf.fallback: false` simulate that on a machine that has one), or
/// how many trees and by what means otherwise.
fn installation_summary(cfg: &Config, project: &satex::project::build_files::Project) -> String {
    let mut request = cfg.distribution_request();
    request.kpse_env = satex::project::paths::env_vars(&effective_paths(cfg, project));
    let (roots, discovery) = satex::caches::distribution::locate(&request);
    if roots.is_empty() {
        "no installation".to_string()
    } else {
        format!("{} tree{} via {}", roots.len(), if roots.len() == 1 { "" } else { "s" }, discovery.as_str())
    }
}

/// What `satex --version` prints: the build, every plugin it can use, and
/// the configuration this directory gives it.
/// Where the run looked: the profile, the TeX installation and the search paths.
fn environment_text(cfg: &Config, project: &satex::project::build_files::Project) -> String {
    let mut out = String::new();
    let row = |out: &mut String, name: &str, value: &str| {
        let _ = writeln!(out, "  {}  {}", render::label(&format!("{name:12}")), value);
    };
    let _ = writeln!(out, "\n{}", render::heading("environment"));
    row(&mut out, "profile", cfg.profile.map(satex::project::config::Profile::as_str).unwrap_or("document"));
    row(&mut out, "installation", &installation_summary(cfg, project));
    let mut unset = Vec::new();
    for effective in effective_paths(cfg, project) {
        if effective.raw.is_empty() {
            unset.push(effective.variable);
            continue;
        }
        let dirs = effective.dirs.len();
        let value = format!(
            "{} ({}, {dirs} {})",
            effective.raw,
            effective.source.as_str(),
            if dirs == 1 { "dir" } else { "dirs" }
        );
        row(&mut out, effective.variable, &value);
    }
    if let Some(output) = satex::project::paths::output_dir(project, &cfg.paths.output_directory, &project.root) {
        row(&mut out, "output dir", &format!("{} ({})", output.raw, output.source.as_str()));
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
        "satex {}{commit}, built {}\n{}\n\n{}\n",
        env!("CARGO_PKG_VERSION"),
        env!("SATEX_BUILD_TIME"),
        env!("CARGO_PKG_REPOSITORY"),
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
    let _ = writeln!(out, "\n{}", render::heading("primitives"));
    for (engine, [all, exact, approximated, unsupported]) in primitive_counts() {
        let padded = format!("{:width$}", engine.as_str());
        let _ = writeln!(
            out,
            "  {}  {all:>4}  {exact} exact, {approximated} approximated, {unsupported} unsupported",
            render::label(&padded)
        );
    }
    let _ = writeln!(out, "\n{}", render::heading(&format!("stubs in effect: {}", cfg.stubs.len())));
    match cfg.stubs.is_empty() {
        true => {
            let _ = writeln!(out, "  none");
        }
        false => {
            let _ = writeln!(out, "  {}", cfg.stubs.keys().cloned().collect::<Vec<_>>().join(", "));
        }
    }
    let _ = writeln!(out, "\n{}", render::heading("configuration"));
    if layers.is_empty() {
        let _ = writeln!(out, "  built-in defaults. Write satex.yaml to change them");
    } else {
        for path in layers {
            let full = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            let _ = writeln!(out, "  {}", full.display());
        }
    }
    out
}

/// Per engine: its primitives, and how many satex runs exactly,
/// approximates soundly or leaves unmodeled ([`satex::engine::builtins::verdict`]).
fn primitive_counts() -> Vec<(Engine, [usize; 4])> {
    use satex::engine::builtins::Verdict;
    let mut it = satex::engine::tex::Interner::default();
    Engine::ALL
        .iter()
        .map(|&engine| {
            let mut counts = [0; 4];
            for meaning in satex::engine::builtins::initial_meanings(&mut it, engine).values() {
                let Some(p) = meaning.prim() else { continue };
                counts[0] += 1;
                counts[match satex::engine::builtins::verdict(p).0 {
                    Verdict::Exact => 1,
                    Verdict::Approximated => 2,
                    Verdict::Unsupported => 3,
                }] += 1;
            }
            (engine, counts)
        })
        .collect()
}

/// The same as [`overview_text`], structured for `--format json`.
fn overview_json(
    cfg: &Config,
    project: &satex::project::build_files::Project,
    layers: &[PathBuf],
) -> serde_json::Value {
    let commit = env!("SATEX_COMMIT");
    let plugins: serde_json::Map<String, serde_json::Value> =
        plugin_rows().into_iter().map(|(kind, items)| (kind.as_str().to_string(), serde_json::json!(items))).collect();
    serde_json::json!({
        "version": env!("CARGO_PKG_VERSION"),
        "commit": if commit.is_empty() { None } else { Some(commit) },
        "built": env!("SATEX_BUILD_TIME"),
        "source": env!("SATEX_SOURCE_ID"),
        "repository": env!("CARGO_PKG_REPOSITORY"),
        "plugin_count": plugins.values().filter_map(|items| items.as_array()).map(Vec::len).sum::<usize>(),
        "plugins": plugins,
        "discovery": discovery_list(cfg),
        "primitives": primitive_counts().into_iter().map(|(engine, [all, exact, approximated, unsupported])| {
            (engine.as_str().to_string(), serde_json::json!({
                "all": all, "exact": exact, "approximated": approximated, "unsupported": unsupported,
            }))
        }).collect::<serde_json::Map<_, _>>(),
        "stubs": cfg.stubs.keys().collect::<Vec<_>>(),
        "configuration": layers.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        "installation": installation_summary(cfg, project),
        "profile": cfg.profile.map(|p| p.as_str()),
        "paths": effective_paths(cfg, project).into_iter().map(|e| serde_json::json!({
            "variable": e.variable,
            "value": e.raw,
            "source": e.source.as_str(),
            "dirs": e.dirs.iter().map(|d| d.display().to_string()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "output_directory": satex::project::paths::output_dir(project, &cfg.paths.output_directory, &project.root).map(|o| serde_json::json!({
            "value": o.raw,
            "source": o.source.as_str(),
            "dir": o.dir.display().to_string(),
        })),
    })
}

/// Append what this run could not follow to the gap log: one JSON object per
/// cause, with the document it came from, so the limits met across many
/// documents accumulate in one file.  `gaps.log: false` turns it off.
fn log_gaps(analysis: &Analysis, cfg: &Config) {
    if !cfg.gaps.log {
        return;
    }
    let records = query::run(analysis, query::Query::Gaps, &query::Filter::Always);
    if records.is_empty() {
        return;
    }
    let Some(path) =
        cfg.gaps.file.clone().or_else(|| satex::caches::format::default_cache_dir().map(|dir| dir.join("gaps.ndjson")))
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
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = file.write_all(text.as_bytes());
    }
}

fn rendered(records: &[query::Record], cli: &Cli, links: Links) -> String {
    cli.format.render(records, links, env!("CARGO_PKG_VERSION"))
}

/// The documents to read when none is named: the roots in `dir`, each
/// analyzed on its own; for `explain` and `scope`, none (the smallest
/// document) when there is not exactly one. Files no root reads are named
/// on stderr.
fn input_for(dir: &Path, explaining: bool) -> Result<Vec<PathBuf>, String> {
    let found = satex::plugins::discovery::scan(dir);
    let unreachable = found.unreachable();
    if !unreachable.is_empty() {
        let names: Vec<String> = unreachable.iter().map(|p| p.display().to_string()).collect();
        anstream::eprintln!("{}", render::status(&format!("satex: no root reads {}", names.join(", "))));
    }
    let roots = found.roots();
    let why = match roots.len() {
        0 => format!("no root document in {}: name one with -f", dir.display()),
        n => format!("{n} documents could each be the root: name one with -f"),
    };
    match (roots.len(), explaining) {
        (1, _) | (2.., false) => Ok(roots),
        (0, false) => Err(why),
        _ => {
            note_minimal_document(&why);
            Ok(Vec::new())
        }
    }
}

/// `explain`/`scope` fell back to [`EMPTY_DOCUMENT`]: said once, on stderr,
/// so a name that means nothing there is not a silent surprise.
fn note_minimal_document(why: &str) {
    anstream::eprintln!("satex: no document named ({why}). Analyzing the smallest LaTeX document instead");
}

/// `--inject`: the text put at `--at` in the main file's `source`, or
/// in another file through the overlay the run reads files from. The file,
/// and the positions where the text begins and ends, bound the trace.
type Injected = (Option<String>, (u32, u32), (u32, u32));

fn inject(cli: &Cli, source: &mut String, text: &str, at: &str) -> Result<Injected, String> {
    let (file, pos) = satex::cli::split_file(at);
    // The main file named by its path is the main file: its text is `source`.
    let main = cli.file.as_deref().and_then(|p| p.canonicalize().ok());
    let file = file.filter(|name| base_dir(cli.file.as_deref()).join(name).canonicalize().ok() != main);
    let (line, col) = pos.split_once(':').unwrap_or((pos, "0"));
    let bad = || format!("`{at}` is no position: use [FILE:]LINE or [FILE:]LINE:COL");
    let (line, col): (u32, u32) = (line.trim().parse().map_err(|_| bad())?, col.trim().parse().map_err(|_| bad())?);
    let from = (line, col.max(1));
    match &file {
        None => {
            let (with, to) = satex::cli::inject_text(source, line, col, text)?;
            // What reads the main file again (the trace's source lines)
            // reads it with the text in it too.
            if let Some(path) = cli.file.as_deref().and_then(|p| p.canonicalize().ok()) {
                satex::project::overlay::set(&path, with.clone());
            }
            *source = with;
            Ok((None, from, to))
        }
        Some(name) => {
            let path = base_dir(cli.file.as_deref()).join(name);
            let path = path.canonicalize().map_err(|e| format!("{}: {e}", path.display()))?;
            let original =
                satex::project::overlay::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let (with, to) = satex::cli::inject_text(&original, line, col, text)?;
            satex::project::overlay::set(&path, with);
            Ok((file, from, to))
        }
    }
}

fn read_source(path: Option<&Path>) -> Result<String, String> {
    match path {
        Some(path) => satex::project::overlay::read_to_string(path).map_err(|e| format!("{}: {e}", path.display())),
        None => {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf).map_err(|e| e.to_string())?;
            Ok(buf)
        }
    }
}

fn base_dir(path: Option<&Path>) -> PathBuf {
    match path {
        Some(path) => satex::project::paths::dir_of(path),
        None => std::env::current_dir().unwrap_or_default(),
    }
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
    /// `--help` is short sentences: no `;` or `—` joins two clauses.
    #[test]
    fn help_joins_no_clauses() {
        fn walk(cmd: &mut clap::Command, faults: &mut Vec<String>) {
            let help = cmd.render_long_help().to_string();
            let prose: String = help.split('`').step_by(2).collect();
            for line in prose.lines() {
                let semicolon =
                    line.match_indices(';').any(|(at, _)| line[at + 1..].starts_with(' ') || at + 1 == line.len());
                if semicolon || line.contains('—') {
                    faults.push(format!("{}: {}", cmd.get_name(), line.trim()));
                }
            }
            for sub in cmd.get_subcommands_mut() {
                walk(sub, faults);
            }
        }
        let mut faults = Vec::new();
        walk(&mut <super::Cli as clap::CommandFactory>::command(), &mut faults);
        assert!(faults.is_empty(), "rephrase into short sentences:\n{}", faults.join("\n"));
    }
}
