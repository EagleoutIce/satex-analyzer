use std::collections::{BTreeMap, BTreeSet};

use crate::facts::LoadStatus;
use crate::lint::fix::{Applicability, Fix};
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::tex::{Span, Sym};

pub static RULES: &[Rule] = &[
    Rule {
        code: "duplicate-package",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a package is loaded more than once",
        explanation: "\
The second request is ignored by LaTeX.  It costs nothing at run time but it
hides which load carries the options.  A request with options the first did
not have is an option clash instead (`option-clash`).

Fix by deleting the later `\\usepackage`.",
        references: &[Ref { source: Source::Source2e, at: "ltclass.dtx" }],
        fixes: &[Applicability::Safe],
        run: duplicate_package,
    },
    Rule {
        code: "unused-package",
        category: Category::Performance,
        severity: Severity::Info,
        summary: "a loaded package defines nothing the document uses",
        explanation: "\
Every name the package defines is unused, and no code outside it reads a
setting it made (a class's `\\textheight` is read for every page).  It may
still act through hooks or fonts, so this is a hint, not a verdict.

Fix, if the package really is unused, by deleting the `\\usepackage`: it is read
on every build.",
        references: &[],
        fixes: &[Applicability::Unsafe],
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

Fix, if the recursion is unintended, by breaking the cycle.  Otherwise make sure
a conditional terminates it.",
        references: &[],
        fixes: &[],
        run: recursive_macro,
    },
    Rule {
        code: "package-after-preamble",
        category: Category::Performance,
        severity: Severity::Warning,
        summary: "a package is loaded after `\\begin{document}`",
        explanation: "\
`\\usepackage` belongs in the preamble.  After `\\begin{document}` LaTeX raises
`\\usepackage before \\begin{document}` and the package cannot set anything up.

Fix by moving the load into the preamble.",
        references: &[Ref { source: Source::Source2e, at: "ltclass.dtx" }],
        fixes: &[Applicability::Unsafe],
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
        references: &[],
        fixes: &[Applicability::Safe],
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
        references: &[],
        fixes: &[Applicability::Unsafe],
        run: unused_dependency,
    },
    Rule {
        code: "preamble-cost",
        category: Category::Performance,
        severity: Severity::Info,
        summary: "what the preamble costs on every build",
        explanation: "\
The packages the document asks for and the files read before
`\\begin{document}`, reported when the preamble takes most of the tokens
the build interprets and is no small one.

A format holds the preamble already interpreted, so a build that starts from
one skips all of it.  [mylatexformat](https://ctan.org/pkg/mylatexformat)
dumps one from the document's own preamble, and TeX reads a `%&`(format) line
before anything else (web2c manual, \"Format files\").

Fix by dumping the preamble into a format and starting every build from it.",
        references: &[Ref { source: Source::Companion, at: "formats" }],
        fixes: &[Applicability::Unsafe],
        run: preamble_cost,
    },
    Rule {
        code: "includeonly-available",
        category: Category::Performance,
        severity: Severity::Info,
        summary: "a document of several `\\include`d parts is built whole",
        explanation: "\
`\\includeonly` names the parts a build reads.  The others keep their page
numbers and labels from their `.aux` files (source2e, ltfiles.dtx).  A draft
build of one chapter then skips the rest.  The rule fires when the document
`\\include`s at least two parts, no `\\includeonly` is in force, and the body
is most of what the build interprets.  It also has to set a book's worth of
paragraph text, `lints.build-cost.characters` in `satex.yaml`: a shorter
document builds whole in seconds.

Fix by listing the parts being written in `\\includeonly` while drafting.",
        references: &[Ref { source: Source::Source2e, at: "ltfiles.dtx" }],
        fixes: &[Applicability::Unsafe],
        run: includeonly_available,
    },
];

fn duplicate_package(report: &mut Report) {
    let analysis = report.analysis();
    for (load, options, at, first_load) in report.index().reloads() {
        if load.options.iter().any(|o| !options.contains(o)) {
            continue;
        }
        let edits = report.edits().delete_load(load);
        // The first load may be a package's own request (`tikz` loads `pgf`).
        let by =
            first_load.and_then(|l| l.by).map(|by| format!(" by {}", analysis.interner.name(by))).unwrap_or_default();
        report.add_fix(
            load.span,
            &load.name,
            format!("{} is already loaded{by} at {}", load.name, report.place(at)),
            Fix::new(format!("delete this `\\usepackage{{{}}}`", load.name), Applicability::Safe, edits),
        );
    }
}

fn unused_package(report: &mut Report) {
    let analysis = report.analysis();
    // The files each package's load read, its own and those it requested.
    let mut files: BTreeMap<Sym, BTreeSet<crate::tex::FileId>> = BTreeMap::new();
    for load in &analysis.facts.loads {
        let (Some(file), Some(name)) = (load.file, analysis.interner.lookup(&load.name)) else { continue };
        files.entry(name).or_default().insert(file);
        if let Some(by) = load.by {
            files.entry(by).or_default().insert(file);
        }
    }
    let of = |file: crate::tex::FileId| files.iter().filter(move |(_, f)| f.contains(&file)).map(|(p, _)| *p);
    // A package's names: those the format did not have and whose meaning
    // the run ends with is the one the package gave them, a scratch name it
    // once set (`\@tempa`) not.
    // Used is what the document's body expands, and what the text of a
    // macro so expanded calls, through any number of macros: the path that
    // would expand it may be one this run did not take. What
    // `\begin{document}` and `\end{document}` run is the packages' own
    // setting up and hooks.
    let document =
        |kind| analysis.facts.occurrences.iter().find(|o| o.kind == kind && o.key == "document").map(|o| o.span);
    let (begin, end) =
        (document(crate::builtins::OccKind::BeginEnvironment), document(crate::builtins::OccKind::EndEnvironment));
    let body = |span: Span| {
        crate::query::origin(analysis, span.file) == "document"
            && match begin {
                Some(b) if span.file == b.file => {
                    (span.line, span.col) >= (b.line, b.col + "\\begin{document}".len() as u32)
                }
                Some(_) => true,
                None => false,
            }
            && end.is_none_or(|e| span.file != e.file || (span.line, span.col) < (e.line, e.col))
    };
    // A parameter TeX reads as it typesets (`\parskip`) left other than
    // the format had it acts whatever the document does, a register so left
    // once code outside the package reads it while the body is typeset (the
    // class's `\textheight`): vertices are numbered in the order the run made them.
    let ended = analysis
        .facts
        .occurrences
        .iter()
        .find(|o| o.kind == crate::builtins::OccKind::EndEnvironment && o.key == "document");
    let typeset = analysis
        .facts
        .expansions
        .iter()
        .filter(|e| body(e.span))
        .filter_map(|e| e.node)
        .chain(ended.map(|o| o.node))
        .min();
    let mut readers: BTreeMap<crate::env::NodeId, BTreeSet<crate::tex::FileId>> = BTreeMap::new();
    for from in typeset.unwrap_or(crate::env::NodeId::MAX)..analysis.graph.len() as crate::env::NodeId {
        let Some(reader) = analysis.graph.vertex(from) else { continue };
        for &(to, kind) in analysis.graph.outgoing(from) {
            if kind.intersects(crate::graph::EdgeKind::READS) {
                readers.entry(to).or_default().insert(reader.span.file);
            }
        }
    }
    let mut provided: BTreeMap<Sym, BTreeSet<Sym>> = BTreeMap::new();
    let mut acts: BTreeSet<Sym> = BTreeSet::new();
    for sym in analysis.env.bound() {
        let Some(binding) = analysis.env.slot(sym) else { continue };
        let Some((&last, file)) = binding.defs.last().and_then(|n| Some((n, analysis.graph.vertex(*n)?.span.file)))
        else {
            continue;
        };
        let format = analysis.kernel.as_ref().and_then(|k| k.slot(sym));
        let parameter = matches!(binding.meaning.prim(), Some(p) if p.is_parameter());
        let register = matches!(binding.meaning, crate::tex::Meaning::Register(..));
        let set = (parameter || register) && format.is_none_or(|f| f.value != binding.value);
        // Its own setting: no other code of the run wrote it (`\count@` is everyone's).
        let read = |package: &Sym| {
            let own = |f: &crate::tex::FileId| files[package].contains(f);
            binding.defs.iter().filter_map(|&n| analysis.graph.vertex(n)).all(|v| own(&v.span.file))
                && readers.get(&last).is_some_and(|r| r.iter().any(|f| !own(f)))
        };
        for package in of(file) {
            if set && (parameter || read(&package)) {
                acts.insert(package);
            } else if format.is_none() && analysis.env.is_defined(sym) {
                provided.entry(package).or_default().insert(sym);
            }
        }
    }
    let graph = &analysis.calls;
    let roots: crate::fixed::HashSet<Sym> =
        analysis.facts.expansions.iter().filter(|e| body(e.span)).map(|e| e.name).collect();
    let mut reached = vec![false; graph.nodes.len()];
    let mut queue: Vec<usize> = (0..graph.nodes.len()).filter(|&i| roots.contains(&graph.nodes[i])).collect();
    while let Some(i) = queue.pop() {
        if std::mem::replace(&mut reached[i], true) {
            continue;
        }
        queue.extend(graph.edges[i].iter().copied().filter(|&j| !reached[j]));
    }
    let used: crate::fixed::HashSet<Sym> =
        roots.iter().copied().chain((0..graph.nodes.len()).filter(|&i| reached[i]).map(|i| graph.nodes[i])).collect();
    for load in &analysis.facts.loads {
        if !load.kind.is_package()
            || load.status != LoadStatus::Read
            || crate::query::origin(analysis, load.span.file) != "document"
        {
            continue;
        }
        let Some(package) = analysis.interner.lookup(&load.name) else { continue };
        let Some(names) = provided.get(&package) else { continue };
        if names.is_empty() || acts.contains(&package) || names.iter().any(|n| used.contains(n)) {
            continue;
        }
        let edits = report.edits().delete_load(load);
        report.add_fix(
            load.span,
            &load.name,
            format!("{} defines {} names, none of which are used", load.name, names.len()),
            Fix::new(
                format!("delete `\\usepackage{{{}}}` if its side effects are not needed", load.name),
                Applicability::Unsafe,
                edits,
            ),
        );
    }
}

fn recursive_macro(report: &mut Report) {
    let analysis = report.analysis();
    let index = report.index();
    let sites = index.definition_order();
    // A cycle nothing expands costs nothing to run.
    let found: Vec<(Span, String, String)> = index
        .recursion()
        .iter()
        .filter(|r| r.observed)
        .filter_map(|crate::machine::Recursion { name, cycle, .. }| {
            let &span = sites.get(name)?;
            let rendered = analysis.interner.cs(*name);
            let message = if cycle.len() > 1 {
                let others: Vec<String> =
                    cycle.iter().filter(|s| *s != name).map(|s| analysis.interner.cs(*s)).collect();
                format!("{rendered} is mutually recursive with {}", others.join(", "))
            } else {
                format!("{rendered} calls itself")
            };
            Some((span, rendered, message))
        })
        .collect();
    for (span, rendered, message) in found {
        report.add(span, &rendered, message, None);
    }
}

fn package_after_preamble(report: &mut Report) {
    let analysis = report.analysis();
    let Some(end) = report.index().document_start() else { return };
    for load in &analysis.facts.loads {
        if !load.kind.is_package() || load.span.file != end.file || load.span.before(end.line, end.col) {
            continue;
        }
        let edits = report.edits().move_to_preamble(load, end);
        report.add_fix(
            load.span,
            &load.name,
            format!("{} is loaded after `\\begin{{document}}`", load.name),
            Fix::new("move the `\\usepackage` into the preamble", Applicability::Unsafe, edits),
        );
    }
    let refused = refused_loads(report, end);
    for load in refused {
        let edits = report.edits().move_to_preamble(&load, end);
        report.add_fix(
            load.span,
            &load.name,
            format!("{} is requested after `\\begin{{document}}`, where LaTeX refuses it", load.name),
            Fix::new("move the `\\usepackage` into the preamble", Applicability::Unsafe, edits),
        );
    }
}

/// Load requests the kernel refused after `\begin{document}`: `\@onlypreamble`
/// lets the command to `\@notprerr`, whose `\errmessage` stands at the call.
/// The command is a package request when a preamble meaning of it passes the
/// kernel's `\@pkgextension`.
fn refused_loads(report: &Report, begin: crate::tex::Span) -> Vec<crate::facts::Load> {
    let analysis = report.analysis();
    let index = report.index();
    let edits = report.edits();
    let interner = &analysis.interner;
    let Some(extension) = interner.lookup("@pkgextension") else { return Vec::new() };
    let mut asks: BTreeMap<Sym, bool> = BTreeMap::new();
    let mut requests = |name: Sym| {
        *asks.entry(name).or_insert_with(|| {
            analysis.facts.defs.iter().any(|def| {
                def.name == name
                    && def.mac.as_ref().is_some_and(|m| m.replacement_text.iter().any(|t| t.cs() == Some(extension)))
            })
        })
    };
    let mut out = Vec::new();
    let mut seen = BTreeSet::new();
    for expansion in &analysis.facts.expansions {
        let span = expansion.span;
        if span.file != begin.file || span.before(begin.line, begin.col) || !seen.insert((span.line, span.col)) {
            continue;
        }
        if index.load_spans().contains(&span) || !index.error_spans().contains(&span) || !requests(expansion.name) {
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
    let at =
        depp.loaded.as_ref().map(|(span, _)| *span).unwrap_or_else(|| crate::tex::Span::new(analysis.main_file, 1, 1));
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

/// The command that dumps a format from this document's preamble, as
/// mylatexformat's manual gives it: the `ini` engine, the LaTeX format it
/// starts from, then `mylatexformat.ltx` and the document.
fn dump_command(engine: crate::config::Engine, job: &str, file: &str) -> String {
    use crate::config::Engine;
    let (ini, format) = match engine {
        Engine::LuaTeX => ("luahbtex", "lualatex"),
        Engine::XeTeX => ("xetex", "xelatex"),
        // LaTeX needs ε-TeX, so a dump is always pdfTeX's.
        Engine::Tex | Engine::PdfTeX => ("pdftex", "pdflatex"),
    };
    format!("{ini} -ini -jobname={job} \"&{format}\" mylatexformat.ltx \"\"\"{file}\"\"\"")
}

fn preamble_cost(report: &mut Report) {
    let analysis = report.analysis;
    let direct =
        analysis.facts.loads.iter().filter(|l| l.kind.is_package() && l.span.file == analysis.main_file).count();
    let (files, steps) = analysis.preamble_cost.unwrap_or((analysis.files.len(), analysis.steps));
    // Only a preamble that is most of the build, and no small one, is worth a format.
    if !analysis.settings.lints.build_cost.costly(steps, analysis.steps) {
        return;
    }
    let span = report.index().document_start().unwrap_or_default();
    let message = format!(
        "{direct} packages requested directly, {files} files read, {}% of the tokens the build takes",
        steps * 100 / analysis.steps.max(1)
    );
    // A format the build already starts from leaves nothing to suggest.
    if analysis.plugins.preload.is_some() {
        report.add(span, "", message, None);
        return;
    }
    let path = analysis.file_name(analysis.main_file).to_string();
    let name = std::path::Path::new(&path);
    let job = name.file_stem().and_then(|s| s.to_str()).unwrap_or("preamble");
    let file = name.file_name().and_then(|s| s.to_str()).unwrap_or(&path);
    let command = dump_command(analysis.plugins.engine, job, file);
    // TeX honors `%&` only as the very first line of the file.
    let edits = report.edits().insert_first(analysis.main_file, &format!("%&{job}"));
    report.add_fix(
        span,
        "",
        message,
        Fix::new(
            format!("dump the preamble into a format with `{command}`, then every build starts from it"),
            Applicability::Unsafe,
            edits,
        ),
    );
}

fn includeonly_available<'a>(report: &mut Report<'a>) {
    let analysis = report.analysis;
    let preamble = analysis.preamble_cost.map_or(0, |(_, steps)| steps);
    let body = analysis.steps.saturating_sub(preamble);
    // `\includeonly` switches `\if@partsw` on (ltfiles.dtx).
    let only = analysis.interner.lookup("if@partsw").is_none_or(|sym| {
        analysis.env.meaning(sym).prim() != Some(crate::builtins::Primitive::If(crate::builtins::Cond::False))
    });
    // An `\include`d part is read with an `.aux` file of its own (ltfiles.dtx).
    let stem = |f: &'a str| std::path::Path::new(f).file_stem().and_then(|s| s.to_str());
    let aux: BTreeSet<&str> =
        analysis.facts.written_files.iter().filter(|f| f.ends_with(".aux")).filter_map(|f| stem(f)).collect();
    let parts: Vec<&str> = analysis
        .facts
        .loads
        .iter()
        .filter(|l| l.span.file == analysis.main_file && stem(&l.name).is_some_and(|s| aux.contains(s)))
        .map(|l| l.name.as_str())
        .collect();
    let cost = &analysis.settings.lints.build_cost;
    let long = analysis.justified_text >= cost.characters;
    if only || parts.len() < 2 || !long || !cost.costly(body, body + preamble) {
        return;
    }
    let Some(span) = report.index().document_start() else { return };
    let edits = report.edits().insert_before(span, &format!("\\includeonly{{{}}}", parts.join(",")));
    report.add_fix(
        span,
        "",
        format!("the build reads all {} `\\include`'d parts", parts.len()),
        Fix::new("while drafting, list only the parts being written in `\\includeonly`", Applicability::Unsafe, edits),
    );
}
