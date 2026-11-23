//! `satex compile`: build the document with the tools this machine has —
//! latexmk where it is installed, else the engine the analysis found — and
//! explain each error and warning the build log reports: by the finding
//! satex made at that place, else by the rule that describes that message.
//!
//! The build reads the project's own latexmkrc; satex only adds what keeps
//! the log readable (`-file-line-error`, long lines) and never opens a
//! viewer (`-pv-`). It asks for SyncTeX data, which editors and the
//! playground's PDF view link the source by.
use std::path::{Path, PathBuf};
use std::process::Command;

use super::{Context, Output};
use crate::queries::query::Record;
use serde_json::json;

/// One message of the build log, where TeX says it arose.
#[derive(Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct Logged {
    path: Option<PathBuf>,
    line: Option<u64>,
    error: bool,
    message: String,
}

/// A build done and its log read: what explaining it needs.
pub struct Built {
    main: PathBuf,
    keep: PathBuf,
    job: String,
    logged: Vec<Logged>,
    /// The status, when the build failed without a TeX error in the log.
    failed: Option<String>,
    /// The key of the explanation: the logged messages, the source, satex and its settings.
    #[cfg_attr(not(unix), allow(dead_code))]
    head: String,
    /// The records the last explanation of this log gave, when nothing it read changed since.
    stored: Option<Vec<Record>>,
    /// Held by a satex that explains aside ([`Built::resumed`]): one per job.
    explaining: Option<std::fs::File>,
}

/// Set for the `satex compile` a build starts aside to explain it ([`Built::aside`]): no build
/// runs there, the analysis does, and what it explains is stored for the next `satex compile`.
pub const ASIDE: &str = "SATEX_COMPILE_ASIDE";

impl Built {
    /// Whether explaining the build needs the analysis: only findings at the logged messages
    /// explain them, and a stored explanation whose inputs did not change is the one it would give.
    /// Only a message with a source line can be explained ([`explain`] matches a finding by its
    /// line): bad boxes the output routine reports with none never start an analysis.
    pub fn needs_analysis(&self) -> bool {
        self.stored.is_none() && self.logged.iter().any(|l| l.line.is_some())
    }

    fn file(&self, ext: &str) -> PathBuf {
        self.keep.join(format!("{}.{ext}", self.job))
    }

    /// Whether this satex explains a build aside: it waits for the analysis, no one waits for it.
    pub fn is_aside(&self) -> bool {
        self.explaining.is_some()
    }

    /// Starts a satex of its own, detached and at the lowest priority, that analyzes the document
    /// and stores the explanation of this build's messages ([`Built::resumed`]). The build's caller
    /// has the PDF and returns; the next `satex compile` of the same text gives the stored records.
    pub fn aside(&self) -> bool {
        let record = json!({ "logged": self.logged, "failed": self.failed, "main": self.main });
        let Ok(satex) = std::env::current_exe() else { return false };
        if std::fs::create_dir_all(&self.keep).is_err()
            || std::fs::write(self.file("explain"), record.to_string()).is_err()
        {
            return false;
        }
        use super::cache::background::{NO_SETUP, detached, nice};
        let mut command = detached(&satex, nice().as_deref(), true);
        command.args(std::env::args_os().skip(1)).env(ASIDE, "1").env(NO_SETUP, "1");
        command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
        command.stderr(std::process::Stdio::null());
        // reaped by a thread: a long-lived caller keeps no zombie
        command.spawn().map(|mut child| std::thread::spawn(move || child.wait())).is_ok()
    }

    /// In a satex started aside ([`ASIDE`]): the build to explain, as its starter recorded it.
    /// `None`: this is no such satex. `Some(None)`: nothing to explain, since another satex is at
    /// it or the record is gone.
    pub fn resumed(context: &Context) -> Option<Option<Built>> {
        std::env::var_os(ASIDE)?;
        let place = Place::of(context.analysis);
        let (keep, job) = (place.keep(), place.job.clone());
        let resume = || {
            let explaining = super::cache::background::try_lock(&keep.join(format!("{job}.explaining"))).ok()??;
            let _ = std::fs::write(keep.join(format!("{job}.explaining")), std::process::id().to_string());
            let record: serde_json::Value =
                serde_json::from_slice(&std::fs::read(keep.join(format!("{job}.explain"))).ok()?).ok()?;
            let logged: Vec<Logged> = serde_json::from_value(record["logged"].clone()).ok()?;
            let failed = record["failed"].as_str().map(str::to_string);
            let head = head(&logged, &failed, &place.main, context);
            let (main, keep) = (place.main.clone(), keep.clone());
            Some(Built {
                main,
                keep,
                job: job.clone(),
                logged,
                failed,
                head,
                stored: None,
                explaining: Some(explaining),
            })
        };
        Some(resume())
    }
}

/// Stops the satex that explains an earlier build of `job` aside: its text is no longer the
/// document's. Its lock says it runs, its file names it.
fn supersede(keep: &Path, job: &str) {
    let file = keep.join(format!("{job}.explaining"));
    if !file.is_file() || !matches!(super::cache::background::try_lock(&file), Ok(None)) {
        return;
    }
    let pid = std::fs::read_to_string(&file).unwrap_or_default();
    if cfg!(unix) && !pid.is_empty() && pid.bytes().all(|b| b.is_ascii_digit()) {
        let _ = Command::new("kill").arg(&pid).status();
    }
}

/// Where a document's build is: its main file, directory, job name and output directory.
struct Place {
    main: PathBuf,
    dir: PathBuf,
    job: String,
    out_dir: Option<PathBuf>,
}

impl Place {
    fn of(analysis: &crate::analysis::machine::Analysis) -> Place {
        let main = PathBuf::from(analysis.file_name(analysis.main_file));
        let main = main.canonicalize().unwrap_or(main);
        let dir =
            main.parent().map(Path::to_path_buf).filter(|d| !d.as_os_str().is_empty()).unwrap_or_else(|| ".".into());
        let job = main.file_stem().and_then(|f| f.to_str()).unwrap_or_default().to_string();
        let out_dir = analysis.project.output_dir().map(|(d, _)| dir.join(d));
        Place { main, dir, job, out_dir }
    }

    /// Where satex keeps its state of this build.
    fn keep(&self) -> PathBuf {
        self.out_dir.clone().unwrap_or_else(|| self.dir.clone()).join(KEEP)
    }
}

/// The key of a build's explanation: the logged messages, the source, satex and its settings.
fn head(logged: &[Logged], failed: &Option<String>, main: &Path, context: &Context) -> String {
    let settings = serde_json::to_vec(&context.analysis.settings).unwrap_or_default();
    // the records are of the logged messages, not of the log (its first line holds the time)
    let said = [format!("{logged:?}{failed:?}{main:?}").as_bytes(), context.source.as_bytes(), &settings].map(hash);
    format!("{}\t{said:016x?}\n", env!("SATEX_SOURCE_ID"))
}

/// The build, explained by the findings of `context`'s analysis (rules only without `compile.explain`).
pub fn run(context: &Context, system: Option<&str>, view: bool) -> Result<Output, String> {
    let built = build_only(context, system, view)?;
    Ok(explained(built, context.analysis, true))
}

/// Each logged message explained: by the stored records where they hold, else by satex's finding
/// there where `analysis` is the document's (`analyzed`; a sketch has none), else by the finding
/// the last analysis made of the same message, else by its rule. Stores what an analysis gave.
pub fn explained(built: Built, analysis: &crate::analysis::machine::Analysis, analyzed: bool) -> Output {
    if let Some(records) = built.stored {
        eprintln!("satex: explained as the last build was, nothing the analysis read changed");
        return Output::Lint(records);
    }
    let wanted = built.needs_analysis() && analysis.settings.compile.explain;
    let asked = wanted && analyzed;
    let findings = if asked { crate::linter::lint(analysis) } else { Vec::new() };
    // what the last analysis said of a message this build repeats, while none of this text is there
    let earlier: Vec<Record> = match wanted && !analyzed {
        true => std::fs::read(built.file("explained.json")).ok().and_then(|j| serde_json::from_slice(&j).ok()),
        false => None,
    }
    .unwrap_or_default();
    let mut records: Vec<Record> = built.logged.iter().map(|l| explain(l, &findings, &earlier, &built.main)).collect();
    if let Some(status) = &built.failed {
        let message = format!("the build failed ({status}) without a TeX error in the log");
        records.push(record(&built.main, None, true, "build", message, None));
    }
    #[cfg(unix)]
    // a run a budget stopped gives findings of the time it had
    if asked && !analysis.stopped() {
        store(&built, analysis, &records);
    }
    Output::Lint(records)
}

/// The explanation's key: its head, then a stamp of every file the analysis read and of the
/// project tree (a file it looked for and did not find appears there); its records beside it.
#[cfg(unix)]
fn store(built: &Built, analysis: &crate::analysis::machine::Analysis, records: &[Record]) {
    let (key, json) = (built.file("explained"), built.file("explained.json"));
    let root = built.main.parent().map(Path::to_path_buf).unwrap_or_default();
    let path = |f: &str| Path::new(f).canonicalize().unwrap_or_else(|_| root.join(f));
    let mut files: Vec<PathBuf> = analysis.files.iter().map(|f| path(&f.path)).collect();
    files.sort_unstable();
    files.dedup();
    let mut text = built.head.clone();
    text.extend(files.iter().map(|f| stamp(f, false)));
    text.push_str(&stamp(&root, true));
    text.push_str(&kpathsea(&root));
    let _ = std::fs::create_dir_all(&built.keep);
    let _ = std::fs::write(&json, serde_json::to_vec(records).unwrap_or_default());
    let _ = std::fs::write(&key, text);
}

/// The stored records of `head`'s explanation, when its key holds.
#[cfg(unix)]
fn stored(keep: &Path, job: &str, head: &str) -> Option<Vec<Record>> {
    let key = std::fs::read_to_string(keep.join(format!("{job}.explained"))).ok()?;
    holds(key.strip_prefix(head)?).then_some(())?;
    serde_json::from_slice(&std::fs::read(keep.join(format!("{job}.explained.json"))).ok()?).ok()
}
/// The stamps of where kpathsea finds a file the project does not hold: its file databases
/// (`ls-R`) and the TeX search path's directories it reads from disk (no `!!`), to every level.
#[cfg(unix)]
fn kpathsea(dir: &Path) -> String {
    let ask = |args: &[&str]| {
        let out = Command::new("kpsewhich").args(args).current_dir(dir).output().ok();
        String::from_utf8_lossy(&out.map(|o| o.stdout).unwrap_or_default()).into_owned()
    };
    let (databases, path) = (ask(&["-all", "ls-R"]), ask(&["-show-path=tex"]));
    let mut key: String = databases.lines().filter(|l| !l.is_empty()).map(|l| stamp(Path::new(l), false)).collect();
    key.extend(searched(dir, &path).map(|d| stamp(&d, true)));
    key
}

/// The search path's directories kpathsea reads from disk, not from an `ls-R` (`!!`).
#[cfg(unix)]
fn searched<'a>(dir: &'a Path, path: &'a str) -> impl Iterator<Item = PathBuf> + 'a {
    let disk = path.trim().split(':').filter(|d| !d.starts_with("!!") && !d.is_empty() && *d != ".");
    disk.map(move |d| dir.join(d.trim_end_matches('/')))
}

#[cfg(not(unix))]
fn stored(_: &Path, _: &str, _: &str) -> Option<Vec<Record>> {
    None
}

/// The build with the tools this machine has, its log read.
pub fn build_only(context: &Context, system: Option<&str>, view: bool) -> Result<Built, String> {
    // no tool this build starts outlives it
    #[cfg(not(target_os = "emscripten"))]
    super::group::die_together();
    let analysis = context.analysis;
    let place = Place::of(analysis);
    let Place { main, dir, job, out_dir } = &place;
    let (main, dir, job, out_dir) = (main.clone(), dir.clone(), job.clone(), out_dir.clone());
    let file = main.file_name().and_then(|f| f.to_str()).ok_or("the document has no file name")?;
    let project = &analysis.project;
    // an explanation of an earlier text, still in the making, is of no use to this build
    supersede(&place.keep(), &job);
    let log_path = out_dir.clone().unwrap_or_else(|| dir.clone()).join(format!("{job}.log"));
    let engine = analysis.plugins.engine;
    // latexmk runs LaTeX: a plain TeX file (`% !TeX program = tex`) runs its own program.
    let latex = analysis.plugins.kernel == crate::plugins::Kernel::Latex;
    let program = match latex || analysis.plugins.program.is_empty() {
        true => engine.program(),
        false => analysis.plugins.program.as_str(),
    };
    // Knuth's `tex` writes DVI only: pdfTeX runs the same plain format and writes PDF.
    let program = if program == "tex" { "pdftex" } else { program };
    let installed = |p: &str| crate::project::paths::which(p).is_ok();
    let arara = analysis.plugins.magic.iter().any(|m| m.key == "arara");
    let asked = system.unwrap_or(&analysis.settings.compile.system);
    // satex's own build yields to a build the project configures itself
    let configured = arara || project.files.iter().any(|f| f.tool == "latexmk" || f.tool == "tectonic");
    let asked = if asked == "satex" && configured { "auto" } else { asked };
    let mut preamble = String::new();
    let chosen = match asked {
        "auto" if arara && installed("arara") => "arara",
        "auto" if dir.join("Tectonic.toml").is_file() && installed("tectonic") => "tectonic",
        "auto" if latex && installed("latexmk") => "latexmk",
        "auto" => "engine",
        "engine" => "engine",
        "satex" => "satex",
        s @ ("latexmk" | "arara" | "tectonic" | "make" | "l3build") if installed(s) => s,
        s @ ("latexmk" | "arara" | "tectonic" | "make" | "l3build") => return Err(format!("{s} is not installed")),
        other => {
            return Err(format!(
                "unknown build system `{other}`: latexmk, arara, tectonic, make, l3build, engine or satex"
            ));
        }
    };
    let status = match chosen {
        // each reads its own configuration and reruns as the document needs
        "arara" => build("arara", &["--verbose"], file, &dir)?,
        "tectonic" => {
            build("tectonic", &["-X", "compile", "--synctex", "--keep-logs", "--keep-intermediates"], file, &dir)?
        }
        "make" => build("make", &[], "", &dir)?,
        "l3build" => build("l3build", &["doc"], "", &dir)?,
        // latexmk reruns as often as the document needs and reads the
        // latexmkrc; the engine is ours only where that names none.
        "latexmk" => {
            // `-f`: past an error, so the references the later runs resolve are not reported.
            let mut args = vec!["-interaction=nonstopmode", "-file-line-error", "-synctex=1", "-f", "-pv-", "-pvc-"];
            if !["pdf_mode", "pdflatex", "lualatex", "xelatex", "latex"].iter().any(|k| project.setting(k).is_some()) {
                args.push(match engine {
                    crate::project::config::Engine::LuaTeX => "-lualatex",
                    crate::project::config::Engine::XeTeX => "-xelatex",
                    _ => "-pdf",
                });
            }
            build("latexmk", &args, file, &dir)?
        }
        "satex" => {
            let at = out_dir.clone().unwrap_or_else(|| dir.clone());
            let c = &analysis.settings.compile;
            let warm = c.warm && cfg!(target_os = "linux");
            let (status, dumped) =
                own(program, latex, context.source, file, &job, &dir, &at, c.passes, warm, &c.clock, c.memoize)?;
            preamble = dumped;
            status
        }
        // The engine alone: again while the log asks for a rerun, as
        // LaTeX's cross-references need.
        _ => {
            let output = out_dir.as_ref().map(|d| format!("-output-directory={}", d.display()));
            let mut args = vec!["-interaction=nonstopmode", "-file-line-error"];
            args.push("-synctex=1");
            args.extend(output.as_deref());
            let mut status = build(program, &args, file, &dir)?;
            for _ in 0..2 {
                let log = std::fs::read_to_string(&log_path).unwrap_or_default();
                if !log.contains("Rerun to get") {
                    break;
                }
                status = build(program, &args, file, &dir)?;
            }
            status
        }
    };
    if view {
        open_viewer(&analysis.settings.compile.viewer, &log_path.with_extension("pdf"));
    }
    let log = std::fs::read(&log_path).map_err(|e| format!("{}: {e}", log_path.display()))?;
    // a preamble read from a format said its messages while the format was dumped
    let mut logged = parse_log(&preamble, &dir);
    logged.extend(parse_log(&String::from_utf8_lossy(&log), &dir));
    let failed = (!status.success() && !logged.iter().any(|l| l.error)).then(|| status.to_string());
    let head = head(&logged, &failed, &main, context);
    let keep = place.keep();
    let stored = stored(&keep, &job, &head).filter(|_| !logged.is_empty());
    Ok(Built { main, keep, job, logged, failed, head, stored, explaining: None })
}

/// The build files a run reads back: equal before and after a pass, the references rest.
const OWN: [&str; 16] =
    ["aux", "toc", "lof", "lot", "out", "bbl", "ind", "nav", "snm", "vrb", "gls", "acr", "nls", "loa", "lol", "thm"];
/// Whether a file of this extension is a build file a run reads back.
pub fn rests(ext: &str) -> bool {
    OWN.contains(&ext)
}
/// Where satex's build keeps its formats and the stamps of its tools' inputs.
pub const KEEP: &str = ".satex-build";

/// What LaTeX writes into the `.aux` of a one-page document without references, and what latexmk
/// puts there before a first pass: a pass that reads it and writes it again is at rest.
const SEED: &str = "\\relax \n\\gdef \\@abspage@last{1}\n";

/// satex's own build in `at`: the engine until the build files rest (at most `passes`), BibTeX,
/// Biber and makeindex where their inputs changed, a LaTeX preamble read from a format dumped once
/// per preamble, beside the first pass that needs it. A build without an `.aux` starts from
/// [`SEED`]. SyncTeX data is compressed beside the pass ([`follow`]). With `warm`, a pass is fed to the engine
/// the last build left waiting where it runs that pass's command (`warm.rs`), and the build leaves
/// one for the next. A build whose inputs are as the last rested one left them runs no pass
/// (`current`). With `memoize`, the build extracts memoize's externs itself where the document may
/// load it (`plugins::memoize`).
/// Gives the status and the dump's log.
#[allow(clippy::too_many_arguments)]
fn own(
    program: &str,
    latex: bool,
    source: &str,
    file: &str,
    job: &str,
    dir: &Path,
    at: &Path,
    passes: u32,
    warm: bool,
    clock: &str,
    memoize: bool,
) -> Result<(std::process::ExitStatus, String), String> {
    let started = std::time::Instant::now();
    let keep = at.join(KEEP);
    std::fs::create_dir_all(&keep).map_err(|e| format!("{}: {e}", keep.display()))?;
    let mut args = vec!["-interaction=nonstopmode".to_string(), "-file-line-error".into(), SYNCTEX.into()];
    // the .fls names every file a pass reads and writes, for `current`
    args.push("-recorder".into());
    // XeTeX hands its pages to xdvipdfmx, which waits for none while the pass runs (a third to
    // a half of a pass): the passes keep the pages, and the driver runs once after the last, as
    // latexmk has it.
    let paged = matches!(program, "xelatex" | "xetex");
    if paged {
        args.push("-no-pdf".into());
    }
    if at != dir {
        args.push(format!("-output-directory={}", at.display()));
    }
    // memoize's option goes before the file, as latexmk's -usepretex puts it; a dumped preamble
    // would not read it, so the passes read the preamble themselves.
    // pdfTeX reads a `%&format` line only from the file it is given: no code before it then
    let memoize = memoize
        && !source.starts_with("%&")
        && crate::plugins::memoize::may_load(source, &at.join(format!("{job}.fls")));
    // a warm engine reads a pipe in the main file's stead: its name stands where the file's would
    // only Linux has warm engines (cli::warm): elsewhere the pass reads the file itself
    #[cfg(target_os = "linux")]
    let (main, piped) = (file, super::warm::pipe(&keep, file).display().to_string());
    #[cfg(not(target_os = "linux"))]
    let (main, piped) = (file, file.to_string());
    let given = |file: &str| match memoize {
        true => format!("{}\\input{{{file}}}", crate::plugins::memoize::PRETEX),
        false => file.to_string(),
    };
    let (pretex, piped_arg) = (given(file), given(&piped));
    let file = pretex.as_str();
    if memoize {
        args.push(format!("-jobname={job}"));
    }
    let mut dump =
        if latex && program == "pdflatex" && !memoize { Dump::of(source, file, job, dir, &keep) } else { None };
    let formats = |args: &[String]| args.iter().any(|a| a.starts_with("-fmt="));
    let read = |ext: &str| std::fs::read(at.join(format!("{job}.{ext}"))).unwrap_or_default();
    let rested = || -> Vec<Vec<u8>> { OWN.iter().map(|e| read(e)).collect() };
    // The build files the last pass read (its `.fls`): latexmk reruns for a change of those
    // only, so a `.toc` no `\tableofcontents` reads lets the build rest.
    let read_back = || -> Vec<bool> {
        let fls = String::from_utf8_lossy(&read("fls")).into_owned();
        let inputs: Vec<&str> = fls.lines().filter_map(|l| l.strip_prefix("INPUT ")).collect();
        OWN.iter().map(|e| inputs.iter().any(|i| i.ends_with(&format!("{job}.{e}")))).collect()
    };
    // a changed .bib reruns BibTeX first: its new .bbl is a changed input
    tools(job, dir, at, &keep)?;
    let done = keep.join(format!("{job}.done"));
    // a pass reads the same from the format as from the preamble: the format is no part of the key
    let head = format!("{}\n", [program, &args.join(" "), file, &clock_key(clock)].join("\t"));
    #[cfg(unix)]
    if let Some(raw) = current(&done, &head) {
        eprintln!("satex: up to date, no file it reads changed ({:.2} s)", started.elapsed().as_secs_f64());
        let read = std::fs::read_to_string(&done).is_ok_and(|key| key.contains(".fmt\t"));
        let dumped = dump.filter(|_| read).map(|d| d.said()).unwrap_or_default();
        return Ok((std::os::unix::process::ExitStatusExt::from_raw(raw), dumped));
    }
    let _ = std::fs::remove_file(&done);
    // kpathsea's databases and search path, asked while the passes run
    let ask = |args: &[&str]| Command::new("kpsewhich").args(args).stdout(std::process::Stdio::piped()).spawn().ok();
    let asked = [ask(&["-all", "ls-R"]), ask(&["-show-path=tex"])];
    let aux = at.join(format!("{job}.aux"));
    if latex && !aux.exists() {
        std::fs::write(&aux, SEED).map_err(|e| format!("{}: {e}", aux.display()))?;
    }
    let mut was = rested();
    // a stale format is dumped aside: the passes read the preamble until it is there
    if let Some(d) = &dump {
        d.start();
    }
    let (mut status, mut ran, mut rest) = (None, 0, false);
    let command = |args: &[String]| -> Vec<String> {
        // web2c opens its input once before TeX does, to look for a `%&format` line: that open
        // would take the pipe's text and TeX's own would wait for more. A document with a format
        // of ours has no such line (`Dump::of`), so the look is told off.
        let last = ["-no-parse-first-line", piped_arg.as_str()];
        let words = [program].into_iter().chain(args.iter().map(String::as_str)).chain(last);
        words.map(str::to_string).collect()
    };
    let mut formatted = false;
    for _ in 0..passes.max(1) {
        ran += 1;
        if let Some(d) = dump.as_ref().filter(|d| !formats(&args) && d.usable()) {
            args.push(format!("-fmt={}", d.fmt.display()));
        }
        formatted = formats(&args);
        let pass_args: Vec<&str> = args.iter().map(String::as_str).collect();
        // the SyncTeX data names its inputs by their whole path
        let whole =
            |name: &str| format!("{}/./{name}", dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()).display());
        let followed = follow(at, job, warm.then(|| (piped.clone(), whole(main))));
        let ran_warm = warmed(warm && formatted, &keep, dir, &command(&args), main, &at.join(format!("{job}.log")));
        if ran_warm.is_some() {
            // what the engine wrote of its input's name is the main file's, as a fresh pass writes it
            // (the log's command line is the name as given, without the `./` of an opened file,
            // under the line of the look for a `%&` line that a fresh engine takes)
            let given = (format!("**{piped}"), format!(" %&-line parsing enabled.\n**{main}"));
            for (ext, name) in [("log", format!("./{main}")), ("fls", main.to_string())] {
                let path = at.join(format!("{job}.{ext}"));
                let text = std::fs::read(&path).unwrap_or_default();
                let text = replaced(&text, given.0.as_bytes(), given.1.as_bytes());
                let _ = std::fs::write(&path, replaced(&text, piped.as_bytes(), name.as_bytes()));
            }
        }
        let ran_pass = match ran_warm {
            Some(status) => Ok(status),
            None => build(program, &pass_args, file, dir),
        };
        followed.finish();
        status = Some(ran_pass?);
        tools(job, dir, at, &keep)?;
        // an extern made now leaves its page in this pass's PDF: one more pass drops it
        let mut extracted = false;
        for (args, script, extern_) in crate::plugins::memoize::extractions(job, at).into_iter().filter(|_| memoize) {
            build("pdftex", &args.each_ref().map(String::as_str), &script, at)?;
            extracted |= at.join(extern_).is_file();
        }
        let now = rested();
        let log = String::from_utf8_lossy(&read("log")).into_owned();
        // mylatexformat replays the preamble's \openout at \begin{document} and can break there (an
        // \openout under \if, glossaries' \makeglossaries): an error a plain run has not. The format
        // is unusable then, and the pass runs again on the preamble itself.
        // the proof's case in a pass, should a body ever break what an empty one does not
        if let Some(d) = dump.take_if(|_| formatted && log.contains(BROKEN)) {
            d.refuse();
            args.retain(|a| !a.starts_with("-fmt="));
            continue;
        }
        // one not there before the pass may be read by the next (a first `.toc`)
        let same = read_back().iter().zip(now.iter().zip(&was)).all(|(read, (n, w))| n == w || !read && !w.is_empty());
        if !extracted && same && !log.contains("Rerun to get") && !log.contains("Label(s) may have changed") {
            rest = true;
            break;
        }
        was = now;
    }
    let mut status = status.ok_or("no pass ran")?;
    let pages = at.join(format!("{job}.xdv"));
    if paged && pages.is_file() {
        let pdf = at.join(format!("{job}.pdf")).display().to_string();
        let driven = build("xdvipdfmx", &["-q", "-E", "-o", &pdf], &pages.display().to_string(), dir)?;
        if status.success() {
            status = driven;
        }
    }
    // the next build's passes read the format when it is there by now: its warm engine too
    if let Some(d) = dump.as_ref().filter(|d| !formats(&args) && d.usable()) {
        args.push(format!("-fmt={}", d.fmt.display()));
    }
    let dumped = dump.filter(|_| formatted).map(|d| d.said()).unwrap_or_default();
    // waited for also when unused: a long-lived `satex gui` keeps no zombies
    let [databases, path] = asked.map(|c| c.and_then(|c| c.wait_with_output().ok()).map(|o| o.stdout));
    let [databases, path] = [databases, path].map(|o| String::from_utf8_lossy(&o.unwrap_or_default()).into_owned());
    // only a build that rested gives what its next one would, failed or not: what it read is its key
    #[cfg(unix)]
    if rest {
        let raw = std::os::unix::process::ExitStatusExt::into_raw(status);
        let _ =
            std::fs::write(&done, format!("{head}{raw}\n{}", reads(dir, at, job, &keep, formatted, &databases, &path)));
    }
    #[cfg(not(unix))]
    let _ = (rest, head, databases, path, clock);
    eprintln!("satex: {ran} pass{} in {:.2} s", if ran == 1 { "" } else { "es" }, started.elapsed().as_secs_f64());
    #[cfg(not(target_os = "linux"))]
    let _ = (warm, command);
    // only on a format: that is the start a warm engine saves, and the document has no `%&` line then
    #[cfg(target_os = "linux")]
    if warm && formats(&args) {
        let held = super::warm::start(&keep, dir, &command(&args), main, &[(PRINT_LINE.0, PRINT_LINE.1)]);
        if let Some(seconds) = held {
            eprintln!("satex: a pdfTeX waits {seconds} s for the next build, then ends (compile.warm)");
        }
    }
    Ok((status, dumped))
}

/// Whether the engine or kpathsea may read the environment variable `key`: TeX's own, and every
/// lowercase one (a `texmf.cnf` setting is overridden by a variable of its name).
pub(super) fn read_by_tex(key: &str) -> bool {
    ["TEX", "BIB", "BST", "KPSE", "MF", "SOURCE_DATE"].iter().any(|p| key.contains(p))
        || key.starts_with(|c: char| c.is_ascii_lowercase())
}

/// What may part this build from the last one besides files: the environment the engine and
/// kpathsea read, and the clock at `clock`'s grain (`compile.clock`), unless it is fixed.
fn clock_key(clock: &str) -> String {
    let mut env: Vec<String> =
        std::env::vars().filter(|(k, _)| read_by_tex(k)).map(|(k, v)| format!("{k}={v}")).collect();
    env.sort_unstable();
    let fixed =
        std::env::var("FORCE_SOURCE_DATE").is_ok_and(|v| v == "1") && std::env::var("SOURCE_DATE_EPOCH").is_ok();
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 60);
    let grain = match (fixed, clock) {
        (true, _) => "fixed".to_string(),
        (_, "day") => format!("day {}", now / (24 * 60)),
        _ => format!("minute {now}"),
    };
    format!("{grain} {}", env.join(" "))
}

/// A file's line of the key: its path and content hash. A directory's: its path with a slash (two
/// when `deep`) and the hash of the names in it, `deep` to every level (a file the build looked for and did not
/// find appears there).
#[cfg(unix)]
fn stamp(path: &Path, deep: bool) -> String {
    let meta = std::fs::metadata(path);
    // a pipe or device gives other bytes each read: its line never holds again
    if meta.as_ref().is_ok_and(|m| !m.is_file() && !m.is_dir()) {
        return format!("{}\t{:?}\n", path.display(), std::time::SystemTime::now());
    }
    if !meta.is_ok_and(|m| m.is_dir()) {
        return format!("{}\t{:016x}\n", path.display(), hash(&std::fs::read(path).unwrap_or_default()));
    }
    fn names(dir: &Path, deep: bool, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            // satex's own files and the history move with every build or commit
            if name == KEEP || name == ".git" {
                continue;
            }
            out.push(e.path().display().to_string());
            if deep && e.file_type().is_ok_and(|t| t.is_dir()) {
                names(&e.path(), deep, out);
            }
        }
    }
    let mut out = Vec::new();
    names(path, deep, &mut out);
    out.sort_unstable();
    format!("{}{}\t{:016x}\n", path.display(), if deep { "//" } else { "/" }, hash(out.join("\n").as_bytes()))
}

/// The key of the build just run: every file its last pass and, when that read the format (`formatted`), the format's dump read or wrote
/// (`-recorder`), the directories outside `dir` they are in, `dir` and the TeX search path's disk
/// directories to every level, and kpathsea's file databases (`ls-R`): where a file found nowhere
/// would appear.
#[cfg(unix)]
fn reads(dir: &Path, at: &Path, job: &str, keep: &Path, formatted: bool, databases: &str, path: &str) -> String {
    // the dump's only when the pass read the format: one dumped aside is still being written
    let fls = [Some(at.join(format!("{job}.fls"))), formatted.then(|| keep.join(format!("{job}.fls")))];
    let text: String = fls.iter().flatten().map(|f| std::fs::read_to_string(f).unwrap_or_default()).collect();
    let mut files: Vec<PathBuf> = text
        .lines()
        .filter_map(|l| l.strip_prefix("INPUT ").or_else(|| l.strip_prefix("OUTPUT ")))
        .map(|p| dir.join(p))
        .filter(|p| !p.starts_with(keep) || p.extension().is_some_and(|e| e == "fmt"))
        .collect();
    files.extend(databases.lines().filter(|l| !l.is_empty()).map(PathBuf::from));
    // the PDF too, where a driver wrote it after the pass: a build without it is not the last one's
    files.push(at.join(format!("{job}.pdf")));
    files.sort_unstable();
    files.dedup();
    let root = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    let mut dirs: Vec<PathBuf> = files
        .iter()
        .filter_map(|f| f.canonicalize().ok()?.parent().map(Path::to_path_buf))
        .filter(|d| !d.starts_with(&root))
        .collect();
    dirs.sort_unstable();
    dirs.dedup();
    let mut key: String = files.iter().map(|f| stamp(f, false)).collect();
    key.extend(dirs.iter().map(|d| stamp(d, false)));
    key.extend([root].into_iter().chain(searched(dir, path)).map(|d| stamp(&d, true)));
    key
}

/// The status of the build the key in `done` was written by, when the key holds now: its head is
/// `head` and every file and directory in it stamps as it did.
#[cfg(unix)]
fn current(done: &Path, head: &str) -> Option<i32> {
    let key = std::fs::read_to_string(done).ok()?;
    let (raw, rest) = key.strip_prefix(head)?.split_once('\n')?;
    holds(rest).then(|| raw.parse().ok()).flatten()
}

/// Whether every line of a key (`stamp`) stamps as it did.
#[cfg(unix)]
fn holds(key: &str) -> bool {
    !key.is_empty()
        && key.lines().all(|l| {
            let path = l.split('\t').next().unwrap_or_default();
            let (path, deep) = match path.strip_suffix("//") {
                Some(d) => (d, true),
                None => (path.strip_suffix('/').unwrap_or(path), false),
            };
            stamp(Path::new(path), deep) == format!("{l}\n")
        })
}

/// The pass `command` by the warm engine (`warm.rs`), named on stderr as a run is.
#[cfg_attr(not(target_os = "linux"), allow(unused_variables))]
fn warmed(
    warm: bool,
    keep: &Path,
    dir: &Path,
    command: &[String],
    file: &str,
    log: &Path,
) -> Option<std::process::ExitStatus> {
    #[cfg(target_os = "linux")]
    if warm {
        let status = super::warm::take(keep, dir, command, file, log, &[PRINT_LINE])?;
        eprintln!("$ {} (warm)", command.join(" "));
        return Some(status);
    }
    None
}

/// Unwrapped log lines keep paths and messages whole. BibTeX refuses 20000 and more ("3 is a bad bad").
const PRINT_LINE: (&str, &str) = ("max_print_line", "10000");

/// BibTeX, Biber and makeindex, each where its input changed since it last ran or its output is missing.
fn tools(job: &str, dir: &Path, at: &Path, keep: &Path) -> Result<(), String> {
    let text = |ext: &str| std::fs::read_to_string(at.join(format!("{job}.{ext}"))).unwrap_or_default();
    let bib = |names: &mut dyn Iterator<Item = &str>| -> String {
        names.map(|n| std::fs::read_to_string(dir.join(n.trim()).with_extension("bib")).unwrap_or_default()).collect()
    };
    let aux = text("aux");
    // what BibTeX reads of the .aux, and the databases it names
    let cites: String =
        aux.lines().filter(|l| ["\\citation", "\\bibdata", "\\bibstyle"].iter().any(|p| l.starts_with(p))).collect();
    let data = aux.lines().filter_map(|l| l.strip_prefix("\\bibdata{")?.strip_suffix('}'));
    let cites = cites.clone() + &bib(&mut data.flat_map(|d| d.split(',')));
    let bcf = text("bcf");
    let sources =
        bcf.split("<bcf:datasource").skip(1).filter_map(|d| d.split_once('>')?.1.split_once('<').map(|(n, _)| n));
    let bcf = bcf.clone() + &bib(&mut sources.map(|n| n.trim_end_matches(".bib")));
    let idx = text("idx");
    let runs = [
        ("bibtex", aux.contains("\\bibdata{") && bcf.is_empty(), cites, "bbl"),
        ("biber", !bcf.is_empty(), bcf, "bbl"),
        ("makeindex", !idx.trim().is_empty(), idx, "ind"),
    ];
    for (tool, wanted, input, output) in runs {
        let stamp = keep.join(format!("{job}.{tool}"));
        let hash = format!("{:016x}", hash(input.as_bytes()));
        let missing = !at.join(format!("{job}.{output}")).is_file();
        if wanted && (missing || std::fs::read_to_string(&stamp).ok().as_deref() != Some(&hash)) {
            build(tool, &[], job, at)?;
            std::fs::write(&stamp, hash).map_err(|e| format!("{}: {e}", stamp.display()))?;
        }
    }
    Ok(())
}

fn hash(bytes: &[u8]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    hasher.finish()
}

/// The format of a LaTeX preamble (mylatexformat) under the build's keep. No build waits for it:
/// a build that finds it stale has a `satex` of its own dump it aside ([`Dump::aside`]) and reads
/// the preamble itself until the format is there and proven. A dump that fails, reports an error or
/// breaks at `\begin{document}` leaves it unusable: then every pass reads the preamble.
pub struct Dump {
    /// The format file, without its extension.
    fmt: PathBuf,
    /// The preamble's hash, the key's first line.
    head: String,
    /// The preamble, for the proof.
    preamble: String,
    main: Option<PathBuf>,
    dir: PathBuf,
    file: String,
}

/// `satex`'s first argument for [`Dump::aside`], then the directory, the main file, the job and the keep.
pub const DUMP: &str = "--compile-dump";

impl Dump {
    /// The format of `source`'s preamble, None where it has none to dump.
    fn of(source: &str, file: &str, job: &str, dir: &Path, keep: &Path) -> Option<Dump> {
        let preamble = &source[..source.find("\\begin{document}")?];
        // a `%&format` line picks a format of its own
        if source.starts_with("%&") {
            return None;
        }
        let head = format!("{:016x}\n", hash(preamble.as_bytes()));
        let main = dir.join(file).canonicalize().ok();
        let (preamble, dir, file) = (preamble.to_string(), dir.to_path_buf(), file.to_string());
        Some(Dump { fmt: keep.join(job), head, preamble, main, dir, file })
    }

    fn key(&self) -> PathBuf {
        self.fmt.with_extension("key")
    }

    /// A file's line of the key: its path and content hash.
    fn stamp(path: &str) -> String {
        format!("{path}\t{:016x}\n", hash(&std::fs::read(path).unwrap_or_default()))
    }

    /// The key's lines after its head, when it is of this preamble and every file the dump read
    /// (but the main one) is as it was.
    fn fresh(&self) -> Option<String> {
        let stored = std::fs::read_to_string(self.key()).ok()?;
        let rest = stored.strip_prefix(&self.head)?;
        let same = |l: &str| l == UNUSABLE || Self::stamp(l.split('\t').next().unwrap_or_default()) == format!("{l}\n");
        rest.lines().all(same).then(|| rest.to_string())
    }

    /// Whether passes can read the format now: its key holds and no dump is writing it.
    fn usable(&self) -> bool {
        self.fresh().is_some_and(|rest| !rest.lines().any(|l| l == UNUSABLE))
            && self.fmt.with_extension("fmt").is_file()
            && self.lock().is_some()
    }

    /// The dump's lock, None while a dump holds it.
    fn lock(&self) -> Option<std::fs::File> {
        super::cache::background::try_lock(&self.fmt.with_extension("dumping")).ok().flatten()
    }

    /// Has a detached `satex` dump a stale format ([`Dump::aside`]), unless one is at it.
    fn start(&self) {
        let Some(lock) = self.lock().filter(|_| self.fresh().is_none()) else { return };
        drop(lock);
        // no key until the dump is done: one that ends early is started again
        let _ = std::fs::remove_file(self.key());
        let Ok(satex) = std::env::current_exe() else { return };
        let (job, keep) = (self.fmt.file_name().unwrap_or_default(), self.fmt.parent().unwrap_or(&self.dir));
        let mut command = super::cache::background::detached(&satex, None, true);
        command.arg(DUMP).args([self.dir.as_os_str(), self.file.as_ref(), job, keep.as_os_str()]);
        command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
        command.stderr(std::process::Stdio::null());
        eprintln!("satex: dumping the preamble's format aside, for the next passes");
        // reaped by a thread: a long-lived caller (`satex gui`) keeps no zombie
        if let Ok(mut child) = command.spawn() {
            std::thread::spawn(move || child.wait());
        }
    }

    /// `satex --compile-dump DIR FILE JOB KEEP`: dumps the format of `FILE`'s preamble and proves it,
    /// then writes its key. One at a time per job (the lock).
    pub fn aside(dir: &Path, file: &str, job: &str, keep: &Path) {
        let Ok(source) = std::fs::read(dir.join(file)) else { return };
        let Some(dump) = Dump::of(&String::from_utf8_lossy(&source), file, job, dir, keep) else { return };
        let Some(_lock) = dump.lock().filter(|_| dump.fresh().is_none()) else { return };
        let run = |program: &str, args: &[&str]| {
            let mut command = Command::new(program);
            command.args(args).current_dir(dir).env(PRINT_LINE.0, PRINT_LINE.1);
            command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
            command.status().is_ok_and(|s| s.success())
        };
        let out = format!("-output-directory={}", keep.display());
        let name = format!("-jobname={job}");
        let ini = ["-ini", "-interaction=nonstopmode", "-file-line-error", "-recorder"];
        let dumped = run("pdftex", &[&ini[..], &[&out, &name, "&pdflatex", "mylatexformat.ltx", file]].concat());
        let fls = std::fs::read_to_string(dump.fmt.with_extension("fls")).unwrap_or_default();
        let mut inputs: Vec<&str> = fls.lines().filter_map(|l| l.strip_prefix("INPUT ")).collect();
        inputs.sort_unstable();
        inputs.dedup();
        inputs.retain(|p| dir.join(p).canonicalize().ok() != dump.main);
        let usable = dumped && !parse_log(&dump.said(), dir).iter().any(|l| l.error) && dump.proven(&run);
        let mut text = dump.head.clone();
        text.extend(inputs.iter().map(|p| Self::stamp(&dir.join(p).display().to_string())));
        if !usable {
            text.push_str(UNUSABLE);
            text.push('\n');
        }
        let _ = std::fs::write(dump.key(), text);
    }

    /// Whether the format reads on into a document: mylatexformat replays the preamble's `\openout`
    /// at `\begin{document}` and can break there (an `\openout` under `\if`, glossaries'
    /// `\makeglossaries`), which a pass on the preamble does not. The proof is a draft pass of the
    /// preamble with an empty body on the format, in a directory of its own with an `.aux` of its own.
    fn proven(&self, run: &dyn Fn(&str, &[&str]) -> bool) -> bool {
        let at = self.fmt.with_extension("proof");
        let probe = at.join(&self.file);
        let text = format!("{}\\begin{{document}}\\end{{document}}\n", self.preamble);
        let job = self.fmt.file_name().map(|j| j.to_string_lossy().into_owned()).unwrap_or_default();
        // its own `.aux`, found before the build's: a pass may be writing that one
        let written = std::fs::create_dir_all(probe.parent().unwrap_or(&at)).is_ok()
            && std::fs::write(&probe, text).is_ok()
            && std::fs::write(at.join(format!("{job}.aux")), SEED).is_ok();
        let args = [
            "-interaction=nonstopmode".to_string(),
            "-file-line-error".into(),
            "-draftmode".into(),
            format!("-output-directory={}", at.display()),
            format!("-jobname={job}"),
            format!("-fmt={}", self.fmt.display()),
            probe.display().to_string(),
        ];
        let ran = written && run("pdflatex", &args.each_ref().map(String::as_str));
        let log = std::fs::read_to_string(at.join(format!("{job}.log"))).unwrap_or_default();
        let _ = std::fs::remove_dir_all(&at);
        ran && !log.contains(BROKEN) && !parse_log(&log, &self.dir).iter().any(|l| l.error)
    }

    /// Marks the format unusable: a pass that read it broke where a pass on the preamble does not.
    fn refuse(&self) {
        let _ = std::fs::write(self.key(), std::fs::read_to_string(self.key()).unwrap_or_default() + UNUSABLE + "\n");
    }

    /// The dump's log: what the preamble said while the format was dumped.
    fn said(&self) -> String {
        std::fs::read_to_string(self.fmt.with_extension("log")).unwrap_or_default()
    }
}

/// The key's line of a format no pass may read.
const UNUSABLE: &str = "unusable";
/// What a log names when the format's replay of the preamble broke.
const BROKEN: &str = "\\MYLATEX@";

/// The engine's SyncTeX option: uncompressed. pdfTeX's own gzip is a quarter of a long pass
/// (1000 pages: 2.8 s without, 3.8 s with), [`follow`] compresses on another core meanwhile.
const SYNCTEX: &str = "-synctex=-1";

/// `text` with every `from` replaced by `to`.
fn replaced(text: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.windows(from.len().max(1)).position(|w| w == from) {
        out.extend_from_slice(&rest[..at]);
        out.extend_from_slice(to);
        rest = &rest[at + from.len()..];
    }
    out.extend_from_slice(rest);
    out
}

/// Whole lines of SyncTeX data with the input named `from` named `to`, as the engine would have
/// written them: an `Input:` line gets the name, and each `!⟨bytes⟩` record, which counts the
/// data's bytes since the record before it, loses the bytes taken out in between (`less`, kept
/// across calls; a record that got shorter itself counts for the next).
fn renamed(lines: &[u8], from: &[u8], to: &[u8], less: &mut i64) -> Vec<u8> {
    let mut out = Vec::with_capacity(lines.len());
    for line in lines.split_inclusive(|&b| b == b'\n') {
        let text = line.strip_suffix(b"\n").unwrap_or(line);
        let count = text.strip_prefix(b"!").filter(|d| !d.is_empty() && d.iter().all(u8::is_ascii_digit));
        let count = count.and_then(|d| std::str::from_utf8(d).ok()?.parse::<i64>().ok());
        if text.starts_with(b"Input:") && text.ends_with(from) {
            out.extend_from_slice(&text[..text.len() - from.len()]);
            out.extend_from_slice(to);
            *less += from.len() as i64 - to.len() as i64;
        } else if let Some(count) = count {
            let moved = format!("!{}", count - *less);
            *less = text.len() as i64 - moved.len() as i64;
            out.extend_from_slice(moved.as_bytes());
        } else {
            out.extend_from_slice(text);
        }
        out.extend_from_slice(&line[text.len()..]);
    }
    out
}

/// The compression of the SyncTeX data the next pass in `at` writes, running beside that pass.
struct Followed {
    done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Followed {
    /// The pass ended: the rest is compressed and `job.synctex.gz` is in place.
    fn finish(mut self) {
        self.done.store(true, std::sync::atomic::Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Follows the SyncTeX file the pass about to run writes (`job.synctex(busy)`, renamed to
/// `job.synctex` at its end) and feeds what it reads to `gzip`: `job.synctex.gz` is whole when the
/// pass is, as the engine itself would have written it. Without gzip the uncompressed file stays,
/// which viewers read too. A pass that ends without SyncTeX data leaves the files as they were.
/// `named`: an input's name the data may hold and the name to put in its place (a warm engine's pipe).
fn follow(at: &Path, job: &str, named: Option<(String, String)>) -> Followed {
    use std::io::{Read, Write};
    use std::sync::atomic::Ordering;
    let name = |ext: &str| at.join(format!("{job}.{ext}"));
    let (busy, plain, gz, next) = (name("synctex(busy)"), name("synctex"), name("synctex.gz"), name("synctex.gz.next"));
    let _ = (std::fs::remove_file(&busy), std::fs::remove_file(&plain));
    let done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let ended = done.clone();
    let thread = std::thread::spawn(move || {
        let pause = || std::thread::sleep(std::time::Duration::from_millis(2));
        let open = || std::fs::File::open(&busy).or_else(|_| std::fs::File::open(&plain));
        let mut data = loop {
            let last = ended.load(Ordering::SeqCst);
            match open() {
                Ok(file) => break file,
                Err(_) if last => return,
                Err(_) => pause(),
            }
        };
        let Ok(out) = std::fs::File::create(&next) else { return };
        let gzip = Command::new("gzip").arg("-c").stdin(std::process::Stdio::piped()).stdout(out).spawn();
        let Ok(mut gzip) = gzip else {
            // the uncompressed data is the pass's: no older compressed file beside it
            let _ = (std::fs::remove_file(&next), std::fs::remove_file(&gz));
            return;
        };
        let mut pipe = gzip.stdin.take();
        let mut chunk = vec![0; 1 << 16];
        let mut whole = true;
        // whole lines only: a name to replace is never cut in two
        let mut lines: Vec<u8> = Vec::new();
        // bytes the renaming took out since the data's last `!` record
        let mut less = 0i64;
        let mut send = |bytes: &[u8]| match &named {
            Some((from, to)) => {
                let named = renamed(bytes, from.as_bytes(), to.as_bytes(), &mut less);
                pipe.as_mut().is_some_and(|p| p.write_all(&named).is_ok())
            }
            None => pipe.as_mut().is_some_and(|p| p.write_all(bytes).is_ok()),
        };
        loop {
            let last = ended.load(Ordering::SeqCst);
            match data.read(&mut chunk) {
                Ok(0) if last => break,
                Ok(0) => pause(),
                Ok(n) => {
                    lines.extend_from_slice(&chunk[..n]);
                    let end = lines.iter().rposition(|&b| b == b'\n').map_or(0, |i| i + 1);
                    whole &= send(&lines[..end]);
                    lines.drain(..end);
                }
                Err(_) => whole = false,
            }
            if !whole {
                break;
            }
        }
        whole &= send(&lines);
        drop(pipe.take());
        whole &= gzip.wait().is_ok_and(|s| s.success());
        // an aborted pass removes its data: what was read of it is no file
        match whole && plain.is_file() {
            true => {
                let _ = (std::fs::rename(&next, &gz), std::fs::remove_file(&plain));
            }
            false => {
                let _ = std::fs::remove_file(&next);
            }
        }
    });
    Followed { done, thread: Some(thread) }
}

/// Opens `pdf` in `viewer` (`auto`/`none`: the system's opener), detached.
fn open_viewer(viewer: &str, pdf: &Path) {
    let program = match viewer {
        "auto" | "none" | "" if cfg!(target_os = "macos") => "open",
        "auto" | "none" | "" if cfg!(windows) => "explorer",
        "auto" | "none" | "" => "xdg-open",
        other => other,
    };
    eprintln!("$ {program} {}", pdf.display());
    let spawned = Command::new(program)
        .arg(pdf)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if let Err(e) = spawned {
        eprintln!("satex: cannot open the PDF with {program}: {e}");
    }
}

/// The errors (`file:line: message`, as `-file-line-error` writes them) and
/// the LaTeX, class and package warnings of a log, each with the file TeX
/// was reading: the innermost `(file` the log opened and has not closed.
/// One run of `program` on `file` in `dir`. Its output is progress on
/// stderr; the findings are the result.
/// Each run is named on stderr first, as a shell shows it (`$ latexmk …`).
fn build(program: &str, args: &[&str], file: &str, dir: &Path) -> Result<std::process::ExitStatus, String> {
    eprintln!("$ {program} {} {file}", args.join(" "));
    Command::new(program)
        .args(args)
        .args((!file.is_empty()).then_some(file))
        .current_dir(dir)
        .env(PRINT_LINE.0, PRINT_LINE.1)
        .stdout(std::process::Stdio::from(std::io::stderr()))
        .status()
        .map_err(|e| format!("cannot run {program}: {e}"))
}

fn parse_log(log: &str, dir: &Path) -> Vec<Logged> {
    let mut out = Vec::new();
    let mut files: Vec<PathBuf> = Vec::new();
    let mut lines = log.lines().peekable();
    while let Some(line) = lines.next() {
        if let Some((path, number, message)) = file_line_error(line) {
            let mut message = message.to_string();
            // A package error goes on in lines that start with `(package)`.
            while let Some(next) = lines.peek().filter(|l| l.starts_with('(') && l.contains(')')) {
                message.push(' ');
                message.push_str(next.split_once(')').map_or("", |(_, rest)| rest).trim());
                lines.next();
            }
            out.push(Logged { path: Some(dir.join(path)), line: Some(number), error: true, message });
            continue;
        }
        if let Some(at) = ["LaTeX Warning: ", "Package ", "Class "].iter().find_map(|p| line.find(p))
            && line[at..].contains("Warning: ")
        {
            let mut message = line[at..].to_string();
            while let Some(next) = lines.peek().filter(|l| l.starts_with('(') || l.starts_with("  ")) {
                message.push(' ');
                message.push_str(next.split_once(')').map_or(*next, |(_, rest)| rest).trim());
                lines.next();
            }
            // The end-of-run summaries repeat what was said where it arose.
            if message.starts_with("LaTeX Warning: There were") || message.contains("Label(s) may have changed") {
                continue;
            }
            let number = message.rsplit_once("on input line ").and_then(|(_, n)| n.trim_end_matches('.').parse().ok());
            out.push(Logged { path: files.last().cloned(), line: number, error: false, message });
            continue;
        }
        // TeX's fatal ends without a file and line: `*** (job aborted, no legal \end found)`
        // (tex.web § 360) and `! Emergency stop.` (§ 93)
        // (an error with its place said so already: `! Emergency stop.` follows it)
        let unplaced = line.strip_prefix("! ").filter(|_| !out.iter().any(|l: &Logged| l.error));
        if let Some(message) = line.strip_prefix("*** (").and_then(|m| m.strip_suffix(')')).or(unplaced) {
            out.push(Logged { path: files.last().cloned(), line: None, error: true, message: message.to_string() });
            continue;
        }
        // `Overfull \hbox (12.3pt too wide) in paragraph at lines 4--7`: by how much, where it begins
        if line.starts_with("Overfull \\") || line.starts_with("Underfull \\") {
            let number = ["at lines ", "at line "]
                .iter()
                .find_map(|p| line.rsplit_once(p))
                .and_then(|(_, n)| n.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok());
            out.push(Logged { path: files.last().cloned(), line: number, error: false, message: line.to_string() });
            continue;
        }
        track_files(line, dir, &mut files);
    }
    out
}

/// `./chapter.tex:12: Undefined control sequence.`
fn file_line_error(line: &str) -> Option<(&str, u64, &str)> {
    let (head, message) = line.split_once(": ")?;
    let (path, number) = head.rsplit_once(':')?;
    let number = number.parse().ok()?;
    (path.ends_with(".tex") || path.ends_with(".sty") || path.ends_with(".cls") || path.contains('/'))
        .then_some((path, number, message))
}

/// The files a log line opens with `(path` and closes with `)`.
fn track_files(line: &str, dir: &Path, files: &mut Vec<PathBuf>) {
    let mut rest = line;
    while let Some(i) = rest.find(['(', ')']) {
        if rest[i..].starts_with(')') {
            files.pop();
            rest = &rest[i + 1..];
            continue;
        }
        let name: String = rest[i + 1..].chars().take_while(|c| !c.is_whitespace() && !"()".contains(*c)).collect();
        let opened = name.contains('.') && (name.starts_with('.') || name.starts_with('/') || name.contains('/'));
        files.push(if opened { dir.join(&name) } else { PathBuf::new() });
        rest = &rest[i + 1 + name.len()..];
    }
}

/// A message without its numbers: the same message of another build, whose lines and pages moved.
fn worded(message: &str) -> String {
    message.chars().filter(|c| !c.is_ascii_digit()).collect()
}

/// What follows an explanation taken from the analysis of an earlier text.
const EARLIER: &str = " (said by the analysis of an earlier text)";

/// The record for one logged message, with satex's finding at that place, else what the last
/// analysis said of the same message at that file (`earlier`, marked as such), else the summary
/// of the rule the message is named by, else what TeX means by it ([`SAID`]).
fn explain(logged: &Logged, findings: &[Record], earlier: &[Record], main: &Path) -> Record {
    let path = logged.path.clone().filter(|p| p.is_file()).unwrap_or_else(|| main.to_path_buf());
    let same = |p: &str| Path::new(p).canonicalize().ok() == path.canonicalize().ok();
    let near = |r: &&Record| {
        r.get("path").and_then(|p| p.as_str()).is_some_and(same)
            && match (logged.line, r.get("line").and_then(|l| l.as_u64())) {
                (Some(a), Some(b)) => a.abs_diff(b) <= 1,
                _ => false,
            }
    };
    let code = code_of(&logged.message);
    // A finding explains the message only when it is about the same thing:
    // the same rule, and the key the message quotes (`` `knuth84' ``).
    let key = logged.message.split_once('`').and_then(|(_, rest)| rest.split_once('\'')).map(|(key, _)| key);
    let about = |r: &&Record| {
        let text = r.get("message").and_then(|m| m.as_str()).unwrap_or_default();
        code.is_none_or(|c| r.get("code").and_then(|c| c.as_str()) == Some(c)) && key.is_none_or(|k| text.contains(k))
    };
    let finding = findings.iter().filter(near).find(about);
    let by_rule = || {
        let rule = code.and_then(|c| crate::linter::all().find(|r| r.code == c)).map(|r| r.summary.to_string());
        rule.or_else(|| SAID.iter().find(|(text, _)| logged.message.contains(text)).map(|(_, why)| why.to_string()))
    };
    // the same message in the same file: only its numbers may differ
    let before = || {
        let text = |r: &Record, key: &str| r.get(key).and_then(|m| m.as_str()).map(str::to_string);
        let again = |r: &&Record| {
            text(r, "path").is_some_and(|p| same(&p))
                && text(r, "message").is_some_and(|m| worded(&m) == worded(&logged.message))
        };
        let fix = earlier.iter().find(again).and_then(|r| text(r, "fix"))?;
        let fix = fix.strip_suffix(EARLIER).unwrap_or(&fix).to_string();
        // a rule's summary is no finding: it is said without the mark
        (Some(&fix) != by_rule().as_ref()).then(|| format!("{fix}{EARLIER}"))
    };
    let why = match finding {
        Some(f) => {
            let text = f.get("message").and_then(|m| m.as_str()).unwrap_or_default();
            let fix = f.get("fix").and_then(|m| m.as_str()).map(|fix| format!(". {fix}")).unwrap_or_default();
            Some(format!("{text}{fix}"))
        }
        None => before().or_else(by_rule),
    };
    let code = code.unwrap_or(if logged.error { "tex-error" } else { "tex-warning" });
    record(&path, logged.line, logged.error, code, logged.message.clone(), why)
}

/// What TeX and LaTeX mean by the messages no lint rule is about, in plain words.
const SAID: [(&str, &str); 11] = [
    (
        "Underfull \\hbox",
        "a line is set loose: its spaces are stretched past what \\tolerance and \\hbadness accept. Rephrase, allow a hyphenation, or accept it",
    ),
    (
        "Underfull \\vbox",
        "a page or column is not filled: its stretchable space cannot reach the height, often before a heading, float or display that did not fit. \\raggedbottom accepts it",
    ),
    (
        "Overfull \\vbox",
        "a page or box is higher than its height: its material cannot shrink enough and sticks out below",
    ),
    ("Font shape", "the font has no such shape, series or size: LaTeX set the substitute it names"),
    ("float specifier changed", "the float cannot go where its specifier says: LaTeX added the place it names"),
    ("Float too large", "the float is higher than the page: it sticks out below"),
    ("Marginpar on page", "a margin note would overlap the one above it and was moved down"),
    (
        "destination with the same identifier",
        "two link targets share a name, often pages numbered alike: links go to the first",
    ),
    (
        "Token not allowed in a PDF string",
        "a bookmark is plain text: hyperref dropped the command it names. \\texorpdfstring gives the bookmark its own text",
    ),
    ("Unused global option", "no package used this class option: a misspelling, or an option of a package not loaded"),
    ("Empty bibliography", "the bibliography has no entry: nothing was cited, or BibTeX or Biber did not run"),
];

/// The lint rule a TeX or LaTeX message is.
fn code_of(message: &str) -> Option<&'static str> {
    const NAMED: [(&str, &str); 8] = [
        ("Overfull \\hbox (", "overfull-line"),
        ("Undefined control sequence", "undefined-control-sequence"),
        ("Environment ", "undefined-environment"),
        ("' not found", "unresolved-file"),
        ("multiply defined", "duplicate-label"),
        ("Citation `", "undefined-citation"),
        ("Reference `", "undefined-reference"),
        ("Missing number", "missing-number"),
    ];
    // a box packed to a width (`detected at line`) is another rule than a paragraph's line
    if message.starts_with("Overfull \\hbox") && message.contains("detected at line") {
        return Some("overfull-box");
    }
    NAMED.iter().find(|(text, _)| message.contains(text)).map(|(_, code)| *code)
}

fn record(path: &Path, line: Option<u64>, error: bool, code: &str, message: String, why: Option<String>) -> Record {
    let mut r = Record::new();
    let file = path.file_name().and_then(|f| f.to_str()).unwrap_or_default();
    r.insert("file".into(), json!(file));
    r.insert("path".into(), json!(path.display().to_string()));
    r.insert("line".into(), json!(line));
    r.insert("col".into(), json!(1));
    r.insert("code".into(), json!(code));
    r.insert("category".into(), json!("correctness"));
    r.insert("severity".into(), json!(if error { "error" } else { "warning" }));
    r.insert("origin".into(), json!("build"));
    r.insert("message".into(), json!(message));
    r.insert("fix".into(), json!(why));
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_log_gives_its_errors_and_warnings_where_they_arose() {
        let log = "This is pdfTeX\n(./main.tex (/usr/share/article.cls)\n(./intro.tex\n\
                   ./intro.tex:3: Undefined control sequence.\nl.3 \\foo\n\n\
                   LaTeX Warning: Reference `x' on page 1 undefined on input line 5.\n)\n\
                   ./main.tex:9: Package foo Error: bad\n(foo)                more here.\n)";
        let got = parse_log(log, Path::new("/d"));
        assert_eq!(got.len(), 3, "{got:?}");
        assert_eq!(got[0].path.as_deref(), Some(Path::new("/d/./intro.tex")));
        assert_eq!((got[0].line, got[0].error), (Some(3), true));
        assert_eq!(got[1].path.as_deref(), Some(Path::new("/d/./intro.tex")));
        assert_eq!((got[1].line, got[1].error), (Some(5), false));
        assert_eq!(got[2].message, "Package foo Error: bad more here.");
        assert_eq!(code_of(&got[0].message), Some("undefined-control-sequence"));
        assert_eq!(code_of(&got[1].message), Some("undefined-reference"));
    }

    #[test]
    fn a_bad_box_is_a_warning_at_its_first_line() {
        let log = "(./main.tex\nOverfull \\hbox (12.3pt too wide) in paragraph at lines 4--7\n[]\\OT1/cmr/m/n/10 x\n\n\
                   Underfull \\vbox (badness 10000) detected at line 9\n)";
        let got = parse_log(log, Path::new("/d"));
        assert_eq!(got.iter().map(|g| (g.line, g.error)).collect::<Vec<_>>(), [(Some(4), false), (Some(9), false)]);
        assert_eq!(got[0].path.as_deref(), Some(Path::new("/d/./main.tex")));
        assert!(got[0].message.contains("12.3pt too wide"), "{got:?}");
    }

    /// SyncTeX data names each input once and counts, in every `!` record, the bytes since the
    /// record before. A warm engine wrote the pipe's name: with the main file's name in its place
    /// the next record counts the difference less, and a record that lost a digit by that passes
    /// one byte on to the record after it. Pins: data that viewers read with counts that are off.
    #[test]
    fn synctex_data_renamed_counts_as_if_written_so() {
        let data = |name: &str, counts: [usize; 3]| {
            format!(
                "SyncTeX Version:1\nInput:1:{name}\nInput:2:/t/a.sty\n!{}\n{{1\n!{}\n}}1\n!{}\n",
                counts[0], counts[1], counts[2]
            )
        };
        // the first record counts the head (1000 with the pipe's name, 13 bytes longer than the file's)
        let (pipe, file) = ("/p/.satex-build/warm/in/d.tex", "/p/./d.tex");
        let less = pipe.len() - file.len();
        let (written, wanted) = (data(pipe, [1000, 9, 12]), data(file, [1000 - less, 8, 12]));
        let mut carried = 0;
        // fed in two parts, as the follower reads it: what was taken out is carried over
        let (head, tail) = written.split_at(written.find("!1000").unwrap());
        let mut got = renamed(head.as_bytes(), pipe.as_bytes(), file.as_bytes(), &mut carried);
        got.extend(renamed(tail.as_bytes(), pipe.as_bytes(), file.as_bytes(), &mut carried));
        assert_eq!(String::from_utf8(got).unwrap(), wanted);
    }

    #[test]
    fn a_finding_explains_only_the_message_about_its_key() {
        let main = Path::new("main.tex");
        let finding = |code: &str, message: &str| record(main, Some(4), false, code, message.into(), None);
        let findings = [
            finding("space-after-command", "the space after \\TeX is lost"),
            finding("undefined-reference", "no `\\label{sec:nope}` anywhere"),
        ];
        let logged = |message: &str| Logged { path: None, line: Some(4), error: false, message: message.into() };
        let why = |message: &str| explain(&logged(message), &findings, &[], main)["fix"].as_str().map(str::to_owned);
        assert_eq!(why("Reference `sec:nope' on page 1 undefined").as_deref(), Some("no `\\label{sec:nope}` anywhere"));
        // another key on that line: the rule's summary, not the finding about `sec:nope`
        assert!(why("Reference `sec:other' on page 1 undefined").is_some_and(|w| !w.contains("sec:nope")));
        // a citation: no finding of its rule there, so none of the others either
        assert!(why("Citation `knuth84' on page 1 undefined").is_some_and(|w| !w.contains("TeX")));
    }
}
