//! Caches are keyed on `format::SEMANTIC_VERSION`, not on the source, so
//! builds of one version share them. This test pins what a fixed kernel and
//! package analyse to: when it changes and the version did not, a cache
//! written by an older build would be replayed wrongly.

use std::fmt::Write;

use satex::format::SEMANTIC_VERSION;
use satex::machine::{Analysis, Machine};

/// The semantic version and the digest it was checked in with.
const GOLDEN: (u32, u64) = (1, 0xd758_66b5_2306_f2ec);

fn digest(analysis: &Analysis) -> u64 {
    let mut text = String::new();
    let name = |s| analysis.interner.name(s);
    for d in &analysis.facts.defs {
        let _ = writeln!(
            text,
            "def {} {:?} {:?}",
            name(d.name),
            d.span,
            d.mac.as_ref().map(|m| satex::tex::detokenize(&m.replacement_text, &analysis.interner))
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
    text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

#[test]
fn the_cached_analysis_changes_only_with_a_semantic_version_bump() {
    let dir = crate::support::scratch_dir(
        crate::support::tmp_root().join(format!("satex-cache-semantics-{}", std::process::id())),
    );
    let tree = crate::support::mini_texmf(&dir);
    let source = "\\usepackage{demo}\n\\demomacro\n\\csname demo@x\\endcsname\n";
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).expect("doc.tex");
    let cfg = crate::support::mini_config(&tree, &dir.join("cache"));
    let cold = Machine::analyze(source, Some(&path), &cfg);
    let warm = Machine::analyze(source, Some(&path), &cfg);
    let (got, again) = (digest(&cold), digest(&warm));
    assert_eq!(got, again, "a replay from the caches analyses differently from a cold run");
    match GOLDEN {
        (version, digest) if version == SEMANTIC_VERSION => assert_eq!(
            digest,
            got,
            "the cached analysis changed without a bump: raise format::SEMANTIC_VERSION to {} and set GOLDEN to ({}, {got:#018x}) \
(if only this test's rendering changed, update the digest alone)",
            SEMANTIC_VERSION + 1,
            SEMANTIC_VERSION + 1
        ),
        _ => panic!("SEMANTIC_VERSION is {SEMANTIC_VERSION}: set GOLDEN to ({SEMANTIC_VERSION}, {got:#018x})"),
    }
}
