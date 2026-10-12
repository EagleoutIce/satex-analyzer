//! `\endinput` in every place it can stand.  tex.web § 362 keeps one global
//! `force_eof`: the primitive only sets it, and the next file to ask for a
//! line takes it and closes.  So the rest of the line runs, the next line
//! does not, and a file opened in between takes the flag over.
//!
//! Every expectation here is what `pdftex`, `xetex` and `luatex` print for
//! the same input, read with `\message` probes.  Each test says what the
//! engine wrote.

use crate::support::{PRELUDE, analyze_plain as analyze, diagnosed};
use crate::support::{defined, installed, last_body as body, last_definition as definition};
use satex::config::{Cache, Config, Engine, Load, Texmf};
use satex::machine::{Analysis, Machine};
use satex::plugin::Kernel;

/// A document on disk, so that `\input` reads real files.
fn analyze_files(source: &str, main: &std::path::Path) -> Analysis {
    let cfg = Config { engine: Some(Engine::PdfTeX), kernel: Some(Kernel::None), ..crate::support::config() };
    std::fs::write(main, source).unwrap();
    Machine::analyze(source, Some(main), &cfg)
}

/// Which of `names` are defined, as one string, so a failure names them all.
fn state(analysis: &Analysis, names: &[&str]) -> String {
    names.iter().map(|n| format!("{n}={}", u8::from(defined(analysis, n)))).collect::<Vec<_>>().join(" ")
}

/// A scratch directory of its own per test.
fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = crate::support::tmp_root().join(format!("satex-endinput-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The installation's LaTeX with the kernel already interpreted: the tests
/// that read a class share one kernel cache, so each reads its own files and
/// not latex.ltx again (as `tests/it/package_cache.rs` does).
fn latex(dir: &std::path::Path) -> Config {
    static KERNEL: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();
    let shared = KERNEL.get_or_init(|| {
        let shared = crate::support::tmp_root().join(format!("satex-endinput-kernel-{}", std::process::id()));
        let cfg = Config {
            cache: Cache { dir: Some(shared.clone()), ..Default::default() },
            load: Load { classes: true, ..Default::default() },
            ..crate::support::config()
        };
        Machine::analyze("\\relax\n", Some(&shared.join("kernel.tex")), &cfg);
        shared
    });
    let cache = dir.join("cache");
    std::fs::create_dir_all(&cache).expect("a cache directory");
    for entry in std::fs::read_dir(shared).expect("the kernel cache").flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().ends_with(".postcard") && !cache.join(&name).exists() {
            std::fs::copy(entry.path(), cache.join(&name)).expect("the kernel cache is copied");
        }
    }
    Config {
        cache: Cache { dir: Some(cache), ..Default::default() },
        load: Load { classes: true, ..Default::default() },
        ..crate::support::config()
    }
}

#[test]
fn the_rest_of_the_line_still_runs() {
    // pdftex on `\message{[L1]}\endinput\message{[rest]}` + a second line:
    // `[L1] [rest]`, and the second line is never read.
    let analysis = analyze("\\def\\one{1}\\endinput\\def\\rest{r}\n\\def\\two{2}\n\\def\\three{3}\n");
    assert_eq!(state(&analysis, &["one", "rest", "two", "three"]), "one=1 rest=1 two=0 three=0");
    assert_eq!(body(&analysis, "rest"), "r");
}

#[test]
fn the_argument_scan_past_the_line_carries_the_file_one_line_on() {
    // `\endinput` names the line the mouth stands on, and an argument read
    // past the end of the line has moved it there (tex.web § 362: the line
    // is already in the buffer).  pdftex reads the second line to its end
    // and stops before the third.
    let analysis = analyze("\\def\\grab#1{\\endinput}\\grab\n{x}\\def\\two{2}\n\\def\\three{3}\n");
    assert_eq!(state(&analysis, &["two", "three"]), "two=1 three=0");
}

#[test]
fn a_macro_from_another_file_ends_the_file_being_read() {
    if !installed() {
        return;
    }
    // pdftex: `(./defs.tex) (./sub.tex [S-L1] [S-rest]) [reached-DEF] [MAIN]`.
    // The macro body runs to its end, the file it was read in ends.
    let dir = scratch("macro");
    std::fs::write(dir.join("defs.tex"), "\\def\\stop{\\endinput\\def\\reached{1}}%\n").unwrap();
    std::fs::write(dir.join("sub.tex"), "\\def\\sone{1}\\stop\\def\\srest{1}\n\\def\\stwo{2}\n").unwrap();
    let source = format!("{PRELUDE}\\input defs \\input sub \\def\\mend{{1}}\n");
    let analysis = analyze_files(&source, &dir.join("main.tex"));
    assert_eq!(
        state(&analysis, &["sone", "reached", "srest", "stwo", "mend"]),
        "sone=1 reached=1 srest=1 stwo=0 mend=1"
    );
}

#[test]
fn a_file_opened_on_the_same_line_takes_the_flag_over() {
    if !installed() {
        return;
    }
    // `force_eof` is one global and `start_input` reads the first line of the
    // new file without asking for it, so the new file is the one that ends.
    // pdftex, xetex and luatex all print
    // `(./sub.tex [S-L1] (./a.tex [A-L1]) (./b.tex [B-L1] [B-L2]) [S-rest]
    //  [S-L2]) [MAIN-end]`
    // for `\def\sone{1}\endinput\input a \input b \def\srest{1}`: a.tex stops
    // after its first line, b.tex is read whole, and sub.tex reads on.
    let dir = scratch("steal");
    std::fs::write(dir.join("a.tex"), "\\def\\aone{1}\n\\def\\atwo{2}\n").unwrap();
    std::fs::write(dir.join("b.tex"), "\\def\\bone{1}\n\\def\\btwo{2}\n").unwrap();
    std::fs::write(
        dir.join("sub.tex"),
        "\\def\\sone{1}\\endinput\\input a \\input b \\def\\srest{1}\n\\def\\stwo{2}\n\\def\\sthree{3}\n",
    )
    .unwrap();
    let source = format!("{PRELUDE}\\input sub \\def\\mend{{1}}\n");
    let analysis = analyze_files(&source, &dir.join("main.tex"));
    assert_eq!(
        state(&analysis, &["sone", "aone", "atwo", "bone", "btwo", "srest", "stwo", "sthree", "mend"]),
        "sone=1 aone=1 atwo=0 bone=1 btwo=1 srest=1 stwo=1 sthree=1 mend=1"
    );
}

#[test]
fn the_innermost_file_opened_takes_the_flag() {
    if !installed() {
        return;
    }
    // The flag passes on down: sub.tex hands it to a.tex, whose first line
    // opens b.tex, so b.tex is the one that ends after a line.  pdftex:
    // `(./sub.tex (./a.tex (./b.tex [B-L1]) [A-L1]) [S-rest] [S-L2])`.
    let dir = scratch("chain");
    std::fs::write(dir.join("b.tex"), "\\def\\bone{1}\n\\def\\btwo{2}\n").unwrap();
    std::fs::write(dir.join("a.tex"), "\\input b \\def\\aone{1}\n\\def\\atwo{2}\n").unwrap();
    std::fs::write(dir.join("sub.tex"), "\\endinput\\input a \\def\\srest{1}\n\\def\\stwo{2}\n").unwrap();
    let source = format!("{PRELUDE}\\input sub \\def\\mend{{1}}\n");
    let analysis = analyze_files(&source, &dir.join("main.tex"));
    assert_eq!(
        state(&analysis, &["bone", "btwo", "aone", "atwo", "srest", "stwo", "mend"]),
        "bone=1 btwo=0 aone=1 atwo=1 srest=1 stwo=1 mend=1"
    );
}

#[test]
fn everyeof_is_not_read_when_endinput_ends_the_file() {
    if !installed() {
        return;
    }
    // etex.ch § 362 inserts `\everyeof` only where the file ran out of
    // lines.  etex on `\everyeof{\message{[EVERYEOF]}}` with two files:
    // `(./e1.tex [E1-L1] [E1-rest]) [MID] (./e2.tex [E2-L1] [EVERYEOF])`.
    // `\seen` is read right after the file, so the main file's own end,
    // which does read `\everyeof`, cannot be taken for it.
    let dir = scratch("everyeof");
    std::fs::write(dir.join("e1.tex"), "\\def\\eone{1}\\endinput\\def\\erest{1}\n\\def\\etwo{2}\n").unwrap();
    std::fs::write(dir.join("e2.tex"), "\\def\\fone{1}\n").unwrap();
    let probe = "\\ifdefined\\saw\\def\\seen{1}\\fi";
    let ended = format!("{PRELUDE}\\everyeof{{\\def\\saw{{1}}}}\\input e1 {probe}\n");
    let analysis = analyze_files(&ended, &dir.join("main.tex"));
    assert_eq!(state(&analysis, &["eone", "erest", "etwo", "seen"]), "eone=1 erest=1 etwo=0 seen=0");
    // A file that simply runs out still reads it.
    let out = format!("{PRELUDE}\\everyeof{{\\def\\saw{{1}}}}\\input e2 {probe}\n");
    let analysis = analyze_files(&out, &dir.join("other.tex"));
    assert_eq!(state(&analysis, &["fone", "seen"]), "fone=1 seen=1");
}

#[test]
fn endinput_inside_scantokens_ends_the_pseudo_file() {
    if !installed() {
        return;
    }
    // eTeX reads a pseudo file with the same code as a file, so the flag is
    // taken by the pseudo file and the real one reads on.  pdftex on
    // `\message{[S-L1]}\scantokens{\endinput}\message{[S-rest]}`:
    // `(./s.tex [S-L1] [S-rest] [S-L2] [S-L3])`.
    let dir = scratch("scantokens");
    std::fs::write(
        dir.join("s.tex"),
        "\\def\\sone{1}\\scantokens{\\endinput}\\def\\srest{1}\n\\def\\stwo{2}\n\\def\\sthree{3}\n",
    )
    .unwrap();
    let source = format!("{PRELUDE}\\input s \\def\\mend{{1}}\n");
    let analysis = analyze_files(&source, &dir.join("main.tex"));
    assert_eq!(state(&analysis, &["sone", "srest", "stwo", "sthree", "mend"]), "sone=1 srest=1 stwo=1 sthree=1 mend=1");
}

#[test]
fn a_pseudo_file_opened_after_endinput_is_empty() {
    if !installed() {
        return;
    }
    // `pseudo_start` forces a line read, so the pending flag closes the
    // pseudo file before anything in it is scanned.  pdftex on
    // `\endinput\scantokens{\message{[ST-a]}}\message{[S7-rest]}` prints
    // neither `[ST-a]`, and reads t.tex to its end:
    // `(./t.tex [S7-L1] [S7-rest] [S7-L2] [S7-L3])`.
    let dir = scratch("pseudo");
    std::fs::write(
        dir.join("t.tex"),
        "\\def\\tone{1}\\endinput\\scantokens{\\def\\inpseudo{1}}\\def\\trest{1}\n\\def\\ttwo{2}\n",
    )
    .unwrap();
    let source = format!("{PRELUDE}\\input t \\def\\mend{{1}}\n");
    let analysis = analyze_files(&source, &dir.join("main.tex"));
    assert_eq!(
        state(&analysis, &["tone", "inpseudo", "trest", "ttwo", "mend"]),
        "tone=1 inpseudo=0 trest=1 ttwo=1 mend=1"
    );
}

#[test]
fn endinput_in_the_only_file_ends_the_run() {
    // pdftex on a main file whose first line holds `\endinput`: `[M-L1]
    // [M-rest]` and then `! Emergency stop.`, so line 2 is never read.
    let analysis = analyze("\\def\\one{1}\\endinput\\def\\rest{1}\n\\def\\two{2}\n");
    assert_eq!(state(&analysis, &["one", "rest", "two"]), "one=1 rest=1 two=0");
    // Nothing is corrupted: the state that was reached is still readable.
    assert!(diagnosed(&analysis, "unbalanced-file").is_empty());
    assert!(diagnosed(&analysis, "file-ended").is_empty());
}

#[test]
fn endinput_does_not_close_a_group() {
    if !installed() {
        return;
    }
    // Closing a file leaves the groups open, which satex reports where TeX
    // would report it at the end of the job (tex.web § 1335).
    let dir = scratch("group");
    std::fs::write(dir.join("g.tex"), "{\\def\\inner{1}\\endinput\\def\\grest{1}\n\\def\\gtwo{2}\n").unwrap();
    let source = format!("{PRELUDE}\\input g \\def\\mend{{1}}\n");
    let analysis = analyze_files(&source, &dir.join("main.tex"));
    assert_eq!(state(&analysis, &["inner", "grest", "gtwo", "mend"]), "inner=1 grest=1 gtwo=0 mend=1");
    let reported = diagnosed(&analysis, "unbalanced-file");
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(reported[0].contains("end of g.tex occurred inside a group at level 1"), "{}", reported[0]);
    assert!(reported[0].contains("opened by { at g.tex:1"), "{}", reported[0]);
}

#[test]
fn endinput_in_skipped_conditional_text_does_nothing() {
    // Skipped text is not expanded and no primitive in it runs (tex.web
    // § 366), so the file reads on.  pdftex on
    // `\iffalse\endinput\fi\message{[after]}` + a second line: `[after]`
    // and the second line too.
    let analysis = analyze("\\iffalse\\endinput\\fi\\def\\after{a}\n\\def\\two{2}\n");
    assert_eq!(state(&analysis, &["after", "two"]), "after=1 two=1");
    assert!(diagnosed(&analysis, "conditional-endinput").is_empty());
}

#[test]
fn endinput_under_an_undecided_condition_reads_on() {
    // One arm ends the file and the other does not, so satex reads the rest
    // on the path that did not end, and what only that path reaches is not
    // certain.
    let source = "\\ifnum\\pdfuniformdeviate2=0 \\endinput\\fi\\def\\after{a}\n\\def\\two{2}\n";
    let analysis = analyze(source);
    assert_eq!(state(&analysis, &["after", "two"]), "after=1 two=1");
    let two = definition(&analysis, "two").expect("read on the path that did not end");
    assert!(!two.certain, "a line only one path reads is not certain");
}

#[test]
fn endinput_past_the_choice_limit_is_reported() {
    // Past `limits.choices` only the first arm is read, and it ends the
    // file: what the other arm would have run is missing, which the run
    // says.
    let cfg = Config {
        load: Load { packages: false, classes: false, inputs: false, format: false, ..Default::default() },
        texmf: Texmf { kpsewhich: false, ..Default::default() },
        limits: satex::config::Limits { choices: 0, ..crate::support::config().limits },
        ..crate::support::config()
    };
    let source = "\\ifnum\\pdfuniformdeviate2=0 \\endinput\\fi\\def\\after{a}\n\\def\\two{2}\n";
    let analysis = Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg);
    assert_eq!(state(&analysis, &["after", "two"]), "after=1 two=0");
    let reported = diagnosed(&analysis, "undecided-condition");
    assert_eq!(reported.len(), 1, "{reported:?}");
    assert!(reported[0].contains("only its first arm is followed"), "{}", reported[0]);
}

#[test]
fn endinput_ends_a_package_read_by_usepackage() {
    if !installed() {
        return;
    }
    // pdflatex on a `.sty` with `\def\zeiA{1}\endinput\def\zeiRest{1}` and a
    // further line: `[A=Y][Rest=Y][B=N]`.
    let dir = scratch("sty");
    std::fs::write(
        dir.join("zei.sty"),
        "\\ProvidesPackage{zei}\n\\def\\zeiA{1}\\endinput\\def\\zeiRest{1}\n\\def\\zeiB{2}\n",
    )
    .unwrap();
    let source = "\\documentclass{article}\n\\usepackage{zei}\n\\begin{document}\\end{document}\n";
    let cfg = latex(&dir);
    let main = dir.join("main.tex");
    std::fs::write(&main, source).unwrap();
    let analysis = Machine::analyze(source, Some(&main), &cfg);
    assert_eq!(state(&analysis, &["zeiA", "zeiRest", "zeiB"]), "zeiA=1 zeiRest=1 zeiB=0");
    assert!(
        analysis.facts.loads.iter().any(|l| l.name == "zei"),
        "{:?}",
        analysis.facts.loads.iter().map(|l| l.name.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn endinput_ends_an_included_file() {
    if !installed() {
        return;
    }
    // `\include` is `\clearpage\@@input` (latex.ltx `\@include`), so the
    // rule is the file's.  pdflatex prints `[INC-L1] [INC-rest]` and then
    // `[AFTER-INCLUDE]`.
    let dir = scratch("include");
    std::fs::write(dir.join("zinc.tex"), "\\def\\incone{1}\\endinput\\def\\increst{1}\n\\def\\inctwo{2}\n").unwrap();
    let source =
        "\\documentclass{article}\n\\begin{document}\n\\include{zinc}\n\\def\\afterinclude{1}\n\\end{document}\n";
    let cfg = latex(&dir);
    let main = dir.join("main.tex");
    std::fs::write(&main, source).unwrap();
    let analysis = Machine::analyze(source, Some(&main), &cfg);
    assert_eq!(
        state(&analysis, &["incone", "increst", "inctwo", "afterinclude"]),
        "incone=1 increst=1 inctwo=0 afterinclude=1"
    );
}

#[test]
fn a_dtx_stops_at_an_endinput_line_and_keeps_one_mid_line() {
    // docstrip's `\processLine` stops at an `\endinput` of its own and does
    // not copy it (docstrip.dtx), so the code view ends there.  An
    // `\endinput` with code after it is ordinary source: it stays in the
    // view and the interpreter applies tex.web § 362 to it.
    let alone = "%    \\begin{macrocode}\n\\def\\a{1}\n\\endinput\n\\def\\b{2}\n%    \\end{macrocode}\n";
    let view = satex::literate::code_view(alone, &satex::literate::Guards::AllButDriver);
    assert!(view.contains("\\def\\a{1}"), "{view:?}");
    assert!(!view.contains("\\endinput"), "the line is not copied: {view:?}");
    assert!(!view.contains("\\def\\b{2}"), "nothing after it is copied: {view:?}");

    let inline = "%    \\begin{macrocode}\n\\def\\a{1}\\endinput\\def\\b{2}\n\\def\\c{3}\n%    \\end{macrocode}\n";
    let view = satex::literate::code_view(inline, &satex::literate::Guards::AllButDriver);
    assert!(view.contains("\\def\\a{1}\\endinput\\def\\b{2}"), "{view:?}");
    assert!(view.contains("\\def\\c{3}"), "the view keeps it, the interpreter ends the file: {view:?}");
}

#[test]
fn a_package_opened_on_the_same_line_takes_the_flag_over() {
    if !installed() {
        return;
    }
    // A package is read through `\@@input` like any other file, so the flag
    // reaches it.  pdflatex on a `zouter.sty` whose second line is
    // `\typeout{[O-L1]}\endinput\RequirePackage{zinner}\typeout{[O-rest]}`:
    // `[O-L1] [I-L1] [O-rest] [O-L2]`.  zinner.sty stops after one line.
    // The package cache cannot serve a file that has to stop there, so this
    // also pins that a pending flag reads the file itself.
    let dir = scratch("require");
    std::fs::write(
        dir.join("zouter.sty"),
        "\\ProvidesPackage{zouter}\n\\def\\oone{1}\\endinput\\RequirePackage{zinner}\\def\\orest{1}\n\\def\\otwo{2}\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("zinner.sty"),
        "\\ProvidesPackage{zinner}\\def\\ione{1}\n\\def\\itwo{2}\n\\def\\ithree{3}\n",
    )
    .unwrap();
    let source = "\\documentclass{article}\n\\usepackage{zouter}\n\\begin{document}\\end{document}\n";
    let cfg = latex(&dir);
    let main = dir.join("main.tex");
    std::fs::write(&main, source).unwrap();
    let analysis = Machine::analyze(source, Some(&main), &cfg);
    assert_eq!(
        state(&analysis, &["oone", "ione", "itwo", "ithree", "orest", "otwo"]),
        "oone=1 ione=1 itwo=0 ithree=0 orest=1 otwo=1"
    );
}
