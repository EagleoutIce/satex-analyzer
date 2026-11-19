//! A warm engine for the `satex` build (`compile.warm`): after a build, pdfTeX starts with the
//! last pass's arguments and waits to open the main file, a named pipe in its place. The next
//! build's pass feeds it the text instead of starting an engine, which saves the start: kpathsea's
//! file database and the format's load.
//!
//! Sound because pdfTeX opens the name it was given (tex.web § 537) and reads nothing of it before
//! the open returns: the pipe gives the bytes the file holds. The file is back in the pipe's place
//! before the pipe is fed, for a later open by name. The engine reads the clock as it starts
//! (§ 241): one started in another minute is not used, nor one on another format file. Linux only:
//! the engine must be seen waiting in its open (`/proc/PID/wchan`), else it may be past it.
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
    let text = std::fs::read_to_string(record_path(keep)).ok()?;
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

/// Stops the engine of `keep`, if any, and waits for its end.
fn stop(keep: &Path) {
    if let Some(r) = read(keep) {
        // the recorded process only: its number may have gone to another since
        let ours = || alive(r.pid) && runs(r.pid, &r.command);
        if ours() {
            let _ = Command::new("kill").arg(r.pid.to_string()).status();
        }
        while ours() {
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    let _ = std::fs::remove_file(record_path(keep));
}

/// Whether process `pid` runs `command`.
fn runs(pid: u32, command: &[String]) -> bool {
    let line = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let words: Vec<&[u8]> = line.split(|&b| b == 0).filter(|w| !w.is_empty()).collect();
    words.len() == command.len() && words.iter().zip(command).all(|(w, c)| *w == c.as_bytes())
}

/// Readies `dir` for files written into it: an engine not seen waiting in its open is stopped,
/// since it could open the main file the next build writes and run ahead of that build.
pub fn settle(keep: &Path, dir: &Path) {
    let Some(r) = read(keep) else { return };
    // The main file is the pipe until this build writes it: an engine not yet in its open is still
    // loading its format, and gets there sooner than a fresh one would.
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

/// The main file's text while the pipe takes its place.
const SOURCE: &str = "source";

/// A run reading `path` while a warm engine's pipe takes its place (a build directory of
/// `satex gui`, run by hand) would wait on the pipe forever: the engine is stopped and the text
/// it kept is put back first. Any other pipe stays.
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

/// The pass `program args file` in `dir` by the waiting engine, when it runs that very command
/// on the current format and started this minute: fed `file`'s text, its status said by its log
/// (it is not this process's child). Any other engine is stopped, and the pass is left to a fresh one.
pub fn take(keep: &Path, dir: &Path, command: &[String], file: &str, log: &Path) -> Option<ExitStatus> {
    let r = read(keep)?;
    let usable = r.command == command
        && runs(r.pid, command)
        && r.minute == minute()
        && r.format == format_stamp(command)
        && waiting(r.pid, dir)
        && std::fs::symlink_metadata(dir.join(file)).is_ok_and(|m| m.is_file());
    if !usable {
        stop(keep);
        return None;
    }
    let text = std::fs::read(dir.join(file)).ok()?;
    let fed = feed(&keep.join(DIR).join("pipe"), &text);
    let _ = std::fs::remove_file(record_path(keep));
    if fed.is_err() {
        if runs(r.pid, command) {
            let _ = Command::new("kill").arg(r.pid.to_string()).status();
        }
        return None;
    }
    while alive(r.pid) && runs(r.pid, command) {
        std::thread::sleep(std::time::Duration::from_millis(1));
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

/// Starts the engine for the next build: `command` in `dir`, `file` a pipe in place of the main
/// file until that build puts the text back. With `env` as the passes have it.
pub fn start(keep: &Path, dir: &Path, command: &[String], file: &str, env: &[(&str, &str)]) {
    stop(keep);
    let at = keep.join(DIR);
    let pipe = at.join("pipe");
    let _ = std::fs::create_dir_all(&at);
    let _ = std::fs::remove_file(&pipe);
    let made = Command::new("mkfifo").arg(&pipe).status().is_ok_and(|s| s.success());
    let link = at.join("link");
    let _ = std::fs::remove_file(&link);
    // the text, kept for a run that finds the pipe in its place (`reclaim`)
    if !made
        || std::fs::copy(dir.join(file), at.join(SOURCE)).is_err()
        || std::fs::hard_link(&pipe, &link).is_err()
        || std::fs::rename(&link, dir.join(file)).is_err()
    {
        return;
    }
    let Ok(out) = std::fs::File::create(at.join("out")) else { return };
    let (Some(program), Ok(err)) = (command.first(), out.try_clone()) else { return };
    let spawned = Command::new(program)
        .args(&command[1..])
        .current_dir(dir)
        .envs(env.iter().copied())
        .stdin(std::process::Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn();
    if let Ok(child) = spawned {
        let text = format!("{}\n{}\n{}\n{}\n", child.id(), minute(), format_stamp(command), command.join("\n"));
        let _ = std::fs::write(record_path(keep), text);
    }
}
