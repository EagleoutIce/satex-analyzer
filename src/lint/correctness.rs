use crate::builtins::OccKind;
use crate::facts::{LoadStatus, MeaningKind};
use crate::lint::fix::{Applicability, Fix};
use crate::lint::shared::own_runaway;
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::plugin::Provider;
use crate::tex::{Span, Sym};

pub static RULES: &[Rule] = &[
    Rule {
        code: "undefined-glossary-entry",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "a glossary or acronym entry is used but never declared",
        explanation: "\
`\\gls` and its relatives name an entry that no `\\newglossaryentry`,
`\\newacronym`, `\\acro` or `\\DeclareAcronym` declares.  A real run stops
with `Glossary entry ... has not been defined`.

Fix by declaring the entry, or by correcting the key.",
        references: &[Ref { source: Source::Glossaries, at: "Defining glossary entries" }],
        fixes: &[],
        run: undefined_glossary_entry,
    },
    Rule {
        code: "shell-escape",
        category: Category::Security,
        severity: Severity::Warning,
        summary: "the document runs a program through `\\write18`",
        explanation: "\
`\\write18` hands its text to the shell.  A run only does that when the engine
was started with `--shell-escape`, or with `--shell-restricted` and the
program is one TeX Live allows.  `shell_escape` in `satex.yaml` says which of
the three this build is.

Fix by running with the flag the document needs, or by removing the call.",
        references: &[Ref { source: Source::Pdftex, at: "-shell-escape" }],
        fixes: &[],
        run: shell_escape,
    },
    Rule {
        code: "missing-graphic",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "an included image is not there for this output target",
        explanation: "\
`\\includegraphics` names a file that graphicx cannot find, with any extension
in `\\Gin@extensions`, beside the document or on `\\graphicspath`.  Which
extensions count is the driver's (pdftex.def, dvips.def, xetex.def), so it
follows the output.

Fix by adding the file, or by producing the format the driver reads.",
        references: &[Ref { source: Source::Graphics, at: "pdftex.def" }],
        fixes: &[],
        run: missing_graphic,
    },
    Rule {
        code: "expl3-signature-mismatch",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "an expl3 function takes other arguments than its name says",
        explanation: "\
An expl3 name carries its signature after the colon
([interface3](https://ctan.org/pkg/expl3), \"Naming conventions\"): one letter
per argument, `N` a single token, `n` a braced group.

What the function really consumes — the calls the run saw, or probe input
for one the document defines — takes a different number of arguments, or an
optional one or a star, which no expl3 signature has.  `\\foo:nn` that takes
one argument leaves the second for whatever follows.

Fix by renaming the function to the signature it has, or its parameter
text.",
        references: &[Ref { source: Source::Interface3, at: "Naming conventions" }],
        fixes: &[],
        run: expl3_signature_mismatch,
    },
    Rule {
        code: "unguarded-recursion",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "a recursive macro has nothing that can stop it",
        explanation: "\
The run saw the macro's expansion reach the macro again, could not see the
recursion end, and found no conditional — no `\\if…`, `\\else` or `\\fi` — to
end it.  A macro nothing expands is judged by the names its replacement text
holds.  A real run expands until TeX runs out of memory:
`TeX capacity exceeded, sorry [main memory size]`.

Fix by adding the test that stops it, or by removing the self-reference.",
        references: &[],
        fixes: &[],
        run: unguarded_recursion,
    },
    Rule {
        code: "undefined-control-sequence",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "a control sequence is used that nothing defines",
        explanation: "\
The name has no meaning at the point it is used, so a real run would stop with
`Undefined control sequence`.  When the message says \"not on every path\", the
definition sits in a conditional arm that this use does not depend on.

Fix by loading the package that provides it, defining it before the use, or
correcting the spelling.",
        references: &[Ref { source: Source::TeXbook, at: "chapter 27" }],
        fixes: &[],
        run: undefined_control_sequences,
    },
    Rule {
        code: "raised-error",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "the document's code raises a TeX error",
        explanation: "\
Running the document reaches `\\errmessage`, which every LaTeX error
(`\\PackageError`, `\\msg_error:nn`, the kernel's own) ends in: a real run
stops there.  A more specific rule that explains the same error reports it
instead.  An error reached only on a path satex could not decide, or after it
lost track of the mode, is not one a real run need reach, and is not reported.
A runaway in package code over an argument the document leaves open is the
document's error too: it is shown at the call that opens it.",
        references: &[],
        fixes: &[],
        run: raised_errors,
    },
    Rule {
        code: "caused-library-error",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "the document makes package code raise a TeX error",
        explanation: "\
An engine error such as `Argument has an extra }` arises inside package code.
The cause is the document's own call into that code.  The finding sits at
that call and names the package line where the error arose.  A runaway
over the document's own unclosed argument is not one: `raised-error` reports
it.  Each call and
error code is one finding, with a count when it repeats.

When satex had to approximate on the way, the error may not be TeX's: it is
a precision note, not a finding.  An exact error after such a one may be its
echo, as TeX's recovery left a state that may not be TeX's.  The finding is
then a warning that names the earlier error.",
        references: &[Ref { source: Source::TeXbook, at: "chapter 27" }],
        fixes: &[],
        run: caused_library_errors,
    },
    Rule {
        code: "undefined-environment",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "`\\begin` names an environment that is not defined",
        explanation: "\
`\\begin{name}` expands `\\name`, and nothing defines it.  A real run reports
`Environment name undefined`.

Fix by loading the package that provides the environment or by defining it with
`\\newenvironment{name}{…}{…}`.",
        references: &[Ref { source: Source::Source2e, at: "ltmiscen.dtx" }],
        fixes: &[],
        run: undefined_environments,
    },
    Rule {
        code: "unresolved-file",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "a package, class or input file is not in the installation",
        explanation: "\
The file was searched for in the document's directory, the configured search
paths and the TEXMF trees, and not found. Case mismatches on case-insensitive
filesystems are also reported: they work on Windows or macOS but break on Linux.
`satex query distribution` shows which installation was searched.

Fix by installing the package, adding its directory to `search_paths` in
`satex.yaml`, pointing `texmf.roots` at the right installation, or
(for case mismatches) renaming the file to match the requested name.",
        references: &[Ref { source: Source::Chktex, at: "27" }],
        fixes: &[],
        run: unresolved_files,
    },
    Rule {
        code: "option-clash",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "a package is loaded twice with different options",
        explanation: "\
LaTeX loads a package once.  A second `\\usepackage` with options the first one
did not have raises `Option clash for package`.

Fix by giving all the options to the first load, or by using
`\\PassOptionsToPackage` before it.",
        references: &[Ref { source: Source::Source2e, at: "ltclass.dtx" }],
        fixes: &[Applicability::Unsafe],
        run: option_clash,
    },
    Rule {
        code: "already-defined",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "`\\newcommand` redefines an existing name",
        explanation: "\
`\\newcommand` refuses to redefine and raises `Command … already defined`.

Fix with `\\renewcommand` if the redefinition is intended, or pick another name.",
        references: &[Ref { source: Source::Source2e, at: "ltdefns.dtx" }],
        fixes: &[Applicability::Unsafe],
        run: already_defined,
    },
    Rule {
        code: "not-defined",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "`\\renewcommand` targets a name that does not exist",
        explanation: "\
`\\renewcommand` requires an existing definition and raises
`Undefined control sequence`.

Fix with `\\newcommand`, or load the package that was supposed to define it.",
        references: &[Ref { source: Source::Source2e, at: "ltdefns.dtx" }],
        fixes: &[Applicability::Safe],
        run: not_defined,
    },
    Rule {
        code: "undefined-reference",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "`\\ref` names a label that is never defined",
        explanation: "\
No `\\label` in the document or its inputs declares this key, so the reference
prints `??`.  When the run left a project file unread, the label may stand
there: the finding is then a precision note.

Fix by adding the `\\label`, or by correcting the key.",
        references: &[Ref { source: Source::Source2e, at: "ltxref.dtx" }],
        fixes: &[],
        run: undefined_reference,
    },
    Rule {
        code: "duplicate-label",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "the same label is defined twice",
        explanation: "\
LaTeX warns `Label … multiply defined` and every reference resolves to the
later one.  The finding stands at the innermost command of the document's
text that writes the label: the `\\begin` inside a `\\sbox` argument, not the
`\\sbox`. Only the kernel's `\\newlabel` (and zref's) warn: a name another
package writes to the `.aux` file twice, such as a tikzmark, is not checked.

Fix by renaming one of them.",
        references: &[Ref { source: Source::Source2e, at: "ltxref.dtx" }],
        fixes: &[],
        run: duplicate_label,
    },
    Rule {
        code: "catcode-escapes-group",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "a category code change is still in force at the end of the run",
        explanation: "\
`\\catcode` is local, so a change made inside a group is undone by the closing
brace.  One still in force at the end of the document was made at the outer
level, and everything read after it is tokenized differently.

Fix by wrapping the change in `{…}` or `\\begingroup…\\endgroup`, or by restoring
the old value explicitly.  `satex query catcodes` shows the current table.",
        references: &[Ref { source: Source::TeXbook, at: "chapter 5" }],
        fixes: &[],
        run: catcode_escapes_group,
    },
    Rule {
        code: "environment-mismatch",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "`\\end` names an environment other than the one that is open",
        explanation: "\
Environments nest like brackets: the name `\\end` gives has to match the one
`\\begin` most recently opened.  chktex tracks this as warnings 9 (`'%s'
expected, found '%s'`), 10 (`Solo '%s' found`) and 15 (`No match found for
'%s'`) for brackets and environments alike.  satex only records `\\begin`/`\\end`
occurrences, so this rule covers environments and not generic `{}`, `[]` or
`()` nesting.

An error raised inside the environment before the `\\end` may have ended it
early, through TeX's recovery or the analysis's approximation of its input.
The finding is then a note that names that error, as is one the analysis
reached on only some of its paths.

Fix by naming the environment that is actually open, or by closing the inner
environments first.",
        references: &[
            Ref { source: Source::Chktex, at: "9" },
            Ref { source: Source::Chktex, at: "10" },
            Ref { source: Source::Chktex, at: "15" },
            Ref { source: Source::Source2e, at: "ltmiscen.dtx" },
        ],
        fixes: &[Applicability::Unsafe],
        run: environment_mismatch,
    },
    Rule {
        code: "expl-syntax-left-on",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "`\\ExplSyntaxOn` is never turned off",
        explanation: "\
With expl3 syntax on, spaces are ignored and `_` and `:` are letters, so
ordinary text stops working.

Fix by adding `\\ExplSyntaxOff` after the code that needs it.",
        references: &[Ref { source: Source::Source2e, at: "ltfiles.dtx" }],
        fixes: &[],
        run: expl_syntax_left_on,
    },
];

fn unguarded_recursion(report: &mut Report) {
    use crate::builtins::Primitive;
    use crate::tex::Tok;

    let analysis = report.analysis();
    let recursive: std::collections::BTreeMap<Sym, bool> =
        report.index().recursion().iter().map(|r| (r.name, r.observed)).collect();
    let sites: crate::fixed::HashSet<crate::tex::Span> =
        analysis.facts.diagnostics.iter().filter(|d| d.code == "recursion-widened").map(|d| d.span).collect();
    let widened: crate::fixed::HashSet<Sym> =
        analysis.facts.expansions.iter().filter(|e| sites.contains(&e.span)).map(|e| e.name).collect();
    for def in &analysis.facts.defs {
        let Some(&observed) = recursive.get(&def.name) else { continue };
        // A recursion the run saw end had something to stop it.
        if observed && !widened.contains(&def.name) {
            continue;
        }
        let Some(mac) = &def.mac else { continue };
        // `\newcommand\foo[1][default]{...}` (and `\DeclareRobustCommand`,
        // `\providecommand`, `\newenvironment`) does not define `\foo` as
        // its body: ltdefns' `\@xargdef` defines it as a dispatcher that
        // reads the optional argument and hands off to `\foo`'s internal
        // counterpart, `\csname\string\foo\endcsname`:
        //   \def\foo{\@protected@testopt \foo \\foo {default}}
        // `\foo` genuinely names itself there, but only as data handed to
        // `\@protected@testopt` (by way of `\expandafter`, already spent by
        // the time this body exists) — never expanded, by construction, for
        // every optional-argument command that exists.  Only a self-name
        // outside that one pairing is a real call to guard against.
        if !observed && only_optarg_wrapper_self_reference(analysis, def.name, mac) {
            continue;
        }
        let guarded = mac.replacement_text.iter().any(|token| match token.tok {
            Tok::Cs(sym) => matches!(
                analysis.env.meaning(sym).prim(),
                Some(Primitive::If(_) | Primitive::Else | Primitive::Fi | Primitive::Unless)
            ),
            _ => false,
        });
        if guarded {
            continue;
        }
        let name = report.cs(def.name);
        // A macro nothing expands cannot loop: the definition is still worth
        // reporting, but it is not what breaks a run.
        let (severity, message) = if observed {
            (Severity::Error, format!("{name} reaches itself with no conditional to stop it"))
        } else {
            (
                Severity::Warning,
                format!(
                    "{name} names itself in its replacement text with no conditional to stop it. \
                     Nothing expands it, so this is read off the text"
                ),
            )
        };
        report.add_as(severity, def.span, &name, message, Some("add the test that ends the recursion".into()));
    }
}

/// Whether every occurrence of `name` in `mac`'s body is the optional-argument
/// dispatch pairing ltdefns and ltcmd both generate: the plain name right
/// after the kernel's own optional-argument dispatcher, immediately followed
/// by the name's internal counterpart (`\csname\string\name\endcsname`,
/// which carries a literal `\` as the first character of its own name).
/// A `false` here means some other occurrence is a real call.
fn only_optarg_wrapper_self_reference(
    analysis: &crate::machine::Analysis,
    name: Sym,
    mac: &crate::tex::MacroDef,
) -> bool {
    use crate::tex::Tok;
    // `\@protected@testopt` (ltdefns, current kernel) and its predecessor
    // `\@testopt`, which a format without the merged definition still uses.
    fn is_dispatcher(n: &str) -> bool {
        matches!(n, "@protected@testopt" | "@testopt")
    }
    let own = analysis.interner.name(name);
    let is_internal_counterpart = |n: &str| n.len() == own.len() + 1 && n.as_bytes()[0] == b'\\' && &n[1..] == own;
    let text = &mac.replacement_text;
    let mut any = false;
    for (i, token) in text.iter().enumerate() {
        let Tok::Cs(sym) = token.tok else { continue };
        if sym != name {
            continue;
        }
        let dispatched =
            i > 0 && matches!(text[i - 1].tok, Tok::Cs(prev) if is_dispatcher(analysis.interner.name(prev)));
        let paired = text.get(i + 1).is_some_and(
            |next| matches!(next.tok, Tok::Cs(sym) if is_internal_counterpart(analysis.interner.name(sym))),
        );
        if !(dispatched && paired) {
            return false;
        }
        any = true;
    }
    any
}

fn undefined_control_sequences(report: &mut Report) {
    // Under ConTeXt satex knows the primitives and nothing else, so every
    // ConTeXt command would look undefined, so it reports none at all.
    if report.analysis.plugins.kernel == crate::plugin::Kernel::Context {
        return;
    }
    // Where a cut made the analysis assume category codes back
    // (`region-catcodes`), a name holding a character that is no letter
    // under any category code a document keeps (`{`, `\\`, a space, ...)
    // was read under codes the analysis only assumed: what TeX reads there
    // is not known.
    let assumed = report.analysis.facts.diagnostics.iter().any(|d| d.code == "region-catcodes");
    for expansion in &report.analysis.facts.expansions {
        if !matches!(expansion.meaning, MeaningKind::Undefined | MeaningKind::UndefinedApproximated) {
            continue;
        }
        let name = report.cs(expansion.name);
        let raw = report.analysis.interner.name(expansion.name);
        let odd = raw.chars().count() > 1 && raw.chars().any(|c| "\\{}$&#% \t\r".contains(c));
        let (severity, message) = if assumed && odd {
            (
                Severity::Warning,
                format!("{name} is undefined where the analysis assumed category codes back after a cut"),
            )
        } else if expansion.meaning == MeaningKind::UndefinedApproximated {
            (Severity::Info, format!("{name} is undefined on a path the analysis approximated"))
        } else if expansion.cds.is_empty() {
            (Severity::Error, format!("{name} is never defined in this run"))
        } else {
            (Severity::Warning, format!("{name} is undefined on a path satex could not decide"))
        };
        report.add_as(
            severity,
            expansion.span,
            &name,
            message,
            Some(format!("define `{name}` or load the package that provides it")),
        );
    }
}

/// Every `\errmessage` a real run reaches, once per place and text, placed
/// at the request that read the file raising it.
fn raised_errors(report: &mut Report) {
    let analysis = report.analysis();
    let view = report.text();
    let mut seen = crate::fixed::HashSet::default();
    // A runaway in library code over the document's own unclosed text, in
    // place of TeX's message about the library macro.
    let own: Vec<_> =
        analysis.facts.diagnostics.iter().filter(|d| exact(d)).filter_map(|d| own_runaway(analysis, d)).collect();
    for (i, o) in analysis.facts.occurrences.iter().enumerate() {
        if !crate::lint::shared::is_error(analysis, o) || !o.certain || !seen.insert((o.span, o.key.as_str())) {
            continue;
        }
        if own.iter().any(|(at, _)| at.file == o.span.file && at.line == o.span.line) {
            continue;
        }
        let mut span = o.span;
        let mut via = None;
        for _ in 0..64 {
            if view.document(span.file) {
                break;
            }
            let Some(load) = analysis.facts.loads.iter().find(|l| l.file == Some(span.file)) else { break };
            via = Some(load.name.clone());
            span = load.span;
        }
        // The headline: what follows the first empty line is help.
        let headline = o.key.split("\n\n").next().unwrap_or_default();
        let text = headline.split_whitespace().collect::<Vec<_>>().join(" ");
        let message = match via {
            Some(name) => format!("reading {name}: {text}"),
            None => text,
        };
        match crate::lint::shared::doubt(analysis, i) {
            Some(cause) => report.add_doubted(span, "", message, &cause),
            None => report.add(span, "", message, None),
        }
    }
    for (span, message) in own {
        report.add(span, "", message, None);
    }
}

/// An engine error raised on a path the analysis followed exactly.
fn exact(d: &crate::facts::Diagnostic) -> bool {
    d.severity == crate::facts::Severity::Warning
}

/// Engine errors in library files, shown at the user's call that led there.
fn caused_library_errors(report: &mut Report) {
    let analysis = report.analysis();
    let view = report.text();
    // The error, the call it is shown at, its count, and the first error on
    // an approximated path that the run raised before it, if any.
    type Row<'a> = (&'a crate::facts::Diagnostic, crate::tex::Span, u32, Option<crate::tex::Span>);
    let mut rows: Vec<Row> = Vec::new();
    // TeX's recovery from an error that may not be TeX's leaves a state
    // that may not be TeX's either: every error after it may be its echo.
    let mut doubt = None;
    for d in &analysis.facts.diagnostics {
        let before = doubt;
        if d.severity == crate::facts::Severity::Unsupported {
            doubt = doubt.or(Some(d.origin.unwrap_or(d.span)));
        }
        if !crate::machine::TEX_ERRORS.iter().any(|(c, _)| *c == d.code) {
            continue;
        }
        let Some(origin) = d.origin else { continue };
        let own = exact(d) && own_runaway(analysis, d).is_some();
        if own || view.document(d.span.file) || !view.document(origin.file) {
            continue;
        }
        let same = |(e, o, _, _): &&mut Row| e.code == d.code && o.file == origin.file && o.line == origin.line;
        match rows.iter_mut().find(same) {
            Some(row) => row.2 += d.count,
            None => rows.push((d, origin, d.count, before)),
        }
    }
    for (d, origin, count, before) in rows {
        let times = if count > 1 { format!(" ({count} times)") } else { String::new() };
        let at = format!("{}:{}", analysis.short_name(d.span.file), d.span.line);
        let mut message = format!("error in {at}: {}, caused here{times}", d.message);
        // An error on an approximated path, or after one, may not be TeX's.
        let exact = exact(d);
        match before {
            _ if !exact => message.push_str(" (the analysis approximated on the way: may not be TeX's)"),
            Some(first) => message
                .push_str(&format!(" (may follow from the analysis gap at {}: check that first)", report.place(first))),
            None => {}
        }
        // An error only an approximated path raised is the analysis's, not
        // a defect to fix: a precision note.
        match before {
            _ if !exact => report.add_imprecision(origin, d.code, message),
            None => report.add_as(Severity::Error, origin, d.code, message, None),
            Some(_) => report.add_as(Severity::Warning, origin, d.code, message, None),
        }
    }
}

/// The kernel's own error (latex.ltx `\begin`): `LaTeX Error: Environment
/// ⟨name⟩ undefined.`
fn undefined_environments(report: &mut Report) {
    let found = kernel_errors(report, |text| {
        let rest = &text[text.find("LaTeX Error: Environment ")? + "LaTeX Error: Environment ".len()..];
        Some(rest[..rest.find(" undefined")?].to_string())
    });
    for (span, name, doubt) in found {
        let message = format!("LaTeX Error: Environment {name} undefined");
        if let Some(cause) = doubt {
            report.add_doubted(span, &name, message, &cause);
            continue;
        }
        report.add(span, &name, message, Some(format!("`\\newenvironment{{{name}}}{{…}}{{…}}`")));
    }
}

fn unresolved_files(report: &mut Report) {
    let distribution = report.analysis.distribution.describe();
    let empty = report.analysis.distribution.indexed == 0;
    let analysis = report.analysis();
    for load in &analysis.facts.loads {
        if load.name.is_empty() || crate::plugin::packages::installable_load(analysis, load) {
            continue;
        }
        let message = match load.status {
            LoadStatus::NotFound if empty => format!("no TeX installation to search in: {distribution}"),
            LoadStatus::NotFound => {
                format!("{} `{}` is not in this installation ({distribution})", load.kind.as_str(), load.name)
            }
            LoadStatus::Unreadable => format!("`{}` was found but could not be read", load.name),
            LoadStatus::CaseMismatch { on_disk } => {
                let on_disk_name = report.analysis.interner.name(on_disk);
                format!(
                    "exists as `{on_disk_name}` but was asked for as `{}`: works here, breaks on case-sensitive systems",
                    load.name
                )
            }
            _ => continue,
        };
        let fix = if empty {
            Some("set texmf.roots or texlive.root in satex.yaml".to_string())
        } else if let LoadStatus::CaseMismatch { on_disk } = load.status {
            let on_disk_name = report.analysis.interner.name(on_disk);
            Some(format!("rename `{on_disk_name}` to match the requested name `{}`", load.name))
        } else {
            Some(format!("install `{}`, or add its directory to search_paths", load.name))
        };
        report.add(load.span, &load.name, message, fix);
    }
    // The kernel's own error when `\IfFileExists` finds nothing (latex.ltx
    // `\@missingfileerror`): `LaTeX Error: File `⟨name⟩' not found.`
    let missing = kernel_errors(report, |text| {
        let rest = &text[text.find("LaTeX Error: File `")? + "LaTeX Error: File `".len()..];
        Some(rest[..rest.find("' not found")?].to_string())
    });
    for (span, name, _) in missing {
        if crate::plugin::packages::installable(analysis, &name).is_some() {
            continue;
        }
        let message = format!("`{name}` is not in this installation ({distribution})");
        report.add(span, &name, message, Some(format!("install `{name}`, or add its directory to search_paths")));
    }
}

fn option_clash(report: &mut Report) {
    for (load, options, at, first_load) in report.index().reloads() {
        let extra: Vec<&str> = load.options.iter().filter(|o| !options.contains(o)).map(String::as_str).collect();
        if extra.is_empty() {
            continue;
        }
        let edits = first_load.and_then(|first| report.edits().add_options(first, &extra));
        report.add_fix(
            load.span,
            &load.name,
            format!("{} was first loaded at {} without {}", load.name, report.place(at), extra.join(", ")),
            Fix::new(
                format!("move the options to the first `\\usepackage{{{}}}`", load.name),
                Applicability::Unsafe,
                edits,
            ),
        );
    }
}

/// The kernel's `\@notdefinable`: `Command \⟨name⟩ already defined`.
fn already_defined(report: &mut Report) {
    let refused = kernel_errors(report, |text| {
        let rest = &text[text.find("Command \\")? + "Command ".len()..];
        Some(rest[..rest.find(" already defined")?].to_string())
    });
    // A name built from unknown text says nothing about the document.
    for (span, name, doubt) in refused.into_iter().filter(|(_, name, _)| !name.contains('\u{2}')) {
        if let Some(cause) = doubt {
            report.add_doubted(span, &name, format!("{name} is already defined"), &cause);
            continue;
        }
        let edits = renamed(report, span, "new", "renew");
        report.add_fix(
            span,
            &name,
            format!("{name} is already defined"),
            Fix::new(format!("use `\\renewcommand{{{name}}}` instead"), Applicability::Unsafe, edits),
        );
    }
    let analysis = report.analysis();
    let refused: Vec<(Span, String, Option<String>)> = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.code == "already-defined")
        .map(|d| (d.span, d.message.clone(), crate::lint::shared::doubt(analysis, d.after)))
        .collect();
    for (span, message, doubt) in refused {
        let name = message.split('`').nth(1).unwrap_or_default().to_string();
        if let Some(cause) = doubt {
            report.add_doubted(span, &name, message, &cause);
            continue;
        }
        let edits = renamed(report, span, "new", "renew");
        report.add_fix(
            span,
            &name,
            message,
            Fix::new(format!("use `\\renewcommand{{{name}}}` instead"), Applicability::Unsafe, edits),
        );
    }
}

/// The defining command at `span` with its `from` prefix turned into `to`.
fn renamed(report: &Report, span: Span, from: &str, to: &str) -> Option<Vec<crate::lint::fix::Edit>> {
    let edits = report.edits();
    edits.rename_command(span, |name| edits.counterpart(name, from, to))
}

/// The kernel's `\renew@command` of an unknown name: `Command \⟨name⟩
/// undefined`.  It defines the name all the same (ltdefns).
fn not_defined(report: &mut Report) {
    let missing = kernel_errors(report, |text| {
        let rest = &text[text.find("Command \\")? + "Command ".len()..];
        Some(rest[..rest.find(" undefined")?].to_string())
    });
    for (span, name, doubt) in missing {
        if let Some(cause) = doubt {
            report.add_doubted(span, &name, format!("{name} is not defined here"), &cause);
            continue;
        }
        let edits = renamed(report, span, "renew", "new");
        report.add_fix(
            span,
            &name,
            format!("{name} is not defined here"),
            Fix::new(format!("use `\\newcommand{{{name}}}` instead"), Applicability::Safe, edits),
        );
    }
}

/// The messages that match `parse`, once per place.
/// With each, the place the run could not follow before it, if any.
fn kernel_errors(report: &Report, parse: impl Fn(&str) -> Option<String>) -> Vec<(Span, String, Option<String>)> {
    let analysis = report.analysis();
    let mut seen = crate::fixed::HashSet::default();
    analysis
        .facts
        .occurrences
        .iter()
        .enumerate()
        .filter(|(_, o)| o.kind == OccKind::Message)
        .filter_map(|(i, o)| Some((i, o.span, parse(&o.key)?)))
        .filter(|(_, span, name)| seen.insert((*span, name.clone())))
        .map(|(i, span, name)| (span, name, crate::lint::shared::doubt(analysis, i)))
        .collect()
}

fn undefined_reference(report: &mut Report) {
    let known = report.index().labels();
    let missing: Vec<(Span, Option<String>, String)> = report
        .analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Ref && !known.contains_key(o.key.as_str()))
        .map(|o| {
            let (span, via) = report.site(o);
            (span, via, o.key.clone())
        })
        .collect();
    for (span, via, key) in missing {
        let message = format!("no `\\label{{{key}}}` anywhere in the document{}", via.unwrap_or_default());
        report.add_absent("defined", span, &key, message, Some(format!("add `\\label{{{key}}}` where it belongs")));
    }
}

fn duplicate_label(report: &mut Report) {
    let first = report.index().labels();
    let repeats: Vec<(Span, String, Span)> = report
        .analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Label)
        // `\newlabel` warns about a name it defined before (ltxref.dtx
        // `\@newl@bel`), as zref's does; other `.aux` names do not.
        .filter(|o| o.detail.as_deref().is_none_or(|head| matches!(head, "newlabel" | "zref@newlabel")))
        .filter_map(|o| {
            let at = report.site(first.get(o.key.as_str())?).0;
            let span = report.site(o).0;
            (at != span).then(|| (span, o.key.clone(), at))
        })
        .collect();
    // One finding per site: a call that writes several labels (a macro's
    // `\label{nb:#1}` and `\label{cell:#1-a}`) names each repeated one.
    let mut sites: Vec<(Span, Vec<(String, Span)>)> = Vec::new();
    for (span, key, at) in repeats {
        match sites.iter_mut().find(|s| s.0 == span) {
            Some(site) if !site.1.iter().any(|(k, _)| *k == key) => site.1.push((key, at)),
            Some(_) => {}
            None => sites.push((span, vec![(key, at)])),
        }
    }
    for (span, labels) in sites {
        let each: Vec<String> =
            labels.iter().map(|(key, at)| format!("`{key}` is already defined at {}", report.place(*at))).collect();
        report.add(
            span,
            &labels[0].0,
            format!("label {}", each.join(", and ")),
            Some("rename one of the two labels".into()),
        );
    }
}

fn catcode_escapes_group(report: &mut Report) {
    let base = crate::tex::CatcodeTable::latex();
    let changed = report.analysis.catcodes.diff(&base);
    let sites: Vec<(Span, String)> = report
        .analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Catcode)
        .map(|o| (o.span, o.key.clone()))
        .collect();
    for (character, code) in changed {
        if character == '@' {
            continue;
        }
        let at = sites.iter().find(|(_, key)| key.starts_with(character)).map(|(span, _)| *span).unwrap_or_default();
        report.add(
            at,
            &character.to_string(),
            format!("`{character}` still has category code {code} at the end of the run"),
            Some("restore it, or make the change inside {…}".to_string()),
        );
    }
}

fn expl_syntax_left_on(report: &mut Report) {
    if report.analysis.catcodes.get('_') != crate::tex::Catcode::Letter {
        return;
    }
    report.add(
        Span::default(),
        "\\ExplSyntaxOn",
        "expl3 syntax is still in force at the end of the run".into(),
        Some("add `\\ExplSyntaxOff`".into()),
    );
}

/// graphics.sty's `\Ginclude@graphics` raises `File `⟨name⟩' not found`
/// when no extension in `\Gin@extensions` names a file on `\Ginput@path`.
/// The lint reports what the real code said, once per call.
fn missing_graphic(report: &mut Report) {
    let mut seen = crate::fixed::HashSet::default();
    let missing: Vec<(Span, String)> = report
        .analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Message)
        // `\@missingfileerror`'s prompt is a file `\input` could not find,
        // which `unresolved-file` reports.
        .filter(|o| !o.key.contains("Default extension"))
        .filter_map(|o| {
            let rest = &o.key[o.key.find("File `")? + "File `".len()..];
            let name = &rest[..rest.find("' not found")?];
            Some((o.span, name.to_string()))
        })
        .filter(|found| seen.insert(found.clone()))
        .collect();
    for (span, name) in missing {
        // epstopdf writes `<base>-eps-converted-to.pdf` while compiling, from
        // `<base>.eps`, so the converted file is missing only until then.
        if let Some(base) = name.strip_suffix("-eps-converted-to.pdf") {
            let edits = report.edits();
            let dirs = [span.file, report.analysis.main_file].into_iter().filter_map(|file| {
                edits.text(file).and_then(|(path, _)| std::path::Path::new(&path).parent().map(|p| p.to_path_buf()))
            });
            if dirs.into_iter().any(|dir| {
                dir.join(format!("{base}.eps")).is_file() || std::path::Path::new(&format!("{base}.eps")).is_file()
            }) {
                continue;
            }
        }
        report.add(span, &name, format!("no file for `{name}` in any format the graphics driver reads"), None);
    }
}

/// The programs a restricted build runs: the installation's own
/// `shell_escape_commands` (texmf.cnf), as the provider's kpathsea program
/// reports it.  `None` when there is nothing to ask, which is not the same
/// answer as an empty list.
fn restricted_commands(provider: Option<Provider>) -> Option<&'static [String]> {
    // Only the two kpathsea names exist, so one cell each beats a keyed cache.
    static TEXLIVE: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    static MIKTEX: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let locator = Provider::kpsewhich_of(provider)?;
    let cell = if locator == Provider::MikTeX.locator() { &MIKTEX } else { &TEXLIVE };
    let commands = cell.get_or_init(|| shell_escape_commands(locator));
    (!commands.is_empty()).then_some(commands.as_slice())
}

fn shell_escape_commands(locator: &str) -> Vec<String> {
    std::process::Command::new(locator)
        .args(["-var-value", "shell_escape_commands"])
        .output()
        .ok()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .map(|text| text.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect())
        .unwrap_or_default()
}

fn shell_escape(report: &mut Report) {
    let analysis = report.analysis;
    let mode = analysis.plugins.shell_escape;
    let provider = analysis.plugins.provider;
    for occurrence in &analysis.facts.occurrences {
        if occurrence.kind != OccKind::ShellEscape {
            continue;
        }
        let program = occurrence.key.split_whitespace().next().unwrap_or_default().to_string();
        // `restricted_commands` runs kpsewhich: only ask once a finding turns on the answer.
        let (severity, message) = match mode {
            crate::config::ShellEscape::None => {
                (Severity::Error, format!("`{program}` cannot run: this build has no shell escape"))
            }
            crate::config::ShellEscape::Restricted => match restricted_commands(provider) {
                Some(allowed) if !allowed.contains(&program) => {
                    (Severity::Error, format!("`{program}` is not one of the programs a restricted build runs"))
                }
                Some(_) => (Severity::Info, format!("`{program}` runs through `\\write18`")),
                // Nothing to read texmf.cnf with, so the allowed list is
                // unknown: say so rather than call the program disallowed.
                None => (
                    Severity::Warning,
                    format!(
                        "`{program}` runs through `\\write18`, and satex found no kpathsea program \
                         to read shell_escape_commands from"
                    ),
                ),
            },
            _ => (Severity::Info, format!("`{program}` runs through `\\write18`")),
        };
        report.add_as(severity, occurrence.span, &program, message, None);
    }
}

/// `\begin{a}` closed by `\end{b}`: chktex warnings 9 (mismatch), 10 (solo
/// close) and 15 (solo open), restricted to environments.  Occurrences are
/// walked in source order and matched with a plain stack, the same algorithm
/// chktex itself uses for brackets.
/// latex.ltx's `\@checkend` raises `\begin{x} on input line n ended by
/// \end{y}`.  The rule reports the errors the run raised.
fn environment_mismatch(report: &mut Report) {
    let between = |text: &str, open: &str| -> Option<String> {
        let rest = &text[text.find(open)? + open.len()..];
        Some(rest[..rest.find('}')?].to_string())
    };
    let facts = &report.analysis.facts;
    let mismatches: Vec<(Span, String, String, String, bool, usize)> = facts
        .occurrences
        .iter()
        .enumerate()
        .filter(|(_, o)| o.kind == OccKind::Message && o.key.contains(" ended by "))
        .filter_map(|(i, o)| {
            let open = between(&o.key, "\\begin{")?;
            let found = between(&o.key, "\\end{")?;
            Some((o.span, open, found, o.key.clone(), o.certain, i))
        })
        .collect();
    // Every other error the run raised, where the document led to it.
    let errors: Vec<Span> = facts
        .diagnostics
        .iter()
        .filter(|d| crate::machine::TEX_ERRORS.iter().any(|(c, _)| *c == d.code))
        .filter_map(|d| d.origin)
        .chain(
            facts
                .occurrences
                .iter()
                .filter(|o| crate::lint::shared::is_error(report.analysis, o) && !o.key.contains(" ended by "))
                .map(|o| o.span),
        )
        .collect();
    // The error raised inside the environment before its `\end`: TeX's
    // recovery from it (or the analysis's over-approximation of the input
    // that caused it) can end the environment early, so the mismatch may
    // be its consequence.
    let cascade = |open: &str, end: Span| {
        let begin = facts
            .occurrences
            .iter()
            .filter(|o| o.kind == OccKind::BeginEnvironment && o.key == open && o.span.file == end.file && o.span < end)
            .map(|o| o.span)
            .max()?;
        errors.iter().filter(|e| begin < **e && **e < end).min().copied()
    };
    // `\@currenvir` starts out as `document`, so an `\end` in the preamble
    // is "ended by" a document that was never begun: it closes nothing, and
    // TeX follows up with `Extra \endgroup`.
    let begun = |name: &str, before: Span| {
        let count = |kind: OccKind| {
            report
                .analysis
                .facts
                .occurrences
                .iter()
                .filter(|o| o.kind == kind && o.key == name && o.span.file == before.file)
                .filter(|o| (o.span.line, o.span.col) < (before.line, before.col))
                .count()
        };
        count(OccKind::BeginEnvironment) > count(OccKind::EndEnvironment)
    };
    for (span, open, found, message, certain, i) in mismatches {
        let message = message.split("\n\n").next().unwrap_or_default().to_string();
        // Over-approximated: reported, but as a consequence to check after
        // its cause, never as a certain fault.
        let cause = cascade(&open, span)
            .map(|at| format!("the error at {}", report.place(at)))
            .or_else(|| crate::lint::shared::doubt(report.analysis(), i));
        if let Some(cause) = cause {
            report.add_doubted(span, &found, message, &cause);
            continue;
        }
        if !certain {
            let message = format!("{message} (on some of the paths the analysis followed)");
            report.add_as(Severity::Info, span, &found, message, None);
            continue;
        }
        if !begun(&open, span) {
            let edits = report.edits().delete_command(span, &found);
            report.add_fix(
                span,
                &found,
                format!("{message}: `\\end{{{found}}}` closes nothing that is open"),
                Fix::new(format!("delete this `\\end{{{found}}}`"), Applicability::Unsafe, edits),
            );
            continue;
        }
        let edits = report.edits().replace_argument(span, &found, &open);
        report.add_fix(
            span,
            &found,
            message,
            Fix::new(
                format!("write `\\end{{{open}}}`, or close `\\begin{{{open}}}` first"),
                Applicability::Unsafe,
                edits,
            ),
        );
    }
}

/// What the packages themselves say when an entry is missing: glossaries'
/// `Glossary entry `⟨label⟩' has not been defined`, acro's `You've requested
/// acronym `⟨label⟩'` and acronym's `Acronym ⟨label⟩ is not defined`.
fn undefined_glossary_entry(report: &mut Report) {
    let quoted = |text: &str, after: &str| -> Option<String> {
        let rest = &text[text.find(after)? + after.len()..];
        Some(rest[..rest.find('\'')?].to_string())
    };
    let mut seen = crate::fixed::HashSet::default();
    let missing: Vec<(Span, String)> = report
        .analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Message)
        .filter_map(|o| {
            let key = quoted(&o.key, "Glossary entry `")
                .filter(|_| o.key.contains("has not been defined"))
                .or_else(|| quoted(&o.key, "requested acronym `"))
                .or_else(|| {
                    let rest = &o.key[o.key.find("Acronym ")? + "Acronym ".len()..];
                    Some(rest[..rest.find(" is not defined")?].to_string())
                })?;
            Some((o.span, key))
        })
        .filter(|found| seen.insert(found.clone()))
        .collect();
    for (span, key) in missing {
        report.add(
            span,
            &key,
            format!("no entry `{key}` is declared"),
            Some(format!("declare it with `\\newglossaryentry{{{key}}}`")),
        );
    }
}

/// An expl3 function's name against what it was seen, or probed, to take.
fn expl3_signature_mismatch(report: &mut Report) {
    let analysis = report.analysis();
    let mut seen = vec![false; analysis.facts.defs.iter().map(|d| d.name.0 as usize + 1).max().unwrap_or(0)];
    let mut found = Vec::new();
    for def in analysis.facts.defs.iter().rev() {
        let Some(mac) = &def.mac else { continue };
        if std::mem::replace(&mut seen[def.name.0 as usize], true) {
            continue;
        }
        let name = analysis.interner.name(def.name);
        let Some(declared) = crate::tex::expl3_arity(name) else { continue };
        // Only the meaning in force is what calls and probes describe.
        let current = analysis.env.meaning(def.name);
        if !current.as_macro().is_some_and(|m| std::rc::Rc::ptr_eq(m, mac)) {
            continue;
        }
        // Probing costs a run per item: a library's functions are judged by
        // the calls the run saw.
        let (mandatory, other) = if def.span.file == analysis.main_file {
            let Some(spec) = crate::query::signature(analysis, def.name, mac) else { continue };
            let mandatory = spec.items.iter().filter(|i| matches!(i, crate::tex::ArgType::Mandatory)).count();
            (mandatory, spec.items.len() - mandatory)
        } else {
            let Some(shape) = analysis.facts.shapes.get(&def.name) else { continue };
            let mandatory = shape.matches("{}").count();
            (mandatory, shape.matches('*').count() + shape.matches('[').count())
        };
        if mandatory == declared as usize && other == 0 {
            continue;
        }
        let takes = match other {
            0 => format!("{mandatory} argument(s)"),
            _ => format!("{mandatory} mandatory argument(s) and {other} optional one(s)"),
        };
        found.push((
            def.span,
            analysis.interner.cs(def.name),
            format!("`{}` names {declared} argument(s) but takes {takes}", analysis.interner.cs(def.name)),
        ));
    }
    for (span, name, message) in found {
        report.add(span, &name, message, None);
    }
}
