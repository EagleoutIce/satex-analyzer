//! Differential testing: satex vs. pdflatex predictions (skipped if no TeX).

use std::path::{Path, PathBuf};
use std::process::Command;

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
    let directory = std::env::temp_dir().join("satex-differential");
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
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

fn satex_verdict(source: &str, name: &str) -> Verdict {
    let directory = scratch();
    let file = directory.join(format!("{name}.tex"));
    std::fs::write(&file, source).expect("writing the document");
    let cfg = crate::support::discover(&directory);
    let analysis = Machine::analyze(source, Some(&file), &cfg);
    satex::lint::lint(&analysis)
        .into_iter()
        .filter(|record| record["origin"] == "document")
        .filter_map(|record| {
            let line = record["line"].as_u64().unwrap_or(0);
            let verdict =
                CLASSES.iter().find(|(_, _, code)| record["code"] == *code).map(|(verdict, _, _)| *verdict)?;
            Some((line, verdict))
        })
        .min_by_key(|(line, _)| *line)
        .map_or(Verdict::Compiles, |(_, verdict)| verdict)
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
        if satex != engine {
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
