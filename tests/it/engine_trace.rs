//! Differential test: `satex trace` against what a real engine says of
//! itself.  `\tracingmacros=2`, `\tracingcommands=2` and e-TeX's
//! `\tracingassigns=1` (with `\tracingonline=1` so nothing is held back for
//! the terminal) make `pdftex -ini -etex` write its own account of every
//! macro expansion, primitive execution and assignment to its log.  This
//! parses that log and checks it against [`satex::machine::Analysis::trace`]
//! for the same document.
//!
//! satex over-approximates: an undecided `\if…\fi` runs both arms, and a
//! widened loop keeps going past what one concrete run would do.  So the
//! right relation is **containment**, not equality, and it differs by kind
//! of step:
//!
//! - **Execute** (a primitive dispatched): every primitive the engine's
//!   `\tracingcommands` names must be an execute step in satex's trace, in
//!   the same order — an ordered subsequence, since satex may execute extra
//!   primitives (both arms of a condition, kernel code the engine skips on
//!   this path) that the engine did not.
//! - **Expand** (a macro called): likewise, every macro name
//!   `\tracingmacros` reports expanding must appear, in order, among
//!   satex's own `Expand` steps.
//! - **Define/Assign** (a name got a new meaning): only the macro
//!   definitions e-TeX's `\tracingassigns` reports (`into \name=macro:…`)
//!   are checked, since a register's `\tracingassigns` line names it as
//!   `\count3`, `\dimen0`, and so on — a form satex's own per-register
//!   symbol does not print the same way, so a register assignment is left
//!   to the `Execute` check on the primitive (`\count`, `\dimen`, …) that
//!   made it.  `Define` and `Assign` are pooled together on satex's side,
//!   since which of the two satex used for a given name is an
//!   implementation detail the engine's log cannot speak to.
//! - **Branch** (a conditional was decided): the engine's `{true}`/`{false}`
//!   right after `{\ifnum}` and its kin must be *among* satex's `Branch`
//!   steps for that same conditional — not the only one, since satex may
//!   have recorded the other arm too when it could not decide the test
//!   itself, but the arm the engine actually took must be there.
//! - **OpenGroup/CloseGroup**: these carry no name (a `{`/`}` is not a
//!   control sequence), so only their counts are compared: satex must have
//!   opened and closed at least as many groups as the engine did.
//!
//! Where satex's trace lacks a step the engine took, `divergences` names it;
//! that is a real gap in the interpreter, not a fixture to relax.

use regex::Regex;

use satex::config::{Config, Engine};
use satex::machine::{Machine, Step};
use satex::plugin::Kernel;

/// Catcodes and tracing switches every probe shares: without a format,
/// INITEX's catcodes are in force (tex.web § 232), so `{`, `}` and `#` are
/// set up by hand, exactly as the engine sees them.
const PRELUDE: &str = "\\catcode`\\{=1 \\catcode`\\}=2 \\catcode`\\#=6 \
\\tracingonline=1 \\tracingmacros=2 \\tracingcommands=2 \\tracingassigns=1 ";

/// A handful of small, real documents: a macro definition and call, a
/// decided conditional, a loop that counts to a fixed bound, a box, an
/// `\edef`, and a group whose definition does not escape it.
const CASES: &[(&str, &str)] = &[
    ("macro-definition-and-call", "\\def\\a{XY}\n\\a\n"),
    ("conditional", "\\count0=1 \\ifnum\\count0=1 \\def\\b{P}\\else\\def\\b{Q}\\fi\n\\b\n"),
    ("counting-loop", "\\def\\loop{\\advance\\count0 by1 \\ifnum\\count0<3 \\loop\\fi}\n\\count0=0 \\loop\n"),
    ("box", "\\setbox0=\\hbox{\\kern3pt}\n\\dimen0=\\wd0\n"),
    ("edef", "\\def\\a{Y}\n\\edef\\b{X\\a}\n"),
    ("group", "{\\def\\a{X}}\n\\ifdefined\\a\\def\\c{D}\\else\\def\\c{U}\\fi\n"),
];

/// What the engine's own tracing said it did, in order: the primitives it
/// executed, the macro definitions it made, the macros it expanded, and the
/// conditionals it decided (with the arm taken).  Groups are counted, not
/// named — see the module doc.
#[derive(Default, Debug)]
struct EngineTrace {
    executed: Vec<String>,
    assigned_macros: Vec<String>,
    expanded: Vec<String>,
    branches: Vec<(String, String)>,
    group_opens: u32,
    group_closes: u32,
}

/// Parse `pdftex -ini -etex`'s log for one probe into an [`EngineTrace`].
/// Only the text up to the first `{\end}` line is read: `\end` starts the
/// page builder and output routine, which satex does not model and which
/// has nothing to do with the interpreter steps this test checks.
fn parse_engine_trace(log: &str) -> EngineTrace {
    let re_exec = Regex::new(r"^\{(?:[a-z]+(?: [a-z]+)*: )?(\\[A-Za-z@]+)\}$").unwrap();
    let re_assign_macro = Regex::new(r"^\{(?:globally )?into (\\[A-Za-z@]+)=macro:").unwrap();
    let re_expand_head = Regex::new(r"^(\\[A-Za-z@]+) .*->").unwrap();

    let mut out = EngineTrace::default();
    let mut pending_cond: Option<String> = None;
    for line in log.lines() {
        if line == "{\\end}" {
            break;
        }
        if let Some(cap) = re_exec.captures(line) {
            let name = cap[1].to_string();
            // A conditional primitive's decision is the very next line
            // (`{\ifnum}` then `{true}` or `{false}`, tex.web § 1294ff).
            pending_cond = name.strip_prefix('\\').filter(|n| n.starts_with("if")).map(|_| name.clone());
            out.executed.push(name);
            continue;
        }
        if let Some(name) = pending_cond.take() {
            match line {
                "{true}" => out.branches.push((name, "true".to_string())),
                "{false}" => out.branches.push((name, "false".to_string())),
                _ => {}
            }
        }
        if let Some(cap) = re_assign_macro.captures(line) {
            out.assigned_macros.push(cap[1].to_string());
            continue;
        }
        if let Some(cap) = re_expand_head.captures(line) {
            out.expanded.push(cap[1].to_string());
            continue;
        }
        match line {
            "{begin-group character {}" => out.group_opens += 1,
            "{end-group character }}" => out.group_closes += 1,
            _ => {}
        }
    }
    out
}

/// Run `pdftex -ini -etex` on `source` (with `\end` appended) and parse its
/// log, or `None` when the run itself failed.
fn engine_trace(source: &str, tag: &str) -> Option<EngineTrace> {
    let dir = crate::support::tmp_root().join(format!("satex-engine-trace-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join("t.tex"), format!("{source}\\end\n")).ok()?;
    let out = std::process::Command::new("pdftex")
        .args(["-ini", "-etex", "-interaction=batchmode", "t.tex"])
        .current_dir(&dir)
        .output()
        .ok()?;
    out.status.success().then_some(())?;
    let log = std::fs::read_to_string(dir.join("t.log")).ok()?;
    Some(parse_engine_trace(&log))
}

/// Whether `needle` occurs in `haystack` in order, as a (not necessarily
/// contiguous) subsequence: the containment relation this whole test rests
/// on, since satex may have extra steps `haystack` alone does not rule out.
fn is_subsequence<T: PartialEq>(needle: &[T], haystack: &[T]) -> bool {
    let mut rest = haystack.iter();
    needle.iter().all(|want| rest.any(|have| have == want))
}

#[test]
fn satex_trace_contains_every_step_a_real_engine_took() {
    if satex::paths::which("pdftex").is_err() {
        eprintln!("no pdftex found; skipping the engine-trace suite");
        return;
    }
    let mut divergences = Vec::new();
    for (tag, body) in CASES {
        let source = format!("{PRELUDE}{body}");
        let Some(engine) = engine_trace(&source, tag) else {
            eprintln!("{tag}: pdftex did not run cleanly; skipping");
            continue;
        };

        let cfg = Config {
            engine: Some(Engine::PdfTeX),
            kernel: Some(Kernel::None),
            trace: true,
            ..crate::support::config()
        };
        let analysis = Machine::analyze(&source, None, &cfg);

        let cs = |sym: satex::tex::Sym| analysis.interner.cs(sym);
        let satex_executed: Vec<String> =
            analysis.trace.iter().filter(|e| e.kind == Step::Execute).map(|e| cs(e.name)).collect();
        let satex_defined_or_assigned: Vec<String> = analysis
            .trace
            .iter()
            .filter(|e| matches!(e.kind, Step::Define | Step::Assign))
            .map(|e| cs(e.name))
            .collect();
        let satex_expanded: Vec<String> =
            analysis.trace.iter().filter(|e| e.kind == Step::Expand).map(|e| cs(e.name)).collect();
        let satex_branches: Vec<(String, String)> = analysis
            .trace
            .iter()
            .filter(|e| e.kind == Step::Branch)
            .map(|e| (cs(e.name), e.detail.as_deref().unwrap_or("").to_string()))
            .collect();
        let (satex_opens, satex_closes) = analysis.trace.iter().fold((0u32, 0u32), |(o, c), e| match e.kind {
            Step::OpenGroup => (o + 1, c),
            Step::CloseGroup => (o, c + 1),
            _ => (o, c),
        });

        if !is_subsequence(&engine.executed, &satex_executed) {
            divergences.push(format!(
                "{tag}: satex's Execute steps do not contain every primitive pdftex ran\n  engine (tracingcommands): {:?}\n  satex (Step::Execute):    {:?}",
                engine.executed, satex_executed
            ));
        }
        if !is_subsequence(&engine.assigned_macros, &satex_defined_or_assigned) {
            divergences.push(format!(
                "{tag}: satex's Define/Assign steps do not contain a macro definition pdftex made\n  engine (tracingassigns): {:?}\n  satex (Define ∪ Assign): {:?}",
                engine.assigned_macros, satex_defined_or_assigned
            ));
        }
        if !is_subsequence(&engine.expanded, &satex_expanded) {
            divergences.push(format!(
                "{tag}: satex's Expand steps do not contain a macro call pdftex made\n  engine (tracingmacros): {:?}\n  satex (Step::Expand):   {:?}",
                engine.expanded, satex_expanded
            ));
        }
        if !is_subsequence(&engine.branches, &satex_branches) {
            divergences.push(format!(
                "{tag}: satex never recorded the branch pdftex actually took\n  engine (tracingcommands true/false): {:?}\n  satex (Step::Branch):                {:?}",
                engine.branches, satex_branches
            ));
        }
        if engine.group_opens > satex_opens || engine.group_closes > satex_closes {
            divergences.push(format!(
                "{tag}: satex opened/closed fewer groups ({satex_opens}/{satex_closes}) than pdftex did ({}/{})",
                engine.group_opens, engine.group_closes
            ));
        }
    }
    assert!(divergences.is_empty(), "\n{}", divergences.join("\n\n"));
}
