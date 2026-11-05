//! Small pictures for completion items. A key's line width or dash
//! pattern is the argument its code hands pgf's setter, drawn here as SVG.
//! Arrow tips and fill patterns are the ones the run declared, drawn by
//! TeX itself and kept in the cache directory (`previews.json`).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use crate::analysis::machine::Analysis;
use crate::engine::tex::{Meaning, Sym};

/// The pgf parameter a key's code sets: its setter, as the code reaches it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stroke {
    Width,
    Dash,
    Arrows,
    Pattern,
    /// A color: the key's value names one.
    Color,
    /// A node: the key's value names one (`right of=a`).
    Node,
    /// A node after `of` (positioning's `right=of a`).
    NodeOf,
}

impl Stroke {
    const SETTERS: [(&'static str, Stroke); 12] = [
        ("pgfsetlinewidth", Stroke::Width),
        ("pgfsetdash", Stroke::Dash),
        ("pgfsetarrows", Stroke::Arrows),
        ("pgfsetfillpattern", Stroke::Pattern),
        ("pgfsetcolor", Stroke::Color),
        ("pgfsetstrokecolor", Stroke::Color),
        ("pgfsetfillcolor", Stroke::Color),
        ("pgfutil@colorlet", Stroke::Color),
        ("pgfutil@doifcolorelse", Stroke::Color),
        // the color a node's text takes (tikz.code.tex `text`)
        ("tikz@textcolor", Stroke::Color),
        // tikz's `right of` and the like, and positioning's `right=of`
        ("tikz@of", Stroke::Node),
        ("tikz@lib@place@handle@", Stroke::NodeOf),
    ];
}

/// How many macros deep a key's code is followed to a setter.
const DEPTH: usize = 3;
/// Pixels per TeX point in a drawn preview.
const PX: f64 = 2.0;
/// The ink of every preview, readable on light and dark backgrounds.
const INK: &str = "#888888";
/// The width, in points, a dash pattern is drawn with when the run left `\pgflinewidth` unknown.
const UNKNOWN_WIDTH: f64 = 0.4;

/// A completion's picture: what it sets, as text, and the SVG.
pub struct Preview {
    pub text: String,
    pub svg: String,
}

impl Preview {
    /// The item's documentation: the text, then the picture as an image.
    pub fn markdown(&self) -> String {
        let uri = percent_encoding::utf8_percent_encode(&self.svg, percent_encoding::NON_ALPHANUMERIC);
        format!("{}\n\n![preview](data:image/svg+xml;utf8,{uri})", self.text)
    }
}

fn body(analysis: &Analysis, sym: Sym) -> Option<std::rc::Rc<crate::engine::tex::MacroDef>> {
    analysis.env.meaning(sym).as_macro().cloned()
}

/// The setters a macro's text reaches within `depth` expansions.
fn reaches(
    analysis: &Analysis,
    toks: &[crate::engine::tex::Token],
    depth: usize,
    seen: &mut BTreeSet<Sym>,
) -> Vec<Stroke> {
    let mut out = Vec::new();
    for sym in toks.iter().filter_map(|t| t.cs()) {
        if !seen.insert(sym) {
            continue;
        }
        let name = analysis.interner.name(sym);
        match Stroke::SETTERS.iter().find(|(n, _)| *n == name) {
            Some((_, s)) => out.push(*s),
            None if depth > 0 => {
                if let Some(m) = body(analysis, sym) {
                    out.extend(reaches(analysis, &m.replacement_text, depth - 1, seen));
                }
            }
            None => {}
        }
    }
    out
}

/// The stored code of a pgfkeys path.
fn code(analysis: &Analysis, path: &str) -> Option<std::rc::Rc<crate::engine::tex::MacroDef>> {
    body(analysis, analysis.interner.lookup(&format!("pgfk@{path}/.@cmd"))?)
}

/// The setters `path`'s code reaches.
pub fn sets(analysis: &Analysis, path: &str) -> Vec<Stroke> {
    code(analysis, path).map_or(Vec::new(), |m| reaches(analysis, &m.replacement_text, DEPTH, &mut BTreeSet::new()))
}

/// What setting `path` to `value` hands pgf's setters: a style's keys
/// followed, a code key's argument where its code reaches a setter.
pub fn strokes(analysis: &Analysis, path: &str, value: Option<&str>, depth: usize) -> Vec<(Stroke, String)> {
    let Some(m) = code(analysis, path) else { return Vec::new() };
    let text = crate::engine::tex::detokenize(&m.replacement_text, &analysis.interner);
    let default = analysis
        .interner
        .lookup(&format!("pgfk@{path}/.@def"))
        .and_then(|s| body(analysis, s))
        .map(|d| crate::engine::tex::detokenize(&d.replacement_text, &analysis.interner));
    let first = m.replacement_text.first().and_then(|t| t.cs()).map(|s| analysis.interner.name(s));
    let style = first == Some("pgfkeysalso");
    // A code key set without a value has nothing to show.
    let Some(value) = value.map(str::to_string).or(default).or_else(|| style.then(String::new)) else {
        return Vec::new();
    };
    if style {
        if depth == 0 {
            return Vec::new();
        }
        let text = text.replace("#1", &value);
        let list = text.split_once('{').map_or("", |(_, rest)| rest.rsplit_once('}').map_or(rest, |(l, _)| l));
        let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
        return items(list)
            .into_iter()
            .flat_map(|item| {
                let (key, v) = item.split_once('=').map_or((item, None), |(k, v)| (k, Some(v.trim())));
                let key = key.trim();
                let sub = if key.starts_with('/') { key.to_string() } else { format!("{dir}/{key}") };
                strokes(analysis, &sub, v, depth - 1)
            })
            .collect();
    }
    let reached = reaches(analysis, &m.replacement_text, DEPTH, &mut BTreeSet::new());
    reached.into_iter().map(|s| (s, value.clone())).collect()
}

/// A key list's items, split at its outer commas.
fn items(list: &str) -> Vec<&str> {
    let (mut out, mut depth, mut from) = (Vec::new(), 0u32, 0);
    for (i, c) in list.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(&list[from..i]);
                from = i + 1;
            }
            _ => {}
        }
    }
    out.push(&list[from..]);
    out.into_iter().filter(|s| !s.trim().is_empty()).collect()
}

/// The value `\pgflinewidth` holds when the run ended, in points.
fn line_width(analysis: &Analysis) -> Option<f64> {
    let sym = analysis.interner.lookup("pgflinewidth")?;
    let Meaning::Register(kind, n) = analysis.env.meaning(sym) else { return None };
    let reg = analysis.interner.lookup(&format!("\u{2}{}{n}", kind.as_str()))?;
    let (_, num) = analysis.env.num(reg)?;
    (num.lo == num.hi).then(|| num.lo as f64 / 65536.0)
}

/// A length in points: a dimension or the line width.
fn points(analysis: &Analysis, s: &str) -> Option<f64> {
    let s = s.trim().trim_start_matches("\\the").trim();
    match s {
        "\\pgflinewidth" => line_width(analysis),
        _ => crate::engine::value::parse_dimen(s).map(|sp| sp as f64 / 65536.0),
    }
}

fn pt(x: f64) -> String {
    format!("{}pt", (x * 100.0).round() / 100.0)
}

/// A line `w` points wide, dashed by `dash` (point lengths, on first).
fn line(w: f64, dash: &[f64]) -> String {
    let dash = match dash.is_empty() {
        true => String::new(),
        false => {
            let px: Vec<String> = dash.iter().map(|d| format!("{:.2}", d * PX)).collect();
            format!(r#" stroke-dasharray="{}""#, px.join(" "))
        }
    };
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="12" viewBox="0 0 64 12"><line x1="2" y1="6" x2="62" y2="6" stroke="{INK}" stroke-width="{:.2}"{dash}/></svg>"#,
        (w * PX).max(0.1)
    )
}

/// The picture of setting `path` to `value`: its width and dash drawn,
/// its arrow tips or pattern from TeX's drawing.
pub fn of_key(analysis: &Analysis, path: &str, value: Option<&str>) -> Option<Preview> {
    let found = strokes(analysis, path, value, DEPTH);
    let width = found.iter().find(|(s, _)| *s == Stroke::Width).and_then(|(_, v)| Some((v, points(analysis, v)?)));
    let dash = found.iter().find(|(s, _)| *s == Stroke::Dash).map(|(_, v)| v.clone());
    if let Some((s, v)) = found.iter().find(|(s, _)| matches!(s, Stroke::Arrows | Stroke::Pattern)) {
        return drawn(analysis, *s, v);
    }
    let w = width.as_ref().map(|(_, w)| *w);
    let mut text = Vec::new();
    if let Some((v, w)) = &width {
        text.push(format!("line width {}", if v.trim().ends_with("pt") { v.trim().to_string() } else { pt(*w) }));
    }
    let mut lengths = Vec::new();
    if let Some(dash) = &dash {
        // TikZ's `on ⟨len⟩ off ⟨len⟩ …`, as its scanner splits it.
        let words = dash.split("phase").next().unwrap_or_default();
        for part in words.split("on").flat_map(|p| p.split("off")).map(str::trim).filter(|p| !p.is_empty()) {
            lengths.push(points(analysis, part).or(w).or_else(|| line_width(analysis))?);
        }
        text.push(match dash.trim() {
            "" => "solid".into(),
            d => format!("dash pattern {d}"),
        });
    }
    if text.is_empty() {
        return None;
    }
    let w = w.or_else(|| line_width(analysis)).unwrap_or(UNKNOWN_WIDTH);
    Some(Preview { text: text.join(", "), svg: line(w, &lengths) })
}

/// The arrow specs the run's declared tips make: `-⟨end⟩` per end, and
/// `⟨start⟩-` per start, with its end where the two differ (`<->`).
pub fn arrow_specs(analysis: &Analysis) -> Vec<String> {
    let brace = |n: &str| if n.contains(char::is_whitespace) { format!("{{{n}}}") } else { n.to_string() };
    let usable = |n: &str| !n.is_empty() && !n.contains(['@', '{', '}', ',', '=', '[', ']']);
    let mut ends = BTreeSet::new();
    let mut starts = BTreeMap::new();
    for index in 0..analysis.interner.len() as u32 {
        let sym = Sym(index);
        let name = analysis.interner.name(sym);
        let Some(m) = (name.starts_with("pgf@ar@")).then(|| body(analysis, sym)).flatten() else { continue };
        if let Some(end) = name.strip_prefix("pgf@ar@code@").or_else(|| name.strip_prefix("pgf@ar@means@")) {
            ends.insert(end.to_string());
        } else if let Some(start) = name.strip_prefix("pgf@ar@start@") {
            starts.insert(start.to_string(), crate::engine::tex::detokenize(&m.replacement_text, &analysis.interner));
        }
    }
    let mut out: Vec<String> = ends.iter().filter(|e| usable(e)).map(|e| format!("-{}", brace(e))).collect();
    for (start, end) in starts.iter().filter(|(s, e)| usable(s) && ends.contains(*e)) {
        out.push(format!("{}-", brace(start)));
        if start != end && usable(end) {
            out.push(format!("{}-{}", brace(start), brace(end)));
        }
    }
    out
}

/// The fill patterns the run declared.
pub fn pattern_names(analysis: &Analysis) -> Vec<String> {
    let names = (0..analysis.interner.len() as u32).map(Sym).filter_map(|sym| {
        let name = analysis.interner.name(sym).strip_prefix("pgf@pattern@name@")?;
        let name = name.strip_prefix("meta@").unwrap_or(name);
        (analysis.env.is_defined(sym) && !name.is_empty() && !name.contains('@')).then(|| name.to_string())
    });
    names.collect::<BTreeSet<_>>().into_iter().collect()
}

/// A sprite's name in `previews.json`.
fn sprite(kind: Stroke, name: &str) -> String {
    format!("{}:{}", if kind == Stroke::Arrows { "arrow" } else { "pattern" }, name.trim())
}

#[derive(Default)]
struct Sheet {
    svgs: BTreeMap<String, String>,
    loaded: bool,
    busy: bool,
    failed: bool,
}

static SHEET: Mutex<Sheet> = Mutex::new(Sheet { svgs: BTreeMap::new(), loaded: false, busy: false, failed: false });

fn sheet_path() -> Option<std::path::PathBuf> {
    crate::caches::format::default_cache_dir().map(|d| d.join("previews.json"))
}

fn load(sheet: &mut Sheet) {
    if !sheet.loaded {
        sheet.loaded = true;
        let read = sheet_path().and_then(|p| std::fs::read(p).ok());
        sheet
            .svgs
            .extend(read.and_then(|b| serde_json::from_slice::<BTreeMap<String, String>>(&b).ok()).unwrap_or_default());
    }
}

/// The picture of a tip or pattern TeX drew. One the sheet lacks is drawn
/// in the background, with every other the run declared, for a later ask.
pub fn drawn(analysis: &Analysis, kind: Stroke, name: &str) -> Option<Preview> {
    let key = sprite(kind, name);
    let mut sheet = SHEET.lock().ok()?;
    load(&mut sheet);
    match sheet.svgs.get(&key) {
        Some(svg) if !svg.is_empty() => {
            let what = if kind == Stroke::Arrows { "arrows" } else { "pattern" };
            Some(Preview { text: format!("{what}={}", name.trim()), svg: svg.clone() })
        }
        Some(_) => None,
        None => {
            if !cfg!(target_os = "emscripten") && !sheet.busy && !sheet.failed {
                sheet.busy = true;
                let missing = wanted(analysis).into_iter().filter(|w| !sheet.svgs.contains_key(w));
                let (libs, wanted): (_, Vec<String>) = (libraries(analysis), missing.collect());
                std::thread::spawn(move || {
                    let svgs = render(&libs, &wanted);
                    if let Ok(mut sheet) = SHEET.lock() {
                        sheet.busy = false;
                        sheet.failed = svgs.is_none();
                        sheet.svgs.extend(svgs.unwrap_or_default());
                        save(&sheet.svgs);
                    }
                });
            }
            None
        }
    }
}

fn save(svgs: &BTreeMap<String, String>) {
    if let (Some(path), Ok(json)) = (sheet_path(), serde_json::to_vec(svgs)) {
        let _ = path.parent().map(std::fs::create_dir_all);
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}

/// Every tip and pattern the run declared, as sprite names.
fn wanted(analysis: &Analysis) -> Vec<String> {
    let arrows = arrow_specs(analysis).into_iter().map(|s| sprite(Stroke::Arrows, &s));
    arrows.chain(pattern_names(analysis).into_iter().map(|p| sprite(Stroke::Pattern, &p))).collect()
}

/// The pgf and TikZ libraries the run loaded, by their files' names.
fn libraries(analysis: &Analysis) -> Vec<String> {
    let mut out = BTreeSet::new();
    for info in &analysis.files {
        let file = info.path.rsplit(['/', '\\']).next().unwrap_or_default();
        let Some(stem) = file.strip_suffix(".code.tex") else { continue };
        if let Some(lib) = stem.strip_prefix("tikzlibrary") {
            out.insert(format!("\\usetikzlibrary{{{lib}}}"));
        } else if let Some(lib) = stem.strip_prefix("pgflibrary") {
            out.insert(format!("\\usepgflibrary{{{lib}}}"));
        }
    }
    out.into_iter().collect()
}

/// Every sprite of `previews.json` the run needs, drawn now: what the
/// `previews` query gives and the site ships for the playground.
pub fn sheet(analysis: &Analysis) -> BTreeMap<String, String> {
    let wanted = wanted(analysis);
    let Ok(mut sheet) = SHEET.lock() else { return BTreeMap::new() };
    load(&mut sheet);
    let missing: Vec<String> = wanted.iter().filter(|w| !sheet.svgs.contains_key(*w)).cloned().collect();
    if !missing.is_empty() {
        sheet.svgs.extend(render(&libraries(analysis), &missing).unwrap_or_default());
        save(&sheet.svgs);
    }
    wanted.into_iter().filter_map(|w| Some((w.clone(), sheet.svgs.get(&w)?.clone()))).collect()
}

/// Draws each sprite on its own page with `latex` and `dvisvgm`, `None`
/// when they cannot run. A sprite TeX could not draw maps to nothing.
fn render(libs: &[String], wanted: &[String]) -> Option<BTreeMap<String, String>> {
    let dir = crate::caches::format::default_cache_dir()?.join(format!("previews-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let pictures: Vec<String> = wanted
        .iter()
        .map(|w| match w.split_once(':') {
            Some(("arrow", spec)) => format!("\\tikz\\draw[ink,thick,{spec}] (0,0) -- (0.9,0);"), // not prose
            Some((_, name)) => {
                format!("\\tikz\\path[pattern={{{name}}},pattern color=ink] (0,0) rectangle (0.9,0.45);") // not prose
            }
            None => String::new(),
        })
        .collect();
    let tex = format!(
        "\\def\\pgfsysdriver{{pgfsys-dvisvgm.def}}\\documentclass{{article}}\\usepackage{{tikz}}\n{}\n\\pagestyle{{empty}}\\definecolor{{ink}}{{HTML}}{{{}}}\n\\begin{{document}}\n{}\n\\end{{document}}\n",
        libs.join("\n"),
        INK.trim_start_matches('#'),
        // Each page names its sprite: a library's error may ship a page of its own.
        pictures
            .iter()
            .enumerate()
            .map(|(i, p)| format!("\\clearpage\\mbox{{\\special{{dvisvgm:raw <!--sprite {i}-->}}{p}}}"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    std::fs::write(dir.join("previews.tex"), tex).ok()?;
    let run = |program: &str, args: &[&str]| std::process::Command::new(program).args(args).current_dir(&dir).output();
    run("latex", &["-interaction=batchmode", "previews.tex"]).ok()?;
    let done = run("dvisvgm", &["--no-fonts", "--exact-bbox", "--page=1-", "--output=p%p.svg", "previews.dvi"]);
    if !done.is_ok_and(|o| o.status.success()) {
        let _ = std::fs::remove_dir_all(&dir);
        return None;
    }
    static MARK: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let mark = MARK.get_or_init(|| regex::Regex::new(r"<!--sprite (\d+)-->").unwrap());
    // At the scale of the drawn lines: `PX` pixels per point.
    static SIZE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let size = SIZE.get_or_init(|| regex::Regex::new(r"(width|height)='([0-9.]+)pt'").unwrap());
    let mut svgs: BTreeMap<String, String> = wanted.iter().map(|w| (w.clone(), String::new())).collect();
    for entry in std::fs::read_dir(&dir).ok()?.flatten() {
        let Ok(svg) = std::fs::read_to_string(entry.path()) else { continue };
        let n = mark.captures(&svg).and_then(|c| c[1].parse::<usize>().ok());
        let (Some(at), Some(w)) = (svg.find("<svg"), n.and_then(|n| wanted.get(n))) else { continue };
        let svg = mark.replace(svg[at..].trim(), "");
        let svg = size.replace_all(&svg, |c: &regex::Captures| {
            format!("{}='{:.2}'", &c[1], c[2].parse::<f64>().unwrap_or_default() * PX)
        });
        svgs.insert(w.clone(), svg.into_owned());
    }
    let _ = std::fs::remove_dir_all(&dir);
    Some(svgs)
}
