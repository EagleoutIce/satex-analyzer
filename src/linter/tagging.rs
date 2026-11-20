//! PDF tagging and accessibility: what `\DocumentMetadata` asked for against what the document
//! gives it. The facts come from [`crate::analysis::tagging`]: the document properties the run
//! recorded and the installation's tagging-status list, never a table of names here.

use super::{Category, Ref, Report, Rule, Severity, Source};
use crate::analysis::tagging::{self, Val};
use crate::engine::builtins::{LoadKind, OccKind};
use crate::engine::tex::Span;
use crate::linter::fix::{Applicability, Edit, Fix, Pos};
use crate::project::config::Engine;

pub static RULES: &[Rule] = &[
    Rule {
        code: "metadata-after-class",
        since: "0.2.5",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "DocumentMetadata after documentclass",
        explanation: "\
`\\DocumentMetadata` sets up the PDF management and tagging code before
anything else is loaded, so it has to be the first command of the document.
After `\\documentclass` it raises an error, and none of its keys take effect:
no tagging, no PDF standard, no language.  A second call after a first one
before the class is fine: it only sets more keys.

Fix by moving the call above `\\documentclass`.",
        references: &[Ref { source: Source::LatexLab, at: "documentmetadata-support" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: metadata_after_class,
    },
    Rule {
        code: "tagging-incompatible-package",
        since: "0.2.5",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "package incompatible with tagging",
        explanation: "\
The LaTeX tagging project keeps a status for each package and class, and TeX
Live ships it as `latex-tagging-status.ltx` (what
`\\DocumentMetadata{check-tagging-status}` reports from).  satex runs that
file and looks up every file the run loaded.  A file marked *currently
incompatible* or *no support* breaks the structure tree or the tagged content
in ways the run does not report: the PDF may fail validation, or read wrongly
in a screen reader.

Fix by replacing the package (the status page names alternatives), or by
switching tagging off until it is supported.",
        references: &[Ref { source: Source::TaggingProject, at: "tagging-status" }],
        fixes: &[],
        engines: &[],
        run: incompatible_packages,
    },
    Rule {
        code: "graphic-without-alt",
        since: "0.2.5",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "graphic without alternative text",
        explanation: "\
With tagging on, every `\\includegraphics` becomes a `Figure` structure
element, which needs alternative text (PDF/UA, ISO 14289-1 § 7.3).  Without
`alt=`, latex-lab warns and uses the file name instead, which a screen reader
then reads out.  A purely decorative graphic is marked `artifact` instead,
and `actualtext=` stands for a graphic that shows text.

The rule reads the keys of the call as written. A call whose options come
from a macro is left alone.  There is no automatic fix: only the author knows
what the graphic shows. The image dialog has a field for it.",
        references: &[
            Ref { source: Source::LatexLab, at: "latex-lab-graphic" },
            Ref { source: Source::PdfUa, at: "13" },
        ],
        fixes: &[],
        engines: &[],
        run: graphics_without_alt,
    },
    Rule {
        code: "pdfua-without-title",
        since: "0.2.5",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "PDF/UA without a title",
        explanation: "\
PDF/UA requires a document title in the XMP metadata (`dc:title`), shown by
the viewer in place of the file name.  pdfmanagement writes it from the
document property `hyperref/pdftitle`.  With tagging on, latex-lab's title
code fills that property from `\\title{…}`. Without it, only
`\\hypersetup{pdftitle={…}}` does.  The run ended with the property empty.

Fix with `\\title{…}` under tagging, or `\\hypersetup{pdftitle={…}}`.  Where
`\\title` is there but latex-lab's title code did not take it, the fix writes
`\\title[pdftitle={…}]{…}` from the title as the run rendered it.",
        references: &[Ref { source: Source::PdfUa, at: "06" }, Ref { source: Source::LatexLab, at: "latex-lab-title" }],
        fixes: &[Applicability::Safe],
        engines: &[],
        run: pdfua_without_title,
    },
    Rule {
        code: "tagging-without-language",
        since: "0.2.5",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "tagging without a language",
        explanation: "\
A tagged PDF names its natural language in the catalog's `/Lang`, which
screen readers use to pick a voice (PDF/UA, ISO 14289-1 § 7.2).
`\\DocumentMetadata{lang=…}` sets it.  Without the key, LaTeX guesses at
`\\begin{document}` from babel's main language, or `en` without babel, and
warns (`lang-missing`).  The guess is wrong for a document in another
language that does not load babel.

The fix writes the language LaTeX guessed, which changes nothing in the PDF
and silences the warning: check that it is the right one.",
        references: &[
            Ref { source: Source::PdfUa, at: "11" },
            Ref { source: Source::LatexLab, at: "documentmetadata-support" },
        ],
        fixes: &[Applicability::Safe],
        engines: &[],
        run: tagging_without_language,
    },
    Rule {
        code: "tagging-unsupported-engine",
        since: "0.2.5",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "engine does not support tagging",
        explanation: "\
The tagging code supports pdfLaTeX and LuaLaTeX.  Under XeLaTeX (and other
DVI-based routes) tagpdf cannot mark interword spaces as real space
characters, so text extracted from the PDF runs together, and the structure
code is not tested there.

Fix by compiling with LuaLaTeX, which the tagging project recommends, or pdfLaTeX.",
        references: &[Ref { source: Source::TaggingProject, at: "usage-instructions" }],
        fixes: &[],
        engines: &[Engine::XeTeX, Engine::Tex],
        run: unsupported_engine,
    },
    Rule {
        code: "tagged-math-without-mathml",
        since: "0.2.5",
        category: Category::Style,
        severity: Severity::Info,
        summary: "formulas without MathML",
        explanation: "\
\
The tagging code attaches MathML to each formula. It reads it from
`<jobname>-mathml.html` or `<jobname>-luamml-mathml.html`. Under LuaLaTeX
luamml writes the second one, but only when unicode-math is loaded
(latex-lab warns `unicode-math-missing` otherwise). pdfLaTeX cannot write
either. The rule fires when the run looked for these files, found none, and
wrote none. The formulas are then tagged with their LaTeX source as
alternative text, which PDF/UA-2 accepts less well than MathML.

Fix by compiling with LuaLaTeX and loading unicode-math, or by making the
MathML file once with such a run.",
        references: &[Ref { source: Source::LatexLab, at: "latex-lab-math" }],
        fixes: &[],
        engines: &[],
        run: math_without_mathml,
    },
];

/// The `\documentclass` the main file wrote.
fn class_call(report: &Report) -> Option<Span> {
    let a = report.analysis();
    a.facts
        .loads
        .iter()
        .find(|l| l.kind == LoadKind::Class && l.depth == 0 && l.span.file == a.main_file)
        .map(|l| l.span)
}

fn metadata_after_class(report: &mut Report) {
    let a = report.analysis();
    let (Some(class), Some(sym)) = (class_call(report), a.interner.lookup("DocumentMetadata")) else { return };
    let after = |s: &Span| s.file == class.file && (s.line, s.col) > (class.line, class.col);
    // the document's own calls: the kernel's `\DocumentMetadata` calls itself again once loaded
    let calls: Vec<Span> =
        a.facts.expansions.iter().filter(|e| e.name == sym && e.span.file == class.file).map(|e| e.span).collect();
    // a call before the class set it up: later calls only set keys, which LaTeX allows
    if calls.iter().any(|s| !after(s)) {
        return;
    }
    for span in calls {
        let edits = move_before(report, span, class);
        let fix = Fix::new("move `\\DocumentMetadata{…}` above `\\documentclass`", Applicability::Unsafe, edits);
        let message = "`\\DocumentMetadata` after `\\documentclass` takes no effect: nothing is tagged".to_string();
        report.add_fix(span, "DocumentMetadata", message, fix);
    }
}

/// The call at `span`, alone on its lines, moved to the line `to` starts; `None` when other text
/// shares its lines.
fn move_before(report: &Report, span: Span, to: Span) -> Option<Vec<Edit>> {
    let edits = report.edits();
    let (path, text, mut scan, _) = edits.command(span)?;
    let end = scan.group()?.end;
    let lead = &text.line(span.line)[..crate::engine::source::byte_of_col(text.line(span.line), span.col)];
    let tail = text.columns(end.line).into_iter().skip(end.col as usize - 1).collect::<String>();
    if !lead.trim().is_empty() || !tail.trim().is_empty() {
        return None;
    }
    let call = text.slice(Pos::of(span), end)?.to_string();
    let mut out = edits.insert_before(to, &call)?;
    let (from, until) = (Pos::new(span.line, 1), Pos::new(end.line + 1, 1));
    out.push(Edit { path, start: from, end: until, replacement: String::new() });
    Some(out)
}

fn incompatible_packages(report: &mut Report) {
    let Some(t) = tagging::of(report.analysis()).filter(|t| t.active()) else { return };
    let a = report.analysis();
    for s in t.statuses.iter().filter(|s| s.incompatible()) {
        let load = &a.facts.loads[s.load];
        // a load another package made is reported at the document's call that led to it
        let span = load.call.filter(|_| load.depth > 0).unwrap_or(load.span);
        let by = (load.depth > 0).then(|| load.by.map(|b| format!(" (loaded by `{}`)", a.interner.name(b))));
        let message = format!("`{}` is {} with tagging{}", s.file, s.meaning(), by.flatten().unwrap_or_default());
        let fix = "replace it, see https://latex3.github.io/tagging-project/tagging-status/".to_string();
        report.add(span, &load.name, message, Some(fix));
    }
}

/// Whether latex-lab tags a graphic with these keys without warning `alt-text-missing`:
/// its keys in order, the last mode key winning (latex-lab-testphase-graphic.sty, keys `Gin`
/// and `tag/graphic`). Mode `alt` (the default, and `tag=⟨struct⟩`) warns when `alt` is empty.
/// `None` when a key name is not plain text.
fn tagged(entries: &[(String, String)]) -> Option<bool> {
    // (mode is `alt`, `alt` is not empty, suspended)
    fn fold(entries: &[(String, String)], state: &mut (bool, bool, bool)) -> Option<()> {
        for (name, value) in entries {
            match (name.as_str(), value.as_str()) {
                ("alt", v) => (state.0, state.1) = (true, !v.is_empty()),
                ("artifact" | "actualtext", _) | ("tag", "artifact") => state.0 = false,
                // `\tag_suspend:n`: the graphic is not tagged at all
                ("tag", "false") => state.2 = true,
                ("tag", _) => state.0 = true,
                ("tagging-setup", v) => fold(&entries_of(v)?, state)?,
                _ => {}
            }
        }
        Some(())
    }
    let mut state = (true, false, false);
    fold(entries, &mut state)?;
    Some(!state.0 || state.1 || state.2)
}

fn graphics_without_alt(report: &mut Report) {
    let Some(_) = tagging::of(report.analysis()).filter(|t| t.active()) else { return };
    let a = report.analysis();
    // graphicx takes `alt` (latex-lab defines it on load, recent graphicx ignores it untagged)
    if !a.defined("KV@Gin@alt") {
        return;
    }
    let mut seen: Vec<Span> = Vec::new();
    for o in a.facts.occurrences.iter().filter(|o| o.kind == OccKind::Graphics) {
        if seen.contains(&o.span) || !report.wanted(o.span) {
            continue;
        }
        seen.push(o.span);
        let Some(entries) = graphic_keys(report, o.span) else { continue };
        if tagged(&entries) != Some(false) {
            continue;
        }
        let message = format!("`{}` has no alternative text: tagging reads out its file name", o.key);
        let fix = "add `alt={…}` describing it, or `artifact` if it is decoration".to_string();
        report.add(o.span, &o.key, message, Some(fix));
    }
}

/// The keys and values of the `\includegraphics` written at `span`, empty without options;
/// `None` where something else stands there, or a key name is not plain text (a macro of options).
pub(crate) fn graphic_keys(report: &Report, span: Span) -> Option<Vec<(String, String)>> {
    let edits = report.edits();
    let (_, text, mut scan, cs) = edits.command(span)?;
    if scan.name(&cs)? != "includegraphics" {
        return None;
    }
    scan.star();
    let Some(opt) = scan.optional() else { return Some(Vec::new()) };
    entries_of(text.slice(opt.inner_start, opt.inner_end)?)
}

/// A key list's entries as (name, value), the value trimmed and one brace pair stripped (keyval
/// does both); `None` when a name is not plain text.
fn entries_of(body: &str) -> Option<Vec<(String, String)>> {
    let mut out = Vec::new();
    for entry in split_top(body) {
        let (name, value) = entry.split_once('=').unwrap_or((entry, ""));
        let name = name.trim();
        if name.contains(['\\', '{', '}', '%']) {
            return None;
        }
        let value = value.trim();
        let value = value.strip_prefix('{').and_then(|v| v.strip_suffix('}')).unwrap_or(value);
        if !name.is_empty() {
            out.push((name.to_string(), value.to_string()));
        }
    }
    Some(out)
}

/// The entries of a key list, split at commas outside braces.
fn split_top(body: &str) -> Vec<&str> {
    let (mut depth, mut from, mut out) = (0i32, 0, Vec::new());
    for (i, c) in body.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                out.push(&body[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    out.push(&body[from..]);
    out
}

fn pdfua_without_title(report: &mut Report) {
    let a = report.analysis();
    let Some(t) = tagging::of(a) else { return };
    let Some(ua) = t.ua() else { return };
    if t.title != Val::Empty {
        return;
    }
    let Some(at) = metadata_call(report) else { return };
    // `\title` in latex-lab's title code takes `[pdftitle=…]`: the fix writes the title there
    let title = a.metadata.iter().find(|m| m.field == "title");
    let call = title.and_then(|_| title_call(report));
    let edits = match (call, title) {
        (Some(span), Some(m)) if crate::analysis::machine::loaded(a, "latex-lab-testphase-title") => {
            title_edit(report, span, &m.text)
        }
        _ => None,
    };
    let message = match title {
        Some(_) => format!("{ua} needs a document title, and the PDF gets none: the `\\title` did not reach it"),
        None => format!("{ua} needs a document title, and the PDF gets none: the document has no `\\title`"),
    };
    let description = match (edits.is_some(), title.is_some()) {
        (true, _) => "write the title as `\\title[pdftitle={…}]`",
        (false, false) if t.active() => "add `\\title{…}`, which tagging writes to the PDF",
        (false, _) => "set it with `\\hypersetup{pdftitle={…}}`",
    };
    let applicability = if edits.is_some() { Applicability::Safe } else { Applicability::Unsafe };
    report.add_fix(at, "DocumentMetadata", message, Fix::new(description, applicability, edits));
}

/// The `\title{…}` the main file wrote, without options.
fn title_call(report: &Report) -> Option<Span> {
    let a = report.analysis();
    let sym = a.interner.lookup("title")?;
    a.facts.expansions.iter().find(|e| e.name == sym && e.span.file == a.main_file).map(|e| e.span)
}

/// `\title{X}` written `\title[pdftitle={X}]{X}`: `X` as the run rendered it, braced; `None`
/// when the call has options already.
fn title_edit(report: &Report, span: Span, title: &str) -> Option<Vec<Edit>> {
    let edits = report.edits();
    let (path, _, mut scan, cs) = edits.command(span)?;
    if scan.name(&cs)? != "title" || scan.optional().is_some() || title.contains(['{', '}', '\\', '%', '#']) {
        return None;
    }
    let at = cs.end;
    Some(vec![Edit { path, start: at, end: at, replacement: format!("[pdftitle={{{title}}}]") }])
}

/// Where the document called `\DocumentMetadata`.
fn metadata_call(report: &Report) -> Option<Span> {
    let a = report.analysis();
    let sym = a.interner.lookup("DocumentMetadata")?;
    a.facts.expansions.iter().find(|e| e.name == sym && report.wanted(e.span)).map(|e| e.span)
}

fn tagging_without_language(report: &mut Report) {
    let a = report.analysis();
    let Some(t) = tagging::of(a).filter(|t| t.active()) else { return };
    // the document set none by `\begin{document}`; what LaTeX then guessed, if the run knows
    if t.lang_set != Val::Empty || a.preamble.is_none() {
        return;
    }
    let Some(at) = metadata_call(report) else { return };
    let guessed = t.lang.text().filter(|l| !l.contains(['{', '}', ',', '=']));
    let message = match guessed {
        Some(l) => {
            format!("no `lang` key: LaTeX guesses `{l}` for the PDF's language, which screen readers read it in")
        }
        None => {
            "no `lang` key: LaTeX guesses the PDF's language at `\\begin{document}`, which screen readers read it in"
                .to_string()
        }
    };
    let fix = match guessed {
        // writing the guess changes nothing in the PDF and says it in the source
        Some(l) => {
            Fix::new(format!("write `lang={l}`"), Applicability::Safe, add_key(report, at, &format!("lang={l}")))
        }
        None => Fix::new("add `lang=…` (`en-US`, `de-DE`) to `\\DocumentMetadata`", Applicability::Unsafe, None),
    };
    report.add_fix(at, "DocumentMetadata", message, fix);
}

/// `item` added to the end of the key list of the call at `span`.
fn add_key(report: &Report, span: Span, item: &str) -> Option<Vec<Edit>> {
    let edits = report.edits();
    let (path, text, mut scan, _) = edits.command(span)?;
    let group = scan.group()?;
    let inner = text.slice(group.inner_start, group.inner_end)?;
    let sep = if inner.trim().is_empty() {
        ""
    } else if inner.trim_end().ends_with(',') {
        " "
    } else {
        ", "
    };
    let end = text.pos(text.offset(group.inner_start)? + inner.trim_end().len());
    Some(vec![Edit { path, start: end, end, replacement: format!("{sep}{item}") }])
}

fn unsupported_engine(report: &mut Report) {
    let a = report.analysis();
    if !tagging::of(a).is_some_and(|t| t.active()) {
        return;
    }
    if matches!(a.plugins.engine, Engine::PdfTeX | Engine::LuaTeX) {
        return;
    }
    let Some(at) = metadata_call(report) else { return };
    let message = format!("tagging is not supported under {}: spaces are not marked", a.plugins.program);
    report.add(at, "DocumentMetadata", message, Some("compile with LuaLaTeX or pdfLaTeX".into()));
}

fn math_without_mathml(report: &mut Report) {
    let a = report.analysis();
    let Some(t) = tagging::of(a).filter(|t| t.active()) else { return };
    if t.ua() != Some("UA-2") || a.facts.text.math.is_empty() {
        return;
    }
    // latex-lab-math.ltx reads `<job>-mathml` and `<job>-luamml-mathml`, and luamml `\iow_open`s
    // the second: the formulas get MathML when one was found or this run writes one
    let mathml = |f: &&String| f.ends_with("-mathml.html") || f.ends_with("-mathml");
    if a.facts.written_files.iter().any(|f| mathml(&f)) || a.facts.found.iter().any(|f| mathml(&f)) {
        return;
    }
    let Some(missing) = a.facts.absent.iter().find(mathml) else { return };
    let Some(at) = metadata_call(report) else { return };
    let message = format!("formulas are tagged without MathML: `{missing}` does not exist and this run writes none");
    let fix = match a.plugins.engine {
        Engine::LuaTeX => "load `unicode-math`, then luamml writes the MathML file",
        _ => "compile with LuaLaTeX and `unicode-math`, or make the MathML file with one such run",
    };
    report.add(at, "DocumentMetadata", message, Some(fix.into()));
}
