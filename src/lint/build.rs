//! The build configuration beside the document — a `latexmkrc`, `% arara:`
//! directives — held against what the run observed the document to need.
//!
//! What an option means is latexmk's own documentation (latexmk(1),
//! "LIST OF CONFIGURATION VARIABLES USABLE IN INITIALIZATION FILES") and
//! arara's ("The official rules").  What the document needs is never guessed
//! from a package name: a `\write18` the run reached, a primitive another
//! engine has, a `\pdfoutput` assignment, a bibliography, glossary or index
//! the document writes.

use std::path::{Path, PathBuf};

use super::{Category, Report, Rule, Severity};
use crate::builtins::OccKind;
use crate::config::Engine;
use crate::machine::Analysis;
use crate::tex::Interner;

pub static RULES: &[Rule] = &[
    Rule {
        code: "build-shell-escape-missing",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "the document runs programs through \\write18, but the build does not allow it",
        explanation: "\
The run reached `\\write18`, which only hands its text to the shell when the
engine is started with `-shell-escape` (or `-shell-restricted` and a program
TeX Live allows).  latexmk starts the engine with the command in `$pdflatex`,
`$lualatex`, `$xelatex` or `$latex`, whichever `$pdf_mode` selects (latexmk(1),
`$pdf_mode`); arara's `pdflatex`, `lualatex` and `xelatex` rules take
`shell: yes` (arara manual, \"The official rules\").

Fix by adding `-shell-escape` to the command, e.g.
`$pdflatex = 'pdflatex -shell-escape %O %S';`, or
`set_tex_cmds('-shell-escape %O %S');` for every engine at once.",
        run: shell_escape_missing,
    },
    Rule {
        code: "build-shell-escape-unneeded",
        category: Category::Security,
        severity: Severity::Warning,
        summary: "the build allows \\write18, but the document never uses it",
        explanation: "\
`-shell-escape` lets every package and every `.aux` or `.bbl` line the
document reads run any program as you.  The run reached no `\\write18`, so the
permission buys nothing and only widens what a hostile input file can do.

Fix by removing `-shell-escape` from the engine command (or `shell: yes` from
the arara directive).",
        run: shell_escape_unneeded,
    },
    Rule {
        code: "build-engine-mismatch",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "the build runs another engine, or another output, than the document needs",
        explanation: "\
latexmk's `$pdf_mode` picks the engine: 1 runs `$pdflatex`, 4 `$lualatex`, 5
`$xelatex`, and 0, 2 and 3 run `$latex` for DVI (latexmk(1), `$pdf_mode`).  The
document contradicts it when a `% !TeX program` line names another engine,
when it uses a primitive only another engine has (`\\directlua` is LuaTeX's,
`\\XeTeXrevision` XeTeX's), or when it sets `\\pdfoutput` to the other output
than latexmk waits for.

Fix by setting `$pdf_mode` to the engine the document is written for.",
        run: engine_mismatch,
    },
    Rule {
        code: "build-bibliography-disabled",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "the document has a bibliography, but the build never runs BibTeX or biber",
        explanation: "\
`$bibtex_use = 0` tells latexmk never to run BibTeX or biber (latexmk(1),
`$bibtex_use`), so the `.bbl` file the bibliography is read from is never made
and every citation stays undefined.

Fix by removing the setting; latexmk's default, 1, runs them whenever the
`.bib` files exist.",
        run: bibliography_disabled,
    },
    Rule {
        code: "build-bibliography-unneeded",
        category: Category::Style,
        severity: Severity::Info,
        summary: "the build configures BibTeX or biber for a document without a bibliography",
        explanation: "\
The latexmkrc sets `$bibtex_use`, `$bibtex` or `$biber`, but the document names
no bibliography database (`\\bibliography`, `\\addbibresource`), so the setting
never takes effect (latexmk(1), `$bibtex_use`).

Fix by removing the setting.",
        run: bibliography_unneeded,
    },
    Rule {
        code: "build-missing-custom-dependency",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "the document writes glossary files, but latexmk has no rule to process them",
        explanation: "\
latexmk runs BibTeX, biber and makeindex by itself, but a glossary is sorted by
a program it does not know about (`makeglossaries`, `makeindex` with a glossary
style, `bib2gls`).  Without a custom dependency (latexmk(1), \"CUSTOM
DEPENDENCIES\") the glossary files are never made and the glossary stays empty.
The glossaries manual gives the rule, e.g.

    add_cus_dep('glo', 'gls', 0, 'makeglossaries');
    sub makeglossaries { system(\"makeglossaries '$_[0]'\"); }",
        run: missing_custom_dependency,
    },
    Rule {
        code: "build-unused-custom-dependency",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a latexmk custom dependency that nothing in the document triggers",
        explanation: "\
`add_cus_dep(from, to, …)` makes latexmk run a program when a file with the
`from` extension is newer than the `to` file it produces (latexmk(1), \"CUSTOM
DEPENDENCIES\").  The document writes no glossary or index, and no file with the
`from` extension is in the project, so the rule never runs.

Fix by removing the rule, or keep it if another document of the project needs it.",
        run: unused_custom_dependency,
    },
    Rule {
        code: "build-command-placeholders",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "an engine command in the latexmkrc lacks %S or %O",
        explanation: "\
latexmk substitutes `%S` with the source file and `%O` with the options it
passes itself (latexmk(1), \"FORMAT OF COMMAND SPECIFICATIONS\").  A command
without `%S` compiles nothing, and one without `%O` loses `-jobname`,
`-output-directory` and the options given on latexmk's own command line.

Fix by ending the command with `%O %S`.",
        run: command_placeholders,
    },
    Rule {
        code: "build-engine-options",
        category: Category::Style,
        severity: Severity::Info,
        summary: "the engine command could report errors as file:line and write SyncTeX data",
        explanation: "\
`-file-line-error` makes the engine report `file:line: message` instead of
`! message`, which editors and satex's log reading can jump to; `-synctex=1`
writes the data that links the PDF back to the source (pdftex(1), luatex(1)).
Both cost nothing measurable.

Fix by adding them to the command, e.g.
`$pdflatex = 'pdflatex -file-line-error -synctex=1 %O %S';`.",
        run: engine_options,
    },
];

/// One `$name = value;` of a latexmkrc, where it stands.
struct Setting {
    line: u32,
    key: String,
    value: String,
}

/// One `add_cus_dep(from, to, must, sub)`.
struct CustomDependency {
    line: u32,
    from: String,
    to: String,
}

/// What satex can read out of a latexmkrc without running Perl: the plain
/// assignments, `set_tex_cmds(…)` and `add_cus_dep(…)`.
struct Latexmkrc {
    path: PathBuf,
    settings: Vec<Setting>,
    /// `set_tex_cmds('…')`: the options every engine command gets.
    tex_cmds: Option<Setting>,
    dependencies: Vec<CustomDependency>,
}

/// The engine command variables latexmk knows (latexmk(1), `$pdflatex` and
/// its siblings), each with the engine it runs.
const COMMANDS: [(&str, Engine); 4] = [
    ("pdflatex", Engine::PdfTeX),
    ("lualatex", Engine::LuaTeX),
    ("xelatex", Engine::XeTeX),
    ("latex", Engine::PdfTeX),
];

/// The options that let `\write18` through (pdftex(1), `-shell-escape`,
/// `-enable-write18`).
const SHELL_ESCAPE: [&str; 4] = ["-shell-escape", "--shell-escape", "-enable-write18", "--enable-write18"];

impl Latexmkrc {
    fn read(path: &Path) -> Option<Latexmkrc> {
        let text = std::fs::read_to_string(path).ok()?;
        let mut rc = Latexmkrc {
            path: path.to_path_buf(),
            settings: Vec::new(),
            tex_cmds: None,
            dependencies: Vec::new(),
        };
        for (number, raw) in text.lines().enumerate() {
            let line = number as u32 + 1;
            let code = perl_code(raw).trim();
            if let Some(rest) = code.strip_prefix('$')
                && let Some((key, value)) = rest.split_once('=')
                && !key.ends_with(['.', '!', '=', '<', '>'])
            {
                let key = key.trim().to_string();
                rc.settings.push(Setting { line, key, value: perl_value(value) });
            } else if let Some(arguments) = call(code, "set_tex_cmds") {
                let value = arguments.first().cloned().unwrap_or_default();
                rc.tex_cmds = Some(Setting { line, key: "set_tex_cmds".into(), value });
            } else if let Some(arguments) = call(code, "add_cus_dep")
                && let [from, to, ..] = arguments.as_slice() {
                    rc.dependencies.push(CustomDependency { line, from: from.clone(), to: to.clone() });
                }
        }
        Some(rc)
    }

    fn setting(&self, key: &str) -> Option<&Setting> {
        self.settings.iter().rev().find(|s| s.key == key)
    }

    /// The engine command variable `$pdf_mode` selects, and the engine it
    /// runs; `None` when `$pdf_mode` is not set here, which leaves the
    /// choice to latexmk's command line.
    fn selected(&self) -> Option<(&'static str, Engine)> {
        let mode = self.setting("pdf_mode")?;
        Some(match mode.value.as_str() {
            "1" => COMMANDS[0],
            "4" => COMMANDS[1],
            "5" => COMMANDS[2],
            _ => COMMANDS[3],
        })
    }

    /// The command lines in force for the engines this build may run: the
    /// selected one, or every one the file sets when none is selected.
    fn commands(&self) -> Vec<(&'static str, Option<&Setting>)> {
        match self.selected() {
            Some((name, _)) => vec![(name, self.setting(name))],
            None => COMMANDS.iter().map(|(name, _)| (*name, self.setting(name))).collect(),
        }
    }

    /// Whether the command for `name` lets `\write18` through, and the line
    /// that says so.
    fn shell_escape(&self, name: &str) -> Option<u32> {
        let allows = |setting: &Setting| {
            setting.value.split_whitespace().any(|word| SHELL_ESCAPE.contains(&word))
        };
        if let Some(setting) = self.setting(name).filter(|s| allows(s)) {
            return Some(setting.line);
        }
        self.tex_cmds.as_ref().filter(|s| allows(s)).map(|s| s.line)
    }

    fn place(&self) -> (&Path, u32) {
        (&self.path, 1)
    }
}

/// A Perl line without its comment.  A `#` inside a quoted string is text.
fn perl_code(line: &str) -> &str {
    let mut quote: Option<char> = None;
    for (at, c) in line.char_indices() {
        match (quote, c) {
            (None, '\'' | '"') => quote = Some(c),
            (Some(open), _) if c == open => quote = None,
            (None, '#') => return &line[..at],
            _ => {}
        }
    }
    line
}

/// The right-hand side of an assignment, without its quotes and `;`.
fn perl_value(value: &str) -> String {
    let value = value.trim().trim_end_matches(';').trim();
    value.trim_matches(['"', '\'']).trim().to_string()
}

/// The arguments of `name(…)` on one line, unquoted.
fn call(code: &str, name: &str) -> Option<Vec<String>> {
    let rest = code.strip_prefix(name)?.trim_start();
    let inner = rest.strip_prefix('(')?;
    let inner = &inner[..inner.rfind(')')?];
    Some(inner.split(',').map(perl_value).collect())
}

/// The latexmkrc files of the project, read.
fn latexmkrcs(analysis: &Analysis) -> Vec<Latexmkrc> {
    analysis
        .project
        .files
        .iter()
        .filter(|file| file.tool == "latexmk")
        .filter_map(|file| Latexmkrc::read(&file.path))
        .collect()
}

/// One `% arara:` directive: the rule it runs and its options.
struct Directive {
    line: u32,
    rule: String,
    options: String,
}

fn directives(analysis: &Analysis) -> Vec<Directive> {
    analysis
        .plugins
        .magic
        .iter()
        .filter(|m| m.key == "arara")
        .map(|m| {
            let text = m.value.trim();
            let (rule, options) = match text.split_once(':') {
                Some((rule, options)) => (rule.trim(), options.trim()),
                None => (text, ""),
            };
            Directive { line: m.line as u32, rule: rule.to_string(), options: options.to_string() }
        })
        .collect()
}

impl Directive {
    /// The engine an arara rule runs, when it is one of the engine rules
    /// (arara manual, \"The official rules\": `pdflatex`, `lualatex`,
    /// `xelatex`, `latex`).
    fn engine(&self) -> Option<Engine> {
        COMMANDS.iter().find(|(name, _)| *name == self.rule).map(|(_, engine)| *engine)
    }

    /// `shell: yes` (or `true`, `on`), as arara's YAML reads it.
    fn shell_escape(&self) -> bool {
        let options = self.options.replace(' ', "");
        ["shell:yes", "shell:true", "shell:on"].iter().any(|o| options.contains(o))
    }
}

fn used(analysis: &Analysis, kinds: &[OccKind]) -> bool {
    analysis.facts.occurrences.iter().any(|o| kinds.contains(&o.kind))
}

/// Whether the document writes lines to a file with one of `extensions`.
fn writes(analysis: &Analysis, extensions: &[&str]) -> bool {
    analysis.facts.written_files.iter().any(|file| {
        Path::new(file).extension().and_then(|e| e.to_str()).is_some_and(|e| extensions.contains(&e))
    })
}

fn file_name(path: &Path) -> String {
    path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string()
}

fn shell_escape_missing(report: &mut Report) {
    let analysis = report.analysis();
    if !used(analysis, &[OccKind::ShellEscape]) {
        return;
    }
    for rc in latexmkrcs(analysis) {
        for (name, setting) in rc.commands() {
            if rc.shell_escape(name).is_some() {
                continue;
            }
            let line = setting.map_or(rc.place().1, |s| s.line);
            let current = setting.map_or(format!("{name} %O %S"), |s| s.value.clone());
            report.add_in(
                &rc.path,
                line,
                &format!("${name}"),
                format!("the document runs programs through \\write18, but `${name}` is `{current}`"),
                Some(format!("${name} = '{}';", with_option(&current, "-shell-escape"))),
            );
        }
    }
    let main = analysis.file_name(analysis.main_file).to_string();
    for directive in directives(analysis) {
        if directive.engine().is_some() && !directive.shell_escape() {
            report.add_in(
                Path::new(&main),
                directive.line,
                &directive.rule,
                format!("the document runs programs through \\write18, but `% arara: {}` has no `shell: yes`", directive.rule),
                Some(format!("% arara: {}: {{ shell: yes }}", directive.rule)),
            );
        }
    }
}

/// `command` with `option` added right after the program name.
fn with_option(command: &str, option: &str) -> String {
    match command.split_once(' ') {
        Some((program, rest)) => format!("{program} {option} {rest}"),
        None => format!("{command} {option}"),
    }
}

fn shell_escape_unneeded(report: &mut Report) {
    let analysis = report.analysis();
    // A run that did not read the packages cannot say that none of them
    // calls the shell.
    if used(analysis, &[OccKind::ShellEscape]) || !analysis.settings.load_packages {
        return;
    }
    for rc in latexmkrcs(analysis) {
        let names: Vec<&str> = match rc.selected() {
            Some((name, _)) => vec![name],
            None => COMMANDS.iter().map(|(name, _)| *name).collect(),
        };
        let mut lines: Vec<u32> = names.iter().filter_map(|name| rc.shell_escape(name)).collect();
        lines.sort_unstable();
        lines.dedup();
        for line in lines {
            report.add_in(
                &rc.path,
                line,
                "-shell-escape",
                "the build lets the document run any program, and the document never runs one".into(),
                Some("remove -shell-escape".into()),
            );
        }
    }
    let main = analysis.file_name(analysis.main_file).to_string();
    for directive in directives(analysis) {
        if directive.engine().is_some() && directive.shell_escape() {
            report.add_in(
                Path::new(&main),
                directive.line,
                &directive.rule,
                "`shell: yes` lets the document run any program, and the document never runs one".into(),
                Some(format!("% arara: {}", directive.rule)),
            );
        }
    }
}

/// The engines whose own primitives the run met undefined: a document that
/// calls `\directlua` needs LuaTeX whatever it was compiled with.
fn engines_needed(analysis: &Analysis) -> Vec<(Engine, String)> {
    let mut it = Interner::default();
    let tables: Vec<(Engine, std::collections::HashMap<crate::tex::Sym, crate::tex::Meaning>)> =
        [Engine::PdfTeX, Engine::LuaTeX, Engine::XeTeX]
            .into_iter()
            .map(|engine| (engine, crate::builtins::initial_meanings(&mut it, engine)))
            .collect();
    let mut out: Vec<(Engine, String)> = Vec::new();
    for expansion in &analysis.facts.expansions {
        if expansion.meaning != crate::facts::MeaningKind::Undefined {
            continue;
        }
        let name = analysis.interner.name(expansion.name);
        let Some(sym) = it.lookup(name) else { continue };
        let having: Vec<Engine> =
            tables.iter().filter(|(_, table)| table.contains_key(&sym)).map(|(e, _)| *e).collect();
        if having.len() == 1 && !out.iter().any(|(_, n)| n == name) {
            out.push((having[0], name.to_string()));
        }
    }
    out
}

fn program_engine(program: &str) -> Option<Engine> {
    let program = program.trim().to_ascii_lowercase();
    COMMANDS
        .iter()
        .chain(&[("pdftex", Engine::PdfTeX), ("luatex", Engine::LuaTeX), ("xetex", Engine::XeTeX)])
        .find(|(name, _)| *name == program)
        .map(|(_, engine)| *engine)
}

fn engine_mismatch(report: &mut Report) {
    let analysis = report.analysis();
    let rcs = latexmkrcs(analysis);
    let arara: Vec<Directive> = directives(analysis).into_iter().filter(|d| d.engine().is_some()).collect();
    if rcs.is_empty() && arara.is_empty() {
        return;
    }
    let named = analysis
        .plugins
        .magic
        .iter()
        .find(|m| m.key.eq_ignore_ascii_case("TeX program"))
        .and_then(|m| program_engine(&m.value).map(|engine| (engine, m.value.trim().to_string())));
    let needed = engines_needed(analysis);
    let pdfoutput = analysis.interner.lookup("pdfoutput").and_then(|sym| {
        let site = analysis.graph.vertices.iter().rev().find(|v| {
            v.name == sym && v.span.file == analysis.main_file && v.tag == crate::graph::VertexTag::VariableDefinition
        })?;
        Some((analysis.env.value(sym).as_int()?, site.span.line))
    });
    let main = analysis.file_name(analysis.main_file).to_string();
    let mut builds: Vec<(PathBuf, u32, String, Engine, bool)> = Vec::new();
    for rc in &rcs {
        if let (Some(mode), Some((name, engine))) = (rc.setting("pdf_mode"), rc.selected()) {
            builds.push((rc.path.clone(), mode.line, format!("$pdf_mode = {}", mode.value), engine, name != "latex"));
        }
    }
    for directive in &arara {
        let engine = directive.engine().unwrap_or(Engine::PdfTeX);
        builds.push((PathBuf::from(&main), directive.line, format!("% arara: {}", directive.rule), engine, directive.rule != "latex"));
    }
    for (path, line, what, engine, pdf) in builds {
        if let Some((wanted, program)) = &named
            && *wanted != engine
        {
            report.add_in(
                &path,
                line,
                what.as_str(),
                format!("`{what}` runs {}, but the document asks for `{program}`", engine.as_str()),
                None,
            );
        }
        for (wanted, name) in needed.iter().filter(|(wanted, _)| *wanted != engine) {
            report.add_in(
                &path,
                line,
                what.as_str(),
                format!("`{what}` runs {}, but the document uses \\{name}, which only {} has", engine.as_str(), wanted.as_str()),
                None,
            );
        }
        if engine == Engine::PdfTeX
            && let Some((value, at)) = pdfoutput
            && (value > 0) != pdf
        {
            report.add_in(
                &path,
                line,
                what.as_str(),
                format!(
                    "`{what}` waits for a {}, but line {at} of the document sets \\pdfoutput={value}",
                    if pdf { "PDF" } else { "DVI file" }
                ),
                None,
            );
        }
    }
}

fn has_bibliography(analysis: &Analysis) -> bool {
    used(analysis, &[OccKind::Bibliography, OccKind::BibItem, OccKind::Cite])
}

fn bibliography_disabled(report: &mut Report) {
    let analysis = report.analysis();
    if !has_bibliography(analysis) {
        return;
    }
    for rc in latexmkrcs(analysis) {
        if let Some(setting) = rc.setting("bibtex_use").filter(|s| s.value.parse::<f64>() == Ok(0.0)) {
            report.add_in(
                &rc.path,
                setting.line,
                "$bibtex_use",
                "the document has a bibliography, but `$bibtex_use = 0` never builds it".into(),
                Some("remove `$bibtex_use = 0;`".into()),
            );
        }
    }
}

fn bibliography_unneeded(report: &mut Report) {
    let analysis = report.analysis();
    if has_bibliography(analysis) || !analysis.settings.load_packages {
        return;
    }
    for rc in latexmkrcs(analysis) {
        for key in ["bibtex_use", "bibtex", "biber"] {
            let Some(setting) = rc.setting(key) else { continue };
            if key == "bibtex_use" && setting.value.parse::<f64>() == Ok(0.0) {
                continue;
            }
            report.add_in(
                &rc.path,
                setting.line,
                &format!("${key}"),
                format!("`${key}` is set, but the document has no bibliography"),
                Some(format!("remove `${key}`")),
            );
        }
    }
}

fn missing_custom_dependency(report: &mut Report) {
    let analysis = report.analysis();
    if !writes(analysis, crate::plugin::tool::Tool::Makeglossaries.consumes()) {
        return;
    }
    for rc in latexmkrcs(analysis) {
        if !rc.dependencies.is_empty() {
            continue;
        }
        let (path, line) = rc.place();
        report.add_in(
            path,
            line,
            "add_cus_dep",
            format!("the document has a glossary, but {} gives latexmk no rule to build it", file_name(path)),
            Some("add_cus_dep('glo', 'gls', 0, 'makeglossaries');".into()),
        );
    }
}

fn unused_custom_dependency(report: &mut Report) {
    let analysis = report.analysis();
    if !analysis.settings.load_packages {
        return;
    }
    for rc in latexmkrcs(analysis) {
        let directory = rc.path.parent().map(Path::to_path_buf).unwrap_or_default();
        for dependency in &rc.dependencies {
            if writes(analysis, &[dependency.from.trim_start_matches('.')]) || has_extension(&directory, &dependency.from) {
                continue;
            }
            report.add_in(
                &rc.path,
                dependency.line,
                "add_cus_dep",
                format!(
                    "the rule from .{} to .{} never runs: the document writes no .{} file, and none is here",
                    dependency.from, dependency.to, dependency.from
                ),
                Some("remove the rule".into()),
            );
        }
    }
}

/// Whether a file with extension `extension` is in `directory` or below it.
fn has_extension(directory: &Path, extension: &str) -> bool {
    let wanted = std::ffi::OsStr::new(extension.trim_start_matches('.'));
    let mut stack = vec![directory.to_path_buf()];
    let mut seen = 0usize;
    // A project directory, not the file system: a bound keeps a latexmkrc in
    // a home directory from walking all of it.
    const MAX_ENTRIES: usize = 10_000;
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            seen += 1;
            if seen > MAX_ENTRIES {
                return true;
            }
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension() == Some(wanted) {
                return true;
            }
        }
    }
    false
}

fn command_placeholders(report: &mut Report) {
    let analysis = report.analysis();
    for rc in latexmkrcs(analysis) {
        for (name, _) in COMMANDS {
            let Some(setting) = rc.setting(name) else { continue };
            // latexmk runs an internal command, not a program, for these.
            if setting.value.starts_with("internal ") || setting.value.is_empty() {
                continue;
            }
            let missing: Vec<&str> = ["%O", "%S"].into_iter().filter(|p| !setting.value.contains(p)).collect();
            if missing.is_empty() {
                continue;
            }
            report.add_in(
                &rc.path,
                setting.line,
                &format!("${name}"),
                format!("`${name}` has no {}", missing.join(" and ")),
                Some(format!("${name} = '{} {}';", setting.value, missing.join(" "))),
            );
        }
    }
}

fn engine_options(report: &mut Report) {
    let analysis = report.analysis();
    for rc in latexmkrcs(analysis) {
        for (name, setting) in rc.commands() {
            let Some(setting) = setting else { continue };
            let words: Vec<&str> = setting.value.split_whitespace().collect();
            let has = |prefixes: &[&str]| {
                words.iter().any(|w| prefixes.iter().any(|p| w.trim_start_matches('-') == p.trim_start_matches('-') || w.starts_with(p)))
            };
            let mut missing = Vec::new();
            if !has(&["-file-line-error"]) {
                missing.push("-file-line-error");
            }
            if !has(&["-synctex", "--synctex"]) {
                missing.push("-synctex=1");
            }
            if missing.is_empty() {
                continue;
            }
            let mut suggestion = setting.value.clone();
            for option in &missing {
                suggestion = with_option(&suggestion, option);
            }
            report.add_in(
                &rc.path,
                setting.line,
                &format!("${name}"),
                format!("`${name}` could also pass {}", missing.join(" and ")),
                Some(format!("${name} = '{suggestion}';")),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_latexmkrc_is_read_without_perl() {
        let dir = std::env::temp_dir().join("satex-lint-build-rc");
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("latexmkrc");
        std::fs::write(
            &path,
            "# a comment\n$pdf_mode = 4; # lualatex\n$lualatex = \"lualatex -shell-escape %O %S\";\n\
             add_cus_dep('glo', 'gls', 0, 'makeglossaries');\nset_tex_cmds('-synctex=1 %O %S');\n",
        )
        .unwrap();
        let rc = Latexmkrc::read(&path).unwrap();
        assert_eq!(rc.selected().map(|(n, _)| n), Some("lualatex"));
        assert_eq!(rc.setting("lualatex").map(|s| s.line), Some(3));
        assert_eq!(rc.shell_escape("lualatex"), Some(3));
        assert_eq!(rc.dependencies.len(), 1);
        assert_eq!((rc.dependencies[0].from.as_str(), rc.dependencies[0].to.as_str()), ("glo", "gls"));
        assert_eq!(rc.tex_cmds.as_ref().map(|s| s.value.as_str()), Some("-synctex=1 %O %S"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
