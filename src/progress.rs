//! Progress bars for long work: building a kernel, caching a package,
//! analyzing a document. Each thread that works has its own line, redrawn
//! in place on stderr (~10 Hz). Messages print above the bars, which stay
//! visible. Silent unless stderr is a terminal, so tests, pipes and the LSP
//! see nothing.

use anstyle::{AnsiColor, Style};
use std::io::{IsTerminal, Write};
use std::sync::Mutex;
use std::thread::ThreadId;

/// The bars and how many lines of them are on the screen.
struct Bars {
    lines: Vec<(ThreadId, Vec<String>)>,
    drawn: usize,
    last_draw_ms: u64,
}

static BARS: Mutex<Bars> = Mutex::new(Bars { lines: Vec::new(), drawn: 0, last_draw_ms: 0 });
const THROTTLE_MS: u64 = 100;
/// Longest label shown, so a bar never wraps on an 80-column terminal.
const LABEL: usize = 48;

fn now_ms() -> u64 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START.get_or_init(std::time::Instant::now).elapsed().as_millis() as u64
}

/// Off when the output is meant for a program (`--format json` and the like).
static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

/// Shows no progress for the rest of the process.
pub fn disable() {
    ENABLED.store(false, std::sync::atomic::Ordering::Relaxed);
}

fn active() -> bool {
    ENABLED.load(std::sync::atomic::Ordering::Relaxed) && std::io::stderr().is_terminal()
}

impl Bars {
    /// Move to the first bar line and clear from there to the end of the screen.
    fn erase(&mut self, out: &mut impl Write) {
        if self.drawn > 1 {
            let _ = write!(out, "\x1b[{}A", self.drawn - 1);
        }
        if self.drawn > 0 {
            let _ = write!(out, "\r\x1b[J");
        }
        self.drawn = 0;
    }

    /// Draw every bar, leaving the cursor at the end of the last one. A
    /// line is cut to the terminal's width: one that wrapped would leave
    /// the erase above one row short.
    fn draw(&mut self, out: &mut impl Write) {
        self.erase(out);
        let width = terminal_size::terminal_size_of(std::io::stderr()).map_or(80, |(w, _)| w.0 as usize);
        let all: Vec<&String> = self.lines.iter().flat_map(|(_, lines)| lines).collect();
        for (i, line) in all.iter().enumerate() {
            let _ = write!(out, "{}{}", if i > 0 { "\n" } else { "" }, fit(line, width.saturating_sub(1)));
        }
        self.drawn = all.len();
        let _ = out.flush();
    }
}

/// `line` cut to `width` visible columns. Escape sequences (colors, OSC 8
/// links) take no room and are kept whole.
fn fit(line: &str, width: usize) -> String {
    use unicode_width::UnicodeWidthChar;
    let mut out = String::with_capacity(line.len());
    let (mut used, mut chars) = (0, line.chars().peekable());
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            out.push(c);
            // CSI ends at a letter; OSC at BEL or ESC `\`.
            match chars.next() {
                Some('[') => {
                    out.push('[');
                    for c in chars.by_ref() {
                        out.push(c);
                        if c.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
                Some(']') => {
                    out.push(']');
                    while let Some(c) = chars.next() {
                        out.push(c);
                        if c == '\x07' || (c == '\x1b' && chars.peek() == Some(&'\\')) {
                            if c == '\x1b' {
                                out.extend(chars.next());
                            }
                            break;
                        }
                    }
                }
                Some(c) => out.push(c),
                None => {}
            }
            continue;
        }
        let w = c.width().unwrap_or(0);
        if used + w > width {
            // Close any color and link the cut left open.
            out.push_str("\x1b[0m\x1b]8;;\x1b\\");
            break;
        }
        used += w;
        out.push(c);
    }
    out
}

fn lock() -> std::sync::MutexGuard<'static, Bars> {
    BARS.lock().unwrap_or_else(|e| e.into_inner())
}

/// `####----` of width 20 for `pct` percent.
fn cells(pct: usize) -> (String, String) {
    let filled = pct.min(100) / 5;
    ("#".repeat(filled), "-".repeat(20 - filled))
}

/// The file being read now, when it is not the job's own: its name, line
/// and how much of it is read.
pub struct Sub {
    pub file: String,
    /// The full path the name links to (OSC 8), so a terminal opens the file.
    pub path: String,
    pub line: u32,
    pub reached: usize,
    pub total: usize,
}

/// `label [####------] 40% 12s/600s` for this thread: `reached` of `total`
/// characters of a file read, and under it `↳ file:line [##---] 24%` for the
/// innermost file when it is another one. Approximate, since what a file
/// loads is not counted, but cheap.
pub fn bar(label: &str, reached: usize, total: usize, lines: (u64, u64), clock: Option<(u64, u64)>, subs: &[Sub]) {
    if total == 0 || !active() {
        return;
    }
    let pct = (reached * 100 / total).min(100);
    let clip = |text: &str, max: usize| -> String {
        if text.chars().count() > max {
            text.chars().take(max - 1).chain(std::iter::once('…')).collect()
        } else {
            text.to_string()
        }
    };
    let label = clip(label, LABEL);
    let count = |n: u64| if n >= 10_000 { format!("{:.1}k", n as f64 / 1000.0) } else { n.to_string() };
    let lines = match lines {
        (0, _) => String::new(),
        (all, 0) => format!(" {} lines", count(all)),
        (all, cached) => format!(" {} lines ({} cached)", count(all), count(cached)),
    };
    let clock = format!("{lines}{}", clock.map(|(spent, limit)| format!(" {spent}/{limit}s")).unwrap_or_default());
    let (dim, done, bold) =
        (Style::new().dimmed(), Style::new().fg_color(Some(AnsiColor::Green.into())), Style::new().bold());
    let paint = |text: String| {
        let mut line = Vec::new();
        let _ = write!(anstream::AutoStream::new(&mut line, anstream::stderr().current_choice()), "{text}");
        String::from_utf8_lossy(&line).into_owned()
    };
    let (full, empty) = cells(pct);
    let mut lines = vec![paint(format!(
        "{dim}satex:{dim:#} {bold}{label}{bold:#} [{done}{full}{done:#}{dim}{empty}{dim:#}] {bold}{pct:>3}%{bold:#}{dim}{clock}{dim:#}"
    ))];
    for sub in subs.iter().filter(|s| s.total > 0) {
        let pct = (sub.reached * 100 / sub.total).min(100);
        let (full, empty) = cells(pct);
        let name = clip(&format!("{}:{}", sub.file, sub.line), LABEL - 8);
        let name = if sub.path.is_empty() {
            name
        } else {
            let path = std::path::absolute(&sub.path).unwrap_or_else(|_| sub.path.clone().into());
            format!("\x1b]8;;file://{}\x1b\\{name}\x1b]8;;\x1b\\", path.display())
        };
        lines.push(paint(format!("  {dim}↳{dim:#} {name} [{done}{full}{done:#}{dim}{empty}{dim:#}] {pct:>3}%")));
    }
    let me = std::thread::current().id();
    let mut bars = lock();
    match bars.lines.iter_mut().find(|(id, _)| *id == me) {
        Some(slot) => slot.1 = lines,
        None => bars.lines.push((me, lines)),
    }
    let now = now_ms();
    if now.saturating_sub(bars.last_draw_ms) >= THROTTLE_MS {
        bars.last_draw_ms = now;
        bars.draw(&mut std::io::stderr().lock());
    }
}

/// What the work is now, for the editor: `text` on its status bar, the
/// rest in that item's hover.
#[derive(Clone, Default, PartialEq, serde::Serialize)]
pub struct Status {
    pub text: String,
    /// `format`, `package`, `preamble`, `document`, `lint` or `probe`.
    #[serde(skip_serializing_if = "str::is_empty")]
    pub phase: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    pub file: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub line: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub files: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub lines: u64,
    /// Lines the kernel and package caches stood for.
    #[serde(skip_serializing_if = "is_zero")]
    pub cached: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub ms: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub paths: u64,
    /// The command a probe calls, and who asked ([`because`]).
    #[serde(skip_serializing_if = "str::is_empty")]
    pub probe: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    pub why: String,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

/// Who hears what the bars say where there are none: the language server,
/// which tells the editor. With what it heard last, and when.
type Listener = Box<dyn Fn(&Status) + Send>;
static LISTENER: Mutex<Option<(Listener, Status, u64)>> = Mutex::new(None);

/// `listener` hears what the work is, at most twice a second, and an empty
/// status once it is idle; `None` stops.
pub fn listen(listener: Option<Listener>) {
    *LISTENER.lock().unwrap_or_else(|e| e.into_inner()) = listener.map(|l| (l, Status::default(), 0));
}

/// What the work is now, for the listener ([`listen`]).
pub fn tell(status: Status) {
    let mut listener = LISTENER.lock().unwrap_or_else(|e| e.into_inner());
    let Some((hear, last, at)) = listener.as_mut() else { return };
    let now = now_ms();
    if status != *last && (status.text.is_empty() || now.saturating_sub(*at) >= 500) {
        hear(&status);
        (*last, *at) = (status, now);
    }
}

/// The work is done: the listener hears an empty status.
pub fn idle() {
    tell(Status::default());
}

thread_local! {
    static WHY: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Runs `f` with `why` added to what this thread's work is for.
pub fn because<T>(why: impl Into<String>, f: impl FnOnce() -> T) -> T {
    struct Pop;
    impl Drop for Pop {
        fn drop(&mut self) {
            WHY.with_borrow_mut(Vec::pop);
        }
    }
    WHY.with_borrow_mut(|w| w.push(why.into()));
    let _pop = Pop;
    f()
}

/// What this thread's work is for, outermost first.
pub fn why() -> String {
    WHY.with_borrow(|w| w.join(": "))
}

/// Print a message above the bars, which are drawn again below it.
pub fn message(args: std::fmt::Arguments) {
    if !active() {
        eprintln!("{args}");
        return;
    }
    let mut bars = lock();
    let mut out = std::io::stderr().lock();
    bars.erase(&mut out);
    let _ = writeln!(out, "{args}");
    bars.draw(&mut out);
}

/// A long step starting, above the bars: `satex:` dimmed, `what` bold
/// yellow, `why` plain and `hint` dimmed, colored only where stderr takes it.
pub fn step(what: &str, why: &str, hint: &str) {
    let (dim, bold) = (Style::new().dimmed(), Style::new().bold().fg_color(Some(AnsiColor::Yellow.into())));
    let mut line = Vec::new();
    let _ = write!(
        anstream::AutoStream::new(&mut line, anstream::stderr().current_choice()),
        "{dim}satex:{dim:#} {bold}{what}{bold:#}: {why} {dim}({hint}){dim:#}"
    );
    message(format_args!("{}", String::from_utf8_lossy(&line)));
}

/// A block of one-time work done, kept above the bars on a terminal:
/// `satex: ✓ cached package tikz (14.2 s)`. Silent elsewhere, as the bars.
pub fn done(what: &str, took: std::time::Duration) {
    if !active() {
        return;
    }
    let (dim, good) = (Style::new().dimmed(), Style::new().fg_color(Some(AnsiColor::Green.into())));
    let mut line = Vec::new();
    let _ = write!(
        anstream::AutoStream::new(&mut line, anstream::stderr().current_choice()),
        "{dim}satex:{dim:#} {good}\u{2713}{good:#} {what} {dim}({:.1} s){dim:#}",
        took.as_secs_f64()
    );
    message(format_args!("{}", String::from_utf8_lossy(&line)));
}

/// `eprintln!` that keeps the progress bars below what it prints.
macro_rules! note {
    ($($arg:tt)*) => {
        $crate::progress::message(format_args!($($arg)*))
    };
}
pub(crate) use note;

/// This thread's work is done: its bar goes, the others stay.
pub fn clear() {
    if !active() {
        return;
    }
    let me = std::thread::current().id();
    let mut bars = lock();
    bars.lines.retain(|(id, _)| *id != me);
    bars.draw(&mut std::io::stderr().lock());
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_line_is_cut_to_the_width_without_counting_escapes() {
        let line = "\x1b[1mabc\x1b[0m \x1b]8;;file:///x\x1b\\defgh\x1b]8;;\x1b\\";
        assert_eq!(super::fit(line, 20), line);
        assert_eq!(super::fit(line, 6), "\x1b[1mabc\x1b[0m \x1b]8;;file:///x\x1b\\de\x1b[0m\x1b]8;;\x1b\\");
    }
}
