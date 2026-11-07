//! The table and image dialogs: where they open (a code lens, a code
//! action, a hover link on every table and figure the run met), what they
//! show (`satex/dialog`) and the text they write (`satex/dialogApply`).
//! The run says what is a table: an environment whose code reaches
//! `\@array`; and what is a figure: the files graphicx read.

use lsp_types::{CodeLens, Command, Position, Range};
use serde_json::{Value as Json, json};

use super::handlers::{to_lsp, to_pos};
use super::snapshot::{self, Arg, Key, Package};
use super::{graphic, tabular};
use crate::analysis::machine::Analysis;
use crate::engine::builtins::OccKind;
use crate::text::{Pos, Text};

/// The command a client opens a dialog with: `{uri, position}`.
pub const OPEN: &str = "satex.openDialog";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Table,
    Image,
    Float,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Kind::Table => "table",
            Kind::Image => "image",
            Kind::Float => "float",
        }
    }
}

/// A table or figure in the text: its kind, byte extent, environment name or resolved file.
struct Target {
    kind: Kind,
    start: usize,
    end: usize,
    detail: Option<String>,
}

/// Every table and figure of the file at `path` the run met.
fn targets(analysis: &Analysis, doc: &Text, path: &str) -> Vec<Target> {
    let file = super::type_at::file_of(analysis, path);
    let mine = |i: u16| file == Some(i);
    let cfg = &analysis.settings.type_plugins;
    let text = doc.body();
    let at = |line: u32, col: u32| doc.offset(Pos::new(line, col)).or_else(|| doc.offset(Pos::new(line, 1)));
    let mut out: Vec<Target> = Vec::new();
    let mut tables: Vec<String> = Vec::new();
    for (begin, _, name) in crate::queries::query::environments(analysis) {
        if !mine(begin.file) {
            continue;
        }
        // a float: the `\begin{…}[placement]` of an environment whose code reaches `\@float`
        if cfg.float.enabled && super::handlers::material(analysis, &name) == Some(FLOAT) {
            if let Some(start) = at(begin.line, begin.col).and_then(|o| tabular::extent(text, o, &name)).map(|(s, _)| s)
            {
                let head = start + format!("\\begin{{{name}}}").len();
                out.push(Target {
                    kind: Kind::Float,
                    start,
                    end: head + placement(&text[head..]).map_or(0, |(_, n)| n),
                    detail: Some(name),
                });
            }
            continue;
        }
        if !cfg.table.enabled {
            continue;
        }
        let table = match tables.contains(&name) {
            true => true,
            false if tabular::is_table(analysis, &name) => {
                tables.push(name.clone());
                true
            }
            false => false,
        };
        let Some((start, end)) =
            table.then(|| at(begin.line, begin.col)).flatten().and_then(|o| tabular::extent(text, o, &name))
        else {
            continue;
        };
        out.push(Target { kind: Kind::Table, start, end, detail: Some(name) });
    }
    // a figure is a call naming a file the run's graphics read: the driver reads a file once, so
    // a second call of the same name is found by its name, not by an occurrence of its own
    let read: Vec<(&str, Option<&String>)> = (analysis.facts.occurrences.iter())
        .filter(|o| cfg.image.enabled && o.kind == OccKind::Graphics && mine(o.span.file))
        .map(|o| (o.key.as_str(), o.detail.as_ref()))
        .collect();
    let cmd = "\\includegraphics";
    for (start, _) in text.match_indices(cmd).filter(|_| !read.is_empty()) {
        let line = text[..start].rfind('\n').map_or(0, |n| n + 1);
        if super::handlers::uncommented(&text[line..start]).len() < start - line {
            continue;
        }
        let Some((img, len)) = graphic::parse(&text[start..], None) else { continue };
        let name = img.file.trim();
        let Some((_, detail)) =
            read.iter().find(|(k, _)| *k == name || k.rsplit_once('.').is_some_and(|(stem, _)| stem == name))
        else {
            continue;
        };
        out.push(Target { kind: Kind::Image, start, end: start + len, detail: detail.cloned() });
    }
    out.sort_by_key(|t| (t.start, t.end));
    out.dedup_by_key(|t| t.start);
    out
}

fn lsp_range(doc: &Text, start: usize, end: usize) -> Range {
    Range::new(to_lsp(doc, doc.pos(start)), to_lsp(doc, doc.pos(end)))
}

fn open_command(kind: Kind, uri: &str, at: Position) -> Command {
    let title = match kind {
        Kind::Table => "Edit table…",
        Kind::Image => "Edit image…",
        Kind::Float => "Edit placement…",
    };
    Command { title: title.into(), command: OPEN.into(), arguments: Some(vec![json!({ "uri": uri, "position": at })]) }
}

/// A code lens above each table or figure of `kind`: the dialog's entry.
pub(super) fn lenses(analysis: &Analysis, doc: &Text, path: &str, kind: Kind) -> Vec<CodeLens> {
    let uri = super::handlers::path_uri(path);
    let uri = uri.as_str();
    targets(analysis, doc, path)
        .into_iter()
        .filter(|t| t.kind == kind)
        .map(|t| {
            let range = lsp_range(doc, t.start, t.start);
            CodeLens { range, command: Some(open_command(t.kind, uri, range.start)), data: None }
        })
        .collect()
}

/// The innermost table or figure around `pos`: one ending at `pos` is left, so the tabular right after
/// `\begin{table}[h]` is the one its lens opens.
fn target_at(analysis: &Analysis, doc: &Text, path: &str, pos: Position) -> Option<Target> {
    let at = doc.offset(to_pos(doc, pos))?;
    targets(analysis, doc, path).into_iter().filter(|t| t.start <= at && at < t.end).min_by_key(|t| t.end - t.start)
}

/// A hover's link to the dialog, on the first line of a table or figure.
pub fn hover_link(analysis: &Analysis, doc: &Text, path: &str, uri: &str, pos: Position) -> Option<String> {
    let t = target_at(analysis, doc, path, pos)?;
    let first = to_lsp(doc, doc.pos(t.start)).line;
    (first == pos.line).then(|| {
        let c = open_command(t.kind, uri, to_lsp(doc, doc.pos(t.start)));
        let args = c.arguments.unwrap_or_default();
        let q = percent(&serde_json::to_string(&args).unwrap_or_default());
        let link = format!("[{}](command:{OPEN}?{q} \"Open the {} dialog\")", c.title, t.kind.name());
        match t.kind {
            Kind::Float => format!("{}\n\n{link}", float_card(analysis, doc, uri, &t)),
            _ => link,
        }
    })
}

/// [`super::handlers::material`]'s index of a float.
const FLOAT: usize = 3;

/// The placement `[…]` at the start of `s` (spaces before it allowed) and the bytes up to its `]`.
fn placement(s: &str) -> Option<(String, usize)> {
    let lead = s.len() - s.trim_start().len();
    let rest = s[lead..].strip_prefix('[')?;
    let close = rest.find(']')?;
    Some((rest[..close].to_string(), lead + close + 2))
}

/// The placement letters `\@xfloat` tests (latex.ltx), with what each lets LaTeX do; `H` when the
/// run defined float.sty's `\@float@HH`. A seed table: the letters' meaning is prose, not run data.
fn letters(analysis: &Analysis) -> Vec<(char, &'static str)> {
    let mut out = vec![
        ('h', "here, where it stands, if it fits there (a lone `h` becomes `ht`)"),
        ('t', "at the top of a page, at most `topnumber` floats taking at most `\\topfraction` of it"),
        ('b', "at the bottom of a page, at most `bottomnumber` floats taking at most `\\bottomfraction`"),
        ('p', "on a page of floats only, which they fill to at least `\\floatpagefraction`"),
        ('!', "ignore the limits above for this float, once"),
    ];
    if analysis
        .interner
        .lookup("@float@HH")
        .is_some_and(|s| !matches!(analysis.env.meaning(s), crate::engine::tex::Meaning::Undefined))
    {
        out.push(('H', "exactly here, never floating (float package). No other letter with it"));
    }
    out
}

/// Why the letters' order is lost: `\@xfloat` (latex.ltx) adds one bit per letter to the float's count.
const ORDER: &str = "The order of the letters does not matter: `\\@xfloat` (latex.ltx, source2e) turns each into a bit, and LaTeX always tries here, top, bottom, then a float page.";

fn letters_json(analysis: &Analysis) -> Json {
    json!(letters(analysis).iter().map(|(c, what)| json!([c.to_string(), what])).collect::<Vec<_>>())
}

/// The placement card, laid out like a length's: a toggle per letter, each a link that writes the
/// brackets with it switched, what each does, and the limits `\@xfloat` weighs as the run ended.
fn float_card(analysis: &Analysis, doc: &Text, uri: &str, t: &Target) -> String {
    let env = t.detail.as_deref().unwrap_or("float");
    let head = t.start + format!("\\begin{{{env}}}").len();
    let written = placement(&doc.body()[head..t.end]);
    let spec = written.as_ref().map(|(p, _)| p.trim().to_string()).filter(|s| !s.is_empty() && s != "!");
    let default = crate::linter::shared::macro_text(analysis, &format!("fps@{env}"));
    let shown = spec.clone().or(default.clone()).unwrap_or_default();
    // the brackets, or the place they go
    let rest = &doc.body()[head..];
    let from = if written.is_some() { head + rest.len() - rest.trim_start().len() } else { t.end };
    let range = lsp_range(doc, from, t.end);
    let known = letters(analysis);
    let mut md = match (&spec, &default) {
        (Some(s), _) => format!("### Placement `[{s}]`: LaTeX may put this {env}\n\n"),
        (_, Some(d)) => format!("### Placement: this {env} takes the default `{d}` (`\\fps@{env}`)\n\n"),
        _ => format!("### Placement: LaTeX may put this {env}\n\n"),
    };
    // one line per letter: its checkmark switches it (a link that rewrites the brackets), then what it means
    for (c, what) in &known {
        let on = shown.contains(*c);
        let next = toggled(&shown, *c);
        let text = if next.is_empty() { String::new() } else { format!("[{next}]") };
        let args = json!([uri, range, text, null]);
        let (mark, state) = if on { ("✓", "**on**") } else { ("○", "off") };
        md.push_str(&format!(
            "- [{mark} `{c}`](command:satex.applyEdit?{}) {state}: {what}\n",
            percent(&args.to_string())
        ));
    }
    for c in shown.chars().filter(|c| !known.iter().any(|(k, _)| k == c)) {
        md.push_str(&format!("- `{c}`: unknown: LaTeX reports an error and uses `p`\n"));
    }
    md.push_str(&format!("\n{ORDER}\n"));
    if let Some(limits) = limits(analysis) {
        md.push_str(&format!("\n{limits}"));
    }
    md
}

/// `spec` with `c` switched: `H` stands alone (float.sty).
fn toggled(spec: &str, c: char) -> String {
    match (spec.contains(c), c) {
        (true, _) => spec.replace(c, ""),
        (false, 'H') => "H".into(),
        (false, _) => format!("{}{c}", spec.replace('H', "")),
    }
}

/// The counters and fractions `\@xfloat` and the output routine weigh, as the run ended (latex.ltx).
fn limits(analysis: &Analysis) -> Option<String> {
    let count = |n: &str| {
        super::plugins::register_value(analysis, &format!("c@{n}")).filter(|v| v.lo == v.hi).map(|v| v.lo.to_string())
    };
    let frac = |n: &str| crate::linter::shared::macro_text(analysis, n);
    let cells: Vec<(String, String)> = [
        ("topnumber", count("topnumber")),
        ("\\topfraction", frac("topfraction")),
        ("bottomnumber", count("bottomnumber")),
        ("\\bottomfraction", frac("bottomfraction")),
        ("totalnumber", count("totalnumber")),
        ("\\textfraction", frac("textfraction")),
        ("\\floatpagefraction", frac("floatpagefraction")),
    ]
    .into_iter()
    .filter_map(|(n, v)| Some((format!("`{n}`"), v?)))
    .collect();
    if cells.is_empty() {
        return None;
    }
    let line: Vec<String> = cells.iter().map(|(n, v)| format!("{n} {}", v.trim())).collect();
    Some(format!("Limits as the run ended: {}\n", line.join(" · ")))
}

/// The placement letters to complete inside a float's `[…]`, its `]` typed or not: those not yet written,
/// inserted at the cursor.
pub fn float_completion(
    analysis: &Analysis,
    doc: &Text,
    path: &str,
    pos: Pos,
) -> Option<Vec<lsp_types::CompletionItem>> {
    let at = doc.offset(pos)?;
    let body = doc.body();
    let t = targets(analysis, doc, path).into_iter().rev().find(|t| t.kind == Kind::Float && t.start <= at)?;
    let head = t.start + format!("\\begin{{{}}}", t.detail.as_deref()?).len();
    let open = head + body[head..].find('[').filter(|&k| body[head..head + k].trim().is_empty())? + 1;
    let known = letters(analysis);
    let written = body[open..].split(|c: char| !known.iter().any(|(k, _)| *k == c)).next().unwrap_or_default();
    if at < open || at > open + written.len() {
        return None;
    }
    let range = lsp_range(doc, at, at);
    Some(
        known
            .into_iter()
            .filter(|(c, _)| !written.contains(*c))
            .enumerate()
            .map(|(k, (c, what))| lsp_types::CompletionItem {
                label: c.to_string(),
                kind: Some(lsp_types::CompletionItemKind::ENUM_MEMBER),
                detail: Some(what.replace('`', "")),
                documentation: Some(lsp_types::Documentation::MarkupContent(lsp_types::MarkupContent {
                    kind: lsp_types::MarkupKind::Markdown,
                    value: format!("`{c}`: {what}\n\n{ORDER}"),
                })),
                sort_text: Some(format!("{k}")),
                text_edit: Some(lsp_types::CompletionTextEdit::Edit(lsp_types::TextEdit::new(range, c.to_string()))),
                ..Default::default()
            })
            .collect(),
    )
}

fn percent(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// What the packages loaded allow: booktabs' rules, siunitx's, tabularx's and array's columns, multirow.
fn context(analysis: &Analysis, doc: &Text) -> Json {
    let mut ctx: serde_json::Map<String, Json> =
        PACKAGES.iter().map(|p| (p.to_string(), json!(crate::linter::shared::loaded(analysis, p)))).collect();
    ctx.insert("preamble".into(), json!(doc.body().contains("\\begin{document}")));
    Json::Object(ctx)
}

/// The packages a table dialog offers features of.
const PACKAGES: [&str; 5] = ["booktabs", "siunitx", "tabularx", "array", "multirow"];

/// The figure's natural size in bp, as the engine reads the file.
fn natural(file: Option<&str>, page: i64) -> Option<[f64; 2]> {
    let path = std::path::PathBuf::from(file?);
    let bytes = crate::project::overlay::read_bytes(&path).ok()?;
    let pdf = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    let (w, h) = match pdf {
        true => crate::engine::pdfbox::page_box(&bytes, "crop", page)?,
        false => crate::engine::pdfbox::raster_size(&bytes, 72)?,
    };
    let bp = |sp: i64| sp as f64 / 65536.0 * 72.0 / 72.27;
    Some([bp(w), bp(h)])
}

fn page(img: &graphic::Image) -> i64 {
    img.keys.iter().find(|(k, _)| k == "page").and_then(|(_, v)| v.trim().parse().ok()).unwrap_or(1)
}

/// The dialog of the table or figure at `pos`: `{kind, range, state, context}`.
pub fn dialog(analysis: &Analysis, doc: &Text, path: &str, pos: Position) -> Option<Json> {
    let t = target_at(analysis, doc, path, pos)?;
    let text = &doc.body()[t.start..t.end];
    let state = match t.kind {
        Kind::Table => serde_json::to_value(tabular::parse(text)?).ok()?,
        Kind::Image => {
            let (img, _) = graphic::parse(text, None)?;
            let natural = natural(t.detail.as_deref(), page(&img));
            serde_json::to_value(graphic::parse(text, natural)?.0).ok()?
        }
        Kind::Float => {
            let env = t.detail.clone()?;
            let head = format!("\\begin{{{env}}}").len();
            json!({ "env": env, "spec": placement(&text[head..]).map(|(p, _)| p) })
        }
    };
    let mut ctx = context(analysis, doc);
    if t.kind == Kind::Image {
        ctx["path"] = json!(t.detail);
        // the graphics driver the run loaded, and whether it reads `decodearray` (it sets `\Gin@decode`)
        ctx["driver"] = json!(crate::linter::shared::macro_text(analysis, "Gin@driver"));
        ctx["decode"] = json!(crate::linter::shared::macro_text(analysis, "Gin@decode").is_some());
    }
    if t.kind == Kind::Float {
        let env = t.detail.as_deref().unwrap_or_default();
        ctx["letters"] = letters_json(analysis);
        ctx["order"] = json!(ORDER);
        ctx["default"] = json!(crate::linter::shared::macro_text(analysis, &format!("fps@{env}")));
    }
    Some(json!({ "kind": t.kind.name(), "range": lsp_range(doc, t.start, t.end), "state": state, "context": ctx }))
}

/// The package a table's columns need that the run lacks: a letter (or `>`, `<`) neither its
/// `\@testpach` tests nor a `\newcolumntype` made; the column's hint, else the installation's
/// provider of `\@testpach`.
fn columns(analysis: &Analysis, t: &tabular::Table) -> Option<Package> {
    let pach = analysis.interner.lookup("@testpach").map(|s| match analysis.env.meaning(s) {
        crate::engine::tex::Meaning::Macro(m) => {
            crate::engine::tex::detokenize(&m.replacement_text, &analysis.interner)
        }
        _ => String::new(),
    });
    let tested = |c: char| {
        [format!("#1{c}"), format!("\\@nextchar {c}")].iter().any(|t| pach.as_ref().is_some_and(|p| p.contains(t)))
    };
    let lacks = |c: char| !tested(c) && !snapshot::defined(analysis, &format!("NC@find@{c}"));
    let wants = |c: &&tabular::Col| {
        c.align.chars().next().is_some_and(lacks)
            || (!c.pre.is_empty() && lacks('>'))
            || (!c.post.is_empty() && lacks('<'))
    };
    t.cols.iter().find(wants)?;
    let name = (t.cols.iter().filter(wants).find_map(|c| c.package.clone()))
        .or_else(|| crate::linter::shared::provider(analysis, "@testpach"))?;
    Some(Package { name, options: String::new() })
}

/// The edits a dialog's `state` makes of the text in `range`, through the snapshot printer:
/// `{edits, notes, text}` (`text`: the range as written then); with `op` (`booktabs`, `siunitx`)
/// the table rewritten first, and its new `state`.
pub fn apply(
    analysis: &Analysis,
    doc: &Text,
    kind: &str,
    range: Range,
    state: &Json,
    op: Option<&str>,
) -> Result<Json, String> {
    let (start, end) = (
        doc.offset(to_pos(doc, range.start)).ok_or("range outside the text")?,
        doc.offset(to_pos(doc, range.end)).ok_or("range outside the text")?,
    );
    let written = |keyed: &[usize], options| snapshot::read(doc, start, keyed, options).ok_or("no call here");
    let mut notes = Vec::new();
    let (node, table) = match kind {
        "table" => {
            let mut t: tabular::Table = serde_json::from_value(state.clone()).map_err(|e| e.to_string())?;
            match op {
                Some("booktabs") => tabular::booktabs(&mut t),
                Some("siunitx") if !tabular::siunitx(&mut t) => return Err("no column holds only numbers".into()),
                _ => {}
            }
            // an S column's text cells braced, however the column became S
            tabular::protect_s(&mut t);
            let switched = tabular::to_tabularx(&mut t);
            if switched {
                notes.push("X columns: written as tabularx, \\linewidth wide".to_string());
            }
            let mut n = snapshot::retarget(&written(&[], false)?, &tabular::emit(&t)).ok_or("no table here")?;
            n.package = columns(analysis, &t);
            (n, (switched || op.is_some()).then_some(t))
        }
        "image" => {
            let img: graphic::Image = serde_json::from_value(state.clone()).map_err(|e| e.to_string())?;
            let mut n = written(&[0], true)?;
            let was = n.args[0].keys.clone().unwrap_or_default();
            // a key written `key=` stays so while its value stays empty
            let set = |(key, v): (String, String)| {
                let value = (!v.is_empty() || was.iter().any(|w| w.key == key && w.value.is_some())).then_some(v);
                Key { key, value }
            };
            n.args[0].keys = Some(graphic::options(&img).into_iter().map(set).collect());
            if let Some(file) = n.args.iter_mut().find(|a| a.open == '{') {
                file.text = img.file;
            }
            (n, None)
        }
        "float" => {
            let spec = state["spec"].as_str().unwrap_or_default().trim();
            let known: Vec<char> = letters(analysis).iter().map(|(c, _)| *c).collect();
            if let Some(c) = spec.chars().find(|c| !known.contains(c)) {
                return Err(format!("`{c}` is no placement here: {}", known.iter().collect::<String>()));
            }
            let mut n = written(&[], false)?;
            match n.args.first_mut().filter(|a| a.open == '[') {
                Some(a) => a.text = spec.into(),
                None => n.args.insert(0, Arg::text('[', spec)),
            }
            (n, None)
        }
        k => return Err(format!("no dialog `{k}`")),
    };
    let (edits, more) = snapshot::printed(analysis, doc, &[node])?;
    notes.extend(more);
    let text = snapshot::reads(doc.body(), start, end, &edits);
    let edits = snapshot::lsp_edits(doc, edits);
    let state = table.map(|t| serde_json::to_value(t).unwrap_or_default());
    Ok(json!({ "edits": edits, "notes": notes, "state": state, "text": text }))
}

/// Code actions on the tables and figures `range` touches: open the dialog,
/// and what a client without dialogs can do at once.
pub fn actions(analysis: &Analysis, doc: &Text, path: &str, uri: &str, range: Range) -> Vec<Json> {
    let (Some(a), Some(b)) = (doc.offset(to_pos(doc, range.start)), doc.offset(to_pos(doc, range.end))) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for t in targets(analysis, doc, path).into_iter().filter(|t| t.start <= b && a <= t.end) {
        let at = to_lsp(doc, doc.pos(t.start));
        let cmd = open_command(t.kind, uri, at);
        out.push(json!({ "title": cmd.title, "kind": "refactor", "command": cmd }));
        let text = &doc.body()[t.start..t.end];
        let r = lsp_range(doc, t.start, t.end);
        let mut edit = |title: &str, state: Json, kind: &str| {
            if let Ok(e) = apply(analysis, doc, kind, r, &state, None) {
                let notes: Vec<String> = e["notes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|n| n.as_str().map(str::to_string))
                    .collect();
                let title = match notes.is_empty() {
                    true => title.to_string(),
                    false => format!("{title} ({})", notes.join(", ")),
                };
                out.push(
                    json!({ "title": title, "kind": "refactor.rewrite", "edit": { "changes": { uri: e["edits"] } } }),
                );
            }
        };
        match t.kind {
            Kind::Table => {
                let Some(table) = tabular::parse(text) else { continue };
                let mut b = table.clone();
                tabular::booktabs(&mut b);
                if b != table {
                    edit("Convert table to booktabs", serde_json::to_value(&b).unwrap_or_default(), "table");
                }
                let mut s = table.clone();
                if tabular::siunitx(&mut s) {
                    edit("Align numbers with siunitx S columns", serde_json::to_value(&s).unwrap_or_default(), "table");
                }
            }
            Kind::Float => {}
            Kind::Image => {
                let Some((img, _)) = graphic::parse(text, None) else { continue };
                let mut r = img.clone();
                r.angle = Some((img.angle.unwrap_or(0.0) + 90.0) % 360.0);
                edit("Rotate image by 90°", serde_json::to_value(&r).unwrap_or_default(), "image");
                if img.width.as_deref() != Some("\\linewidth") {
                    let mut w = img.clone();
                    (w.width, w.height, w.scale) = (Some("\\linewidth".into()), None, None);
                    edit("Fit image to the line width", serde_json::to_value(&w).unwrap_or_default(), "image");
                }
            }
        }
    }
    out
}
