//! A package the document loads that this machine does not have, but the
//! distribution could install.  `unresolved-file` reports the file the
//! search did not find and cannot say why. This rule separates the typo
//! from the package that is only missing here, and names the package that
//! ships the file, which is often not the one the document loads:
//! `subcaption.sty` is in `caption`.
//!
//! The answer comes from the distribution's own manager
//! ([`crate::plugins::packages::Manager`]), never from a table here, and
//! only while `plugins.packages` is on.

use super::{Category, Ref, Report, Rule, Severity, Source};
use crate::engine::tex::CatcodeTable;
use crate::linter::fix::{Applicability, Edit, Fix, Pos, Scan};
use crate::plugins::packages::{Manager, Provided, shell, tree_writable, unresolved};

pub static RULES: &[Rule] = &[
    Rule {
        code: "uninstalled-package",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "the package is not installed, and the distribution could install it",
        explanation: "\
The file was not found in any tree the run searched, and the distribution's
package manager names a package that ships it.  The load is therefore no
typo: the package is only missing from this machine.  A LaTeX package and
the package that carries it often differ in name, so the finding names both.

Fix by running the command the finding gives, or `satex install`, which never
runs as part of `lint --fix`.  The question is off by default because it reads
the package repository: turn on `plugins.packages` in `satex.yaml`.",
        references: &[
            Ref { source: Source::TexLive, at: "tlmgr" },
            Ref { source: Source::MikTeX, at: "miktex packages" },
        ],
        fixes: &[],
        engines: &[],
        run: uninstalled_packages,
    },
    Rule {
        code: "package-successor",
        category: Category::Style,
        severity: Severity::Info,
        summary: "the document loads a package that a successor continues",
        explanation: "\
Some packages live on in a fork or a successor that keeps their interface
and adds to it: `wrapfig2` continues `wrapfig`.  The rule names the
successor and what moving to it changes, and its fix loads the successor in
the package's place.  It stays quiet where the move would break: when another
package loads the old one too (wrapfig2 refuses to load next to wrapfig), when
the successor is loaded already, or when it is not installed.

The pairs are data, not code: `lints.package-successor.successors` lists
each package, its successor and what changes, and a project adds its own or
clears the list.",
        references: &[],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: package_successors,
    },
];

/// Each package the document loads itself whose successor would take its place.
fn package_successors(report: &mut Report) {
    let analysis = report.analysis();
    let loads = &analysis.facts.loads;
    let loaded = |name: &str| loads.iter().any(|l| l.kind.is_package() && l.name == name);
    let installed = |name: &str| {
        crate::caches::distribution::tex_file(&analysis.distribution.roots, &format!("{name}.sty")).is_some()
    };
    for entry in &analysis.settings.lints.package_successor.successors {
        let (from, to) = (entry.package.as_str(), entry.successor.as_str());
        let mut own = loads.iter().filter(|l| l.kind.is_package() && l.name == from);
        let by_package = |l: &&crate::analysis::facts::Load| l.by.is_some() || l.depth > 0;
        let Some(load) = own.clone().find(|l| !by_package(l)) else { continue };
        // a package that loads the old one keeps it loaded next to the successor
        if own.any(|l| by_package(&l)) || loaded(to) || !installed(to) {
            continue;
        }
        let edits = rename_load(report, load.span, from, to);
        let message = format!("`{from}` is continued by `{to}`: {}", entry.changes);
        let fix = Fix::new(format!("load `{to}` instead of `{from}`"), Applicability::Unsafe, edits);
        report.add_fix(load.span, from, message, fix);
    }
}

/// The edit that names `to` for `from` in the `\usepackage{…}` list at `span`.
fn rename_load(report: &Report, span: crate::engine::tex::Span, from: &str, to: &str) -> Option<Vec<Edit>> {
    let (path, text) = report.edits().text(span.file)?;
    let mut scan = Scan::at(&text, Pos::new(span.line, span.col), CatcodeTable::latex())?;
    let head = scan.advance()?;
    matches!(scan.name(&head)?, "usepackage" | "RequirePackage").then_some(())?;
    scan.optional();
    let list = scan.group()?;
    let (base, inner) = (text.offset(list.inner_start)?, text.slice(list.inner_start, list.inner_end)?);
    let mut at = 0;
    for item in inner.split(',') {
        let lead = item.len() - item.trim_start().len();
        if item.trim() == from {
            let start = base + at + lead;
            let (start, end) = (text.pos(start), text.pos(start + from.len()));
            return Some(vec![Edit { path, start, end, replacement: to.to_string() }]);
        }
        at += item.len() + 1;
    }
    None
}

fn uninstalled_packages(report: &mut Report) {
    let analysis = report.analysis();
    let Ok(manager) = Manager::enabled(&analysis.settings, analysis.plugins.provider) else { return };
    if analysis.distribution.indexed == 0 {
        return;
    }
    let usermode = tree_writable(&analysis.distribution.roots) == Some(false);
    for missing in unresolved(analysis) {
        let Provided::By(package) = manager.provides(&missing.file) else { continue };
        let command = shell(&manager.install_command(std::slice::from_ref(&package), usermode));
        let message = format!("`{}` is not installed here, and `{package}` ships {}", missing.name, missing.file);
        report.add(missing.span, &missing.name, message, Some(format!("run `{command}`, or `satex install`")));
    }
}
