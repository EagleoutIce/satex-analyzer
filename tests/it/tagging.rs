//! PDF tagging: what `\DocumentMetadata` set up, read from the run (`satex::analysis::tagging`),
//! and the lints that compare it with what a tagged PDF needs.

use satex::analysis::machine::Analysis;
use satex::analysis::tagging::{self, Val};

/// `source` as `main.tex` of a directory of its own, so that fixes have a file to edit.
fn analyze(source: &str) -> Analysis {
    let dir = crate::support::tmp_root().join(format!("tagging-{:x}", fxhash(source)));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.png"), PNG).unwrap();
    let main = dir.join("main.tex");
    std::fs::write(&main, source).unwrap();
    satex::analysis::machine::Machine::analyze(source, Some(&main), &crate::support::config())
}

/// A name per source, so tests running at once keep apart.
fn fxhash(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// The messages of the lint `code` on `analysis`.
fn found(analysis: &Analysis, code: &str) -> Vec<String> {
    satex::linter::lint(analysis)
        .iter()
        .filter(|f| f.get("code").is_some_and(|c| c == code))
        .map(|f| f.get("message").and_then(|m| m.as_str()).unwrap_or_default().to_string())
        .collect()
}

/// The replacement texts of the lint `code`'s fixes.
fn fixes(analysis: &Analysis, code: &str) -> Vec<String> {
    satex::linter::lint(analysis)
        .iter()
        .filter(|f| f.get("code").is_some_and(|c| c == code))
        .flat_map(|f| f.get("edits").and_then(|e| e.as_array()).cloned().unwrap_or_default())
        .map(|e| e["replacement"].as_str().unwrap_or_default().to_string())
        .collect()
}

/// A body as documents have one: text and a figure with its alternative text. (A body of one
/// word ends in a joined state under tagging, where the title and language are unknown.)
const BODY: &str = "Text about ducks.\n\n\\includegraphics[alt={A duck}]{a}";

fn metadata(options: &str, preamble: &str, body: &str) -> String {
    format!(
        "\\DocumentMetadata{{{options}}}\n\\documentclass{{article}}\n{preamble}\\begin{{document}}\n{body}\n\\end{{document}}\n"
    )
}

/// The facts come from the document properties `\DocumentMetadata` records, read with
/// `\GetDocumentProperties` where the run ended: tagging, the standard (UA's part from
/// `{2}{2024}`), the language, the PDF version the standard forces, and an empty title.
/// The class's status comes from the installation's `latex-tagging-status.ltx`, run as TeX.
#[test]
fn the_run_says_what_tagging_it_set_up() {
    if !crate::support::installed() {
        return;
    }
    let a = analyze(&metadata("lang=de-DE,pdfstandard=ua-2,tagging=on", "", "x"));
    let t = tagging::of(&a).expect("tagging facts");
    assert!(t.active(), "{:?}", t.tagging);
    assert_eq!(t.standards, ["UA-2"]);
    assert_eq!(t.lang, Val::Text("de-DE".into()));
    assert_eq!(t.lang_set, Val::Text("de-DE".into()));
    assert_eq!(t.pdfversion, Val::Text("2.0".into()));
    // no `\title`, no pdftitle: never a title; unknown where the run's end is joined (the
    // fault at `\end{document}` under tagging, see ~/satex-learnings.md 2026-11-16)
    assert!(matches!(t.title, Val::Empty | Val::Unknown), "{:?}", t.title);
    let class = t.statuses.iter().find(|s| s.file == "article.cls").expect("article.cls in the status list");
    assert_eq!(class.meaning(), "compatible");
    // the tagging code's own files (code 8) are no packages to judge
    assert!(t.statuses.iter().all(|s| !s.file.starts_with("tagpdf")), "{:?}", t.statuses);
}

/// A document without `\DocumentMetadata` has no tagging facts and no tagging findings:
/// the rules are silent on the documents they are not about.
#[test]
fn without_document_metadata_there_is_nothing_to_tag() {
    if !crate::support::installed() {
        return;
    }
    let a = analyze("\\documentclass{article}\n\\usepackage{listings}\n\\begin{document}\nx\n\\end{document}\n");
    assert!(tagging::of(&a).is_none());
    assert_eq!(found(&a, "tagging-incompatible-package"), Vec::<String>::new());
}

/// `\DocumentMetadata{}` turns pdfmanagement on but not tagging: `document/tagging` is
/// `inactive`, and a package the status list marks incompatible breaks nothing.
#[test]
fn metadata_without_tagging_is_inactive() {
    if !crate::support::installed() {
        return;
    }
    let a = analyze(&metadata("", "\\usepackage{listings}\n", "x"));
    let t = tagging::of(&a).expect("tagging facts");
    assert_eq!(t.tagging, Val::Text("inactive".into()));
    assert_eq!(found(&a, "tagging-incompatible-package"), Vec::<String>::new());
}

/// With tagging on, a package the status list marks *currently incompatible* (listings) is
/// reported where the document loads it; one marked compatible (graphicx) is not.
#[test]
fn an_incompatible_package_is_reported_from_the_status_list() {
    if !crate::support::installed() {
        return;
    }
    let a = analyze(&metadata("lang=en,tagging=on", "\\usepackage{graphicx}\n\\usepackage{listings}\n", "x"));
    let msgs = found(&a, "tagging-incompatible-package");
    assert_eq!(msgs.len(), 1, "{msgs:?}");
    assert!(msgs[0].contains("`listings.sty` is currently incompatible"), "{msgs:?}");
}

/// PDF/UA needs `dc:title`. Under tagging latex-lab's title code copies `\title` into
/// `hyperref/pdftitle` at the last page, so `\title{Hello}` alone gives the PDF its title
/// (pdflatex's XMP holds `Hello`): no finding. Without `\title` the PDF has none: one. Red
/// while every tagged run faulted at `\end{document}` (`\the` of an `\int_const:Nn` of an
/// unknown object number printed nothing, `useobjnum` read no number), joining the end.
#[test]
fn pdfua_without_any_title_is_reported() {
    if !crate::support::installed() {
        return;
    }
    let titled =
        analyze(&metadata("lang=en,pdfstandard=ua-2,tagging=on", "\\usepackage{graphicx}\n\\title{Hello}\n", BODY));
    assert_eq!(tagging::of(&titled).expect("tagging facts").title, Val::Text("Hello".into()));
    assert_eq!(found(&titled, "pdfua-without-title"), Vec::<String>::new());
    let a = analyze(&metadata("lang=en,pdfstandard=ua-2,tagging=on", "\\usepackage{graphicx}\n", BODY));
    assert_eq!(found(&a, "pdfua-without-title").len(), 1);
    // no formula: no MathML to miss
    assert_eq!(found(&a, "tagged-math-without-mathml"), Vec::<String>::new());
}

/// A title written as `\title[pdftitle=…]` is no finding. The MathML rule needs a formula
/// the run set: it once fired on a document without math, from the math module's search for
/// the MathML file alone; with `$x$` under pdfLaTeX and no `main-mathml.html` it fires.
#[test]
fn a_pdf_title_is_no_finding_and_a_formula_wants_mathml() {
    if !crate::support::installed() {
        return;
    }
    let a = analyze(&metadata(
        "lang=en,pdfstandard=ua-2,tagging=on",
        "\\usepackage{graphicx}\n\\title[pdftitle={Hello}]{Hello}\n",
        &format!("{BODY} $x$"),
    ));
    assert_eq!(found(&a, "pdfua-without-title"), Vec::<String>::new());
    assert_eq!(found(&a, "tagged-math-without-mathml").len(), 1);
}

/// Without `lang=`, LaTeX guesses the language at `\begin{document}` (`en` without babel).
/// The rule tells the guess from a written key by the state at `\begin{document}`, and its
/// fix writes the guess, which leaves the PDF as it is.
#[test]
fn a_guessed_language_is_reported_and_written() {
    if !crate::support::installed() {
        return;
    }
    let a = analyze(&metadata("tagging=on", "\\usepackage{graphicx}\n", BODY));
    let t = tagging::of(&a).expect("tagging facts");
    assert_eq!(t.lang_set, Val::Empty);
    assert_eq!(found(&a, "tagging-without-language").len(), 1);
    // written after the key the call has
    assert_eq!(fixes(&a, "tagging-without-language"), [", lang=en"]);
}

/// `\DocumentMetadata` after `\documentclass` is an error that leaves tagging off, found by
/// where the run expanded it. A later call after a first one before the class is allowed
/// (the first redefines it to set keys), so it is no finding.
#[test]
fn metadata_after_the_class_is_reported() {
    if !crate::support::installed() {
        return;
    }
    let a =
        analyze("\\documentclass{article}\n\\DocumentMetadata{tagging=on}\n\\begin{document}\nx\n\\end{document}\n");
    assert_eq!(found(&a, "metadata-after-class").len(), 1);
    // a second call after a first one before the class only sets keys
    let a = analyze(
        "\\DocumentMetadata{}\n\\documentclass{article}\n\\DocumentMetadata{lang=en}\n\\begin{document}\nx\n\\end{document}\n",
    );
    assert_eq!(found(&a, "metadata-after-class"), Vec::<String>::new());
}

/// With tagging on, `\includegraphics` without `alt`, `artifact` or `actualtext` is reported;
/// one with them is not, and one whose options come from a macro is left alone (its keys
/// are not known from the text).
#[test]
fn a_graphic_without_alternative_text_is_reported() {
    if !crate::support::installed() {
        return;
    }
    let body = "\\includegraphics[width=1cm]{a}\n\\includegraphics[alt={A dot}]{a}\n\\includegraphics[artifact]{a}\n\\def\\opts{width=1cm}\\expandafter\\includegraphics\\expandafter[\\opts]{a}";
    let a = analyze(&metadata("lang=en,tagging=on", "\\usepackage{graphicx}\n", body));
    let msgs = found(&a, "graphic-without-alt");
    assert_eq!(msgs.len(), 1, "{msgs:?}");
}

/// A 1×1 PNG: a figure the graphics driver reads the size of.
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0x00,
    0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49,
    0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00,
    0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// `\newcount` with an allocation counter the run knows only as an interval (paths that
/// allocated different numbers of registers, as tagpdf's loading leaves) names a register
/// whose `\meaning` prints that interval as typed digits. supp-pdf strips that meaning a
/// character at a time (`\ifnum10<9#1`) until the number, then compares it with 20: `9`
/// followed by a digit of the interval is itself an interval, so both tests decide. Before,
/// the meaning ended in unknown digits, the loop never decided, and every tagged document
/// with graphicx stalled at `\begin{document}`.
#[test]
fn an_allocated_register_of_an_interval_number_decides_supp_pdfs_loop() {
    if !crate::support::installed() {
        return;
    }
    let a = crate::support::analyze_latex(
        "\\documentclass{article}\n\\ifnum\\pdfuniformdeviate 2=0 \\global\\count10=300 \\else \\global\\count10=310 \\fi\n\\def\\strip#1{\\ifnum10<9#1 #1\\else\\expandafter\\strip\\fi}\n\\newcount\\foo\n\\edef\\y{\\meaning\\foo}\n\\edef\\z{\\ifnum\\expandafter\\strip\\meaning\\foo>20 big\\else small\\fi}\n\\begin{document}\n\\end{document}\n",
    );
    let y = crate::support::body(&a, "y");
    assert!(y.contains("301..311"), "{y}");
    assert_eq!(crate::support::body(&a, "z"), "big");
}

/// A register allocated while the counter is an interval is one of the registers in that
/// interval, so it holds what they hold: zero when none was written, and a candidate's own
/// value when one was. tagpdf's
/// `\l__tag_loglevel_int` is such a register. Before, it read as any number, so every
/// `\int_compare:nNnT {\l__tag_loglevel_int} > {2}` forked into tagpdf's debug code (the
/// `mc-used` intarray), and a tagged `\maketitle` or tabular ran 8 paths to the end (300 s).
#[test]
fn a_register_of_an_interval_number_holds_what_its_candidates_hold() {
    if !crate::support::installed() {
        return;
    }
    let a = crate::support::analyze_latex(
        "\\documentclass{article}\n\\ifnum\\pdfuniformdeviate 2=0 \\global\\count10=300 \\else \\global\\count10=310 \\fi\n\\newcount\\foo\n\\edef\\y{\\ifnum\\foo>2 big\\else small\\fi}\n\\newcount\\baz\n\\baz=4\n\\edef\\z{\\the\\baz}\n\\global\\count306=7\n\\newcount\\qux\n\\edef\\v{\\ifnum\\qux>2 big\\else small\\fi}\n\\begin{document}\n\\end{document}\n",
    );
    assert_eq!(crate::support::body(&a, "y"), "small");
    assert_eq!(crate::support::body(&a, "z"), "4");
    // a candidate that holds 7 makes the test undecided
    assert_ne!(crate::support::body(&a, "v"), "small");
}

/// The body after a tagged `\maketitle` is followed on one path. The title's tabular and
/// struct code read tagpdf's log level, a register allocated while `\count10` was an
/// interval; while that read as any number, the title forked into tagpdf's debug code and
/// up to 40 paths ran on (300 s, body unread). Now the run ends on few paths and reaches
/// the definition after the title.
#[test]
fn the_body_after_a_tagged_title_is_followed() {
    if !crate::support::installed() {
        return;
    }
    // The run that reads tagpdf first peaks while loading it (14 paths at tagpdf.sty's random ids); a
    // warm run replays the package cache. So warm it, then compare two warm runs.
    let untitled_source = metadata("lang=en-US,tagging=on", "\\title{Ducks}\n", "\\gdef\\after{reached}\nText.");
    analyze(&untitled_source);
    let a =
        analyze(&metadata("lang=en-US,tagging=on", "\\title{Ducks}\n", "\\maketitle\n\\gdef\\after{reached}\nText."));
    assert_eq!(crate::support::body(&a, "after"), "reached");
    let untitled = analyze(&untitled_source);
    // the title's tabular may fork a little (4 paths against 2 on the CLI); the bug ran 40
    const TITLE_PATHS: usize = 4;
    assert!(
        a.peak_paths <= untitled.peak_paths + TITLE_PATHS,
        "{} paths, {} without the title",
        a.peak_paths,
        untitled.peak_paths
    );
    assert!(found(&a, "missing-end").is_empty());
}

/// `\fp_eval:n` over a position read back from the aux (unknown digits), as latex-lab's
/// tagged graphic computes its BBox. Bug pinned: the l3fp parser inspected the typed unknown
/// digit by digit and forked 92k times (37 s for two sums); a tagged `\includegraphics` of an
/// existing file hit the time limit there.
#[test]
fn an_fp_sum_over_an_unknown_position_stays_on_few_paths() {
    if !crate::support::installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\ExplSyntaxOn\n\\tl_gset:Ne \\x {\\dim_to_decimal_in_bp:n {\\the\\pdflastxpos sp}}\n\\tl_gset:Ne \\y {\\fp_eval:n {\\x + 10}}\n\\tl_gset:Ne \\z {\\fp_eval:n {\\y + \\x}}\n\\ExplSyntaxOff\n\\gdef\\after{reached}\n\\end{document}\n";
    // the first run of a fresh cache peaks while it loads the packages; the body is what counts
    crate::support::analyze_latex(source);
    let a = crate::support::analyze_latex(source);
    assert_eq!(crate::support::body(&a, "after"), "reached");
    // The bug forked per digit: 112.6k forks. A scan that runs out of choices now ends as one
    // unknown, so its arms (a few at once, never the document's paths) stay a handful.
    const FEW_FORKS: u64 = 1_000;
    assert!(a.forks <= FEW_FORKS, "{} forks", a.forks);
}

/// The smallest form of the root above: an unknown integer position, no decimals.
/// The first run peaks while it loads the packages, so the cache is warmed and the body's
/// forks are bounded, as in the sum above. Bug pinned: `part_digits` keeps unbounded typed
/// digits as one token, so each char test in l3fp is undecided again per arm (67.7k forks, 18 s).
/// Splitting them fixes this but breaks the pgfmath canaries (fp-work.patch).
#[test]
fn an_fp_eval_of_an_unknown_integer_stays_on_few_paths() {
    if !crate::support::installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\n\\ExplSyntaxOn\\tl_gset:Ne\\x{\\int_use:N\\pdflastxpos}\\tl_gset:Ne\\y{\\fp_eval:n{\\x}}\\ExplSyntaxOff\n\\gdef\\after{reached}\n\\end{document}\n";
    crate::support::analyze_latex(source);
    let a = crate::support::analyze_latex(source);
    // one scan of the unknown integer: a handful of arms, never one per digit
    const FEW_FORKS: u64 = 100;
    assert!(a.forks <= FEW_FORKS, "{} forks", a.forks);
    assert_eq!(crate::support::body(&a, "after"), "reached");
}
