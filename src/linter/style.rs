use std::collections::{BTreeMap, BTreeSet};

use crate::analysis::facts::CsnameRole;
use crate::analysis::machine::Analysis;
use crate::engine::builtins::{Material, ModeCmd, OccKind, Primitive};
use crate::engine::mode::Modes;
use crate::engine::tex::{Catcode, Span, Sym, Tok, Token};
use crate::linter::fix::{Applicability, Fix};
use crate::linter::{Category, Ref, Report, Rule, Severity, Source};

pub static RULES: &[Rule] = &[
    Rule {
        code: "dead-definition",
        since: "0.1.0",
        category: Category::Style,
        severity: Severity::Info,
        summary: "definition replaced before use",
        explanation: "\
The name is defined, and defined again, with no use in between: the first
replacement text never reaches the typesetter.  Both definitions are made on
every path, so this is not a conditional fallback.  The first is no
`\\providecommand`, which yields to a later definition on purpose, and no empty
declaration (`\\newcommand\\x{}` before a `\\def\\x`, `\\tl_new:N`), and both
are statements the file reads in order: a definition inside a macro body runs
whenever the macro is called, so a default that a key handler or a later call
overrides is left alone.

Fix by deleting the first definition, or by checking which one was meant.",
        references: &[],
        fixes: &[Applicability::Safe, Applicability::Unsafe],
        engines: &[],
        run: dead_definition,
    },
    Rule {
        code: "unused-label",
        since: "0.1.0",
        category: Category::Style,
        severity: Severity::Info,
        summary: "label never referenced",
        explanation: "\
Fix by deleting the `\\label`, or by checking the `\\ref` that was meant to use it.",
        references: &[],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: unused_label,
    },
    Rule {
        code: "group-local-definition",
        since: "0.1.0",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "definition lost when its group closes",
        explanation: "\
`\\def` and `\\newcommand` are local.  Inside `{…}`, `\\begingroup…\\endgroup` or
an environment, the definition disappears at the closing brace.

Fix by moving the definition out of the group, or by prefixing it with
`\\global`.",
        references: &[Ref { source: Source::TeXbook, at: "chapter 5" }],
        fixes: &[],
        engines: &[],
        run: group_local_definition,
    },
    Rule {
        code: "primitive-tex-command",
        since: "0.1.0",
        category: Category::Style,
        severity: Severity::Info,
        summary: "plain TeX primitive in LaTeX",
        explanation: "\
chktex warning 41 (`You ought to not use primitive TeX in LaTeX code`) flags
the primitives in chktexrc's `Primitives` list.  satex reports one only where
the name still means the primitive, and names the LaTeX command for the same
job.

For `\\csname` the job is the one the run saw the name formed for (defined,
aliased, tested or used), and the suggestion is
[etoolbox](https://ctan.org/pkg/etoolbox)'s command for it when the document
has it.

Fix by using LaTeX's interface for the same job.",
        references: &[Ref { source: Source::Chktex, at: "41" }, Ref { source: Source::L2tabu, at: "1" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: primitive_tex_command,
    },
    Rule {
        code: "unused-definition",
        since: "0.1.0",
        category: Category::Style,
        severity: Severity::Info,
        summary: "macro never used",
        explanation: "\
Nothing expands this name.  A name some other definition of it gives a
meaning that is read is used: a default a later call overrides is no unused
name, and `dead-definition` covers a definition replaced before any use.

Fix by deleting it, or by checking why nothing calls it yet.",
        references: &[],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: unused_definition,
    },
    Rule {
        code: "stray-space",
        since: "0.1.0",
        category: Category::Style,
        severity: Severity::Info,
        summary: "space from a line end in a macro",
        explanation: "\
Inside a macro's replacement text, TeX turns the end of a line into a space
token unless a control word already skipped it or a `%` swallowed the rest
of the line (tex.web § 347).

The finding follows the modes the run read that space in (§ 1043: a space is
glue only in horizontal mode).  It says the macro *inserts* a space where
every reading was horizontal, and that it *may* where some might have been.
Only a space that a character, box or rule follows in its list is reported.
The glue that ends a paragraph goes (§ 816), and glue before it only widens
the end of the last line. One an `\\unskip` takes is gone too (§ 1105), as
at `\\end{center}`.

Fix by writing `%` at the end of that line.",
        references: &[
            Ref { source: Source::TeXWeb, at: "347" },
            Ref { source: Source::TeXWeb, at: "816" },
            Ref { source: Source::TeXWeb, at: "1043" },
        ],
        fixes: &[Applicability::Safe],
        engines: &[],
        run: stray_space,
    },
];

fn bsphacked(body: &[Token], bsphack: Option<Sym>, esphack: Option<Sym>) -> bool {
    match (body.first(), body.last(), bsphack, esphack) {
        (Some(first), Some(last), Some(b), Some(e)) => first.tok == Tok::Cs(b) && last.tok == Tok::Cs(e),
        _ => false,
    }
}

/// Whether `span` is where a line ended: the [`Mouth`](crate::engine::tex::Mouth)
/// captures a space token's span before consuming the character, so an
/// end-of-line space's column sits one past the line's last character,
/// where an explicit space's does not.
fn line_end_space(text: Option<&crate::linter::fix::Text>, span: Span) -> bool {
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
    par: Option<crate::engine::tex::Sym>,
    starts_par: &crate::fixed::HashSet<crate::engine::tex::Sym>,
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
    let starts_par: crate::fixed::HashSet<crate::engine::tex::Sym> = analysis
        .facts
        .defs
        .iter()
        .filter(|d| {
            d.mac.as_ref().and_then(|m| m.replacement_text.first()).and_then(|t| t.cs()).is_some_and(|c| Some(c) == par)
        })
        .map(|d| d.name)
        .collect();
    let mut texts: crate::fixed::HashMap<crate::engine::tex::FileId, Option<std::rc::Rc<crate::linter::fix::Text>>> =
        crate::fixed::HashMap::default();
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
            if line_end_space(text.as_deref(), tok.span) && analysis.facts.kept_spaces.contains(&tok.span) {
                let modes = analysis.facts.spaces.get(&tok.span).copied().unwrap_or_default();
                let verb = match (modes.meets(Modes::ANY_HORIZONTAL), modes.within(Modes::ANY_HORIZONTAL)) {
                    (false, _) => continue,
                    (true, true) => "inserts",
                    (true, false) => "may insert",
                };
                let name = report.cs(def.name);
                let message = format!(
                    "stray space in `{name}`: its body ends a line without `%`, so using `{name}` {verb} a space there"
                );
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
    let unused: Vec<(Span, Option<String>, String)> = report
        .index()
        .labels()
        .iter()
        .filter(|(key, _)| !referenced.contains(*key))
        // A key written into the `.aux` file named after it is an `\include`d
        // part's checkpoint, which the part's own file carries back.
        .filter(|(key, _)| !analysis.facts.written_files.contains(&format!("{key}.aux")))
        .map(|(key, o)| {
            let (span, via) = report.site(o);
            (span, via, key.to_string())
        })
        .collect();
    for (span, via, key) in unused {
        let message = format!("label `{key}` is never referenced{}", via.as_deref().unwrap_or_default());
        if report.unread("referenced").is_some() {
            report.add_absent("referenced", span, &key, message, None);
            continue;
        }
        // Deleting the call would take what else it does with it.
        if via.is_some() {
            report.add(span, &key, message, Some(format!("reference `{key}`, or stop the macro writing it")));
            continue;
        }
        let edits = report.edits().delete_command(span, &key);
        report.add_fix(
            span,
            &key,
            message,
            Fix::new(format!("delete `\\label{{{key}}}`"), Applicability::Unsafe, edits),
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
        // Read before the group closes (`\begingroup\edef\x{…\endgroup…}\x`): it did its work.
        if from_macro || report.index().used_definition(def) {
            continue;
        }
        let name = report.cs(def.name);
        report.add(
            def.span,
            &name,
            format!("{name} is defined inside a group and disappears when it closes"),
            Some("move it out of the group, or write `\\global` before it".to_string()),
        );
    }
}

// chktex's other `Primitives` (`\catcode`, `\expandafter`, …) have no LaTeX
// alternative: reported only when `latex_alternatives` names them.

/// A primitive is only worth reporting while it still means the primitive: a
/// document that redefines `\count` as its own macro is using its own macro.
fn primitive_tex_command(report: &mut Report) {
    let analysis = report.analysis();
    // The alternatives are LaTeX's: a plain TeX document has no other
    // interface than the primitives.
    if analysis.plugins.kernel != crate::plugins::Kernel::Latex {
        return;
    }
    let configured = &analysis.settings.latex_alternatives;
    // One lookup per expansion instead of a scan of the table per expansion.
    let mut table: BTreeMap<Sym, (&str, bool)> = BTreeMap::new();
    // chktex's warning 41 (chktexrc's `Primitives`), from `conventions.alternatives`.
    for (primitive, alternative) in &analysis.settings.conventions.alternatives {
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
    // per role and whether `@` is a letter there: the alternative and the name it may write
    type Answer = ((CsnameRole, bool), (&'static str, Option<Sym>));
    let mut roles: Vec<Answer> = Vec::new();
    let mut interfaces: BTreeMap<Sym, String> = BTreeMap::new();
    for expansion in &analysis.facts.expansions {
        let Some(&(alternative, overridden)) = table.get(&expansion.name) else { continue };
        let primitive = analysis.env.meaning(expansion.name).prim();
        // a package or class of the project is code, where the primitives are its tools (`xlistings.sty`'s
        // `\\csname`); the advice is for the document's text
        let code_file = matches!(
            analysis.files.get(expansion.span.file as usize).map(|f| f.kind),
            Some(crate::engine::builtins::LoadKind::Package | crate::engine::builtins::LoadKind::Class)
        );
        if primitive.is_none() || code_file {
            continue;
        }
        let role = (primitive == Some(Primitive::Csname) && !overridden)
            .then(|| analysis.facts.csnames.get(&expansion.span).copied())
            .flatten();
        let name = report.cs(expansion.name);
        // The document's own files read `@` as other: a kernel `\@namedef` written there breaks the line.
        let document = report.index().origin(expansion.span.file) == "document";
        // The command spelled out expands one step later than the primitive,
        // which an `\expandafter` chain around it may count on.
        let alternate = role.map(|role| {
            let found = match roles.iter().find(|(seen, _)| *seen == (role, document)) {
                Some((_, found)) => *found,
                None => {
                    let found = csname_alternative(analysis, role, document);
                    roles.push(((role, document), found));
                    found
                }
            };
            (role, found)
        });
        let (message, fix) = match alternate {
            Some((role, (purpose, found))) => (
                format!("`{name}` is plain TeX. Here it {purpose}"),
                found.map(|sym| {
                    let edits = report.edits().csname(expansion.span, role, sym);
                    let spelled = interfaces.entry(sym).or_insert_with(|| interface(analysis, sym)).clone();
                    Fix::new(format!("use {spelled}"), Applicability::Unsafe, edits)
                }),
            ),
            None if alternative.is_empty() => {
                (format!("`{name}` is plain TeX, and LaTeX has no interface of its own for it"), None)
            }
            None => {
                // A glue primitive whose alternative is one command takes the
                // same glue as its argument: `\vskip x` is `\vspace{x}`.
                let glue = matches!(
                    primitive,
                    Some(Primitive::Mode(ModeCmd::Vertical(Material::Glue) | ModeCmd::Horizontal(Material::Glue)))
                );
                let command = alternative.strip_prefix('`').and_then(|a| a.strip_suffix('`'));
                let edits = command
                    .filter(|c| glue && !c.contains(' '))
                    .and_then(|c| report.edits().glue_call(expansion.span, c));
                (
                    format!("`{name}` is plain TeX, and LaTeX has its own interface for this"),
                    Some(Fix::new(format!("use {alternative}"), Applicability::Unsafe, edits)),
                )
            }
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
/// kernel's (source2e, ltdefns.dtx). The kernel's `\@namedef` counts only
/// in package code: the document reads `@` as other.
fn csname_alternative(analysis: &Analysis, role: CsnameRole, document: bool) -> (&'static str, Option<Sym>) {
    let c = &analysis.settings.conventions.csname;
    let (purpose, candidates): (&str, &[String]) = match role {
        CsnameRole::Use => ("uses a control sequence by name", &c.r#use),
        CsnameRole::Define { global: false, expand: false } => ("defines a control sequence by name", &c.def),
        CsnameRole::Define { global: true, expand: false } => ("defines a control sequence by name globally", &c.gdef),
        CsnameRole::Define { global: false, expand: true } => {
            ("defines a control sequence by name with `\\edef`", &c.edef)
        }
        CsnameRole::Define { global: true, expand: true } => {
            ("defines a control sequence by name with `\\xdef`", &c.xdef)
        }
        CsnameRole::Let => ("lets a control sequence named by text", &c.r#let),
        CsnameRole::LetTo => ("lets a control sequence to one named by text", &c.let_to),
        CsnameRole::Test => ("tests whether a control sequence is undefined", &c.test),
    };
    let found = candidates
        .iter()
        .filter(|name| !document || crate::linter::shared::writable(name))
        .filter_map(|name| analysis.interner.lookup(name))
        .find(|sym| analysis.env.is_defined(*sym));
    (purpose, found)
}

/// A command, with the package the run saw define it.
fn interface(analysis: &Analysis, sym: crate::engine::tex::Sym) -> String {
    let package = analysis
        .facts
        .defs
        .iter()
        .rev()
        .find(|def| def.name == sym)
        .and_then(|def| def.package)
        .map(|package| format!(" ({})", analysis.interner.name(package)));
    format!("`{}`{}", analysis.interner.cs(sym), package.unwrap_or_default())
}

/// Whether `name` is private by `lints.unused-definition.private`.
fn looks_private(private: &[regex::Regex], name: &str) -> bool {
    let name = crate::engine::tex::bare_name(name);
    private.iter().any(|pattern| pattern.is_match(name))
}

/// pgfkeys stores a key as `\pgfk@/path/key/.@body` etc.: the user sets it.
fn is_pgf_key(name: &str) -> bool {
    name.starts_with("\\pgfk@")
}

/// The names replacement texts hold, whether or not a run expands them: a library read alone
/// calls none of its commands, and a text names what is defined only after it.
struct NamedByText {
    /// Reached over the call graph from the names given.
    reached: crate::fixed::HashSet<Sym>,
    /// Names built from pieces: `\csname foo@#1\endcsname`, `\use:c { __foo_#1:n }`.
    built: regex::RegexSet,
}

impl NamedByText {
    /// What the texts of every name outside `reportable` name, and of every name those name.
    fn of<'a>(
        analysis: &Analysis,
        reportable: &crate::fixed::HashSet<Sym>,
        texts: impl Iterator<Item = &'a crate::analysis::facts::Definition>,
    ) -> NamedByText {
        let graph = &analysis.calls;
        let built = texts.filter_map(|def| def.mac.as_deref()).flat_map(|m| built_names(analysis, &m.replacement_text));
        let built = regex::RegexSet::new(built).unwrap_or_else(|_| regex::RegexSet::empty());
        // The token that names a macro in its own definition is no use of it, wherever the
        // definer's plumbing keeps it (`\reserved@a` holds the name `\newcommand` defines).
        let mut sites: crate::fixed::HashMap<Sym, Vec<Span>> = crate::fixed::HashMap::default();
        for def in &analysis.facts.defs {
            sites.entry(def.name).or_default().push(def.span);
        }
        let plumbing = |i: usize, j: usize| {
            let (holder, name) = (graph.nodes[i], graph.nodes[j]);
            if !reportable.contains(&name) {
                return false;
            }
            let text = analysis.env.meaning(holder).as_macro().map(|m| m.replacement_text.clone());
            let defined = |span: &Span| sites.get(&name).is_some_and(|at| at.contains(span));
            let mut written = text.iter().flat_map(|t| t.iter()).filter(|t| t.cs() == Some(name)).peekable();
            written.peek().is_some() && written.all(|t| defined(&t.span))
        };
        // A name a text builds is used as one outside is.
        let outside = |name: &Sym| !reportable.contains(name) || built.is_match(analysis.interner.name(*name));
        let mut reached = crate::fixed::HashSet::default();
        let mut work: Vec<usize> = (0..graph.nodes.len()).filter(|&i| outside(&graph.nodes[i])).collect();
        // What a name outside copies is used as that name is (`\cs_new_eq:NN \mod_public:n
        // \__mod_private:n`): the copy reads the meaning, and nothing reads the copy in this run.
        use crate::analysis::graph::{EdgeKind, VertexTag};
        for def in analysis.facts.defs.iter().filter(|def| outside(&def.name)) {
            let copied =
                analysis.graph.outgoing(def.node).iter().filter(|(_, kind)| kind.intersects(EdgeKind::DEFINED_BY));
            let sources = copied.filter_map(|&(to, _)| analysis.graph.vertex(to)).filter(|v| v.tag == VertexTag::Use);
            for source in sources.map(|v| v.name).filter(|source| *source != def.name) {
                reached.insert(source);
                work.extend(graph.get(source));
            }
        }
        while let Some(i) = work.pop() {
            let named = graph.edges[i].iter().copied().filter(|&j| !plumbing(i, j));
            work.extend(named.filter(|&j| reached.insert(graph.nodes[j])));
        }
        NamedByText { reached, built }
    }

    fn has(&self, analysis: &Analysis, name: Sym) -> bool {
        self.reached.contains(&name) || self.built.is_match(analysis.interner.name(name))
    }
}

/// The names `text` builds, each as a regex on the whole name: between `\csname` and
/// `\endcsname`, and in an expl3 argument of type `c` (interface3, "The naming scheme").
/// A character stands for itself, a parameter or a command for any text. A name with no
/// character of its own says nothing and is left out.
fn built_names(analysis: &Analysis, text: &[Token]) -> Vec<String> {
    let prim = |t: &Token| t.cs().and_then(|cs| analysis.env.meaning(cs).prim());
    let pattern = |name: &[Token]| {
        let mut out = String::from("^");
        let mut literal = false;
        for token in name {
            match token.tok {
                Tok::Chr(c, _) => {
                    literal = true;
                    out.push_str(&regex::escape(c.encode_utf8(&mut [0; 4])));
                }
                _ if out.ends_with(".*") => {}
                _ => out.push_str(".*"),
            }
        }
        out.push('$');
        literal.then_some(out)
    };
    let mut out = Vec::new();
    for (i, token) in text.iter().enumerate() {
        let rest = &text[i + 1..];
        if prim(token) == Some(Primitive::Csname) {
            let end = rest.iter().position(|t| prim(t) == Some(Primitive::Endcsname)).unwrap_or(rest.len());
            out.extend(pattern(&rest[..end]));
            continue;
        }
        let Some((_, signature)) = token.cs().and_then(|cs| analysis.interner.name(cs).split_once(':')) else {
            continue;
        };
        let mut at = 0;
        // `p` and `w` take text no signature describes: what follows them is not counted.
        for kind in signature.chars().take_while(|k| !matches!(k, 'p' | 'w')) {
            let Some(first) = rest.get(at) else { break };
            let argument = match first.tok {
                Tok::Chr(_, Catcode::Begin) => {
                    let mut depth = 0usize;
                    let close = rest[at..].iter().position(|t| {
                        match t.tok {
                            Tok::Chr(_, Catcode::Begin) => depth += 1,
                            Tok::Chr(_, Catcode::End) => depth -= 1,
                            _ => {}
                        }
                        depth == 0
                    });
                    let Some(close) = close else { break };
                    at += close + 1;
                    &rest[at - close..at - 1]
                }
                _ => {
                    at += 1;
                    &rest[at - 1..at]
                }
            };
            if kind == 'c' {
                out.extend(pattern(argument));
            }
        }
    }
    out
}

fn unused_definition(report: &mut Report) {
    let analysis = report.analysis();
    let index = report.index();
    let rule = &analysis.settings.lints.unused_definition;
    let ignore: Vec<regex::Regex> = rule.ignore.iter().filter_map(|p| regex::Regex::new(p).ok()).collect();
    let private: Vec<regex::Regex> = rule.private.iter().filter_map(|p| regex::Regex::new(p).ok()).collect();
    let read: BTreeSet<Sym> =
        analysis.facts.defs.iter().filter(|def| index.used_definition(def)).map(|def| def.name).collect();
    // What the rule may report: own macros no run read, a library's private ones only.
    let mut candidates = Vec::new();
    for def in &analysis.facts.defs {
        // A name one call defines for another it defines, whose text calls
        // it (ltcmd's `\fd code`), is that name's implementation.
        if def.tag != "macro"
            || read.contains(&def.name)
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
        // The empty name (`\csname\endcsname`, or a name the run could not
        // decide) is always built.
        let written = !name.trim_start_matches('\\').is_empty()
            && report.edits().text(def.span.file).is_none_or(|(_, text)| {
                let needle = if name.starts_with('\\') { name.clone() } else { format!("\\{name}") };
                text.line(def.span.line).contains(&needle)
            });
        if !written {
            continue;
        }
        if (!rule.pgf_keys && is_pgf_key(&name))
            || (!rule.report_public && !looks_private(&private, &name))
            || ignore.iter().any(|re| re.is_match(&name))
        {
            continue;
        }
        candidates.push((def, name));
    }
    // Every other name is there to be used, by the document, the kernel's hooks or a library's
    // users: what its text names is used too, whether or not this run expanded it.
    let reportable: crate::fixed::HashSet<Sym> = candidates.iter().map(|(def, _)| def.name).collect();
    let own = || analysis.facts.defs.iter().filter(|def| index.own_definition(def));
    let named = NamedByText::of(analysis, &reportable, own());
    // a file that sets no text and only defines (`stats.tex`, the numbers a script writes for the text
    // to pick from) gives one finding: its unused names are its data, not a slip each
    let data_files = crate::linter::shared::definition_files(analysis, &report.edits());
    let mut data: BTreeMap<crate::engine::tex::FileId, Vec<(&crate::analysis::facts::Definition, String)>> =
        BTreeMap::new();
    for (def, name) in candidates {
        if named.has(analysis, def.name) {
            continue;
        }
        if data_files.contains(&def.span.file) {
            data.entry(def.span.file).or_default().push((def, name));
            continue;
        }
        if report.unread("used").is_some() {
            report.add_absent("used", def.span, &name, format!("{name} is never used"), None);
            continue;
        }
        let edits = report.edits().delete_definition(def);
        report.add_fix(
            def.span,
            &name,
            format!("{name} is never used"),
            Fix::new(format!("delete `{name}`"), Applicability::Unsafe, edits),
        );
    }
    for (file, unused) in data {
        let report_one = |report: &mut Report, def: &crate::analysis::facts::Definition, name: &str| {
            let edits = report.edits().delete_definition(def);
            let fix = Fix::new(format!("delete `{name}`"), Applicability::Unsafe, edits);
            report.add_fix(def.span, name, format!("{name} is never used"), fix);
        };
        if unused.len() <= 3 {
            unused.iter().for_each(|(def, name)| report_one(report, def, name));
            continue;
        }
        let shown: Vec<&str> = unused.iter().take(5).map(|(_, n)| n.as_str()).collect();
        let message = format!(
            "{} names {} defines are never used: {}{}",
            unused.len(),
            analysis.file_name(file).rsplit('/').next().unwrap_or_default(),
            shown.join(", "),
            if unused.len() > shown.len() { ", …" } else { "" }
        );
        report.add(unused[0].0.span, "file", message, None);
    }
}

fn dead_definition(report: &mut Report) {
    let analysis = report.analysis();
    let index = report.index();
    let mut written: Vec<&crate::analysis::facts::Definition> = analysis
        .facts
        .defs
        .iter()
        // A name package code builds per call (`\\cite`'s keys, a sankey
        // node's flags) is no definition the document wrote.
        .filter(|def| def.certain && def.stated && def.spelled && def.mac.is_some() && index.own_definition(def))
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
            // A `\providecommand`-family definition is a fallback that yields on purpose.
            let by = analysis.interner.name(first.by).to_ascii_lowercase();
            let fallback = by.starts_with("provide");
            // A name declared empty and then defined: `\newcommand\x{}` checks that the name is
            // free before a `\def\x#1[#2]{…}`, and expl3 wants `\tl_new:N` before `\tl_set:Nn`.
            let declared = by.contains("new") && first.mac.as_ref().is_some_and(|m| m.replacement_text.is_empty());
            if first.span == second.span
                || first.depth != second.depth
                || fallback
                || declared
                || index.used_definition(first)
            {
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
                Fix::new(format!("delete this definition of `{rendered}`"), applicability, edits),
            );
        }
    }
}
