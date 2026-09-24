use std::collections::{BTreeMap, BTreeSet};

use crate::facts::LoadStatus;
use crate::lint::fix::{Applicability, Fix};
use crate::lint::{Category, Report, Rule, Severity};
use crate::lint::shared::{definition_order, document_start, first_loads};
use crate::tex::Sym;

pub static RULES: &[Rule] = &[
    Rule {
        code: "duplicate-package",
        category: Category::Performance,
        severity: Severity::Warning,
        summary: "a package is loaded more than once",
        explanation: "\
The second request is ignored by LaTeX.  It costs nothing at run time but it
hides which load carries the options.

Fix by deleting the later `\\usepackage`.",
        run: duplicate_package,
    },
    Rule {
        code: "unused-package",
        category: Category::Performance,
        severity: Severity::Info,
        summary: "a loaded package defines nothing the document uses",
        explanation: "\
Every name the package defines is unused.  The package may still be needed for
its side effects — page layout, hooks, fonts, `\\AtBeginDocument` code — so this
is a hint, not a verdict.

Fix, if the package really is unused, by deleting the `\\usepackage`: it is read
on every build.",
        run: unused_package,
    },
    Rule {
        code: "recursive-macro",
        category: Category::Performance,
        severity: Severity::Info,
        summary: "a macro calls itself, directly or through others",
        explanation: "\
Recursion is normal in TeX, but an unguarded one loops until TeX runs out of
memory, and the analysis has to widen at it — everything the recursion would
have defined becomes unknown.

Fix, if the recursion is unintended, by breaking the cycle; otherwise make sure
a conditional terminates it.",
        run: recursive_macro,
    },
    Rule {
        code: "package-after-preamble",
        category: Category::Performance,
        severity: Severity::Warning,
        summary: "a package is loaded after \\begin{document}",
        explanation: "\
`\\usepackage` belongs in the preamble; after `\\begin{document}` LaTeX raises
`\\usepackage before \\begin{document}` and the package cannot set anything up.

Fix by moving the load into the preamble.",
        run: package_after_preamble,
    },
    Rule {
        code: "missing-dependency",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "a TeX Live package the run reads from is not in the dependency file",
        explanation: "\
depp names the TeX Live package of every file a run reads by where it sits in
the tree (`tex/⟨format⟩/⟨package⟩/`) and writes them to `DEPENDS.txt`.  The
project's dependency file lacks one of them, so an installation made from it
— a minimal Docker image, a `texlive.withPackages` — cannot build the
document.

Fix by adding the package to the dependency file, or by rerunning depp.",
        run: missing_dependency,
    },
    Rule {
        code: "unused-dependency",
        category: Category::Performance,
        severity: Severity::Info,
        summary: "the dependency file lists a TeX Live package no file of the run comes from",
        explanation: "\
Every installation made from the dependency file carries the package, but the
run reads nothing from it.  It may still be needed for what satex does not
read — fonts, hyphenation patterns, Lua libraries, programs — so this is a
hint, not a verdict.

Fix, if the package really is unused, by deleting its line.",
        run: unused_dependency,
    },
];

fn duplicate_package(report: &mut Report) {
    let first = first_loads(report.analysis);
    let loads: Vec<_> = report.analysis.facts.loads.clone();
    for load in loads {
        if !load.kind.is_package() {
            continue;
        }
        let Some((options, at)) = first.get(load.name.as_str()) else { continue };
        if *at == load.span || load.options.iter().any(|o| !options.contains(o)) {
            continue;
        }
        let edits = report.edits().delete_load(&load);
        report.add_fix(
            load.span,
            &load.name,
            format!("{} is already loaded at {at}", load.name),
            Fix::new(format!("delete this \\usepackage{{{}}}", load.name), Applicability::Safe, edits),
        );
    }
}

fn unused_package(report: &mut Report) {
    let analysis = report.analysis;
    // The files each package's load read, its own and those it requested.
    let mut files: BTreeMap<Sym, BTreeSet<crate::tex::FileId>> = BTreeMap::new();
    for load in &analysis.facts.loads {
        let (Some(file), Some(name)) = (load.file, analysis.interner.lookup(&load.name)) else { continue };
        files.entry(name).or_default().insert(file);
        if let Some(by) = load.by {
            files.entry(by).or_default().insert(file);
        }
    }
    let mut provided: BTreeMap<Sym, BTreeSet<Sym>> = BTreeMap::new();
    for def in &analysis.facts.defs {
        // The kernel's own bookkeeping for the load (`\ver@…`, `\opt@…`,
        // hook code) is written in the kernel's files, not the package's.
        if def.package.is_none_or(|p| !files.get(&p).is_some_and(|f| f.contains(&def.span.file))) {
            continue;
        }
        // What the kernel sets inside a group while the package loads
        // (`\ProvidesPackage` writes its log line with `\protect` let to
        // `\string`) is gone when the group closes: the package does not
        // provide it.
        if def.depth > 0 && !def.global {
            continue;
        }
        if let Some(package) = def.package {
            provided.entry(package).or_default().insert(def.name);
        }
    }
    let used: BTreeSet<Sym> = analysis.facts.expansions.iter().map(|e| e.name).collect();
    let loads: Vec<_> = analysis.facts.loads.clone();
    for load in loads {
        if !load.kind.is_package() || load.status != LoadStatus::Read {
            continue;
        }
        let Some(package) = analysis.interner.lookup(&load.name) else { continue };
        let Some(names) = provided.get(&package) else { continue };
        if names.is_empty() || names.iter().any(|n| used.contains(n)) {
            continue;
        }
        let edits = report.edits().delete_load(&load);
        report.add_fix(
            load.span,
            &load.name,
            format!("{} defines {} names, none of which are used", load.name, names.len()),
            Fix::new(
                format!("delete \\usepackage{{{}}} if its side effects are not needed", load.name),
                Applicability::Unsafe,
                edits,
            ),
        );
    }
}

fn recursive_macro(report: &mut Report) {
    let analysis = report.analysis;
    let sites = definition_order(analysis);
    // A cycle nothing expands costs nothing to run.
    for crate::machine::Recursion { name, cycle, .. } in analysis.recursion().into_iter().filter(|r| r.observed) {
        let Some(&span) = sites.get(&name) else { continue };
        let rendered = analysis.interner.cs(name);
        let message = if cycle.len() > 1 {
            let others: Vec<String> =
                cycle.iter().filter(|s| **s != name).map(|s| analysis.interner.cs(*s)).collect();
            format!("{rendered} is mutually recursive with {}", others.join(", "))
        } else {
            format!("{rendered} calls itself")
        };
        report.add(span, &rendered, message, None);
    }
}

fn package_after_preamble(report: &mut Report) {
    let Some(end) = document_start(report.analysis) else { return };
    let loads: Vec<_> = report.analysis.facts.loads.clone();
    for load in loads {
        if !load.kind.is_package() || load.span.file != end.file || load.span.before(end.line, end.col) {
            continue;
        }
        let edits = report.edits().move_to_preamble(&load, end);
        report.add_fix(
            load.span,
            &load.name,
            format!("{} is loaded after \\begin{{document}}", load.name),
            Fix::new("move the \\usepackage into the preamble", Applicability::Unsafe, edits),
        );
    }
    let refused = refused_loads(report.analysis, &report.edits(), end);
    for load in refused {
        let edits = report.edits().move_to_preamble(&load, end);
        report.add_fix(
            load.span,
            &load.name,
            format!("{} is requested after \\begin{{document}}, where LaTeX refuses it", load.name),
            Fix::new("move the \\usepackage into the preamble", Applicability::Unsafe, edits),
        );
    }
}

/// Load requests the kernel refused after `\begin{document}`: `\@onlypreamble`
/// lets the command to `\@notprerr`, whose `\errmessage` stands at the call.
/// The command is a package request when a preamble meaning of it passes the
/// kernel's `\@pkgextension`.
fn refused_loads(
    analysis: &crate::machine::Analysis,
    edits: &crate::lint::fix::Edits,
    begin: crate::tex::Span,
) -> Vec<crate::facts::Load> {
    let interner = &analysis.interner;
    let Some(extension) = interner.lookup("@pkgextension") else { return Vec::new() };
    let error = |o: &crate::facts::Occurrence| crate::lint::shared::is_error(analysis, o);
    let requests = |name: Sym| {
        analysis.facts.defs.iter().any(|def| {
            def.name == name
                && def.mac.as_ref().is_some_and(|m| m.replacement_text.iter().any(|t| t.cs() == Some(extension)))
        })
    };
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for expansion in &analysis.facts.expansions {
        let span = expansion.span;
        if span.file != begin.file || span.before(begin.line, begin.col) || !seen.insert((span.line, span.col)) {
            continue;
        }
        if analysis.facts.loads.iter().any(|l| l.span == span)
            || !analysis.facts.occurrences.iter().any(|o| o.span == span && error(o))
            || !requests(expansion.name)
        {
            continue;
        }
        let Some((options, names)) = edits.load_request(span) else { continue };
        for name in names {
            out.push(crate::facts::Load {
                name,
                kind: crate::builtins::LoadKind::Package,
                options: options.clone(),
                span,
                by: Some(expansion.name),
                file: None,
                path: None,
                status: LoadStatus::NotFollowed,
                required: None,
                provided: None,
                depth: 0,
            });
        }
    }
    out
}


fn missing_dependency(report: &mut Report) {
    let Some(depp) = &report.analysis.plugins.depp else { return };
    let Some(file) = &depp.file else { return };
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
    for needed in depp.missing() {
        let directive = depp.declared.last().and_then(|d| d.directive.as_deref());
        let edits = report.edits().add_dependency(file, &needed.package, directive);
        report.add_fix(
            needed.span,
            &needed.package,
            format!("{} is read from but {name} does not list it", needed.package),
            Fix::new(format!("add `{}` to {name}", needed.package), Applicability::Safe, edits),
        );
    }
}

fn unused_dependency(report: &mut Report) {
    let analysis = report.analysis;
    let Some(depp) = &analysis.plugins.depp else { return };
    let Some(file) = &depp.file else { return };
    let name = file.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
    let at = depp
        .loaded
        .as_ref()
        .map(|(span, _)| *span)
        .unwrap_or_else(|| crate::tex::Span::new(analysis.main_file, 1, 1));
    for declared in depp.unused() {
        let edits = report.edits().drop_dependency(file, declared.line, &declared.package);
        report.add_fix(
            at,
            &declared.package,
            format!("{name}:{} lists {}, but nothing the run reads comes from it", declared.line, declared.package),
            Fix::new(
                format!("delete `{}` from {name} if nothing else needs it", declared.package),
                Applicability::Unsafe,
                edits,
            ),
        );
    }
}
