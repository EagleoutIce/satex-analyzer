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
//!
//! The other rules here read how loads relate: a name one package made
//! that a later one makes again, a load the class or a package made
//! already, a name close to an installed package's.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::{Category, Ref, Report, Rule, Severity, Source};
use crate::analysis::facts::{Definition, Load};
use crate::engine::builtins::LoadKind;
use crate::engine::tex::{CatcodeTable, Sym};
use crate::linter::fix::{Applicability, Edit, Fix, Pos, Scan};
use crate::plugins::packages::{Manager, Provided, installable, shell, tree_writable, unresolved};

pub static RULES: &[Rule] = &[
    Rule {
        code: "uninstalled-package",
        since: "0.2.0",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "package not installed",
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
        since: "0.2.1",
        category: Category::Style,
        severity: Severity::Info,
        summary: "package has a successor",
        explanation: "\
Some packages have a successor with the same commands and more.  For
example, `wrapfig2` continues `wrapfig`.  The rule names the successor and
what changes.  Its fix loads the successor instead.  It stays quiet where the
switch would break something: when another package also loads the old one
(wrapfig2 refuses to load next to wrapfig), when the successor is loaded
already, or when it is not installed.

The pairs are data, not code: `lints.package-successor.successors` lists
each package, its successor and what changes, and a project adds its own or
clears the list.",
        references: &[],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: package_successors,
    },
    Rule {
        code: "package-load-order",
        since: "0.2.5",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "package overrides an earlier one",
        explanation: "\
A package loaded later redefines a command an earlier package made, and the
document uses that command.  The later definition wins, often by accident.
The finding names the command and both packages, as the run saw them.  A
command the document defines itself, or one the later package gives the same
meaning again, is none.

Where the earlier package defines the command without checking that it is
new, loading the later package first keeps the earlier version.  The fix
then moves the later `\\usepackage` above the earlier one.  The rule stays
quiet where the later package names the earlier one in its own file: it
adapts it on purpose, often after `\\@ifpackageloaded`.",
        references: &[Ref { source: Source::Source2e, at: "ltclass.dtx" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: package_load_order,
    },
    Rule {
        code: "redundant-package-load",
        since: "0.2.5",
        category: Category::Style,
        severity: Severity::Info,
        summary: "package loaded already",
        explanation: "\
The class or an earlier package loads this package already, with the options
the document asks for.  LaTeX ignores the second request.  The document's own
request twice is `duplicate-package`, and one with new options is
`option-clash`.

Fix by deleting the request.  Keep it if the document should still work with
another class that does not load the package.",
        references: &[Ref { source: Source::Source2e, at: "ltclass.dtx" }],
        fixes: &[Applicability::Safe],
        engines: &[],
        run: redundant_package_load,
    },
    Rule {
        code: "unknown-package-name",
        since: "0.2.5",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "package name misspelled",
        explanation: "\
No file of this name is installed, and an installed package has a name a
letter or two away.  The load is then most likely a typo, and LaTeX stops
with `File … not found`.  A name the package manager can install is
`uninstalled-package` instead, and a name with no close match stays
`unresolved-file`.

Fix by loading the installed package the finding names.",
        references: &[Ref { source: Source::Source2e, at: "ltclass.dtx" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: unknown_package_names,
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

/// The installed package or class a missing `file` (`graphix.sty`) most
/// likely misspells, unless the package manager could install `file`
/// itself. `unresolved-file` skips what this answers for.
pub(crate) fn respelled(analysis: &crate::analysis::machine::Analysis, file: &str) -> Option<String> {
    let (stem, extension) = file.rsplit_once('.')?;
    if analysis.distribution.indexed == 0 || installable(analysis, file).is_some() {
        return None;
    }
    let roots = &analysis.distribution.roots;
    let names = crate::caches::distribution::names_with(roots, "tex", &format!(".{extension}"));
    super::correctness::closest(stem, &names).filter(|n| *n != stem).map(str::to_string)
}

/// Each package or class missing here whose name is near an installed one's.
fn unknown_package_names(report: &mut Report) {
    let analysis = report.analysis();
    for missing in unresolved(analysis) {
        let Some(near) = respelled(analysis, &missing.file) else { continue };
        let edits = rename_load(report, missing.span, &missing.name, &near);
        let message = format!("`{}` is not installed, but `{near}` is", missing.name);
        report.add_fix(
            missing.span,
            &missing.name,
            message,
            Fix::new(format!("load `{near}`"), Applicability::Unsafe, edits),
        );
    }
}

/// The package or class that loaded `load`'s package first, on every path,
/// when `load` is the document's own and adds no option to `options`.
pub(crate) fn loaded_before(load: &Load, options: &[String], first: Option<&Load>) -> Option<Sym> {
    let own = load.by.is_none() && load.depth == 0 && load.options.iter().all(|o| options.contains(o));
    first.filter(|f| own && f.certain).and_then(|f| f.by)
}

/// The document's own loads of a package the class or another package loaded
/// first, with every option the document gives.
fn redundant_package_load(report: &mut Report) {
    let analysis = report.analysis();
    let loads = analysis.facts.loads.as_slice();
    for (load, options, _, first) in report.index().reloads() {
        let (Some(by), Some(first)) = (loaded_before(load, options, first), first) else { continue };
        let by = analysis.interner.name(by);
        // the request stands in the kernel: name the document's line that led to it
        let at = root_load(loads, &analysis.interner, first).span;
        let edits = report.edits().delete_load(load);
        report.add_fix(
            load.span,
            &load.name,
            format!("`{by}` loads `{}` already, by the request at {}", load.name, report.place(at)),
            Fix::new(format!("delete this `\\usepackage{{{}}}`", load.name), Applicability::Safe, edits),
        );
    }
}

/// The load a package came in by at the top: its own, or the first load of
/// the package that requested it, and so on up.
fn root_load<'a>(loads: &'a [Load], interner: &crate::engine::tex::Interner, load: &'a Load) -> &'a Load {
    let mut at = load;
    for _ in 0..loads.len() {
        match at.by.and_then(|by| loads.iter().find(|l| l.name == interner.name(by))) {
            Some(parent) if !std::ptr::eq(parent, at) => at = parent,
            _ => break,
        }
    }
    at
}

/// Whether `text` holds `name` as a word of its own.
fn names(text: &str, name: &str) -> bool {
    let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '-' || c == '@');
    text.match_indices(name)
        .any(|(at, _)| !word(text[..at].chars().next_back()) && !word(text[at + name.len()..].chars().next()))
}

/// Names the document uses that a package redefines after another package
/// made them: one finding per pair of packages, at the later one's load.
fn package_load_order(report: &mut Report) {
    let analysis = report.analysis();
    let (interner, loads, index) = (&analysis.interner, analysis.facts.loads.as_slice(), report.index());
    let mut by_name: HashMap<&str, &Load> = HashMap::default();
    loads.iter().rev().filter(|l| l.kind == LoadKind::Package).for_each(|l| _ = by_name.insert(&l.name, l));
    // made by the package's own file, not by code it called
    let own = |d: &Definition| {
        let load = by_name.get(interner.name(d.package?)).copied()?;
        (load.file == Some(d.span.file)).then_some(load)
    };
    let used: HashSet<Sym> =
        analysis.facts.expansions.iter().filter(|e| index.origin(e.span.file) == "document").map(|e| e.name).collect();
    let mut defs: Vec<&Definition> = analysis.facts.defs.iter().filter(|d| used.contains(&d.name)).collect();
    defs.sort_by_key(|d| d.node);
    let mut first: HashMap<Sym, &Definition> = HashMap::default();
    let mut seen = HashSet::new();
    // by the later package's load, then the earlier one's: their places in `loads`
    let mut pairs: BTreeMap<(usize, usize), Vec<Sym>> = BTreeMap::new();
    let place = |l: &Load| loads.iter().position(|x| std::ptr::eq(x, l)).unwrap_or_default();
    // A name the document defines itself is its own by the time it uses it: a scratch name
    // (`\\@tempa`, `\\do`) every file defines before use.
    let here = |d: &&Definition| d.package.is_none() && index.origin(d.span.file) == "document";
    let defined_here: HashSet<Sym> = analysis.facts.defs.iter().filter(here).map(|d| d.name).collect();
    for def in defs {
        let made = *first.entry(def.name).or_insert(def);
        let (Some(a), Some(b)) = (own(made), own(def)) else { continue };
        // The same meaning again changes nothing (`\\Gin@driver` by xcolor and by graphics).
        let changes = made.mac != def.mac && !defined_here.contains(&def.name);
        if a.name != b.name && def.redefines && def.certain && changes && seen.insert(def.name) {
            pairs.entry((place(b), place(a))).or_default().push(def.name);
        }
    }
    let mut read: HashMap<&str, String> = HashMap::default();
    for ((b, a), found) in pairs {
        let (a, b) = (&loads[a], &loads[b]);
        // a package that names the other in its own file adapts it on purpose
        let text = read
            .entry(&b.name)
            .or_insert_with(|| b.path.as_deref().and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default());
        if names(text, &a.name) {
            continue;
        }
        let (ra, rb) = (root_load(loads, interner, a), root_load(loads, interner, b));
        if rb.by.is_some() || rb.kind != LoadKind::Package {
            continue;
        }
        let made = first[&found[0]];
        let name = interner.name(made.name);
        let more = match found.len() - 1 {
            0 => String::new(),
            n => format!(" and {n} more"),
        };
        let mut message = format!("`{}` redefines `\\{name}`{more}, which `{}` made", b.name, a.name);
        // a checked definition (`\newcommand`) would refuse the name the later package made
        let checked = interner.name(made.by).to_lowercase();
        if checked.starts_with("new") || checked.starts_with("provide") {
            report.add(rb.span, &b.name, message, None);
            continue;
        }
        message.push_str(&format!(": loading `{}` before `{}` keeps `{}`'s version", b.name, a.name, a.name));
        let movable =
            ra.by.is_none() && ra.kind == LoadKind::Package && ra.span != rb.span && ra.span.file == rb.span.file;
        let edits = movable.then(|| report.edits().move_before(rb, ra.span)).flatten();
        let fix = Fix::new(format!("load `{}` before `{}`", b.name, a.name), Applicability::Unsafe, edits);
        report.add_fix(rb.span, &b.name, message, fix);
    }
}
