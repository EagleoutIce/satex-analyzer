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
use crate::engine::builtins::{Cond, OccKind, Primitive};
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
struct Parts {
    /// The byte of `\begin{document}` and of `\end{document}`.
    begin: usize,
    end: usize,
    class: Option<Call>,
}

impl Parts {
    /// The file at `path` when it holds the run's `document` environment.
    fn of(analysis: &Analysis, doc: &Text, path: &str) -> Option<Parts> {
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
fn loads(text: &str, end: usize, package: &str) -> Vec<Call> {
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
fn keyed(doc: &Text, at: usize, options: bool) -> Option<Json> {
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
    let twoside = match a.interner.lookup("if@twoside").map(|s| a.env.meaning(s)) {
        Some(Meaning::Primitive(Primitive::If(Cond::True))) => Some(true),
        Some(Meaning::Primitive(Primitive::If(Cond::False))) => Some(false),
        _ => None,
    };
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
    let unit = ["cm", "mm"].into_iter().find(|u| cfg.units.iter().any(|c| c == u)).unwrap_or("pt");
    json!({ "title": "Page layout", "sections": [{
        "kind": "page",
        "run": run,
        "twoside": twoside,
        "family": family(a),
        "class": a.facts.loads.iter().find(|l| l.kind == crate::engine::builtins::LoadKind::Class).map(|l| &l.name),
        "papers": papers(a),
        "units": units,
        "unit": unit,
        "decimals": cfg.decimals,
        "geometry": { "loaded": geometry, "global": global, "package": package },
        "koma": koma,
        "part": { "here": here, "marks": marks, "end": end },
        "header": header(cx, parts),
    }]})
}

/// The value of a count register `\name` when the run ended.
fn count(a: &Analysis, name: &str) -> Option<i64> {
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
    out.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
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
