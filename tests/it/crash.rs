//! Crash resistance: no input ends satex by a signal, a panic, an internal
//! error or a hang. A crash is never a known red: `scripts/ci-test.sh` refuses
//! a `crash::` line in tests/known-red.txt, and a stack overflow ends the
//! whole test binary, which fails the shard whatever the ledger says.
//!
//! Two tiers. In the process, on a 2 MiB thread: hostile shapes (deep nesting,
//! runaway recursion, huge one-of fan-outs, overflowing arithmetic) and seeded
//! mutants of small documents, thousands in seconds. As child processes with a
//! 2 MiB main stack and a memory cap: `satex lint` and a `satex lsp --lines`
//! session on the site examples, the benchmark documents and the templates.
//! The source a run died on stays in [`last`]: the anonymous MWE.

use crate::support::{PRELUDE, bare, config, installed, satex, tmp_root};
use satex::analysis::machine::Machine;
use satex::project::config::{Config, Load};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The stack every run here gets: what a spawned thread has by default, a
/// quarter of the binary's main thread.
const STACK: usize = 2 << 20;
/// Steps one in-process run may take: enough to nest as deep as the shapes go.
const STEPS: u64 = 400_000;
/// Memory one in-process run may hold.
const MEMORY: u64 = 512 << 20;
/// How deep the hostile shapes nest: past every nesting bound satex has.
const DEEP: usize = 1500;
/// Mutants made of each small document.
const MUTANTS: usize = 120;
/// An in-process run longer than this is named: these sources are tiny.
const SLOW: Duration = Duration::from_secs(1);
/// A child run far above what its document needs, far below a hang.
const LIMIT: Duration = Duration::from_secs(600);
/// What a child may hold: `limits.memory` stops the run cleanly before it.
const CHILD_MEMORY: &str = "3GiB";
/// `limits.seconds` of a child's runs: a run that satex stops itself is no
/// crash, and a document that needs longer is a budget test's matter.
const CHILD_SECONDS: &str = "limits.seconds=60";

/// Where the source being run is kept: it is still there after an abort.
fn last(name: &str) -> PathBuf {
    tmp_root().join(format!("crash-last-{name}.tex"))
}

/// xorshift64*: mutants are the same on every run and machine.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
}

/// What a mutation puts in: the tokens whose balance TeX checks.
const INSERTS: [&str; 16] = [
    "{",
    "}",
    "$",
    "$$",
    "#",
    "%",
    "&",
    "\\fi ",
    "\\else ",
    "\\endcsname ",
    "\\csname ",
    "\\begingroup ",
    "\\endgroup ",
    "\\par ",
    "\\relax ",
    "ä€𝒜",
];

/// `MUTANTS` variants of `source`: cut after every line, then lines dropped,
/// doubled or swapped and tokens put in at random places (never inside a character).
fn mutants(source: &str, seed: u64) -> Vec<String> {
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    let mut out: Vec<String> = (1..lines.len()).map(|n| lines[..n].concat()).collect();
    let mut rng = Rng(seed | 1);
    while out.len() < MUTANTS {
        let mut ls: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
        for _ in 0..1 + rng.below(3) {
            let i = rng.below(ls.len());
            match rng.below(5) {
                0 => drop(ls.remove(i)),
                1 => ls.insert(i, ls[i].clone()),
                2 => {
                    let j = rng.below(ls.len());
                    ls.swap(i, j);
                }
                _ => {
                    let at = (0..=ls[i].len()).filter(|&b| ls[i].is_char_boundary(b)).nth(rng.below(ls[i].len() + 1));
                    ls[i].insert_str(at.unwrap_or(0), INSERTS[rng.below(INSERTS.len())]);
                }
            }
            if ls.is_empty() {
                break;
            }
        }
        out.push(ls.concat());
    }
    out
}

/// Plain TeX's `\loop`, which the shapes below use without a format.
const LOOP: &str =
    r"\def\loop#1\repeat{\def\body{#1}\iterate}\def\iterate{\body\let\next\iterate\else\let\next\relax\fi\next}";

/// Sources built to exhaust something: each line of this list is one class of crash.
fn hostile() -> Vec<(&'static str, String)> {
    let rep = |s: &str, n: usize| s.repeat(n);
    let nest = |open: &str, close: &str| format!("{}x{}", rep(open, DEEP), rep(close, DEEP));
    // One more text of `\x` per undecided test, joined at each `\fi`.
    let fan: String = (0..300).map(|k| format!("\\ifnum\\pdfelapsedtime>{k} \\def\\x{{v{k}}}\\fi ")).collect();
    let grown: String = (0..60)
        .map(|k| format!("\\ifnum\\pdfelapsedtime>{k} \\edef\\x{{\\x b}}\\else\\edef\\x{{c\\x}}\\fi "))
        .collect();
    let uses = r"\edef\y{\x\x\x}\expandafter\def\csname\x\endcsname{}\message{\x}\edef\z{\detokenize\expandafter{\x}}\count1=0\x\relax \ifx\x\y\fi \uppercase\expandafter{\x}\immediate\write-1{\x}\expandafter\let\x\relax";
    vec![
        ("groups", nest("{", "}")),
        ("groups-open", rep("{", DEEP)),
        ("groups-closed", rep("}", DEEP)),
        ("begingroups", nest("\\begingroup ", "\\endgroup ")),
        ("boxes", nest("\\hbox{", "}")),
        ("vboxes", nest("\\vbox{\\hbox{", "}}")),
        ("math", rep("$", DEEP)),
        ("math-boxes", nest("$\\hbox{", "}$")),
        ("left-right", format!("${}x{}$", rep("\\left(", DEEP), rep("\\right)", DEEP))),
        ("aligns", nest("\\halign{#\\cr ", "\\cr}")),
        ("conditionals", nest("\\iftrue ", "\\fi ")),
        ("conditionals-open", rep("\\iftrue ", DEEP)),
        ("undecided", nest("\\ifnum\\pdfelapsedtime>0 ", "\\fi ")),
        ("csnames", format!("{}x{}", rep("\\csname ", DEEP), rep("\\endcsname ", DEEP))),
        ("expandafters", format!("{}\\relax", rep("\\expandafter", DEEP))),
        ("noexpands", format!("\\edef\\a{{{}}}", rep("\\noexpand\\expandafter", DEEP))),
        ("numbers", format!("\\count1={}1 ", rep("\\numexpr(", DEEP))),
        ("self", r"\def\a{\a}\a".into()),
        ("self-grows", r"\def\a#1{\a{#1#1}}\a{x}".into()),
        ("mutual", r"\def\a{\b}\def\b{\a\a}\a".into()),
        ("self-number", r"\def\a{\number\a}\a".into()),
        ("self-csname", r"\def\a{\csname\a\endcsname}\a".into()),
        ("self-edef", r"\def\a{\edef\b{\a}}\a".into()),
        ("self-roman", r"\def\a{\romannumeral\a}\a".into()),
        ("self-if", r"\def\a{\ifnum\a>0 \fi}\a".into()),
        ("self-the", r"\def\a{\the\a}\a".into()),
        ("self-box", r"\def\a{\setbox0\hbox{\a}}\a".into()),
        ("self-expandafter", r"\def\a{\expandafter\a\a}\a".into()),
        ("self-aftergroup", r"\def\a{{\aftergroup\a\aftergroup\a}}\a".into()),
        ("self-everypar", r"\everypar{x\par}x".into()),
        ("self-output", r"\output{\unvbox255 \penalty-10000 }\hbox{x}\penalty-10000 ".into()),
        ("doubling", format!("\\def\\a{{xxxxxxxx}}{}", rep("\\edef\\a{\\a\\a}", 60))),
        ("doubling-toks", format!("\\toks0{{xxxxxxxx}}{}", rep("\\toks0\\expandafter{\\the\\toks0 \\the\\toks0 }", 60))),
        ("long-line", rep("x ", 400_000)),
        ("pars", rep("x\\par ", 100_000)),
        ("fan-out", format!("{fan}{uses}")),
        ("fan-nested", format!("\\def\\x{{a}}{grown}{uses}")),
        ("repetition", format!("{LOOP}\\def\\x{{}}\\loop\\edef\\x{{\\x a}}\\ifnum\\pdfelapsedtime<9 \\repeat {uses}")),
        (
            "repetition-nested",
            format!("{LOOP}\\def\\x{{}}\\loop\\edef\\x{{\\x a}}\\ifnum\\pdfelapsedtime<9 \\repeat {grown}{uses}"),
        ),
        (
            "overflow",
            r#"\count1=2147483647 \advance\count1 1 \multiply\count1 \count1 \divide\count1 0 \count2=-2147483647 \advance\count2-1 \divide\count2-1
\dimen0=16383.99999pt \multiply\dimen0 99999 \advance\dimen0\dimen0 \dimen1=-\maxdimen \advance\dimen1-\maxdimen
\edef\a{\number\numexpr 2147483647*2147483647\relax}\edef\a{\number\numexpr -2147483647-1\relax}\edef\a{\number\numexpr 1/0\relax}
\edef\a{\the\dimexpr 16383pt*16383\relax}\edef\a{\the\glueexpr 1pt plus 16383fill*99999\relax}\edef\a{\the\numexpr\maxdimen*\maxdimen/1\relax}
\skip0=16383pt plus 16383fill minus 16383filll \multiply\skip0 2147483647 \hskip\skip0 \kern\maxdimen\kern\maxdimen
\count 99999=1 \dimen-1=1pt \catcode 99999999=5 \catcode`a=99 \char 99999999 \mathchar 2147483647 \delimiter 2147483647
\penalty-2147483647 \penalty 2147483647 \hbox to-\maxdimen{x}\vbox spread\maxdimen{x}\vrule width\maxdimen height\maxdimen
\romannumeral 2147483647 \number-2147483648 \uccode 2147483647=1 \lccode`a=2147483647 \lowercase{a}\fontdimen 2147483647\nullfont=1pt
\hyphenchar\nullfont=2147483647 \scriptspace=99999999999999999999pt \count3=99999999999999999999 \count4='777777777777 \count5=\string"FFFFFFFFF
\vsplit 0 to-\maxdimen \setbox 2147483647\hbox{} \box-1 \wd 99999=1pt \parshape 2147483647 \hangafter-2147483647 \looseness 2147483647
\endlinechar=2147483647 \newlinechar=-2147483647 \escapechar=99999999 \tolerance-1 \hsize=-\maxdimen x\par \pdfstrcmp{a}{b}"#.into(),
        ),
        ("characters", "\\def\\ä€𝒜{𝒜}\\ä€𝒜 \\csname ä\u{0}\u{7f}\u{ffff}\u{10fffd}\\endcsname ^^M^^@^^?^^ff ^^^^ffff ^^^^^^10ffff \\catcode`ä=13 ä".into()),
    ]
}

/// The small documents the mutants are made of: each mixes what a cut or a
/// stray token can leave open.
const SMALL: [&str; 4] = [
    r"\def\a#1,#2.{(#1|#2)}\a x,y.
\def\b#1{\ifx#1\relax\else\expandafter\b\fi}\b abc\relax
\edef\c{\expandafter\noexpand\csname d\number\count1\endcsname}
\setbox0\hbox{\vbox{x\par $y^{z}_{w}$}}\wd0=\dimexpr 2\ht0+1pt\relax
\begingroup\catcode`\!=13 \gdef!{\endgroup}!",
    r"\newcount\n \n=0
\loop\advance\n 1 \ifnum\n<5 \edef\x{\x\number\n}\repeat
\ifnum\pdfelapsedtime>3 \def\y{a}\else\def\y{bb}\fi
\expandafter\def\csname k\y\endcsname#1{[#1]}\csname k\y\endcsname{\y}
\halign{#\hfil&\hfil#\cr a&b\cr\noalign{\kern1pt}c&d\cr}",
    r"\toks0{\a\b}\toks2\expandafter{\the\toks0 \c}
\uppercase{\def\q{abc}}\lowercase\expandafter{\q}
\openout3=crash-out \write3{\the\toks2}\immediate\write-1{\meaning\q}
\ifcase\count2 a\or b\else c\fi \ifdim\wd0>1pt \ifvoid0 \fi\fi
$$\left(\frac{a}{b}\right)\eqno(1)$$ \vskip 1pt plus 1fil \penalty-10000",
    r"\ExplSyntaxOn
\tl_set:Nn \l_tmpa_tl { a , b , c }
\clist_map_inline:Nn \l_tmpa_tl { \tl_put_right:Nn \l_tmpb_tl { #1 } }
\int_step_inline:nn { 4 } { \seq_put_right:Nn \l_tmpa_seq { #1 } }
\ExplSyntaxOff
\newcommand\w[2][o]{#1#2}\w[\w{x}]{\textbf{y}}\begin{center}\w{z}\end{center}",
];

/// Runs `sources` one after another on a 2 MiB thread and fails on the first
/// that satex could not survive in order (an internal error). An overflowing
/// stack ends the test binary: the source it ran is in [`last`].
fn runs_in_process(name: &'static str, cfg: Config, sources: Vec<(String, String)>) {
    let path = last(name);
    let run = move || {
        let mut died = Vec::new();
        for (label, source) in sources {
            std::fs::write(&path, &source).expect("the source being run is kept");
            let start = Instant::now();
            let analysis = Machine::analyze(&source, None, &cfg);
            if start.elapsed() > SLOW {
                eprintln!("crash::{name} {label}: {:.1?}", start.elapsed());
            }
            if analysis.facts.diagnostics.iter().any(|d| d.code == "internal-error") {
                let keep = path.with_file_name(format!("crash-{name}-{}.tex", died.len()));
                std::fs::write(&keep, &source).expect("the MWE is kept");
                died.push(format!("{label}: {}", keep.display()));
            }
        }
        died
    };
    let died = std::thread::Builder::new().stack_size(STACK).spawn(run).expect("a thread").join().expect("no panic");
    assert!(died.is_empty(), "satex hit a bug on:\n{}", died.join("\n"));
}

/// The bare engine, held to few steps and little memory so a runaway shape ends at once.
fn small(cfg: Config) -> Config {
    let mut cfg = cfg;
    cfg.limits.steps = STEPS;
    cfg.limits.memory = bytesize::ByteSize(MEMORY);
    cfg
}

/// Intuition: nesting, recursion and growth past every bound satex has must
/// end at that bound, reported, on a quarter of the usual stack. Pins the
/// crash classes one shape each: recursion per group, box, conditional,
/// `\csname` or expansion level; a walk into one-of texts that fan out, nest
/// or hold themselves; arithmetic past TeX's ranges; memory that doubles.
#[test]
fn hostile_shapes_end_in_order() {
    let sources: Vec<(String, String)> =
        hostile().into_iter().map(|(n, s)| (n.to_string(), format!("{PRELUDE}{s}"))).collect();
    // The playground's wasm build runs the same shapes (doc/site/e2e/crash.mjs).
    if let Some(dir) = std::env::var_os("SATEX_CRASH_SHAPES") {
        std::fs::create_dir_all(&dir).expect("the shapes' folder");
        for (name, source) in &sources {
            std::fs::write(Path::new(&dir).join(format!("{name}.tex")), source).expect("a shape is written");
        }
    }
    runs_in_process("hostile", small(bare()), sources);
}

/// Intuition: an editor shows satex every half-typed state of a document, so
/// a text cut anywhere, or with a line or a token too many, must still be
/// analysed in order. Pins crashes on unbalanced input: `MUTANTS` seeded
/// mutants of each small plain-TeX document.
#[test]
fn mutants_of_plain_documents_end_in_order() {
    let mut sources = Vec::new();
    for (i, doc) in SMALL[..3].iter().enumerate() {
        let doc = format!("{LOOP}\n{doc}");
        sources.extend(
            mutants(&doc, i as u64 + 1)
                .into_iter()
                .enumerate()
                .map(|(k, m)| (format!("{i}/{k}"), format!("{PRELUDE}{m}"))),
        );
    }
    runs_in_process("plain", small(bare()), sources);
}

/// Intuition: the same on the real LaTeX kernel, where a cut lands inside
/// expl3 loops and `\begin`/`\end` pairs. Pins crashes of the kernel's own
/// error recovery on unbalanced input.
#[test]
fn mutants_of_kernel_documents_end_in_order() {
    if !installed() {
        return;
    }
    // The limits stay the defaults: the kernel's cache is keyed by them.
    let cfg = Config { load: Load { packages: false, classes: false, ..Default::default() }, ..config() };
    let sources = mutants(SMALL[3], 7).into_iter().enumerate().map(|(k, m)| (format!("kernel/{k}"), m)).collect();
    runs_in_process("kernel", cfg, sources);
}

/// What a child process must not say: each is a crash satex did not contain.
const DEATHS: [&str; 5] =
    ["panicked at", "has overflowed its stack", "memory allocation of", "internal-error", "satex hit a bug"];

/// [`child`] on a 2 MiB stack: its stdout.
fn survives(dir: &Path, args: &[&str], stdin: &str) -> Result<String, String> {
    child(dir, args, stdin, Some(STACK)).map(|(out, _)| out)
}

/// `satex args` in `dir` with `stack` bytes of main stack and thread stacks
/// (`None`: what a user's run has), `stdin` fed, ended by itself within
/// [`LIMIT`], by no signal and with none of [`DEATHS`] in what it printed.
/// Returns its stdout and stderr.
pub(crate) fn child(dir: &Path, args: &[&str], stdin: &str, stack: Option<usize>) -> Result<(String, String), String> {
    use std::io::Write;
    use std::process::Stdio;
    let mut command = match stack {
        // The main thread's stack is the shell's limit.
        Some(stack) if cfg!(unix) => {
            let mut sh = std::process::Command::new("sh");
            let exe = satex();
            sh.args(["-c", &format!("ulimit -s {}; exec \"$0\" \"$@\"", stack >> 10)]).arg(exe.get_program());
            sh.envs(exe.get_envs().filter_map(|(k, v)| Some((k, v?))));
            sh
        }
        _ => satex(),
    };
    command.args(args).current_dir(dir);
    if let Some(stack) = stack {
        command.env("RUST_MIN_STACK", stack.to_string());
    }
    let mut child =
        command.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("satex starts");
    let mut input = child.stdin.take().expect("a pipe");
    let text = stdin.to_string();
    let feed = std::thread::spawn(move || {
        let _ = input.write_all(text.as_bytes());
    });
    fn read(mut pipe: impl std::io::Read + Send + 'static) -> std::thread::JoinHandle<String> {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            String::from_utf8_lossy(&bytes).into_owned()
        })
    }
    let (out, err) = (read(child.stdout.take().expect("a pipe")), read(child.stderr.take().expect("a pipe")));
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().expect("satex runs") {
            break status;
        }
        if start.elapsed() > LIMIT {
            let _ = child.kill();
            return Err(format!("`satex {}` did not end within {LIMIT:?}", args.join(" ")));
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let _ = feed.join();
    let (out, err) = (out.join().expect("read"), err.join().expect("read"));
    if status.code().is_none() {
        return Err(format!("`satex {}` was ended by a signal ({status}): {}", args.join(" "), tail(&err)));
    }
    match DEATHS.iter().find(|d| err.contains(**d) || out.contains(**d)) {
        Some(death) => Err(format!("`satex {}` said `{death}`: {}", args.join(" "), tail(&err))),
        None => Ok((out, err)),
    }
}

/// The end of what a child printed, for the failure message.
fn tail(text: &str) -> String {
    const SHOWN: usize = 1200;
    let from = (text.len().saturating_sub(SHOWN)..=text.len()).find(|&b| text.is_char_boundary(b)).unwrap_or(0);
    text[from..].to_string()
}

/// A `satex lsp --lines` session on `text` as `main.tex` in `dir`: opened and
/// analysed, asked at places spread over the text (line ends, past the end,
/// inside multi-byte characters by UTF-16 offset), then changed to each of
/// `edits` and asked again. Every request must be answered.
fn session(dir: &Path, text: &str, edits: &[String]) -> Result<(), String> {
    use serde_json::json;
    let uri = format!("file://{}", crate::support::tex_path(&dir.join("main.tex")));
    let mut lines = vec![
        json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"processId": null, "rootUri": null, "capabilities": {}}}),
        json!({"jsonrpc": "2.0", "method": "initialized", "params": {}}),
    ];
    let mut id = 1;
    for (version, text) in std::iter::once(text).chain(edits.iter().map(String::as_str)).enumerate() {
        let doc = json!({"uri": uri, "languageId": "latex", "version": version + 1, "text": text});
        lines.push(match version {
            0 => json!({"jsonrpc": "2.0", "method": "textDocument/didOpen", "params": {"textDocument": doc}}),
            _ => json!({"jsonrpc": "2.0", "method": "textDocument/didChange", "params":
                {"textDocument": {"uri": uri, "version": version + 1}, "contentChanges": [{"text": text}]}}),
        });
        lines.push(json!({"jsonrpc": "2.0", "method": "satex/analyze", "params": {"first": uri}}));
        let rows: Vec<&str> = text.lines().collect();
        let mut places: Vec<(usize, usize)> = vec![(0, 0), (rows.len(), 0), (rows.len() + 9, 99), (0, 1 << 20)];
        for k in 0..PLACES {
            let line = k * rows.len().max(1) / PLACES;
            let width = rows.get(line).map_or(0, |r| r.encode_utf16().count());
            places.extend([(line, width), (line, width / 2), (line, width / 2 + 1)]);
        }
        for (line, character) in places {
            for method in ["textDocument/hover", "textDocument/completion", "textDocument/definition", "satex/view"] {
                id += 1;
                lines.push(json!({"jsonrpc": "2.0", "id": id, "method": method, "params":
                    {"textDocument": {"uri": uri}, "position": {"line": line, "character": character}}}));
            }
        }
        for method in ["textDocument/codeLens", "textDocument/documentSymbol", "textDocument/foldingRange"] {
            id += 1;
            lines.push(json!({"jsonrpc": "2.0", "id": id, "method": method, "params": {"textDocument": {"uri": uri}}}));
        }
    }
    let input: String = lines.iter().map(|l| format!("{l}\n")).collect();
    let memory = format!("limits.memory={CHILD_MEMORY}");
    let out = survives(dir, &["--set", &memory, "--set", CHILD_SECONDS, "lsp", "--lines"], &input)?;
    let answered = out
        .lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .flat_map(|v| v.as_array().cloned().unwrap_or_else(|| vec![v]))
        .filter(|m| m.get("id").is_some() && m.get("method").is_none())
        .count();
    match answered == id {
        true => Ok(()),
        false => Err(format!("the session answered {answered} of {id} requests")),
    }
}

/// Places per text a session asks at, besides its corners.
const PLACES: usize = 6;

/// A fresh folder with the files of `from`'s: a document's own, beside its `main.tex`.
pub(crate) fn folder(tag: &str, from: Option<&Path>) -> PathBuf {
    let dir = crate::support::scratch_dir(tmp_root().join(tag));
    for entry in from.into_iter().flat_map(std::fs::read_dir).flatten().flatten() {
        if entry.path().is_file() {
            let _ = std::fs::copy(entry.path(), dir.join(entry.file_name()));
        }
    }
    dir
}

/// The playground's examples: each one's name and path.
pub(crate) fn site_examples() -> Vec<(String, PathBuf)> {
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("doc/site/examples");
    let mut paths: Vec<PathBuf> =
        std::fs::read_dir(&examples).expect("the examples").flatten().map(|e| e.path()).collect();
    paths.sort();
    paths.retain(|p| p.extension().is_some_and(|e| e == "tex"));
    paths.into_iter().map(|p| (p.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string(), p)).collect()
}

/// The benchmark documents (benches/docs.txt): each one's name and path.
pub(crate) fn benchmarks() -> Vec<(String, PathBuf)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let list = std::fs::read_to_string(root.join("benches/docs.txt")).expect("the benchmark list");
    let paths = list.lines().filter(|l| !l.trim().is_empty() && !l.starts_with('#'));
    let paths = paths.map(|l| root.join(l.split_whitespace().next().expect("a path")));
    paths.map(|p| (p.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string(), p)).collect()
}

/// Every template this installation can write (`satex new`, offline), each written to a
/// folder of its own under `tag`: its id and that folder. A class the installation lacks,
/// or a template only its provider has, is left out.
pub(crate) fn templates(tag: &str) -> Vec<(String, PathBuf)> {
    let listed = satex().args(["new", "--list", "--format", "json"]).output().expect("satex runs");
    let listed: serde_json::Value = serde_json::from_slice(&listed.stdout).expect("the templates as JSON");
    let ids = listed.as_array().expect("a list").iter().map(|t| t["id"].as_str().expect("an id").to_string());
    ids.filter_map(|id| {
        let dir = crate::support::scratch_dir(tmp_root().join(format!("{tag}-{id}")));
        let made = satex()
            .args(["new", "-t", &id, "--no-check", "--force", "--set", "templates.fetch=false"])
            .arg(&dir)
            .output();
        (made.is_ok_and(|o| o.status.success()) && dir.join("main.tex").exists()).then_some((id, dir))
    })
    .collect()
}

/// `lint` and a session with a few mutants on `text`, as `main.tex` in a copy of `from`'s folder.
fn document_survives(tag: &str, from: Option<&Path>, text: &str, engine: &[&str]) -> Vec<String> {
    let dir = folder(&format!("crash-{tag}"), from);
    let all = mutants(text, tag.len() as u64);
    // A cut in the preamble, one in the body, and two with stray tokens.
    let picked: Vec<String> =
        [all.len() / 8, all.len() / 3, MUTANTS - 2, MUTANTS - 1].iter().filter_map(|&i| all.get(i).cloned()).collect();
    let mut failed = Vec::new();
    for (k, source) in std::iter::once(text.to_string()).chain(picked.iter().cloned()).enumerate() {
        std::fs::write(dir.join("main.tex"), &source).expect("the document is written");
        let args =
            [&["lint", "-f", "main.tex", "--max-memory", CHILD_MEMORY, "--set", CHILD_SECONDS][..], engine].concat();
        if let Err(why) = survives(&dir, &args, "") {
            let keep = last(&format!("{tag}-{k}"));
            std::fs::write(&keep, &source).expect("the MWE is kept");
            failed.push(format!("{tag} mutant {k} ({}): {why}", keep.display()));
        }
    }
    std::fs::write(dir.join("main.tex"), text).expect("the document is written");
    if let Err(why) = session(&dir, text, &picked) {
        failed.push(format!("{tag} session: {why}"));
    }
    failed
}

/// Intuition: the playground's examples are the first documents a visitor
/// edits. Pins crashes of `lint` and of the language server on them and on
/// cut or unbalanced variants, on a 2 MiB stack.
#[test]
fn the_site_examples_survive_lint_and_a_session() {
    if !installed() {
        return;
    }
    let mut failed = Vec::new();
    for (name, path) in site_examples() {
        let text = std::fs::read_to_string(&path).expect("an example");
        failed.extend(document_survives(&format!("site-{name}"), path.parent(), &text, &[]));
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// Intuition: a new document starts from a provider template, so each must
/// survive from its first line on. Pins crashes and hangs on every template
/// the TeX installation ships (`satex new`), cut or unbalanced variants too.
#[test]
fn the_templates_survive_lint_and_a_session() {
    if !installed() {
        return;
    }
    let mut failed = Vec::new();
    for (id, dir) in templates("crash-new") {
        let text = std::fs::read_to_string(dir.join("main.tex")).expect("the template");
        failed.extend(document_survives(&format!("template-{id}"), Some(&dir), &text, &[]));
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// Intuition: the benchmark documents are the breadth of packages satex
/// meets. Pins crashes of `lint` on each and on its mutants.
#[test]
fn the_benchmark_documents_survive_lint() {
    if !installed() {
        return;
    }
    let mut failed = Vec::new();
    for (name, path) in benchmarks() {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let tag = format!("bench-{name}");
        let dir = folder(&format!("crash-{tag}"), path.parent());
        let all = mutants(&text, tag.len() as u64);
        for (k, source) in [text.clone(), all[all.len() / 3].clone(), all[MUTANTS - 1].clone()].iter().enumerate() {
            std::fs::write(dir.join("main.tex"), source).expect("the document is written");
            let args = ["lint", "-f", "main.tex", "--max-memory", CHILD_MEMORY, "--set", CHILD_SECONDS];
            if let Err(why) = survives(&dir, &args, "") {
                let keep = last(&format!("{tag}-{k}"));
                std::fs::write(&keep, source).expect("the MWE is kept");
                failed.push(format!("{tag} mutant {k} ({}): {why}", keep.display()));
            }
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
