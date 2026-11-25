//! Progress bars for long work: building a kernel, caching a package,
//! analyzing a document. Each thread that works has its own line, redrawn
//! in place on stderr (~10 Hz), under an optional headline. Messages print
//! above them, which stay visible. One writer draws all of it, and only rows
//! that changed. Silent unless stderr is a terminal, so tests, pipes and the
//! LSP see nothing.

use anstyle::{AnsiColor, Style};
use std::io::{IsTerminal, Write};
use std::sync::Mutex;
use std::thread::ThreadId;

/// The bars, the headline above them and the rows on the screen.
struct Bars {
    lines: Vec<(ThreadId, Vec<String>)>,
    /// A line for the whole run above the bars ([`headline`]).
    head: Option<String>,
    /// The rows as last drawn, cut to the width.
    shown: Vec<String>,
    last_draw_ms: u64,
}

static BARS: Mutex<Bars> = Mutex::new(Bars { lines: Vec::new(), head: None, shown: Vec::new(), last_draw_ms: 0 });
const THROTTLE_MS: u64 = 100;
/// Columns and rows of a terminal that does not say its size.
const SIZE: (usize, usize) = (80, 24);
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
    ENABLED.load(std::sync::atomic::Ordering::Relaxed) && terminal(std::io::stderr())
}

/// Whether `stream` is a terminal. Emscripten says every standard stream
/// is, also where a page prints the lines one by one.
pub fn terminal(stream: impl IsTerminal) -> bool {
    stream.is_terminal() && !cfg!(target_os = "emscripten")
}

impl Bars {
    /// The rows to show: the headline, then every bar, each cut to `width`, and no more than
    /// `height` of them. A frame taller than the screen pushes its first rows into the
    /// scrollback, where the next frame cannot overwrite them: each would stay there for good.
    fn rows(&self, (width, height): (usize, usize)) -> Vec<String> {
        let bars = self.lines.iter().flat_map(|(_, lines)| lines);
        self.head.iter().chain(bars).take(height.max(1)).map(|line| fit(line, width.saturating_sub(1))).collect()
    }

    /// Prints `above` (lines that stay) and then the rows over the old ones,
    /// leaving the cursor at the end of the last row. Nothing is written when
    /// there is nothing above and the rows are those shown, so an unchanged
    /// frame never repaints. Each row overwrites its old one in place and
    /// clears only its rest, inside a synchronized update with the cursor
    /// hidden: the terminal never shows a half-drawn or empty frame. A row is
    /// cut to the terminal's width: one that wrapped would leave the next
    /// frame one row short.
    fn draw(&mut self, out: &mut impl Write, above: &str) {
        let size = terminal_size::terminal_size_of(std::io::stderr()).map_or(SIZE, |(w, h)| (w.0.into(), h.0.into()));
        let rows = self.rows(size);
        if above.is_empty() && rows == self.shown {
            return;
        }
        let _ = out.write_all(frame(self.shown.len(), above, &rows).as_bytes());
        let _ = out.flush();
        self.shown = rows;
    }
}

/// The bytes that turn `shown` rows into `rows`, with `above` printed first.
fn frame(shown: usize, above: &str, rows: &[String]) -> String {
    let mut frame = String::from("\x1b[?2026h\x1b[?25l");
    if shown > 1 {
        frame.push_str(&format!("\x1b[{}A", shown - 1));
    }
    if shown > 0 {
        frame.push('\r');
    }
    for line in above.lines() {
        frame.push_str(line);
        frame.push_str("\x1b[K\n");
    }
    // Wrapping off while the rows are drawn (DECAWM): where the width was
    // misreported, a long row is clipped at the margin instead of wrapping
    // onto a row the next frame does not reach.
    frame.push_str("\x1b[?7l");
    for (i, row) in rows.iter().enumerate() {
        if i > 0 {
            frame.push('\n');
        }
        frame.push_str(row);
        frame.push_str("\x1b[K");
    }
    // Rows the last frame had and this one has not.
    frame.push_str("\x1b[J\x1b[?7h\x1b[?25h\x1b[?2026l");
    frame
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
/// Which run of the build the bars are of (latexmk's rounds), and at most how many.
static RUN: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
static RUNS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

/// Package caches this process loaded instead of reading the package, and built.
static CACHES: [std::sync::atomic::AtomicU32; 2] = [const { std::sync::atomic::AtomicU32::new(0) }; 2];

/// One more package cache was loaded: the bar counts them.
pub fn cache_loaded() {
    CACHES[0].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// One more package cache was built: the bar counts them too.
pub fn cache_built() {
    CACHES[1].fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

pub fn set_run(run: u32, runs: u32) {
    RUN.store(run, std::sync::atomic::Ordering::Relaxed);
    RUNS.store(runs, std::sync::atomic::Ordering::Relaxed);
}

/// `steps`: the tokens read so far, and on the line being read.
pub fn bar(
    label: &str,
    reached: usize,
    total: usize,
    lines: (u64, u64),
    steps: (u64, u64),
    clock: Option<(u64, u64)>,
    subs: &[Sub],
) {
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
    let run = RUN.load(std::sync::atomic::Ordering::Relaxed);
    let label = clip(&if run > 1 { format!("run {run}: {label}") } else { label.to_string() }, LABEL);
    let count = |n: u64| if n < 10_000 { n.to_string() } else { compact(n) };
    let lines = match lines {
        (0, _) => String::new(),
        (all, 0) => format!(" {} lines", count(all)),
        (all, cached) => format!(" {} lines ({} cached)", count(all), count(cached)),
    };
    let caches = match CACHES.each_ref().map(|n| n.load(std::sync::atomic::Ordering::Relaxed)) {
        [0, 0] => String::new(),
        [n, 0] => format!(" {n} packages from caches"),
        [0, b] => format!(" {b} packages cached"),
        [n, b] => format!(" {} packages: {n} from caches, {b} cached", n + b),
    };
    let tokens = if steps.0 > 0 { format!("{caches} {} steps", count(steps.0)) } else { caches };
    let here = if steps.1 >= 100_000 { format!(" {} on this line", count(steps.1)) } else { String::new() };
    let clock =
        format!("{lines}{tokens}{}", clock.map(|(spent, limit)| format!(" {spent}/{limit}s")).unwrap_or_default());
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
        lines.push(paint(format!(
            "  {dim}↳{dim:#} {name} [{done}{full}{done:#}{dim}{empty}{dim:#}] {pct:>3}%{dim}{here}{dim:#}"
        )));
    }
    if subs.iter().all(|s| s.total == 0) && !here.is_empty() {
        lines[0].push_str(&paint(format!("{dim}{here}{dim:#}")));
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
        bars.draw(&mut std::io::stderr().lock(), "");
    }
}

/// What a cache build met so far on this thread: each file by path as (its size, the share read),
/// a closed one read whole, and the share last shown.
type Met = (std::collections::HashMap<String, (u64, f64)>, (usize, usize));
thread_local! { static MET: std::cell::RefCell<Met> = Default::default(); }

/// How much of a cache build is read, from the files `open` under it now as (path, read, all):
/// latex.ltx inputs expl3-code.tex a few percent in, which reads UnicodeData.txt line by line
/// with `\read` (expl3-code.tex:35859), so the first file's own share stands still for most of
/// the build. Each file met counts by its size on disk, a closed one whole. The share only grows,
/// though a file met later adds to all; a first file read less than before is a new build.
pub fn share(open: &[(String, usize, usize)]) -> (usize, usize) {
    let part = |read: usize, all: usize| if all == 0 { 1.0 } else { read as f64 / all as f64 };
    MET.with_borrow_mut(|(met, last)| {
        let Some((root, read, all)) = open.first() else { return (1, 1) };
        if met.get(root).is_none_or(|&(_, was)| part(*read, *all) < was) {
            (*met, *last) = Default::default();
        }
        for (_, share) in met.values_mut() {
            *share = 1.0;
        }
        for (path, read, all) in open {
            let size = met.get(path).map(|e| e.0).or_else(|| std::fs::metadata(path).ok().map(|m| m.len()));
            met.insert(path.clone(), (size.unwrap_or(*all as u64), part(*read, *all)));
        }
        let (done, total) =
            met.values().fold((0.0, 0u64), |(d, t), &(size, share)| (d + size as f64 * share, t + size));
        let now = ((done / total.max(1) as f64 * 1000.0) as usize, 1000);
        *last = if now.0 >= last.0 { now } else { *last };
        *last
    })
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
    /// Characters of the job's file read, of all of them.
    #[serde(skip_serializing_if = "is_zero")]
    pub reached: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub total: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub ms: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub paths: u64,
    /// The command a probe calls, and who asked ([`because`]).
    #[serde(skip_serializing_if = "str::is_empty")]
    pub probe: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    pub why: String,
    /// The build's run, of at most `runs` ([`set_run`]): [`tell`] fills them.
    #[serde(skip_serializing_if = "is_zero")]
    pub run: u64,
    #[serde(skip_serializing_if = "is_zero")]
    pub runs: u64,
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
pub fn tell(mut status: Status) {
    if !status.text.is_empty() {
        status.run = RUN.load(std::sync::atomic::Ordering::Relaxed).into();
        status.runs = RUNS.load(std::sync::atomic::Ordering::Relaxed).into();
    }
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

/// What runs between two commands of a document's run on this thread.
pub type Pause = Box<dyn FnMut()>;

thread_local! {
    static PAUSE: std::cell::RefCell<Option<Pause>> = const { std::cell::RefCell::new(None) };
    static PAUSED: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Runs `f` with `pause` called between the commands of the runs it makes:
/// the language server answers from its last analysis there.
pub fn pausing<T>(pause: Pause, f: impl FnOnce() -> T) -> T {
    let _restore = Restore(PAUSE.replace(Some(pause)));
    f()
}

/// `cell`'s value, made by `f` if there is none. `f` may pause, and a request answered
/// there may make the value too: the first one made stays.
pub fn built<T>(cell: &std::sync::OnceLock<T>, f: impl FnOnce() -> T) -> &T {
    if let Some(value) = cell.get() {
        return value;
    }
    let value = f();
    cell.get_or_init(|| value)
}

struct Restore(Option<Pause>);

impl Drop for Restore {
    fn drop(&mut self) {
        PAUSE.set(self.0.take());
    }
}

/// A run stands between two of its commands: every so often, the pause runs.
pub fn pause() {
    let n = PAUSED.get().wrapping_add(1);
    PAUSED.set(n);
    if n.is_multiple_of(64) {
        pause_now();
    }
}

/// Between two long steps of work (a lint rule each): the pause runs now.
pub fn pause_now() {
    if let Some(mut pause) = PAUSE.take() {
        pause();
        PAUSE.set(Some(pause));
        PAUSED_AT.set(Some(std::time::Instant::now()));
    }
}

/// How long work goes on between two pauses at most, in ms: what a request waits during a run
/// (`lsp.pause_ms`, set by the server; until then at every look).
static PAUSE_EVERY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Sets [`PAUSE_EVERY`].
pub fn pause_every(ms: u64) {
    PAUSE_EVERY.store(ms, std::sync::atomic::Ordering::Relaxed);
}

thread_local! {
    static PAUSED_AT: std::cell::Cell<Option<std::time::Instant>> = const { std::cell::Cell::new(None) };
}

/// Inside a long command, asked every so many steps: the pause runs once [`PAUSE_EVERY`]
/// passed since the last one. A step count alone waits as long as the steps are slow.
pub fn pause_if_due() {
    if PAUSE.with_borrow(Option::is_none) {
        return;
    }
    let every = std::time::Duration::from_millis(PAUSE_EVERY.load(std::sync::atomic::Ordering::Relaxed));
    if PAUSED_AT.get().is_none_or(|t| t.elapsed() >= every) {
        pause_now();
    }
}

/// The seconds each part of some work took, written as one note (`satex: <what>: a 0.01s, …`)
/// when dropped (`-vv`).
pub struct Laps(Option<(std::time::Instant, String)>);

impl Laps {
    pub fn new(on: bool, what: &str) -> Self {
        Laps(on.then(|| (std::time::Instant::now(), format!("satex: {what}:"))))
    }

    /// The time since the last lap went to `what`.
    pub fn lap(&mut self, what: &str) {
        if let Some((clock, said)) = &mut self.0 {
            use std::fmt::Write;
            let _ = write!(said, " {what} {:.2}s,", clock.elapsed().as_secs_f64());
            *clock = std::time::Instant::now();
        }
    }
}

impl Drop for Laps {
    fn drop(&mut self) {
        if let Some((_, said)) = &self.0 {
            message(format_args!("{}", said.trim_end_matches(',')));
        }
    }
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
    bars.draw(&mut std::io::stderr().lock(), &format!("{args}\n"));
}

/// `line` above the bars until [`finish`]; `None` takes it away.
pub fn headline(line: Option<String>) {
    if !active() {
        return;
    }
    let mut bars = lock();
    bars.head = line;
    bars.draw(&mut std::io::stderr().lock(), "");
}

/// Findings made while the run goes on: `line` above the bars, and `listing`
/// printed above them each time `l` is pressed. `None` where no key can be
/// read, so the caller prints the listing itself. The keys are read until the
/// guard drops.
pub fn findings(line: &str, listing: &str) -> Option<super::keys::Keys> {
    if !active() {
        return None;
    }
    let paint = |text: &str| {
        let mut out = Vec::new();
        let _ = write!(anstream::AutoStream::new(&mut out, anstream::stderr().current_choice()), "{text}");
        String::from_utf8_lossy(&out).into_owned()
    };
    let listing = paint(listing.trim_end());
    let keys = super::keys::listen(move |key| {
        if key.eq_ignore_ascii_case(&b'l') {
            message(format_args!("{listing}"));
        }
    })?;
    headline(Some(paint(line)));
    Some(keys)
}

/// The run is over: the headline and every bar go.
pub fn finish() {
    if !active() {
        return;
    }
    let mut bars = lock();
    (bars.head, bars.lines) = (None, Vec::new());
    bars.draw(&mut std::io::stderr().lock(), "");
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
        $crate::util::progress::message(format_args!($($arg)*))
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
    bars.draw(&mut std::io::stderr().lock(), "");
}

/// `n` in thousands (`k`), millions (`M`) or billions (`B`), one decimal: `1.2B`.
pub fn compact(n: u64) -> String {
    let (value, unit) = match n {
        1_000_000_000.. => (n as f64 / 1e9, "B"),
        1_000_000.. => (n as f64 / 1e6, "M"),
        1_000.. => (n as f64 / 1e3, "k"),
        _ => return n.to_string(),
    };
    format!("{value:.1}{unit}").replace(&format!(".0{unit}"), unit)
}

#[cfg(test)]
mod tests {
    fn bars(rows: &[&str]) -> super::Bars {
        let lines = vec![(std::thread::current().id(), rows.iter().map(|r| r.to_string()).collect())];
        super::Bars { lines, head: None, shown: Vec::new(), last_draw_ms: 0 }
    }

    /// A frame the screen already shows is not written again. Repainting unchanged rows
    /// at 10 Hz made the bars flicker on terminals without synchronized updates.
    #[test]
    fn an_unchanged_frame_writes_nothing() {
        let (mut bars, mut out) = (bars(&["satex: a", "  b"]), Vec::new());
        bars.draw(&mut out, "");
        assert!(!out.is_empty());
        out.clear();
        bars.draw(&mut out, "");
        assert!(out.is_empty());
    }

    /// A message and the rows below it are one update that overwrites the old rows in place.
    /// The old writer erased the rows (`ESC [J`) and drew them anew in two writes, which
    /// showed a blank frame in between.
    #[test]
    fn a_message_overwrites_the_rows_in_one_update() {
        let (mut bars, mut out) = (bars(&["satex: a", "  b"]), Vec::new());
        bars.draw(&mut out, "");
        out.clear();
        bars.head = Some("satex: 2 early findings so far".into());
        bars.draw(&mut out, "note\n");
        let text = String::from_utf8(out).unwrap();
        assert_eq!(text.matches("\x1b[?2026h").count(), 1);
        let (before, rows) = text.split_once("note").unwrap();
        assert!(!before.contains("\x1b[J"), "erased before writing: {text:?}");
        assert_eq!(rows.matches("\x1b[J").count(), 1, "only the rows past the frame go: {text:?}");
        assert!(rows.find("early findings") < rows.find("satex: a"), "the headline stands first: {text:?}");
    }

    /// A frame has no more rows than the screen. One row more scrolled the headline into the
    /// scrollback, where no later frame reaches it: `N early findings so far` stayed there
    /// once for each time a deep `\\input` chain made the bars taller than a small terminal.
    #[test]
    fn a_frame_is_no_taller_than_the_screen() {
        let mut bars = bars(&["satex: a", "  b", "  c"]);
        bars.head = Some("head".into());
        let all = bars.lines[0].1.len() + 1;
        assert_eq!(bars.rows((super::SIZE.0, all)).len(), all);
        assert_eq!(bars.rows((super::SIZE.0, all - 1)), ["head", "satex: a", "  b"]);
    }

    #[test]
    fn a_line_is_cut_to_the_width_without_counting_escapes() {
        let line = "\x1b[1mabc\x1b[0m \x1b]8;;file:///x\x1b\\defgh\x1b]8;;\x1b\\";
        assert_eq!(super::fit(line, 20), line);
        assert_eq!(super::fit(line, 6), "\x1b[1mabc\x1b[0m \x1b]8;;file:///x\x1b\\de\x1b[0m\x1b]8;;\x1b\\");
    }

    /// A cache build's share counts every file met, a data file read by `\read` too: the
    /// kernel's first file stood at 5 % while expl3 read UnicodeData.txt for a minute. It never
    /// goes back when a file is met, and a new build of the same first file starts at its share.
    #[test]
    fn a_cache_build_share_counts_the_files_met_and_only_grows() {
        let f = |p: &str, r, a| (format!("/nonexistent/{p}"), r, a);
        let share = |open: &[(String, usize, usize)]| super::share(open).0;
        assert_eq!(share(&[f("latex.ltx", 50, 1000)]), 50);
        // expl3 met: the share is held, not dropped to 50 / 11000
        assert_eq!(share(&[f("latex.ltx", 50, 1000), f("expl3.tex", 0, 10000)]), 50);
        let reading = share(&[f("latex.ltx", 50, 1000), f("expl3.tex", 9000, 10000), f("data.txt", 1000, 4000)]);
        let read = share(&[f("latex.ltx", 50, 1000), f("expl3.tex", 9000, 10000), f("data.txt", 3000, 4000)]);
        assert!(50 < reading && reading < read, "{reading} {read}");
        // expl3 and the data file closed: read whole
        assert!(share(&[f("latex.ltx", 900, 1000)]) > 950);
        assert_eq!(share(&[f("latex.ltx", 10, 1000)]), 10, "a new build");
    }
}
