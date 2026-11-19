//! A run resumed from a checkpoint after an edit says exactly what a full
//! run of the edited document says ([`satex::analysis::machine::Checkpoints`]).

use crate::support::installed;
use satex::analysis::machine::{Analysis, Checkpoints, Machine};
use std::path::Path;

fn at(analysis: &Analysis, span: satex::engine::tex::Span) -> String {
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

/// A build's rerun, after the `.aux` changed, resumes on the line that
/// opens the document: the preamble before it read nothing that changed.
#[test]
fn a_rerun_resumes_where_the_document_begins() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join("incremental-rerun");
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.tex");
    let text = "\\documentclass{article}\n\\usepackage{xcolor}\n\\begin{document}\n\\tableofcontents\n\\section{One}\\label{one}\nSee \\ref{one}.\n\\end{document}\n";
    std::fs::write(&main, text).unwrap();
    let cfg = crate::support::config();
    let mut checkpoints = Checkpoints::new(&cfg);
    let analysis = Machine::build(&mut checkpoints, text, &main, &cfg, false);
    assert!(analysis.build_runs > 1, "the build ran once");
    let opens = 1 + text.lines().position(|l| l.starts_with("\\begin{document}")).unwrap() as u32;
    assert_eq!(checkpoints.resumed.map(|(_, line)| line), Some(opens), "{} runs", analysis.build_runs);
    let _ = std::fs::remove_dir_all(&dir);
}

/// A known document is one path however often the editor builds it: the
/// second build reads the files the first left, which hold what it writes.
#[test]
fn a_known_document_built_again_typesets_one_path() {
    if !installed() {
        return;
    }
    let main = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/paper.tex");
    let text = std::fs::read_to_string(&main).unwrap();
    let cfg = crate::support::config();
    let mut checkpoints = Checkpoints::new(&cfg);
    for build in 1..=2 {
        let analysis = Machine::build(&mut checkpoints, &text, &main, &cfg, true);
        let rows = satex::queries::query::run(
            &analysis,
            satex::queries::query::Query::Typeset,
            &satex::queries::query::Filter::Always,
        );
        // A choice is `{a|b}` in the notation's private-use brackets.
        let optional: Vec<&str> = rows
            .iter()
            .filter(|r| r["kind"] == "paragraph")
            .filter_map(|r| r["marked"].as_str())
            .filter(|m| m.contains("\u{E000}{\u{E001}"))
            .collect();
        assert!(optional.is_empty(), "build {build}: {optional:?}");
    }
}

/// An edit inside a body read as one argument (a beamer frame) resumes, the
/// next time, inside the document before that body: the run on the way to
/// the edited line keeps a checkpoint shortly before it, and says what a
/// full run says.
#[test]
fn an_edit_in_a_collected_body_resumes_before_it() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join("incremental-frame");
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.tex");
    let lines = [
        "\\documentclass{article}",
        "\\long\\def\\slide#1\\endslide{#1\\par}",
        "\\begin{document}",
        "\\slide",
        "First slide, line one.",
        "\\endslide",
        "Between the slides.",
        "",
        "\\slide",
        "Second slide, line one.",
        "Second slide, line two.",
        "\\endslide",
        "\\end{document}",
    ];
    let body = 1 + lines.iter().position(|l| l.starts_with("\\begin{document}")).unwrap() as u32;
    let edited = 1 + lines.iter().position(|l| l.starts_with("Second slide, line one")).unwrap();
    let mut text = lines.join("\n") + "\n";
    std::fs::write(&main, &text).unwrap();
    let cfg = crate::support::config();
    // as the editor builds it: the build's `.aux` decides what `\begin{document}` reads
    let mut checkpoints = Checkpoints::new(&cfg).every(64);
    Machine::build(&mut checkpoints, &text, &main, &cfg, true);
    for edit in 1..=2 {
        let mut now = lines.to_vec();
        let line = format!("Second slide, line one {edit}.");
        now[edited - 1] = &line;
        text = now.join("\n") + "\n";
        std::fs::write(&main, &text).unwrap();
        let incremental = Machine::build(&mut checkpoints, &text, &main, &cfg, true);
        let at = checkpoints.resumed.clone();
        let full = Machine::build(&mut Checkpoints::new(&cfg), &text, &main, &cfg, true);
        assert_eq!(first_difference(&said(&incremental), &said(&full)), None, "edit {edit}, resumed at {at:?}");
    }
    let at = checkpoints.resumed.map(|(_, line)| line);
    assert!(at.is_some_and(|l| l > body && l < edited as u32), "resumed at {at:?}, the body opens on line {body}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A package's look-ahead (`\usepackage`'s `\@ifnextchar[`) that holds the
/// `\begin` of the next line still leaves a restart point before the line
/// that opens the document: the build's rerun does not load the package again.
#[test]
fn a_rerun_after_a_package_with_look_ahead_resumes_where_the_document_begins() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join("incremental-rerun-graphicx");
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.tex");
    let text = "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\n\\section{One}\\label{one}\nSee \\ref{one}.\n\\end{document}\n";
    std::fs::write(&main, text).unwrap();
    let cfg = crate::support::config();
    let mut checkpoints = Checkpoints::new(&cfg);
    let analysis = Machine::build(&mut checkpoints, text, &main, &cfg, false);
    assert!(analysis.build_runs > 1, "the build ran once");
    let opens = 1 + text.lines().position(|l| l.starts_with("\\begin{document}")).unwrap() as u32;
    assert_eq!(checkpoints.resumed.map(|(_, line)| line), Some(opens), "{} runs", analysis.build_runs);
    let _ = std::fs::remove_dir_all(&dir);
}

const PARAGRAPHS: &str = r"\documentclass{article}
\begin{document}
\section{One}\label{one}
\def\name{first}
A first paragraph with a few words in it.

A second paragraph that the edits change.\vspace{2cm}

A third paragraph that reads \name\ and refers to \ref{one}.

A fourth paragraph, the last one.
\end{document}
";

/// After an edit, a run that resumes before it checks, at each later line the
/// last run kept a checkpoint on, whether its state is the last run's again.
/// Then the last run's rest is reused, with this run's facts up to there
/// (`Checkpoints::converged`). A word in a paragraph changes no binding, mode
/// or file written, so the states meet on the next line. A changed definition
/// that later text reads, a changed label the `.aux` gets, or a page grown
/// longer keeps them apart. An inserted line moves every later place, so the
/// rest is run again. In every case the result is a full run's.
#[test]
fn a_run_after_an_edit_reuses_the_last_runs_rest_only_when_the_states_meet() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join("incremental-converge");
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.tex");
    let cfg = crate::support::config();
    let line_of = |text: &str, part: &str| 1 + text.lines().position(|l| l.contains(part)).unwrap() as u32;
    // (text replaced, its replacement, whether the rest is reused)
    let edits = [
        ("that the edits change", "that one edit changed", true),
        ("\\def\\name{first}", "\\def\\name{second}", false),
        ("\\label{one}", "\\label{two}", false),
        ("\\vspace{2cm}", "\\vspace{30cm}", false),
        ("that the edits change.", "that the edits change.\nAn inserted line.", false),
    ];
    for (from, to, meets) in edits {
        std::fs::write(&main, PARAGRAPHS).unwrap();
        let mut checkpoints = Checkpoints::new(&cfg).every(64);
        checkpoints.analyze(PARAGRAPHS, &main);
        let text = PARAGRAPHS.replacen(from, to, 1);
        std::fs::write(&main, &text).unwrap();
        // Twice: the second run meets the first's spliced state.
        for run in 1..=2 {
            let text = text.replacen("paragraph, the last", &format!("paragraph {run}, the last"), 1);
            std::fs::write(&main, &text).unwrap();
            let incremental = checkpoints.analyze(&text, &main);
            assert!(checkpoints.resumed.is_some(), "{from}: the run did not resume");
            let full = Machine::analyze_once(&text, Some(&main), &cfg);
            let differs = first_difference(&said(&incremental), &said(&full));
            assert_eq!(differs, None, "after replacing {from} by {to}, run {run}");
        }
        let text = PARAGRAPHS.replacen(from, to, 1);
        std::fs::write(&main, PARAGRAPHS).unwrap();
        let mut checkpoints = Checkpoints::new(&cfg).every(64);
        checkpoints.analyze(PARAGRAPHS, &main);
        std::fs::write(&main, &text).unwrap();
        let incremental = checkpoints.analyze(&text, &main);
        let full = Machine::analyze_once(&text, Some(&main), &cfg);
        assert_eq!(first_difference(&said(&incremental), &said(&full)), None, "after replacing {from} by {to}");
        let at = checkpoints.converged.as_ref().map(|(_, line, _)| *line);
        match meets {
            true => assert!(at.is_none_or(|l| l > line_of(&text, to)), "{from}: met at {at:?}"),
            false => assert_eq!(at, None, "{from}: the rest must run again"),
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A word typed in a paragraph leaves nothing a later line reads, so the run
/// after it reuses the last run's rest from a later line on: `Text`'s offsets
/// into the characters set move by the word's change in length. The line right
/// after the paragraph may not meet yet: the `\par` call open there recorded
/// the edited line's width. Typing on, each run meets the one before at the
/// checkpoint it kept where it met its own predecessor.
#[test]
fn a_word_typed_in_a_paragraph_reuses_the_last_runs_rest() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join("incremental-word");
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.tex");
    let cfg = crate::support::config();
    std::fs::write(&main, PARAGRAPHS).unwrap();
    let mut checkpoints = Checkpoints::new(&cfg).every(64);
    checkpoints.analyze(PARAGRAPHS, &main);
    // Each as long as the text it replaces: see the next test for one that is not.
    for typed in ["one edit changed", "one edit changes", "one edit changed"] {
        let text = PARAGRAPHS.replacen("that the edits change", &format!("that {typed}"), 1);
        std::fs::write(&main, &text).unwrap();
        let incremental = checkpoints.analyze(&text, &main);
        let full = Machine::analyze_once(&text, Some(&main), &cfg);
        assert_eq!(first_difference(&said(&incremental), &said(&full)), None, "typed {typed}");
        let edited = 1 + text.lines().position(|l| l.contains(typed)).unwrap() as u32;
        let at = checkpoints.converged.as_ref().map(|(_, line, _)| *line);
        assert!(at.is_some_and(|l| l > edited), "typed {typed}: met at {at:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A word typed that is shorter or longer than the one it replaces moves every
/// later character's offset, and the counts of tokens taken and assignments
/// made. Whether or not the runs meet (here the page builder's item count
/// differs to the page end, so they do not), the result is the full run's.
#[test]
fn a_longer_word_typed_in_a_paragraph_gives_the_full_runs_result() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join("incremental-longer");
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.tex");
    let cfg = crate::support::config();
    std::fs::write(&main, PARAGRAPHS).unwrap();
    let mut checkpoints = Checkpoints::new(&cfg).every(64);
    checkpoints.analyze(PARAGRAPHS, &main);
    let text = PARAGRAPHS.replacen("that the edits change", "that an edit changed", 1);
    std::fs::write(&main, &text).unwrap();
    let incremental = checkpoints.analyze(&text, &main);
    let full = Machine::analyze_once(&text, Some(&main), &cfg);
    assert_eq!(first_difference(&said(&incremental), &said(&full)), None);
    let _ = std::fs::remove_dir_all(&dir);
}

/// The typeset rows of `path`'s lines `lo..=hi`, without the page order: a paragraph the page
/// builder still holds has none yet.
fn rows_in(analysis: &Analysis, path: &str, lo: u32, hi: u32) -> Vec<satex::queries::query::Record> {
    use satex::queries::query::{Filter, Query, run};
    (run(analysis, Query::Typeset, &Filter::Always).into_iter())
        .filter(|r| r.get("path").and_then(|p| p.as_str()) == Some(path))
        .filter(|r| {
            r.get("line").and_then(|l| l.as_u64()).is_some_and(|l| (u64::from(lo)..=u64::from(hi)).contains(&l))
        })
        .map(|mut r| {
            r.remove("placed");
            r
        })
        .collect()
}

/// An edit inside a paragraph runs that paragraph first, from the checkpoint the full run
/// resumes from, up to the paragraph's end (`RunHooks::preview`): TeX breaks a paragraph into lines
/// from the state at its start and its own text, so its rows are the full run's. Pins: the
/// playground waited for the whole run (and the page builder holds the paragraph until the
/// page ships, so the runs never meet earlier); a longer word and a word in the last
/// paragraph are covered too.
#[test]
fn a_paragraph_run_first_typesets_the_edited_paragraph_as_the_full_run() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join("incremental-preview");
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.tex");
    let cfg = crate::support::config();
    std::fs::write(&main, PARAGRAPHS).unwrap();
    let mut checkpoints = Checkpoints::new(&cfg).every(64);
    checkpoints.analyze(PARAGRAPHS, &main);
    let edits =
        [("the edits change", "an edit changed"), ("the last one", "the very last one"), ("a few words", "few words")];
    let mut text = PARAGRAPHS.to_string();
    for (was, now) in edits {
        text = text.replacen(was, now, 1);
        std::fs::write(&main, &text).unwrap();
        let got = std::rc::Rc::new(std::cell::RefCell::new(None));
        let sink = got.clone();
        checkpoints.hooks.preview = Some(Box::new(move |run, path, lo, hi| {
            *sink.borrow_mut() = Some((rows_in(&run, path, lo, hi), path.to_string(), lo, hi));
        }));
        checkpoints.analyze(&text, &main);
        let (rows, path, lo, hi) = got.take().unwrap_or_else(|| panic!("no paragraph run for {now}"));
        let edited = 1 + text.lines().position(|l| l.contains(now)).unwrap() as u32;
        assert!(lo <= edited && edited <= hi, "{now}: lines {lo}-{hi}");
        assert!(!rows.is_empty(), "{now}: no rows");
        let full = Machine::analyze_once(&text, Some(&main), &cfg);
        assert_eq!(rows, rows_in(&full, &path, lo, hi), "{now}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The paragraph rows of a run, without page rows and places: what the lax page model keeps.
fn paragraph_rows(analysis: &Analysis) -> Vec<satex::queries::query::Record> {
    use satex::queries::query::{Filter, Query, run};
    (run(analysis, Query::Typeset, &Filter::Always).into_iter())
        .filter(|r| r.get("kind").and_then(|k| k.as_str()) != Some("page"))
        .map(|mut r| {
            r.remove("placed");
            r
        })
        .collect()
}

/// After an edit, the run hands on a whole provisional analysis as soon as its state equals
/// the last run's apart from the page being built (`RunHooks::provisional`): the rows after are the last
/// run's, its page breaks marked not exact. Every paragraph row equals a full run's, so the
/// Typeset view shows the whole document long before the exact run ends. Bug pinned: the
/// view waited for the exact run, which the page builder keeps from meeting the last one
/// until the page ships.
#[test]
fn a_lax_page_model_reuses_the_rest_of_the_last_run() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join("incremental-lax");
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.tex");
    let cfg = crate::support::config();
    std::fs::write(&main, PARAGRAPHS).unwrap();
    let mut checkpoints = Checkpoints::new(&cfg).every(64);
    checkpoints.analyze(PARAGRAPHS, &main);
    let mut text = PARAGRAPHS.to_string();
    // A word in a paragraph: the states meet once the page differs only in what it holds.
    // Dropping a word may move an output routine (on a cold cache it does here): no
    // provisional run then may be, the exact one stands alone. None is ever wrong.
    for (was, now, met) in [("the edits change", "an edit changed", Some(true)), ("a few words", "few words", None)] {
        text = text.replacen(was, now, 1);
        std::fs::write(&main, &text).unwrap();
        let got = std::rc::Rc::new(std::cell::RefCell::new(None));
        let sink = got.clone();
        checkpoints.hooks.provisional = Some(Box::new(move |run| *sink.borrow_mut() = Some(run)));
        checkpoints.analyze(&text, &main);
        checkpoints.hooks.provisional = None;
        let lax = got.take();
        if let Some(met) = met {
            assert_eq!(lax.is_some(), met, "{now}: a provisional run");
        }
        if let Some(lax) = lax {
            let full = Machine::analyze_once(&text, Some(&main), &cfg);
            assert_eq!(paragraph_rows(&lax), paragraph_rows(&full), "{now}");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// Every checkpoint copies what grows along the document (graph, facts,
/// sites), so a long document's checkpoints took several times the run's own
/// memory.  `limits.checkpoint_memory` bounds what they copy together, and a
/// rerun from the fewer that stay says what a full run says.
#[test]
fn checkpoints_stay_within_their_memory() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join("incremental-memory");
    std::fs::create_dir_all(&dir).unwrap();
    let body: String = (0..120)
        .map(|i| format!("\\section{{S{i}}}\\label{{s{i}}}\\newcommand\\m{i}{{x{i}}}\\m{i} see \\ref{{s{i}}}.\n\n"))
        .collect();
    let text = format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\\end{{document}}\n");
    let main = dir.join("main.tex");
    std::fs::write(&main, &text).unwrap();
    let mut cfg = crate::support::config();
    cfg.limits.checkpoint_memory = bytesize::ByteSize::b(0);
    let mut unbounded = Checkpoints::new(&cfg).every(64);
    unbounded.analyze(&text, &main);
    let (count, bytes) = unbounded.kept();
    assert!(count >= 2 && bytes > 0, "{count} checkpoints of {bytes} bytes");
    let budget = bytes / 3;
    cfg.limits.checkpoint_memory = bytesize::ByteSize::b(budget as u64);
    let mut bounded = Checkpoints::new(&cfg).every(64);
    bounded.analyze(&text, &main);
    let (kept, held) = bounded.kept();
    assert!(kept >= 1 && kept < count, "{kept} of {count} kept");
    assert!(held <= budget || kept == 1, "{held} bytes held, {budget} allowed");
    let edited = text.replacen("x100", "y100", 1);
    std::fs::write(&main, &edited).unwrap();
    let incremental = bounded.analyze(&edited, &main);
    assert!(bounded.resumed.is_some(), "the rerun resumed");
    let full = Machine::analyze_once(&edited, Some(&main), &cfg);
    assert_eq!(first_difference(&said(&incremental), &said(&full)), None);
}

/// A small deterministic generator, so a failing sequence is replayed by its seed.
struct Lcg(u64);

impl Lcg {
    fn below(&mut self, n: usize) -> usize {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % n.max(1)
    }
}

/// One random edit of `text` as typed in the editor: a word inserted, a character appended,
/// a word deleted, a line doubled or a blank line added. Only body lines change, after `\begin{document}`
/// (or anywhere in a file without one), never the last one, so the document still ends.
fn random_edit(text: &str, rng: &mut Lcg, n: usize) -> String {
    let mut lines: Vec<String> = text.split('\n').map(String::from).collect();
    let body = lines.iter().position(|l| l.starts_with("\\begin{document}")).map_or(0, |i| i + 1);
    let last = lines.iter().rposition(|l| l.starts_with("\\end{document}") || !l.trim().is_empty()).unwrap_or(0);
    let i = body + rng.below(last.saturating_sub(body));
    let line = &mut lines[i];
    match rng.below(5) {
        0 => *line = line.replacen(' ', &format!(" word{n} "), 1),
        1 => line.push('x'),
        2 => {
            let words: Vec<&str> = line.split(' ').collect();
            let plain: Vec<usize> = (0..words.len()).filter(|&w| words[w].chars().all(char::is_alphabetic)).collect();
            if let Some(&w) = plain.get(rng.below(plain.len())) {
                *line =
                    words.iter().enumerate().filter(|&(k, _)| k != w).map(|(_, s)| *s).collect::<Vec<_>>().join(" ");
            }
        }
        3 => {
            let copy = line.clone();
            lines.insert(i, copy);
        }
        _ => lines.insert(i, String::new()),
    }
    lines.join("\n")
}

/// Random edit sequences, each built as the editor builds it (checkpoints kept across edits, the
/// build's files left for the next build), say what a fresh build of the same files says, step
/// for step. The model of the LSP test `typing_a_command_ends_with_the_fresh_run_s_findings`, over
/// every kind of incremental reuse at once: a resume before the edit, a splice of the last run's
/// rest where the states meet, a rerun after the `.aux` changed. Pins any reuse that is not exact.
#[test]
fn random_edit_sequences_build_what_a_fresh_build_does() {
    if !installed() {
        return;
    }
    /// Edits per sequence, and sequences per fixture (one seed each).
    const EDITS: usize = 6;
    const SEEDS: u64 = 2;
    let dir = crate::support::tmp_root().join("incremental-random");
    let fixtures: [&[(&str, &str)]; 2] =
        [&[("main.tex", MAIN), ("chapter.tex", CHAPTER), ("appendix.tex", APPENDIX)], &[("main.tex", PARAGRAPHS)]];
    let cfg = crate::support::config();
    for (k, files) in fixtures.iter().enumerate() {
        for seed in 0..SEEDS {
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            for (name, text) in files.iter() {
                std::fs::write(dir.join(name), text).unwrap();
            }
            let main = dir.join("main.tex");
            let read = |p: &Path| std::fs::read_to_string(p).unwrap();
            let mut checkpoints = Checkpoints::new(&cfg).every(64);
            Machine::build(&mut checkpoints, &read(&main), &main, &cfg, true);
            let mut rng = Lcg(seed * 7919 + k as u64);
            for n in 1..=EDITS {
                let (name, _) = files[rng.below(files.len())];
                let path = dir.join(name);
                let edited = random_edit(&read(&path), &mut rng, n);
                std::fs::write(&path, &edited).unwrap();
                let incremental = Machine::build(&mut checkpoints, &read(&main), &main, &cfg, true);
                let at = (checkpoints.resumed.clone(), checkpoints.converged.clone());
                let full = Machine::build(&mut Checkpoints::new(&cfg), &read(&main), &main, &cfg, true);
                let differs = first_difference(&said(&incremental), &said(&full));
                assert_eq!(
                    differs, None,
                    "fixture {k}, seed {seed}, edit {n} in {name} (resumed, met: {at:?}):\n{edited}"
                );
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// A run after an edit keeps a checkpoint at the first line past each of the next pages it
/// ships (`AFTER_EDIT`): the page holding the edited paragraph is where the states meet again,
/// the page builder holds that paragraph until then (a chapter's `\clearpage` here). So the
/// next edit there meets this run one page on and splices the rest, a small share of the
/// document's steps, with the full run's result. Pins: the next stale checkpoint was an eighth of the step spacing away, and
/// on a long thesis (few checkpoints within the memory budget) a typed word ran the rest of the
/// document again.
#[test]
fn a_second_edit_meets_the_last_run_a_page_on() {
    if !installed() {
        return;
    }
    /// The edit's run takes at most this share of a full run's steps.
    const SHARE: u64 = 4;
    let dir = crate::support::tmp_root().join("incremental-page-on");
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.tex");
    let words = "Some words that fill a paragraph of text across a few lines on the page here";
    /// Paragraphs per chapter, each chapter ends its page.
    const CHAPTER: usize = 10;
    let body: String = (0..60)
        .map(|i| {
            let end = if (i + 1) % CHAPTER == 0 { "\\clearpage\n" } else { "" };
            format!("Paragraph {i}. {words}, {words}, {words}.\n{end}\n")
        })
        .collect();
    let text = |typed: &str| {
        format!(
            "\\documentclass{{article}}\n\\begin{{document}}\n{}\\end{{document}}\n",
            body.replacen("Paragraph 3.", typed, 1)
        )
    };
    std::fs::write(&main, text("Paragraph 3.")).unwrap();
    let cfg = crate::support::config();
    let mut checkpoints = Checkpoints::new(&cfg);
    checkpoints.analyze(&text("Paragraph 3."), &main);
    // As long as the text replaced: the line breaks stay.
    for typed in ["Paragraph 3!", "Paragraph 3?"] {
        std::fs::write(&main, text(typed)).unwrap();
        let incremental = checkpoints.analyze(&text(typed), &main);
        let full = Machine::analyze_once(&text(typed), Some(&main), &cfg);
        assert_eq!(first_difference(&said(&incremental), &said(&full)), None, "typed {typed}");
        let (ran, all) = checkpoints.ran;
        if typed.ends_with('?') {
            assert!(checkpoints.converged.is_some(), "typed {typed}: the runs never met");
            assert!(ran * SHARE <= all, "typed {typed}: ran {ran} of {all} steps");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// An edit in a file read by `\include` resumes where that file is included, or inside it, with
/// the full run's result. `\include` tests the file (`\IfFileExists` reads its size) before it
/// reads it, so no checkpoint inside it holds after an edit there. Red (tests/known-red.txt): a
/// thesis of included chapters runs from `\begin{document}` again for every typed word.
#[test]
fn an_edit_in_an_included_file_resumes_where_it_is_included() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join("incremental-include");
    std::fs::create_dir_all(&dir).unwrap();
    let main = dir.join("main.tex");
    let chapter = dir.join("chapter.tex");
    let main_text =
        "\\documentclass{article}\n\\begin{document}\nBefore.\n\\include{chapter}\nAfter.\n\\end{document}\n";
    std::fs::write(&main, main_text).unwrap();
    let paragraphs: Vec<String> = (0..30).map(|i| format!("Paragraph {i} of the chapter.\n")).collect();
    std::fs::write(&chapter, paragraphs.join("\n")).unwrap();
    let cfg = crate::support::config();
    let mut checkpoints = Checkpoints::new(&cfg).every(64);
    Machine::build(&mut checkpoints, main_text, &main, &cfg, true);
    // The first edit's run keeps checkpoints on the way to the edited line, the second resumes from one.
    for edit in ["edited", "edited again"] {
        let edited = paragraphs.join("\n").replacen("Paragraph 20 of", &format!("Paragraph 20, {edit}, of"), 1);
        std::fs::write(&chapter, &edited).unwrap();
        let incremental = Machine::build(&mut checkpoints, main_text, &main, &cfg, true);
        let at = checkpoints.resumed.clone();
        let full = Machine::build(&mut Checkpoints::new(&cfg), main_text, &main, &cfg, true);
        assert_eq!(first_difference(&said(&incremental), &said(&full)), None, "{edit}: resumed at {at:?}");
        let includes = 1 + main_text.lines().position(|l| l.starts_with("\\include")).unwrap() as u32;
        let near = |(path, line): &(String, u32)| path.ends_with("chapter.tex") || *line >= includes;
        if edit != "edited" {
            assert!(
                at.as_ref().is_some_and(near),
                "{edit}: resumed at {at:?}, the chapter is included on line {includes}"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}
