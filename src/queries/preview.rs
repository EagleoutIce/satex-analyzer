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
    /// A node shape, drawn by TeX as a sample node.
    Shape,
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
/// The text box (cm) a shape sample is drawn around. A box, not `minimum width`/`height` on no text: those
/// collapse a `trapezium` into a line and stand a `single arrow` upright (its height is its length).
pub const SAMPLE_W: f64 = 1.0;
pub const SAMPLE_H: f64 = 0.5;
/// The line an arrow sprite draws its tips on, in cm.
const ARROW_W: f64 = 0.9;
/// Room around an arrow sprite's line, in cm, half of it above and below: more than a thick line's tips reach
/// (`diamond` 0.28 cm back, 0.09 cm up).
const TIP_ROOM: f64 = 0.3;
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

/// The node shapes the run declared: pgf defines `pgf@sh@s@⟨name⟩` for each.
pub fn shape_names(analysis: &Analysis) -> Vec<String> {
    let names = (0..analysis.interner.len() as u32).map(Sym).filter_map(|sym| {
        let name = analysis.interner.name(sym).strip_prefix("pgf@sh@s@")?;
        (analysis.env.is_defined(sym) && !name.is_empty() && !name.contains('@')).then(|| name.to_string())
    });
    names.collect::<BTreeSet<_>>().into_iter().collect()
}

/// The shapes the installation's libraries declare that the run lacks, each with its load.
pub fn library_shapes(analysis: &Analysis) -> Vec<(String, String)> {
    let have = shape_names(analysis);
    let usable = |n: &str| !n.is_empty() && !n.contains(['#', '\\', '@']);
    let mut out = crate::linter::shared::library_declared(analysis, "shape");
    out.retain(|(n, _)| usable(n) && !have.contains(n));
    out.sort();
    out
}

/// A sprite's name in `previews.json`.
fn sprite(kind: Stroke, name: &str) -> String {
    let kind = match kind {
        Stroke::Arrows => "arrow",
        Stroke::Shape => "shape",
        _ => "pattern",
    };
    format!("{kind}:{}", name.trim())
}

#[derive(Default)]
struct Sheet {
    svgs: BTreeMap<String, String>,
    /// The file the sheet was read from, once read: the installation it was drawn with.
    loaded: Option<Option<std::path::PathBuf>>,
    busy: bool,
    failed: bool,
}

static SHEET: Mutex<Sheet> = Mutex::new(Sheet { svgs: BTreeMap::new(), loaded: None, busy: false, failed: false });

/// The sheet of the installation `analysis` ran on: TeX draws the sprites,
/// so one drawn by another release (its tips, its `dvisvgm`) is not this one's.
fn sheet_path(analysis: &Analysis) -> Option<std::path::PathBuf> {
    // `-v3`: shapes drawn around a text box; an older sheet holds cut-off tips or collapsed shapes
    let name = format!("previews-v3-{:016x}.json", installation_key(analysis));
    crate::caches::format::default_cache_dir().map(|d| d.join(name))
}

/// The installation a sprite is drawn with: its trees, release and format,
/// and the `dvisvgm` on `PATH` ([`installation_hash`]).
fn installation_key(analysis: &Analysis) -> u64 {
    static DVISVGM: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let dvisvgm = DVISVGM.get_or_init(|| {
        let out = std::process::Command::new("dvisvgm").arg("--version").output();
        out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_default()
    });
    let d = &analysis.distribution;
    installation_hash(&d.roots, d.year, d.format_version.as_deref(), dvisvgm)
}

/// [`installation_key`] of its parts, hashed (FNV-1a) the same in every process.
fn installation_hash(roots: &[std::path::PathBuf], year: Option<u32>, format: Option<&str>, dvisvgm: &str) -> u64 {
    let roots: Vec<String> = roots.iter().map(|r| r.display().to_string()).collect();
    let text = format!("{}\n{year:?}\n{format:?}\n{dvisvgm}", roots.join("\n"));
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

/// The sheet of `analysis`'s installation, read once; another
/// installation's is set aside.
fn load(sheet: &mut Sheet, analysis: &Analysis) {
    let path = sheet_path(analysis);
    if sheet.loaded.as_ref() != Some(&path) {
        *sheet = Sheet { loaded: Some(path.clone()), ..Sheet::default() };
        // else the sheet a site build drew (`previews.json`, the playground's): no TeX draws one in the browser
        let read = |p: std::path::PathBuf| {
            Some(read_sheet(&p))
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| read_sheet(&p.with_file_name("previews.json")))
        };
        sheet.svgs = path.map(read).unwrap_or_default();
    }
}

/// The sprites drawn in the sheet at `path`. One TeX could not draw is never
/// read back: a later run tries it again.
fn read_sheet(path: &std::path::Path) -> BTreeMap<String, String> {
    let read = std::fs::read(path).ok();
    let svgs = read.and_then(|b| serde_json::from_slice::<BTreeMap<String, String>>(&b).ok()).unwrap_or_default();
    svgs.into_iter().filter(|(_, svg)| !svg.is_empty()).collect()
}

/// The picture of a tip or pattern TeX drew ([`sprite_of`]).
pub fn drawn(analysis: &Analysis, kind: Stroke, name: &str) -> Option<Preview> {
    let svg = sprite_of(analysis, &sprite(kind, name))?;
    let what = match kind {
        Stroke::Arrows => "arrows",
        Stroke::Shape => "shape",
        _ => "pattern",
    };
    Some(Preview { text: format!("{what}={}", name.trim()), svg })
}

/// The sample of offered font `name` (`type_plugins.font.fonts`) TeX set ([`sprite_of`]).
pub fn font_sample(analysis: &Analysis, name: &str) -> Option<String> {
    sprite_of(analysis, &font_sprite(name))
}

/// The sprite `key` of the sheet. One the sheet lacks is drawn in the
/// background, with every other the run wants, for a later ask.
fn sprite_of(analysis: &Analysis, key: &str) -> Option<String> {
    let mut sheet = SHEET.lock().ok()?;
    load(&mut sheet, analysis);
    match sheet.svgs.get(key) {
        Some(svg) if !svg.is_empty() => Some(svg.clone()),
        Some(_) => None,
        None => {
            if !cfg!(target_os = "emscripten") && !sheet.busy && !sheet.failed {
                sheet.busy = true;
                let mut plan = wanted(analysis);
                plan.names.retain(|w| !sheet.svgs.contains_key(w));
                let path = sheet.loaded.clone().flatten();
                std::thread::spawn(move || {
                    let svgs = render(&plan);
                    if let Ok(mut sheet) = SHEET.lock()
                        && sheet.loaded.as_ref() == Some(&path)
                    {
                        sheet.busy = false;
                        sheet.failed = svgs.is_none();
                        sheet.svgs.extend(svgs.unwrap_or_default());
                        save(path.as_deref(), &sheet.svgs);
                    }
                });
            }
            None
        }
    }
}

/// The sheet written to `path`, without the sprites TeX did not draw: a
/// failure is this process's, never a later one's.
fn save(path: Option<&std::path::Path>, svgs: &BTreeMap<String, String>) {
    let drawn: BTreeMap<&String, &String> = svgs.iter().filter(|(_, svg)| !svg.is_empty()).collect();
    if let (Some(path), Ok(json)) = (path, serde_json::to_vec(&drawn)) {
        let _ = path.parent().map(std::fs::create_dir_all);
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(tmp, path);
        }
    }
}

/// The sprites a sheet holds and what draws them: the TikZ loads, and each font sprite's preamble.
struct Plan {
    names: Vec<String>,
    libs: Vec<String>,
    fonts: BTreeMap<String, String>,
}

/// The text a font's sample sets.
const FONT_SAMPLE: &str = "Aa Bb 123";

/// A font sample's sprite name in `previews.json`: `font:Libertinus`.
fn font_sprite(name: &str) -> String {
    format!("font:{name}")
}

/// Every tip, pattern and shape the run declared, every shape the installation's libraries
/// declare and every offered font the installation has a package of, as sprite names, with
/// what draws them. A font is drawn by its pdfLaTeX package, as `type_plugins.font` writes it.
fn wanted(analysis: &Analysis) -> Plan {
    let more = library_shapes(analysis);
    let mut libs = libraries(analysis);
    libs.extend(more.iter().map(|(_, l)| l.clone()).filter(|l| !libs.contains(l)).collect::<BTreeSet<_>>());
    let arrows = arrow_specs(analysis).into_iter().map(|s| sprite(Stroke::Arrows, &s));
    let patterns = pattern_names(analysis).into_iter().map(|p| sprite(Stroke::Pattern, &p));
    let shapes = shape_names(analysis).into_iter().chain(more.into_iter().map(|(n, _)| n));
    let fonts: BTreeMap<String, String> = (analysis.settings.type_plugins.font.fonts.iter())
        .filter_map(|(name, spec)| {
            let package = spec.first().filter(|p| crate::linter::shared::installed_package(analysis, p))?;
            let sans = spec.get(2).is_some_and(|s| s == "sans").then_some("\\renewcommand\\familydefault\\sfdefault");
            let preamble = format!("\\usepackage[T1]{{fontenc}}\\usepackage{{{package}}}{}", sans.unwrap_or_default());
            Some((font_sprite(name), preamble))
        })
        .collect();
    let names = arrows.chain(patterns).chain(shapes.map(|s| sprite(Stroke::Shape, &s))).chain(fonts.keys().cloned());
    Plan { names: names.collect(), libs, fonts }
}

/// The pgf and TikZ libraries the run loaded, by their files' names.
pub fn libraries(analysis: &Analysis) -> Vec<String> {
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
    let mut plan = wanted(analysis);
    let Ok(mut sheet) = SHEET.lock() else { return BTreeMap::new() };
    load(&mut sheet, analysis);
    let wanted = plan.names.clone();
    plan.names.retain(|w| !sheet.svgs.contains_key(w));
    if !plan.names.is_empty() {
        sheet.svgs.extend(render(&plan).unwrap_or_default());
        save(sheet.loaded.clone().flatten().as_deref(), &sheet.svgs);
    }
    wanted.into_iter().filter_map(|w| Some((w.clone(), sheet.svgs.get(&w)?.clone()))).collect()
}

/// Draws each sprite of `plan` on its own page with `latex` and `dvisvgm`, `None`
/// when they cannot run. A sprite TeX could not draw maps to nothing. The TikZ
/// sprites share a document, each font sample has one of its own (its package's).
fn render(plan: &Plan) -> Option<BTreeMap<String, String>> {
    let dir = crate::caches::format::default_cache_dir()?.join(format!("previews-{}", std::process::id()));
    let (fonts, wanted): (Vec<String>, Vec<String>) =
        plan.names.iter().cloned().partition(|w| plan.fonts.contains_key(w));
    let pictures: Vec<String> = wanted
        .iter()
        .map(|w| match w.split_once(':') {
            // a tip declared the old way (`\pgfarrowsdeclare`: the `arrows` library, `hooks`, `latex'`) adds nothing to the
            // picture's bounding box, so the sprite cut it off: the points around the line keep room for any tip
            Some(("arrow", spec)) => format!(
                "\\tikz{{\\path (-{TIP_ROOM}cm,-{h}cm) ({}cm,{h}cm);\\draw[ink,thick,{spec}] (0,0) -- ({ARROW_W}cm,0);}}", // not prose
                ARROW_W + TIP_ROOM,
                h = TIP_ROOM / 2.0
            ),
            // around a box twice as wide as high: a shape that keeps its own proportions shows it
            Some(("shape", name)) => format!(
                "\\tikz\\node[draw=ink,fill=ink,fill opacity=.15,shape={{{name}}},inner sep=0pt] {{\\rule{{{SAMPLE_W}cm}}{{0pt}}\\rule{{0pt}}{{{SAMPLE_H}cm}}}};" // not prose
            ),
            Some((_, name)) => {
                format!("\\tikz\\path[pattern={{{name}}},pattern color=ink] (0,0) rectangle (0.9,0.45);") // not prose
            }
            None => String::new(),
        })
        .collect();
    let tikz = format!(
        "\\def\\pgfsysdriver{{pgfsys-dvisvgm.def}}\\documentclass{{article}}\\usepackage{{tikz}}\n{}",
        plan.libs.join("\n")
    );
    let mut svgs = BTreeMap::new();
    if !wanted.is_empty() {
        svgs.extend(draw(&dir, &tikz, &wanted, &pictures)?);
    }
    for (i, font) in fonts.iter().enumerate() {
        let preamble = format!("\\documentclass{{article}}\\usepackage{{xcolor}}{}", plan.fonts[font]);
        let picture = format!("\\mbox{{\\color{{ink}}{FONT_SAMPLE}}}");
        svgs.extend(
            draw(&dir.join(format!("font-{i}")), &preamble, std::slice::from_ref(font), &[picture]).unwrap_or_default(),
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
    Some(svgs)
}

/// Each of `pictures` on a page of its own after `preamble`, in `dir`, as the sprites `wanted`.
fn draw(
    dir: &std::path::Path,
    preamble: &str,
    wanted: &[String],
    pictures: &[String],
) -> Option<BTreeMap<String, String>> {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).ok()?;
    let tex = format!(
        "{preamble}\n\\pagestyle{{empty}}\\definecolor{{ink}}{{HTML}}{{{}}}\n\\begin{{document}}\n{}\n\\end{{document}}\n",
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
    let run = |program: &str, args: &[&str]| std::process::Command::new(program).args(args).current_dir(dir).output();
    run("latex", &["-interaction=batchmode", "previews.tex"]).ok()?;
    // a page drawn: dvisvgm 2.6 (TeX Live 2019) ends well on an option it lacks
    let drawn =
        || std::fs::read_dir(dir).is_ok_and(|d| d.flatten().any(|e| e.path().extension() == Some("svg".as_ref())));
    let svg = |exact: &[&str]| {
        let args = [exact, &["--no-fonts", "--page=1-", "--output=p%p.svg", "previews.dvi"]].concat();
        run("dvisvgm", &args).is_ok_and(|o| o.status.success()) && drawn()
    };
    // that dvisvgm has no `--exact-bbox`; glyph-free sprites need none
    if !svg(&["--exact-bbox"]) && !svg(&[]) {
        return None;
    }
    static MARK: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let mark = MARK.get_or_init(|| regex::Regex::new(r"<!--sprite (\d+)-->").unwrap());
    // At the scale of the drawn lines: `PX` pixels per point.
    static SIZE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let size = SIZE.get_or_init(|| regex::Regex::new(r"(width|height)='([0-9.]+)pt'").unwrap());
    let mut svgs: BTreeMap<String, String> = wanted.iter().map(|w| (w.clone(), String::new())).collect();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let Ok(svg) = std::fs::read_to_string(entry.path()) else { continue };
        let n = mark.captures(&svg).and_then(|c| c[1].parse::<usize>().ok());
        let (Some(at), Some(w)) = (svg.find("<svg"), n.and_then(|n| wanted.get(n))) else { continue };
        let svg = mark.replace(svg[at..].trim(), "");
        let svg = size.replace_all(&svg, |c: &regex::Captures| {
            format!("{}='{:.2}'", &c[1], c[2].parse::<f64>().unwrap_or_default() * PX)
        });
        svgs.insert(w.clone(), svg.into_owned());
    }
    Some(svgs)
}

/// The font size, in px, a formula preview is drawn at.
const FORMULA_PX: f64 = 18.0;

/// What the run set for the formula whose source stands between `from` and `to` (line, column)
/// in `file`: the characters its text facts hold there (macros as the run expanded them) with
/// their scripts, as text and SVG. A character the run could not know is a `?`; one only some
/// paths set is faint. `None` where the run set no math there.
pub fn formula(
    analysis: &Analysis,
    file: crate::engine::tex::FileId,
    from: (u32, u32),
    to: (u32, u32),
) -> Option<Preview> {
    use crate::analysis::facts::Align;
    use std::fmt::Write;
    let text = &analysis.facts.text;
    let n = text.site.len().min(text.at.len());
    let inside =
        |i: &usize| text.site[*i].file == file && (from..=to).contains(&(text.site[*i].line, text.site[*i].col));
    let mut own: Vec<usize> = (0..n).filter(inside).collect();
    // a measurement (amsmath sets an `align` twice) sets text in boxes lost unused
    let (on_page, lost) = super::query::reach(text);
    own.retain(|&i| !lost[i]);
    if own.iter().any(|&i| on_page[i]) {
        own.retain(|&i| on_page[i]);
    }
    let is_math = |i: usize| text.math.binary_search_by_key(&(i as u32), |m| m.0).is_ok();
    // what a macro's body set counts at its definition: the formula is all set from its first
    // character here to its last, and the math right before and after (`\norm`'s `‖`)
    let (mut lo, mut hi) = (*own.first()?, *own.last()?);
    while lo > 0 && is_math(lo - 1) && is_math(lo) {
        lo -= 1;
    }
    while hi + 1 < n && is_math(hi + 1) && is_math(hi) {
        hi += 1;
    }
    let own: Vec<usize> = (lo..=hi).filter(|&i| !lost[i] && on_page[i] == on_page[own[0]]).collect();
    let math = own.iter().filter(|&&i| is_math(i)).count();
    if math == 0 || math * 2 < own.len() {
        return None;
    }
    let (first, last) = (own[0], own[own.len() - 1]);
    let atoms = super::query::math_atoms(text, &analysis.settings.conventions.math_symbols, first..last + 1);
    let chars: Vec<char> = text.chars.chars().skip(first).take(last + 1 - first).collect();
    let arms: BTreeSet<u32> = text.arms.iter().map(|a| a.0).collect();
    let mut marks = text.align.iter().skip_while(|(at, _)| (*at as usize) < first).peekable();
    // each script open now: superscript or not, and where its text starts in `plain`
    let (mut plain, mut spans, mut scripts) = (String::new(), String::new(), Vec::<(bool, usize)>::new());
    let close = |plain: &mut String, scripts: &mut Vec<(bool, usize)>| {
        let Some((_, at)) = scripts.pop() else { return };
        // a one-character script reads without braces
        match plain[at..].chars().count() == 1 {
            true => _ = plain.remove(at - 1),
            false => plain.push('}'),
        }
    };
    for &i in &own {
        while let Some((_, mark)) = marks.next_if(|(at, _)| *at as usize <= i) {
            match mark {
                Align::Script(sup) => {
                    plain.push_str(if *sup { "^{" } else { "_{" });
                    scripts.push((*sup, plain.len()));
                }
                Align::ScriptEnd => close(&mut plain, &mut scripts),
                _ => {}
            }
        }
        let c = chars[i - first];
        // math sets no spaces (tex.web § 1045)
        if c == ' ' && is_math(i) {
            continue;
        }
        let (tag, glyph) = atoms.get(&i).copied().unwrap_or(('0', None));
        let unknown = crate::engine::tex::is_unknown_char(c);
        let shown = if unknown { "?".to_string() } else { super::query::public(glyph.unwrap_or(c)) };
        plain.push_str(if unknown { "⟨?⟩" } else { &shown });
        let size = FORMULA_PX * 0.7f64.powi(scripts.len().min(2) as i32);
        let rise: f64 = scripts.iter().map(|&(sup, _)| if sup { 0.4 } else { -0.25 }).sum::<f64>() * FORMULA_PX;
        let faint = unknown || arms.contains(&(i as u32));
        let _ = write!(
            spans,
            "<tspan y=\"{:.1}\" font-size=\"{size:.1}\"{}{}>{}</tspan>",
            FORMULA_PX * 1.6 - rise,
            if tag == 'i' { " font-style=\"italic\"" } else { "" },
            if faint { " opacity=\"0.45\"" } else { "" },
            shown.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
        );
    }
    while !scripts.is_empty() {
        close(&mut plain, &mut scripts);
    }
    let width = 8.0 + FORMULA_PX * 0.62 * plain.chars().count() as f64;
    let height = FORMULA_PX * 2.4;
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.0}\" height=\"{height:.0}\"><text x=\"4\" fill=\"{INK}\" font-family=\"Latin Modern Math, STIX Two Math, Cambria Math, serif\" xml:space=\"preserve\">{spans}</text></svg>"
    );
    Some(Preview { text: plain, svg })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sheet is the installation's: another TeX Live release, format or
    /// `dvisvgm` draws its sprites anew. Pins one sheet shared by all, whose
    /// sprite an older release could not draw stayed undrawn after an upgrade.
    #[test]
    fn each_installation_has_its_own_sheet() {
        let roots = [std::path::PathBuf::from("/texlive/2022/texmf-dist")];
        let key = |year, dvisvgm| installation_hash(&roots, Some(year), Some("2022-06-01"), dvisvgm);
        assert_eq!(key(2022, "dvisvgm 2.13"), key(2022, "dvisvgm 2.13"));
        assert_ne!(key(2022, "dvisvgm 2.13"), key(2025, "dvisvgm 2.13"));
        assert_ne!(key(2022, "dvisvgm 2.13"), key(2022, "dvisvgm 3.4"));
    }

    /// A sprite TeX did not draw (`""`) is this process's failure: it is
    /// neither written nor read back, so a later run draws it again.
    #[test]
    fn an_undrawn_sprite_is_not_kept() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/scratch/preview-sheet-test");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("sheet-{}.json", std::process::id()));
        let svgs =
            BTreeMap::from([("arrow:a".to_string(), "<svg/>".to_string()), ("arrow:b".to_string(), String::new())]);
        save(Some(&path), &svgs);
        assert_eq!(read_sheet(&path).keys().collect::<Vec<_>>(), ["arrow:a"]);
        std::fs::write(&path, r#"{"arrow:a":"<svg/>","arrow:c":""}"#).unwrap();
        assert_eq!(read_sheet(&path).keys().collect::<Vec<_>>(), ["arrow:a"]);
        let _ = std::fs::remove_file(&path);
    }
}
