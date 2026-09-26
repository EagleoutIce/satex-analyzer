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
    let cfg = Config::discover(&directory);
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
