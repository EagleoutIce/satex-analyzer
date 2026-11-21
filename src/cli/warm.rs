//! A warm engine for the `satex` build (`compile.warm`): after a build, pdfTeX starts with the
//! last pass's arguments in the project's directory and waits to open its input: a named pipe
//! under the build's keep ([`pipe`]), named like the main file, given in the main file's stead.
//! The next build's pass feeds it the text instead of starting an engine, which saves the start:
//! kpathsea's file database and the format's load.
//!
//! Sound because pdfTeX opens the name it was given (tex.web § 537) and reads nothing of it before
//! the open returns: the pipe gives the bytes the file holds. No file of the project is touched
//! and the engine runs where a fresh one does, so every other name means what it means there
//! (`../shared`, a link, the main file opened again by name). What tells the pipe's name from the
//! main file's is only where the engine writes it: the log, the recorder's list and the SyncTeX
//! data, where the build puts the main file's name back (and the log's memory statistics: the
//! longer name takes more of the string pool). The job name is the file's own (§ 537:
//! the name without its directory). The engine reads the clock as it starts
//! (§ 241): one started in another minute is not used, nor one on another format file or under
//! another environment as far as TeX reads it. Linux only:
//! the engine must be seen waiting in its open (`/proc/PID/wchan`), else it may be past it.
//!
//! An engine no build took is of no use once its minute is over, so it ends then: each one gets
//! a reaper (`satex --warm-reap KEEP`, [`reap`]) that outlives the build. Who uses or ends an
//! engine first takes its record by rename ([`claim`]): a build and the reaper never both act.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

/// The engine's directory under the build's keep (`.satex-build`): its pipe, record and output.
const DIR: &str = "warm";
/// What the kernel names the wait of a pipe's open for its other end (`fs/pipe.c`).
const WAITING: &str = "wait_for_partner";

/// The engine waiting for the next pass: its process, the minute it started, the format file's
/// size and time, and the command it runs (in `dir`).
struct Record {
    pid: u32,
    minute: u64,
    format: String,
    command: Vec<String>,
}

fn record_path(keep: &Path) -> PathBuf {
    keep.join(DIR).join("engine")
}

fn read(keep: &Path) -> Option<Record> {
    read_at(&record_path(keep))
}

fn read_at(record: &Path) -> Option<Record> {
    let text = std::fs::read_to_string(record).ok()?;
    let mut lines = text.lines();
    let pid = lines.next()?.parse().ok()?;
    let minute = lines.next()?.parse().ok()?;
    let format = lines.next()?.to_string();
    Some(Record { pid, minute, format, command: lines.map(str::to_string).collect() })
}

fn minute() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() / 60)
}

/// The format file a command loads (`-fmt=PATH`), by size and time: a dump in between is another format.
fn format_stamp(command: &[String]) -> String {
    let fmt = command.iter().find_map(|a| a.strip_prefix("-fmt="));
    let meta = fmt.and_then(|f| std::fs::metadata(format!("{f}.fmt")).ok());
    meta.map(|m| format!("{} {:?}", m.len(), m.modified().ok())).unwrap_or_default()
}

/// Whether process `pid` waits in a pipe's open, with `dir` as its directory.
fn waiting(pid: u32, dir: &Path) -> bool {
    let proc = PathBuf::from(format!("/proc/{pid}"));
    let wchan = std::fs::read_to_string(proc.join("wchan")).unwrap_or_default();
    let cwd = std::fs::read_link(proc.join("cwd")).ok();
    wchan.trim() == WAITING && cwd.is_some_and(|c| dir.canonicalize().is_ok_and(|d| d == c))
}

fn alive(pid: u32) -> bool {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    // the state follows the command's closing parenthesis; a zombie has ended
    stat.rsplit_once(')').and_then(|(_, rest)| rest.split_whitespace().next()).is_some_and(|s| s != "Z")
}

/// The record of `keep`'s engine, taken: whoever renames it first has the engine, for use or to end it.
fn claim(keep: &Path) -> Option<Record> {
    let mine = keep.join(DIR).join(format!("engine.{}", std::process::id()));
    std::fs::rename(record_path(keep), &mine).ok()?;
    let record = read_at(&mine);
    let _ = std::fs::remove_file(&mine);
    record
}

/// Stops the engine of `keep`, if any, and waits for its end.
fn stop(keep: &Path) {
    if let Some(r) = claim(keep) {
        end(&r);
    }
}

/// Stops the engine `r` names and waits for its end.
fn end(r: &Record) {
    // the recorded process only: its number may have gone to another since
    let ours = || alive(r.pid) && runs(r.pid, &r.command);
    if ours() {
        let _ = Command::new("kill").arg(r.pid.to_string()).status();
    }
    while ours() {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// `satex --warm-reap KEEP`: waits until the minute of `keep`'s engine is over, then stops that
/// engine unless a build took it. Started beside each engine ([`start`]), it outlives the build.
pub fn reap(keep: &Path) {
    let Some(r) = read(keep) else { return };
    let now = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    // a second past the minute: a build that found it usable has claimed it by then
    let over = (r.minute + 1) * 60 + 1;
    while now() < over && read(keep).is_some_and(|e| e.pid == r.pid) {
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    if read(keep).is_some_and(|e| e.pid == r.pid) {
        stop(keep);
    }
}

/// Whether process `pid` runs `command`.
fn runs(pid: u32, command: &[String]) -> bool {
    let line = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let words: Vec<&[u8]> = line.split(|&b| b == 0).filter(|w| !w.is_empty()).collect();
    words.len() == command.len() && words.iter().zip(command).all(|(w, c)| *w == c.as_bytes())
}

/// Waits for the engine of `keep` to load its format and reach its open; one not seen waiting
/// there in time is stopped. (An engine of an older satex waits on a pipe in the main file's own
/// place: stopped here, it never opens the file a build writes there.)
pub fn settle(keep: &Path, dir: &Path) {
    let Some(r) = read(keep) else { return };
    let deadline = std::time::Instant::now() + LOADING;
    while alive(r.pid) && !waiting(r.pid, dir) && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    if !waiting(r.pid, dir) {
        stop(keep);
    }
}

/// How long a build waits for the engine to finish loading: about a fresh engine's start.
const LOADING: std::time::Duration = std::time::Duration::from_millis(500);

/// `satex`'s first argument for [`reap`], then the keep.
pub const REAP: &str = "--warm-reap";

/// The main file's text while an older satex's pipe took its place.
const SOURCE: &str = "source";

/// A run reading `path` while the pipe of an older satex's warm engine takes its place would wait
/// on the pipe forever: the engine is stopped and the text it kept is put back first. Any other
/// pipe stays.
pub fn reclaim(path: &Path) {
    let pipe = std::fs::symlink_metadata(path).is_ok_and(|m| std::os::unix::fs::FileTypeExt::is_fifo(&m.file_type()));
    let keep = path.parent().unwrap_or(Path::new(".")).join(super::compile::KEEP);
    let Ok(text) = std::fs::read(keep.join(DIR).join(SOURCE)) else { return };
    if pipe {
        stop(&keep);
        let _ = put(path, &text);
    }
}

/// Writes `text` to `path` through a new file put in its place: a pipe there is replaced, never fed.
pub fn put(path: &Path, text: &[u8]) -> std::io::Result<()> {
    let next = path.with_file_name(format!(".{}.next", path.file_name().and_then(|n| n.to_str()).unwrap_or("file")));
    std::fs::write(&next, text)?;
    std::fs::rename(&next, path)
}

/// The pipe the engine of `keep` reads in `file`'s stead: named like it, so the job is too.
pub fn pipe(keep: &Path, file: &str) -> PathBuf {
    keep.join(DIR).join("in").join(file)
}

/// The pass `command` (on [`pipe`]) in `dir` by the waiting engine, when it runs that very command
/// on the current format and started this minute: fed `file`'s text, its status said by its log
/// (it is not this process's child). Any other engine is stopped, and the pass is left to a fresh one.
/// The variables TeX may read (`compile::read_by_tex`) of `vars`, with `more` as a pass adds them.
fn read_by_tex(vars: impl Iterator<Item = (OsString, OsString)>, more: &[(&str, &str)]) -> Vec<(OsString, OsString)> {
    let mut read: Vec<(OsString, OsString)> = vars
        .filter(|(k, _)| super::compile::read_by_tex(&k.to_string_lossy()))
        .filter(|(k, _)| !more.iter().any(|(m, _)| k == m))
        .chain(more.iter().map(|(k, v)| (k.into(), v.into())))
        .collect();
    read.sort_unstable();
    read
}

/// What process `pid` has of them: an engine started under another search path or pinned clock
/// is not the engine a pass would start now.
fn environment(pid: u32) -> Option<Vec<(OsString, OsString)>> {
    use std::os::unix::ffi::OsStringExt;
    let all = std::fs::read(format!("/proc/{pid}/environ")).ok()?;
    let pairs = all.split(|&b| b == 0).filter(|p| !p.is_empty()).map(|p| {
        let at = p.iter().position(|&b| b == b'=').unwrap_or(p.len());
        (OsString::from_vec(p[..at].to_vec()), OsString::from_vec(p[(at + 1).min(p.len())..].to_vec()))
    });
    Some(read_by_tex(pairs, &[]))
}

/// (`env`: what the passes add to this process's environment.)
pub fn take(
    keep: &Path,
    dir: &Path,
    command: &[String],
    file: &str,
    log: &Path,
    env: &[(&str, &str)],
) -> Option<ExitStatus> {
    // an engine still loading its format gets to its open sooner than a fresh one would
    settle(keep, dir);
    let r = claim(keep)?;
    let usable = r.command == command
        && runs(r.pid, command)
        && r.minute == minute()
        && r.format == format_stamp(command)
        && environment(r.pid).is_some_and(|theirs| theirs == read_by_tex(std::env::vars_os(), env))
        && waiting(r.pid, dir)
        && std::fs::symlink_metadata(dir.join(file)).is_ok_and(|m| m.is_file());
    let text = usable.then(|| std::fs::read(dir.join(file)).ok()).flatten();
    let Some(text) = text else {
        end(&r);
        return None;
    };
    let fed = feed(&pipe(keep, file), &text);
    if fed.is_err() {
        if runs(r.pid, command) {
            let _ = Command::new("kill").arg(r.pid.to_string()).status();
        }
        return None;
    }
    // An engine back in a pipe's open took the text and asks for it again: no build waits for
    // that. It is ended, and the pass is a fresh engine's.
    let mut asked: Option<std::time::Instant> = None;
    while alive(r.pid) && runs(r.pid, command) {
        std::thread::sleep(std::time::Duration::from_millis(1));
        match waiting(r.pid, dir) {
            true if asked.get_or_insert_with(std::time::Instant::now).elapsed() > LOADING => {
                end(&r);
                return None;
            }
            true => {}
            false => asked = None,
        }
    }
    // web2c exits 1 once TeX's history is past warnings (tex.web § 76): an error in the log
    let failed = std::fs::read_to_string(log).is_ok_and(|l| l.lines().any(|l| l.starts_with("! ")));
    Some(std::os::unix::process::ExitStatusExt::from_raw(i32::from(failed) << 8))
}

/// The text into the pipe the engine waits on. Opened without waiting: no engine at its other end
/// is an error, not a hang.
fn feed(pipe: &Path, text: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut w = std::fs::OpenOptions::new().write(true).custom_flags(libc::O_NONBLOCK).open(pipe)?;
    let mut rest = text;
    while !rest.is_empty() {
        match w.write(rest) {
            Ok(n) => rest = &rest[n..],
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(1))
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

/// Starts the engine for the next build: `command` in `dir`, its input the [`pipe`] of `file`.
/// With `env` as the passes have it. Gives the seconds it waits at most, None when none started.
pub fn start(keep: &Path, dir: &Path, command: &[String], file: &str, env: &[(&str, &str)]) -> Option<u64> {
    stop(keep);
    let (at, pipe) = (keep.join(DIR), pipe(keep, file));
    let _ = std::fs::create_dir_all(pipe.parent().unwrap_or(&at));
    let _ = std::fs::remove_file(&pipe);
    if !Command::new("mkfifo").arg(&pipe).status().is_ok_and(|s| s.success()) {
        return None;
    }
    let Ok(out) = std::fs::File::create(at.join("out")) else { return None };
    let (Some(program), Ok(err)) = (command.first(), out.try_clone()) else { return None };
    let mut engine = Command::new(program);
    engine.args(&command[1..]).current_dir(dir).envs(env.iter().copied());
    engine.stdin(std::process::Stdio::null()).stdout(out).stderr(err);
    // it outlives the build: nothing the build was handed stays open with it
    super::group::alone(&mut engine);
    let spawned = engine.spawn();
    let child = spawned.ok()?;
    let text = format!("{}\n{}\n{}\n{}\n", child.id(), minute(), format_stamp(command), command.join("\n"));
    let _ = std::fs::write(record_path(keep), text);
    // its reaper, in a group of its own: it outlives this build and who ends the build's group
    if let Ok(satex) = std::env::current_exe() {
        let mut reaper = Command::new(satex);
        reaper.arg(REAP).arg(keep).stdin(std::process::Stdio::null());
        reaper.stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        std::os::unix::process::CommandExt::process_group(&mut reaper, 0);
        super::group::alone(&mut reaper);
        let _ = reaper.spawn();
    }
    // the reaper ends it a second after its minute
    Some(61 - std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() % 60))
}
