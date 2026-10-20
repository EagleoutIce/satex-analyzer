//! A run resumed from a checkpoint after an edit says exactly what a full
//! run of the edited document says ([`satex::machine::Checkpoints`]).

use crate::support::installed;
use satex::machine::{Analysis, Checkpoints, Machine};
use std::path::Path;

fn at(analysis: &Analysis, span: satex::tex::Span) -> String {
    format!("{}:{}:{}", analysis.file_name(span.file), span.line, span.col)
}

/// Everything compared: the diagnostics, the facts in the order they were
/// recorded, what was typeset, and how many steps it took.
fn said(analysis: &Analysis) -> Vec<String> {
    let name = |sym| analysis.interner.name(sym).to_string();
    let diagnostics = analysis
        .facts
        .diagnostics
        .iter()
        .map(|d| format!("{:?}|{}|{}|{}|{}", d.severity, d.code, at(analysis, d.span), d.count, d.message));
    let defs = analysis
        .facts
        .defs
        .iter()
        .map(|d| format!("def|{}|{}|{}|{}|{}", name(d.name), name(d.by), d.tag, at(analysis, d.span), d.certain));
    let expansions = analysis
        .facts
        .expansions
        .iter()
        .map(|e| format!("use|{}|{}|{}|{:?}", name(e.name), at(analysis, e.span), e.count, e.meaning));
    let occurrences =
        analysis.facts.occurrences.iter().map(|o| format!("occ|{:?}|{}|{}", o.kind, o.key, at(analysis, o.span)));
    let summary = [format!("steps|{}", analysis.steps), format!("typeset|{}", analysis.typeset)];
    diagnostics.chain(defs).chain(expansions).chain(occurrences).chain(summary).collect()
}

/// The first place `a` and `b` differ, if they do.
fn first_difference(a: &[String], b: &[String]) -> Option<(usize, Option<String>, Option<String>)> {
    (0..a.len().max(b.len())).find(|&i| a.get(i) != b.get(i)).map(|i| (i, a.get(i).cloned(), b.get(i).cloned()))
}

const MAIN: &str = r"\documentclass{article}
\newcommand\greet[1]{Hello, #1!}
\newcounter{things}
\begin{document}
\section{One}
\greet{world} \stepcounter{things}
Some text -- with a dash.

\input{chapter}

\section{Three}
\ifnum\value{things}>1 many\else few\fi
\begingroup \def\inner{x}\inner \endgroup
\greet{again} \thethings

% Undecided: the run goes on as two paths to the `\fi`.
\ifodd\pdfuniformdeviate 7
Odd branch text.
\def\branch{odd}
\else
Even branch text.
\def\branch{even}
\fi
\branch\ and on.

\section{Four}
Done \undefinedcommand.
\include{appendix}
\end{document}
";

const APPENDIX: &str = r"\section{Appendix}
\greet{appendix}

An appendix line.
Its last line.
";

const CHAPTER: &str = r"\section{Two}
\stepcounter{things}
\renewcommand\greet[1]{Hi, #1.}
\greet{chapter}
More text.

Last line of the chapter.
";

/// Each edit: the file, the text replaced, and its replacement.
const EDITS: &[(&str, &str, &str)] = &[
    // Late in the document: resumes past the input.
    ("main.tex", "Done \\undefinedcommand.", "Done \\undefinedcommand!"),
    // A definition late on that changes what follows.
    ("main.tex", "\\section{Four}", "\\section{Four}\\def\\undefinedcommand{y}"),
    // In the input file.
    ("chapter.tex", "Last line of the chapter.", "Last line of the chapter, \\stepcounter{things}."),
    ("chapter.tex", "{Hi, #1.}", "{Hey, #1.}"),
    // In an included file, which `\include` goes on after.
    ("appendix.tex", "Its last line.", "Its last line, \\greet{edited}."),
    // In the preamble.
    ("main.tex", "{Hello, #1!}", "{Hello there, #1!}"),
    // Where two paths run.
    ("main.tex", "Even branch text.", "Even branch, edited."),
    ("main.tex", "\\def\\branch{odd}", "\\def\\branch{odd}\\def\\other{}"),
    // Back to late, after all of the above.
    ("main.tex", "\\section{Three}", "\\section{Three and a half}"),
];

#[test]
fn a_resumed_run_says_what_a_full_run_says() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join("incremental");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("main.tex"), MAIN).unwrap();
    std::fs::write(dir.join("chapter.tex"), CHAPTER).unwrap();
    std::fs::write(dir.join("appendix.tex"), APPENDIX).unwrap();
    let main = dir.join("main.tex");
    let cfg = crate::support::config();
    let mut checkpoints = Checkpoints::new(&cfg).every(64);
    let read = |p: &Path| std::fs::read_to_string(p).unwrap();
    let first = checkpoints.analyze(&read(&main), &main);
    let full = Machine::analyze_once(&read(&main), Some(&main), &cfg);
    assert_eq!(first_difference(&said(&first), &said(&full)), None, "the first run");
    let mut resumed = 0;
    for (file, from, to) in EDITS {
        let path = dir.join(file);
        let text = read(&path);
        assert!(text.contains(from), "{from} in {file}");
        std::fs::write(&path, text.replacen(from, to, 1)).unwrap();
        let incremental = checkpoints.analyze(&read(&main), &main);
        resumed += usize::from(checkpoints.resumed.is_some());
        let full = Machine::analyze_once(&read(&main), Some(&main), &cfg);
        let at = checkpoints.resumed.clone();
        let differs = first_difference(&said(&incremental), &said(&full));
        assert_eq!(differs, None, "after replacing {from} by {to} in {file}, resumed at {at:?}");
    }
    assert!(resumed >= 5, "only {resumed} runs resumed from a checkpoint");
}
