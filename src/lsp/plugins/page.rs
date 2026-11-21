//! Page layout: the page geometry the run computed (`\paperwidth`, `\textwidth`, `\topmargin`, …),
//! drawn to scale in a dialog (`views.js` `page`). It writes geometry's options, or typearea's
//! `DIV` and `BCOR`, `\newgeometry` … `\restoregeometry` around a part of the body, and the
//! header and footer of fancyhdr or scrlayer-scrpage, whichever the run loaded. It opens from a
//! lens on `\documentclass` and geometry's calls, a link on their keys' hover, and on a
//! sectioning command or a blank line.

use super::super::snapshot::{self, Arg, Node, Package};
use super::{Cx, Group, Hit, TypePlugin, call_at, span};
use crate::analysis::machine::Analysis;
use crate::analysis::roles::Ty;
use crate::engine::builtins::OccKind;
use crate::engine::tex::Meaning;
use crate::engine::value::{UNIT, UNITS};
use crate::linter::shared::{loaded, macro_text};
use crate::project::config::TypePlugins;
use crate::text::{Pos, Text};
use lsp_types::CodeLens;
use serde_json::{Value as Json, json};

pub(super) struct Page;

/// The lengths of LaTeX's page model the dialog shows (clsguide, "Page layout").
const LENGTHS: [&str; 14] = [
    "paperwidth",
    "paperheight",
    "textwidth",
    "textheight",
    "oddsidemargin",
    "evensidemargin",
    "topmargin",
    "headheight",
    "headsep",
    "footskip",
    "marginparwidth",
    "marginparsep",
    "hoffset",
    "voffset",
];
/// geometry's calls whose keys the dialog writes.
const SETTERS: [&str; 2] = ["geometry", "newgeometry"];
/// The calls of the header packages, by the package that defines them.
const HEADERS: [(&str, &[&str]); 2] = [
    ("fancyhdr", &["fancyhf", "fancyhead", "fancyfoot", "renewcommand"]),
    ("scrlayer-scrpage", &["ihead", "chead", "ohead", "ifoot", "cfoot", "ofoot"]),
];
const LENS: &str = "Page layout…";

impl TypePlugin for Page {
    fn name(&self) -> &'static str {
        "page"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "the page geometry to scale: margins, paper, header and footer, for the document or a part"
    }
    // the run's `\documentclass` and geometry's calls trigger it, not a type of the text
    fn types(&self) -> &'static [Ty] {
        &[]
    }
    fn features(&self) -> &'static [&'static str] {
        &["lens", "hover", "view"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.page.enabled
    }

    fn entry(&self, cx: &Cx, _hit: &Hit) -> Option<String> {
        let parts = Parts::of(cx.analysis, cx.doc, cx.path)?;
        let at = cx.doc.offset(cx.pos)?;
        let inside = |(_, g, _): &(usize, Vec<Group>, usize)| g.iter().any(|g| g.inner.contains(&at));
        let keys = parts.geometry_calls(cx.doc.body()).iter().any(inside);
        keys.then(|| {
            let args = json!([cx.uri(), super::super::handlers::to_lsp(cx.doc, cx.pos), "page"]);
            format!("[{LENS}](command:satex.openView?{})", super::units::encode(&args.to_string()))
        })
    }

    fn view(&self, cx: &Cx, _hit: &Hit) -> Option<Json> {
        let parts = Parts::of(cx.analysis, cx.doc, cx.path)?;
        let line = cx.pos.line;
        let blank = cx.doc.line(line).trim().is_empty();
        (blank || parts.lines(cx.analysis, cx.doc, cx.path).contains(&line)).then(|| view(cx, &parts))
    }

    fn lenses(&self, analysis: &Analysis, doc: &Text, path: &str) -> Vec<CodeLens> {
        let Some(parts) = Parts::of(analysis, doc, path) else { return Vec::new() };
        let mut lines: Vec<u32> = parts.class.iter().map(|c| doc.pos(c.0).line).collect();
        lines.extend(parts.geometry_calls(doc.body()).iter().map(|c| doc.pos(c.0).line));
        lines.sort_unstable();
        lines.dedup();
        lines.into_iter().map(|l| super::lens(doc, path, l, LENS, "page")).collect()
    }
}

/// A call as [`call_at`] reads it: its start, groups and end.
pub(super) type Call = (usize, Vec<Group>, usize);

/// Where the main file's preamble and body are.
pub(super) struct Parts {
    /// The byte of `\begin{document}` and of `\end{document}`.
    pub begin: usize,
    pub end: usize,
    pub class: Option<Call>,
}

impl Parts {
    /// The file at `path` when it holds the run's `document` environment.
    pub(super) fn of(analysis: &Analysis, doc: &Text, path: &str) -> Option<Parts> {
        let (b, e) = super::pairs(analysis, path, "document").into_iter().next()?;
        let text = doc.body();
        let begin = doc.offset(Pos::new(b.span.line, 1))?;
        let begin = begin + text[begin..].find("\\begin")?;
        let end = doc.offset(Pos::new(e.span.line, 1))?;
        let end = end + text[end..].find("\\end").unwrap_or(0);
        let class = calls(text, "documentclass", 0, begin).into_iter().next();
        Some(Parts { begin, end, class })
    }

    /// `\usepackage[…]{geometry}`, `\geometry{…}` and `\newgeometry{…}` as written.
    fn geometry_calls(&self, text: &str) -> Vec<Call> {
        let mut out = loads(text, self.begin, "geometry");
        out.extend(SETTERS.iter().flat_map(|s| calls(text, s, 0, text.len())));
        out
    }

    /// The lines the view opens on: the class, geometry's and the header's calls, sectioning commands.
    fn lines(&self, analysis: &Analysis, doc: &Text, path: &str) -> Vec<u32> {
        let text = doc.body();
        let mut calls: Vec<Call> = self.geometry_calls(text);
        calls.extend(self.class.clone());
        calls.extend(HEADERS.iter().flat_map(|(_, names)| names.iter().flat_map(|n| calls_of(text, n, 0, self.begin))));
        let mut lines: Vec<u32> = calls.iter().map(|c| doc.pos(c.0).line).collect();
        lines.extend(sections(analysis, path).map(|o| o.span.line));
        lines
    }
}

/// The calls of `\name` in `text[from..to]` outside comments.
pub(super) fn calls(text: &str, name: &str, from: usize, to: usize) -> Vec<Call> {
    let cs = format!("\\{name}");
    let mut out = Vec::new();
    for (i, _) in text[from..to].match_indices(&cs) {
        let at = from + i;
        let line = text[..at].rfind('\n').map_or(0, |n| n + 1);
        let next = text[at + cs.len()..].chars().next();
        if next.is_some_and(|c| c.is_ascii_alphabetic() || c == '@')
            || super::super::handlers::uncommented(&text[line..at]).len() < at - line
        {
            continue;
        }
        if let Some((_, groups, end)) = call_at(text, at) {
            out.push((at, groups, end));
        }
    }
    out
}

/// [`calls`] by a name of [`HEADERS`]: `\renewcommand` only of a rule's width.
fn calls_of(text: &str, name: &str, from: usize, to: usize) -> Vec<Call> {
    let mut out = calls(text, name, from, to);
    if name == "renewcommand" {
        out.retain(|(_, g, _)| g.first().is_some_and(|g| text[g.inner.clone()].trim().ends_with("rulewidth")));
    }
    out
}

/// The `\usepackage` and `\RequirePackage` calls before `end` that load `package`.
pub(super) fn loads(text: &str, end: usize, package: &str) -> Vec<Call> {
    let names = |c: &Call| c.1.iter().rfind(|g| g.open == '{').map(|g| text[g.inner.clone()].to_string());
    ["usepackage", "RequirePackage"]
        .iter()
        .flat_map(|n| calls(text, n, 0, end))
        .filter(|c| names(c).is_some_and(|n| n.split(',').any(|p| p.trim() == package)))
        .collect()
}

/// The run's sectioning commands in the file at `path`.
fn sections<'a>(analysis: &'a Analysis, path: &str) -> impl Iterator<Item = &'a crate::analysis::facts::Occurrence> {
    let file = super::super::type_at::file_of(analysis, path);
    analysis.facts.occurrences.iter().filter(move |o| o.kind == OccKind::Section && Some(o.span.file) == file)
}

/// The call at `at` as a snapshot node, its first argument a key list (`options`: the first
/// `[…]`, an unwritten one when there is none).
pub(super) fn keyed(doc: &Text, at: usize, options: bool) -> Option<Json> {
    serde_json::to_value(snapshot::read(doc, at, &[0], options)?).ok()
}

/// A new call before `\begin{document}`, its first argument an empty key list.
fn new_call(doc: &Text, at: usize, name: &str, args: Vec<Arg>, note: Option<&str>) -> Json {
    let mut n = serde_json::to_value(Node::new_call(name, args, span(doc, at, at).start)).unwrap_or_default();
    n["note"] = json!(note);
    n
}

/// The class family the run met: `koma` with typearea, `memoir` with memoir's layout macros, else `basic`.
fn family(a: &Analysis) -> &'static str {
    let defined = |n: &str| a.interner.lookup(n).is_some_and(|s| !matches!(a.env.meaning(s), Meaning::Undefined));
    if loaded(a, "typearea") {
        "koma"
    } else if defined("checkandfixthelayout") {
        "memoir"
    } else {
        "basic"
    }
}

/// The view: the run's lengths, the papers geometry defines, where each kind of edit writes.
fn view(cx: &Cx, parts: &Parts) -> Json {
    let (a, doc) = (cx.analysis, cx.doc);
    let text = doc.body();
    let run: serde_json::Map<String, Json> = LENGTHS
        .iter()
        .filter_map(|n| Some((n.to_string(), super::register_value(a, n).map(|v| json!([v.lo, v.hi]))?)))
        .collect();
    let twoside = super::styles::flag(a, "if@twoside");
    let units: Vec<Json> = UNITS
        .iter()
        .filter(|(u, ..)| *u != "mu")
        .map(|(u, n, d)| json!({ "name": u, "sp": UNIT as f64 * *n as f64 / *d as f64 }))
        .collect();
    let cfg = &cx.cfg().units;
    let line_start = |at: usize| text[..at].rfind('\n').map_or(0, |n| n + 1);
    let before_document = line_start(parts.begin);
    // geometry: its last `\geometry` in the preamble, else its `\usepackage`, else a new load
    let geometry = loaded(a, "geometry");
    let set = calls(text, "geometry", 0, parts.begin).pop().and_then(|c| keyed(doc, c.0, false));
    let global = set.or_else(|| loads(text, parts.begin, "geometry").first().and_then(|c| keyed(doc, c.0, true)));
    let global = global.unwrap_or_else(|| match geometry {
        true => new_call(doc, before_document, "geometry", vec![Arg::keys('{', Vec::new())], None),
        false => new_call(
            doc,
            before_document,
            "usepackage",
            vec![Arg::keys('[', Vec::new()), Arg::text('{', "geometry")],
            Some("adds \\usepackage{geometry}"),
        ),
    });
    // typearea: its options where the run loaded it, the class's when the class did
    let koma = loaded(a, "typearea")
        .then(|| {
            let own = loads(text, parts.begin, "typearea").into_iter().next().and_then(|c| keyed(doc, c.0, true));
            let mut t = own.or_else(|| parts.class.as_ref().and_then(|c| keyed(doc, c.0, true)))?;
            t["div"] = json!(count(a, "ta@div").filter(|d| *d > 3));
            t["bcor"] = json!(super::register_value(a, "ta@bcor").map(|v| v.lo));
            Some(t)
        })
        .flatten();
    let at_line = |l: u32| doc.offset(Pos::new(l, 1)).map(|o| span(doc, o, o).start);
    let here = (cx.pos.line > doc.pos(parts.begin).line && cx.pos.line < doc.pos(parts.end).line)
        .then(|| at_line(cx.pos.line))
        .flatten();
    let marks: Vec<Json> = sections(a, cx.path)
        .filter_map(|o| Some(json!({ "title": o.key, "level": o.detail, "at": at_line(o.span.line)? })))
        .collect();
    let end = span(doc, line_start(parts.end), line_start(parts.end)).start;
    // `\newgeometry` loads geometry with `pass` when the run has none: the layout elsewhere stays
    let package = (!geometry).then(|| Package { name: "geometry".into(), options: "pass".into() });
    let papers = papers(a);
    let unit = ["cm", "mm"].into_iter().find(|u| cfg.units.iter().any(|c| c == u)).unwrap_or("pt");
    json!({ "title": "Page layout", "sections": [{
        "kind": "page",
        "run": run,
        "twoside": twoside,
        "family": family(a),
        "class": a.facts.loads.iter().find(|l| l.kind == crate::engine::builtins::LoadKind::Class).map(|l| &l.name),
        "papers": papers,
        "paper": paper_names(a, &papers),
        "tagging": tagging(a),
        "styles": styles(a),
        "units": units,
        "unit": unit,
        "decimals": cfg.decimals,
        "geometry": { "loaded": geometry, "global": global, "package": package },
        "koma": koma,
        "part": { "here": here, "marks": marks, "end": end },
        "header": header(cx, parts),
        "numbering": numbering(cx, parts),
        "columns": { "two": super::styles::flag(a, "if@twocolumn"), "class": parts.class.as_ref().and_then(|c| keyed(doc, c.0, true)) },
        "binding": super::register_value(a, "Gm@bindingoffset").map(|v| v.lo),
        "pagecolor": super::styles::pagecolor(cx, parts.begin),
        "headings": super::styles::headings(cx, family(a), parts.begin, span(doc, before_document, before_document).start),
    }]})
}

/// The matter marks after which a numbering style is set; each sets its own (classes.dtx).
const MATTERS: [&str; 4] = ["frontmatter", "mainmatter", "appendix", "backmatter"];

/// Page numbering: the places it may restart (the document's start, the matter marks, the
/// `\pagenumbering` calls as written) and whether `page` restarts with each chapter.
fn numbering(cx: &Cx, parts: &Parts) -> Json {
    let (a, doc) = (cx.analysis, cx.doc);
    let text = doc.body();
    // a new call goes on the line after the one it follows: a matter mark sets its own style first
    let after = |at: usize| span(doc, after_byte(text, at), after_byte(text, at)).start;
    let node = |at: usize| serde_json::to_value(snapshot::read(doc, at, &[], false)?).ok();
    let written: Vec<Json> = calls(text, "pagenumbering", parts.begin, parts.end)
        .into_iter()
        .filter_map(|c| Some(json!({ "node": node(c.0)?, "row": doc.pos(c.0).line })))
        .collect();
    let marks: Vec<Json> = MATTERS
        .iter()
        .flat_map(|m| calls(text, m, parts.begin, parts.end).into_iter().map(move |c| (m, c)))
        .map(|(m, c)| json!({ "name": m, "row": doc.pos(c.0).line, "at": after(c.2) }))
        .collect();
    // `page` in the chapter's reset list (`\cl@chapter`), and the call that put it there
    let chapters = macro_text(a, "cl@chapter");
    let reset = calls(text, "counterwithin", 0, parts.begin)
        .into_iter()
        .find(|c| text[c.0..c.2].contains("{page}") && text[c.0..c.2].contains("{chapter}"))
        .and_then(|c| node(c.0));
    let before_document = text[..parts.begin].rfind('\n').map_or(0, |n| n + 1);
    json!({
        "start": after(parts.begin),
        "calls": written,
        "marks": marks,
        "chapters": chapters.is_some(),
        "per_chapter": chapters.is_some_and(|t| t.contains("\\@elt {page}") || t.contains("\\@elt{page}")),
        "reset": reset,
        "insert": span(doc, before_document, before_document).start,
    })
}

/// The byte after the line end at or past `at`.
fn after_byte(text: &str, at: usize) -> usize {
    text[at..].find('\n').map_or(text.len(), |n| at + n + 1)
}

/// The value of a count register `\name` when the run ended.
pub(super) fn count(a: &Analysis, name: &str) -> Option<i64> {
    let v = super::register_value(a, name)?;
    (v.lo == v.hi).then_some(v.lo)
}

/// The papers geometry defines (`\Gm@a4paper` is `\Gm@setsize{#1}(210,297){mm}`), in sp:
/// from the run's macros, else from the installed `geometry.sty` (unloaded, the dialog may load it).
fn papers(a: &Analysis) -> Vec<Json> {
    let size = |paper: &str, body: &str| {
        let (size, unit) = body.split_once('(')?.1.split_once(')')?;
        let (w, h) = size.split_once(',')?;
        let unit = unit.trim().trim_start_matches('{').split('}').next()?;
        let sp = |v: &str| crate::engine::value::parse_dimen(&format!("{}{unit}", v.trim()));
        Some(json!({ "name": paper, "w": sp(w)?, "h": sp(h)? }))
    };
    let mut out: Vec<Json> = a
        .interner
        .names()
        .iter()
        .filter_map(|name| {
            let paper = name.strip_prefix("Gm@")?;
            let Meaning::Macro(m) = a.env.meaning(a.interner.lookup(name)?) else { return None };
            size(paper, &crate::engine::tex::text_of(&m.replacement_text, &a.interner))
        })
        .collect();
    if out.is_empty() {
        let sty = crate::caches::distribution::tex_file(&a.distribution.roots, "geometry.sty")
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        out = sty
            .lines()
            .filter_map(|l| {
                let (paper, body) = l.strip_prefix("\\@namedef{Gm@")?.split_once('}')?;
                size(paper, body)
            })
            .collect();
    }
    // the class's own paper options (`\ds@letterpaper` sets `\paperheight` and `\paperwidth`):
    // what a run without geometry, or a browser without its file, can still name
    for p in class_papers(a) {
        match out.iter_mut().find(|o| o["name"] == p["name"]) {
            Some(o) => o["class"] = json!(true),
            None => out.push(p),
        }
    }
    out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    out
}

/// The paper options the run's classes declare, read from the files the run read (the kernel
/// sets each `\ds@⟨option⟩` to `\relax` once processed): each `\DeclareOption{⟨name⟩}{…}` whose
/// code sets both `\paperwidth` and `\paperheight` to a length written out, in sp.
fn class_papers(a: &Analysis) -> Vec<Json> {
    let length = |body: &str, name: &str| {
        let (_, rest) = body.split_once(&format!("\\{name}"))?;
        let rest = rest.trim_start().trim_start_matches('=').trim_start();
        let v = match rest.strip_prefix('{') {
            Some(r) => r.split('}').next()?,
            None => rest.split(|c: char| c == '\\' || c.is_whitespace()).next()?,
        };
        crate::engine::value::parse_dimen(&v.replace(' ', ""))
    };
    // the braced group at the start of `text` (after blanks and comments), and what follows it
    let group = |text: &str| -> Option<(String, usize)> {
        let start = text.len() - text.trim_start().len();
        let rest = &text[start..];
        let rest = rest.strip_prefix('{')?;
        let mut depth = 1;
        for (k, c) in rest.char_indices() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                return Some((rest[..k].to_string(), start + 1 + k + 1));
            }
        }
        None
    };
    let mut out = Vec::new();
    let classes = a.facts.loads.iter().filter(|l| l.kind == crate::engine::builtins::LoadKind::Class);
    for path in classes.filter_map(|l| l.path.as_deref()) {
        let Ok(bytes) = crate::project::overlay::read_bytes(std::path::Path::new(path)) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        // comments out: a `%` ends the line's code
        let code: String = text.lines().map(|l| l.split('%').next().unwrap_or("")).collect::<Vec<_>>().join("\n");
        for (at, _) in code.match_indices("\\DeclareOption") {
            let rest = &code[at + "\\DeclareOption".len()..];
            let Some((name, used)) = group(rest) else { continue };
            let Some((body, _)) = group(&rest[used..]) else { continue };
            if let (Some(w), Some(h)) = (length(&body, "paperwidth"), length(&body, "paperheight")) {
                out.push(json!({ "name": name.trim(), "w": w, "h": h, "class": true }));
            }
        }
    }
    out
}

/// The papers whose size is the run's `\paperwidth` × `\paperheight`, upright or turned.
fn paper_names(a: &Analysis, papers: &[Json]) -> Vec<String> {
    let (Some(w), Some(h)) = (count(a, "paperwidth"), count(a, "paperheight")) else { return Vec::new() };
    let mut out: Vec<(bool, String)> = papers
        .iter()
        .filter(|p| {
            let (pw, ph) = (p["w"].as_i64(), p["h"].as_i64());
            (pw, ph) == (Some(w), Some(h)) || (pw, ph) == (Some(h), Some(w))
        })
        .filter_map(|p| Some((p["class"] != true, p["name"].as_str()?.to_string())))
        .collect();
    // the class's own options first: `letterpaper` before geometry's `ansiapaper` of the same size
    out.sort();
    let mut out: Vec<String> = out.into_iter().map(|(_, n)| n).collect();
    out.dedup();
    out
}

/// The header and footer: the package the run loaded for them, its calls as written in the
/// preamble, where new ones go, and what loading fancyhdr adds when none is loaded.
fn header(cx: &Cx, parts: &Parts) -> Json {
    let (a, doc) = (cx.analysis, cx.doc);
    let text = doc.body();
    let package = HEADERS.iter().find(|(p, _)| loaded(a, p));
    let before_document = text[..parts.begin].rfind('\n').map_or(0, |n| n + 1);
    let Some((package, names)) = package else {
        return json!({ "package": null, "insert": span(doc, before_document, before_document).start,
            "load": { "name": "fancyhdr" } });
    };
    let mut found: Vec<(usize, Json)> = Vec::new();
    for name in names.iter() {
        for (at, ..) in calls_of(text, name, 0, parts.begin) {
            found.extend(
                snapshot::read(doc, at, &[], false).and_then(|n| serde_json::to_value(n).ok()).map(|n| (at, n)),
            );
        }
    }
    found.sort_by_key(|(at, _)| *at);
    // after the package's load in this file, else before `\begin{document}`
    let after =
        loads(text, parts.begin, package).first().map(|c| text[c.2..].find('\n').map_or(text.len(), |n| c.2 + n + 1));
    let insert = after.unwrap_or(before_document);
    let fancy = macro_text(a, "@oddhead").is_some_and(|t| t.contains("f@nch"));
    json!({
        "package": package,
        "calls": found.into_iter().map(|(_, c)| c).collect::<Vec<_>>(),
        "insert": span(doc, insert, insert).start,
        "style": *package == "fancyhdr" && !fancy,
        "rules": { "head": macro_text(a, "headrulewidth"), "foot": macro_text(a, "footrulewidth") },
    })
}

/// The tagging state of the packages the dialog's edits may load: `active` when
/// `\DocumentMetadata` turned tagging on, each package's status by the installation's
/// `latex-tagging-status.ltx` (`unknown` where unlisted or not looked up: no `\DocumentMetadata`).
fn tagging(a: &Analysis) -> Json {
    use crate::analysis::tagging;
    let t = tagging::of(a);
    let names: Vec<&str> = ["geometry", "typearea"].into_iter().chain(HEADERS.iter().map(|(p, _)| *p)).collect();
    let codes = t.map(|_| tagging::statuses(a, &names.iter().map(|n| format!("{n}.sty")).collect::<Vec<_>>()));
    let word = |code: Option<u8>| match code {
        Some(4) => "compatible",
        Some(3) => "partial",
        Some(1 | 2) => "incompatible",
        _ => "unknown",
    };
    let status: serde_json::Map<String, Json> = names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.to_string(), json!(word(codes.as_ref().and_then(|c| c.get(i).copied().flatten())))))
        .collect();
    json!({ "active": t.is_some_and(|t| t.active()), "status": status })
}

/// What the page style in force at the run's end prints: `\@oddhead`, `\@evenhead`,
/// `\@oddfoot` and `\@evenfoot` as written, and split into left, centre and right. The kernel's
/// styles put the parts apart with `\hfil` (plain: `\hfil\thepage\hfil`); fancyhdr's are its
/// fields. A part is `null` where the text is not known or is not made that way.
fn styles(a: &Analysis) -> Json {
    let fancy = super::styles::macro_text(a, "@oddhead").is_some_and(|t| t.contains("f@nch"));
    let field = |side: char, part: char, at: char| {
        // `\f@nch@def` appends a `\strut` to what the document wrote
        super::styles::macro_text(a, &format!("f@nch@{side}{part}{at}"))
            .map(|t| t.trim().trim_end_matches("\\strut").trim().to_string())
    };
    let one = |name: &str, side: char, at: char| {
        let text = super::styles::macro_text(a, name);
        let parts: [Option<String>; 3] = match fancy {
            true => ['l', 'c', 'r'].map(|p| field(side, p, at)),
            false => text.as_deref().map_or([None, None, None], apart),
        };
        json!({ "text": text, "left": parts[0], "center": parts[1], "right": parts[2] })
    };
    json!({
        "fancy": fancy,
        "head": { "odd": one("@oddhead", 'o', 'h'), "even": one("@evenhead", 'e', 'h') },
        "foot": { "odd": one("@oddfoot", 'o', 'f'), "even": one("@evenfoot", 'e', 'f') },
    })
}

/// A kernel style's line split at its top-level `\hfil`s: one part is the left, two left and
/// right, three left, centre and right. A conditional or more parts: not known.
fn apart(text: &str) -> [Option<String>; 3] {
    let text = text.trim().trim_start_matches("\\reset@font").trim();
    if text.contains("\\if") {
        return [None, None, None];
    }
    let (mut parts, mut depth, mut from) = (Vec::new(), 0i32, 0);
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            b'\\' => {
                let name =
                    text[i + 1..].chars().take_while(|c| c.is_ascii_alphabetic() || *c == '@').collect::<String>();
                if depth == 0 && name == "hfil" {
                    parts.push(text[from..i].trim().to_string());
                    from = i + 5;
                }
                // a control symbol is its one character, of any width
                i += 1 + if name.is_empty() {
                    text[i + 1..].chars().next().map_or(0, char::len_utf8)
                } else {
                    name.len()
                };
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(text[from..].trim().to_string());
    let some = |s: &str| Some(s.to_string());
    match parts.as_slice() {
        [l] => [some(l), some(""), some("")],
        [l, r] => [some(l), some(""), some(r)],
        [l, c, r] => [some(l), some(c), some(r)],
        _ => [None, None, None],
    }
}
