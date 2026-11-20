//! The background builder (`cache.background`): asked once, satex prepares
//! the kernels and the installed packages in a detached, low-priority
//! `satex cache build --background`, so later runs start from caches.
//!
//! One builder per cache directory. It holds no build lock, so a foreground
//! run never waits on it: a run missing a cache builds it itself, and of two
//! writers one stores (`StoreLock`). Its files in the cache directory, none
//! named `.lock` or `.postcard` so pruning and `cache clear` leave them:
//! - `background.run`: locked while it runs; keeps it single.
//! - `background.start`: locked while a run decides to start one.
//! - `background.state`: `started|done|full|stopped|failed <cache key>`.
//!   `started` left by a builder that is gone means it crashed.
//! - `background.stop`: left by `satex cache stop`; only a builder run by
//!   hand clears it.
//! - `background.answer`: the setup answer where the global config could
//!   not take it.
//! - `background.log`: what it did, appended.
//!
//! A state for this satex build other than `stopped` keeps it from being
//! started again; a new satex build starts it again.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

use crate::project::config::{Background, Config};

/// Never ask in this environment (set in CI, scripts, the builder itself).
pub const NO_SETUP: &str = "SATEX_NO_SETUP";

/// Set for a builder a run started: it never clears `background.stop`.
pub const STARTED_BY_RUN: &str = "SATEX_BACKGROUND_STARTED";

/// Where `nice` may be: absolute paths only, never a `PATH` entry, which may
/// be relative or the current directory and run a planted `./nice`.
const NICE: [&str; 2] = ["/usr/bin/nice", "/bin/nice"];

/// How long a start waits for the builder to take its lock, so two runs
/// starting at once never both spawn one.
const START_WAIT: std::time::Duration = std::time::Duration::from_secs(5);

/// The question, with what it costs and where it goes.
pub fn question(cfg: &Config, dir: &Path, global: Option<&Path>) -> String {
    format!(
        "Analyze the TeX kernels and installed packages in the background so later runs start fast? \
         It runs at the lowest priority, writes up to {} (limits.cache_size) into {}, and stops with \
         `satex cache stop`. The answer is kept as cache.background in {}.",
        bytesize::ByteSize(cfg.limits.cache_size.as_u64()),
        dir.display(),
        global.map_or("the global config".into(), |p| p.display().to_string()),
    )
}

/// Whether `var` (the environment) says never to ask nor build:
/// `SATEX_NO_SETUP` or `CI` set, or the browser.
pub fn opted_out(var: impl Fn(&str) -> Option<std::ffi::OsString>) -> bool {
    let set = |name: &str| var(name).is_some_and(|v| !v.is_empty());
    cfg!(target_os = "emscripten") || set(NO_SETUP) || set("CI")
}

/// [`offer`] at a command-line start. Asks only when stdin, stdout and
/// stderr are terminals, so a run with piped output never waits for an
/// answer, and only when `may_ask` (the command reads no terminal input of
/// its own).
pub fn at_start(cfg: &Config, may_ask: bool) {
    use crate::util::progress::terminal;
    if opted_out(|name| std::env::var_os(name)) {
        return;
    }
    let interactive =
        may_ask && terminal(std::io::stdin()) && terminal(std::io::stdout()) && terminal(std::io::stderr());
    let Ok(exe) = std::env::current_exe() else { return };
    let global = crate::project::config::global_config_file();
    offer(cfg, interactive, global.as_deref(), &exe, &mut std::io::stdin().lock(), &mut std::io::stderr());
}

/// The answer a line gives: enter, `y`, `yes`, `n`, `no`, any case.
fn answer_of(line: &str) -> Option<Background> {
    match line.trim().to_ascii_lowercase().as_str() {
        "" | "y" | "yes" => Some(Background::Yes),
        "n" | "no" => Some(Background::No),
        _ => None,
    }
}

/// At a foreground start: with `cache.background: yes` start the builder if
/// it has work; with `ask` and an `interactive` terminal ask once, record
/// the answer in `global` and start it on yes. An answer that is none of
/// [`answer_of`]'s is asked once more, then left for next time. Returns
/// what holds now.
pub fn offer(
    cfg: &Config,
    interactive: bool,
    global: Option<&Path>,
    exe: &Path,
    input: &mut impl BufRead,
    out: &mut impl Write,
) -> Background {
    let Some(dir) = crate::caches::format::enabled_cache_dir(cfg) else { return Background::No };
    let mut choice = cfg.cache.background;
    if choice == Background::Ask {
        choice = recorded(&dir).unwrap_or(Background::Ask);
    }
    if choice == Background::Ask && interactive {
        let mut asked = None;
        for _ in 0..2 {
            let _ = write!(out, "satex: {} [Y/n] ", question(cfg, &dir, global));
            let _ = out.flush();
            let mut line = String::new();
            // A closed input is no answer: asked again next time.
            if input.read_line(&mut line).is_err() || line.is_empty() {
                return Background::Ask;
            }
            asked = answer_of(&line);
            if asked.is_some() {
                break;
            }
            let _ = writeln!(out, "satex: answer y or n");
        }
        let Some(answer) = asked else { return Background::Ask };
        choice = answer;
        let kept = global.ok_or_else(|| "no global config directory".to_string()).and_then(|f| record(f, choice));
        if let Err(why) = kept {
            // Kept beside the caches instead, so it is not asked again.
            let _ = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(file(&dir, "answer"), choice.as_str()));
            let _ = writeln!(
                out,
                "satex: {why}. Add `cache: {{background: {}}}` to the global config to keep it there",
                choice.as_str()
            );
        }
    }
    if choice == Background::Yes {
        match start(cfg, &dir, exe) {
            Ok(Some(_)) => {
                let _ = writeln!(out, "satex: preparing caches in the background (`satex cache` shows how far)");
            }
            Ok(None) => {}
            Err(why) => {
                let _ = writeln!(out, "satex: the background build did not start: {why}");
            }
        }
    }
    choice
}

/// The answer kept beside the caches when the global config took none.
fn recorded(dir: &Path) -> Option<Background> {
    Background::parse(std::fs::read_to_string(file(dir, "answer")).ok()?.trim())
}

/// Records `choice` as `cache.background` in the config file `file`.
pub fn record(file: &Path, choice: Background) -> Result<(), String> {
    crate::project::config::record_scalar(file, &["cache"], "background", choice.as_str())
}

fn file(dir: &Path, ext: &str) -> PathBuf {
    dir.join("background").with_extension(ext)
}

/// An exclusive lock on `path`, held while the file lives. `Ok(None)` when
/// another process holds it; `Err` when it cannot be opened at all.
fn try_lock(path: &Path) -> std::io::Result<Option<std::fs::File>> {
    let file = std::fs::File::options().read(true).write(true).create(true).truncate(false).open(path)?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(std::fs::TryLockError::WouldBlock) => Ok(None),
        Err(std::fs::TryLockError::Error(e)) => Err(e),
    }
}

/// Whether a builder holds `dir`'s `background.run`.
pub fn running(dir: &Path) -> bool {
    file(dir, "run").exists() && matches!(try_lock(&file(dir, "run")), Ok(None))
}

/// The recorded state, when it is for this satex build.
fn state(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(file(dir, "state")).ok()?;
    let (state, key) = text.trim().split_once(' ')?;
    (key == crate::caches::format::cache_key()).then(|| state.to_string())
}

fn set_state(dir: &Path, state: &str) {
    let _ = std::fs::write(file(dir, "state"), format!("{state} {}\n", crate::caches::format::cache_key()));
}

/// Starts the builder for `dir` unless it runs, was stopped, or has a state
/// for this satex build (done, full, failed). Decided under
/// `background.start`, and waits until the builder holds its lock, so two
/// runs never both start one. `Ok(Some(pid))` when it was started.
pub fn start(cfg: &Config, dir: &Path, exe: &Path) -> Result<Option<u32>, String> {
    let dir = &std::path::absolute(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    if file(dir, "stop").exists() || state(dir).is_some_and(|s| s != "stopped") || running(dir) {
        return Ok(None);
    }
    if !exe.is_file() {
        return Err(format!("{}: no such program", exe.display()));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let Some(_deciding) = try_lock(&file(dir, "start")).map_err(|e| e.to_string())? else { return Ok(None) };
    if running(dir) {
        return Ok(None);
    }
    let mut log =
        std::fs::File::options().create(true).append(true).open(file(dir, "log")).map_err(|e| e.to_string())?;
    let nice = nice_of(&NICE);
    let _ = writeln!(
        log,
        "priority: {}",
        if nice.is_some() || cfg!(windows) { "lowest" } else { "normal (no nice in /usr/bin or /bin)" }
    );
    let command = |breakaway: bool| {
        let mut command = detached(exe, nice.as_deref(), breakaway);
        command
            .args(["cache", "build", "--background"])
            .args(cfg.cache.index.engines.iter().flat_map(|e| ["--engine", e.as_str()]))
            // The global config and this cache directory, never a project's.
            .current_dir(home(dir))
            .env("SATEX_CACHE", dir)
            .env(NO_SETUP, "1")
            .env(STARTED_BY_RUN, "1")
            .stdin(std::process::Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log.try_clone()?);
        command.spawn()
    };
    // A job that forbids breaking away refuses the first try (Windows).
    let mut child = command(true).or_else(|_| command(false)).map_err(|e| format!("{}: {e}", exe.display()))?;
    let pid = child.id();
    let deadline = std::time::Instant::now() + START_WAIT;
    while !running(dir) && std::time::Instant::now() < deadline {
        if let Ok(Some(status)) = child.try_wait() {
            if !status.success() {
                set_state(dir, "failed");
            }
            return Ok(Some(pid));
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    // Reaped when it ends, so a long-lived parent (the language server)
    // keeps no zombie.
    std::thread::spawn(move || child.wait());
    Ok(Some(pid))
}

/// Where the builder runs: the file system's root above `dir`, which holds
/// no project's `satex.yaml`, so the global config is the one it reads.
fn home(dir: &Path) -> &Path {
    dir.ancestors().last().unwrap_or(dir)
}

/// The first of `candidates` that is an absolute path to a file.
fn nice_of(candidates: &[&str]) -> Option<PathBuf> {
    candidates.iter().map(PathBuf::from).find(|p| p.is_absolute() && p.is_file())
}

/// Windows' creation flags for the builder: the idle priority class, no
/// console (`DETACHED_PROCESS`), out of the terminal's Ctrl+C
/// (`CREATE_NEW_PROCESS_GROUP`), and, where the job allows it, out of the
/// starting process's job, so closing the editor does not end it.
#[cfg_attr(not(windows), allow(dead_code))]
fn creation_flags(breakaway: bool) -> u32 {
    const IDLE_PRIORITY_CLASS: u32 = 0x40;
    const DETACHED_PROCESS: u32 = 0x8;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    let flags = IDLE_PRIORITY_CLASS | DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP;
    if breakaway { flags | CREATE_BREAKAWAY_FROM_JOB } else { flags }
}

/// `exe`, detached from the terminal's signals, at the lowest priority:
/// under `nice -n 19` where there is one, Windows' idle class.
// Unix reads `nice`, Windows `breakaway`.
#[allow(unused_variables)]
fn detached(exe: &Path, nice: Option<&Path>, breakaway: bool) -> std::process::Command {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let mut command = match nice {
            Some(nice) => {
                let mut c = std::process::Command::new(nice);
                c.args(["-n", "19"]).arg(exe);
                c
            }
            None => std::process::Command::new(exe),
        };
        command.process_group(0);
        command
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let mut command = std::process::Command::new(exe);
        command.creation_flags(creation_flags(breakaway));
        command
    }
    #[cfg(not(any(unix, windows)))]
    std::process::Command::new(exe)
}

pub(crate) use crate::caches::format::full;

/// Whether the builder should end before its next item: stopped or full.
pub(crate) fn should_stop(cfg: &Config, dir: &Path) -> bool {
    file(dir, "stop").exists() || full(cfg, dir)
}

/// `satex cache build --background`: the builder itself. Ends at once when
/// another one runs. Started by a run it honors `satex cache stop`, even one
/// given while it started; run `by_hand` it clears the stop and resumes.
pub fn run(
    cfg: &Config,
    engines: &[crate::project::config::Engine],
    by_hand: bool,
    out: &mut impl Write,
) -> Result<(), String> {
    let dir = cfg.cache.dir.clone().or_else(crate::caches::format::default_cache_dir).ok_or("no cache directory")?;
    let dir = std::path::absolute(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let Some(_lock) = try_lock(&file(&dir, "run")).map_err(|e| e.to_string())? else {
        let _ = writeln!(out, "a background build runs already");
        return Ok(());
    };
    if by_hand {
        let _ = std::fs::remove_file(file(&dir, "stop"));
    }
    if file(&dir, "stop").exists() {
        set_state(&dir, "stopped");
        let _ = writeln!(out, "stopped: `satex cache stop` was given");
        return Ok(());
    }
    set_state(&dir, "started");
    let mut run = cfg.clone();
    run.cache.dir = Some(dir.clone());
    run.cache.index.background = true;
    run.limits.threads = 1;
    let mut progress = |line: &str| {
        let _ = writeln!(out, "{line}");
    };
    let prepared = match super::build::prepare(&run, false, &[], engines, None, &mut progress) {
        Ok(prepared) => prepared,
        Err(why) => {
            set_state(&dir, "failed");
            return Err(why);
        }
    };
    let built = prepared.iter().filter(|p| matches!(p.outcome, "built" | "rebuilt")).count();
    // Done only when every item ran.
    let end = match prepared.iter().any(|p| p.outcome == "skipped") {
        false => "done",
        true if file(&dir, "stop").exists() => "stopped",
        true => "full",
    };
    set_state(&dir, end);
    let _ = writeln!(out, "{end}: {built} built of {} item(s)", prepared.len());
    Ok(())
}

/// `satex cache stop`: the builder ends after the item it is on and is not
/// started again until `satex cache build --background`. Returns whether
/// one was running.
pub fn stop(dir: &Path) -> Result<bool, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::write(file(dir, "stop"), "").map_err(|e| e.to_string())?;
    Ok(running(dir))
}

/// The builder's state for `satex cache`: `running`, `stopped`, `done`,
/// `full`, `failed`, `not started`, said at normal priority where it ran
/// without `nice`, and its log's last line.
pub fn status(dir: &Path) -> (String, Option<String>) {
    let mut state = match (running(dir), file(dir, "stop").exists(), state(dir).as_deref()) {
        (true, ..) => "running",
        (_, true, _) => "stopped",
        (_, _, Some("started" | "failed")) => "failed",
        (_, _, Some("done")) => "done",
        (_, _, Some("full")) => "full: limits.cache_size reached",
        (_, _, Some("stopped")) => "stopped",
        _ => "not started",
    }
    .to_string();
    let log = std::fs::read_to_string(file(dir, "log")).unwrap_or_default();
    if log.lines().rev().find(|l| l.starts_with("priority: ")).is_some_and(|l| l.contains("normal")) {
        state.push_str(", at normal priority");
    }
    (state, log.lines().rev().find(|l| !l.trim().is_empty()).map(str::to_string))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory of the crate's own, never `/tmp`.
    fn scratch(name: &str) -> PathBuf {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/scratch/background-unit")
            .join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    fn cfg(dir: &Path) -> Config {
        let mut cfg = Config::default();
        cfg.cache.dir = Some(dir.join("cache"));
        cfg
    }

    /// An `exe` that cannot run: `offer` must never start a builder here.
    fn no_exe(dir: &Path) -> PathBuf {
        dir.join("missing-satex")
    }

    /// The question is asked once, on a terminal only, and its answer lands
    /// in the global config so the next start does not ask again. Enter
    /// alone means yes, as `[Y/n]` says. A start that cannot spawn says so
    /// and does not fail the run.
    #[test]
    fn the_answer_is_recorded_and_enter_means_yes() {
        let dir = scratch("answer");
        let global = dir.join("config/satex.yaml");
        std::fs::create_dir_all(global.parent().unwrap()).unwrap();
        std::fs::write(&global, "# mine\nlint_off: [x]\n").unwrap();
        let mut out = Vec::new();
        let got = offer(&cfg(&dir), true, Some(&global), &no_exe(&dir), &mut "\n".as_bytes(), &mut out);
        assert_eq!(got, Background::Yes);
        let text = std::fs::read_to_string(&global).unwrap();
        assert!(text.starts_with("# mine\n") && text.contains("cache:\n  background: yes\n"), "{text}");
        let said = String::from_utf8(out).unwrap();
        assert!(said.contains("[Y/n]") && said.contains("did not start"), "{said}");
        // Answered again (the config changed by hand to ask): no replaces yes.
        let got = offer(&cfg(&dir), true, Some(&global), &no_exe(&dir), &mut "n\n".as_bytes(), &mut Vec::new());
        assert_eq!(got, Background::No);
        let text = std::fs::read_to_string(&global).unwrap();
        assert!(text.contains("  background: no\n") && !text.contains("yes"), "{text}");
        let parsed: Config = serde_yaml_ng::from_str(&text).expect("the file still fits the config");
        assert_eq!(parsed.cache.background, Background::No);
    }

    /// A typo is no answer: asked once more, and two typos leave the
    /// question for next time instead of recording a permanent no.
    #[test]
    fn an_unknown_answer_is_asked_again_and_never_recorded() {
        let dir = scratch("typo");
        let global = dir.join("satex.yaml");
        let mut out = Vec::new();
        let got = offer(&cfg(&dir), true, Some(&global), &no_exe(&dir), &mut "yse\nno\n".as_bytes(), &mut out);
        assert_eq!(got, Background::No, "the second line answers");
        assert_eq!(String::from_utf8(out).unwrap().matches("[Y/n]").count(), 2);
        std::fs::remove_file(&global).unwrap();
        let got = offer(&cfg(&dir), true, Some(&global), &no_exe(&dir), &mut "x\nmaybe\n".as_bytes(), &mut Vec::new());
        assert_eq!(got, Background::Ask);
        assert!(!global.exists(), "nothing recorded");
    }

    /// A global config the answer cannot go into (an inline `cache:` with
    /// nested values): the answer is kept beside the caches, the line to add
    /// is printed, and the next start does not ask again.
    #[test]
    fn an_answer_the_config_cannot_take_is_kept_and_not_asked_again() {
        let dir = scratch("unkept");
        let global = dir.join("satex.yaml");
        let text = "cache: {index: {class: book}}\n";
        std::fs::write(&global, text).unwrap();
        let mut out = Vec::new();
        assert_eq!(
            offer(&cfg(&dir), true, Some(&global), &no_exe(&dir), &mut "n\n".as_bytes(), &mut out),
            Background::No
        );
        assert!(String::from_utf8(out).unwrap().contains("Add `cache: {background: no}`"));
        assert_eq!(std::fs::read_to_string(&global).unwrap(), text, "the file is left as it was");
        let mut out = Vec::new();
        assert_eq!(
            offer(&cfg(&dir), true, Some(&global), &no_exe(&dir), &mut "y\n".as_bytes(), &mut out),
            Background::No
        );
        assert!(out.is_empty(), "not asked again");
    }

    /// Never asked without a terminal (pipes, editors, CI): the choice stays
    /// `ask` and nothing is written or started. A recorded `no` is never
    /// asked again, even on a terminal. A closed input is no answer.
    #[test]
    fn no_terminal_no_question_and_no_stays_no() {
        let dir = scratch("quiet");
        let global = dir.join("satex.yaml");
        let mut out = Vec::new();
        let got = offer(&cfg(&dir), false, Some(&global), &no_exe(&dir), &mut "y\n".as_bytes(), &mut out);
        assert_eq!(got, Background::Ask);
        assert!(out.is_empty() && !global.exists());
        let got = offer(&cfg(&dir), true, Some(&global), &no_exe(&dir), &mut "".as_bytes(), &mut Vec::new());
        assert_eq!(got, Background::Ask, "end of input answers nothing");
        assert!(!global.exists());
        let mut no = cfg(&dir);
        no.cache.background = Background::No;
        let mut out = Vec::new();
        assert_eq!(offer(&no, true, Some(&global), &no_exe(&dir), &mut "y\n".as_bytes(), &mut out), Background::No);
        assert!(out.is_empty() && !global.exists(), "no is respected");
    }

    /// `SATEX_NO_SETUP` and `CI` opt out, set to anything but empty: CI
    /// jobs and scripts on a terminal are never asked.
    #[test]
    fn the_environment_opts_out() {
        let env = |pairs: &'static [(&'static str, &'static str)]| {
            move |name: &str| pairs.iter().find(|(k, _)| *k == name).map(|(_, v)| std::ffi::OsString::from(v))
        };
        assert!(!opted_out(env(&[])));
        assert!(opted_out(env(&[(NO_SETUP, "1")])));
        assert!(opted_out(env(&[("CI", "true")])));
        assert!(!opted_out(env(&[(NO_SETUP, "")])), "empty is unset");
    }

    /// Caches off: there is nothing to prepare, so nothing is asked.
    #[test]
    fn caches_off_means_no_question() {
        let dir = scratch("off");
        let mut off = cfg(&dir);
        off.cache.enabled = false;
        let mut out = Vec::new();
        assert_eq!(offer(&off, true, None, &no_exe(&dir), &mut "y\n".as_bytes(), &mut out), Background::No);
        assert!(out.is_empty());
    }

    /// `nice` is never looked up in `PATH`: a relative entry would run a
    /// planted program of that name. A relative candidate that exists (this
    /// crate's `Cargo.toml`, relative to the test's directory) is refused.
    #[test]
    fn nice_is_only_taken_from_an_absolute_path() {
        assert_eq!(nice_of(&["Cargo.toml"]), None);
        assert_eq!(nice_of(&["/no/such/nice"]), None);
        let here = concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml");
        assert_eq!(nice_of(&["Cargo.toml", here]), Some(PathBuf::from(here)));
    }

    /// Windows: idle priority, no console, out of the terminal's Ctrl+C,
    /// and the breakaway flag only on the first try. `CREATE_NO_WINDOW` is
    /// not among them: it does nothing beside `DETACHED_PROCESS`.
    #[test]
    fn the_windows_flags_detach_and_break_away_once() {
        assert_eq!(creation_flags(false), 0x40 | 0x8 | 0x200);
        assert_eq!(creation_flags(true), creation_flags(false) | 0x0100_0000);
        assert_eq!(creation_flags(true) & 0x0800_0000, 0, "no CREATE_NO_WINDOW");
    }

    /// The lock says whether a builder runs; `stop` leaves the marker that
    /// keeps `start` from spawning, and a finished build for this satex
    /// leaves nothing to start either. Neither path spawns `exe` here. A
    /// directory that does not exist has no builder (the lock cannot even
    /// be opened).
    #[test]
    fn stop_and_done_keep_the_builder_from_starting() {
        let dir = scratch("stop").join("cache");
        assert!(!running(&dir), "no directory, no builder");
        std::fs::create_dir_all(&dir).unwrap();
        let cfg = Config::default();
        assert_eq!(status(&dir).0, "not started");
        let held = try_lock(&file(&dir, "run")).unwrap().expect("the lock");
        assert!(running(&dir) && status(&dir).0 == "running");
        assert!(stop(&dir).expect("stop"), "it was running");
        drop(held);
        assert_eq!(status(&dir).0, "stopped");
        assert_eq!(start(&cfg, &dir, &dir.join("missing")), Ok(None), "a stopped builder stays stopped");
        assert!(should_stop(&cfg, &dir));
        std::fs::remove_file(file(&dir, "stop")).unwrap();
        for done in ["done", "full", "failed", "started"] {
            set_state(&dir, done);
            assert_eq!(start(&cfg, &dir, &dir.join("missing")), Ok(None), "{done}: nothing to start for this build");
        }
        assert_eq!(status(&dir).0, "failed", "started, and no builder holds the lock: it crashed");
        // Another satex build: the state no longer counts, and the spawn of
        // a missing binary is an error, not a panic.
        std::fs::write(file(&dir, "state"), "done other\n").unwrap();
        assert!(start(&cfg, &dir, &dir.join("missing")).is_err());
    }

    /// The size bound ends the builder: a directory already at
    /// `limits.cache_size` gets nothing more. Files below the directory
    /// (the builder's `index/` scratch) count too.
    #[test]
    fn a_full_cache_directory_stops_the_builder() {
        let dir = scratch("full");
        let mut cfg = Config::default();
        cfg.limits.cache_size = bytesize::ByteSize::b(100);
        assert!(!should_stop(&cfg, &dir));
        std::fs::create_dir_all(dir.join("index")).unwrap();
        std::fs::write(dir.join("index/kernel.tex"), vec![0u8; 60]).unwrap();
        assert!(!should_stop(&cfg, &dir));
        std::fs::write(dir.join("package-x-v1-a-b.postcard"), vec![0u8; 40]).unwrap();
        assert!(should_stop(&cfg, &dir));
        cfg.limits.cache_size = bytesize::ByteSize::b(0);
        assert!(!should_stop(&cfg, &dir), "0 is no bound");
    }

    /// The builder's files are not caches: pruning, which sweeps stale
    /// `.lock` files and unusable `.postcard` files, leaves every one.
    #[test]
    fn pruning_leaves_the_builders_files() {
        let dir = scratch("prune");
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(3600);
        for ext in ["run", "start", "state", "stop", "log", "answer"] {
            let f = std::fs::File::create(file(&dir, ext)).unwrap();
            f.set_modified(old).unwrap();
        }
        crate::analysis::machine::prune_caches(&dir, 1);
        for ext in ["run", "start", "state", "stop", "log", "answer"] {
            assert!(file(&dir, ext).exists(), "{ext} was pruned");
        }
        assert_eq!(crate::caches::format::clear(&dir).unwrap(), 0, "and `cache clear` too");
    }

    /// A fake builder: a script that counts its starts in the cache
    /// directory it was given, then runs `then` (Linux, `flock(1)`).
    #[cfg(target_os = "linux")]
    fn fake(dir: &Path, then: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let exe = dir.join("fake-satex");
        let script = format!("#!/bin/sh\necho \"$SATEX_CACHE\" >> \"$SATEX_CACHE/starts\"\n{then}\n");
        std::fs::write(&exe, script).unwrap();
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755)).unwrap();
        exe
    }

    #[cfg(target_os = "linux")]
    fn starts(dir: &Path) -> Vec<String> {
        std::fs::read_to_string(dir.join("starts")).unwrap_or_default().lines().map(str::to_string).collect()
    }

    /// Two runs starting at once start one builder: the decision is taken
    /// under `background.start`, and a start returns only once its builder
    /// holds `background.run`. The builder is reaped when it ends, so the
    /// language server, which lives on, keeps no zombie.
    #[cfg(target_os = "linux")]
    #[test]
    fn two_starts_spawn_one_builder_and_it_is_reaped() {
        let dir = scratch("race");
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let exe = fake(&dir, "exec flock -n \"$SATEX_CACHE/background.run\" sleep 1");
        let cfg = cfg(&dir);
        let pids: Vec<Option<u32>> = std::thread::scope(|s| {
            let a = s.spawn(|| start(&cfg, &cache, &exe).unwrap());
            let b = s.spawn(|| start(&cfg, &cache, &exe).unwrap());
            vec![a.join().unwrap(), b.join().unwrap()]
        });
        assert_eq!(pids.iter().flatten().count(), 1, "{pids:?}");
        assert_eq!(starts(&cache).len(), 1);
        let pid = pids.into_iter().flatten().next().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let stat = format!("/proc/{pid}/stat");
        loop {
            match std::fs::read_to_string(&stat) {
                Err(_) => break,
                Ok(text) => {
                    let zombie = text.rsplit_once(") ").is_some_and(|(_, rest)| rest.starts_with('Z'));
                    assert!(!(zombie && std::time::Instant::now() > deadline), "a zombie: {text}");
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    /// A relative cache directory reaches the builder as an absolute one:
    /// it runs at the file system's root, where the relative path would name
    /// another directory.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_relative_cache_directory_is_made_absolute() {
        let dir = scratch("relative");
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let exe = fake(&dir, "exit 0");
        let here = std::env::current_dir().unwrap();
        let relative = cache.strip_prefix(&here).expect("the scratch is below the test's directory");
        assert!(relative.is_relative());
        start(&cfg(&dir), relative, &exe).unwrap().expect("started");
        assert_eq!(starts(&cache), vec![cache.display().to_string()]);
    }

    /// A builder that fails is not started again by every run: its state is
    /// `failed` for this satex build, and only a new build (or one by hand)
    /// tries again.
    #[cfg(target_os = "linux")]
    #[test]
    fn a_failing_builder_is_not_started_again() {
        let dir = scratch("failing");
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).unwrap();
        let exe = fake(&dir, "exit 3");
        start(&cfg(&dir), &cache, &exe).unwrap().expect("started");
        assert_eq!(status(&cache).0, "failed");
        assert_eq!(start(&cfg(&dir), &cache, &exe).unwrap(), None);
        assert_eq!(starts(&cache).len(), 1, "started once");
        std::fs::write(file(&cache, "state"), "failed other-build\n").unwrap();
        start(&cfg(&dir), &cache, &exe).unwrap().expect("a new build tries again");
        assert_eq!(starts(&cache).len(), 2);
    }
}
