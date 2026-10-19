//! File contents that stand in for what is on disk: `satex lint --fix`
//! applies its edits here, reruns the analysis on them, and only writes the
//! files once the fixes have settled.  The interpreter runs on one thread,
//! so the overlay is that thread's.

use crate::fixed::HashMap;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

thread_local! {
    static FILES: RefCell<HashMap<PathBuf, String>> = RefCell::new(HashMap::default());
    /// The files read or looked for while [`log`] is on.
    static LOG: RefCell<Option<Vec<Read>>> = const { RefCell::new(None) };
    /// Each file's [`fingerprint`] by its size and time.
    static PRINTS: RefCell<HashMap<PathBuf, (Stamp, u64)>> = RefCell::new(HashMap::default());
}

/// A file's size and time.
type Stamp = (u64, std::time::SystemTime);

/// A file a run read, or only looked for: its path, and what it found.
#[derive(Clone, Debug, PartialEq)]
pub struct Read {
    pub path: PathBuf,
    /// Whether it was only looked for.
    pub probe: bool,
    /// Its [`fingerprint`], or whether it was there.
    pub print: u64,
}

impl Read {
    /// Whether the file is still what the run found.
    pub fn holds(&self) -> bool {
        self.print == if self.probe { presence(&self.path) } else { fingerprint(&self.path) }
    }
}

/// Starts logging the files read on this thread, or stops it: what was logged comes back.
pub fn log(on: bool) -> Vec<Read> {
    LOG.with_borrow_mut(|log| std::mem::replace(log, on.then(Vec::new))).unwrap_or_default()
}

fn push(path: &Path, probe: bool, print: impl FnOnce() -> u64) {
    LOG.with_borrow_mut(|log| log.as_mut().map(|l| l.push(Read { path: path.to_path_buf(), probe, print: print() })));
}

/// Notes that a run looked for `path`, which is a file or not.
pub fn probed(path: &Path) {
    push(path, true, || presence(path));
}

/// Whether `path` is there to read: 1, else 0.
fn presence(path: &Path) -> u64 {
    u64::from(FILES.with_borrow(|files| files.contains_key(path)) || path.is_file())
}

/// What `path` holds now, as a hash: the overlay's text, else the file's
/// bytes, else that it is missing. A file's is read again only once its
/// size or time changed.
pub fn fingerprint(path: &Path) -> u64 {
    let stamp = |m: std::fs::Metadata| Some((m.len(), m.modified().ok()?));
    let on_disk = !FILES.with_borrow(|files| files.contains_key(path));
    let stamp = on_disk.then(|| std::fs::metadata(path).ok().and_then(stamp)).flatten();
    if let Some(stamp) = stamp
        && let Some((kept, print)) = PRINTS.with_borrow(|prints| prints.get(path).copied())
        && kept == stamp
    {
        return print;
    }
    let print = hash(&read_bytes_unlogged(path));
    if let Some(stamp) = stamp {
        PRINTS.with_borrow_mut(|prints| prints.insert(path.to_path_buf(), (stamp, print)));
    }
    print
}

fn hash(bytes: &std::io::Result<Vec<u8>>) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::hash::DefaultHasher::new();
    bytes.as_ref().ok().hash(&mut hasher);
    hasher.finish()
}

pub fn read_to_string(path: &Path) -> std::io::Result<String> {
    let bytes = read_bytes(path)?;
    Ok(String::from_utf8(bytes).unwrap_or_else(|e| e.into_bytes().iter().map(|&b| b as char).collect()))
}

/// Writes `text` back the way the file on disk is written: the bytes
/// [`read_to_string`] decoded as latin-1 go back as latin-1, and a file
/// whose lines end in CRLF keeps them, so a fix's bare `\n` does not leave
/// the file with mixed endings.  The original is read again rather than
/// remembered, since a fix may name a file this thread never read.
pub fn write(path: &Path, text: &str) -> std::io::Result<()> {
    let original = std::fs::read(path).unwrap_or_default();
    let text = if crlf(&original) { to_crlf(text) } else { text.to_string() };
    match std::str::from_utf8(&original) {
        Ok(_) => std::fs::write(path, text),
        // Latin-1 is what `read_to_string` decoded, and only it reverses byte for byte;
        // a fix that inserts a character outside it leaves no choice but UTF-8.
        Err(_) if text.chars().all(|c| c as u32 <= 0xFF) => {
            std::fs::write(path, text.chars().map(|c| c as u32 as u8).collect::<Vec<u8>>())
        }
        Err(_) => std::fs::write(path, text),
    }
}

/// Whether every line ending in `bytes` is a CRLF.
fn crlf(bytes: &[u8]) -> bool {
    let feeds = bytes.iter().filter(|&&b| b == b'\n').count();
    let returns = bytes.windows(2).filter(|w| w == b"\r\n").count();
    feeds > 0 && feeds == returns
}

fn to_crlf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\n', "\r\n")
}

/// A file's bytes, as [`crate::source::decode`] wants them: what the
/// overlay holds for it, or what is on disk.
pub fn read_bytes(path: &Path) -> std::io::Result<Vec<u8>> {
    let bytes = read_bytes_unlogged(path);
    push(path, false, || hash(&bytes));
    bytes
}

fn read_bytes_unlogged(path: &Path) -> std::io::Result<Vec<u8>> {
    match FILES.with(|files| files.borrow().get(path).cloned()) {
        // The overlay holds a file as it is written, so its bytes are its.
        Some(text) => Ok(text.into_bytes()),
        None => std::fs::read(path),
    }
}

pub fn set(path: &Path, text: String) {
    FILES.with(|files| files.borrow_mut().insert(path.to_path_buf(), text));
}

pub fn clear() {
    FILES.with(|files| files.borrow_mut().clear());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("satex-overlay-{name}-{}", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path
    }

    #[test]
    fn a_crlf_file_keeps_its_line_endings() {
        let path = scratch("crlf");
        std::fs::write(&path, b"a\r\nb\r\n").unwrap();
        let text = read_to_string(&path).unwrap();
        write(&path, &text.replace("b", "b\nc")).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"a\r\nb\r\nc\r\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn an_lf_file_keeps_its_line_endings() {
        let path = scratch("lf");
        std::fs::write(&path, b"a\nb\n").unwrap();
        write(&path, "a\nb\nc\n").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"a\nb\nc\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_latin_1_file_is_written_back_as_latin_1() {
        let path = scratch("latin1");
        std::fs::write(&path, b"caf\xe9 -- x\n").unwrap();
        let text = read_to_string(&path).unwrap();
        assert!(text.contains("café"));
        write(&path, &text.replace("--", "-x-")).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"caf\xe9 -x- x\n");
        let _ = std::fs::remove_file(&path);
    }
}
