//! The preflight rules: lines, boxes and pictures TeX packs wider than they
//! may be, reported by the amount TeX writes to its log, and quiet wherever
//! a width is not known.

use crate::support::installed;
use satex::analysis::machine::Machine;
use satex::project::config::{Config, Load};
use satex::queries::query::Record;

const CODES: [&str; 3] = ["overfull-line", "overfull-box", "wider-than-line"];

/// The layout findings of an article with `body`.
fn records(name: &str, body: &str) -> Vec<Record> {
    let directory = crate::support::tmp_root().join("satex-layout").join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let document = format!(
        "\\documentclass{{article}}\n\\usepackage{{graphicx}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n"
    );
    let main = directory.join("main.tex");
    std::fs::write(&main, &document).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(&document, Some(&main), &cfg);
    let out = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|r| CODES.contains(&r["code"].as_str().unwrap_or_default()))
        .collect();
    let _ = std::fs::remove_dir_all(&directory);
    out
}

/// `code line: message` of each finding; the body starts on line 4.
fn found(name: &str, body: &str) -> Vec<String> {
    let mut out: Vec<String> = records(name, body)
        .iter()
        .map(|r| format!("{} {}: {}", r["code"].as_str().unwrap(), r["line"], r["message"].as_str().unwrap()))
        .collect();
    out.sort();
    out
}

/// An `\hbox to` whose kern passes its width by `excess` (the pdflatex log's
/// `Overfull \hbox (12.0pt too wide) detected at line 4`); shrinkable glue
/// takes its shrink off. Pins the § 664 arithmetic: natural less shrink.
#[test]
fn an_hbox_too_narrow_for_its_material_is_reported_by_its_excess() {
    if !installed() {
        return;
    }
    let (to, kern, glue, shrink) = (20, 30, 5, 3);
    let excess = kern + glue - shrink - to;
    let body = format!("\\hbox to {to}pt{{\\kern {kern}pt\\hskip {glue}pt minus {shrink}pt}}");
    assert_eq!(
        found("hbox", &body),
        [format!("overfull-box 4: the box closed here is {excess}.0pt too wide for its {to}.0pt width")]
    );
}

/// What TeX does not report stays quiet: infinite shrink (`\hss`, as
/// `\makebox[w]` uses), an excess within `\hfuzz`, and a box that fits.
/// Mutations of the case above, each of which flips TeX's verdict.
#[test]
fn boxes_tex_does_not_report_are_quiet() {
    if !installed() {
        return;
    }
    for (name, body) in [
        ("hss", "\\hbox to 20pt{\\hss\\kern 30pt\\hss}"),
        ("fuzz", "\\hfuzz=11pt \\hbox to 20pt{\\kern 30pt}"),
        ("fits", "\\hbox to 20pt{\\kern 22pt\\hskip 1pt minus 3pt}"),
        ("makebox", "\\noindent\\makebox[\\linewidth]{\\kern 1.5\\linewidth}"),
    ] {
        assert_eq!(found(name, body), Vec::<String>::new(), "{name}");
    }
}

/// A rule wider than the line: the rule itself sticks out by how far it
/// passes `\linewidth`. The overfull line it makes is the same finding, so
/// it is not reported again; a line overfull by a kern still is.
#[test]
fn a_rule_wider_than_the_line_is_reported_once() {
    if !installed() {
        return;
    }
    let rule = found("rule", "\\noindent\\rule{\\dimexpr\\linewidth+12pt}{1pt}\n");
    assert_eq!(rule, ["wider-than-line 4: this is 12.0pt wider than the 345.0pt line and sticks out into the margin"]);
    let kern = found("kern", "\\noindent\\kern\\dimexpr\\linewidth+12pt\\relax\n");
    assert_eq!(kern, ["overfull-line 5: a line of the paragraph ending here is 12.0pt too wide for its 345.0pt line"]);
}

/// Paragraph shapes (pdflatex logs each as noted): a uniform `\parshape`
/// packs to its one length (12.0pt), and so does the line breaker here.
/// `\hangindent` and a `\parshape` of differing lines (5.0pt, 50.0pt in
/// the log) are not followed, so quiet. A final `\hss` goes before the
/// break (§ 816): 5.0pt. One mid-line is infinite shrink, an error TeX
/// repairs (§ 826): quiet.
#[test]
fn shaped_paragraphs_report_only_what_is_followed() {
    if !installed() {
        return;
    }
    let shaped = [
        ("\\parshape 1 20pt 100pt \\noindent\\kern112pt\\par", Some("12.0pt too wide for its 100.0pt line")),
        ("\\hangindent 30pt \\hangafter 1 \\noindent\\kern 350pt\\par", None),
        ("\\parshape 2 0pt 100pt 0pt 200pt \\noindent\\kern 150pt\\par", None),
        ("\\noindent\\kern 350pt\\hss\\par", Some("5.0pt too wide for its 345.0pt line")),
        ("\\noindent\\kern 350pt\\hss x\\par", None),
    ];
    for (body, want) in shaped {
        let found = found("shape", body);
        match want {
            Some(w) => assert!(found.len() == 1 && found[0].ends_with(w), "{body}: {found:?}"),
            None => assert_eq!(found, Vec::<String>::new(), "{body}"),
        }
    }
}

/// TeX hyphenates a long word at the line's end rather than letting it
/// stick out (its second pass, § 863): pdflatex logs nothing for this
/// paragraph, and satex, hyphenating with the same patterns or not
/// following the word, stays quiet.
#[test]
fn a_word_tex_hyphenates_is_quiet() {
    if !installed() {
        return;
    }
    let body = "Some text with an unbreakable word: Donaudampfschifffahrtsgesellschaftskapitaenswitwenrentenversicherungsanstaltsgebaeude and more text here to fill the line.\n";
    assert_eq!(found("hyphen", body), Vec::<String>::new());
}

/// § 664: a box is overfull only where its finite shrink falls short, so
/// a negative `\hfuzz` reports neither a box of infinite shrink nor an
/// empty one, which pdflatex leaves out of its log too.
#[test]
fn a_negative_hfuzz_reports_no_box_that_fits() {
    if !installed() {
        return;
    }
    let body = "{\\hfuzz=-1pt \\hbox to 20pt{\\hss\\kern 30pt\\hss}\\hbox to -5pt{}}";
    assert_eq!(found("fuzz", body), Vec::<String>::new());
}

/// The overfull facts go through the document cache (postcard) as they are.
#[test]
fn overfull_facts_survive_the_cache_encoding() {
    if !installed() {
        return;
    }
    let directory = crate::support::tmp_root().join("satex-layout").join(format!("cache-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let document = "\\documentclass{article}\n\\begin{document}\n\\hbox to 1pt{\\kern 2pt}\n\\end{document}\n";
    let main = directory.join("main.tex");
    std::fs::write(&main, document).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(document, Some(&main), &cfg);
    let facts = &analysis.facts.overfull;
    assert!(facts.values().any(|o| o.count == 1 && !o.unsure), "{facts:?}");
    let bytes = postcard::to_allocvec(facts).unwrap();
    let back: satex::fixed::HashMap<_, satex::analysis::facts::Overfull> = postcard::from_bytes(&bytes).unwrap();
    assert_eq!(&back, facts);
    let _ = std::fs::remove_dir_all(&directory);
}

/// A width satex cannot know, a random one, or a wide arm of a conditional
/// it cannot decide: no report, never a guess. Pins the soundness gate.
#[test]
fn unknown_widths_are_never_guessed() {
    if !installed() {
        return;
    }
    for (name, body) in [
        ("random", "\\hbox to 20pt{\\kern \\pdfuniformdeviate 100pt}"),
        ("arm", "\\hbox to 20pt{\\ifnum\\pdfuniformdeviate 2=0 \\kern 30pt\\else\\kern 5pt\\fi}"),
        ("line", "\\noindent\\rule{\\dimexpr\\linewidth+\\pdfuniformdeviate 100pt}{1pt}\n"),
    ] {
        assert_eq!(found(name, body), Vec::<String>::new(), "{name}");
    }
}

/// An `\includegraphics` whose `width=` passes the line gets the edit to
/// `width=\linewidth`, over the value alone; other keys get advice only.
#[test]
fn a_too_wide_picture_gets_linewidth() {
    if !installed() {
        return;
    }
    let image = std::process::Command::new("kpsewhich").arg("example-image.pdf").output().unwrap();
    if image.stdout.is_empty() {
        return;
    }
    let width = "1.5\\linewidth";
    let wide = records("picture", &format!("\\noindent\\includegraphics[width={width}]{{example-image}}\n"));
    let pic = wide.iter().find(|r| r["code"] == "wider-than-line").expect("reported");
    let edit = &pic["edits"][0];
    assert_eq!(edit["replacement"], "\\linewidth");
    let start = edit["start"]["col"].as_u64().unwrap();
    let end = edit["end"]["col"].as_u64().unwrap();
    assert_eq!(end - start, width.len() as u64, "{edit}");
    // Keys that change what `width=` means, or that satex does not know, and
    // a comment that could hide a key: advice only.
    for options in ["angle=0,", "xscale=1,", "% height=1cm\n"] {
        let body = format!("\\noindent\\includegraphics[{options}width={width}]{{example-image}}\n");
        let turned = records("turned", &body);
        let pic = turned.iter().find(|r| r["code"] == "wider-than-line").expect("reported");
        assert!(pic.get("edits").is_none(), "{options}: {pic:?}");
    }
}
