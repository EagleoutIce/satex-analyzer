//! Cold against warm over whole documents: a run that builds its package caches, the run served
//! from them, and a run served from the kept analysis must say the same, row for row.

use std::path::{Path, PathBuf};

use crate::support::installed;
use satex::analysis::machine::{Analysis, Machine};

fn at(analysis: &Analysis, span: satex::engine::tex::Span) -> String {
    format!("{}:{}:{}", analysis.file_name(span.file), span.line, span.col)
}

/// Everything a run said, in order: diagnostics, definitions, uses, occurrences and its steps.
pub fn said(analysis: &Analysis) -> Vec<String> {
    let name = |sym| analysis.interner.name(sym).to_string();
    let f = &analysis.facts;
    let diagnostics = f
        .diagnostics
        .iter()
        .map(|d| format!("{:?}|{}|{}|{}|{}", d.severity, d.code, at(analysis, d.span), d.count, d.message));
    let defs = f
        .defs
        .iter()
        .map(|d| format!("def|{}|{}|{}|{}|{}", name(d.name), name(d.by), d.tag, at(analysis, d.span), d.certain));
    let uses = f
        .expansions
        .iter()
        .map(|e| format!("use|{}|{}|{}|{:?}|{:?}", name(e.name), at(analysis, e.span), e.count, e.meaning, e.mode));
    let occurrences = f.occurrences.iter().map(|o| format!("occ|{:?}|{}|{}", o.kind, o.key, at(analysis, o.span)));
    let summary = [format!("steps|{}", analysis.steps), format!("typeset|{}", analysis.typeset)];
    diagnostics.chain(defs).chain(uses).chain(occurrences).chain(summary).collect()
}

/// The rows only one of two runs has, as a short report; empty when they agree.
pub fn difference(a: &[String], b: &[String]) -> String {
    if a == b {
        return String::new();
    }
    let count = |rows: &[String]| {
        let mut m = std::collections::BTreeMap::<String, i64>::new();
        rows.iter().for_each(|r| *m.entry(r.clone()).or_default() += 1);
        m
    };
    let (x, y) = (count(a), count(b));
    let mut out = Vec::new();
    for (row, n) in &x {
        if y.get(row) != Some(n) {
            out.push(format!("< {row}"));
        }
    }
    for (row, n) in &y {
        if x.get(row) != Some(n) {
            out.push(format!("> {row}"));
        }
    }
    if out.is_empty() {
        out.push("order only".into());
    }
    out.join("\n")
}

/// The file of a fixture that holds its `\documentclass`.
fn main_of(root: &Path) -> Option<PathBuf> {
    let mut files: Vec<_> = std::fs::read_dir(root).ok()?.flatten().map(|e| e.path()).collect();
    files.sort_by_key(|p| (p.file_stem().is_none_or(|s| s != "main"), p.clone()));
    files.into_iter().find(|p| {
        p.extension().is_some_and(|e| e == "tex")
            && std::fs::read_to_string(p)
                .is_ok_and(|t| t.contains("\\documentclass") || t.contains("\\DocumentMetadata"))
    })
}

/// One fixture three ways from an empty cache holding only the kernel: packages built,
/// packages served, the kept analysis served. Returns the report of what differs.
fn three_ways(root: &Path, tag: &str) -> String {
    let Some(path) = main_of(root) else { return String::new() };
    let source = std::fs::read_to_string(&path).expect("fixture source");
    let mut cfg = crate::support::discover(root);
    cfg.cache.dir = Some(crate::support::scratch_dir(
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/scratch/coldwarm").join(tag),
    ));
    crate::support::seed_kernel(&cfg);
    cfg.limits.seconds = 0;
    cfg.cache.document = false;
    let t = std::time::Instant::now();
    let cold = said(&Machine::analyze(&source, Some(&path), &cfg));
    let t_cold = t.elapsed().as_secs_f32();
    let warm = said(&Machine::analyze(&source, Some(&path), &cfg));
    cfg.cache.document = true;
    let keep = said(&Machine::analyze(&source, Some(&path), &cfg));
    let kept = said(&Machine::analyze(&source, Some(&path), &cfg));
    let mut report = String::new();
    for (what, a, b) in [("cold/warm", &cold, &warm), ("warm/keeping", &warm, &keep), ("keeping/kept", &keep, &kept)] {
        let d = difference(a, b);
        if !d.is_empty() {
            report += &format!("== {tag} {what}: {} rows\n{d}\n", d.lines().count());
        }
    }
    eprintln!(
        "coldwarm {tag}: {t_cold:.1}s cold, {} rows, {}",
        cold.len(),
        if report.is_empty() { "same" } else { "DIFFERENT" }
    );
    report
}

/// Sweep tool: `SATEX_SWEEP=<substring or all>` runs every fixture document whose name holds
/// it (and the site examples under `site/`), `SATEX_SWEEP_JOBS` at a time, and writes what
/// differs to target/scratch/coldwarm/<name>.diff. Without the variable it does nothing.
#[test]
fn sweep() {
    let Ok(filter) = std::env::var("SATEX_SWEEP") else { return };
    if !installed() {
        return;
    }
    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut jobs: Vec<(String, PathBuf)> = std::fs::read_dir(base.join("tests/fixtures/documents"))
        .unwrap()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    jobs.sort();
    jobs.retain(|(n, _)| filter == "all" || filter.split(',').any(|f| n.contains(f)));
    let jobs = std::sync::Mutex::new(jobs);
    let failed = std::sync::Mutex::new(Vec::new());
    let n = std::env::var("SATEX_SWEEP_JOBS").ok().and_then(|n| n.parse().ok()).unwrap_or(4);
    std::thread::scope(|s| {
        for _ in 0..n {
            s.spawn(|| {
                loop {
                    let Some((name, root)) = jobs.lock().unwrap().pop() else { break };
                    let report = three_ways(&root, &name);
                    let out = base.join("target/scratch/coldwarm").join(format!("{name}.diff"));
                    if report.is_empty() {
                        let _ = std::fs::remove_file(out);
                    } else {
                        std::fs::write(out, &report).unwrap();
                        failed.lock().unwrap().push(name);
                    }
                }
            });
        }
    });
    let failed = failed.into_inner().unwrap();
    assert!(failed.is_empty(), "cold and warm differ: {failed:?}");
}

/// A package that runs Lua while it loads, then a document that reads what that Lua set. No
/// cache holds the Lua state, so a load served from its cache must leave the state as reading
/// it does: both runs know `pkgvalue`.
/// Pins: the served load skipped the package's `\directlua`, the document's chunk then read a
/// global the state never got, and the two runs disagreed on `\seven`.
#[test]
fn a_package_that_runs_lua_says_the_same_built_and_served() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join("coldwarm-lua");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("luapkg.sty"),
        "\\ProvidesPackage{luapkg}\n\\newcommand\\before{}\n\\directlua{pkgvalue = 7}\n\\newcommand\\after{}\n",
    )
    .unwrap();
    let text = "\\documentclass{article}\n\\usepackage{luapkg}\n\\begin{document}\n\
                \\ifnum\\directlua{tex.print(pkgvalue)}=7 \\def\\seven{}\\fi\n\\end{document}\n";
    let main = dir.join("main.tex");
    std::fs::write(&main, text).unwrap();
    let mut cfg = crate::support::config();
    cfg.engine = Some(satex::project::config::Engine::LuaTeX);
    cfg.cache.dir = Some(dir.join("cache"));
    crate::support::seed_kernel(&cfg);
    cfg.cache.document = false;
    let built = Machine::analyze(text, Some(&main), &cfg);
    let served = Machine::analyze(text, Some(&main), &cfg);
    assert!(crate::support::defined(&built, "seven"), "the run that read the package knows pkgvalue");
    assert!(crate::support::defined(&served, "seven"), "the run served from its cache knows pkgvalue");
    assert_eq!(difference(&said(&built), &said(&served)), "", "packages built vs served");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A build runs the document again once its `.aux` settles, from a checkpoint of the first
/// run. The Lua state is not part of a checkpoint: it has gone on to the first run's end. A
/// run from a checkpoint before a Lua chunk would run that chunk a second time on it.
/// Pins: the counter stepped once per run read 2 in the second run, so `\one` was lost.
#[test]
fn a_rerun_of_a_build_does_not_run_lua_on_the_first_runs_state() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join("coldwarm-lua-rerun");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let text = "\\documentclass{article}\n\\directlua{n = 0}\n\\begin{document}\n\\directlua{n = n + 1}\n\
                \\section{A}\\label{a} See Section~\\ref{a}.\n\n\
                \\ifnum\\directlua{tex.print(n)}=1 \\def\\one{}\\fi\n\\end{document}\n";
    let main = dir.join("main.tex");
    std::fs::write(&main, text).unwrap();
    let mut cfg = crate::support::config();
    cfg.engine = Some(satex::project::config::Engine::LuaTeX);
    cfg.cache.dir = Some(dir.join("cache"));
    crate::support::seed_kernel(&cfg);
    let mut checkpoints = satex::analysis::machine::Checkpoints::new(&cfg);
    let built = Machine::build(&mut checkpoints, text, &main, &cfg, true);
    assert!(crate::support::defined(&built, "one"), "every run steps the counter once, from a fresh state");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A package loaded with an option, read and then served from its cache. Reading it runs
/// the option's tokens, which stand in the `\usepackage` call, so the run's "last place"
/// was inside the option list; a served load never ran them. What the loader does after the
/// file (`\catcode` of `@` back to other) is a row raised at that last place.
/// Pins: the built run raised it at the option (a row of its own), the served run at the
/// call (the same row as the loader's first `\catcode`): one row more built than served.
#[test]
fn a_row_raised_after_a_load_stands_at_the_same_place_built_and_served() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join("coldwarm-place");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let text = "\\documentclass{article}\n\\usepackage[T1]{fontenc}\n\\begin{document}\nText.\n\\end{document}\n";
    let main = dir.join("main.tex");
    std::fs::write(&main, text).unwrap();
    let mut cfg = crate::support::config();
    cfg.cache.dir = Some(dir.join("cache"));
    crate::support::seed_kernel(&cfg);
    cfg.cache.document = false;
    let built = said(&Machine::analyze(text, Some(&main), &cfg));
    let served = said(&Machine::analyze(text, Some(&main), &cfg));
    assert_eq!(difference(&built, &served), "", "packages built vs served");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A line that overruns `limits.line_steps` is cut at a check of the line guard, and the
/// guard checks every so many steps. Three loops on one line, each past the budget: the cut
/// must fall on the same step whether the class was read or served from its cache.
/// Pins: a read load was checked along its way and a served one not at all, so the checks
/// after it fell on other steps, the loops were cut some passes apart, and the step count
/// and use counts of the two runs differed.
#[test]
fn a_line_cut_falls_on_the_same_step_built_and_served() {
    if !installed() {
        return;
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/documents/cut_inside_include");
    assert_eq!(three_ways(&root, "cut-built-served"), "");
}

/// A document beside a package of its own, analyzed twice from a cache that holds only the
/// kernel: the package read, then served. Returns what the two runs said.
fn built_and_served(tag: &str, package: &str, body: &str, line_steps: u64) -> (Vec<String>, Vec<String>) {
    let dir = crate::support::tmp_root().join(format!("coldwarm-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("own.sty"), format!("\\ProvidesPackage{{own}}\n{package}\n")).unwrap();
    let text =
        format!("\\documentclass{{article}}\n\\usepackage{{own}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n");
    let main = dir.join("main.tex");
    std::fs::write(&main, &text).unwrap();
    let mut cfg = crate::support::config();
    cfg.cache.dir = Some(dir.join("cache"));
    crate::support::seed_kernel(&cfg);
    cfg.cache.document = false;
    if line_steps > 0 {
        cfg.limits.line_steps = line_steps;
    }
    let built = said(&Machine::analyze(&text, Some(&main), &cfg));
    let served = said(&Machine::analyze(&text, Some(&main), &cfg));
    let _ = std::fs::remove_dir_all(&dir);
    (built, served)
}

/// Steps one line of the package below may take: far fewer than its loop needs.
const SHORT_LINE: u64 = 20_000;

/// A package whose loop overruns the line budget is cut while it loads: what the cut code
/// assigned is unknown from there on. The run that reads the package and the run served from
/// what was stored of it must both know that: an error the document then raises is no more
/// certain in one than in the other.
#[test]
fn a_load_that_was_cut_leaves_the_same_doubt_built_and_served() {
    if !installed() {
        return;
    }
    let package = "\\count255=0 \\loop\\advance\\count255 1 \\ifnum\\count255<9000000 \\repeat\n\\newcommand\\after{}";
    let body = "\\ifnum\\count255=9000000 \\def\\whole{}\\fi\n\\undefinedcommand\n\\after";
    let (built, served) = built_and_served("cut-load", package, body, SHORT_LINE);
    assert!(built.iter().any(|row| row.contains("line-budget-cut")), "the package's loop was not cut");
    assert_eq!(difference(&built, &served), "", "packages built vs served");
}

/// A package that draws a random number while it loads makes a typed unknown, named by a
/// serial. The document draws another and shows both: the names the two runs give them, and
/// every row that prints them, must not depend on whether the package was read or served.
#[test]
fn typed_numbers_made_in_a_load_are_named_the_same_built_and_served() {
    if !installed() {
        return;
    }
    let package = "\\newcount\\drawn \\drawn=\\pdfuniformdeviate 1000 \\newcommand\\after{}";
    let body = "\\count255=\\pdfuniformdeviate 1000 \\typeout{drawn \\the\\drawn, \\the\\count255}\n\
                \\ifnum\\drawn>\\count255 \\def\\larger{}\\fi\n\\after";
    let (built, served) = built_and_served("typed-load", package, body, 0);
    assert_eq!(difference(&built, &served), "", "packages built vs served");
}

/// A package with an undecided choice whose arms define a name differently: after the join
/// the name has two meanings. The document uses it and raises an error after: both runs
/// must hold the same meanings and the same certainty.
#[test]
fn a_choice_made_in_a_load_joins_the_same_built_and_served() {
    if !installed() {
        return;
    }
    let package = "\\ifodd\\pdfuniformdeviate 7 \\def\\branch{odd}\\else\\def\\branch{even}\\def\\onlyeven{}\\fi";
    let body = "\\typeout{\\branch}\\onlyeven\n\\undefinedcommand";
    let (built, served) = built_and_served("fork-load", package, body, 0);
    assert!(built.iter().any(|row| row.contains("undefinedcommand")), "the document's error is among the rows");
    assert_eq!(difference(&built, &served), "", "packages built vs served");
}

/// A document beside the package `own.sty`, built with the clock at `built`, then run with
/// the clock at `served`, on that cache and on one of its own. Returns the three runs.
fn at_two_clocks(tag: &str, package: &str, body: &str, built: &str, served: &str) -> [Analysis; 3] {
    let dir = crate::support::tmp_root().join(format!("coldwarm-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("own.sty"), format!("\\ProvidesPackage{{own}}\n{package}\n\\newcommand\\after{{}}\n"))
        .unwrap();
    let text = format!(
        "\\documentclass{{article}}\n\\usepackage{{own}}\n{body}\n\\begin{{document}}\n\\after\n\\end{{document}}\n"
    );
    let main = dir.join("main.tex");
    std::fs::write(&main, &text).unwrap();
    let mut cfg = crate::support::config();
    cfg.cache.document = false;
    let mut run = |cache: &str, clock: &str| {
        cfg.clock = clock.parse().unwrap();
        cfg.cache.dir = Some(dir.join(cache));
        crate::support::seed_kernel(&cfg);
        Machine::analyze(&text, Some(&main), &cfg)
    };
    let runs = [run("cache", built), run("cache", served), run("alone", served)];
    let _ = std::fs::remove_dir_all(&dir);
    runs
}

/// A package reads the job's clock, as in TeX: what it makes of `\time` and `\year` while it
/// loads is the date of the run, read or served. A cache built at one date must not replay
/// that date at another: the product is stored as arithmetic on the clock and computed again
/// where it is served.
/// Pins: a load stored the minute the cache was built and a warm run replayed it at another
/// minute; and (clockfix land-02) loads that read an unknown clock left datetime2's date and
/// pgfmath's seed unknown, so hyperref's `\pdfstringdef` ran away and theses lost their end.
#[test]
fn a_package_that_reads_the_clock_reads_the_date_it_runs_at() {
    if !installed() {
        return;
    }
    let package = "\\newcount\\loaded \\loaded=\\time \\multiply\\loaded\\year";
    // 12:00 is minute 720, 08:30 minute 510.
    let body = format!(
        "\\ifnum\\loaded={} \\def\\atbuilt{{}}\\fi \\ifnum\\loaded={} \\def\\atserved{{}}\\fi\n\
         \\ifnum\\year=2027 \\def\\docyear{{}}\\fi \\ifnum\\time=510 \\def\\doctime{{}}\\fi",
        720 * 2026,
        510 * 2027
    );
    let [built, served, alone] = at_two_clocks("clock", package, &body, "2026-01-01T12:00Z", "2027-06-15T08:30Z");
    let known =
        |a: &Analysis| ["atbuilt", "atserved", "docyear", "doctime"].map(|n| crate::support::presence(a, n)).join("");
    assert_eq!(known(&built), "1000", "built: the load's product and the document's date are 2026's");
    assert_eq!(known(&served), "0111", "served at 2027: the load's product is 2027's, not replayed");
    assert_eq!(difference(&said(&alone), &said(&served)), "", "served at another date vs read at it");
}

/// How the cache of `own` did in `analysis`: `cached` when all of it was served.
fn own_cache(analysis: &Analysis) -> String {
    let state = analysis.package_caches.iter().rev().find(|(name, ..)| name == "own");
    state.map(|(.., state)| state.clone()).unwrap_or_default()
}

/// datetime2 takes the job's start date apart by macro arguments while it loads, so the rest
/// of its file keys on the exact date (after tikz, whose loads leave the long lists it edits). Served at another second, the load is read again from
/// there and stored as another branch. That branch must be served the next time at that date.
/// Pins: a branch recorded after served segments compared a text it edited with the one from
/// before the load, not with what the served segments left. Its stored edit never fitted
/// ("an edited text differs"), so every run read the rest again and stored one more branch.
#[test]
fn a_branch_read_after_a_served_part_is_served_the_next_time() {
    if !installed() {
        return;
    }
    let dir = crate::support::tmp_root().join("coldwarm-branch");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let text = "\\documentclass{article}\n\\usepackage{tikz}\n\\usepackage{datetime2}\n\\begin{document}\n\\today\n\\end{document}\n";
    let main = dir.join("main.tex");
    std::fs::write(&main, text).unwrap();
    let mut cfg = crate::support::config();
    cfg.cache.document = false;
    cfg.cache.dir = Some(dir.join("cache"));
    let mut run = |clock: &str| {
        cfg.clock = clock.parse().unwrap();
        crate::support::seed_kernel(&cfg);
        Machine::analyze(text, Some(&main), &cfg)
    };
    let runs = ["2026-01-01T12:00:01Z", "2026-01-01T12:00:02Z", "2026-01-01T12:00:02Z"].map(&mut run);
    let _ = std::fs::remove_dir_all(&dir);
    let state = |a: &Analysis| {
        let found = a.package_caches.iter().rev().find(|(name, ..)| name == "datetime2");
        found.map(|(.., state)| state.clone()).unwrap_or_default()
    };
    // An installation without datetime2 has nothing to pin.
    if state(&runs[0]).is_empty() {
        return;
    }
    assert_ne!(state(&runs[1]), "cached", "another second: read again from where it takes the date apart");
    assert_eq!(state(&runs[2]), "cached", "that second again: {:?}", runs[2].package_caches);
    assert_eq!(difference(&said(&runs[1]), &said(&runs[2])), "", "read at that date vs served at it");
}

/// A package that only stores the date (datetime2 without `\pdfcreationdate` does so: the
/// hour and minute by arithmetic on `\time`, the year printed into a macro) does nothing
/// that depends on which date it is. Its cache is a function of the clock: built at one
/// date it is served whole at another, with that date's values filled in.
/// Pins: every segment that read the clock was keyed on its value, so with `clock: now`
/// such a load was read again every minute and stored once more each time.
#[test]
fn a_date_a_package_only_stores_is_filled_in_where_it_is_served() {
    if !installed() {
        return;
    }
    let package = "\\newcount\\clk \\clk=\\time \\divide\\clk by 60 \\edef\\hour{\\the\\clk}\n\
                   \\multiply\\clk by -60 \\advance\\clk by \\time \\edef\\minute{\\the\\clk}\n\
                   \\edef\\thisyear{in \\the\\year!}\\edef\\stamp{\\pdfcreationdate}\\clk=-\\month";
    // 08:30 on 2027-06-15, as the package's arithmetic leaves it.
    let body = "\\ifnum\\hour=8 \\def\\ha{}\\fi \\ifnum\\minute=30 \\def\\ma{}\\fi \\ifnum\\clk=-6 \\def\\mo{}\\fi\n\
                \\def\\want{in 2027!}\\ifx\\thisyear\\want \\def\\ya{}\\fi\n\
                \\edef\\want{\\pdfcreationdate}\\ifx\\stamp\\want \\def\\sa{}\\fi";
    let [built, served, alone] =
        at_two_clocks("clock-flow", package, body, "2026-01-01T12:00:07Z", "2027-06-15T08:30:41Z");
    let known = |a: &Analysis| ["ha", "ma", "mo", "ya", "sa"].map(|n| crate::support::presence(a, n)).join("");
    assert_eq!(known(&built), "00001", "built in 2026: only the stamp is the document's own");
    assert_eq!(known(&served), "11111", "served in 2027: every stored part is 2027's");
    assert_eq!(own_cache(&served), "cached", "no part of the load is read again: {:?}", served.package_caches);
    assert_eq!(difference(&said(&alone), &said(&served)), "", "served at another date vs read at it");
}

/// pgfmath's seed, as its file has it: in a group, `\time` times `\year`, 1 when that is 0,
/// printed into a global macro. The test on the product is a decision: the cache keeps how
/// it came out, not the minute. Any minute with a product that is not 0 is served, and
/// midnight (product 0, the other arm) is read again.
/// Pins: the seed's segment was keyed on `\time`, so every tikz document read the rest of
/// pgfmath again once a minute; unknown instead, the test parted every such run in two.
#[test]
fn a_seed_made_of_the_clock_is_served_while_its_test_comes_out_alike() {
    if !installed() {
        return;
    }
    let package = "\\newcount\\clk\n\\begingroup\n\\clk=\\time\n\\multiply\\clk by\\year\n\
                   \\ifnum\\clk=0 \\clk=1 \\fi\n\\xdef\\seed{\\the\\clk}\n\\endgroup";
    let body = format!("\\ifnum\\seed={} \\def\\served{{}}\\fi \\ifnum\\seed=1 \\def\\midnight{{}}\\fi", 510 * 2027);
    let known = |a: &Analysis| ["served", "midnight"].map(|n| crate::support::presence(a, n)).join("");
    let [_, served, alone] = at_two_clocks("clock-seed", package, &body, "2026-01-01T12:00Z", "2027-06-15T08:30Z");
    assert_eq!(known(&served), "10", "the seed is the serving minute's");
    assert_eq!(own_cache(&served), "cached", "the test came out alike: {:?}", served.package_caches);
    assert_eq!(difference(&said(&alone), &said(&served)), "", "served at another minute vs read at it");
    let [_, served, alone] = at_two_clocks("clock-seed0", package, &body, "2026-01-01T12:00Z", "2027-06-15T00:00Z");
    assert_eq!(known(&served), "01", "at midnight the product is 0 and the seed 1");
    assert_ne!(own_cache(&served), "cached", "the test comes out differently: read again");
    assert_eq!(difference(&said(&alone), &said(&served)), "", "served at midnight vs read at it");
}

/// A package that looks at the date (a name built from the year, a test on it) is served
/// only where it would do the same: the name keys the load on the year, the test on its
/// outcome. Same year, other day: served. Next year: read again, with that year's name.
/// Pins the fallback: whatever is not stored or tested as arithmetic keys on the exact value.
#[test]
fn a_date_a_package_looks_at_is_read_again_where_it_differs() {
    if !installed() {
        return;
    }
    let package = "\\expandafter\\def\\csname in\\the\\year\\endcsname{}\\ifnum\\month>6 \\def\\late{}\\fi";
    let body = "";
    let known = |a: &Analysis| ["in2026", "in2027", "late"].map(|n| crate::support::presence(a, n)).join("");
    let [built, served, alone] = at_two_clocks("clock-look", package, body, "2026-01-01T12:00Z", "2026-03-09T08:30Z");
    assert_eq!((known(&built).as_str(), known(&served).as_str()), ("100", "100"));
    assert_eq!(own_cache(&served), "cached", "same year, same half: {:?}", served.package_caches);
    assert_eq!(difference(&said(&alone), &said(&served)), "", "served on another day vs read on it");
    let [_, served, alone] = at_two_clocks("clock-look2", package, body, "2026-01-01T12:00Z", "2026-09-09T08:30Z");
    assert_eq!(known(&served), "101", "the test on the month comes out differently");
    assert_ne!(own_cache(&served), "cached");
    assert_eq!(difference(&said(&alone), &said(&served)), "", "served in the other half vs read in it");
    let [_, served, alone] = at_two_clocks("clock-look3", package, body, "2026-01-01T12:00Z", "2027-01-01T12:00Z");
    assert_eq!(known(&served), "010", "the name is the serving year's");
    assert_ne!(own_cache(&served), "cached");
    assert_eq!(difference(&said(&alone), &said(&served)), "", "served next year vs read then");
}

/// pgfmath seeds its random generator from the clock while it loads (`\pgfmathsetseed`).
/// Built at one date and served at another, a random number the document draws must take the
/// same course as in a run that read pgfmath at that date.
/// Pins: the served run replayed the seed of the minute the cache was built.
#[test]
fn pgfmath_draws_the_same_built_at_one_date_and_served_at_another() {
    if !installed() {
        return;
    }
    let [_, served, alone] = at_two_clocks(
        "clock-pgf",
        "\\RequirePackage{pgfmath}",
        "\\pgfmathsetmacro\\r{rnd}",
        "2026-01-01T12:00Z",
        "2027-06-15T08:30Z",
    );
    assert_eq!(difference(&said(&alone), &said(&served)), "", "served at another date vs read at it");
}

/// pgfmath's `rand` with a seed the run does not know (the clock read while pgfmath loads):
/// pdflatex compiles this, so the document reaches its end with no certain error.
/// Pins: every path was dropped at the end of the file and `missing-end` was certain.
#[test]
fn pgfmath_rand_with_an_unknown_seed_reaches_the_end() {
    if !installed() {
        return;
    }
    let mut cfg = crate::support::config();
    cfg.clock = satex::project::config::Clock::Unknown;
    let text =
        "\\documentclass{article}\\usepackage{pgfmath}\n\\pgfmathparse{rand}\n\\begin{document}x\\end{document}\n";
    let a = Machine::analyze(text, None, &cfg);
    let codes = crate::support::codes(&a);
    assert!(!codes.iter().any(|c| c == "missing-end" || c == "analysis-ended-early"), "{codes:?}");
}

/// One run of [`one_cache`]: the document's file name, its text, the files beside it.
type Run<'a> = (&'a str, &'a str, &'a [(&'a str, &'a str)]);

/// Runs on one cache that holds only the kernel, one after another, each beside the package
/// `own.sty`: per run its file name, its text and the files it finds beside it. Returns each
/// run's analysis, and the last document analyzed from a cache of its own.
fn one_cache(tag: &str, package: &str, runs: &[Run]) -> (Vec<Analysis>, Analysis) {
    let dir = crate::support::tmp_root().join(format!("coldwarm-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("own.sty"), format!("\\ProvidesPackage{{own}}\n{package}\n\\newcommand\\after{{}}\n"))
        .unwrap();
    let mut cfg = crate::support::config();
    cfg.cache.document = false;
    let mut analyze = |cache: &str, (name, text, files): &Run| {
        files.iter().for_each(|(file, content)| std::fs::write(dir.join(file), content).unwrap());
        let main = dir.join(name);
        std::fs::write(&main, text).unwrap();
        cfg.cache.dir = Some(dir.join(cache));
        crate::support::seed_kernel(&cfg);
        Machine::analyze(text, Some(&main), &cfg)
    };
    let shared: Vec<Analysis> = runs.iter().map(|run| analyze("cache", run)).collect();
    let alone = analyze("alone", runs.last().unwrap());
    let _ = std::fs::remove_dir_all(&dir);
    (shared, alone)
}

/// `\jobname` is the run's, not any file's: a package that expands it while it loads did
/// something else in another job. Two documents load the same package on one cache, and the
/// second must see its own job name in all that the package made of it: an `\edef`, an `\xdef`,
/// a name built from it, a file written by it and a file looked up by it.
/// Pins: the segment read no binding for `\jobname`, so the second document replayed the first
/// one's load: `\JobAtLoad` was `a` in `b` (memoize's `\mmzPrefix` was `main.memo.dir/`).
#[test]
fn a_package_that_expands_the_job_name_sees_each_documents_own() {
    if !installed() {
        return;
    }
    let package = "\\edef\\JobAtLoad{\\jobname}\\xdef\\JobGlobal{\\jobname}\n\
                   \\expandafter\\def\\csname at\\jobname\\endcsname{}\n\
                   \\newwrite\\ownout \\immediate\\openout\\ownout=\\jobname.zz \\immediate\\write\\ownout{x}\\immediate\\closeout\\ownout\n\
                   \\IfFileExists{\\jobname.cfg}{\\def\\HasCfg{}}{}";
    let text = "\\documentclass{article}\n\\usepackage{own}\n\\edef\\here{\\jobname}\n\
                \\ifx\\here\\JobAtLoad \\def\\okedef{}\\fi \\ifx\\here\\JobGlobal \\def\\okxdef{}\\fi\n\
                \\expandafter\\ifx\\csname at\\jobname\\endcsname\\relax\\else\\def\\okcsname{}\\fi\n\
                \\IfFileExists{\\jobname.zz}{\\def\\okopenout{}}{}\n\\begin{document}\n\\after\n\\end{document}\n";
    let (runs, alone) = one_cache("jobname", package, &[("a.tex", text, &[]), ("b.tex", text, &[("b.cfg", "cfg\n")])]);
    let names = ["okedef", "okxdef", "okcsname", "okopenout", "HasCfg"];
    let known = |a: &Analysis| names.map(|n| crate::support::presence(a, n)).join("");
    assert_eq!(known(&runs[0]), "11110", "a, read");
    assert_eq!(known(&runs[1]), "11111", "b, after a on one cache");
    assert_eq!(difference(&said(&alone), &said(&runs[1])), "", "b alone vs b after a");
}

/// A file read for its bytes (`\pdfmdfivesum file`, `\pdffilesize`) while a package loads is
/// as much a part of what the load did as a file it reads as TeX. Its content changes between
/// two runs of one document: the second must hash what the file holds then.
/// Pins: only the name's lookup was kept, not the file's content, so the second run replayed
/// the first one's digest and `\Sum` no longer matched the file.
#[test]
fn a_file_a_package_hashes_is_stamped_like_one_it_reads() {
    if !installed() {
        return;
    }
    let package = "\\edef\\Sum{\\pdfmdfivesum file{data.txt}}";
    let text = "\\documentclass{article}\n\\usepackage{own}\n\\edef\\Now{\\pdfmdfivesum file{data.txt}}\n\
                \\ifx\\Sum\\Now \\def\\same{}\\fi\n\\begin{document}\n\\after\n\\end{document}\n";
    let runs: [Run; 2] =
        [("main.tex", text, &[("data.txt", "one\n")]), ("main.tex", text, &[("data.txt", "another\n")])];
    let (runs, alone) = one_cache("hashed", package, &runs);
    assert_eq!(crate::support::presence(&runs[0], "same"), "1", "read");
    assert_eq!(crate::support::presence(&runs[1], "same"), "1", "after the file changed");
    assert_eq!(difference(&said(&alone), &said(&runs[1])), "", "alone vs after the edit");
}

/// A package that only stores the job name (`\edef\auxname{\jobname.aux}`) does the same in
/// every job. On one cache the second document is served the load whole, with its own name
/// filled in.
/// Pins: keyed on the job name alone, such a load would be read again in every document
/// (and unkeyed, as before, it replayed the first document's name).
#[test]
fn a_job_name_a_package_only_stores_is_filled_in_where_it_is_served() {
    if !installed() {
        return;
    }
    let package = "\\edef\\AuxName{\\jobname.aux}\\xdef\\JobGlobal{\\jobname}";
    let text = "\\documentclass{article}\n\\usepackage{own}\n\\edef\\here{\\jobname.aux}\\edef\\job{\\jobname}\n\
                \\ifx\\here\\AuxName \\def\\okedef{}\\fi \\ifx\\job\\JobGlobal \\def\\okxdef{}\\fi\n\
                \\begin{document}\n\\after\n\\end{document}\n";
    let (runs, alone) = one_cache("jobflow", package, &[("first.tex", text, &[]), ("b.tex", text, &[])]);
    let known = |a: &Analysis| ["okedef", "okxdef"].map(|n| crate::support::presence(a, n)).join("");
    assert_eq!(known(&runs[0]), "11", "first, read");
    assert_eq!(known(&runs[1]), "11", "b, after first on one cache");
    assert_eq!(own_cache(&runs[1]), "cached", "no part of the load is read again: {:?}", runs[1].package_caches);
    assert_eq!(difference(&said(&alone), &said(&runs[1])), "", "b alone vs b after first");
}
