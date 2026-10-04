//! The same binary on the same document must say the same thing every time.
//! Two runs differ when an order the analysis never fixed reaches the
//! answer: a hash map walked for a decision, a parallel directory walk, a
//! tie left to whichever file the filesystem listed first.  Each test here
//! compares whole rows in order, so a reordering fails too.

use std::path::PathBuf;
use std::process::Command;

use crate::support::installed;
use satex::machine::{Analysis, Machine};

fn at(analysis: &Analysis, span: satex::tex::Span) -> String {
    format!("{}:{}:{}", analysis.file_name(span.file), span.line, span.col)
}

/// Every diagnostic the run raised, as a report shows it, in report order.
fn rows(analysis: &Analysis) -> Vec<String> {
    analysis
        .facts
        .diagnostics
        .iter()
        .map(|d| format!("{:?}|{}|{}|{}|{}", d.severity, d.code, at(analysis, d.span), d.count, d.message))
        .collect()
}

/// What the run recorded, in the order it recorded it: a document that
/// diagnoses nothing still has thousands of these, so a decision taken in
/// the wrong order shows here even where no diagnostic moves.
fn facts(analysis: &Analysis) -> Vec<String> {
    let name = |sym| analysis.interner.name(sym).to_string();
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
    defs.chain(expansions).chain(occurrences).collect()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("satex-determinism-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temporary directory");
    dir
}

/// Enough expl3 to reach the loops and undecided conditionals where an
/// order-sensitive decision would show.
const EXPL3: &str = r"\documentclass{article}
\ExplSyntaxOn
\seq_new:N \l_my_seq
\seq_put_right:Nn \l_my_seq {LF}
\seq_put_right:Nn \l_my_seq {OsF}
\prg_new_protected_conditional:Nnn \my_exists:n { TF }
  { \prg_return_true: }
\ExplSyntaxOff
\begin{document}
\ExplSyntaxOn
\seq_map_inline:Nn \l_my_seq
  { \my_exists:nTF {##1} { \seq_map_break:n { \typeout{stop} } } { } }
\ExplSyntaxOff
x -- y
\end{document}
";

/// Two analyses in one process share the hash seed, so this catches an
/// order that changes within a run rather than between runs.
#[test]
fn one_process_analyzing_twice_says_the_same() {
    if !installed() {
        return;
    }
    let cfg = crate::support::config();
    let first = Machine::analyze(EXPL3, None, &cfg);
    let second = Machine::analyze(EXPL3, None, &cfg);
    assert_eq!(rows(&first), rows(&second), "two analyses of one document in one process diagnose differently");
    assert_eq!(facts(&first), facts(&second), "two analyses of one document in one process record differently");
    assert!(facts(&first).len() > 100, "the run should record something for the comparison to mean anything");
}

/// The first analysis builds the kernel cache and the second reads it: a
/// message `\everyjob` prints must carry the same span in both (the build
/// once left its `\input` call set, dating the banner to latex.ltx).
#[test]
fn building_the_cache_and_reading_it_say_the_same() {
    if !installed() {
        return;
    }
    let dir = scratch("cold");
    let mut cfg = crate::support::config();
    cfg.cache.enabled = true;
    cfg.cache.dir = Some(dir.join("cache"));
    let built = Machine::analyze(EXPL3, None, &cfg);
    let read = Machine::analyze(EXPL3, None, &cfg);
    assert_eq!(rows(&built), rows(&read), "the run that built the cache and the run that read it diagnose differently");
    assert_eq!(facts(&built), facts(&read), "the run that built the cache and the run that read it record differently");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A fresh process draws a fresh `RandomState` seed, so every hash map in
/// the run is walked in another order.  Both runs share one cache
/// directory, warmed first, so nothing but the process differs.  The clock
/// is pinned: `now` is an input, and \pdfcreationdate shows its seconds.
#[test]
fn two_processes_agree() {
    if !installed() {
        return;
    }
    let dir = scratch("processes");
    std::fs::write(dir.join("d.tex"), EXPL3).expect("write document");
    let cache = dir.join("cache");
    let run = |query: &str| {
        let output = Command::new(env!("CARGO_BIN_EXE_satex"))
            .args([
                "query",
                query,
                "-f",
                "d.tex",
                "--format",
                "json",
                "--set",
                "limits.seconds=0",
                "--set",
                "clock=2026-09-29T14:30:00Z",
            ])
            .current_dir(&dir)
            .env("SATEX_CACHE", &cache)
            .output()
            .expect("satex runs");
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    // The first run builds the caches; what is compared is two runs that
    // both read them, so only the process differs.
    let _ = run("diagnostics");
    for query in ["diagnostics", "definitions", "expansions", "occurrences", "gaps"] {
        let first = run(query);
        let second = run(query);
        assert_eq!(first, second, "two processes disagree on `query {query}`");
        if query == "definitions" {
            assert!(first.len() > 1000, "`query definitions` should answer something: {first}");
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The names of a set, walked through two hashers seeded differently on
/// purpose: what `RandomState` does between processes, done here so the two
/// orders really are two orders.
fn two_orders(names: &[String]) -> (Vec<String>, Vec<String>) {
    use std::hash::{BuildHasher, Hasher};

    /// A hasher whose seed the caller picks.
    #[derive(Clone, Copy, Default)]
    struct Seeded<const S: u64>;

    struct SeededHasher(std::collections::hash_map::DefaultHasher);

    impl<const S: u64> BuildHasher for Seeded<S> {
        type Hasher = SeededHasher;
        fn build_hasher(&self) -> SeededHasher {
            let mut inner = std::collections::hash_map::DefaultHasher::new();
            inner.write_u64(S);
            SeededHasher(inner)
        }
    }

    impl Hasher for SeededHasher {
        fn finish(&self) -> u64 {
            self.0.finish()
        }
        fn write(&mut self, bytes: &[u8]) {
            self.0.write(bytes);
        }
    }

    let one: std::collections::HashSet<&String, Seeded<1>> = names.iter().collect();
    let two: std::collections::HashSet<&String, Seeded<7>> = names.iter().collect();
    (one.into_iter().cloned().collect(), two.into_iter().cloned().collect())
}

/// Each file declares its own `% !TeX root`, so which one the project
/// starts from is a choice the walk would otherwise make: the two
/// directories hold the same files, written in two different orders.
#[test]
fn the_file_order_does_not_decide_the_declared_root() {
    let names: Vec<String> = (0..48).map(|i| format!("r{i:02}")).collect();
    let (one, two) = two_orders(&names);
    assert_ne!(one, two, "the two seeds should walk the names in different orders");
    let write = |dir: &PathBuf, order: &[String]| {
        for name in order {
            let text = format!("% !TeX root = {name}.tex\n\\input{{x}}\n");
            std::fs::write(dir.join(format!("{name}.tex")), text).expect("write file");
        }
    };
    let (first_dir, second_dir) = (scratch("order-a"), scratch("order-b"));
    write(&first_dir, &one);
    write(&second_dir, &two);
    let named = |found: &satex::plugin::Discovery| {
        found.declared_root.as_ref().map(|root| root.file_name().unwrap().to_string_lossy().into_owned())
    };
    for threads in [1, 8] {
        let a = satex::plugin::discovery::scan_with(&first_dir, threads);
        let b = satex::plugin::discovery::scan_with(&second_dir, threads);
        assert_eq!(named(&a), named(&b), "the file order decided the `% !TeX root` on {threads} thread(s)");
        assert_eq!(named(&a), Some("r00.tex".to_string()), "the first root in path order should win");
    }
    let _ = std::fs::remove_dir_all(&first_dir);
    let _ = std::fs::remove_dir_all(&second_dir);
}

/// The parallel walker finishes its threads in no fixed order, so one
/// directory is scanned repeatedly and every list compared.
#[test]
fn a_parallel_project_walk_reports_one_thing() {
    let dir = scratch("walk");
    for i in 0..48 {
        let text = format!("% !TeX root = r{i:02}.tex\nx\n");
        std::fs::write(dir.join(format!("r{i:02}.tex")), text).expect("write file");
    }
    let first = satex::plugin::discovery::scan_with(&dir, 8);
    for _ in 0..8 {
        let again = satex::plugin::discovery::scan_with(&dir, 8);
        assert_eq!(first.declared_root, again.declared_root, "the parallel walk picked another `% !TeX root`");
        assert_eq!(first.inputs, again.inputs);
        assert_eq!(first.documents, again.documents);
        assert_eq!(first.packages, again.packages);
    }
    let single = satex::plugin::discovery::scan_with(&dir, 1);
    assert_eq!(first.declared_root, single.declared_root, "the walkers disagree on the `% !TeX root`");
    assert_eq!(first.inputs, single.inputs, "the walkers disagree on the inputs");
    let _ = std::fs::remove_dir_all(&dir);
}
