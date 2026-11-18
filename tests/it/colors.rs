//! Lints over the colors xcolor stores through `\color@⟨name⟩` and the
//! `\pdfcolorstack`s the run executed.

use std::path::PathBuf;

use crate::support::installed;
use satex::analysis::machine::Machine;

fn scratch(name: &str) -> PathBuf {
    let directory = crate::support::tmp_root().join("satex-colors").join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).expect("a scratch directory");
    directory
}

/// The `name` of every finding of `code` on a document whose preamble and
/// body are given, in the order reported.
fn findings(name: &str, code: &str, preamble: &str, body: &str) -> Vec<String> {
    let directory = scratch(name);
    let document = format!("\\documentclass{{article}}\n{preamble}\n\\begin{{document}}\n{body}\n\\end{{document}}\n");
    let main = directory.join("main.tex");
    std::fs::write(&main, &document).unwrap();
    let cfg = crate::support::config();
    let analysis = Machine::analyze(&document, Some(&main), &cfg);
    let out = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|record| record["code"] == code)
        .map(|record| record["name"].as_str().unwrap_or_default().to_string())
        .collect();
    let _ = std::fs::remove_dir_all(&directory);
    out
}

#[test]
fn a_colour_used_before_it_is_ever_defined_is_reported() {
    if !installed() {
        return;
    }
    assert_eq!(
        findings("undef", "undefined-color", "\\usepackage{xcolor}", "\\textcolor{nosuchcolor}{x}"),
        ["nosuchcolor"]
    );
}

#[test]
fn a_colour_defined_and_used_raises_nothing() {
    if !installed() {
        return;
    }
    let f = findings(
        "used",
        "undefined-color",
        "\\usepackage{xcolor}\n\\definecolor{myred}{RGB}{255,0,0}",
        "\\textcolor{myred}{x}",
    );
    assert!(f.is_empty(), "{f:?}");
}

/// xcolor looks a mix (`green!50!blue`) up as a name before it parses it as an expression: that
/// lookup is no use of an undefined color.
#[test]
fn a_colour_mix_is_no_undefined_colour() {
    if !installed() {
        return;
    }
    let f = findings("mix", "undefined-color", "\\usepackage{xcolor}", "\\textcolor{green!50!blue}{x}");
    assert!(f.is_empty(), "{f:?}");
}

/// A mix of unknown percentage makes the color spec text that is one of several, or typed text,
/// past the paths one command may part into. `\@gobbletwo` in `\XC@clean` once took such a
/// token whole as its second argument, so the spec came out empty and `\c@lor@@rgb`'s delimited
/// scan read the rest of the document to its end: `missing-end`. An undelimited argument takes
/// the first token and puts the rest back.
/// `\colorlet` sanitizes the spec with `\meaning`: a spec that is one of
/// several exact triples (the mix of an unknown percentage after a loop's
/// paths join) was printed as untyped text. Its components then split into
/// arms TeX never takes, one with a single number, and `\c@lor@@rgb` read
/// that one past the document's end. Each text is now printed apart.
#[test]
fn a_colour_mix_in_a_loop_keeps_the_document_end() {
    if !installed() {
        return;
    }
    let body = "\\foreach \\i in {1,2} {\\pgfmathsetmacro{\\w}{1+\\pdfuniformdeviate 5}\\colorlet{c\\i}{blue!\\w!red}}";
    let f = findings("unknown-mix-loop", "missing-end", "\\usepackage{tikz}", body);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn a_colour_mix_of_unknown_percentage_keeps_the_document_end() {
    if !installed() {
        return;
    }
    let body = "\\edef\\w{\\the\\numexpr1+\\pdfuniformdeviate 5\\relax}\\color{blue!\\w!red}x";
    let f = findings("unknown-mix", "missing-end", "\\usepackage{xcolor}", body);
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn a_defined_colour_never_read_back_is_reported() {
    if !installed() {
        return;
    }
    assert_eq!(
        findings("unused", "unused-color", "\\usepackage{xcolor}\n\\definecolor{leftover}{RGB}{1,2,3}", "hello"),
        ["leftover"]
    );
}

#[test]
fn a_colour_used_by_colorlet_is_not_reported_as_unused() {
    if !installed() {
        return;
    }
    let f = findings(
        "letused",
        "unused-color",
        "\\usepackage{xcolor}\n\\definecolor{base}{RGB}{1,2,3}\n\\colorlet{tint}{base!50}",
        "\\textcolor{tint}{x}",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn redefining_a_colour_with_a_different_value_is_reported() {
    if !installed() {
        return;
    }
    assert_eq!(
        findings(
            "conflict",
            "conflicting-color-definition",
            "\\usepackage{xcolor}\n\\definecolor{myred}{RGB}{255,0,0}\n\\definecolor{myred}{RGB}{0,0,0}",
            "hello"
        ),
        ["myred"]
    );
}

#[test]
fn redefining_a_colour_with_the_same_value_is_not_reported() {
    if !installed() {
        return;
    }
    let f = findings(
        "same",
        "conflicting-color-definition",
        "\\usepackage{xcolor}\n\\definecolor{myred}{RGB}{255,0,0}\n\\definecolor{myred}{RGB}{255,0,0}",
        "hello",
    );
    assert!(f.is_empty(), "{f:?}");
}

#[test]
fn a_pdfcolorstack_push_never_popped_is_reported() {
    if !installed() {
        return;
    }
    assert_eq!(
        findings("unbalanced", "unbalanced-color-stack", "", "\\pdfcolorstack 3 push {1 0 0 rg}"),
        ["pdfcolorstack"]
    );
}

#[test]
fn a_balanced_pdfcolorstack_raises_nothing() {
    if !installed() {
        return;
    }
    let f =
        findings("balanced", "unbalanced-color-stack", "", "\\pdfcolorstack 3 push {1 0 0 rg}\\pdfcolorstack 3 pop");
    assert!(f.is_empty(), "{f:?}");
}

/// beamer's color themes look their colors up (`fg`, `itemize/enumerate body.fg`) without any of
/// them being an error: no undefined color.
#[test]
fn beamers_color_lookups_are_no_undefined_colours() {
    if !installed() {
        return;
    }
    let directory = scratch("beamer");
    let document = "\\documentclass{beamer}\n\\begin{document}\n\\begin{frame}{T}\n\\begin{itemize}\n\\item<1-> A\n\\end{itemize}\n\\end{frame}\n\\end{document}\n";
    let main = directory.join("main.tex");
    std::fs::write(&main, document).unwrap();
    let analysis = Machine::analyze(document, Some(&main), &crate::support::config());
    let found: Vec<String> = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|record| record["code"] == "undefined-color")
        .map(|record| record["name"].as_str().unwrap_or_default().to_string())
        .collect();
    assert!(found.is_empty(), "{found:?}");
    let _ = std::fs::remove_dir_all(&directory);
}

/// A color named after a role and given an existing color's name is an alias on purpose, also
/// through a package's wrapper whose internal name the document never wrote.
#[test]
fn a_colour_aliased_to_an_existing_name_is_no_duplicate() {
    if !installed() {
        return;
    }
    let f = findings(
        "alias",
        "duplicate-color-value",
        "\\usepackage{xcolor}\n\\makeatletter\\newcommand*\\rolelet[2]{\\colorlet{my@role@#1}{#2}}\\makeatother\n\\colorlet{warning}{orange}\n\\rolelet{comments}{orange}",
        "\\textcolor{warning}{x}",
    );
    assert!(f.is_empty(), "{f:?}");
}

/// A color whose components spell an existing color is a duplicate, named as the call wrote it.
#[test]
fn a_colour_with_the_components_of_another_is_a_duplicate() {
    if !installed() {
        return;
    }
    let f = findings(
        "duplicate",
        "duplicate-color-value",
        "\\usepackage{xcolor}\n\\makeatletter\\newcommand*\\roledef[2]{\\definecolor{my@role@#1}{rgb}{#2}}\\makeatother\n\\definecolor{myred}{rgb}{1,0,0}\n\\roledef{alert}{1,0,0}",
        "\\textcolor{myred}{x}",
    );
    assert_eq!(f, ["myred", "alert"]);
}

/// Recoloring a package's color (xcolor's `red`) is what the document means to do: no conflict.
#[test]
fn recolouring_a_package_colour_is_no_conflict() {
    if !installed() {
        return;
    }
    let f = findings(
        "recolor",
        "conflicting-color-definition",
        "\\usepackage{xcolor}\n\\definecolor{red}{rgb}{0.8,0,0}",
        "\\textcolor{red}{x}",
    );
    assert!(f.is_empty(), "{f:?}");
}

/// Every finding of `code` on the document, whole records.
fn records(name: &str, code: &str, preamble: &str, body: &str) -> Vec<satex::queries::query::Record> {
    let directory = scratch(name);
    let document = format!("\\documentclass{{article}}\n{preamble}\n\\begin{{document}}\n{body}\n\\end{{document}}\n");
    let main = directory.join("main.tex");
    std::fs::write(&main, &document).unwrap();
    let analysis = Machine::analyze(&document, Some(&main), &crate::support::config());
    let out = satex::linter::lint(&analysis).into_iter().filter(|record| record["code"] == code).collect();
    let _ = std::fs::remove_dir_all(&directory);
    out
}

/// The replacement text of a finding's only edit.
fn replacement(record: &satex::queries::query::Record) -> String {
    let edits = record["edits"].as_array().expect("edits");
    assert_eq!(edits.len(), 1, "{record:?}");
    edits[0]["replacement"].as_str().unwrap().to_string()
}

const XCOLOR: &str = "\\usepackage{xcolor}";

/// Intuition: light gray on the white page is hard to read. Pins the WCAG ratio read off the
/// color the run computed, and the fix that darkens the user's own `\definecolor` in its model.
#[test]
fn pale_text_on_the_page_is_low_contrast() {
    if !installed() {
        return;
    }
    let pale = 0.8;
    let preamble = format!("{XCOLOR}\n\\definecolor{{pale}}{{gray}}{{{pale}}}");
    let found = records("pale", "low-contrast-color", &preamble, "\\textcolor{pale}{Faint words}");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["name"], "pale");
    let ratio = satex::project::config::LowContrastColor::default().ratio;
    assert!(found[0]["message"].as_str().unwrap().contains(&format!("{ratio}:1")), "{found:?}");
    assert_eq!(found[0]["applicability"], "unsafe");
    let darker: f64 = replacement(&found[0]).parse().expect("a gray level");
    assert!(darker < pale, "{found:?}");
}

/// Intuition: plain black text and a dark named color read well. Pins that ordinary text raises
/// nothing.
#[test]
fn dark_text_is_no_low_contrast() {
    if !installed() {
        return;
    }
    let found = records("dark", "low-contrast-color", XCOLOR, "Plain words. \\textcolor{blue}{Blue words}");
    assert!(found.is_empty(), "{found:?}");
}

/// Intuition: white text in a black `\colorbox` is fine, on a yellow one it is not. Pins that the
/// box's fill rule (`\color@block`) is the background of the characters the box holds.
#[test]
fn a_colorbox_is_the_background_of_its_text() {
    if !installed() {
        return;
    }
    let dark = records("dark-box", "low-contrast-color", XCOLOR, "\\colorbox{black}{\\color{white}Dark box}");
    assert!(dark.is_empty(), "{dark:?}");
    let light = records("light-box", "low-contrast-color", XCOLOR, "\\colorbox{yellow}{\\color{white}Light box}");
    assert_eq!(light.len(), 1, "{light:?}");
    assert!(light[0]["message"].as_str().unwrap().contains("`yellow`"), "{light:?}");
}

/// Intuition: `\pagecolor` repaints the page under all text. Pins the page color as the
/// background, both ways: white text reads on black, black text does not.
#[test]
fn the_page_color_is_the_background() {
    if !installed() {
        return;
    }
    let white = records("page-white", "low-contrast-color", XCOLOR, "\\pagecolor{black}\\color{white}Night text");
    assert!(white.is_empty(), "{white:?}");
    let black = records("page-black", "low-contrast-color", XCOLOR, "\\pagecolor{black}Unseen text");
    assert_eq!(black.len(), 1, "{black:?}");
}

/// Intuition: WCAG lets large text pass with less contrast. Pins the size read from `\f@size`:
/// mid gray fails at the body size and passes in `\Huge`.
#[test]
fn large_text_needs_less_contrast() {
    if !installed() {
        return;
    }
    let small = records("small-gray", "low-contrast-color", XCOLOR, "\\textcolor{gray}{Small words}");
    assert_eq!(small.len(), 1, "{small:?}");
    let large = records("large-gray", "low-contrast-color", XCOLOR, "{\\Huge\\textcolor{gray}{Big words}\\par}");
    assert!(large.is_empty(), "{large:?}");
}

/// Intuition: a TikZ node's fill is drawn by literals satex does not read. Pins that text over
/// such a drawing has an unknown background and is left alone, not judged against the page.
#[test]
fn text_over_a_tikz_fill_is_left_alone() {
    if !installed() {
        return;
    }
    let body = "\\tikz\\node[fill=black,text=white]{Node text};";
    let found = records("tikz-fill", "low-contrast-color", "\\usepackage{tikz}", body);
    assert!(found.is_empty(), "{found:?}");
}

/// Intuition: a dark green and a vermillion mark opposite meanings, but look the same to a reader
/// with protanopia. Pins the simulation, the named deficiency and an Okabe-Ito fix in the user's
/// own model.
#[test]
fn green_and_vermillion_in_one_paragraph_are_indistinct() {
    if !installed() {
        return;
    }
    let (good, bad) = ("0,128,0", "213,94,0");
    let preamble = format!("{XCOLOR}\n\\definecolor{{good}}{{RGB}}{{{good}}}\n\\definecolor{{bad}}{{RGB}}{{{bad}}}");
    let found = records("green-red", "indistinct-colors", &preamble, "\\textcolor{good}{yes} or \\textcolor{bad}{no}");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0]["name"], "bad");
    assert!(found[0]["message"].as_str().unwrap().contains("protanopia"), "{found:?}");
    let spec = replacement(&found[0]);
    let bytes: Vec<u8> = spec.split(',').map(|b| b.parse().expect("an RGB byte")).collect();
    assert_eq!(bytes.len(), good.split(',').count(), "{spec}");
    assert_ne!(spec, bad);
}

/// Intuition: Okabe-Ito's blue and orange stay apart under every deficiency. Pins that a
/// color-blind-safe pair raises nothing.
#[test]
fn okabe_ito_colors_are_distinct() {
    if !installed() {
        return;
    }
    let preamble =
        format!("{XCOLOR}\n\\definecolor{{one}}{{RGB}}{{0,114,178}}\n\\definecolor{{two}}{{RGB}}{{230,159,0}}");
    let found = records("okabe", "indistinct-colors", &preamble, "\\textcolor{one}{yes} or \\textcolor{two}{no}");
    assert!(found.is_empty(), "{found:?}");
}

/// Intuition: colors far apart in the document are not compared. Pins the scope: two
/// paragraphs outside any environment are not side by side, one environment around both is.
#[test]
fn only_colors_side_by_side_are_compared() {
    if !installed() {
        return;
    }
    let preamble =
        format!("{XCOLOR}\n\\definecolor{{good}}{{RGB}}{{0,128,0}}\n\\definecolor{{bad}}{{RGB}}{{213,94,0}}");
    let apart = "\\textcolor{good}{yes}\n\n\\textcolor{bad}{no}";
    let found = records("apart", "indistinct-colors", &preamble, apart);
    assert!(found.is_empty(), "{found:?}");
    let together = format!("\\begin{{center}}\n{apart}\n\\end{{center}}");
    let found = records("together", "indistinct-colors", &preamble, &together);
    assert_eq!(found.len(), 1, "{found:?}");
}

/// Intuition: a page background drawn at shipout (beamer's canvas, eso-pic) is not white, and
/// satex does not read its color. Pins that white text there is left alone, not judged against
/// a white page.
#[test]
fn a_page_drawn_at_shipout_is_an_unknown_background() {
    if !installed() {
        return;
    }
    let preamble =
        format!("{XCOLOR}\n\\AddToHook{{shipout/background}}{{\\put(0,0){{\\pdfliteral{{0 g 0 0 600 800 re f}}}}}}");
    let found = records("shipout", "low-contrast-color", &preamble, "\\color{white}Night text");
    assert!(found.is_empty(), "{found:?}");
}

/// Intuition: a character set before xcolor loads must not blind the run to colors later. Pins
/// that the color macros a character's state reads are looked up again once they exist: the
/// first lookup missed `\current@page@color`, so every later page color read as none.
#[test]
fn colors_defined_after_the_first_character_are_still_read() {
    if !installed() {
        return;
    }
    let preamble = format!("\\setbox0\\hbox{{x}}\n{XCOLOR}");
    let found = records("late-names", "low-contrast-color", &preamble, "\\pagecolor{black}Unseen text");
    assert_eq!(found.len(), 1, "{found:?}");
}
