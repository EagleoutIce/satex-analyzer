//! Font and size: the document's base size (a class option, KOMA-Script's `fontsize=`) and its
//! main font (a package on pdfLaTeX, `\setmainfont` with fontspec on LuaLaTeX and XeLaTeX), as
//! the run set them. A lens on `\documentclass` opens a view of both. Each choice is a set of
//! snapshots the printer writes, also offered as code actions.

use super::super::snapshot::{self, Arg, Key, Node, Package};
use super::page::{Parts, calls, loads};
use super::styles::macro_text;
use super::{Cx, Hit, TypePlugin, rewrite, span};
use crate::analysis::machine::Analysis;
use crate::analysis::roles::Ty;
use crate::linter::shared::{installed_package, loaded};
use crate::project::config::{Engine, TypePlugins};
use crate::text::Text;
use lsp_types::{CodeAction, CodeLens, TextEdit};
use serde_json::{Value as Json, json};

pub(super) struct Font;

const LENS: &str = "Font and size…";

impl TypePlugin for Font {
    fn name(&self) -> &'static str {
        "font"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "the document's font and base size: a class option, a font package or fontspec"
    }
    // the run's `\documentclass` triggers it, not a type of the text
    fn types(&self) -> &'static [Ty] {
        &[]
    }
    fn features(&self) -> &'static [&'static str] {
        &["lens", "actions", "view"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.font.enabled
    }
    fn actions(&self, cx: &Cx, _hit: &Hit) -> Vec<CodeAction> {
        let Some((_, choices)) = here(cx) else { return Vec::new() };
        choices
            .into_iter()
            // the machine's fonts stay in the view's list: hundreds of actions would bury the others
            .filter(|c| !c.on && !c.edits.is_empty() && c.group != "System fonts" && c.group != "Math font")
            .map(|c| rewrite(cx, format!("{}: {}", c.group, c.title), c.edits))
            .collect()
    }
    fn view(&self, cx: &Cx, _hit: &Hit) -> Option<Json> {
        let (state, choices) = here(cx)?;
        let items = |g: &str| -> Vec<Json> {
            choices
                .iter()
                .filter(|c| c.group == g)
                .map(|c| json!({ "title": c.title, "detail": c.detail, "on": c.on, "edits": c.edits, "svg": c.svg }))
                .collect()
        };
        let mut sections = vec![
            json!({ "kind": "text", "markdown": state.about() }),
            json!({ "kind": "actions", "title": "Base size", "items": items("Base size") }),
            json!({ "kind": "actions", "title": "Font", "items": items("Font") }),
        ];
        // fontspec finds the machine's fonts by name too, and the installation has many math fonts: lists to filter
        for (title, placeholder) in [("System fonts", "a font of this machine"), ("Math font", "a math font")] {
            let list = items(title);
            if !list.is_empty() {
                sections.push(json!({ "kind": "choices", "title": title, "placeholder": placeholder, "focus": false, "items": list }));
            }
        }
        Some(json!({ "title": "Font and size", "sections": sections }))
    }
    fn lenses(&self, analysis: &Analysis, doc: &Text, path: &str) -> Vec<CodeLens> {
        let class = Parts::of(analysis, doc, path).and_then(|p| p.class);
        class.map(|c| super::lens(doc, path, (doc.pos(c.0).line, 1), LENS, "font")).into_iter().collect()
    }
}

/// What the run set: the engine's font loading, the base size, the default family.
struct State {
    unicode: bool,
    size: Option<f64>,
    family: Option<String>,
    font: Option<String>,
}

impl State {
    fn of(a: &Analysis, fonts: &[(String, Vec<String>)]) -> State {
        let unicode = matches!(a.plugins.engine, Engine::LuaTeX | Engine::XeTeX);
        let size = macro_text(a, "f@size").and_then(|s| s.trim().parse().ok());
        // `\familydefault` names one of the three families, or a family itself
        let family = macro_text(a, "familydefault").map(|f| {
            let f = f.trim().to_string();
            f.strip_prefix('\\').and_then(|n| macro_text(a, n)).map_or(f, |t| t.trim().to_string())
        });
        let font = current(a, fonts, family.as_deref());
        State { unicode, size, family, font }
    }
    fn about(&self) -> String {
        let how = match self.unicode {
            true => "The engine reads OpenType fonts: a font is set with fontspec's `\\setmainfont`.",
            false => "pdfLaTeX: a font is a package, with the T1 encoding.",
        };
        let size = self.size.map_or("not known".into(), |s| format!("{s}pt"));
        let font = self.font.clone().or_else(|| self.family.as_ref().map(|f| format!("family `{f}`")));
        format!("{how} The run's base size is {size}, its font {}.", font.unwrap_or_else(|| "not known".into()))
    }
}

/// A choice of the view: its group, title, whether it is the run's, the edits writing it and
/// its sample as TeX set it (the preview sheet's `font:` sprite).
struct Choice {
    group: &'static str,
    title: String,
    detail: String,
    on: bool,
    edits: Vec<TextEdit>,
    svg: Option<String>,
}

/// The letters and digits of a font's name, lower case: `LibertinusSerif(0)` and `Libertinus Serif` agree.
fn plain(name: &str) -> String {
    let name = name.split('(').next().unwrap_or(name);
    name.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect()
}

/// The offered font the run set: the last one whose package it loaded, else the one whose name
/// is the default family's. A family the run set without an offered package names no offer.
fn current(a: &Analysis, fonts: &[(String, Vec<String>)], family: Option<&str>) -> Option<String> {
    let load = |p: &str| a.facts.loads.iter().rposition(|l| l.name == p);
    let by_load = fonts.iter().filter_map(|(n, v)| Some((load(v.first()?)?, n))).max();
    if let Some((_, n)) = by_load {
        return Some(n.clone());
    }
    let family = family?;
    fonts.iter().find(|(_, v)| v.get(1).is_some_and(|f| plain(f) == plain(family))).map(|(n, _)| n.clone())
}

/// The families fontconfig knows (what fontspec finds by name beyond the TeX tree), sorted, once
/// per process; none where `fc-list` cannot be run (the browser).
fn system_families() -> &'static [String] {
    static FAMILIES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    FAMILIES.get_or_init(|| {
        let out = std::process::Command::new("fc-list").args([":", "family"]).output();
        let text = out.ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
        // `DejaVu Sans,DejaVu Sans Condensed`: a font's family names, its first the plain one
        let names = text.iter().flat_map(|t| t.lines()).filter_map(|l| l.split(',').next()).map(str::trim);
        names
            .filter(|n| !n.is_empty())
            .map(String::from)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    })
}

/// The state and choices on the class's line, none elsewhere.
fn here(cx: &Cx) -> Option<(State, Vec<Choice>)> {
    let parts = Parts::of(cx.analysis, cx.doc, cx.path)?;
    let class = parts.class.clone()?;
    (cx.doc.pos(class.0).line == cx.pos.line).then_some(())?;
    let cfg = &cx.cfg().font;
    let fonts: Vec<(String, Vec<String>)> = cfg.fonts.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let state = State::of(cx.analysis, &fonts);
    let (a, doc) = (cx.analysis, cx.doc);
    let mut out = Vec::new();
    let node = snapshot::read(doc, class.0, &[0], true)?;
    for size in &cfg.sizes {
        let on = state.size.zip(pt(size)).is_some_and(|(s, v)| (s - v).abs() < 0.5);
        let nodes = vec![sized(a, &node, size)];
        out.push(Choice {
            group: "Base size",
            title: size.clone(),
            detail: String::new(),
            on,
            edits: edits(a, doc, &nodes),
            svg: None,
        });
    }
    let field = |spec: &[String], i: usize| spec.get(i).filter(|f| !f.is_empty()).cloned();
    // the other offered fonts' packages: a choice drops their loads
    let others = |spec: &[String]| -> Vec<&str> {
        let package = spec.first().map_or("", String::as_str);
        fonts
            .iter()
            .filter_map(|(_, v)| v.first().map(String::as_str))
            .filter(|p| !p.is_empty() && *p != package)
            .collect()
    };
    for (name, spec) in &fonts {
        let on = state.font.as_deref() == Some(name.as_str());
        // what this engine can set: a font name for fontspec, an installed package for pdfLaTeX
        let detail = match state.unicode {
            true => field(spec, 1).map(|f| format!("\\setmainfont{{{f}}}")),
            false => field(spec, 0).filter(|p| on || installed_package(a, p)).map(|p| format!("\\usepackage{{{p}}}")),
        };
        let Some(detail) = detail else { continue };
        let nodes = font_nodes(cx, &parts, &state, &others(spec), spec, Set::Main);
        let svg = crate::queries::preview::font_sample(a, name);
        out.push(Choice { group: "Font", title: name.clone(), detail, on, edits: edits(a, doc, &nodes), svg });
    }
    if state.unicode {
        let named: Vec<String> = fonts.iter().filter_map(|(_, v)| field(v, 1)).collect();
        for family in system_families().iter().filter(|f| !named.contains(f)) {
            let spec = [String::new(), family.clone()];
            let nodes = font_nodes(cx, &parts, &state, &others(&spec), &spec, Set::Main);
            let detail = format!("\\setmainfont{{{family}}}");
            let on = state.family.as_deref().is_some_and(|f| plain(f) == plain(family));
            out.push(Choice {
                group: "System fonts",
                title: family.clone(),
                detail,
                on,
                edits: edits(a, doc, &nodes),
                svg: None,
            });
        }
    }
    // the installation's math fonts: packages declaring math symbol fonts (pdfLaTeX), OpenType
    // fonts with a MATH table (unicode-math)
    let mut math = crate::linter::shared::declared(a, "mathfont");
    math.sort_by_key(|(name, _)| name.to_lowercase());
    let set = calls(doc.body(), "setmathfont", 0, parts.begin).pop();
    let set =
        set.and_then(|c| c.1.iter().rfind(|g| g.open == '{').map(|g| doc.body()[g.inner.clone()].trim().to_string()));
    for (name, files) in &math {
        let Some(file) = files.first() else { continue };
        let (detail, on, nodes) = match (state.unicode, file.strip_suffix(".sty")) {
            (false, Some(package)) => {
                // added, the loads kept: many of these packages set the text font too (lmodern, kpfonts)
                let nodes = font_nodes(cx, &parts, &state, &[], &[package.to_string()], Set::Math);
                (format!("\\usepackage{{{package}}}"), loaded(a, package), nodes)
            }
            (true, None) => {
                let nodes = font_nodes(cx, &parts, &state, &[], &[String::new(), file.clone()], Set::Math);
                let on = set.as_deref().is_some_and(|s| s == file || plain(s) == plain(name));
                (format!("\\setmathfont{{{file}}}"), on, nodes)
            }
            _ => continue,
        };
        let svg = crate::queries::preview::math_font_sample(a, name);
        out.push(Choice { group: "Math font", title: name.clone(), detail, on, edits: edits(a, doc, &nodes), svg });
    }
    Some((state, out))
}

/// What a font choice sets: the main font, or the math font.
#[derive(Clone, Copy, PartialEq)]
enum Set {
    Main,
    Math,
}

/// The printer's edits of `nodes`, none when refused.
fn edits(a: &Analysis, doc: &Text, nodes: &[Node]) -> Vec<TextEdit> {
    // the font's calls stand in the preamble, where no command is written in
    snapshot::edits(a, doc, None, nodes)
}

/// A size option in points: `11pt` is 11.
fn pt(option: &str) -> Option<f64> {
    option.strip_suffix("pt")?.parse().ok()
}

/// The class's options with `size` as its size: a written size option in its place and form,
/// else KOMA-Script's `fontsize=` where the class is one, else the bare option.
fn sized(a: &Analysis, class: &Node, size: &str) -> Node {
    let mut n = class.clone();
    let Some(keys) = n.args.first_mut().and_then(|g| g.keys.as_mut()) else { return n };
    let bare = |k: &Key| k.value.is_none() && pt(&k.key).is_some();
    let written = keys.iter().position(|k| bare(k) || k.key == "fontsize");
    let new = match written.map(|i| &keys[i]) {
        Some(k) if k.key == "fontsize" => Key { key: k.key.clone(), value: Some(size.into()) },
        None if a.defined("KOMAClassName") => Key { key: "fontsize".into(), value: Some(size.into()) },
        _ => Key { key: size.into(), value: None },
    };
    match written {
        Some(i) => keys[i] = new,
        None => keys.push(new),
    }
    n
}

/// The snapshots that make `spec` the main or math font: the loads of `others` dropped, then
/// its package (pdfLaTeX) or `\setmainfont` (fontspec, `\setmathfont` with unicode-math), and
/// the sans default where it is sans.
fn font_nodes(cx: &Cx, parts: &Parts, state: &State, others: &[&str], spec: &[String], set: Set) -> Vec<Node> {
    let (a, doc) = (cx.analysis, cx.doc);
    let text = doc.body();
    let line = text[..parts.begin].rfind('\n').map_or(0, |n| n + 1);
    let at = span(doc, line, line).start;
    let (package, name, sans) = (
        spec.first().map_or("", String::as_str),
        spec.get(1).map_or("", String::as_str),
        spec.get(2).is_some_and(|s| s == "sans"),
    );
    let mut out = Vec::new();
    // each call loading another offered font: that name out, the call dropped when it loads nothing else,
    // on pdfLaTeX the first such call loads the new package in its place
    let mut calls_seen: Vec<usize> = Vec::new();
    let mut placed = state.unicode || loaded(a, package) || !loads(text, parts.begin, package).is_empty();
    for p in others {
        for c in loads(text, parts.begin, p) {
            if calls_seen.contains(&c.0) {
                continue;
            }
            calls_seen.push(c.0);
            let Some(mut n) = snapshot::read(doc, c.0, &[], false) else { continue };
            let Some(list) = n.args.iter_mut().rfind(|g| g.open == '{') else { continue };
            let mut names: Vec<&str> = list.text.split(',').map(str::trim).filter(|q| !others.contains(q)).collect();
            if !placed {
                names.push(package);
                placed = true;
            }
            match names.is_empty() {
                true => n.drop = true,
                false => list.text = names.join(","),
            }
            out.push(n);
        }
    }
    if state.unicode {
        let (cmd, pkg) = if set == Set::Math { ("setmathfont", "unicode-math") } else { ("setmainfont", "fontspec") };
        match calls(text, cmd, 0, parts.begin).pop().and_then(|c| snapshot::read(doc, c.0, &[], false)) {
            Some(mut n) => {
                if let Some(g) = n.args.iter_mut().rfind(|g| g.open == '{') {
                    g.text = name.into();
                }
                out.push(n);
            }
            None => {
                let mut n = Node::call(cmd, &[name], at);
                n.package = Some(Package { name: pkg.into(), options: String::new() });
                out.push(n);
            }
        }
    } else {
        // fonts other than Computer Modern want the T1 encoding
        let t1 = macro_text(a, "encodingdefault").is_some_and(|e| e.trim() == "T1")
            || !loads(text, parts.begin, "fontenc").is_empty();
        if !t1 && set == Set::Main {
            out.push(Node::new_call("usepackage", vec![Arg::text('[', "T1"), Arg::text('{', "fontenc")], at));
        }
        if !placed {
            out.push(Node::call("usepackage", &[package], at));
        }
    }
    let sf = state.family.is_some() && state.family == macro_text(a, "sfdefault").map(|t| t.trim().to_string());
    // a sans main font with a package: the default family is the sans one
    if sans && !state.unicode && !sf {
        out.push(Node::call("renewcommand", &["\\familydefault", "\\sfdefault"], at));
    }
    if set == Set::Main && (!sans || state.unicode) {
        for c in calls(text, "renewcommand", 0, parts.begin) {
            let sets = c.1.iter().map(|g| text[g.inner.clone()].trim()).collect::<Vec<_>>()
                == ["\\familydefault", "\\sfdefault"];
            if let Some(n) = sets.then(|| snapshot::read(doc, c.0, &[], false)).flatten() {
                out.push(Node { drop: true, ..n });
            }
        }
    }
    out
}
