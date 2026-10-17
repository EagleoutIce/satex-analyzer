//! Runs the analysis may not drop: where it cannot follow every path a
//! choice makes, it over-approximates them (unknown values, a cut
//! expansion, paths kept) rather than following one and losing the rest.

use crate::support::definition;
use satex::config::{Config, Engine, Load, Texmf};
use satex::machine::{Analysis, Machine};
use satex::plugin::Kernel;

/// A scratch directory of its own per test.
fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = crate::support::tmp_root().join(format!("satex-dropped-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// INITEX with `\input` reading real files, and `limits` set by `tune`.
fn analyze_files(dir: &std::path::Path, source: &str, tune: impl FnOnce(&mut Config)) -> Analysis {
    let mut cfg = Config {
        engine: Some(Engine::PdfTeX),
        kernel: Some(Kernel::None),
        load: Load { packages: false, classes: false, format: false, ..Default::default() },
        texmf: Texmf { kpsewhich: false, ..Default::default() },
        ..crate::support::config()
    };
    tune(&mut cfg);
    let main = dir.join("main.tex");
    let source = format!("{}{source}", crate::support::PRELUDE);
    std::fs::write(&main, &source).unwrap();
    Machine::analyze(&source, Some(&main), &cfg)
}

/// Every diagnostic, as `code: message`.
fn all(analysis: &Analysis) -> Vec<String> {
    analysis.facts.diagnostics.iter().map(|d| format!("{}: {}", d.code, d.message)).collect()
}

/// An error in the document's own text is kept, past an approximation too:
/// one arm, past a `\write` to a stream satex cannot pin down, leaves
/// `\grab`'s argument open to the end of the file (tex.web §§ 336-339), and
/// says so. The other arm reads both files.
#[test]
fn an_error_only_an_approximation_reaches_leaves_the_files_read() {
    let dir = scratch("approximated-error");
    std::fs::write(dir.join("a.tex"), "\\gdef\\ina{}\n").unwrap();
    std::fs::write(dir.join("b.tex"), "\\gdef\\inb{}\n").unwrap();
    let source = "\\def\\grab#1{}\n\\ifnum\\pdfuniformdeviate2=0 \\relax\\else\\immediate\\write\\pdfuniformdeviate9{x}\\expandafter\\grab\\expandafter{\\fi\n\\input a\n\\input b\n";
    let analysis = analyze_files(&dir, source, |_| {});
    let read = |name: &str| definition(&analysis, name).is_some();
    assert!(read("ina") && read("inb"), "{:#?}", all(&analysis));
    let reported = all(&analysis);
    assert!(
        reported.iter().any(|d| d.starts_with("runaway-argument"))
            && !reported.iter().any(|d| d.starts_with("assumed-compiles")),
        "{reported:#?}"
    );
}

/// An expl3 function satex steps over by its signature reads its
/// arguments as a macro call does: a file ending first ends the scan
/// there, a runaway (tex.web §§ 336-339), and the text after the
/// `\input` is read as the document's again, not as the argument.
#[test]
fn a_stepped_over_call_does_not_read_its_argument_past_its_file() {
    let dir = scratch("stepped-over");
    std::fs::write(dir.join("b.tex"), "\\catcode`\\_=11 \\catcode`\\:=11 \\foo_bar:n\n").unwrap();
    let source = "\\input b\n{\\gdef\\after{}}\n";
    let analysis = analyze_files(&dir, source, |_| {});
    assert!(definition(&analysis, "after").is_some(), "{:#?}", all(&analysis));
    let runaway = all(&analysis);
    assert!(runaway.iter().any(|d| d.starts_with("runaway-argument") && d.contains("foo_bar:n")), "{runaway:#?}");
}

/// A count a frame steps up at its start and back at its end, while one
/// arm of an undecided choice opens an argument a paragraph end runs
/// into (tex.web § 396): TeX abandons the call, the step back with it,
/// and the count stays 1 on that arm with an error, 0 on the arm that
/// compiles (`pdftex -ini`, each arm forced by `\pdfsetrandomseed`).
/// Whether the arms approximated (`\write` to a stream not known) or
/// not, the count afterwards is 0 or unknown, never an exact 1.
#[test]
fn a_runaway_that_abandons_a_frame_end_leaves_no_exact_count_of_its_own() {
    let head =
        "\\count10=0 \\def\\grab#1{}\\def\\fs{\\global\\advance\\count10 1 }\\def\\fe{\\global\\advance\\count10 -1 }";
    let w = "\\immediate\\write\\pdfuniformdeviate9{y}";
    let arms = [("\\relax", ""), ("\\relax", w), (w, w), (w, "")];
    for (clean, runaway) in arms {
        let source = format!(
            "{head}\\fs\\ifnum\\pdfuniformdeviate2=0 {clean}\\else{runaway}\\expandafter\\grab\\expandafter{{\\fi\\fe\n\n\\xdef\\P{{\\the\\count10}}"
        );
        let analysis = crate::support::analyze_plain(&source);
        let diagnostics = all(&analysis);
        assert!(diagnostics.iter().any(|d| d.starts_with("runaway-argument")), "{diagnostics:#?}");
        let count = crate::support::last_body(&analysis, "P");
        assert!(count == "0" || count.contains("unknown"), "arms {clean:?} {runaway:?}: {count:?}");
    }
}
