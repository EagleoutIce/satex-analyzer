use crate::builtins::OccKind;
use crate::facts::{LoadStatus, MeaningKind};
use crate::lint::fix::{Applicability, Fix};
use crate::lint::shared::{first_loads, labels};
use crate::lint::{Category, Report, Rule, Severity};
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
        run: undefined_glossary_entry,
    },
    Rule {
        code: "shell-escape",
        category: Category::Security,
        severity: Severity::Warning,
        summary: "the document runs a program through \\write18",
        explanation: "\
`\\write18` hands its text to the shell.  A run only does that when the engine
was started with `--shell-escape`, or with `--shell-restricted` and the
program is one TeX Live allows.  `shell_escape` in `satex.yaml` says which of
the three this build is.

Fix by running with the flag the document needs, or by removing the call.",
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
        run: missing_graphic,
    },
    Rule {
        code: "expl3-signature-mismatch",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "an expl3 function takes other arguments than its name says",
        explanation: "\
An expl3 name carries its signature after the colon ([interface3](https://ctan.org/pkg/expl3), \"Naming
conventions\"): one letter per argument, `N` a single token, `n` a braced
group, and so on.  What the function really consumes (the calls the run saw,
or, for one defined in the document, running it on probe input) takes a
different number of arguments, or an optional one or a star, which no expl3
signature has.  `\\foo:nn` that takes one argument leaves the second for
whatever follows.

Fix by renaming the function to the signature it has, or its parameter text.",
        run: expl3_signature_mismatch,
    },
    Rule {
        code: "unguarded-recursion",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "a recursive macro has nothing that can stop it",
        explanation: "\
The run saw the macro's expansion reach the macro again, from a call written
in the replacement text of a macro of the cycle (a tail call included), the
run could not see the recursion end, and its replacement text contains no
conditional — no `\\if…`, `\\else` or
`\\fi` — so nothing can end the recursion.  A macro nothing expands is judged
by the names its replacement text holds, and reported as a warning.  A real run expands until TeX runs out of memory:
`TeX capacity exceeded, sorry [main memory size]`.

Fix by adding the test that stops it, or by removing the self-reference.",
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

Fix what the message says.",
        run: raised_errors,
    },
    Rule {
        code: "undefined-environment",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "\\begin names an environment that is not defined",
        explanation: "\
`\\begin{name}` expands `\\name`, and nothing defines it.  A real run reports
`Environment name undefined`.

Fix by loading the package that provides the environment or by defining it with
`\\newenvironment{name}{…}{…}`.",
        run: undefined_environments,
    },
    Rule {
        code: "unresolved-file",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "a package, class or input file is not in the installation",
        explanation: "\
The file was searched for in the document's directory, the configured search
paths and the TEXMF trees, and not found.  `satex query distribution` shows
which installation was searched.

Fix by installing the package, adding its directory to `search_paths` in
`satex.yaml`, or pointing `texmf_roots` at the right installation.",
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
        run: option_clash,
    },
    Rule {
        code: "already-defined",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "\\newcommand redefines an existing name",
        explanation: "\
`\\newcommand` refuses to redefine and raises `Command … already defined`.

Fix with `\\renewcommand` if the redefinition is intended, or pick another name.",
        run: already_defined,
    },
    Rule {
        code: "not-defined",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "\\renewcommand targets a name that does not exist",
        explanation: "\
`\\renewcommand` requires an existing definition and raises
`Undefined control sequence`.

Fix with `\\newcommand`, or load the package that was supposed to define it.",
        run: not_defined,
    },
    Rule {
        code: "undefined-reference",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "\\ref names a label that is never defined",
        explanation: "\
No `\\label` in the document or its inputs declares this key, so the reference
prints `??`.

Fix by adding the `\\label`, or by correcting the key.",
        run: undefined_reference,
    },
    Rule {
        code: "duplicate-label",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "the same label is defined twice",
        explanation: "\
LaTeX warns `Label … multiply defined` and every reference resolves to the
later one.

Fix by renaming one of them.",
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
        run: catcode_escapes_group,
    },
    Rule {
        code: "environment-mismatch",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "\\end names an environment other than the one that is open",
        explanation: "\
Environments nest like brackets: the name `\\end` gives has to match the one
`\\begin` most recently opened.  chktex tracks this as warnings 9 (`'%s'
expected, found '%s'`), 10 (`Solo '%s' found`) and 15 (`No match found for
'%s'`) for brackets and environments alike; satex only records `\\begin`/`\\end`
occurrences, so this rule covers environments and not generic `{}`, `[]` or
`()` nesting.

Fix by naming the environment that is actually open, or by closing the inner
environments first.",
        run: environment_mismatch,
    },
    Rule {
        code: "expl-syntax-left-on",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "\\ExplSyntaxOn is never turned off",
        explanation: "\
With expl3 syntax on, spaces are ignored and `_` and `:` are letters, so
ordinary text stops working.

Fix by adding `\\ExplSyntaxOff` after the code that needs it.",
        run: expl_syntax_left_on,
    },
];

fn unguarded_recursion(report: &mut Report) {
    use crate::builtins::Primitive;
    use crate::tex::Tok;

    let analysis = report.analysis();
    let recursive: std::collections::BTreeMap<Sym, bool> =
        analysis.recursion().into_iter().map(|r| (r.name, r.observed)).collect();
    let sites: std::collections::HashSet<crate::tex::Span> =
        analysis.facts.diagnostics.iter().filter(|d| d.code == "recursion-widened").map(|d| d.span).collect();
    let widened: std::collections::HashSet<Sym> =
        analysis.facts.expansions.iter().filter(|e| sites.contains(&e.span)).map(|e| e.name).collect();
    let defs: Vec<_> = analysis.facts.defs.clone();
    for def in defs {
        let Some(&observed) = recursive.get(&def.name) else { continue };
        // A recursion the run saw end had something to stop it.
        if observed && !widened.contains(&def.name) {
            continue;
        }
        let Some(mac) = &def.mac else { continue };
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
                    "{name} names itself in its replacement text with no conditional to stop it; \
                     nothing expands it, so this is read off the text"
                ),
            )
        };
        report.add_as(severity, def.span, &name, message, Some("add the test that ends the recursion".into()));
    }
}

fn undefined_control_sequences(report: &mut Report) {
    // Under ConTeXt satex knows the primitives and nothing else, so every
    // ConTeXt command would look undefined; it reports none rather than all.
    if report.analysis.plugins.kernel == crate::plugin::Kernel::Context {
        return;
    }
    for expansion in &report.analysis.facts.expansions {
        if expansion.meaning != MeaningKind::Undefined {
            continue;
        }
        let name = report.cs(expansion.name);
        let (severity, message) = if expansion.cds.is_empty() {
            (Severity::Error, format!("{name} is never defined in this run"))
        } else {
            (Severity::Warning, format!("{name} is undefined on a path satex could not decide"))
        };
        report.add_as(
            severity,
            expansion.span,
            &name,
            message,
            Some(format!("define {name} or load the package that provides it")),
        );
    }
}

/// Every `\errmessage` a real run reaches, once per place and text, placed
/// at the request that read the file raising it.
fn raised_errors(report: &mut Report) {
    let analysis = report.analysis();
    let mut seen = std::collections::HashSet::new();
    for o in &analysis.facts.occurrences {
        if !crate::lint::shared::is_error(analysis, o) || !o.certain || !seen.insert((o.span, o.key.as_str())) {
            continue;
        }
        let mut span = o.span;
        let mut via = None;
        for _ in 0..64 {
            if crate::query::origin(analysis, span.file) == "document" {
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
        report.add(span, "", message, None);
    }
}

/// The kernel's own error (latex.ltx `\begin`): `LaTeX Error: Environment
/// ⟨name⟩ undefined.`
fn undefined_environments(report: &mut Report) {
    let found = kernel_errors(report, |text| {
        let rest = &text[text.find("LaTeX Error: Environment ")? + "LaTeX Error: Environment ".len()..];
        Some(rest[..rest.find(" undefined")?].to_string())
    });
    for (span, name) in found {
        let message = format!("LaTeX Error: Environment {name} undefined");
        report.add(span, &name, message, Some(format!("\\newenvironment{{{name}}}{{…}}{{…}}")));
    }
}

fn unresolved_files(report: &mut Report) {
    let distribution = report.analysis.distribution.describe();
    let empty = report.analysis.distribution.indexed == 0;
    let loads: Vec<_> = report.analysis.facts.loads.clone();
    for load in loads {
        if load.name.is_empty() {
            continue;
        }
        let message = match load.status {
            LoadStatus::NotFound if empty => format!("no TeX installation to search in: {distribution}"),
            LoadStatus::NotFound => {
                format!("{} `{}` is not in this installation ({distribution})", load.kind.as_str(), load.name)
            }
            LoadStatus::Unreadable => format!("`{}` was found but could not be read", load.name),
            _ => continue,
        };
        let fix = if empty {
            Some("set texmf_roots or texlive_root in satex.yaml".to_string())
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
    for (span, name) in missing {
        let message = format!("`{name}` is not in this installation ({distribution})");
        report.add(span, &name, message, Some(format!("install `{name}`, or add its directory to search_paths")));
    }
}

fn option_clash(report: &mut Report) {
    let first = first_loads(report.analysis);
    let loads: Vec<_> = report.analysis.facts.loads.clone();
    for load in loads {
        if !load.kind.is_package() {
            continue;
        }
        let Some((options, at)) = first.get(load.name.as_str()) else { continue };
        if *at == load.span {
            continue;
        }
        let extra: Vec<&str> = load.options.iter().filter(|o| !options.contains(o)).map(String::as_str).collect();
        if extra.is_empty() {
            continue;
        }
        let first_load = report.analysis.facts.loads.iter().find(|l| l.span == *at && l.name == load.name).cloned();
        let edits = first_load.and_then(|first| report.edits().add_options(&first, &extra));
        report.add_fix(
            load.span,
            &load.name,
            format!("{} was first loaded at {at} without {}", load.name, extra.join(", ")),
            Fix::new(
                format!("move the options to the first \\usepackage{{{}}}", load.name),
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
    for (span, name) in refused {
        let edits = renamed(report, span, "new", "renew");
        report.add_fix(
            span,
            &name,
            format!("{name} is already defined"),
            Fix::new(format!("use \\renewcommand{{{name}}} instead"), Applicability::Unsafe, edits),
        );
    }
    let refused: Vec<(Span, String)> = report
        .analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.code == "already-defined")
        .map(|d| (d.span, d.message.clone()))
        .collect();
    for (span, message) in refused {
        let name = message.split('`').nth(1).unwrap_or_default().to_string();
        let edits = renamed(report, span, "new", "renew");
        report.add_fix(
            span,
            &name,
            message,
            Fix::new(format!("use \\renewcommand{{{name}}} instead"), Applicability::Unsafe, edits),
        );
    }
}

/// The defining command at `span` with its `from` prefix turned into `to`.
fn renamed(report: &Report, span: Span, from: &str, to: &str) -> Option<Vec<crate::lint::fix::Edit>> {
    let edits = report.edits();
    edits.rename_command(span, |name| edits.counterpart(name, from, to))
}

/// The kernel's `\renew@command` of an unknown name: `Command \⟨name⟩
/// undefined`;
/// it defines the name all the same (ltdefns).
fn not_defined(report: &mut Report) {
    let missing = kernel_errors(report, |text| {
        let rest = &text[text.find("Command \\")? + "Command ".len()..];
        Some(rest[..rest.find(" undefined")?].to_string())
    });
    for (span, name) in missing {
        let edits = renamed(report, span, "renew", "new");
        report.add_fix(
            span,
            &name,
            format!("{name} is not defined here"),
            Fix::new(format!("use \\newcommand{{{name}}} instead"), Applicability::Safe, edits),
        );
    }
}

/// The messages that match `parse`, once per place.
fn kernel_errors(report: &Report, parse: impl Fn(&str) -> Option<String>) -> Vec<(Span, String)> {
    let mut seen = std::collections::HashSet::new();
    report
        .analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Message)
        .filter_map(|o| Some((o.span, parse(&o.key)?)))
        .filter(|found| seen.insert(found.clone()))
        .collect()
}

fn undefined_reference(report: &mut Report) {
    let known = labels(report.analysis);
    let missing: Vec<(Span, String)> = report
        .analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Ref && !known.contains_key(o.key.as_str()))
        .map(|o| (o.span, o.key.clone()))
        .collect();
    for (span, key) in missing {
        report.add(
            span,
            &key,
            format!("no \\label{{{key}}} anywhere in the document"),
            Some(format!("add \\label{{{key}}} where it belongs")),
        );
    }
}

fn duplicate_label(report: &mut Report) {
    let first = labels(report.analysis);
    let repeats: Vec<(Span, String, Span)> = report
        .analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Label)
        .filter_map(|o| {
            let at = *first.get(o.key.as_str())?;
            (at != o.span).then(|| (o.span, o.key.clone(), at))
        })
        .collect();
    for (span, key, at) in repeats {
        report.add(
            span,
            &key,
            format!("label `{key}` is already defined at {at}"),
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
        Some("add \\ExplSyntaxOff".into()),
    );
}

/// graphics.sty's `\Ginclude@graphics` raises `File `⟨name⟩' not found`
/// when no extension in `\Gin@extensions` names a file on `\Ginput@path`;
/// the lint reports what the real code said, once per call.
fn missing_graphic(report: &mut Report) {
    let mut seen = std::collections::HashSet::new();
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
        report.add(span, &name, format!("no file for `{name}` in any format the graphics driver reads"), None);
    }
}

/// The programs a restricted build runs: the installation's own
/// `shell_escape_commands` (texmf.cnf), as `kpsewhich -var-value` reports it.
fn restricted_commands() -> Vec<String> {
    static COMMANDS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    COMMANDS
        .get_or_init(|| {
            std::process::Command::new("kpsewhich")
                .args(["-var-value", "shell_escape_commands"])
                .output()
                .ok()
                .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
                .map(|text| text.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect())
                .unwrap_or_default()
        })
        .clone()
}

fn shell_escape(report: &mut Report) {
    let analysis = report.analysis;
    let mode = analysis.settings.shell_escape;
    let allowed = restricted_commands();
    let occurrences: Vec<_> = analysis.facts.occurrences.clone();
    for occurrence in occurrences {
        if occurrence.kind != OccKind::ShellEscape {
            continue;
        }
        let program = occurrence.key.split_whitespace().next().unwrap_or_default().to_string();
        let (severity, message) = match mode {
            crate::config::ShellEscape::None => {
                (Severity::Error, format!("`{program}` cannot run: this build has no shell escape"))
            }
            crate::config::ShellEscape::Restricted if !allowed.contains(&program) => {
                (Severity::Error, format!("`{program}` is not one of the programs a restricted build runs"))
            }
            _ => (Severity::Info, format!("`{program}` runs through \\write18")),
        };
        report.add_as(severity, occurrence.span, &program, message, None);
    }
}

/// `\begin{a}` closed by `\end{b}`: chktex warnings 9 (mismatch), 10 (solo
/// close) and 15 (solo open), restricted to environments.  Occurrences are
/// walked in source order and matched with a plain stack, the same algorithm
/// chktex itself uses for brackets.
/// latex.ltx's `\@checkend` raises `\begin{x} on input line n ended by
/// \end{y}`; the rule reports the errors the run raised.
fn environment_mismatch(report: &mut Report) {
    let between = |text: &str, open: &str| -> Option<String> {
        let rest = &text[text.find(open)? + open.len()..];
        Some(rest[..rest.find('}')?].to_string())
    };
    let mismatches: Vec<(Span, String, String, String)> = report
        .analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Message && o.key.contains(" ended by "))
        .filter_map(|o| {
            let open = between(&o.key, "\\begin{")?;
            let found = between(&o.key, "\\end{")?;
            Some((o.span, open, found, o.key.clone()))
        })
        .collect();
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
    for (span, open, found, message) in mismatches {
        if !begun(&open, span) {
            let edits = report.edits().delete_command(span, &found);
            report.add_fix(
                span,
                &found,
                format!("{message}: \\end{{{found}}} closes nothing that is open"),
                Fix::new(format!("delete this \\end{{{found}}}"), Applicability::Unsafe, edits),
            );
            continue;
        }
        let edits = report.edits().replace_argument(span, &found, &open);
        report.add_fix(
            span,
            &found,
            message,
            Fix::new(format!("write \\end{{{open}}}, or close \\begin{{{open}}} first"), Applicability::Unsafe, edits),
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
    let mut seen = std::collections::HashSet::new();
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
            Some(format!("declare it with \\newglossaryentry{{{key}}}")),
        );
    }
}

/// An expl3 function's name against what it was seen, or probed, to take.
fn expl3_signature_mismatch(report: &mut Report) {
    let analysis = report.analysis();
    let mut seen = std::collections::HashSet::new();
    let mut found = Vec::new();
    for def in analysis.facts.defs.iter().rev() {
        let Some(mac) = &def.mac else { continue };
        if !seen.insert(def.name) {
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
