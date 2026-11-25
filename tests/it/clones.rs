//! Code clones: blocks that repeat up to their values get one finding at the
//! first place, holes listed, and a macro fix that keeps the document whole.

use crate::support::installed;
use satex::analysis::machine::{Analysis, Machine};
use satex::linter::apply;
use satex::project::config::{Clones, Config, Load};
use satex::queries::query::Record;

const FIGURES: &str = "\
\\begin{figure}[ht]\\centering\\includegraphics[width=.8\\linewidth]{example-image-a}\\caption{Alpha results}\\label{fig:a}\\end{figure}
\\begin{figure}[ht]\\centering\\includegraphics[width=.7\\linewidth]{example-image-b}\\caption{Beta results}\\label{fig:b}\\end{figure}
\\begin{figure}[ht]\\centering\\includegraphics[width=.6\\linewidth]{example-image-c}\\caption{Gamma results}\\label{fig:c}\\end{figure}
";

/// A table, and `extra` after its header row.
fn table(extra: &str) -> String {
    format!(
        "\\begin{{table}}[ht]\n\\centering\n\\caption{{Timing}}\n\\begin{{tabular}}{{lrr}}\n\\toprule\nName & Count & Mean \\\\\n\\midrule\n{extra}alpha & 1 & 2 \\\\\n\\bottomrule\n\\end{{tabular}}\n\\end{{table}}\n"
    )
}

struct Project {
    dir: std::path::PathBuf,
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn config(clones: Clones) -> Config {
    let mut cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    cfg.lints.clones = clones;
    cfg
}

fn document(preamble: &str, body: &str) -> String {
    format!(
        "\\documentclass{{article}}\n\\usepackage{{graphicx,booktabs}}\n{preamble}\\begin{{document}}\n{body}\\end{{document}}\n"
    )
}

fn analyze(name: &str, source: &str, cfg: &Config) -> (Project, Analysis) {
    let dir = crate::support::tmp_root().join(format!("satex-clones-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).unwrap();
    let analysis = Machine::analyze(source, Some(&path), cfg);
    (Project { dir }, analysis)
}

fn clones(analysis: &Analysis) -> Vec<Record> {
    satex::linter::lint(analysis).into_iter().filter(|r| r["code"] == "clones").collect()
}

fn found(name: &str, source: &str, cfg: Clones) -> Vec<Record> {
    let (_project, analysis) = analyze(name, source, &config(cfg));
    clones(&analysis)
}

fn message(r: &Record) -> &str {
    r["message"].as_str().unwrap_or_default()
}

fn errors(analysis: &Analysis) -> Vec<String> {
    let mut codes: Vec<String> = satex::linter::lint(analysis)
        .into_iter()
        .filter(|r| r["severity"] == "error")
        .map(|r| r["code"].as_str().unwrap_or_default().to_string())
        .collect();
    codes.sort();
    codes
}

/// Three figures that differ in width, file, caption and label: one finding
/// with four holes, and its fix gives a document with no new errors. (Two
/// such figures save less text than a macro's eight arguments cost.)
#[test]
fn figures_differing_in_values_get_one_macro() {
    if !installed() {
        return;
    }
    let source = document("", FIGURES);
    let cfg = config(Clones::default());
    let (project, analysis) = analyze("figures", &source, &cfg);
    let found = clones(&analysis);
    assert_eq!(found.len(), 1, "{found:?}");
    let finding = &found[0];
    assert_eq!(finding["line"], 4);
    assert_eq!(finding["related"][0]["line"], 5, "{finding:?}");
    let text = message(finding);
    for hole in [
        "4 places",
        "`width` (.8 / .7 / .6)",
        "(example-image-a / example-image-b / example-image-c)",
        "(Alpha results / Beta results / Gamma results)",
        "(fig:a / fig:b / fig:c)",
    ] {
        assert!(text.contains(hole), "{hole} missing: {text}");
    }
    assert_eq!(finding["applicability"], "unsafe");
    let before = errors(&analysis);
    let main = analysis.file_name(analysis.main_file).to_string();
    let keep = |r: &Record| r["code"] == "clones";
    let outcome = apply::fixpoint(&main, &cfg, found, &keep, true);
    apply::write(&outcome).unwrap();
    let fixed = std::fs::read_to_string(&main).unwrap();
    assert!(fixed.contains("\\newcommand{\\figureblock}[4]{\\begin{figure}[ht]"), "{fixed}");
    assert!(fixed.contains("\\figureblock{.8}{example-image-a}{Alpha results}{fig:a}"), "{fixed}");
    assert!(fixed.contains("\\figureblock{.7}{example-image-b}{Beta results}{fig:b}"), "{fixed}");
    assert!(fixed.contains("\\figureblock{.6}{example-image-c}{Gamma results}{fig:c}"), "{fixed}");
    let again = Machine::analyze(&fixed, Some(std::path::Path::new(&main)), &cfg);
    assert_eq!(errors(&again), before, "{fixed}");
    assert!(clones(&again).is_empty(), "{fixed}");
    // The labels are still set: the macro does what the copies did.
    for label in ["fig:a", "fig:b", "fig:c"] {
        assert!(again.facts.occurrences.iter().any(|o| o.key == label), "{label} lost: {fixed}");
    }
    drop(project);
}

#[test]
fn exact_copies_are_one_finding() {
    if !installed() {
        return;
    }
    let block = table("");
    let found = found("exact", &document("", &format!("{block}\n{block}")), Clones::default());
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(message(&found[0]).starts_with("this block repeats unchanged at"), "{found:?}");
}

/// One inserted line keeps 30 of 34 tokens alike: a near copy at 0.85, none at 0.95.
#[test]
fn a_copy_with_an_inserted_line_is_near() {
    if !installed() {
        return;
    }
    let source = document("", &format!("{}\n{}", table(""), table("\\cmidrule{2-3}\n")));
    let near = found("near", &source, Clones::default());
    assert_eq!(near.len(), 1, "{near:?}");
    assert!(message(&near[0]).contains("1 place: `\\midrule` (nothing / \\cmidrule{2-3})"), "{near:?}");
    let strict = found("strict", &source, Clones { similarity: 0.95, ..Clones::default() });
    assert!(strict.is_empty(), "{strict:?}");
}

#[test]
fn short_boilerplate_is_no_clone() {
    if !installed() {
        return;
    }
    let list = "\\begin{itemize}\\item One\\item Two\\end{itemize}\n";
    let found = found("short", &document("", &list.repeat(2)), Clones::default());
    assert!(found.is_empty(), "{found:?}");
}

/// The packages repeat themselves plenty: none of it is the document's.
#[test]
fn library_code_is_never_read() {
    if !installed() {
        return;
    }
    let found = found("library", &document("\\usepackage{amsmath,array}\n", "Text.\n"), Clones::default());
    assert!(found.is_empty(), "{found:?}");
}

/// Ten numbers differ: no macro takes ten parameters, so the copy is no
/// clone to report (it was shown as one, with a fix nobody could apply).
#[test]
fn more_holes_than_parameters_are_no_clone() {
    if !installed() {
        return;
    }
    let tabular = |digits: [u8; 10]| {
        let row = |d: &[u8]| d.iter().map(u8::to_string).collect::<Vec<_>>().join(" & ");
        format!(
            "\\begin{{tabular}}{{ccccc}}\n\\toprule\nA & B & C & D & E \\\\\n\\midrule\n{} \\\\\n{} \\\\\n\\bottomrule\n\\end{{tabular}}\n",
            row(&digits[..5]),
            row(&digits[5..])
        )
    };
    let body = format!("{}\n{}", tabular([1, 2, 3, 4, 5, 6, 7, 8, 9, 0]), tabular([0, 9, 8, 7, 6, 5, 4, 3, 2, 1]));
    let found = found("many", &document("", &body), Clones::default());
    assert!(found.is_empty(), "{found:?}");
}

/// A macro must save text: a long block with one short value in two places
/// does; a short line with three values in two places costs more in
/// arguments than it shares, and is no clone.
#[test]
fn a_clone_must_save_more_than_its_arguments_cost() {
    if !installed() {
        return;
    }
    let low = Clones { min_tokens: 8, ..Clones::default() };
    let long = |v: &str| {
        format!(
            "\\begin{{center}}\n\\fbox{{\\textbf{{Result}} of the \\emph{{first}} experiment: {v}}}\n\\end{{center}}\n"
        )
    };
    let found = self::found("long", &document("", &format!("{}\nText.\n\n{}", long("A"), long("B"))), low.clone());
    assert_eq!(found.len(), 1, "{found:?}");
    let short = |a: &str, b: &str, c: &str| format!("\\textbf{{{a}}} and \\emph{{{b}}} or {c}.\n");
    let found = self::found(
        "short",
        &document("", &format!("{}\nText.\n\n{}", short("a", "b", "c"), short("x", "y", "z"))),
        low,
    );
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn equal_macro_bodies_define_one_through_the_other() {
    if !installed() {
        return;
    }
    let preamble = "\\newcommand\\strong[1]{\\textbf{#1}}\n\\newcommand\\heavy[1]{\\textbf{#1}}\n";
    let found = found("bodies", &document(preamble, "\\strong{a} \\heavy{b}\n"), Clones::default());
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(message(&found[0]).starts_with("`\\heavy` has the same body as `\\strong`"), "{found:?}");
    assert_eq!(found[0]["edits"][0]["replacement"], "\\strong{#1}", "{found:?}");
}

/// A 300-page thesis worth of source: the search stays well under a second.
#[test]
fn a_thesis_sized_source_is_searched_fast() {
    let page = |i: usize| {
        format!(
            "\\section{{Part {i}}}\nSome text about part {i} with a \\cite{{key{i}}} and \\ref{{fig:{i}}} in it, long enough to fill lines of a page.\n\n\
             \\begin{{figure}}[ht]\\centering\\includegraphics[width=.{}\\linewidth]{{img{i}}}\\caption{{Result {i}}}\\label{{fig:{i}}}\\end{{figure}}\n\
             \\begin{{itemize}}\\item one {i}\\item two\\item three\\end{{itemize}}\n{}",
            i % 9 + 1,
            "More prose that differs per page, with numbers like 3.14 and words. ".repeat(30)
        )
    };
    let source: String = (0..300).map(page).collect();
    let started = std::time::Instant::now();
    let groups = satex::linter::clones::groups_in(&source, &Clones::default());
    let took = started.elapsed();
    assert!(!groups.is_empty());
    assert!(took < std::time::Duration::from_secs(1), "{} bytes took {took:?}", source.len());
}

/// Copied figures are worth acting on: the lint reports them, but an editor
/// never offers to turn `clones` off as the project's style.
#[test]
fn clones_are_never_offered_as_style() {
    let (_project, analysis) = analyze("style", &document("", FIGURES), &config(Clones::default()));
    let records = satex::linter::lint(&analysis);
    assert!(records.iter().any(|r| r["code"] == "clones"), "the copies are reported");
    let prompt = satex::project::config::StylePrompt { min_findings: 1, ..Default::default() };
    assert!(satex::linter::style_candidates(&records, &prompt).iter().all(|c| c["code"] != "clones"));
}

/// Intuition: a name the document keeps in one command, typed out by hand
/// elsewhere, is a copy that stays behind when the name changes. Pins the
/// `spelled-out-macro` info for a one-word macro and a glossary entry's name,
/// and silence for the uses that go through the command.
#[test]
fn a_word_a_command_sets_typed_out_is_reported() {
    if !crate::support::installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\usepackage{glossaries}\n\\makenoidxglossaries\n\
        \\newglossaryentry{cellr}{name={cellR},description={a cell}}\n\\newcommand\\model{TinyNet}\n\
        \\makeatletter\\def\\@tempa{beats}\\makeatother\n\\newcommand\\unused{uses}\n\\newcommand\\branch{main}\n\\begin{document}\n\\model{} beats \\gls{cellr} on \\branch.\nTinyNet uses cellR too, the main one.\n\\end{document}\n";
    let analysis = satex::analysis::machine::Machine::analyze(source, None, &crate::support::config());
    let found: Vec<String> = satex::linter::lint(&analysis)
        .iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("spelled-out-macro"))
        .filter_map(|f| f.get("message").and_then(|m| m.as_str()).map(str::to_string))
        .collect();
    assert_eq!(found.len(), 2, "{found:?}");
    // never an everyday word a macro holds (`\\branch` = `main`)
    assert!(!found.iter().any(|m| m.contains("main")), "{found:?}");
    // never an internal scratch macro that happens to hold a word of the text (`\\reserved@a`, `\\@tempa`)
    assert!(!found.iter().any(|m| m.contains('@')), "{found:?}");
    assert!(found.iter().any(|m| m.contains("`\\model{}`")), "{found:?}");
    assert!(found.iter().any(|m| m.contains("`\\gls{cellr}`")), "{found:?}");
    // in a caption that goes to the list of figures (a moving argument) glossaries advises
    // `\\glsentrytext`; with a short title only that goes there and `\\gls` stands
    let caption = |cap: &str| {
        let source = format!(
            "\\documentclass{{article}}\n\\usepackage{{glossaries}}\n\\makenoidxglossaries\n\\newglossaryentry{{cellr}}{{name={{cellR}},description={{a cell}}}}\n\\begin{{document}}\n\\gls{{cellr}}\n\\begin{{figure}}\\caption{cap}\\end{{figure}}\n\\listoffigures\n\\end{{document}}\n"
        );
        let analysis = satex::analysis::machine::Machine::analyze(&source, None, &crate::support::config());
        satex::linter::lint(&analysis)
            .iter()
            .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("spelled-out-macro"))
            .filter_map(|f| f.get("message").and_then(|m| m.as_str()).map(str::to_string))
            .collect::<Vec<_>>()
    };
    let long = caption("{Runs of cellR}");
    assert!(long.len() == 1 && long[0].contains("\\glsentrytext{cellr}"), "{long:?}");
    let short = caption("[Runs]{Runs of cellR}");
    assert!(short.len() == 1 && short[0].contains("`\\gls{cellr}`"), "{short:?}");
}

/// Intuition: the fix of `spelled-out-macro` writes the command in prose,
/// where `_` and `:` are not letters. Pins silence for an expl3-style name
/// the document wrote under `\ExplSyntaxOn` (`\my_word:`): the fix
/// `\my_word:{}` would break the line, the bug was an internal name shown
/// to the author.
#[test]
fn a_word_set_by_a_name_the_prose_cannot_write_is_not_reported() {
    if !crate::support::installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\ExplSyntaxOn\\def\\my_word:{Gamma}\\ExplSyntaxOff\n\\begin{document}\n\
        \\ExplSyntaxOn\\my_word:\\ExplSyntaxOff{} and Gamma again.\n\\end{document}\n";
    let analysis = satex::analysis::machine::Machine::analyze(source, None, &crate::support::config());
    let found: Vec<String> = satex::linter::lint(&analysis)
        .iter()
        .filter(|f| f.get("code").and_then(|c| c.as_str()) == Some("spelled-out-macro"))
        .filter_map(|f| f.get("message").and_then(|m| m.as_str()).map(str::to_string))
        .collect();
    assert!(found.is_empty(), "{found:?}");
}

/// Intuition: a file a script writes, only definitions (`stats.tex`: one number per macro), is data:
/// its unused names are one finding, its look-alike lines and equal values no clones. Pins 1289
/// unused-definition and 334 clone findings on one thesis's generated statistics file.
#[test]
fn a_file_of_definitions_is_one_finding_and_no_clones() {
    if !installed() {
        return;
    }
    let dir = crate::support::scratch_dir(crate::support::tmp_root().join("data-file"));
    let stats: String =
        (0..25).map(|i| format!("\\newcommand{{\\StatRun{}Value}}{{{}}}\n", (b'A' + i as u8) as char, i % 3)).collect();
    std::fs::write(dir.join("stats.tex"), stats).unwrap();
    let main = "\\documentclass{article}\n\\input{stats}\n\\begin{document}\nRuns: \\StatRunAValue.\n\\end{document}\n";
    std::fs::write(dir.join("main.tex"), main).unwrap();
    let analysis = Machine::analyze(main, Some(&dir.join("main.tex")), &crate::support::config());
    let rows = satex::linter::lint(&analysis);
    let unused: Vec<&str> =
        (rows.iter()).filter(|r| r["code"] == "unused-definition").filter_map(|r| r["message"].as_str()).collect();
    assert!(unused.len() == 1 && unused[0].starts_with("24 names stats.tex defines"), "{unused:?}");
    assert!(!rows.iter().any(|r| r["code"] == "clones"), "{rows:?}");
}
