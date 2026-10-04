use std::collections::{BTreeMap, BTreeSet};

use crate::builtins::{OccKind, Primitive};
use crate::facts::CsnameRole;
use crate::lint::fix::{Applicability, Fix};
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::machine::Analysis;
use crate::mode::Modes;
use crate::tex::{Catcode, Span, Sym, Tok, Token};

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
        references: &[],
        run: dead_definition,
    },
    Rule {
        code: "unused-label",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a label is never referenced",
        explanation: "\
Fix by deleting the `\\label`, or by checking the `\\ref` that was meant to use it.",
        references: &[],
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
        references: &[Ref { source: Source::TeXbook, at: "chapter 5" }],
        run: group_local_definition,
    },
    Rule {
        code: "primitive-tex-command",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a plain TeX primitive is used where LaTeX has its own interface",
        explanation: "\
chktex warning 41 (`You ought to not use primitive TeX in LaTeX code`) flags
the primitives in chktexrc's `Primitives` list.  satex reports one only where
the name still means the primitive, and names the LaTeX command for the same
job.

For `\\csname` the job is the one the run saw the name formed for — defined,
aliased, tested or used — and the suggestion is
[etoolbox](https://ctan.org/pkg/etoolbox)'s command for it when the document
has it.

Fix by using LaTeX's interface for the same job.",
        references: &[Ref { source: Source::Chktex, at: "41" }, Ref { source: Source::L2tabu, at: "1" }],
        run: primitive_tex_command,
    },
    Rule {
        code: "unused-definition",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a macro defined in the document is never used",
        explanation: "\
Nothing expands this name.

Fix by deleting it, or by checking why nothing calls it yet.",
        references: &[],
        run: unused_definition,
    },
    Rule {
        code: "stray-space",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a macro body picked up a space from an unescaped line end",
        explanation: "\
Inside a macro's replacement text, TeX turns the end of a line into a space
token unless a control word already skipped it or a `%` swallowed the rest
of the line (tex.web § 347).

The finding follows the modes the run read that space in (§ 1043: a space is
glue only in horizontal mode).  It says the macro *inserts* a space where
every reading was horizontal, and that it *may* where some might have been.

Fix by writing `%` at the end of that line.",
        references: &[Ref { source: Source::TeXWeb, at: "347" }, Ref { source: Source::TeXWeb, at: "1043" }],
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
fn line_end_space(text: Option<&crate::lint::fix::Text>, span: Span) -> bool {
    let Some(text) = text else { return false };
    // Only blanks, which tex.web § 31 drops, may stand between it and the end.
    let line = text.line(span.line);
    (line.trim_end_matches(' ').chars().count() as u32 + 1..=line.chars().count() as u32 + 1).contains(&span.col)
}

/// A line end between two characters separates two words, as meant. One
/// right before a paragraph end is the paragraph's last glue, which `\par`
/// removes (tex.web § 1096).
fn harmless_space(
    body: &[Token],
    i: usize,
    par: Option<crate::tex::Sym>,
    starts_par: &std::collections::HashSet<crate::tex::Sym>,
) -> bool {
    let word = |t: Option<&Token>| t.is_some_and(|t| t.is_cat(Catcode::Letter) || t.is_cat(Catcode::Other));
    let next = body.get(i + 1);
    let ends_par = next.and_then(|t| t.cs()).is_some_and(|c| Some(c) == par || starts_par.contains(&c));
    (word(i.checked_sub(1).and_then(|j| body.get(j))) && word(next)) || ends_par
}

fn stray_space(report: &mut Report) {
    let analysis = report.analysis();
    let index = report.index();
    let bsphack = analysis.interner.lookup("@bsphack");
    let esphack = analysis.interner.lookup("@esphack");
    let used = index.expanded();
    let par = analysis.interner.lookup("par");
    // Macros whose text starts a new paragraph (`\def\blk#1{\par…}`).
    let starts_par: std::collections::HashSet<crate::tex::Sym> = analysis
        .facts
        .defs
        .iter()
        .filter(|d| {
            d.mac.as_ref().and_then(|m| m.replacement_text.first()).and_then(|t| t.cs()).is_some_and(|c| Some(c) == par)
        })
        .map(|d| d.name)
        .collect();
    let mut texts: std::collections::HashMap<crate::tex::FileId, Option<std::rc::Rc<crate::lint::fix::Text>>> =
        std::collections::HashMap::new();
    for def in &analysis.facts.defs {
        let Some(mac) = &def.mac else { continue };
        if def.package.is_some() || !used.contains(def.name) || !index.written_definition(def) {
            continue;
        }
        let body = &mac.replacement_text;
        if bsphacked(body, bsphack, esphack) {
            continue;
        }
        // tex.web § 347: two space tokens never sit next to each other in a
        // replacement text, so each has real material beside it.
        for (i, tok) in body.iter().enumerate() {
            if !tok.is_space() || harmless_space(body, i, par, &starts_par) {
                continue;
            }
            let text = texts
                .entry(tok.span.file)
                .or_insert_with(|| report.edits().text(tok.span.file).map(|(_, text)| text))
                .clone();
            if line_end_space(text.as_deref(), tok.span) {
                let modes = analysis.facts.spaces.get(&tok.span).copied().unwrap_or_default();
                let verb = match (modes.meets(Modes::ANY_HORIZONTAL), modes.within(Modes::ANY_HORIZONTAL)) {
                    (false, _) => continue,
                    (true, true) => "inserts",
                    (true, false) => "may insert",
                };
                let name = report.cs(def.name);
                let message = format!("{name}'s body ends a line without `%`; using {name} {verb} a space there");
                // A body whose `}` starts the next line closes on this one instead.
                let closes = i + 1 == body.len()
                    && text.as_deref().is_some_and(|t| t.line(tok.span.line + 1).trim_start().starts_with('}'));
                let fix = if closes {
                    Fix::new(
                        "move the `}` up to this line",
                        Applicability::Safe,
                        report.edits().join_next_line(tok.span),
                    )
                } else {
                    Fix::new(
                        "write % at the end of this line",
                        Applicability::Safe,
                        report.edits().insert_percent(tok.span),
                    )
                };
                report.add_fix(tok.span, &name, message, fix);
            }
        }
    }
}

fn unused_label(report: &mut Report) {
    let analysis = report.analysis();
    let referenced: BTreeSet<&str> =
        analysis.facts.occurrences.iter().filter(|o| o.kind == OccKind::Ref).map(|o| o.key.as_str()).collect();
    let unused: Vec<(Span, String)> = report
        .index()
        .labels()
        .iter()
        .filter(|(key, _)| !referenced.contains(*key))
        .map(|(key, span)| (*span, key.to_string()))
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
    let analysis = report.analysis();
    let baseline = analysis.document_depth.unwrap_or(0);
    for def in &analysis.facts.defs {
        if def.depth <= baseline || def.global || def.package.is_some() {
            continue;
        }
        // A macro that defines a scratch name while it runs inside a group is
        // how TeX code keeps its work local.  Only a definition written
        // straight in the document can be meant to outlive the group.
        let from_macro = analysis.graph.vertices.get(def.node as usize).is_some_and(|v| v.within.is_some());
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

/// The primitives chktex's warning 41 flags (chktexrc's `Primitives` list)
/// for which LaTeX has a command that does the same job (usrguide, the LaTeX
/// Companion).  `latex_alternatives` in `satex.yaml` adds to this or
/// overrides an entry.
const LATEX_ALTERNATIVES: &[(&str, &str)] = &[
    ("above", "\\frac"),
    ("advance", "\\addtocounter or \\addtolength"),
    ("chardef", "\\newcommand"),
    ("copy", "\\usebox"),
    ("count", "\\newcounter and \\value"),
    ("countdef", "\\newcounter"),
    ("cr", "\\\\"),
    ("crcr", "\\\\"),
    ("csname", "\\@nameuse"),
    ("dimen", "\\newlength"),
    ("dimendef", "\\newlength"),
    ("divide", "\\setlength with calc"),
    ("font", "\\newfont or \\selectfont"),
    ("hskip", "\\hspace"),
    ("vskip", "\\vspace"),
    ("openout", "\\newwrite"),
];

// The rest of chktex's `Primitives` list has no LaTeX interface at all:
// `\catcode`, `\closein`, `\closeout`, `\delcode`, `\expandafter` (and
// `\noexpand`, `\relax` and the like, the same story, never listed above
// either).  A finding for one of these could only repeat that back with
// nothing to act on, so [`primitive_tex_command`] does not track them on its
// own — a `latex_alternatives` entry naming one of these (even with an empty
// value) asks for it by name, and is reported.

/// A primitive is only worth reporting while it still means the primitive: a
/// document that redefines `\count` as its own macro is using its own macro.
fn primitive_tex_command(report: &mut Report) {
    let analysis = report.analysis();
    // The alternatives are LaTeX's: a plain TeX document has no other
    // interface than the primitives.
    if analysis.plugins.kernel != crate::plugin::Kernel::Latex {
        return;
    }
    let configured = &analysis.settings.latex_alternatives;
    // One lookup per expansion instead of a scan of the table per expansion.
    let mut table: BTreeMap<Sym, (&str, bool)> = BTreeMap::new();
    for (primitive, alternative) in LATEX_ALTERNATIVES {
        if let Some(sym) = analysis.interner.lookup(primitive) {
            table.insert(sym, (alternative, false));
        }
    }
    for (primitive, alternative) in configured {
        if let Some(sym) = analysis.interner.lookup(primitive) {
            table.insert(sym, (alternative.as_str(), true));
        }
    }
    // `csname_alternative` and `interface` both search the run: the answers
    // repeat for every `\csname` the document writes.
    let mut roles: Vec<(CsnameRole, (&'static str, Option<Sym>))> = Vec::new();
    let mut interfaces: BTreeMap<Sym, String> = BTreeMap::new();
    for expansion in &analysis.facts.expansions {
        let Some(&(alternative, overridden)) = table.get(&expansion.name) else { continue };
        let primitive = analysis.env.meaning(expansion.name).prim();
        if primitive.is_none() {
            continue;
        }
        let role = (primitive == Some(Primitive::Csname) && !overridden)
            .then(|| analysis.facts.csnames.get(&expansion.span).copied())
            .flatten();
        let name = report.cs(expansion.name);
        // The command spelled out expands one step later than the primitive,
        // which an `\expandafter` chain around it may count on.
        let alternate = role.map(|role| {
            let found = match roles.iter().find(|(seen, _)| *seen == role) {
                Some((_, found)) => *found,
                None => {
                    let found = csname_alternative(analysis, role);
                    roles.push((role, found));
                    found
                }
            };
            (role, found)
        });
        let (message, fix) = match alternate {
            Some((role, (purpose, found))) => (
                format!("{name} is plain TeX; here it {purpose}"),
                found.map(|sym| {
                    let edits = report.edits().csname(expansion.span, role, sym);
                    let spelled = interfaces.entry(sym).or_insert_with(|| interface(analysis, sym)).clone();
                    Fix::new(format!("use {spelled}"), Applicability::Unsafe, edits)
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
        // Nothing to act on: `\csname` here, say, whose role satex could not
        // tie to a concrete replacement.  A `latex_alternatives` entry named
        // this primitive on purpose, so its finding stays even with no fix.
        if fix.is_none() && !overridden {
            continue;
        }
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
    let analysis = report.analysis();
    let index = report.index();
    let rule = &analysis.settings.lints.unused_definition;
    let ignore: Vec<regex::Regex> = rule.ignore.iter().filter_map(|p| regex::Regex::new(p).ok()).collect();
    for def in &analysis.facts.defs {
        // A name one call defines for another it defines, whose text calls
        // it (ltcmd's `\fd code`), is that name's implementation.
        if def.tag != "macro"
            || index.implements_sibling(def)
            || !index.own_definition(def)
            || index.used_definition(def)
        {
            continue;
        }
        let name = report.cs(def.name);
        // An internal name (`@`) of a package, class or config file is the
        // implementation of that file, often built with `\csname`: not
        // something its user can act on.
        if name.contains('@')
            && report.edits().text(def.span.file).is_some_and(|(path, _)| {
                std::path::Path::new(&path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| matches!(e, "sty" | "cls" | "cfg" | "def" | "clo"))
            })
        {
            continue;
        }
        // A name not written where it is defined was built there (`\csname`
        // in a macro the line calls, like a `\cite` wrapper): the line holds
        // nothing to delete, and the name is looked up the same way.
        let written = report.edits().text(def.span.file).is_none_or(|(_, text)| {
            let needle = if name.starts_with('\\') { name.clone() } else { format!("\\{name}") };
            text.line(def.span.line).contains(&needle)
        });
        if !written {
            continue;
        }
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
    let analysis = report.analysis();
    let index = report.index();
    let mut written: Vec<&crate::facts::Definition> = analysis
        .facts
        .defs
        .iter()
        .filter(|def| def.certain && def.mac.is_some() && index.own_definition(def))
        .collect();
    // Grouped by name, as the report is ordered.  Only a name defined twice
    // can have a definition nothing reads.
    written.sort_by_key(|def| def.name);
    for sites in written.chunk_by(|a, b| a.name == b.name) {
        let name = sites[0].name;
        for pair in sites.windows(2) {
            let (first, second) = (pair[0], pair[1]);
            // Whatever reads the first definition, through any name, reads
            // it before the second replaces it.
            if first.span == second.span || first.depth != second.depth || index.used_definition(first) {
                continue;
            }
            let rendered = analysis.interner.cs(name);
            // Only a `\def` redefines whatever came before.  A `\newcommand` or
            // `\renewcommand` after the deleted one would change what it does.
            let applicability = match analysis.env.meaning(second.by).prim() {
                Some(Primitive::Def { .. }) => Applicability::Safe,
                _ => Applicability::Unsafe,
            };
            let edits = report.edits().delete_definition(first);
            report.add_fix(
                first.span,
                &rendered,
                format!("{rendered} is redefined at {} before anything uses it", report.place(second.span)),
                Fix::new(format!("delete this definition of {rendered}"), applicability, edits),
            );
        }
    }
}
