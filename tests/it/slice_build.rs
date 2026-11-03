//! A backward slice is itself a working file of the same kind.
//!
//! Every case below is a whole file — plain TeX, e-TeX, a LaTeX document, a
//! package, a class — with a probe line that prints what the criterion
//! computes.  The slice around the probe is reconstructed, run through the
//! engine that runs the original, and has to compile without an error and
//! print the same probe.  A slice that drops something the probe depends on
//! prints something else or does not compile at all.
//!
//! A probe is written `\message{SATEX<label|…|SATEX}` (or with `\typeout`),
//! so that it can be found in the log however the engine wraps it.

use std::path::{Path, PathBuf};
use std::process::Command;

use satex::analysis::machine::Machine;
use satex::project::config::Config;
use satex::queries::query::{self, At, Direction};

/// Where the installed engines live; `None` skips the case.
fn engine(name: &str) -> Option<PathBuf> {
    let which = Command::new("kpsewhich").arg("--var-value=SELFAUTOLOC").output().ok()?;
    let directory = PathBuf::from(String::from_utf8(which.stdout).ok()?.trim());
    let binary = directory.join(name);
    binary.is_file().then_some(binary)
}

fn scratch(name: &str) -> PathBuf {
    let directory = crate::support::tmp_root().join("satex-slice-build").join(name);
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

/// What a run of the engine left behind: whether it stopped cleanly, every
/// probe it printed, and the text of the PDF when there is one.
struct Run {
    ok: bool,
    log: String,
    probes: Vec<(String, String)>,
    text: Option<String>,
}

const OPEN: &str = "SATEX<";
const CLOSE: &str = "|SATEX";

fn probes(log: &str) -> Vec<(String, String)> {
    let flat = log.replace('\n', "");
    let mut out = Vec::new();
    let mut rest = flat.as_str();
    while let Some(start) = rest.find(OPEN) {
        rest = &rest[start + OPEN.len()..];
        let Some(bar) = rest.find('|') else { break };
        let Some(end) = rest.find(CLOSE) else { break };
        if end < bar {
            break;
        }
        out.push((rest[..bar].to_string(), rest[bar + 1..end].to_string()));
        rest = &rest[end + CLOSE.len()..];
    }
    out
}

/// Run `binary` on `file` in `directory` as often as cross-references need.
fn compile(binary: &Path, directory: &Path, file: &str, passes: usize) -> Run {
    let stem = Path::new(file).file_stem().unwrap().to_string_lossy().to_string();
    let mut ok = true;
    for _ in 0..passes {
        let status = Command::new(binary)
            .current_dir(directory)
            .env("max_print_line", "100000")
            .args(["-interaction=nonstopmode", "-halt-on-error"])
            .arg(file)
            .output()
            .expect("running the engine");
        ok &= status.status.success();
        if !ok {
            break;
        }
    }
    let log = std::fs::read_to_string(directory.join(format!("{stem}.log"))).unwrap_or_default();
    let pdf = directory.join(format!("{stem}.pdf"));
    let text = (pdf.is_file() && satex::project::paths::which("pdftotext").is_ok())
        .then(|| {
            let out = Command::new("pdftotext").arg(&pdf).arg("-").output().ok()?;
            Some(String::from_utf8_lossy(&out.stdout).to_string())
        })
        .flatten();
    Run { ok, probes: probes(&log), log, text }
}

/// One file of the corpus and the criterion to slice it on.
struct Case {
    name: &'static str,
    engine: &'static str,
    /// The file the slice is taken from: `main.tex`, `mine.sty`, `mine.cls`.
    file: &'static str,
    source: &'static str,
    /// The document that runs the file, when the file is a package or a
    /// class; the file itself otherwise.
    driver: Option<&'static str>,
    /// Names sliced on, besides the position.
    names: &'static [&'static str],
    /// The line the criterion is at, in `file`.
    line: u32,
    /// The probe whose output the slice has to keep.
    probe: &'static str,
    /// A line of the PDF's text that has to come out the same.
    text: Option<&'static str>,
    passes: usize,
    /// Files the document reads besides itself, beside it in both runs.
    files: &'static [(&'static str, &'static str)],
}

impl Case {
    const fn new(name: &'static str, engine: &'static str, source: &'static str, line: u32) -> Case {
        Case {
            name,
            engine,
            file: "main.tex",
            source,
            driver: None,
            names: &[],
            line,
            probe: "a",
            text: None,
            passes: 1,
            files: &[],
        }
    }
}

fn config(directory: &Path) -> Config {
    crate::support::discover(directory)
}

/// The reconstruction for `case`, or why there is none.
fn sliced(case: &Case, directory: &Path) -> String {
    let path = directory.join(case.file);
    std::fs::write(&path, case.source).unwrap();
    let cfg = config(directory);
    let analysis = Machine::analyze(case.source, Some(&path), &cfg);
    let names: Vec<String> = case.names.iter().map(|n| (*n).to_string()).collect();
    let at = At::here(case.line, 1);
    let records = query::slice(&analysis, &names, Some(&at), Direction::Backward);
    query::reconstruct(&analysis, &records, case.source)
}

fn value<'a>(run: &'a Run, probe: &str) -> Option<&'a str> {
    run.probes.iter().find(|(label, _)| label == probe).map(|(_, v)| v.as_str())
}

fn line_with<'a>(text: &'a str, start: &str) -> Option<&'a str> {
    text.lines().find(|line| line.trim_start().starts_with(start))
}

/// The end of a log, where the engine says why it stopped.
fn tail(log: &str) -> String {
    let lines: Vec<&str> = log.lines().collect();
    lines[lines.len().saturating_sub(20)..].join("\n")
}

/// Run `case` and its slice; the failure message, if the slice does not
/// behave like the original.
fn check(case: &Case) -> Option<String> {
    let binary = engine(case.engine)?;
    let root = scratch(case.name);
    let original = root.join("original");
    let slice = root.join("slice");
    std::fs::create_dir_all(&original).unwrap();
    std::fs::create_dir_all(&slice).unwrap();
    for (name, text) in case.files {
        std::fs::write(original.join(name), text).unwrap();
    }
    let text = sliced(case, &original);
    // A LaTeX document's slice is standalone: it compiles in an empty directory.
    if !text.contains("\\documentclass") {
        for (name, text) in case.files {
            std::fs::write(slice.join(name), text).unwrap();
        }
    }
    std::fs::write(slice.join(case.file), &text).unwrap();
    let run_file = match case.driver {
        Some(driver) => {
            std::fs::write(original.join("driver.tex"), driver).unwrap();
            std::fs::write(slice.join("driver.tex"), driver).unwrap();
            "driver.tex"
        }
        None => case.file,
    };
    let before = compile(&binary, &original, run_file, case.passes);
    assert!(before.ok, "{}: the original does not compile:\n{}", case.name, tail(&before.log));
    let wanted = value(&before, case.probe).map(str::to_string);
    assert!(
        wanted.is_some() || case.text.is_some() || case.probe.is_empty(),
        "{}: the original prints no probe {}",
        case.name,
        case.probe
    );
    let after = compile(&binary, &slice, run_file, case.passes);
    if !after.ok {
        return Some(format!(
            "{}: the slice does not compile with {}:\n--- slice\n{text}--- log\n{}",
            case.name,
            case.engine,
            tail(&after.log)
        ));
    }
    let got = value(&after, case.probe);
    if wanted.is_some() && got != wanted.as_deref() {
        return Some(format!("{}: the probe prints {got:?} instead of {wanted:?}\n--- slice\n{text}", case.name));
    }
    if let Some(start) = case.text {
        let want = before.text.as_deref().and_then(|t| line_with(t, start)).map(str::to_string);
        let got = after.text.as_deref().and_then(|t| line_with(t, start)).map(str::to_string);
        if want.is_some() && want != got {
            return Some(format!("{}: the PDF says {got:?} instead of {want:?}\n--- slice\n{text}", case.name));
        }
    }
    let _ = std::fs::remove_dir_all(&root);
    None
}

fn run_all(cases: &[Case]) {
    if satex::project::paths::which("kpsewhich").is_err() {
        return;
    }
    let failures: Vec<String> = cases.iter().filter_map(check).collect();
    assert!(
        failures.is_empty(),
        "{} of {} slices misbehave:\n\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n\n")
    );
}

// --- plain TeX and its extensions ----------------------------------------

const PLAIN_CATCODES: &str = r"\catcode`\@=11
\def\p@int{P}
\def\shown{\p@int}
\catcode`\@=12
\def\unused{U}
\newcount\mycount
\mycount=7
\advance\mycount by 3
\message{SATEX<a|\shown:\the\mycount|SATEX}
\bye
";

const PLAIN_ACTIVE: &str = r"\catcode`\~=13
\def~{tilde}
{\catcode`\!=13 \gdef!{bang}}
\def\other{O}
\message{SATEX<a|~|SATEX}
\catcode`\!=13
\message{SATEX<b|!|SATEX}
\bye
";

const PLAIN_GROUPS: &str = r"\def\x{outer}
{\def\x{inner}}
\def\setres{\gdef\res{R}}
{\aftergroup\setres}
\begingroup \def\y{local}\global\let\z\y \endgroup
\message{SATEX<a|\x:\res:\z|SATEX}
\bye
";

const PLAIN_CONDITIONALS: &str = r"\newif\ifdraft
\drafttrue
\ifdraft \def\mode{draft}\else \def\mode{final}\fi
\def\other{O}
\ifx\other\undefined \def\seen{no}\else \def\seen{yes}\fi
\message{SATEX<a|\mode:\seen|SATEX}
\bye
";

const PLAIN_EXPAND: &str = r"\def\a{A}
\edef\b{\a\a}
\expandafter\def\csname built\endcsname{C}
\def\c{\csname built\endcsname}
\let\d\b
\def\a{changed}
\message{SATEX<a|\b:\c:\d|SATEX}
\bye
";

const ETEX: &str = r"\def\n{5}
\edef\m{\the\numexpr\n*3\relax}
\protected\def\p{P}
\def\q{\detokenize{\p}}
\unless\ifdefined\undefinedthing \def\r{fresh}\fi
\message{SATEX<a|\m:\q:\r|SATEX}
\end
";

const PDFTEX: &str = r"\pdfoutput=1
\def\s{abc}
\edef\cmp{\pdfstrcmp{\s}{abd}}
\edef\hash{\pdfmdfivesum{\s}}
\message{SATEX<a|\cmp:\hash|SATEX}
\end
";

const PLAIN_BEGINGROUP: &str = r"\def\unused{U}
\begingroup
\catcode`\~=12
\gdef\tl{~}
\endgroup
\def~{T}
\message{SATEX<a|\tl~|SATEX}
\bye
";

// `\lastpenalty` is 0 at the start of the page, so the engine takes the
// `\else` arm, while satex, which does not model the page, keeps both arms.
const PLAIN_UNDECIDED: &str = r"\def\unused{U}
\ifnum\lastpenalty>0
  \def\when{late}
\else
  \def\when{early}
\fi
\def\other{O}
\message{SATEX<a|\when|SATEX}
\bye
";

const PLAIN_PARAMETERS: &str = r"\tolerance=500
\def\unused{U}
\advance\tolerance by 20
\hsize=100pt
\message{SATEX<a|\the\tolerance|SATEX}
\bye
";

const PLAIN_LOOP: &str = r"\newcount\n
\def\acc{}
\loop
  \edef\acc{\acc x}
  \advance\n by 1
\ifnum\n<3 \repeat
\def\unused{U}
\message{SATEX<a|\acc|SATEX}
\bye
";

#[test]
fn plain_slices_run_like_the_original() {
    run_all(&[
        Case::new("plain-catcodes", "tex", PLAIN_CATCODES, 9),
        Case { probe: "a", ..Case::new("plain-active-a", "tex", PLAIN_ACTIVE, 5) },
        Case { probe: "b", ..Case::new("plain-active-b", "tex", PLAIN_ACTIVE, 7) },
        Case::new("plain-groups", "tex", PLAIN_GROUPS, 6),
        Case::new("plain-conditionals", "tex", PLAIN_CONDITIONALS, 6),
        Case::new("plain-expand", "tex", PLAIN_EXPAND, 7),
        Case::new("etex", "etex", ETEX, 6),
        Case::new("pdftex", "pdftex", PDFTEX, 5),
        Case::new("plain-begingroup", "tex", PLAIN_BEGINGROUP, 7),
        Case::new("plain-undecided", "tex", PLAIN_UNDECIDED, 8),
        Case::new("plain-loop", "tex", PLAIN_LOOP, 8),
        Case::new("plain-parameters", "tex", PLAIN_PARAMETERS, 5),
    ]);
}

// --- LaTeX documents -----------------------------------------------------

const LATEX_MACROS: &str = r"\documentclass{article}
\usepackage{amsmath}
\makeatletter
\def\my@helper{hidden}
\newcommand{\shown}[1][x]{\my@helper-#1}
\makeatother
\newcommand{\unrelated}{U}
\newenvironment{framedtext}[1]{\def\inside{#1}}{}
\begin{document}
Some text \unrelated.
\begin{framedtext}{in}\typeout{SATEX<b|\inside|SATEX}\end{framedtext}
\typeout{SATEX<a|\shown[y]|SATEX}
\end{document}
";

const LATEX_COUNTERS: &str = r"\documentclass{article}
\newcounter{foo}
\newcounter{bar}[foo]
\setcounter{foo}{3}
\newcounter{other}
\begin{document}
\stepcounter{foo}
\addtocounter{bar}{4}
\stepcounter{other}
\typeout{SATEX<a|\thefoo:\thebar:\the\value{foo}|SATEX}
\end{document}
";

const LATEX_REFS: &str = r"\documentclass{article}
\begin{document}
\section{One}
Text.
\section{Two}\label{sec:two}
\subsection{Deep}\label{sec:deep}
\section{Three}
See \ref{sec:two} and \ref{sec:deep} on \pageref{sec:two}.
\end{document}
";

const LATEX_CONDITIONALS: &str = r"\documentclass{article}
\newif\iffinal
\finaltrue
\iffinal
  \newcommand{\state}{final}
\else
  \newcommand{\state}{draft}
\fi
\begin{document}
\typeout{SATEX<a|\state|SATEX}
\end{document}
";

const EXPL3: &str = r"\documentclass{article}
\ExplSyntaxOn
\tl_new:N \l_my_tl
\tl_set:Nn \l_my_tl { expl }
\cs_new:Npn \my_show: { \tl_use:N \l_my_tl - \int_eval:n { 2 * 3 } }
\NewDocumentCommand \showit { } { \my_show: }
\ExplSyntaxOff
\newcommand{\unrelated}{U}
\begin{document}
\typeout{SATEX<a|\showit|SATEX}
\end{document}
";

const TIKZ: &str = r"\documentclass{article}
\usepackage{tikz}
\tikzset{my/.style={red}}
\pgfmathsetmacro{\radius}{2*3}
\newcommand{\unrelated}{U}
\begin{document}
\begin{tikzpicture}\draw[my] (0,0) circle (\radius);\end{tikzpicture}
\typeout{SATEX<a|\radius|SATEX}
\end{document}
";

const LATEX_INPUT: &str = r"\documentclass{article}
\input{part}
\usepackage{mypkg}
\newcommand{\unrelated}{U}
\begin{document}
\typeout{SATEX<a|\frompart:\frompkg|SATEX}
\end{document}
";

const PART: &str = r"\makeatletter
\def\part@helper{inner}
\newcommand{\frompart}{\part@helper}
\makeatother
\newcommand{\alsounrelated}{V}
";

const MYPKG: &str = r"\ProvidesPackage{mypkg}
\newcommand{\frompkg}{pkg}
";

const LATEX_BODY_EXPL: &str = r"\documentclass{article}
\newcommand{\unrelated}{U}
\begin{document}
\ExplSyntaxOn
\tl_const:Nn \c_my_tl { body }
\ExplSyntaxOff
Text \unrelated.
\ExplSyntaxOn
\typeout{SATEX<a|\c_my_tl|SATEX}
\ExplSyntaxOff
\end{document}
";

const LATEX_UNDECIDED: &str = r"\documentclass{article}
\IfFileExists{nowhere-at-all.tex}{%
  \newcommand{\found}{yes}%
}{%
  \newcommand{\found}{no}%
}
\newcommand{\unrelated}{U}
\begin{document}
\typeout{SATEX<a|\found|SATEX}
\end{document}
";

#[test]
fn document_slices_run_like_the_original() {
    run_all(&[
        Case::new("latex-macros", "pdflatex", LATEX_MACROS, 12),
        Case { probe: "b", ..Case::new("latex-environment", "pdflatex", LATEX_MACROS, 11) },
        Case {
            names: &["framedtext"],
            probe: "b",
            ..Case::new("latex-environment-name", "pdflatex", LATEX_MACROS, 11)
        },
        Case::new("latex-counters", "pdflatex", LATEX_COUNTERS, 10),
        Case { text: Some("See"), passes: 2, ..Case::new("latex-refs", "pdflatex", LATEX_REFS, 8) },
        Case::new("latex-conditionals", "pdflatex", LATEX_CONDITIONALS, 10),
        Case::new("expl3", "pdflatex", EXPL3, 10),
        Case::new("tikz", "pdflatex", TIKZ, 8),
        Case::new("lualatex", "lualatex", LATEX_MACROS, 12),
        Case {
            files: &[("part.tex", PART), ("mypkg.sty", MYPKG)],
            ..Case::new("latex-input", "pdflatex", LATEX_INPUT, 6)
        },
        Case::new("latex-body-expl", "pdflatex", LATEX_BODY_EXPL, 9),
        Case::new("latex-undecided", "pdflatex", LATEX_UNDECIDED, 9),
        Case { names: &["\\shown"], probe: "", ..Case::new("latex-name-only", "pdflatex", LATEX_MACROS, 0) },
    ]);
}

// --- packages and classes ------------------------------------------------

const PACKAGE: &str = r"\NeedsTeXFormat{LaTeX2e}
\ProvidesPackage{mine}[2026/01/01 v1.0 test]
\RequirePackage{xcolor}
\newif\ifmine@loud
\DeclareOption{loud}{\mine@loudtrue}
\ProcessOptions\relax
\def\mine@word{word}
\newcommand{\unrelated}{U}
\ifmine@loud
  \newcommand{\mineshout}{\MakeUppercase{\mine@word}}
\else
  \newcommand{\mineshout}{\mine@word}
\fi
\endinput
";

const PACKAGE_DRIVER: &str = r"\documentclass{article}
\usepackage[loud]{mine}
\begin{document}
\typeout{SATEX<a|\meaning\mineshout|SATEX}
\end{document}
";

const CLASS: &str = r"\NeedsTeXFormat{LaTeX2e}
\ProvidesClass{mine}[2026/01/01 v1.0 test]
\LoadClass{article}
\newcommand{\unrelated}{U}
\newcommand*{\@venue}{nowhere}
\newcommand*{\venue}[1]{\renewcommand*{\@venue}{#1}}
\newcommand*{\showvenue}{\@venue}
";

const CLASS_DRIVER: &str = r"\documentclass{mine}
\venue{Here}
\begin{document}
\typeout{SATEX<a|\showvenue|SATEX}
\end{document}
";

#[test]
fn package_and_class_slices_load_like_the_original() {
    run_all(&[
        Case {
            file: "mine.sty",
            driver: Some(PACKAGE_DRIVER),
            names: &["\\mineshout"],
            line: 0,
            ..Case::new("package", "pdflatex", PACKAGE, 0)
        },
        Case {
            file: "mine.cls",
            driver: Some(CLASS_DRIVER),
            names: &["\\showvenue", "\\venue"],
            line: 0,
            ..Case::new("class", "pdflatex", CLASS, 0)
        },
    ]);
}

fn line_of(source: &str, needle: &str) -> u32 {
    source.lines().position(|l| l.contains(needle)).expect("the needle") as u32 + 1
}

// --- standalone -----------------------------------------------------------

/// A one-page PDF `width` by `height` bp, its cross-reference table counted.
fn tiny_pdf(width: u32, height: u32) -> String {
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {width} {height}] >>"),
    ];
    let mut pdf = String::from("%PDF-1.4\n");
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", i + 1));
    }
    let xref = pdf.len();
    pdf.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1));
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1));
    pdf
}

/// A graphic the slice does not ship: its stand-in takes the same room.
const LATEX_GRAPHIC: &str = r"\documentclass{article}
\usepackage{graphicx}
\begin{document}
\sbox0{\includegraphics[width=3cm]{pic}}
\typeout{SATEX<a|\the\wd0 x \the\ht0|SATEX}
\end{document}
";

#[test]
fn a_standalone_slice_compiles_in_an_empty_directory() {
    let pic: &'static str = Box::leak(tiny_pdf(200, 50).into_boxed_str());
    let files: &'static [(&str, &str)] = Box::leak(Box::new([("pic.pdf", pic)]));
    let graphic =
        Case { files, ..Case::new("standalone-graphic", "pdflatex", LATEX_GRAPHIC, line_of(LATEX_GRAPHIC, OPEN)) };
    let directory = scratch("standalone-graphic-text");
    std::fs::write(directory.join("pic.pdf"), pic).unwrap();
    let text = sliced(&graphic, &directory);
    assert!(text.contains("example-image"), "the graphic stays:\n{text}");
    let input = Case {
        files: &[("part.tex", PART), ("mypkg.sty", MYPKG)],
        ..Case::new("standalone-input", "pdflatex", LATEX_INPUT, line_of(LATEX_INPUT, OPEN))
    };
    run_all(&[graphic, input]);
}

/// `slice --out` that slices nothing fails and says why: the playground's
/// Slice shows that reason, not a bare "failed".
#[test]
fn an_empty_slice_with_out_fails_with_its_reason() {
    let directory = scratch("empty-out");
    std::fs::write(directory.join("doc.tex"), "\\relax\n").unwrap();
    let output = crate::support::satex()
        // a line past the end: nothing the run reached
        .args([
            "slice",
            "--at",
            "40:1",
            "--out",
            "slice",
            "-f",
            "doc.tex",
            "--set",
            "engine=tex",
            "--set",
            "kernel=plain",
        ])
        .current_dir(&directory)
        .output()
        .expect("satex runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "an empty slice succeeded: {stderr}");
    assert!(stderr.contains("names nothing the run reached"), "no reason given: {stderr}");
    assert!(!directory.join("slice").exists(), "an empty slice wrote a directory");
}

// --- the oracle -----------------------------------------------------------

/// The playground's examples, sliced at a few lines each: every slice has
/// to typeset its line exactly as the original, pixel for pixel.
#[test]
fn playground_slices_typeset_their_line_like_the_original() {
    if satex::project::paths::which("synctex").is_err() || engine("pdflatex").is_none() {
        return;
    }
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("doc/site/examples");
    let picks: [(&str, &str, &[&str]); 3] = [
        (
            "thesis.tex",
            "chapter.tex",
            &["Figure~\\ref{fig:flow} draws", "\\node (satex)", "Literate programming", "\\caption{"],
        ),
        ("slides.tex", "slides.tex", &["\\item<2->", "One \\pause"]),
        ("rerun.tex", "rerun.tex", &["See Section"]),
    ];
    let mut failures = Vec::new();
    for (main, file, needles) in picks {
        let work = scratch(&format!("oracle-{main}"));
        let doc = satex::oracle::slice::Document::prepare(&examples, main, &work).expect("the example builds");
        let source = std::fs::read_to_string(examples.join(file)).unwrap();
        let criteria: Vec<(String, u32)> = needles.iter().map(|n| (file.to_string(), line_of(&source, n))).collect();
        let slices = doc.slice_all(&criteria, true);
        for ((file, line, dir), verdict) in slices.iter().zip(satex::oracle::slice::check_all(&doc, &slices, 4)) {
            if matches!(verdict, satex::oracle::slice::Verdict::Mismatch(_) | satex::oracle::slice::Verdict::Fail(_)) {
                failures.push(format!("{file}:{line}: {verdict:?} ({})", dir.display()));
            }
        }
    }
    assert!(failures.is_empty(), "slices typeset differently:\n{}", failures.join("\n"));
}
