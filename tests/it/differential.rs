//! Differential testing: satex vs. pdflatex predictions (skipped if no TeX).

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::support::analyze_latex;
use satex::analysis::machine::Machine;
use satex::project::config::Config;

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
    Ok(satex::linter::lint(&analysis)
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
    let cfg = Config { kernel: Some(satex::plugins::Kernel::None), ..crate::support::config() };
    let analysis = Machine::analyze(&source, None, &cfg);
    let sym = analysis.interner.lookup("r");
    let def = sym.and_then(|sym| analysis.facts.defs.iter().rev().find(|d| d.name == sym));
    match def.and_then(|d| d.mac.as_ref()) {
        Some(mac) => satex::engine::tex::detokenize(&mac.replacement_text, &analysis.interner),
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
    // Paragraph by paragraph, as the challenges page compares them.
    let computed: Vec<(String, bool)> = rows
        .iter()
        .filter(|r| r["kind"] == "paragraph")
        .map(|r| (r["detail"].as_str().unwrap_or("").to_string(), r["open"].as_bool().unwrap_or(false)))
        .collect();
    let engine: Vec<String> = engine.into_iter().map(String::from).collect();
    for (e, range) in satex::queries::query::pair_paragraphs(&computed, &engine) {
        let c = computed[range].iter().map(|c| c.0.as_str()).collect::<Vec<_>>().join(" ");
        let verdict = satex::queries::query::typeset_verdict(&c, &e);
        assert_ne!(verdict, "DIFFERS", "{stem}: pdflatex sets {e:?}, satex computes {c:?} (all: {computed:?})");
    }
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

/// A page that fills past `\\vsize` breaks there in pdfTeX even where satex assumes pages break
/// only where forced: the rows mark the break and count the pages pdflatex makes.
#[test]
fn a_filled_page_is_marked_where_it_breaks() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("typeset-page-fill");
    std::fs::create_dir_all(&dir).unwrap();
    let sentence = "Some text that fills the page with words and more words. ";
    let paragraphs: Vec<String> = (0..15).map(|_| sentence.repeat(4)).collect();
    let body = paragraphs.join("\n\n");
    std::fs::write(
        dir.join("p.tex"),
        format!("\\documentclass{{article}}\n\\begin{{document}}\n\\section{{A}}\n{body}\n\\end{{document}}\n"),
    )
    .unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["query", "typeset", "-f", "p.tex", "--format", "json", "--set", "limits.seconds=0"])
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("json");
    let pages: Vec<&str> = rows.iter().filter(|r| r["kind"] == "page").filter_map(|r| r["detail"].as_str()).collect();
    // pdflatex sets these 15 three-line paragraphs on two pages
    assert_eq!(pages, ["page 1 → 2 by here", "page 2"]);
}

/// The sample paper with `\\drafttrue` and `booktabs`, as pdflatex sets it (pdftotext): `\\repeat`
/// is `\\fi`, so the skipped branch ends early and sets `[1]11` in the preamble, on a page of its
/// own; `\\maketitle` breaks it. The figure's rule stays in the figure, and the table fills page
/// 2, so the conclusion opens page 3.
#[test]
fn the_sample_paper_breaks_its_pages_where_pdflatex_does() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("typeset-sample-pages");
    std::fs::create_dir_all(&dir).unwrap();
    let samples = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples");
    for file in ["preamble.tex", "mypackage.sty", "satex.yaml"] {
        std::fs::copy(samples.join(file), dir.join(file)).unwrap();
    }
    let paper = std::fs::read_to_string(samples.join("paper.tex")).unwrap();
    let paper = paper.replacen("\\input{preamble.tex}", "\\drafttrue\n\\input{preamble.tex}", 1).replacen(
        "\\begin{document}",
        "\\usepackage{booktabs}\n\\begin{document}",
        1,
    );
    std::fs::write(dir.join("paper.tex"), paper).unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["query", "typeset", "-f", "paper.tex", "--format", "json", "--set", "limits.seconds=0"])
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("json");
    // each page record, and each paragraph by its first word
    let seq: Vec<String> = rows
        .iter()
        .filter_map(|r| {
            let detail = r["detail"].as_str()?.replace('·', "");
            match r["kind"].as_str()? {
                "page" => Some(detail),
                "paragraph" => Some(detail.split_whitespace().next()?.to_string()),
                _ => None,
            }
        })
        .collect();
    let expected = [
        "[1]11",
        "page 1 → 2",
        "A",
        "satex",
        "November",
        "1",
        "This",
        "2",
        "We",
        "ℒ=∑i=1N‖xi−xi‖2(1)",
        "▬",
        "Figure",
        "3",
        "Equation",
        "Table",
        "Method",
        "page 2 → 3 by here",
        "4",
        "satex",
        "page 3",
    ];
    assert_eq!(seq, expected, "{rows:?}");
}

/// Rules of unknown height may fill any number of pages: the page count after them has an
/// open upper bound, and the assumption that pages break only where forced is named.
#[test]
fn pages_of_unbounded_height_have_an_open_count() {
    let rule = "\\rule{1pt}{\\pdfuniformdeviate 2000pt}\\par\n";
    let rows = typeset_rows("open-pages", "", &format!("{}\\clearpage\nB", rule.repeat(4)));
    let pages: Vec<&str> = rows.iter().filter(|r| r["kind"] == "page").filter_map(|r| r["detail"].as_str()).collect();
    assert!(pages.iter().any(|p| p.starts_with("page 1–? ")), "{pages:?}");
    assert_eq!(pages.last(), Some(&"page 2–?"), "{pages:?}");
    let assumed = rows.iter().filter(|r| r["kind"] == "assumption" && r["detail"] == "forced-page-breaks").count();
    assert_eq!(assumed, 1, "{rows:?}");
}

/// A loop of an unknown count of tall rules is the same: pdfTeX makes up to hundreds of pages.
#[test]
fn a_loop_of_unknown_length_names_the_forced_breaks_assumption() {
    let body = "\\newcount\\n \\n=\\pdfuniformdeviate 2000 \\relax\n\\loop \\rule{1pt}{100pt}\\par \\advance\\n by -1 \\ifnum\\n>0 \\repeat\n\\clearpage\nB\n\\clearpage\nC";
    let rows = typeset_rows("open-loop", "", body);
    let assumed = rows.iter().any(|r| r["kind"] == "assumption" && r["detail"] == "forced-page-breaks");
    assert!(assumed, "{rows:?}");
    let pages: Vec<&str> = rows.iter().filter(|r| r["kind"] == "page").filter_map(|r| r["detail"].as_str()).collect();
    assert!(!pages.contains(&"page 1 → 2"), "{pages:?}");
}

/// Math atoms a math code names (`\\mathchardef` constants such as `\\alpha` and `\\sum`,
/// `\\delimiter`'s small variant) are typeset with their glyph, inline and displayed.
#[test]
fn math_symbols_are_typeset() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("typeset-math-symbols");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("m.tex"),
        "\\documentclass{article}\n\\begin{document}\nSee $\\alpha\\leq\\langle x\\rangle$.\n\\[\\sum\\limits_{i=1}^{n} \\Gamma\\]\n\\end{document}\n",
    )
    .unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["query", "typeset", "-f", "m.tex", "--format", "json", "--set", "limits.seconds=0"])
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("json");
    let text: String = rows.iter().filter(|r| r["kind"] == "paragraph").filter_map(|r| r["detail"].as_str()).collect();
    for glyph in ['α', '≤', '⟨', '⟩', '∑', 'Γ'] {
        assert!(text.contains(glyph), "{glyph} missing in {text:?}");
    }
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
            .map_or("<none>".into(), |m| satex::engine::tex::detokenize(&m.replacement_text, &analysis.interner))
    };
    // pdflatex 2025: \showbox gives 4, 4 and 2 lines.
    assert_eq!(
        ["rA", "rB", "rC"].map(value),
        ["42.94444pt,1.94444pt,4", "42.83331pt,1.94444pt,4", "18.94444pt,0.0pt,2"]
    );
}

/// A paragraph end only some paths make leaves the paragraph open: a run
/// that did not break there is covered by it taken with the next one.
#[test]
fn an_open_paragraph_pairs_with_what_it_may_go_on_into() {
    use satex::queries::query::{pair_paragraphs, typeset_verdict};
    let paragraphs = |page: &str| -> Vec<(String, bool)> {
        [(format!("The label was on page {page}."), true), ("Here.".into(), true), ("{Here. |}".into(), false)].into()
    };
    let covered = |computed: &[(String, bool)], set: &[&str]| {
        let set: Vec<String> = set.iter().map(|s| s.to_string()).collect();
        pair_paragraphs(computed, &set).into_iter().all(|(e, range)| {
            let c = computed[range].iter().map(|c| c.0.as_str()).collect::<Vec<_>>().join(" ");
            typeset_verdict(&c, &e) != "DIFFERS"
        })
    };
    // `⟨?⟩` may stand for more than one character, so it may swallow a run on.
    for computed in [paragraphs("{1|2}"), paragraphs("{⟨?⟩|2}")] {
        assert!(covered(&computed, &["The label was on page 1.", "Here."]), "{computed:?}");
        assert!(covered(&computed, &["The label was on page 2. Here."]), "{computed:?}");
    }
    let computed = paragraphs("{1|2}");
    // A closed end stays one: nothing goes on past it.
    let closed: Vec<(String, bool)> = computed.iter().map(|(t, _)| (t.clone(), false)).collect();
    let set = vec!["The label was on page 2. Here.".to_string()];
    let pairs = pair_paragraphs(&closed, &set);
    assert!(pairs.iter().all(|(_, r)| r.len() <= 1), "{pairs:?}");
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
    typeset_fixpoint: "11-fixpoint",
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
    let rows = satex::queries::query::run(
        &analysis,
        satex::queries::query::Query::Typeset,
        &satex::queries::query::Filter::Always,
    );
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
    let rows = satex::queries::query::run(
        &analysis,
        satex::queries::query::Query::Typeset,
        &satex::queries::query::Filter::Always,
    );
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
    let rows = satex::queries::query::run(
        &analysis,
        satex::queries::query::Query::Typeset,
        &satex::queries::query::Filter::Always,
    );
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
    // the quick jobs' TeX Live has no tcolorbox
    let found = std::process::Command::new("kpsewhich").arg("tcolorbox.sty").output();
    if !found.is_ok_and(|o| !o.stdout.is_empty()) {
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("defined-together");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("t.tex"),
        "\\documentclass{article}\n\\usepackage{tcolorbox}\n\\newcounter{steps}\n\\newtcolorbox{note}{}\n\\NewDocumentCommand\\pick{m}{#1}\n\\begin{document}\nx\n\\end{document}\n",
    )
    .unwrap();
    let out = crate::support::satex()
        .current_dir(&dir)
        .args(["explain", "\\c@steps", "\\note", "\\pick", "--format", "json", "-f", "t.tex"])
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).expect("json");
    let field = |name: &str, key: &str| -> Vec<String> {
        rows.iter()
            .filter(|r| r["name"] == name)
            .flat_map(|r| r[key].as_array().cloned().unwrap_or_default())
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    };
    let together = |name: &str| field(name, "together");
    assert!(together("\\c@steps").iter().any(|n| n == "\\thesteps"), "{rows:?}");
    assert!(together("\\note").iter().any(|n| n == "\\endnote"), "{rows:?}");
    // the code's own partners (`\pick code`) are folded apart, never shown as names to write
    assert!(together("\\pick").is_empty(), "{rows:?}");
    assert!(!field("\\pick", "together_internal").is_empty(), "{rows:?}");
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

/// A character both arms of an undecided conditional set is no choice:
/// `{Z|}` would say a path may lack it.
#[test]
fn text_both_arms_set_is_no_choice() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("typeset-both-arms");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("j.tex"),
        "\\catcode`\\{=1 \\catcode`\\}=2\n\\def\\z{Z}\\ifodd\\year \\z\\else\\z\\fi Q\\par\n\\end\n",
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
    assert_eq!(text, ["ZQ"]);
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
    typeset_doc(
        tag,
        &format!("\\documentclass{{article}}\n{preamble}\n\\begin{{document}}\n{body}\n\\end{{document}}\n"),
        &[],
    )
}

/// The typeset record of a whole document, with extra `--set` settings.
fn typeset_doc(tag: &str, source: &str, sets: &[&str]) -> Vec<serde_json::Value> {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("typeset-{tag}"));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("t.tex"), source).unwrap();
    let mut args = vec!["query", "typeset", "-f", "t.tex", "--format", "json", "--set", "limits.seconds=0"];
    for set in sets {
        args.extend(["--set", set]);
    }
    let out = crate::support::satex().current_dir(&dir).args(args).output().expect("satex runs");
    serde_json::from_slice(&out.stdout).expect("json")
}

/// The paragraphs of typeset rows, by their text.
fn paragraph<'a>(rows: &'a [serde_json::Value], text: &str) -> &'a serde_json::Value {
    rows.iter()
        .find(|r| r["kind"] == "paragraph" && r["detail"].as_str().is_some_and(|d| d.contains(text)))
        .unwrap_or_else(|| panic!("a paragraph with {text}: {rows:?}"))
}

/// The marks of a row's alignment channel, in order.
fn marks(row: &serde_json::Value) -> Vec<String> {
    row["align"].as_array().into_iter().flatten().filter_map(|m| m["mark"].as_str().map(String::from)).collect()
}

/// A box set into a register inside a box that is placed stays off the page
/// (amsmath measures `(` that way in every math size change): lost with the
/// group that set it, it is no text at all; the placed box's own text is.
#[test]
fn a_box_set_aside_inside_a_placed_box_is_not_placed() {
    if !crate::support::installed() {
        return;
    }
    let rows = typeset_rows("aside", "", "\\mbox{ab\\setbox0=\\hbox{(}cd} ef");
    let row = paragraph(&rows, "ab");
    let marked = row["marked"].as_str().unwrap();
    assert!(!marked.contains('(') && !marked.contains('⟦'), "the measured ( is lost: {row}");
    assert!(marked.contains("abcd"), "the placed text is set: {row}");
    // text set wholly in a box kept unplaced is kept, marked held (a float the run did not follow)
    let body = "x\n\n\\begin{center}\n\\global\\setbox3=\\hbox{kept aside}\n\\end{center}\n\ny";
    let rows = typeset_rows("held", "", body);
    assert_eq!(paragraph(&rows, "kept aside")["held"], true, "{rows:?}");
}

/// An alignment beginning right where an environment begins belongs to it,
/// with its rules, and each cell says where its infinite glue stood: a `c`
/// column fills on both sides of its text, an `l` column after it.
#[test]
fn a_tabular_keeps_its_begin_and_its_column_fills() {
    if !crate::support::installed() {
        return;
    }
    let body = "\\begin{table}\\caption{C}\n\\begin{tabular}{lc}A&B\\\\\\hline x&y\\end{tabular}\\end{table}";
    let rows = typeset_rows("tabular", "", body);
    let table = paragraph(&rows, "A");
    let m = marks(table);
    assert_eq!(m.first().map(String::as_str), Some("begin"), "{table}");
    assert!(table["marked"].as_str().unwrap().contains("―"), "the \\hline rules a row: {table}");
    // the first row: `l` (text, then fill), the cell break, `c` (fill, text, fill)
    let row: Vec<&str> = m.iter().map(String::as_str).skip(1).take_while(|&k| k != "row").collect();
    assert_eq!(row, ["fill", "cell", "fill", "fill"], "{table}");
    assert!(!marks(paragraph(&rows, "Table 1")).contains(&"begin".to_string()));
}

/// A page `\clearpage` ships out stands between the paragraphs it parts, numbered.
#[test]
fn a_shipped_page_is_a_record_between_paragraphs() {
    if !crate::support::installed() {
        return;
    }
    let rows = typeset_rows("pages", "", "One.\\clearpage\nTwo.");
    let kinds: Vec<String> = rows
        .iter()
        .filter(|r| r["kind"] != "assumption")
        .map(|r| format!("{} {}", r["kind"].as_str().unwrap(), r["detail"].as_str().unwrap_or("")))
        .collect();
    assert_eq!(kinds, ["paragraph One.", "page page 1 → 2", "paragraph Two.", "page page 2"]);
}

/// Text set while the driver's literals moved the drawing far off the page
/// (pgf's `\pgfsys@begininvisible`, beamer's covered text) is marked
/// `hide` … `show`: it keeps its place, not its ink.
#[test]
fn text_set_off_the_page_is_hidden() {
    if !crate::support::installed() {
        return;
    }
    let body = "a \\pdfliteral{1 0 0 1 2000 2000 cm}bc\\pdfliteral{1 0 0 1 -2000 -2000 cm} d";
    let rows = typeset_rows("invisible", "", body);
    let row = paragraph(&rows, "bc");
    let at = |k: &str| row["align"].as_array().unwrap().iter().find(|m| m["mark"] == k).and_then(|m| m["at"].as_u64());
    let text = row["detail"].as_str().unwrap();
    assert_eq!(
        (at("hide"), at("show")),
        (text.find("bc").map(|b| b as u64), text.find(" d").map(|b| b as u64)),
        "{row}"
    );
}

/// The rows of a page keep the order their boxes went onto it: a box made
/// after another but placed above it ranks first (a frame's title).
#[test]
fn rows_rank_by_where_their_boxes_were_placed() {
    if !crate::support::installed() {
        return;
    }
    let body = "\\setbox1=\\vbox{body text\\par}\\setbox2=\\vbox{the title\\par}\\box2 \\box1";
    let rows = typeset_rows("placed", "", body);
    let rank = |t: &str| paragraph(&rows, t)["placed"].as_u64().expect("placed");
    assert!(rank("the title") < rank("body text"), "{rows:?}");
}

/// beamer's `\pause`: one page per step, the text after a pause covered
/// (set, hidden) on the steps before it: "One", "One two", "One two three".
#[test]
fn beamer_pause_covers_text_per_step() {
    if !crate::support::installed() {
        return;
    }
    let source = "\\documentclass{beamer}\n\\begin{document}\n\\begin{frame}{Pause}\n  One \\pause two \\pause three\n\\end{frame}\n\\end{document}\n";
    let rows = typeset_doc("pause", source, &["limits.steps=400000000"]);
    let pages = rows.iter().filter(|r| r["kind"] == "page").count();
    assert_eq!(pages, 3, "{rows:?}");
    // each step's visible words: the text outside `hide` … `show`
    let shown: Vec<String> = rows
        .iter()
        .filter(|r| r["kind"] == "paragraph" && r["detail"].as_str().is_some_and(|d| d.contains("One")))
        .map(|r| {
            let text: Vec<char> = r["detail"].as_str().unwrap().chars().collect();
            let mut out = String::new();
            let mut hidden = false;
            let ms = r["align"].as_array().cloned().unwrap_or_default();
            for (k, c) in text.iter().enumerate() {
                for m in ms.iter().filter(|m| m["at"].as_u64() == Some(k as u64)) {
                    hidden = m["mark"] == "hide" || (hidden && m["mark"] != "show");
                }
                if !hidden {
                    out.push(*c);
                }
            }
            out.split_whitespace().collect::<Vec<_>>().join(" ")
        })
        .collect();
    assert_eq!(shown, ["One", "One two", "One two three"]);
}

/// Each typeset character finds its source: a run of source text counts on
/// from its site, a macro's text stands at its call. The PDF marks a
/// finding on only the characters of its range by these.
#[test]
fn a_paragraph_records_the_source_site_of_its_characters() {
    let rows = typeset_rows("sites", "\\def\\name{Knuth}", "Hello \\name{} and you");
    let row = rows.iter().find(|r| r["detail"].as_str().is_some_and(|d| d.contains("Hello"))).expect("paragraph");
    let body = row["line"].as_u64().unwrap();
    let text: Vec<char> = row["detail"].as_str().unwrap().chars().collect();
    let sites = row["sites"].as_array().expect("sites");
    // the site of the character at offset `at`: its entry's column, plus the characters since
    let site = |at: usize| {
        let s = sites.iter().rev().find(|s| s["at"].as_u64().unwrap() as usize <= at).expect("an entry");
        let step = if s["same"] == true { 0 } else { at - s["at"].as_u64().unwrap() as usize };
        (s["line"].as_u64().unwrap(), s["col"].as_u64().unwrap() + step as u64)
    };
    let find = |w: &str| text.iter().collect::<String>().find(w).map(|b| text_offset(&text, b)).unwrap();
    assert_eq!(site(find("Hello")), (body, 1), "{row}");
    // inside the call: the last letter of its name
    let call = "Hello \\name".len() as u64;
    assert!((0..5).all(|k| site(find("Knuth") + k) == (body, call)), "the macro's text stands at its call: {row}");
    let and = "Hello \\name{} and".find("and").unwrap() as u64 + 1;
    assert_eq!(site(find("and")), (body, and), "{row}");
}

/// A picture is drawn from what the run wrote for it: its two node frames,
/// the line between them and the arrow tip pgf drew as a curve, with each
/// node's text in its frame and with `yshift` where TikZ put the node.
#[test]
fn a_picture_is_drawn_from_the_literals_the_run_wrote() {
    if !crate::support::installed() {
        return;
    }
    let body = "\\begin{tikzpicture}\n\\node[draw] (a) {A};\n\\node[draw] (b) at (2,0) [yshift=5mm] {B};\n\\draw[->] (a) -- (b);\n\\end{tikzpicture}";
    let rows = typeset_rows("picture", "\\usepackage{tikz}", body);
    // A picture with text is a paragraph's `sketch`, one without a note's in `sketches`.
    let drawn = |r: &serde_json::Value| {
        r["sketches"].as_array().cloned().unwrap_or_default().into_iter().chain([r["sketch"].clone()])
    };
    let picture = rows.iter().flat_map(drawn).find(|p| p.is_object());
    let picture = picture.unwrap_or_else(|| panic!("a drawn picture: {rows:?}"));
    let paths = picture["paths"].as_array().unwrap();
    assert!(paths.len() >= 4, "two frames, a line and a tip: {picture}");
    assert!(paths.iter().any(|p| p["d"].as_str().unwrap().contains('C')), "the tip is pgf's curve: {picture}");
    let at = |name: &str| {
        let t = picture["texts"].as_array().unwrap().iter().find(|t| t["text"] == name).expect("a text");
        (t["x"].as_f64().unwrap(), t["y"].as_f64().unwrap())
    };
    let ((ax, ay), (bx, by)) = (at("A"), at("B"));
    // 2cm is 56.9pt to the right, 5mm is 14.2pt up, which SVG's y has negative
    assert!((bx - ax - 56.9).abs() < 0.5 && (ay - by - 14.2).abs() < 0.5, "{picture}");
    assert_eq!(picture["approximate"], false);
}

/// The char offset of byte offset `b` in `text`.
fn text_offset(text: &[char], b: usize) -> usize {
    let mut bytes = 0;
    text.iter()
        .take_while(|c| {
            bytes += c.len_utf8();
            bytes <= b
        })
        .count()
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

/// A label whose page flips every run (challenge 11): the build widens its
/// `.aux`, and a run that writes one line fewer keeps that text apart
/// instead of reading one line of unknown text. The text before the
/// flip is still typeset.
#[test]
fn a_page_that_flips_every_run_keeps_the_text_before_it() {
    let rows = typeset_rows(
        "flip",
        "\\usepackage{refcount}",
        "The label was on page \\pageref{x}.\n\\ifnum\\getpagerefnumber{x}=1 \\newpage\\fi\n\\label{x}Here.",
    );
    let texts: Vec<&str> = rows.iter().filter_map(|r| r["detail"].as_str()).collect();
    assert!(texts.iter().any(|t| t.contains("The label was on page")), "{texts:?}");
}

/// `\fbox` draws its rules around a box made before: the text is one framed
/// box, its rules no longer notes of their own; `\rule` keeps its size.
#[test]
fn a_framed_box_and_a_rule_keep_their_layout() {
    if !crate::support::installed() {
        return;
    }
    let rows = typeset_rows("frame", "", "See \\fbox{boxed words} and \\rule{2cm}{4pt} here.");
    let row = paragraph(&rows, "boxed");
    let m = marks(row);
    let at = |k: &str| row["align"].as_array().unwrap().iter().find(|x| x["mark"] == k).and_then(|x| x["at"].as_u64());
    let text = row["detail"].as_str().unwrap();
    assert_eq!(at("frame"), text.find("boxed").map(|b| b as u64), "{row}");
    assert_eq!(at("frameend"), text.find(" and").map(|b| b as u64), "{row}");
    assert!(!text.contains('―') && !text.contains('│'), "the frame's rules are the frame: {row}");
    // 2cm and 4pt as TeX scales them, to the hundredth of a point
    let bar = m.iter().find(|k| k.starts_with("bar ")).expect("a bar's size");
    assert_eq!(bar, "bar 56.91 4.00", "{row}");
}

/// A math script is marked from `^`/`_` to the end of its character or group.
#[test]
fn math_scripts_are_marked() {
    if !crate::support::installed() {
        return;
    }
    let rows = typeset_rows("scripts", "", "$x^2 + y_{ab}$");
    let row = paragraph(&rows, "x");
    let m: Vec<String> = marks(row).into_iter().filter(|k| ["sup", "sub", "scriptend"].contains(&k.as_str())).collect();
    assert_eq!(m, ["sup", "scriptend", "sub", "scriptend"], "{row}");
}

/// An operator's scripts are limits over and under it in display math or after `\\limits`, at its
/// side inline or after `\\nolimits` (tex.web §§ 749, 1159); a script of a script is no limit.
#[test]
fn display_operators_take_limits() {
    if !crate::support::installed() {
        return;
    }
    let body = "Inline $\\sum_{i=1}^{n} a$ and $\\sum\\limits_{j} b$.\n\\[\\sum_{i=1}^{N} x_{\\sum_k} \\quad \\sum\\nolimits_{m} y\\]";
    let rows = typeset_rows("limits", "", body);
    let scripts: Vec<String> = marks(paragraph(&rows, "∑"))
        .into_iter()
        .filter(|k| ["sup", "sub", "over", "under"].contains(&k.as_str()))
        .collect();
    // inline: at the side, but under after `\\limits`; displayed: limits, a script's operator and `\\nolimits` at the side
    assert_eq!(scripts, ["sub", "sup", "under", "under", "over", "sub", "sub", "sub"]);
    let row = paragraph(&rows, "∑");
    let ops = row["marked"].as_str().unwrap_or("").matches("\u{6}L").count();
    assert_eq!(ops, 2, "the operators with limits are tagged: {row}");
}

/// `\\eqno` and `\\leqno` mark where an equation's number begins: a view sets it at the margin.
#[test]
fn equation_numbers_are_marked() {
    if !crate::support::installed() {
        return;
    }
    for (option, mark) in [("", "eqno"), ("[leqno]", "leqno")] {
        let rows = typeset_rows(
            &format!("eqno{mark}"),
            &format!("\\usepackage{option}{{amsmath}}"),
            "\\begin{equation} c \\end{equation}",
        );
        let row = paragraph(&rows, "(1)");
        let at =
            row["align"].as_array().into_iter().flatten().find(|m| m["mark"] == mark).and_then(|m| m["at"].as_u64());
        // the number follows the formula
        assert_eq!(
            at,
            row["detail"].as_str().and_then(|d| d.chars().position(|c| c == '(')).map(|k| k as u64),
            "{mark}: {row}"
        );
    }
}

/// A box set and lost unused (amsmath measuring `(` for its strut, a register put back at a group's
/// end) never reaches the page: no `⟦…⟧` of it. A box kept in a register stays maybe-used.
#[test]
fn boxes_lost_unused_are_not_typeset() {
    if !crate::support::installed() {
        return;
    }
    let body = "A{\\setbox0=\\hbox{lost}}B\\setbox2=\\hbox{gone}\\setbox2=\\hbox{kept}C\n\n\\setbox4=\\hbox{used}{\\setbox4=\\hbox{inner}}\\box4\n\n$\\hat{x}$";
    let rows = typeset_rows("lost-boxes", "\\usepackage{amsmath}", body);
    let text: String = rows.iter().filter(|r| r["kind"] == "paragraph").filter_map(|r| r["detail"].as_str()).collect();
    for lost in ["lost", "gone", "inner", "("] {
        assert!(!text.contains(lost), "{lost} is lost: {text:?}");
    }
    assert!(text.contains("⟦kept⟧") && text.contains("used"), "{text:?}");
    let x = paragraph(&rows, "x");
    assert_eq!(x["detail"].as_str().map(|d| d.matches('x').count()), Some(1), "{x}");
}

/// A key read after the paragraph before ended belongs to the next one: `\\ref` opening a paragraph
/// is not the heading's, a `\\label` after the heading is.
#[test]
fn a_key_opening_a_paragraph_is_its_own() {
    if !crate::support::installed() {
        return;
    }
    let body = "\\section{Results}\n\\label{sec:r}\n\n\\ref{sec:r} defines it.\n\nSee\n\\ref{sec:r}.";
    let rows = typeset_rows("key-par", "", body);
    let keys = |row: &serde_json::Value| -> Vec<String> {
        row["keys"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|k| Some(format!("{}:{}", k["kind"].as_str()?, k["key"].as_str()?)))
            .collect()
    };
    assert_eq!(keys(paragraph(&rows, "Results")), ["label:sec:r"]);
    let opening = paragraph(&rows, "defines");
    assert_eq!(keys(opening), ["ref:sec:r"]);
    assert_eq!(opening["keys"][0]["at"], 0, "{opening}");
    assert_eq!(keys(paragraph(&rows, "See")), ["ref:sec:r"]);
}

/// A character read in the user's file elsewhere than where it is set (`\\title`'s argument, set by
/// `\\maketitle`) names where it was read; text read where it is set names nothing.
#[test]
fn title_text_knows_where_it_was_read() {
    if !crate::support::installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\title{Alpha Beta}\n\\author{Carol}\n\\date{Today}\n\\begin{document}\n\\maketitle\nPlain text.\n\\end{document}\n";
    let rows = typeset_doc("origins", source, &[]);
    for (text, line, col) in [("Alpha", 2, 8), ("Carol", 3, 9), ("Today", 4, 7)] {
        let row = paragraph(&rows, text);
        let at = row["detail"].as_str().and_then(|d| d.find(text)).expect("in detail") as u64;
        let o =
            row["origins"].as_array().into_iter().flatten().rfind(|o| o["at"].as_u64() <= Some(at)).expect("an origin");
        assert_eq!(
            (o["line"].as_u64(), o["col"].as_u64().map(|c| c + at - o["at"].as_u64().unwrap())),
            (Some(line), Some(col)),
            "{text}: {row}"
        );
    }
    assert!(paragraph(&rows, "Plain")["origins"].is_null(), "{rows:?}");
}

/// A paragraph names every environment of the user's it is set in, outermost first, with the lines
/// it begins and ends on: a view marks where each begins and ends.
#[test]
fn paragraphs_name_their_environments() {
    if !crate::support::installed() {
        return;
    }
    let body = "\\begin{quote}\nOuter.\n\\begin{center}\nInner.\n\\end{center}\n\\end{quote}";
    let rows = typeset_rows("env-chain", "", body);
    let chain = |text| -> Vec<(String, u64, u64)> {
        paragraph(&rows, text)["envs"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|e| {
                (
                    e["env"].as_str().unwrap_or("").into(),
                    e["line"].as_u64().unwrap_or(0),
                    e["end"].as_u64().unwrap_or(0),
                )
            })
            .collect()
    };
    assert_eq!(chain("Outer"), [("quote".into(), 4, 9)]);
    assert_eq!(chain("Inner"), [("quote".into(), 4, 9), ("center".into(), 6, 8)]);
}

/// Text an undecided conditional may set names its switch: a `choice` key before its `{`, and the
/// cause with the same node, so a view can show one arm at a time.
#[test]
fn may_text_names_its_switch() {
    if !crate::support::installed() {
        return;
    }
    let body = "\\ifnum\\pdfuniformdeviate 2=0 Heads\\else Tails\\fi.";
    let rows = typeset_rows("switch", "", body);
    let row = paragraph(&rows, "Heads");
    let choice = row["keys"].as_array().into_iter().flatten().find(|k| k["kind"] == "choice").expect("a choice key");
    let cause =
        row["causes"].as_array().into_iter().flatten().find(|c| c["on"].is_string()).expect("a cause with its switch");
    assert_eq!(choice["key"], cause["on"], "{row}");
}
