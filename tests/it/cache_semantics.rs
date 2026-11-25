//! A cache is keyed on the interpreter's source digest (`format::cache_key`),
//! so a cache another build wrote is never read. What is left to check is
//! that this build's own caches mean what a cold run computes: a replay of a
//! fixed kernel and package yields every definition, diagnostic, expansion,
//! load and graph size the run that wrote the caches did.

use std::fmt::Write;

use satex::analysis::machine::{Analysis, Machine};

fn facts(analysis: &Analysis) -> String {
    let mut text = String::new();
    let name = |s| analysis.interner.name(s);
    for d in &analysis.facts.defs {
        let _ = writeln!(
            text,
            "def {} {:?} {:?}",
            name(d.name),
            d.span,
            d.mac.as_ref().map(|m| satex::engine::tex::detokenize(&m.replacement_text, &analysis.interner))
        );
    }
    for d in &analysis.facts.diagnostics {
        let _ = writeln!(text, "diag {} {:?} {}", d.code, d.span, d.count);
    }
    for e in &analysis.facts.expansions {
        let _ = writeln!(text, "exp {} {:?} {} {}", name(e.name), e.span, e.meaning.as_str(), e.count);
    }
    for l in &analysis.facts.loads {
        let _ = writeln!(text, "load {} {:?} {:?}", l.name, l.kind, l.span);
    }
    let _ = writeln!(text, "graph {} {}", analysis.graph.len(), analysis.graph.edge_count());
    text
}

/// The bug it pins: a cached kernel or package that replays other facts than
/// the run that stored it, so a warm run answers differently from a cold one.
#[test]
fn a_replay_from_the_caches_keeps_every_fact() {
    let dir = crate::support::scratch_dir(
        crate::support::tmp_root().join(format!("satex-cache-semantics-{}", std::process::id())),
    );
    let tree = crate::support::mini_texmf(&dir);
    let source = "\\usepackage{demo}\n\\demomacro\n\\csname demo@x\\endcsname\n";
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).expect("doc.tex");
    let cfg = crate::support::mini_config(&tree, &dir.join("cache"));
    let cold = facts(&Machine::analyze(source, Some(&path), &cfg));
    let warm = facts(&Machine::analyze(source, Some(&path), &cfg));
    assert!(cold.contains("load demo"), "the fixture loads its package:\n{cold}");
    assert_eq!(cold, warm, "a replay from the caches analyses differently from a cold run");
}
