//! Differential testing: satex vs. pdflatex predictions (skipped if no TeX).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::support::analyze_latex;
use satex::config::Config;
use satex::machine::Machine;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Verdict {
    Compiles,
    UndefinedControlSequence,
    AlreadyDefined,
    NotDefined,
    UndefinedEnvironment,
    FileNotFound,
}

const CLASSES: [(Verdict, &str, &str); 5] = [
    (Verdict::UndefinedControlSequence, "! Undefined control sequence", "undefined-control-sequence"),
    (Verdict::AlreadyDefined, "already defined", "already-defined"),
    (Verdict::UndefinedEnvironment, "! LaTeX Error: Environment", "undefined-environment"),
    (Verdict::NotDefined, " undefined.", "not-defined"),
    (Verdict::FileNotFound, "! LaTeX Error: File `", "unresolved-file"),
];

fn engine() -> Option<PathBuf> {
    let which = Command::new("kpsewhich").arg("--var-value=SELFAUTOLOC").output().ok()?;
    let directory = PathBuf::from(String::from_utf8(which.stdout).ok()?.trim());
    let binary = directory.join("pdflatex");
    binary.is_file().then_some(binary)
}

fn scratch() -> PathBuf {
    let directory = crate::support::tmp_root().join(format!("satex-differential-{}", std::process::id()));
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    // Absolute: the engine runs inside it, so a relative TMPDIR would point elsewhere.
    directory.canonicalize().expect("a scratch directory")
}

fn engine_verdict(binary: &Path, source: &str, name: &str) -> Verdict {
    let directory = scratch();
    let file = directory.join(format!("{name}.tex"));
    std::fs::write(&file, source).expect("writing the document");
    let output = Command::new(binary)
        .current_dir(&directory)
        .args(["-interaction=nonstopmode", "-halt-on-error", "-no-shell-escape"])
        .arg(&file)
        .output()
        .expect("running the engine");
    let log = String::from_utf8_lossy(&output.stdout);
    CLASSES
        .iter()
        .filter_map(|(verdict, marker, _)| log.find(marker).map(|at| (at, *verdict)))
        .min_by_key(|(at, _)| *at)
        .map_or(Verdict::Compiles, |(_, verdict)| verdict)
}

/// `Err` names the limits that cut the run: a cut run predicts nothing.
fn satex_verdict(source: &str, name: &str) -> Result<Verdict, String> {
    let directory = scratch();
    let file = directory.join(format!("{name}.tex"));
    std::fs::write(&file, source).expect("writing the document");
    let cfg = crate::support::discover(&directory);
    let analysis = Machine::analyze(source, Some(&file), &cfg);
    if !analysis.limits.is_empty() {
        let hits: Vec<String> = analysis.limits.iter().map(|h| format!("limits.{} x{}", h.key, h.count)).collect();
        return Err(hits.join(", "));
    }
    Ok(satex::lint::lint(&analysis)
        .into_iter()
        .filter(|record| record["origin"] == "document")
        .filter_map(|record| {
            let line = record["line"].as_u64().unwrap_or(0);
            let verdict =
                CLASSES.iter().find(|(_, _, code)| record["code"] == *code).map(|(verdict, _, _)| *verdict)?;
            Some((line, verdict))
        })
        .min_by_key(|(line, _)| *line)
        .map_or(Verdict::Compiles, |(_, verdict)| verdict))
}

const CASES: [(&str, &str, Verdict); 7] = [
    (
        "plain",
        r"\documentclass{article}
\begin{document}
Hello.
\end{document}
",
        Verdict::Compiles,
    ),
    (
        "defined",
        r"\documentclass{article}
\newcommand{\greet}{hello}
\begin{document}
\greet
\end{document}
",
        Verdict::Compiles,
    ),
    (
        "misspelled",
        r"\documentclass{article}
\newcommand{\greet}{hello}
\begin{document}
\greeet
\end{document}
",
        Verdict::UndefinedControlSequence,
    ),
    (
        "missing-package-command",
        r"\documentclass{article}
\begin{document}
\includegraphics{x}
\end{document}
",
        Verdict::UndefinedControlSequence,
    ),
    (
        "renewed-without-definition",
        r"\documentclass{article}
\renewcommand{\nosuchmacro}{x}
\begin{document}
Hello.
\end{document}
",
        Verdict::NotDefined,
    ),
    (
        "unknown-environment",
        r"\documentclass{article}
\begin{document}
\begin{nosuchenvironment}
x
\end{nosuchenvironment}
\end{document}
",
        Verdict::UndefinedEnvironment,
    ),
    (
        "missing-package",
        r"\documentclass{article}
\usepackage{nosuchpackage}
\begin{document}
Hello.
\end{document}
",
        Verdict::FileNotFound,
    ),
];

#[test]
fn satex_predicts_what_the_engine_does() {
    let Some(binary) = engine() else {
        eprintln!("no pdflatex found; skipping the differential suite");
        return;
    };
    let mut disagreements = Vec::new();
    for (name, source, expected) in CASES {
        let engine = engine_verdict(&binary, source, name);
        assert_eq!(engine, expected, "the case `{name}` does not describe the engine's behavior");
        let satex = satex_verdict(source, name);
        if satex.as_ref() != Ok(&engine) {
            disagreements.push(format!("{name}: satex said {satex:?}, the engine said {engine:?}"));
        }
    }
    assert!(disagreements.is_empty(), "{}", disagreements.join("\n"));
}

/// What a document reads back for a box dimension, the space factor or the
/// tail of the current list (tex.web § 420, § 418, § 424).  Each case is
/// run through `pdftex -ini -etex` and through satex with no format, so
/// both see INITEX's catcodes and parameters.  Each puts its answer in
/// `\r`.
const PRELUDE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7 \\font\\f=cmr10 \\f ";

const TYPESET_CASES: &[(&str, &str)] = &[
    // tex.web § 649 `hpack`: the natural size of the list.
    ("hbox-empty", r"\setbox0=\hbox{}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("hbox-word", r"\setbox0=\hbox{abc}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("hbox-kern", r"\setbox0=\hbox{\kern3pt}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("hbox-negative-kern", r"\setbox0=\hbox{\kern-3pt}\xdef\r{\the\wd0}"),
    ("hbox-space", r"\setbox0=\hbox{a b}\xdef\r{\the\wd0}"),
    // tex.web §§ 755-758: a character script on a character nucleus.
    (
        "math-subscript",
        concat!(
            r"\font\mi=cmmi10 \font\ms=cmmi7 \font\sy=cmsy10 \font\ss=cmsy7 \font\ex=cmex10 \font\rs=cmr7 \textfont0=\f \scriptfont0=\rs ",
            r"\textfont2=\sy \scriptfont2=\ss \scriptscriptfont2=\ss \textfont3=\ex \scriptfont3=\ex \scriptscriptfont3=\ex ",
            r"\textfont1=\mi \scriptfont1=\ms \catcode`\$=3 \catcode`\_=8 \scriptspace=0.5pt ",
            r"\setbox0=\hbox{$A_1$}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"
        ),
    ),
    (
        "math-superscript",
        concat!(
            r"\font\mi=cmmi10 \font\ms=cmmi7 \font\sy=cmsy10 \font\ss=cmsy7 \font\ex=cmex10 \font\rs=cmr7 \textfont0=\f \scriptfont0=\rs ",
            r"\textfont2=\sy \scriptfont2=\ss \scriptscriptfont2=\ss \textfont3=\ex \scriptfont3=\ex \scriptscriptfont3=\ex ",
            r"\textfont1=\mi \scriptfont1=\ms \catcode`\$=3 \scriptspace=0.5pt ",
            r"\setbox0=\hbox{$f^2x$}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"
        ),
    ),
    ("hbox-ligature", r"\setbox0=\hbox{ff}\xdef\r{\the\wd0}"),
    ("hbox-kern-pair", r"\setbox0=\hbox{Vo}\xdef\r{\the\wd0}"),
    ("hbox-nested", r"\setbox0=\hbox{\hbox{a}\hbox{b}}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    // tex.web § 463 `scan_rule_spec`: 0.4pt, the rest running.
    ("hbox-vrule", r"\setbox0=\hbox{\vrule}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("hbox-vrule-spec", r"\setbox0=\hbox{\vrule height 2pt depth 1pt width 5pt}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    // tex.web § 644: `to` and `spread` set the size whatever the list holds.
    ("hbox-to", r"\setbox0=\hbox to 0pt{abc}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("hbox-spread", r"\setbox0=\hbox spread 3pt{}\xdef\r{\the\wd0}"),
    // tex.web § 1113 `append_italic_correction`.
    ("hbox-italic", r"\setbox0=\hbox{f\/}\xdef\r{\the\wd0}"),
    ("hbox-italic-ligature", r"\setbox0=\hbox{ff\/}\xdef\r{\the\wd0}"),
    ("hbox-italic-after-kern", r"\setbox0=\hbox{a\kern1pt\/}\xdef\r{\the\wd0}"),
    ("hbox-italic-after-box", r"\setbox0=\hbox{\hbox{a}\/}\xdef\r{\the\wd0}"),
    // tex.web § 668 `vpack`, § 1086 `\boxmaxdepth` and § 1087 `\vtop`.
    ("vbox-empty", r"\setbox0=\vbox{}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("vbox-kern", r"\setbox0=\vbox{\kern3pt}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("vbox-box", r"\setbox0=\vbox{\hbox{abc}}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("vbox-box-depth", r"\setbox0=\vbox{\hbox{g}}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("vbox-box-kern", r"\setbox0=\vbox{\hbox{a}\kern3pt}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("vbox-kern-box", r"\setbox0=\vbox{\kern3pt\hbox{a}}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("vbox-hrule", r"\setbox0=\vbox{\hrule}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("vbox-spread", r"\setbox0=\vbox spread 3pt{}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("vtop-kern", r"\setbox0=\vtop{\kern3pt}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("vtop-box", r"\setbox0=\vtop{\hbox{abc}}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("vtop-kern-box", r"\setbox0=\vtop{\kern3pt\hbox{a}}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    // tex.web § 653, § 670, § 1073: `\raise` and relatives.
    ("hbox-raise", r"\setbox0=\hbox{\raise2pt\hbox{}}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("hbox-raise-word", r"\setbox0=\hbox{\raise2pt\hbox{abc}}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("hbox-lower-word", r"\setbox0=\hbox{\lower3pt\hbox{a}}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("vbox-moveright", r"\setbox0=\vbox{\moveright3pt\hbox{a}}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("vbox-moveleft", r"\setbox0=\vbox{\moveleft3pt\hbox{a}}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    ("hbox-raise-register", r"\setbox1=\hbox{a}\setbox0=\hbox{\raise2pt\copy1}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
    // tex.web § 1086: `\boxmaxdepth` moves the depth into the height.
    ("vbox-boxmaxdepth", r"\setbox0=\vbox{\boxmaxdepth=\maxdimen\hbox{g}}\xdef\r{\the\wd0}"),
    (
        "vbox-boxmaxdepth-outer",
        "\\boxmaxdepth=1pt \\setbox0=\\vbox{\\hbox{g}}\\xdef\\r{\\the\\wd0,\\the\\ht0,\\the\\dp0}",
    ),
    // tex.web § 1080: `\lastbox` is void when the tail is no box.
    ("lastbox-after-glue", r"\setbox0=\hbox{a\hskip3pt\setbox1=\lastbox\xdef\r{\the\wd1,\the\lastskip}}"),
    // tex.web § 1078: a leader box goes on no list of its own.
    ("hbox-leaders", r"\setbox0=\hbox{a\leaders\hbox{.}\hskip5pt}\xdef\r{\the\wd0}"),
    // tex.web § 420: a void register reads as zero in all three.
    ("void-box", r"\xdef\r{\the\wd3,\the\ht3,\the\dp3}"),
    ("hbox-unhbox-void", r"\setbox0=\hbox{\unhbox1}\xdef\r{\the\wd0}"),
    // tex.web § 424: the tail of the current list.
    ("lastskip-hbox", r"\setbox0=\hbox{a\hskip3pt\xdef\r{\the\lastskip}}"),
    ("lastskip-hbox-empty", r"\setbox0=\hbox{\xdef\r{\the\lastskip}}"),
    ("lastskip-hbox-char", r"\setbox0=\hbox{ab\xdef\r{\the\lastskip}}"),
    ("lastskip-par", "\\noindent a\\hskip3pt\\xdef\\r{\\the\\lastskip}\\par"),
    ("lastskip-par-char", "\\noindent ab\\xdef\\r{\\the\\lastskip}\\par"),
    ("lastskip-par-space", "\\noindent a \\xdef\\r{\\the\\lastskip}\\par"),
    ("lastskip-par-start", "\\noindent\\xdef\\r{\\the\\lastskip}\\par"),
    ("lastkern-par", "\\noindent a\\kern3pt\\xdef\\r{\\the\\lastkern}\\par"),
    ("lastpenalty-par", "\\noindent a\\penalty7 \\xdef\\r{\\the\\lastpenalty}\\par"),
    // tex.web § 1105: `\unskip` takes the tail away.
    ("unskip-par", "\\noindent a\\hskip3pt\\unskip\\xdef\\r{\\the\\lastskip}\\par"),
    ("unskip-hbox", r"\setbox0=\hbox{\kern3pt\hskip5pt\unskip}\xdef\r{\the\wd0}"),
    // tex.web § 418 and § 1034: the space factor.
    ("spacefactor-start", r"\setbox0=\hbox{\xdef\r{\the\spacefactor}}"),
    ("spacefactor-upper", r"\setbox0=\hbox{A\xdef\r{\the\spacefactor}}"),
    ("spacefactor-upper-period", r"\setbox0=\hbox{A.\xdef\r{\the\spacefactor}}"),
    ("spacefactor-after-kern", r"\setbox0=\hbox{A\kern1pt\xdef\r{\the\spacefactor}}"),
    ("spacefactor-after-box", r"\setbox0=\hbox{A\hbox{}\xdef\r{\the\spacefactor}}"),
    ("spacefactor-after-rule", r"\setbox0=\hbox{A\vrule\xdef\r{\the\spacefactor}}"),
    ("spacefactor-par", "\\noindent A\\xdef\\r{\\the\\spacefactor}\\par"),
    ("spacefactor-par-start", "\\noindent\\xdef\\r{\\the\\spacefactor}\\par"),
    ("spacefactor-vmode", r"\xdef\r{\the\spacefactor}"),
    ("vbox-hrule-width", r"\setbox0=\vbox{\hrule width 5pt}\xdef\r{\the\wd0,\the\ht0,\the\dp0}"),
];

/// Cases satex is not meant to follow exactly: it may answer "I do not
/// know", but never a number the engine does not print.
const TYPESET_APPROXIMATE: &[(&str, &str)] = &[
    // tex.web § 679: the interline glue between two boxes is not followed,
    // so only the width of the `\vbox` is.
    ("vbox-two-boxes", r"\setbox0=\vbox{\hbox{a}\hbox{bb}}\xdef\r{\the\wd0}"),
    ("vbox-two-boxes-height", r"\setbox0=\vbox{\hbox{a}\hbox{bb}}\xdef\r{\the\ht0}"),
    // The engine writes the character it has no metrics for off the list;
    // satex widens instead (tex.web § 1034 `char_warning`).
    ("hbox-missing-char", r"\setbox0=\hbox{A\char200}\xdef\r{\the\wd0}"),
    // tex.web § 1119: a `\discretionary`'s third list goes on the enclosing
    // one, which satex does not follow.
    ("hbox-discretionary", r"\setbox0=\hbox{a\discretionary{-}{}{}b}\xdef\r{\the\wd0}"),
    // tex.web § 1123 `\accent`.
    ("hbox-accent", r"\setbox0=\hbox{\accent23 a}\xdef\r{\the\wd0}"),
];

fn engine_dimensions(binary: &Path) -> Vec<String> {
    let directory = scratch();
    let out = directory.join("typeset.out");
    let _ = std::fs::remove_file(&out);
    let mut source = String::from("\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \\catcode`\\^=7\n");
    source.push_str("\\immediate\\openout0=typeset.out\n");
    for (name, case) in TYPESET_CASES.iter().chain(TYPESET_APPROXIMATE) {
        source.push_str(&format!("{{{PRELUDE}{case}\\immediate\\write0{{{name} => \\r}}}}\n"));
    }
    source.push_str("\\end\n");
    let file = directory.join("typeset.tex");
    std::fs::write(&file, &source).expect("writing the probe");
    Command::new(binary)
        .current_dir(&directory)
        .args(["-ini", "-etex", "-interaction=nonstopmode", "typeset.tex"])
        .output()
        .expect("running the engine");
    let written = std::fs::read_to_string(&out).expect("the engine's answers");
    written
        .lines()
        .filter_map(|line| line.split_once(" => ").map(|(name, value)| (name.to_string(), value.to_string())))
        .collect::<std::collections::HashMap<_, _>>()
        .into_iter()
        .collect::<Vec<_>>()
        .into_iter()
        .map(|(name, value)| format!("{name}\u{1}{value}"))
        .collect()
}

fn satex_dimension(case: &str) -> String {
    let source = format!("{PRELUDE}{case}\n");
    let cfg = Config { kernel: Some(satex::plugin::Kernel::None), ..crate::support::config() };
    let analysis = Machine::analyze(&source, None, &cfg);
    let sym = analysis.interner.lookup("r");
    let def = sym.and_then(|sym| analysis.facts.defs.iter().rev().find(|d| d.name == sym));
    match def.and_then(|d| d.mac.as_ref()) {
        Some(mac) => satex::tex::detokenize(&mac.replacement_text, &analysis.interner),
        None => "<none>".into(),
    }
}

/// satex may answer "I do not know", never a number the engine does not
/// print: a wrong `0.0pt` would send `\ifdim\wd0=0pt` down the wrong arm.
#[test]
fn box_dimensions_never_disagree_with_the_engine() {
    let Some(directory) = engine().and_then(|p| p.parent().map(Path::to_path_buf)) else {
        eprintln!("no pdftex found; skipping the typesetting suite");
        return;
    };
    let binary = directory.join("pdftex");
    if !binary.is_file() {
        eprintln!("no pdftex found; skipping the typesetting suite");
        return;
    }
    let truth: std::collections::HashMap<String, String> = engine_dimensions(&binary)
        .into_iter()
        .map(|line| {
            let (name, value) = line.split_once('\u{1}').expect("a name and a value");
            (name.to_string(), value.to_string())
        })
        .collect();
    let mut wrong = Vec::new();
    let mut vague = Vec::new();
    for (name, case) in TYPESET_CASES.iter().chain(TYPESET_APPROXIMATE) {
        let engine = truth.get(*name).unwrap_or_else(|| panic!("the engine answered `{name}`"));
        let satex = satex_dimension(case);
        if satex.contains("unknown") {
            vague.push(*name);
        } else if &satex != engine {
            wrong.push(format!("{name}: satex said `{satex}`, the engine said `{engine}`"));
        }
    }
    assert!(wrong.is_empty(), "satex reported a number the engine does not:\n{}", wrong.join("\n"));
    // Every case in `TYPESET_CASES` is one satex is meant to follow exactly.
    vague.retain(|name| TYPESET_CASES.iter().any(|(c, _)| c == name));
    assert!(vague.is_empty(), "satex could not follow: {vague:?}");
}

/// The text satex computes a challenge typesets equals or covers what one
/// pdflatex run set (kept beside it as `.pdflatex.txt`). `DIFFERS` is
/// unsound.
fn typeset_covers(stem: &str) {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("doc/src/wiki/challenges");
    let engine = std::fs::read_to_string(dir.join(format!("{stem}.pdflatex.txt"))).expect("kept pdflatex text");
    let engine: Vec<&str> = engine.lines().filter(|l| !l.starts_with("! ")).collect();
    // One at a time: racing cold runs on one kernel cache lose the kernel.
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let out = crate::support::satex()
        .current_dir(&dir)
        // No wall clock: a busy runner must not cut the kernel build short
        // and turn the comparison into `⟨?⟩`.
        .args(["query", "typeset", "-f", &format!("{stem}.tex"), "--format", "json", "--set", "limits.seconds=0"])
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("json");
    // A run cut short typesets `⟨?⟩`, which covers anything: sound, but it
    // would let a challenge pass without being followed.
    let cut: Vec<&serde_json::Value> = rows.iter().filter(|r| r.get("cut").is_some()).collect();
    assert!(cut.is_empty(), "{stem}: the run was cut short: {cut:?}");
    let computed: Vec<&str> =
        rows.iter().filter(|r| r["kind"] == "paragraph").filter_map(|r| r["detail"].as_str()).collect();
    let verdict = satex::query::typeset_verdict(&computed.join(" "), &engine.join(" "));
    assert_ne!(verdict, "DIFFERS", "{stem}: pdflatex sets {engine:?}, satex computes {computed:?}");
}

/// A kernel read only partly sets nothing sure: without `\documentclass`
/// the class name would read as text pdflatex never sets.
#[test]
fn a_kernel_cut_short_typesets_unknown_text() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("doc/src/wiki/challenges");
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["query", "typeset", "-f", "05-packages.tex", "--format", "json"])
        .args(["--set", "limits.seconds=0", "--set", "limits.format_steps=20000", "--set", "cache.enabled=false"])
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("json");
    let text: Vec<&str> =
        rows.iter().filter(|r| r["kind"] == "paragraph").filter_map(|r| r["detail"].as_str()).collect();
    assert_eq!(text, ["⟨?⟩"]);
    let cut: Vec<&str> = rows.iter().filter_map(|r| r["cut"].as_str()).collect();
    assert_eq!(cut, ["kernel"]);
}

/// Capability: typesetting feedback. A table's `|` and `\\hline` and a
/// `\\rule` bar are drawn between the text (tex.web § 463). Struts, rules
/// of no width (`\\strut`, array's `\\@arstrut`), are not. Its cells
/// and rows are marked, so a view can draw the grid.
#[test]
fn rules_are_typeset_between_the_text() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("typeset-rules");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("r.tex"),
        "\\documentclass{article}\n\\begin{document}\nA\\strut B\n\n\\begin{tabular}{|l|c|}\\hline a & b \\\\ \\hline\\end{tabular}\n\nRule \\rule{1cm}{2pt} here.\n\\end{document}\n",
    )
    .unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["query", "typeset", "-f", "r.tex", "--format", "json", "--set", "limits.seconds=0"])
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("json");
    let text: Vec<String> = rows
        .iter()
        .filter(|r| r["kind"] == "paragraph")
        .filter_map(|r| r["detail"].as_str())
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect();
    assert_eq!(text, ["AB", "―│ a │ b │―", "Rule ▬ here."]);
    // the table's grid: where it begins, parts its cell and row, and ends
    let marks: Vec<&str> = rows
        .iter()
        .flat_map(|r| r["align"].as_array().into_iter().flatten())
        .filter_map(|m| m["mark"].as_str())
        .filter(|m| matches!(*m, "begin" | "cell" | "row" | "end"))
        .collect();
    assert_eq!(marks, ["begin", "cell", "row", "end"]);
}

/// Capability: typesetting feedback. The hyphenation points satex marks
/// are the ones pdflatex's `\showhyphens` shows (tex.web § 923), from the
/// format's own `\patterns` and `\hyphenation`.
#[test]
fn hyphenation_points_match_showhyphens() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("hyphenation");
    std::fs::create_dir_all(&dir).unwrap();
    let words = "dependencies hyphenation typesetting paragraph analysis Uncertainty associate";
    std::fs::write(
        dir.join("h.tex"),
        format!("\\documentclass{{article}}\n\\begin{{document}}\n\\hsize=1cm\n{words}\n\\end{{document}}\n"),
    )
    .unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["query", "typeset", "-f", "h.tex", "--format", "json", "--set", "limits.seconds=0"])
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("json");
    let text: Vec<&str> =
        rows.iter().filter(|r| r["kind"] == "paragraph").filter_map(|r| r["detail"].as_str()).collect();
    // pdflatex: de-pen-den-cies hy-phen-ation type-set-ting para-graph anal-y-sis Un-cer-tainty as-so-ciate
    assert_eq!(text, ["de·pen·den·cies hy·phen·ation type·set·ting para·graph anal·y·sis Un·cer·tainty as·so·ciate"]);
}

/// Capability: typesetting feedback. Knuth-Plass (tex.web §§ 813-890):
/// a paragraph whose items are all known breaks into the lines pdflatex
/// makes, so `\ht`, `\dp` and `\prevgraf` of its `\vbox` are exact.
#[test]
fn paragraph_lines_match_pdflatex() {
    let par = |t: &str, r: &str| {
        format!(
            "\\setbox0=\\vbox{{\\hsize=3cm {t}\\par\\xdef\\p{{\\the\\prevgraf}}}}\\xdef\\{r}{{\\the\\ht0,\\the\\dp0,\\p}}\n"
        )
    };
    let source = format!(
        "\\documentclass{{article}}\n\\begin{{document}}\n{}{}{}\\end{{document}}\n",
        par("The quick brown fox jumps over the lazy dog and keeps running far away.", "rA"),
        par("Typesetting paragraphs with dependencies requires careful hyphenation of words.", "rB"),
        par("\\centering files, dependencies, more", "rC"),
    );
    let analysis = analyze_latex(&source);
    let value = |name: &str| {
        let sym = analysis.interner.lookup(name);
        let def = sym.and_then(|sym| analysis.facts.defs.iter().rev().find(|d| d.name == sym));
        def.and_then(|d| d.mac.as_ref())
            .map_or("<none>".into(), |m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
    };
    // pdflatex 2025: \showbox gives 4, 4 and 2 lines.
    assert_eq!(
        ["rA", "rB", "rC"].map(value),
        ["42.94444pt,1.94444pt,4", "42.83331pt,1.94444pt,4", "18.94444pt,0.0pt,2"]
    );
}

/// One test per challenge, named by its capability.
macro_rules! challenges {
    ($($test:ident: $stem:literal,)*) => {
        $(#[test]
        fn $test() {
            typeset_covers($stem);
        })*
    };
}

challenges! {
    typeset_catcode: "01-catcode",
    typeset_code_as_data: "02-code-is-data",
    typeset_run_time_arguments: "03-arguments",
    typeset_typesetting_feedback: "04-typesetting",
    typeset_package_interpreters: "05-packages",
    typeset_unknown_inputs: "06-unknown",
    typeset_termination: "07-termination",
    typeset_global_state: "08-global-state",
    typeset_progressive_recursion: "09-progressive",
    typeset_engine: "10-engine",
    typeset_termination_input_stack: "r1-input-stack",
    typeset_termination_growing_key: "r2-growing-key",
    typeset_unknown_inputs_choice: "r3-choice",
    typeset_code_as_data_self_modifying: "r4-self-modifying",
    typeset_unknown_inputs_token_domain: "r5-token-domain",
    typeset_unknown_inputs_relational: "r6-relational",
    typeset_termination_merge_cost: "r7-merge-cost",
    typeset_typesetting_feedback_hyphenation: "s2-rounded-paragraph",
}

/// `\quad` after a section number is space on the page: the computed text
/// says `1 Intro`, as pdftotext reads it.
#[test]
fn a_section_number_and_its_title_are_apart() {
    if !crate::support::installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\section{Intro}\nText.\n\\end{document}\n";
    let analysis = analyze_latex(source);
    let rows = satex::query::run(&analysis, satex::query::Query::Typeset, &satex::query::Filter::Always);
    let texts: Vec<&str> = rows.iter().filter_map(|r| r.get("detail").and_then(|d| d.as_str())).collect();
    assert!(texts.contains(&"1 Intro"), "{texts:?}");
}

/// A slot reads as the character its font's encoding puts there: `\{`
/// sets slot 102 of an OMS font, a brace, while `\char102` in OT1 is `f`.
#[test]
fn a_symbol_reads_as_its_encoding_says() {
    if !crate::support::installed() {
        return;
    }
    let source =
        "\\documentclass{article}\n\\begin{document}\na\\{b\\}c \\char102 \\texttt{\\char92}\n\\end{document}\n";
    let analysis = analyze_latex(source);
    let rows = satex::query::run(&analysis, satex::query::Query::Typeset, &satex::query::Filter::Always);
    let texts: Vec<&str> = rows.iter().filter_map(|r| r.get("detail").and_then(|d| d.as_str())).collect();
    assert!(texts.contains(&"a{b}c f\\"), "{texts:?}");
}

/// The typeset text says where its font changes, by the kernel's defaults:
/// `\textbf` is `\bfdefault`, `\itshape` `\itdefault`, and so on.
#[test]
fn the_typeset_text_knows_its_fonts() {
    if !crate::support::installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\nA \\textbf{B} {\\itshape C} \\texttt{D} \\textsf{\\textsc{E}} F\n\\end{document}\n";
    let analysis = analyze_latex(source);
    let rows = satex::query::run(&analysis, satex::query::Query::Typeset, &satex::query::Filter::Always);
    let row = rows.iter().find(|r| r.get("kind").and_then(|k| k.as_str()) == Some("paragraph")).expect("a paragraph");
    let detail: Vec<char> = row["detail"].as_str().unwrap().chars().collect();
    let fonts = row["fonts"].as_array().expect("fonts");
    let style = |at: usize| {
        fonts
            .iter()
            .take_while(|f| f["at"].as_u64().unwrap() as usize <= at)
            .last()
            .map_or("", |f| f["style"].as_str().unwrap())
    };
    let styled: Vec<(char, &str)> =
        detail.iter().enumerate().filter(|(_, c)| c.is_alphabetic()).map(|(i, &c)| (c, style(i))).collect();
    assert_eq!(styled, [('A', ""), ('B', "bf"), ('C', "it"), ('D', "tt"), ('E', "sf sc"), ('F', "")], "{row:?}");
}

/// Capability: explain. The names one call of the document defines are
/// linked: `\newcounter{steps}` makes `\c@steps` with `\thesteps`, and
/// `\newtcolorbox{note}` makes the environment's `\note` with `\endnote`.
#[test]
fn names_one_call_defines_are_defined_together() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("defined-together");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("t.tex"),
        "\\documentclass{article}\n\\usepackage{tcolorbox}\n\\newcounter{steps}\n\\newtcolorbox{note}{}\n\\begin{document}\nx\n\\end{document}\n",
    )
    .unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["explain", "\\c@steps", "\\note", "--format", "json", "-f", "t.tex"])
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("json");
    let together = |name: &str| -> Vec<String> {
        rows.iter()
            .filter(|r| r["name"] == name)
            .flat_map(|r| r["together"].as_array().cloned().unwrap_or_default())
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    };
    assert!(together("\\c@steps").iter().any(|n| n == "\\thesteps"), "{rows:?}");
    assert!(together("\\note").iter().any(|n| n == "\\endnote"), "{rows:?}");
}

/// Paths that part after a conditional or at a name with several meanings
/// each set their own text: the typeset text holds the alternatives side by
/// side instead of the characters of both interleaved.
#[test]
fn text_of_parted_paths_stays_alternatives() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("typeset-parted");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("j.tex"),
        "\\catcode`\\{=1 \\catcode`\\}=2\n\\def\\t{Ax}\\ifodd\\year \\def\\t{By}\\fi\nZ\\t Q\\par\n\\end\n",
    )
    .unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["query", "typeset", "-f", "j.tex", "--format", "json", "--set", "limits.seconds=0"])
        .args(["--set", "engine=tex", "--set", "kernel=plain", "--set", "clock=unknown"])
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("json");
    let text: Vec<&str> =
        rows.iter().filter(|r| r["kind"] == "paragraph").filter_map(|r| r["detail"].as_str()).collect();
    assert!(matches!(text[..], ["Z{By|Ax}Q"] | ["Z{Ax|By}Q"]), "{text:?}");
}

/// Capability: typesetting feedback. The ligatures the font's program
/// sets (tex.web § 1035) are marked where they stand: what it joined and
/// the glyph, so a view can show the join.
#[test]
fn ligatures_are_marked_where_they_stand() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("typeset-ligatures");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("l.tex"), "\\documentclass{article}\n\\begin{document}\nA fine 10--12.\n\\end{document}\n")
        .unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["query", "typeset", "-f", "l.tex", "--format", "json", "--set", "limits.seconds=0"])
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("json");
    let ligatures: Vec<(String, String)> = rows
        .iter()
        .flat_map(|r| r["ligatures"].as_array().cloned().unwrap_or_default())
        .map(|l| (l["from"].as_str().unwrap_or_default().to_string(), l["to"].as_str().unwrap_or_default().to_string()))
        .collect();
    assert!(ligatures.contains(&("fi".into(), "fi".into())), "{ligatures:?}");
    assert!(ligatures.contains(&("--".into(), "–".into())), "{ligatures:?}");
}

/// At a use, a name paths defined differently holds one of their texts:
/// `explain --at` lists each definition the use read.
#[test]
fn a_use_after_a_join_names_every_definition_it_read() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("explain-one-of");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("j.tex"),
        "\\catcode`\\{=1 \\catcode`\\}=2\n\\def\\t{Ax}\\ifodd\\year \\def\\t{By}\\fi\nZ\\t Q\\par\n\\end\n",
    )
    .unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["explain", "\\t", "--at", "3", "--format", "json", "-f", "j.tex", "--set", "limits.seconds=0"])
        .args(["--set", "engine=tex", "--set", "kernel=plain", "--set", "clock=unknown"])
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("json");
    let mut bodies: Vec<&str> =
        rows[0]["one_of"].as_array().into_iter().flatten().filter_map(|a| a["body"].as_str()).collect();
    bodies.sort_unstable();
    assert_eq!(bodies, ["Ax", "By"], "{rows:?}");
}

/// Capability: typesetting feedback. A table of contents line is its
/// number in a box of the width `\numberline` gives it, the title, leaders
/// and the page number in a box of its own: the marks a view sets it by.
#[test]
fn a_contents_line_keeps_its_boxes_and_leaders() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("typeset-contents");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("c.tex"),
        "\\documentclass{article}\n\\begin{document}\n\\makeatletter\n\\@dottedtocline{1}{1.5em}{2.3em}{\\numberline{1}Intro}{1}\n\\makeatother\n\\end{document}\n",
    )
    .unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["query", "typeset", "-f", "c.tex", "--format", "json", "--set", "limits.seconds=0"])
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("json");
    let marks: Vec<&str> = rows
        .iter()
        .flat_map(|r| r["align"].as_array().into_iter().flatten())
        .filter_map(|m| m["mark"].as_str())
        .collect();
    assert!(
        marks.iter().any(|m| m.starts_with("box ")) && marks.contains(&"boxend") && marks.contains(&"leaders"),
        "{marks:?}"
    );
}

/// A forced line break (`\\`) stands in the paragraph where it is set: the
/// record marks it between the two lines' characters.
#[test]
fn a_forced_line_break_is_marked_in_its_paragraph() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("typeset-break");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("b.tex"), "\\documentclass{article}\n\\begin{document}\nOne\\\\\nTwo\n\\end{document}\n")
        .unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["query", "typeset", "-f", "b.tex", "--format", "json", "--set", "limits.seconds=0"])
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("json");
    let row = rows.iter().find(|r| r["detail"].as_str().is_some_and(|d| d.contains("One"))).expect("the paragraph");
    let breaks: Vec<u64> = row["align"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|m| m["mark"] == "break")
        .filter_map(|m| m["at"].as_u64())
        .collect();
    assert_eq!(breaks, [3], "{row}");
}

/// The typeset record of a document body, run in a scratch directory.
fn typeset_rows(tag: &str, preamble: &str, body: &str) -> Vec<serde_json::Value> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("typeset-{tag}"));
    std::fs::create_dir_all(&dir).unwrap();
    let source = format!("\\documentclass{{article}}\n{preamble}\n\\begin{{document}}\n{body}\n\\end{{document}}\n");
    std::fs::write(dir.join("t.tex"), source).unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["query", "typeset", "-f", "t.tex", "--format", "json", "--set", "limits.seconds=0"])
        .output()
        .expect("satex runs");
    serde_json::from_slice(&out.stdout).expect("json")
}

/// A paragraph over several source lines says where each line's text starts,
/// and the color the run had selected there is recorded, whatever command
/// selected it.
#[test]
fn a_paragraph_records_its_source_lines_and_the_colors_the_run_set() {
    let rows = typeset_rows(
        "lines",
        "\\usepackage{xcolor}\n\\def\\twice#1{#1#1}",
        "\\twice{ab}\n\\textcolor{green}{Hey} {\\color[rgb]{1,0,0}red} plain",
    );
    let row = rows.iter().find(|r| r["detail"].as_str().is_some_and(|d| d.starts_with("abab"))).expect("paragraph");
    let first = row["line"].as_u64().unwrap();
    let (hey, red) = {
        let text = row["detail"].as_str().unwrap();
        (text.find("Hey").unwrap() as u64, text.find("red").unwrap() as u64)
    };
    let lines = row["lines"].as_array().expect("a line change");
    assert_eq!(lines.len(), 1, "{row}");
    assert_eq!((lines[0]["line"].as_u64().unwrap(), lines[0]["at"].as_u64().unwrap()), (first + 1, hey), "{row}");
    let colors: Vec<(&str, u64)> = row["fonts"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|f| Some((f["color"].as_str()?, f["at"].as_u64()?)))
        .collect();
    assert_eq!(colors, [("#00ff00", hey), ("#ff0000", red)], "{row}");
}

/// Bold comes from the font series the run selected, not from the name of
/// the command that selected it: an alias of `\bfseries` and a macro that
/// sets the series itself both make bold text.
#[test]
fn bold_text_is_the_series_the_run_selected_whatever_selected_it() {
    let rows = typeset_rows(
        "bold",
        "\\let\\strong\\bfseries\n\\def\\loud{\\fontseries{b}\\selectfont}",
        "{\\strong aa} {\\loud bb} cc",
    );
    let row = rows.iter().find(|r| r["detail"].as_str().is_some_and(|d| d.starts_with("aa"))).expect("paragraph");
    let bold: Vec<u64> = row["fonts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["style"].as_str().is_some_and(|s| s.split(' ').any(|x| x == "bf")))
        .filter_map(|f| f["at"].as_u64())
        .collect();
    assert_eq!(bold, [0, 3], "{row}");
}

/// A footnote's text is set in an insert: it follows its paragraph as a record of its own, not
/// inside the paragraph's text.
#[test]
fn a_footnote_follows_its_paragraph() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("typeset-footnote");
    std::fs::create_dir_all(&dir).unwrap();
    let source = "\\documentclass{article}\n\\begin{document}\nMain\\footnote{Aside} text.\n\\end{document}\n";
    std::fs::write(dir.join("f.tex"), source).unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["query", "typeset", "-f", "f.tex", "--format", "json", "--set", "limits.seconds=0"])
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("json");
    let detail = |r: &serde_json::Value| r["detail"].as_str().unwrap_or_default().to_string();
    let main = rows.iter().find(|r| detail(r).contains("Main")).expect("the paragraph");
    assert!(!detail(main).contains("Aside"), "{main}");
    let foot = rows.iter().find(|r| r["footnote"] == true).expect("the footnote");
    assert!(detail(foot).contains("Aside"), "{foot}");
}
