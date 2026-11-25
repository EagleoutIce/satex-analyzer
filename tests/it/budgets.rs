//! Step-count budgets: a fixed problem that gets slow again fails here.
//! Counts are tokens, never wall-clock, so they do not depend on the machine.
//! Each limit is the value measured on the date noted, failing at +20%.
//! Document budgets run on a warm kernel cache and count analysis steps:
//! the kernel (`kernel_build` measures it once) and package loads are out.

use satex::analysis::machine::{Analysis, Machine};
use satex::project::config::{Cache, Load};
use std::path::PathBuf;

const SLACK: u64 = 120;

fn fresh(name: &str) -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/scratch/budgets").join(name);
    crate::support::scratch_dir(dir)
}

fn within(what: &str, steps: u64, measured: u64) {
    eprintln!("budget {what}: {steps} (measured {measured})");
    crate::support::record_steps(&format!("budgets::{}", what.replace(' ', "_")), steps);
    assert!(steps * 100 <= measured * SLACK, "{what}: {steps} steps, measured {measured}, fail at +20%");
}

fn analysis_steps(a: &Analysis) -> u64 {
    a.steps - a.load_steps
}

fn run(name: &str, source: &str) -> Analysis {
    run_at(name, source, None)
}

fn run_at(name: &str, source: &str, path: Option<&std::path::Path>) -> Analysis {
    let cfg = satex::project::config::Config {
        // package caches only: a stored whole run would serve the work these budgets measure
        cache: Cache { dir: Some(fresh(name)), document: false, ..Default::default() },
        load: Load { packages: true, classes: true, ..Default::default() },
        ..crate::support::config()
    };
    // Documents are measured against a warm kernel; `kernel_build` measures the kernel.
    crate::support::seed_kernel(&cfg);
    Machine::analyze(source, path, &cfg)
}

fn budget(name: &str, source: &str, measured: u64) -> Option<Analysis> {
    if !crate::support::installed() {
        return None;
    }
    let analysis = run(name, source);
    eprintln!("budget {name}: {} steps, {} loading", analysis.steps, analysis.load_steps);
    without(name, &analysis, &["line-budget-cut", "budget-exhausted"]);
    within(name, analysis_steps(&analysis), measured);
    Some(analysis)
}

fn without(name: &str, analysis: &Analysis, codes: &[&str]) {
    for code in codes {
        assert!(analysis.facts.diagnostics.iter().all(|d| d.code != *code), "{name}: {code}");
    }
}

const TIKZ: &str = "\\documentclass{article}\\usepackage{tikz}";

// measured 2026-10-03, fail at +20%
const KERNEL: u64 = 620765941;
// analysis steps (package loads excluded), measured 2026-10-17
const ROUNDED: u64 = 1667940;
const CLOUD: u64 = 1953470;
const PARAGRAPH: u64 = 1456856;
const DATETIME: u64 = 1734775;
/// Target 5773865; measured 14815177 on 2026-11-04, see plan.
const METADATA: u64 = 14_815_177;
/// What `\DocumentMetadata` cost in all on 2026-10-03, nearly all of it
/// tagpdf's loading: a load that runs unknown values (`\int_rand:n`) is
/// analysis too, so its loading has a budget of its own.
/// Target 8_400_000; measured 87_207_865 on 2026-11-04, see plan.
const METADATA_LOADING: u64 = 87_207_865;
const FLAG: u64 = 852156;
const NOTEBOOK: u64 = 3669404;
const OVERFULL: u64 = 2287795;
// 2026-11-17: calls that pop or peek the parser stacks no longer summarize (unsound), +31% steps
// 2026-11-22: a typed rest that may be empty is read a token per argument, never whole (sound),
// so pgfmath's digit walks take a pass per digit. Target 1_740_671.
const PGFMATH_TYPED: u64 = 2_830_219;
/// Measured 2026-11-24 on TeX Live 2025 (2022: 657k, 2019: 132k).
const PGFMATH_LOOP: u64 = 1_104_426;
const GRAPHIC: u64 = 1181589;
const REGEX_SCAN: u64 = 10924212;

#[test]
fn kernel_build() {
    if !crate::support::installed() {
        return;
    }
    let dir = fresh("kernel");
    let cfg = satex::project::config::Config {
        cache: Cache { dir: Some(dir.clone()), rebuild: true, ..Default::default() },
        ..crate::support::config()
    };
    let analysis = Machine::analyze("", Some(&dir.join("kernel.tex")), &cfg);
    let format = analysis.format.as_ref().expect("a format");
    assert!(!format.cached, "the kernel was read from a cache");
    assert!(format.complete(), "the kernel was not read to the end");
    let cut: Vec<_> = analysis.facts.diagnostics.iter().filter(|d| d.code == "line-budget-cut").collect();
    assert!(cut.is_empty(), "the kernel build was cut: {cut:?}");
    within("kernel", format.steps, KERNEL);
}

/// A list the run no longer records (`limits.list_items` reached) has no exact lines or
/// sizes from there on. That loss was silent: nothing named the setting, so a very large
/// document quietly lost detail in its later part. The finding names it.
#[test]
fn recorded_lists_past_their_limit_are_reported() {
    if !crate::support::installed() {
        return;
    }
    let source = "\\documentclass{article}\n\\begin{document}\nSome words of a paragraph, enough of them to record.\n\\end{document}\n";
    let named = |cfg: &satex::project::config::Config| {
        let analysis = Machine::analyze_once(source, None, cfg);
        analysis.facts.diagnostics.iter().any(|d| d.code == "limit-reached" && d.message.contains("limits.list_items"))
    };
    let mut cfg = crate::support::config();
    assert!(!named(&cfg), "the default limit was reached by one paragraph");
    cfg.limits.list_items = 8;
    assert!(named(&cfg), "the reached limit was not named");
}

// termination: a kernel build is bounded by `limits.seconds` too.
#[test]
fn a_kernel_build_over_the_seconds_limit_stops_with_the_limit_named() {
    // A kernel without expl3 (before 2020-02) is built well within the smallest limit, one second.
    if !crate::support::installed() || !crate::support::kernel_has("\\ExplSyntaxOn") {
        return;
    }
    let dir = fresh("kernel-seconds");
    let mut cfg = satex::project::config::Config {
        cache: Cache { dir: Some(dir.clone()), rebuild: true, ..Default::default() },
        ..crate::support::config()
    };
    cfg.limits.seconds = 1;
    let start = std::time::Instant::now();
    let analysis = Machine::analyze("", Some(&dir.join("kernel.tex")), &cfg);
    assert!(start.elapsed().as_secs() < 20, "the build took {:?}", start.elapsed());
    let hit =
        analysis.facts.diagnostics.iter().any(|d| d.code == "limit-reached" && d.message.contains("limits.seconds"));
    assert!(hit, "no seconds limit named");
    assert!(!analysis.format.as_ref().expect("a format").complete(), "the kernel was read to the end");
}

#[test]
fn tikz_rounded_node() {
    let src = format!(
        "{TIKZ}\\begin{{document}}\\begin{{tikzpicture}}\\node[text width=4cm, rounded corners, draw] {{p}};\\end{{tikzpicture}}\\end{{document}}"
    );
    budget("rounded", &src, ROUNDED);
}

#[test]
fn tikz_cloud() {
    let src = format!(
        "{TIKZ}\\usetikzlibrary{{shapes.symbols}}\\begin{{document}}\\begin{{tikzpicture}}\\node[draw,align=center,shape=cloud] {{Feat.\\\\Identif.}};\\end{{tikzpicture}}\\end{{document}}"
    );
    budget("cloud", &src, CLOUD);
}

#[test]
fn tikz_paragraph_node() {
    let src = format!(
        "{TIKZ}\\begin{{document}}\\begin{{tikzpicture}}\\node[draw,text width=9mm] (b) {{x}};\\end{{tikzpicture}}\\end{{document}}"
    );
    budget("paragraph", &src, PARAGRAPH);
}

/// A text width below `\\parindent` plus `~~` is one overfull line, so the node's
/// size is exact and no path splits in `\\pgfpointnormalised`.
#[test]
fn tikz_overfull_signal_bend() {
    let src = format!(
        "{TIKZ}\\usetikzlibrary{{shapes.symbols}}\\begin{{document}}\\begin{{tikzpicture}}\\node[signal,text width=5mm,gray] (b) at (0,1) {{~~}};\\node (a) at (0,0) {{}};\\draw (a) to[bend right] (b);\\end{{tikzpicture}}\\end{{document}}"
    );
    budget("overfull", &src, OVERFULL);
}

#[test]
fn overfull_paragraph_height_is_exact() {
    if !crate::support::installed() {
        return;
    }
    let src = "\\documentclass{article}\\begin{document}\\setbox0\\vbox{\\hsize14.2pt ~~\\par}\\ifdim\\ht0=12345pt \\UNDEF\\fi\\end{document}";
    let analysis = run("overfull-height", src);
    assert!(analysis.facts.diagnostics.iter().all(|d| d.code != "assumed-compiles"));
}

#[test]
fn datetime2_hyperref_title() {
    budget(
        "datetime2",
        "\\documentclass{article}\\usepackage[en-US]{datetime2}\\usepackage{hyperref}\\date{\\today}\\begin{document}\\maketitle\\end{document}",
        DATETIME,
    );
}

/// pgfmath parses of a bounded number and of `rand`, whose digits it takes
/// apart one by one (`\pgfmathrand@@`): the paths a digit split makes stay
/// few, and a number nothing bounds is not split at all.
#[test]
fn pgfmath_parse_of_typed_numbers() {
    let analysis = budget(
        "pgfmath-typed",
        "\\documentclass{article}\\usepackage{pgfmath}\\newcount\\k\\loop\\pgfmathparse{rand}\\pgfmathparse{\\pdfuniformdeviate 1000 + 1}\\advance\\k1 \\ifnum\\k<20 \\repeat\\begin{document}x\\end{document}",
        PGFMATH_TYPED,
    );
    if let Some(analysis) = analysis {
        without("pgfmath-typed", &analysis, &["limit-reached", "repeated-failure"]);
    }
}

/// Intuition: a `\pgfmathparse` met again in a loop, whose run forks on an unknown
/// number, applies what its earlier runs left (`call-learned-effect`) instead of
/// parsing again. Bug: the runs never taught their effect, since the `rand` run wrote
/// the seed and the forked run left `\pgfmathresult` vague. TeX Live 2022's parser then
/// joined paths at different parser states: 506M tokens for two passes.
#[test]
fn a_parse_in_a_loop_learns_its_effect() {
    let analysis = budget(
        "pgfmath-loop",
        "\\documentclass{article}\\usepackage{pgfmath}\\newcount\\k\\loop\\pgfmathparse{rand}\\pgfmathparse{\\pdfuniformdeviate 1000 + 1}\\advance\\k1 \\ifnum\\k<2 \\repeat\\begin{document}x\\end{document}",
        PGFMATH_LOOP,
    );
    if let Some(analysis) = analysis {
        without("pgfmath-loop", &analysis, &["limit-reached", "repeated-failure"]);
    }
}

/// The steps a warm run runs: the second of two in one cache directory,
/// less what the package caches replayed.
fn warm_steps(name: &str, source: &str) -> Option<u64> {
    if !crate::support::installed() {
        return None;
    }
    let cfg = crate::support::warm_config(name);
    let cold = Machine::analyze(source, None, &cfg);
    let warm = Machine::analyze(source, None, &cfg);
    eprintln!("budget {name}: {} steps, {} served, cold {}", warm.steps, warm.served_steps, cold.steps);
    without(name, &warm, &["line-budget-cut", "budget-exhausted"]);
    // a gap may repeat, but never past the bound: a run that pays for each repeat is the budget's real cost
    let bound = cfg.limits.repeat_failures;
    let over: Vec<_> = warm.facts.diagnostics.iter().filter(|d| d.count > bound).map(|d| (d.code, d.count)).collect();
    assert_eq!(over, Vec::new(), "{name}: gaps past the bound of {bound}");
    Some(warm.steps - warm.served_steps)
}

/// What a warm run of a short document may run, its loads replayed. A
/// replay leaves the lists as the read did, so what the loads put on the
/// main vertical list is worked through again: the cold run's cost.
// With `\DocumentMetadata`, what tagpdf needs: as `WARM_METADATA`, its management is what costs.
const WARM_TAGPDF: u64 = 19_400_000;
const WARM_METADATA: u64 = 19_340_000;

#[test]
fn document_metadata_warm() {
    let source = "\\DocumentMetadata{lang=en}\n\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n";
    if let Some(steps) = warm_steps("metadata-warm", source) {
        within("metadata warm", steps, WARM_METADATA);
    }
}

#[test]
fn tagpdf_warm() {
    // tagpdf needs the PDF management `\\DocumentMetadata` starts: without it pdflatex fails too.
    let source =
        "\\DocumentMetadata{}\n\\documentclass{article}\n\\usepackage{tagpdf}\n\\begin{document}\nx\n\\end{document}\n";
    if let Some(steps) = warm_steps("tagpdf-warm", source) {
        within("tagpdf warm", steps, WARM_TAGPDF);
    }
}

/// What the loads of `testphase=latest` may run warm: only the `\\RequirePackage`
/// wrappers (the kernel's hook names per file) and the `.def`/`.ltx` inputs.
/// Measured 2026-11-20: 4.6M of 57M, tagpdf and latex-lab replay from their caches.
const WARM_TESTPHASE_LOADING: u64 = 4_620_000;

/// Intuition: a tagged document loads tagpdf, latex-lab and the PDF management
/// before `\\documentclass`. Warm, they replay from the package caches like any
/// package. The bug it pins: the warm `--stats` line showed 57M loading tokens,
/// all credited by replayed segments, and read as if every run loaded them again.
/// Cold and warm must find the same.
#[test]
fn testphase_latest_loads_from_cache_warm() {
    // TeX Live 2022 has no `testphase=latest` ("LaTeX-lab package 'latest' not found"), 2019 no `\DocumentMetadata`.
    if !crate::support::installed_file_has("latex-lab-testphase-latest.sty", "") {
        return;
    }
    let source = "\\DocumentMetadata{testphase=latest,lang=en}\n\\documentclass{article}\n\\begin{document}\nx\n\\end{document}\n";
    let cfg = crate::support::warm_config("testphase-warm");
    let cold = Machine::analyze(source, None, &cfg);
    let warm = Machine::analyze(source, None, &cfg);
    let codes = |a: &Analysis| a.facts.diagnostics.iter().map(|d| (d.code, d.count)).collect::<Vec<_>>();
    assert_eq!(codes(&cold), codes(&warm), "cold and warm differ");
    assert_eq!(cold.facts.occurrences.len(), warm.facts.occurrences.len(), "cold and warm differ");
    assert_eq!(cold.served_steps, 0, "the cold run has no cache to replay");
    assert!(warm.served_steps * 10 >= warm.load_steps * 9, "warm loads not replayed: {}", warm.served_steps);
    within("testphase warm loading", warm.load_steps - warm.served_steps, WARM_TESTPHASE_LOADING);
}

#[test]
fn document_metadata() {
    let source = "\\DocumentMetadata{}\\documentclass{article}\\begin{document}x\\end{document}";
    if let Some(analysis) = budget("metadata", source, METADATA) {
        within("metadata loading", analysis.load_steps, METADATA_LOADING);
    }
}

#[test]
fn expl3_flag_unknown_times() {
    // A flag raised an unknown number of times in the preamble: the height
    // loop meets its head with the counter read as unknown digits.
    let analysis = budget(
        "flag",
        "\\documentclass{article}\n\\ExplSyntaxOn\n\\flag_new:n{test}\n\\int_step_inline:nn{\\pdfuniformdeviate 9}{\\flag_raise:n{test}}\n\\flag_clear:n{test}\n\\ExplSyntaxOff\n\\begin{document}x\\end{document}",
        FLAG,
    );
    if let Some(analysis) = analysis {
        without("flag", &analysis, &["limit-reached", "repeated-failure"]);
    }
}

/// An unknown font size selects one font of unknown metrics, not a path per
/// size range: `\textrm` and `\color` in math cost what they cost with a known size.
#[test]
fn an_unknown_font_size_does_not_multiply_paths() {
    if !crate::support::installed() {
        return;
    }
    let body = "\\(\\textrm{x}^{\\color{gray}y}\\)".repeat(20);
    let doc = |size: &str| {
        format!(
            "\\documentclass{{article}}\\usepackage{{xcolor}}\\begin{{document}}\\count255={size}\\fontsize{{\\the\\count255}}{{12}}\\selectfont {body}\\end{{document}}"
        )
    };
    let known = run("font-known", &doc("11 "));
    let unknown = run("font-unknown", &doc("\\pdfuniformdeviate 20 "));
    let (known, unknown_steps) = (analysis_steps(&known), analysis_steps(&unknown));
    eprintln!("analysis steps: known {known} unknown {unknown_steps}");
    assert!(unknown_steps < known * 2, "{unknown_steps} vs {known}");
    assert!(!unknown.facts.diagnostics.iter().any(|d| d.code == "budget-exhausted"));
}

/// A document that only grows stays under `limits.memory`: the run stops
/// itself there (budget exhausted), and the process holds no more than the
/// limit and a half besides what it held before the run.
#[test]
fn a_growing_document_stays_under_its_memory_limit() {
    const LIMIT: u64 = 64_000_000;
    let dir = fresh("memory");
    let seed = "0123456789".repeat(1000);
    let source = format!(
        "\\catcode`\\{{=1 \\catcode`\\}}=2 \\catcode`\\#=6\n\\def\\b{{{seed}}}\n\\def\\l{{\\advance\\count1 1 \\expandafter\\edef\\csname m\\the\\count1\\endcsname{{\\b}}\\l}}\\l\n"
    );
    std::fs::write(dir.join("grow.tex"), source).expect("write document");
    let output = crate::support::satex()
        .args(["lint", "--no-config", "--stats", "-f", "grow.tex", "--max-memory", "64M"])
        .args(["--set", "load.format=false", "--set", "limits.seconds=0"])
        .current_dir(&dir)
        .output()
        .expect("satex runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.code().is_some(), "killed: {stderr}");
    assert!(stderr.contains("limits.memory"), "the memory limit did not stop the run: {stderr}");
    let peak: u64 = stderr
        .split_whitespace()
        .zip(stderr.split_whitespace().skip(1))
        .find_map(|(n, unit)| (unit == "MiB").then(|| n.parse().ok()).flatten())
        .expect("a peak in --stats");
    // Resident memory: the limit and a half, and the binary itself.
    assert!(peak << 20 <= LIMIT * 3 / 2 + (48 << 20), "peak {peak} MiB past a 64M limit");
}

/// A build's second run holds about what its first held, not both. The first run makes many
/// definitions and writes an `.aux` the second reads on its first line, so the second runs it
/// all again. Pins the build keeping run 1 three times while run 2 ran: its analysis (the
/// fallback report), its end and its last checkpoint (to meet it again), all sharing its tables.
/// On a thesis run 1's 4 GB stayed under run 2, which then stopped at the 8 GiB limit.
#[test]
fn a_build_run_holds_no_earlier_run() {
    let dir = fresh("memory-build");
    let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6\n\
        \\openin1=\\jobname.aux \\ifeof1 \\else\\closein1 \\input \\jobname.aux \\fi\n\
        \\def\\l{\\advance\\count1 1 \\expandafter\\def\\csname m\\the\\count1\\endcsname{x}\\ifnum\\count1<300000 \\expandafter\\l\\fi}\\l\n\
        \\immediate\\openout2=\\jobname.aux \\immediate\\write2{\\relax}\\immediate\\closeout2\n";
    std::fs::write(dir.join("doc.tex"), source).expect("write document");
    let peak = |runs: &str| {
        let output = crate::support::satex()
            .args(["lint", "--no-config", "--stats", "-v", "-f", "doc.tex"])
            .args(["--set", "load.format=false", "--set", "limits.seconds=0", "--set", "cache.document=false"])
            .args(["--set", &format!("limits.build_runs={runs}")])
            .current_dir(&dir)
            .output()
            .expect("satex runs");
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let peak: u64 = stderr
            .split_whitespace()
            .zip(stderr.split_whitespace().skip(1))
            .find_map(|(n, unit)| (unit == "MiB").then(|| n.parse().ok()).flatten())
            .expect("a peak in --stats");
        (peak, stderr)
    };
    let (one, _) = peak("1");
    let (two, stderr) = peak("2");
    assert!(stderr.contains("build run 2 took"), "no second run: {stderr}");
    // One run, its encoded report and the allocator's slack: 1.5 times, both runs held 2.3 times.
    assert!(two * 5 <= one * 9, "two runs peak at {two} MiB, one at {one} MiB");
}

/// A build whose second run a budget stops reports its first run, now kept encoded beside the
/// second ([`a_build_run_holds_no_earlier_run`]): run 1 defines `\First`; run 2 reads the `.aux`
/// that run 1 wrote and loops past `limits.steps`. The report is run 1's, decoded whole, and
/// says that run 2 was stopped.
#[test]
fn a_build_stopped_in_its_second_run_reports_its_first() {
    let dir = fresh("build-fallback");
    let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6\n\
        \\openin1=\\jobname.aux \\ifeof1 \\def\\First{}\\else\\closein1 \\input \\jobname.aux \\fi\n\
        \\ifx\\again\\undefined\\else\\def\\l{\\advance\\count1 1 \\ifnum\\count1<100000000 \\expandafter\\l\\fi}\\l\\fi\n\
        \\immediate\\openout2=\\jobname.aux \\immediate\\write2{\\def\\noexpand\\again{}}\\immediate\\closeout2\n";
    let main = dir.join("doc.tex");
    std::fs::write(&main, source).expect("write document");
    let mut cfg = satex::project::config::Config {
        cache: Cache { dir: Some(dir.clone()), document: false, ..Default::default() },
        load: Load { format: false, ..Default::default() },
        ..crate::support::config()
    };
    (cfg.limits.steps, cfg.limits.seconds) = (1_000_000, 0);
    let analysis = Machine::analyze_build(source, Some(&main), &cfg);
    assert!(crate::support::defined(&analysis, "First"), "the report is not run 1's");
    let said = |d: &satex::analysis::facts::Diagnostic| d.code == "budget-exhausted" && d.message.contains("run 2");
    assert!(analysis.facts.diagnostics.iter().any(said), "run 2 was not stopped: {:?}", analysis.limits);
}

/// A document that only makes small things (a name and a one-token body
/// per round) still stops at `limits.memory` with that verdict, holding no
/// more than the limit past what an empty run holds. Small allocations
/// take about twice what they ask of the system allocator: counted at
/// their size alone, such a run held twice its limit (13 GB on a tikz
/// picture under 8 GiB) or aborted past the hard ceiling.
#[test]
fn many_small_allocations_stop_at_the_memory_limit() {
    const LIMIT: u64 = 64_000_000;
    let dir = fresh("memory-small");
    let preamble = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6\n";
    let grow = "\\def\\l{\\advance\\count1 1 \\expandafter\\def\\csname m\\the\\count1\\endcsname{x}\\l}\\l\n";
    std::fs::write(dir.join("empty.tex"), preamble).expect("write document");
    std::fs::write(dir.join("small.tex"), format!("{preamble}{grow}")).expect("write document");
    let run = |file: &str| {
        let output = crate::support::satex()
            .args(["lint", "--no-config", "--stats", "-f", file, "--max-memory", &LIMIT.to_string()])
            .args(["--set", "load.format=false", "--set", "limits.seconds=0"])
            .current_dir(&dir)
            .output()
            .expect("satex runs");
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        assert!(output.status.code().is_some(), "killed: {stderr}");
        let peak: u64 = stderr
            .split_whitespace()
            .zip(stderr.split_whitespace().skip(1))
            .find_map(|(n, unit)| (unit == "MiB").then(|| n.parse().ok()).flatten())
            .expect("a peak in --stats");
        (peak << 20, stderr)
    };
    let (empty, _) = run("empty.tex");
    let (peak, stderr) = run("small.tex");
    assert!(stderr.contains("limits.memory"), "the memory limit did not stop the run: {stderr}");
    // A check runs every few hundred tokens: a quarter of the limit covers what grows between.
    assert!(
        peak <= empty + LIMIT * 5 / 4,
        "peak {} MiB past a {LIMIT}-byte limit over {} MiB",
        peak >> 20,
        empty >> 20
    );
}

/// A path dropped for a certain error in a package (here an image of unknown
/// width that may be zero, as `\includegraphics` divides by: scaled to a random
/// width below 100sp, since satex reads the logo's own box exactly) waits until the run
/// ends. The document's own errors are kept, so the check stands in a package. The paths a later picture parts into are joined from where they
/// part, not from that error: else every join compares all the document
/// did since.
#[test]
fn a_dropped_error_path_does_not_weigh_on_later_joins() {
    if !crate::support::installed() {
        return;
    }
    let doc = |before: &str| {
        format!(
            "{TIKZ}\\usetikzlibrary{{positioning}}{before}\\begin{{document}}\\begin{{tikzpicture}}\\node (a) {{N}};\\node[below=1mm of a, text width=4cm, rounded corners, draw] {{\\begin{{obeylines}}p(x)\\\\r(x)\\end{{obeylines}}}};\\end{{tikzpicture}}\\end{{document}}"
        )
    };
    let logo = concat!(env!("CARGO_MANIFEST_DIR"), "/doc/logo/logo.pdf");
    let error = format!(
        "\\pdfximage width \\pdfuniformdeviate 100sp {{{logo}}}\\setbox0\\hbox{{\\pdfrefximage\\pdflastximage}}\\ifdim\\wd0=0pt \\errmessage{{zero}}\\fi"
    );
    let sty = fresh("dropped-package").join("zerocheck");
    std::fs::write(sty.with_extension("sty"), error).expect("write package");
    // The document stands in a directory of its own, so the package is not its file.
    let main = fresh("dropped-document").join("main.tex");
    let plain = run_at("dropped-plain", &doc(""), Some(&main));
    let dropped =
        run_at("dropped-error", &doc(&format!("\\usepackage{{{}}}", crate::support::tex_path(&sty))), Some(&main));
    let codes = |a: &Analysis| {
        use satex::analysis::facts::Severity::{Unsupported, Warning};
        let faults = a.facts.diagnostics.iter().filter(|d| matches!(d.severity, Unsupported | Warning));
        let mut codes: Vec<&str> = faults.map(|d| d.code).collect();
        codes.sort_unstable();
        codes
    };
    assert!(dropped.facts.diagnostics.iter().any(|d| d.code == "assumed-compiles"), "the error path is dropped");
    assert_eq!(codes(&plain), codes(&dropped));
    eprintln!("joined slots: plain {} dropped {}", plain.joined_slots, dropped.joined_slots);
    assert!(
        dropped.joined_slots * 100 <= plain.joined_slots * SLACK + 1000,
        "{} vs {}",
        dropped.joined_slots,
        plain.joined_slots
    );
}

/// `--stats` names the lines the run spent its steps on: a `\loop` shows.
#[test]
fn stats_hotspots_show_the_loop_line() {
    let dir = fresh("hotspots");
    let source = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6\n\\count1=0\n\\def\\l{\\advance\\count1 1 \\ifnum\\count1<500 \\expandafter\\l\\fi}\\l\n";
    std::fs::write(dir.join("loop.tex"), source).expect("write document");
    let output = crate::support::satex()
        .args(["lint", "--no-config", "--stats", "-f", "loop.tex", "--set", "load.format=false"])
        .current_dir(&dir)
        .output()
        .expect("satex runs");
    let stderr = String::from_utf8_lossy(&output.stderr);
    let steps = stderr.lines().find(|l| l.contains("hot steps")).unwrap_or_else(|| panic!("no hot steps: {stderr}"));
    assert!(steps.contains("loop.tex:3"), "the loop's line is not a hotspot: {steps}");
}

/// `\includegraphics[width=…]` of a PDF: graphics normalizes the scale
/// factor's unknown digits one digit at a time (`\GPT@Reverse`,
/// `\GPT@ZapLeadingZeros`), which must meet its fixpoint and not run away.
#[test]
fn includegraphics_with_a_width_is_cheap() {
    let analysis = budget(
        "graphic",
        "\\documentclass{article}\\usepackage{graphicx}\\begin{document}\\includegraphics[width=3cm]{example-image}\\end{document}",
        GRAPHIC,
    );
    if let Some(analysis) = analysis {
        without(
            "graphic",
            &analysis,
            &["limit-reached", "repeated-failure", "runaway-argument", "extra-right-brace", "recursion-widened"],
        );
    }
}

/// A notebook cell: a positioned tikz node holding an inline listing, in a
/// figure. Its pgfmath runs over unknown sizes, whose joined results must
/// not leave conditionals open that keep later paths from joining.
#[test]
fn notebook_cell_with_an_inline_listing() {
    let cell = |n: u32, prev: u32, code: &str| {
        format!(
            "\\node[below=1mm of {prev}, inner sep=2mm, text width=4cm, rounded corners, draw] ({n}) {{\\small\\begin{{obeylines}}\\lstinline{{{code}}}\\end{{obeylines}}}};\\node[below left=0.5mm and 0mm of {n}.north east] {{\\footnotesize $A_{n}$}};"
        )
    };
    let src = format!(
        "{TIKZ}\\usetikzlibrary{{positioning}}\\usepackage{{listings}}\\begin{{document}}\\begin{{figure}}\\begin{{tikzpicture}}\\node (0) {{N}};{}{}{}\\end{{tikzpicture}}\\caption{{Simple}}\\end{{figure}}After.\\end{{document}}",
        cell(1, 0, "print(x)"),
        cell(2, 1, "x <- x + 1"),
        cell(3, 2, "x <- 2"),
    );
    budget("notebook_cell", &src, NOTEBOOK);
}

/// Capabilities: termination.
/// A run past `limits.seconds` stops there: the clock is read every
/// `CHECK_INTERVAL` steps, whichever step lands on the interval.
#[test]
fn a_seconds_limit_stops_the_run() {
    use satex::{
        plugins::Kernel,
        project::config::{Config, Engine},
    };
    let mut cfg = Config { engine: Some(Engine::PdfTeX), kernel: Some(Kernel::None), ..crate::support::config() };
    (cfg.limits.steps, cfg.limits.line_steps, cfg.limits.site_expansions) = (u64::MAX, 0, u32::MAX);
    (cfg.limits.facts, cfg.limits.vertices, cfg.limits.seconds) = (usize::MAX, usize::MAX, 1);
    let started = std::time::Instant::now();
    // Millions of tokens, read in seconds: only the clock stops it early.
    let source = "\\relax\n".repeat(8_000_000);
    let analysis = Machine::analyze(&source, None, &cfg);
    assert!(started.elapsed().as_secs() < 20, "ran {:?}", started.elapsed());
    assert!(
        analysis.facts.diagnostics.iter().any(|d| d.code == "budget-exhausted" && d.message.contains("limits.seconds")),
        "{} steps in {:?}: {:?}",
        analysis.steps,
        started.elapsed(),
        analysis.facts.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

/// Capabilities: termination, package interpreters. A concrete l3regex scan
/// over many lines, as a documentation package reads defaults from its own
/// source: deep input under every call, one path. Its steps are TeX's own
/// work; the shape memo must leave them and the end unchanged.
#[test]
fn expl3_regex_scan_of_lines() {
    let source = "\\documentclass{article}\n\\ExplSyntaxOn\n\\seq_new:N \\l_m_seq \\prop_new:N \\g_d_prop\n\\cs_new_protected:Npn \\scan:n #1 { \\regex_extract_once:nnNT { \\A \\s* ([^=]+?) \\s* / \\. default \\s* = \\s* (.*?) \\s* \\Z } {#1} \\l_m_seq { \\prop_gput:Nxx \\g_d_prop { \\seq_item:Nn \\l_m_seq { 2 } } { \\seq_item:Nn \\l_m_seq { 3 } } } }\n\\int_step_inline:nn { 40 } { \\scan:n { key ##1 /.default = value~##1 } }\n\\ExplSyntaxOff\n\\begin{document}x\\end{document}";
    if let Some(analysis) = budget("regex-scan", source, REGEX_SCAN) {
        without("regex-scan", &analysis, &["limit-reached", "endless-loop"]);
    }
}

/// `--timings` names what a slow run spent its steps on: the macros
/// expanded most and the document lines whose reading they followed.
#[test]
fn timings_name_the_costly_line_and_macro() {
    let cfg = satex::project::config::Config { timings: true, ..crate::support::bare() };
    let source = format!("{}\\def\\x#1{{#1#1#1#1}}\n\\x{{\\x{{\\x{{\\relax}}}}}}\n\\relax\n", crate::support::PRELUDE);
    let analysis = satex::analysis::machine::Machine::analyze(&source, None, &cfg);
    let sites = analysis.timings.top_sites(1);
    assert_eq!(sites.first().map(|((_, line), _)| *line), Some(2), "{sites:?}");
    let calls = analysis.timings.top_calls(1);
    assert_eq!(calls.first().map(|(sym, count)| (analysis.interner.name(*sym), *count)), Some(("x", 21)));
}

/// A run a budget stops before a LaTeX document's body began has not read what
/// the document defines, often not even the kernel: findings like "`\newcommand`
/// is never defined" would be false. Only why it stopped is shown.
#[test]
fn a_run_stopped_before_the_body_reports_only_the_stop() {
    let mut cfg = crate::support::config();
    cfg.limits.steps = 20_000;
    let analysis = Machine::analyze(
        "\\documentclass{article}\n\\newcommand\\x{y}\n\\begin{document}\n\\x\n\\end{document}\n",
        None,
        &cfg,
    );
    assert!(analysis.exhausted, "the budget stops it");
    let codes: Vec<String> =
        satex::linter::lint(&analysis).iter().filter_map(|r| r["code"].as_str().map(str::to_string)).collect();
    assert!(codes.iter().all(|c| c == "limit-reached" || c == "analysis-ended-early"), "{codes:?}");
}
