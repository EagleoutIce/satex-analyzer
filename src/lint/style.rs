use std::collections::{BTreeMap, BTreeSet};

use crate::builtins::{OccKind, Primitive};
use crate::facts::CsnameRole;
use crate::lint::fix::{Applicability, Fix};
use crate::lint::shared::labels;
use crate::lint::{Category, Report, Rule, Severity};
use crate::machine::Analysis;
use crate::mode::Modes;
use crate::tex::{Span, Sym, Tok, Token};

pub static RULES: &[Rule] = &[
    Rule {
        code: "dead-definition",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a definition is replaced before anything uses it",
        explanation: "\
The name is defined, and defined again, with no use in between: the first
replacement text never reaches the typesetter.  Both definitions are made on
every path, so this is not a conditional fallback.

Fix by deleting the first definition, or by checking which one was meant.",
        run: dead_definition,
    },
    Rule {
        code: "unused-label",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a label is never referenced",
        explanation: "\
Nothing refers to this label.  Harmless, but usually a leftover or a typo in
the reference.

Fix by deleting the `\\label`, or by checking the `\\ref` that was meant to use it.",
        run: unused_label,
    },
    Rule {
        code: "group-local-definition",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "a definition is made inside a group and lost when it closes",
        explanation: "\
`\\def` and `\\newcommand` are local.  Inside `{…}`, `\\begingroup…\\endgroup` or
an environment, the definition disappears at the closing brace.

Fix by moving the definition out of the group, or by prefixing it with
`\\global`.",
        run: group_local_definition,
    },
    Rule {
        code: "primitive-tex-command",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a plain TeX primitive is used where LaTeX has its own interface",
        explanation: "\
chktex warning 41 (`You ought to not use primitive TeX in LaTeX code`) flags
the primitives in chktexrc's `Primitives` list, which chktex leaves off by
default.  satex reports one only where the name still means the primitive, and
names the LaTeX command for the same job where there is one.  For `\\csname`
the job is what the run saw the name formed for: defined after
`\\expandafter\\def`, made an alias after `\\expandafter\\let`, tested by
`\\expandafter\\ifx…\\relax`, or used; the suggestion is etoolbox's command
for it (`\\csdef`, `\\csgdef`, `\\csedef`, `\\csxdef`, `\\cslet`, `\\letcs`,
`\\ifcsundef`, `\\csuse`) when the document has it, else the kernel's
(`\\@namedef`, `\\@ifundefined`, `\\@nameuse`).
`latex_alternatives` in `satex.yaml` adds entries or overrides them.

Fix by using LaTeX's interface for the same job, if one exists.",
        run: primitive_tex_command,
    },
    Rule {
        code: "unused-definition",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a macro defined in the document is never used",
        explanation: "\
Nothing expands this name.  It may be intended for later, or it may be dead.

Fix by deleting it if it is dead.",
        run: unused_definition,
    },
    Rule {
        code: "stray-space",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a macro body picked up a space from an unescaped line end",
        explanation: "\
Inside a macro's replacement text, TeX turns the end of a line into a space
token unless a control word already skipped it, or a `%` swallowed the rest
of the line ([tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) § 347).  This is the classic missing `%`: an extra
space nobody wrote, which shows up wherever the macro is used.

A body wrapped start-to-end in the kernel's own `\\@bsphack…\\@esphack` pair is
exempt, since that pair keeps whatever is inside it from reaching the page.
The finding follows the modes the run was in each time it read that space
([tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) § 1043: a space is glue only in horizontal mode): it says the
macro *inserts* a space when every reading was in horizontal mode, that it
*may* when some might have been, and nothing when none was or the space
was never read as material (an argument delimiter, a `\\write`).

Fix by writing `%` at the end of that line.",
        run: stray_space,
    },
];

fn bsphacked(body: &[Token], bsphack: Option<Sym>, esphack: Option<Sym>) -> bool {
    match (body.first(), body.last(), bsphack, esphack) {
        (Some(first), Some(last), Some(b), Some(e)) => first.tok == Tok::Cs(b) && last.tok == Tok::Cs(e),
        _ => false,
    }
}

/// Whether `span` is where a line ended: the [`Mouth`](crate::tex::Mouth)
/// captures a space token's span before consuming the character, so an
/// end-of-line space's column sits one past the line's last character,
/// where an explicit space's does not.
fn line_end_space(report: &Report, span: Span) -> bool {
    let Some((_, text)) = report.edits().text(span.file) else { return false };
    span.col == text.line(span.line).chars().count() as u32 + 1
}

fn stray_space(report: &mut Report) {
    let analysis = report.analysis;
    let bsphack = analysis.interner.lookup("@bsphack");
    let esphack = analysis.interner.lookup("@esphack");
    let used: BTreeSet<Sym> = analysis.facts.expansions.iter().map(|e| e.name).collect();
    let defs: Vec<_> = analysis.facts.defs.clone();
    for def in defs {
        let Some(mac) = def.mac.clone() else { continue };
        if def.package.is_some()
            || !used.contains(&def.name)
            || !crate::lint::shared::written_definition(analysis, &def)
        {
            continue;
        }
        let body = &mac.replacement_text;
        if bsphacked(body, bsphack, esphack) {
            continue;
        }
        // Two space tokens never sit next to each other in a replacement
        // text ([tex.web](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) § 347), so each has real material beside it.
        for tok in body.iter() {
            if tok.is_space() && line_end_space(report, tok.span) {
                let modes = analysis.facts.spaces.get(&tok.span).copied().unwrap_or_default();
                let verb = match (modes.meets(Modes::ANY_HORIZONTAL), modes.within(Modes::ANY_HORIZONTAL)) {
                    (false, _) => continue,
                    (true, true) => "inserts",
                    (true, false) => "may insert",
                };
                let name = report.cs(def.name);
                let message = format!("{name}'s body ends a line without `%`; using {name} {verb} a space there");
                let edits = report.edits().insert_percent(tok.span);
                report.add_fix(
                    tok.span,
                    &name,
                    message,
                    Fix::new("write % at the end of this line", Applicability::Safe, edits),
                );
            }
        }
    }
}

fn unused_label(report: &mut Report) {
    let analysis = report.analysis;
    let referenced: BTreeSet<&str> =
        analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Ref).map(|o| o.key.as_str()).collect();
    let unused: Vec<(Span, String)> = labels(analysis)
        .into_iter()
        .filter(|(key, _)| !referenced.contains(key))
        .map(|(key, span)| (span, key.to_string()))
        .collect();
    for (span, key) in unused {
        let edits = report.edits().delete_command(span, &key);
        report.add_fix(
            span,
            &key,
            format!("label `{key}` is never referenced"),
            Fix::new(format!("delete \\label{{{key}}}"), Applicability::Unsafe, edits),
        );
    }
}

fn group_local_definition(report: &mut Report) {
    let baseline = report.analysis.document_depth.unwrap_or(0);
    let defs: Vec<_> = report.analysis.facts.defs.clone();
    for def in defs {
        if def.depth <= baseline || def.global || def.package.is_some() {
            continue;
        }
        // A macro that defines a scratch name while it runs inside a group is
        // how TeX code keeps its work local; only a definition written
        // straight in the document can be meant to outlive the group.
        let from_macro = report.analysis.graph.vertices.get(def.node as usize).is_some_and(|v| v.within.is_some());
        if from_macro {
            continue;
        }
        let name = report.cs(def.name);
        report.add(
            def.span,
            &name,
            format!("{name} is defined inside a group and disappears when it closes"),
            Some("move it out of the group, or write \\global before it".to_string()),
        );
    }
}

/// The primitives chktex's warning 41 flags (chktexrc's `Primitives` list), with the LaTeX command that
/// does the same job where there is one (usrguide; the LaTeX Companion).  An
/// empty replacement means LaTeX offers no direct equivalent, so the finding
/// only says that the primitive is plain TeX.  `latex_alternatives` in
/// `satex.yaml` adds to this or overrides an entry.
const LATEX_ALTERNATIVES: &[(&str, &str)] = &[
    ("above", "\\frac"),
    ("advance", "\\addtocounter or \\addtolength"),
    ("catcode", ""),
    ("chardef", "\\newcommand"),
    ("closein", ""),
    ("closeout", ""),
    ("copy", "\\usebox"),
    ("count", "\\newcounter and \\value"),
    ("countdef", "\\newcounter"),
    ("cr", "\\\\"),
    ("crcr", "\\\\"),
    ("csname", "\\@nameuse"),
    ("delcode", ""),
    ("dimen", "\\newlength"),
    ("dimendef", "\\newlength"),
    ("divide", "\\setlength with calc"),
    ("expandafter", ""),
    ("font", "\\newfont or \\selectfont"),
    ("hskip", "\\hspace"),
    ("vskip", "\\vspace"),
    ("openout", "\\newwrite"),
];

/// A primitive is only worth reporting while it still means the primitive: a
/// document that redefines `\count` as its own macro is using its own macro.
fn primitive_tex_command(report: &mut Report) {
    let analysis = report.analysis;
    // The alternatives are LaTeX's: a plain TeX document has no other
    // interface than the primitives.
    if analysis.plugins.kernel != crate::plugin::Kernel::Latex {
        return;
    }
    let configured = &analysis.settings.latex_alternatives;
    let expansions: Vec<_> = analysis.facts.expansions.clone();
    for expansion in expansions {
        let name = analysis.interner.name(expansion.name);
        let alternative = match configured.get(name) {
            Some(alternative) => alternative.as_str(),
            None => match LATEX_ALTERNATIVES.iter().find(|(primitive, _)| *primitive == name) {
                Some((_, alternative)) => alternative,
                None => continue,
            },
        };
        if analysis.env.meaning(expansion.name).prim().is_none() {
            continue;
        }
        let role = (analysis.env.meaning(expansion.name).prim() == Some(Primitive::Csname)
            && !configured.contains_key(name))
        .then(|| analysis.facts.csnames.get(&expansion.span).copied())
        .flatten();
        let name = report.cs(expansion.name);
        // The command spelled out expands one step later than the primitive,
        // which an `\expandafter` chain around it may count on.
        let (message, fix) = match role.map(|role| (role, csname_alternative(analysis, role))) {
            Some((role, (purpose, found))) => (
                format!("{name} is plain TeX; here it {purpose}"),
                found.map(|sym| {
                    let edits = report.edits().csname(expansion.span, role, sym);
                    Fix::new(format!("use {}", interface(analysis, sym)), Applicability::Unsafe, edits)
                }),
            ),
            None if alternative.is_empty() => {
                (format!("{name} is plain TeX, and LaTeX has no interface of its own for it"), None)
            }
            None => (
                format!("{name} is plain TeX; LaTeX has its own interface for this"),
                Some(Fix::new(format!("use {alternative}"), Applicability::Unsafe, None)),
            ),
        };
        match fix {
            Some(fix) => report.add_fix(expansion.span, &name, message, fix),
            None => report.add(expansion.span, &name, message, None),
        }
    }
}

/// The command that does what a `\csname` was observed doing: etoolbox's
/// when the run defined it (etoolbox manual §§ 3.1.1, 3.1.2, 3.6.1), else the
/// kernel's (source2e, ltdefns.dtx).
fn csname_alternative(analysis: &Analysis, role: CsnameRole) -> (&'static str, Option<Sym>) {
    let (purpose, candidates): (&str, &[&str]) = match role {
        CsnameRole::Use => ("uses a control sequence by name", &["csuse", "@nameuse"]),
        CsnameRole::Define { global: false, expand: false } => {
            ("defines a control sequence by name", &["csdef", "@namedef"])
        }
        CsnameRole::Define { global: true, expand: false } => {
            ("defines a control sequence by name globally", &["csgdef"])
        }
        CsnameRole::Define { global: false, expand: true } => {
            ("defines a control sequence by name with \\edef", &["csedef"])
        }
        CsnameRole::Define { global: true, expand: true } => {
            ("defines a control sequence by name with \\xdef", &["csxdef"])
        }
        CsnameRole::Let => ("lets a control sequence named by text", &["cslet"]),
        CsnameRole::LetTo => ("lets a control sequence to one named by text", &["letcs"]),
        CsnameRole::Test => ("tests whether a control sequence is undefined", &["ifcsundef", "@ifundefined"]),
    };
    let found =
        candidates.iter().filter_map(|name| analysis.interner.lookup(name)).find(|sym| analysis.env.is_defined(*sym));
    (purpose, found)
}

/// A command, with the package the run saw define it.
fn interface(analysis: &Analysis, sym: crate::tex::Sym) -> String {
    let package = analysis
        .facts
        .defs
        .iter()
        .rev()
        .find(|def| def.name == sym)
        .and_then(|def| def.package)
        .map(|package| format!(" ({})", analysis.interner.name(package)));
    format!("{}{}", analysis.interner.cs(sym), package.unwrap_or_default())
}

/// Whether `name` (a control word, as [`Report::cs`] writes it) reads as the
/// module-internal naming a package or class author would not want the
/// public to rely on: an `@` (kernel convention, source2e `classes.dtx`) or
/// expl3's `module_function:signature` (interface3, "The naming scheme").
fn looks_private(name: &str) -> bool {
    name.contains('@') || (name.contains('_') && name.contains(':'))
}

/// pgfkeys stores a key as `\pgfk@/path/key/.@body` etc.: the user sets it.
fn is_pgf_key(name: &str) -> bool {
    name.starts_with("\\pgfk@")
}

fn unused_definition(report: &mut Report) {
    let analysis = report.analysis;
    let used: BTreeSet<Sym> = analysis.facts.expansions.iter().map(|e| e.name).collect();
    let defs: Vec<_> = analysis.facts.defs.clone();
    let rule = &analysis.settings.lints.unused_definition;
    let ignore: Vec<regex::Regex> = rule.ignore.iter().filter_map(|p| regex::Regex::new(p).ok()).collect();
    // A name one call defines for another it defines, whose text calls it
    // (ltcmd's `\fd code`, `\newcommand`'s `\\fe`), is that name's
    // implementation: the other is the one reported.
    let implementation = |def: &crate::facts::Definition| {
        defs.iter().any(|other| {
            other.span == def.span
                && other.by == def.by
                && other.name != def.name
                && other.mac.as_ref().is_some_and(|m| m.replacement_text.iter().any(|t| t.cs() == Some(def.name)))
        })
    };
    for def in &defs {
        if def.package.is_some()
            || def.tag != "macro"
            || used.contains(&def.name)
            || implementation(def)
            || !crate::lint::shared::written_definition(analysis, def)
        {
            continue;
        }
        let name = report.cs(def.name);
        if (!rule.pgf_keys && is_pgf_key(&name))
            || (!rule.report_public && !looks_private(&name))
            || ignore.iter().any(|re| re.is_match(&name))
        {
            continue;
        }
        let edits = report.edits().delete_definition(def);
        report.add_fix(
            def.span,
            &name,
            format!("{name} is never used"),
            Fix::new(format!("delete {name}"), Applicability::Unsafe, edits),
        );
    }
}

fn dead_definition(report: &mut Report) {
    let analysis = report.analysis;
    let mut uses: BTreeMap<Sym, Vec<crate::env::NodeId>> = BTreeMap::new();
    for expansion in &analysis.facts.expansions {
        if let Some(node) = expansion.node {
            uses.entry(expansion.name).or_default().push(node);
        }
    }
    let mut defs: BTreeMap<Sym, Vec<&crate::facts::Definition>> = BTreeMap::new();
    for def in &analysis.facts.defs {
        if def.certain && def.mac.is_some() && crate::lint::shared::written_definition(analysis, def) {
            defs.entry(def.name).or_default().push(def);
        }
    }
    for (name, sites) in defs {
        let empty = Vec::new();
        let read = uses.get(&name).unwrap_or(&empty);
        for pair in sites.windows(2) {
            let (first, second) = (pair[0], pair[1]);
            if first.span == second.span || first.depth != second.depth {
                continue;
            }
            if read.iter().any(|node| *node > first.node && *node < second.node) {
                continue;
            }
            let rendered = analysis.interner.cs(name);
            // Only a `\def` redefines whatever came before; a `\newcommand` or
            // `\renewcommand` after the deleted one would change what it does.
            let applicability = match analysis.env.meaning(second.by).prim() {
                Some(Primitive::Def { .. }) => Applicability::Safe,
                _ => Applicability::Unsafe,
            };
            let edits = report.edits().delete_definition(first);
            report.add_fix(
                first.span,
                &rendered,
                format!("{rendered} is redefined at {} before anything uses it", second.span),
                Fix::new(format!("delete this definition of {rendered}"), applicability, edits),
            );
        }
    }
}
