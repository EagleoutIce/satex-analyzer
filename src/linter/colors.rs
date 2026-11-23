//! Colors xcolor stores and reads through `\color@⟨name⟩` (xcolor § 2.5),
//! and the `\pdfcolorstack`s the run executed (pdfTeX manual § 8.9.8), as
//! [`crate::analysis::observe`] recorded them.

use crate::fixed::HashMap;

use crate::analysis::facts::{ColorDef, CsnameRole, StackOp};
use crate::analysis::machine::Analysis;
use crate::engine::tex::Span;
use crate::linter::fix::{Applicability, Fix};
use crate::linter::{Category, Ref, Report, Rule, Severity, Source};
use crate::project::config::Engine;
use crate::text::{Edit, Pos};
use crate::util::color::{Rgb, driver_rgb, hex, specs};

pub static RULES: &[Rule] = &[
    Rule {
        code: "undefined-color",
        since: "0.2.0",
        category: Category::Correctness,
        severity: Severity::Error,
        summary: "color used before it is defined",
        explanation: "\
xcolor reaches every color through `\\color@⟨name⟩` (xcolor § 2.5). A name
the run never defined there raises `Undefined color` when xcolor itself
looks it up.  The rule reads the same lookup the run made: a name a
`\\color`, `\\textcolor` or `\\colorlet` reached while nothing had defined
it.

Fix by defining the color first, or by correcting its name.",
        references: &[Ref { source: Source::Xcolor, at: "2.5" }],
        fixes: &[],
        engines: &[],
        run: undefined_color,
    },
    Rule {
        code: "unused-color",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Info,
        summary: "color never used",
        explanation: "\
The document names a color with `\\definecolor`, `\\providecolor` or
`\\colorlet` and never reads it back through `\\color`, `\\textcolor` or a
`\\colorlet` source.

Silent for a color a package defines: a library may define more colors
than any one document uses.  So is a name with `@`, which code built and no
document text wrote, and xcolor's `.` for the current color.",
        references: &[Ref { source: Source::Xcolor, at: "2.5" }],
        fixes: &[],
        engines: &[],
        run: unused_color,
    },
    Rule {
        code: "conflicting-color-definition",
        since: "0.2.0",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "color name defined twice",
        explanation: "\
Two `\\definecolor`-family calls store different colors under the same
name. Only the last one is ever read back.  Silent where the first is a
package's: a default the document recolors on purpose.

Fix by giving the second color its own name, or by removing the earlier
definition.",
        references: &[Ref { source: Source::Xcolor, at: "2.5" }],
        fixes: &[],
        engines: &[],
        run: conflicting_color_definition,
    },
    Rule {
        code: "duplicate-color-value",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Info,
        summary: "two names for one color",
        explanation: "\
Two `\\definecolor`-family calls, the later one the document's own, store
the same model and components under different names.  Not wrong, but one
name likely means to be the other.

Silent unless the run could read both colors back exactly, and for a
`\\colorlet` given exactly another color's name: an alias, such as a role
named after the color it takes.",
        references: &[Ref { source: Source::Xcolor, at: "2.5" }],
        fixes: &[],
        engines: &[],
        run: duplicate_color_value,
    },
    Rule {
        code: "unbalanced-color-stack",
        since: "0.2.0",
        category: Category::Correctness,
        severity: Severity::Warning,
        summary: "unbalanced color stack",
        explanation: "\
`\\pdfcolorstack⟨stack⟩ push` and `pop` nest like `q`/`Q` in the page's own
content stream (pdfTeX manual, § 8.9.8): a viewer that reads the stack back
after the page still expects it empty.  A stack number the run could not
decide is left out, since satex would otherwise be guessing which stack a
push or pop belongs to.

Fix by balancing every push with a pop in the same group.",
        references: &[Ref { source: Source::Pdftex, at: "8.9.8" }],
        fixes: &[],
        engines: &[Engine::PdfTeX, Engine::LuaTeX],
        run: unbalanced_color_stack,
    },
    Rule {
        code: "low-contrast-color",
        since: "0.2.5",
        category: Category::Style,
        severity: Severity::Warning,
        summary: "text color with too little contrast",
        explanation: "\
Text needs a contrast ratio of at least 4.5:1 against its background to be
readable for people with low vision (WCAG 2, success criterion 1.4.3).
Large text needs 3:1: from 18pt, or 14pt in bold.  The rule reads the color
each character was set in, as the run computed it.  The background is the
page, `\\pagecolor`, or the `\\colorbox` the character sits in.  Text over
something satex does not read, such as a TikZ fill or a page background
drawn at shipout, is left alone.

The fix darkens or lightens the color in its hue until it reaches the
ratio.  It rewrites the document's own `\\definecolor` where there is one.
The limits are `lints.low-contrast-color`.",
        references: &[Ref { source: Source::Wcag, at: "1.4.3" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: low_contrast_color,
    },
    Rule {
        code: "indistinct-colors",
        since: "0.2.5",
        category: Category::Style,
        severity: Severity::Info,
        summary: "colors alike with color blindness",
        explanation: "\
Two colors the document uses side by side look alike to readers with a
color vision deficiency.  Side by side means in one environment below the
document, such as a figure or a TikZ picture, or as text colors in one
paragraph.  satex simulates protanopia, deuteranopia and tritanopia with
the matrices of Machado, Oliveira and Fernandes (2009).  It then measures
the distance of the two colors in CIELAB.  A pair that differs clearly for
normal vision but comes closer than `lints.indistinct-colors.below` under a
simulation is reported, with the deficiency named.

The fix suggests a color of the Okabe-Ito palette, which stays apart under
all three.  It rewrites the document's own `\\definecolor` where there is
one.  Telling the meanings apart by more than color also helps: line
styles, marks or labels.",
        references: &[Ref { source: Source::Wcag, at: "1.4.1" }],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: indistinct_colors,
    },
];

/// Whether color-definition findings apply here: a document defines
/// colors for its own prose, but so does a package meant to be loaded by
/// one. Only the profiles [`crate::linter::text`] also treats as code stay
/// silent.
fn definitions_matter(analysis: &Analysis) -> bool {
    !matches!(analysis.settings.profile, Some(crate::project::config::Profile::Literate))
}

/// A name no document defines itself: one with `@`, which code built, or xcolor's `.`, the
/// current color every `\color` sets.  No finding names it.
fn internal(name: &str) -> bool {
    name.contains('@') || name == "."
}

fn undefined_color(report: &mut Report) {
    let analysis = report.analysis();
    if !definitions_matter(analysis) {
        return;
    }
    // xcolor looks a whole expression (`green!50!blue`) up as a name before it parses it: only a
    // lookup the run answered with an error at that call was a color that does not exist
    let raised: Vec<_> =
        (analysis.facts.occurrences.iter()).filter(|o| crate::linter::shared::is_error(analysis, o)).collect();
    // Another error on the line says nothing about this color: the error must name it.
    let failed = |at: &crate::engine::tex::Span, name: &str| {
        raised.iter().any(|e| (e.span.file, e.span.line) == (at.file, at.line) && e.key.contains(name))
    };
    for use_ in &analysis.facts.color_uses {
        // A `\ifx`/`\@ifundefinedcolor`-style test deliberately asks
        // whether the color exists. Only reading or aliasing it assumes
        // that it does.
        if use_.defined || matches!(use_.role, CsnameRole::Test) || !failed(&use_.span, &use_.name) {
            continue;
        }
        let severity = if use_.certain { Severity::Error } else { Severity::Warning };
        report.add_as(
            severity,
            use_.span,
            &use_.name,
            format!("color `{}` is used but never defined", use_.name),
            None,
        );
    }
}

fn unused_color(report: &mut Report) {
    let analysis = report.analysis();
    if !definitions_matter(analysis) {
        return;
    }
    let used: crate::fixed::HashSet<&str> = analysis
        .facts
        .color_uses
        .iter()
        .filter(|u| matches!(u.role, CsnameRole::Use | CsnameRole::LetTo))
        .map(|u| u.name.as_str())
        .collect();
    for def in &analysis.facts.colors {
        if def.package.is_some() || internal(&def.shown) || used.contains(def.name.as_str()) {
            continue;
        }
        report.add_absent("used", def.span, &def.shown, format!("color `{}` is never used", def.shown), None);
    }
}

fn conflicting_color_definition(report: &mut Report) {
    let analysis = report.analysis();
    if !definitions_matter(analysis) {
        return;
    }
    let mut by_name: HashMap<&str, Vec<&ColorDef>> = HashMap::default();
    for def in &analysis.facts.colors {
        by_name.entry(def.name.as_str()).or_default().push(def);
    }
    for defs in by_name.into_values() {
        for pair in defs.windows(2) {
            let [first, second] = pair else { continue };
            let (Some(a), Some(b)) = (&first.value, &second.value) else { continue };
            // A package's color is a default the document may recolor.
            if !first.certain || !second.certain || a == b || first.package.is_some() || internal(&second.shown) {
                continue;
            }
            let name = &second.shown;
            report.add(
                second.span,
                name,
                format!(
                    "color `{name}` is redefined here as {}, but was {} at {}",
                    b.hull.spell(),
                    a.hull.spell(),
                    report.place(first.span)
                ),
                None,
            );
        }
    }
}

fn duplicate_color_value(report: &mut Report) {
    let analysis = report.analysis();
    if !definitions_matter(analysis) {
        return;
    }
    // An alias names an existing color on purpose: a role (`comments`) given a color (`orange`).
    let exact: Vec<&ColorDef> =
        analysis.facts.colors.iter().filter(|d| d.certain && d.value.is_some() && !d.alias).collect();
    let mut seen: crate::fixed::HashSet<&str> = crate::fixed::HashSet::default();
    for (i, doc) in exact.iter().enumerate() {
        if doc.package.is_some() || internal(&doc.shown) || !seen.insert(doc.name.as_str()) {
            continue;
        }
        // The first color defined before it with its value, never a package's internal one.
        let Some(other) = exact[..i].iter().find(|other| {
            other.name != doc.name && other.value == doc.value && !(other.package.is_some() && internal(&other.name))
        }) else {
            continue;
        };
        report.add(
            doc.span,
            &doc.shown,
            format!(
                "color `{}` is exactly `{}` ({})",
                doc.shown,
                other.shown,
                doc.value.as_ref().map_or_else(String::new, |v| v.hull.spell())
            ),
            None,
        );
    }
}

fn unbalanced_color_stack(report: &mut Report) {
    let analysis = report.analysis();
    let mut open: HashMap<i64, Vec<crate::engine::tex::Span>> = HashMap::default();
    let mut findings = Vec::new();
    for entry in &analysis.facts.color_stack {
        let Some(stack) = entry.stack else { continue };
        match entry.op {
            StackOp::Push => open.entry(stack).or_default().push(entry.span),
            StackOp::Pop => match open.get_mut(&stack) {
                Some(pushes) if !pushes.is_empty() => {
                    pushes.pop();
                }
                _ => findings.push((entry.span, format!("stack {stack} is popped with nothing pushed to it"))),
            },
            StackOp::Set | StackOp::Current => {}
        }
    }
    for (stack, pushes) in open {
        for span in pushes {
            findings.push((span, format!("stack {stack} is pushed here and never popped")));
        }
    }
    for (span, message) in findings {
        report.add(span, "pdfcolorstack", message, None);
    }
}

const BLACK: Rgb = [0.0; 3];
const WHITE: Rgb = [1.0; 3];
/// Two colors this close in each component are one: half a step of an 8-bit channel.
const SAME: f64 = 0.5 / 255.0;
/// The steps a fix takes from a color towards black or white.
const STEPS: u32 = 100;

/// sRGB's transfer function undone: light, linear in each channel (IEC 61966-2-1).
fn linear(v: f64) -> f64 {
    if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

fn encoded(v: f64) -> f64 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

/// WCAG 2's relative luminance.
fn luminance(c: Rgb) -> f64 {
    let [r, g, b] = c.map(linear);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// WCAG 2's contrast ratio: the lighter luminance over the darker, each with 0.05 of flare.
fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (x, y) = (luminance(a), luminance(b));
    (x.max(y) + 0.05) / (x.min(y) + 0.05)
}

/// CIELAB under D65.
fn lab(c: Rgb) -> [f64; 3] {
    let [r, g, b] = c.map(linear);
    let xyz = [
        (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047,
        0.2126 * r + 0.7152 * g + 0.0722 * b,
        (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883,
    ];
    let [fx, fy, fz] = xyz.map(|t| if t > 216.0 / 24389.0 { t.cbrt() } else { (24389.0 / 27.0 * t + 16.0) / 116.0 });
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIE76's Delta E: the distance in CIELAB.
fn distance(a: Rgb, b: Rgb) -> f64 {
    let (p, q) = (lab(a), lab(b));
    (0..3).map(|i| (p[i] - q[i]).powi(2)).sum::<f64>().sqrt()
}

/// The deficiencies simulated, with the linear-RGB matrices of Machado, Oliveira and Fernandes
/// (2009) at severity 1.
const DEFICIENCIES: [(&str, [[f64; 3]; 3]); 3] = [
    ("protanopia", [[0.152286, 1.052583, -0.204868], [0.114503, 0.786281, 0.099216], [-0.003882, -0.048116, 1.051998]]),
    (
        "deuteranopia",
        [[0.367322, 0.860646, -0.227968], [0.280085, 0.672501, 0.047413], [-0.011820, 0.042940, 0.968881]],
    ),
    ("tritanopia", [[1.255528, -0.076749, -0.178779], [-0.078411, 0.930809, 0.147602], [0.004733, 0.691367, 0.303900]]),
];

fn simulate(c: Rgb, m: &[[f64; 3]; 3]) -> Rgb {
    let l = c.map(linear);
    [0, 1, 2].map(|i| encoded(m[i][0] * l[0] + m[i][1] * l[1] + m[i][2] * l[2]))
}

/// Okabe and Ito's palette for color universal design (2008), black left out.
const OKABE_ITO: [(&str, [u8; 3]); 7] = [
    ("orange", [230, 159, 0]),
    ("sky blue", [86, 180, 233]),
    ("bluish green", [0, 158, 115]),
    ("yellow", [240, 228, 66]),
    ("blue", [0, 114, 178]),
    ("vermillion", [213, 94, 0]),
    ("reddish purple", [204, 121, 167]),
];

fn bytes(c: Rgb) -> [u8; 3] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn of_bytes(b: [u8; 3]) -> Rgb {
    b.map(|v| f64::from(v) / 255.0)
}

/// The definition a color is read back from: the document's own last one with exactly that
/// value, else a package's.
fn definition(analysis: &Analysis, c: Rgb) -> Option<&ColorDef> {
    let same = |d: &&ColorDef| {
        !internal(&d.shown)
            && d.value.as_ref().and_then(|v| v.rgb()).is_some_and(|v| (0..3).all(|i| (v[i] - c[i]).abs() <= SAME))
    };
    let defs = &analysis.facts.colors;
    defs.iter().rev().filter(same).find(|d| d.package.is_none()).or_else(|| defs.iter().rev().find(same))
}

/// The colors the author chose: defined in the document's files or named by its own text, the name
/// standing in the source line itself (`\\color{red}`, `\\definecolor{mine}…`), not in a theme or a
/// macro the line calls (`\\usecolortheme`, `\\alert`).
fn author_colors(report: &Report) -> Vec<(String, Rgb)> {
    let analysis = report.analysis();
    let edits = report.edits();
    let spells = |at: Span, name: &str| edits.text(at.file).is_some_and(|(_, text)| text.line(at.line).contains(name));
    let value = |name: &str| analysis.facts.colors.iter().rev().find(|d| d.name == name)?.value.as_ref()?.rgb();
    let used = (analysis.facts.color_uses.iter())
        .filter(|u| matches!(u.role, CsnameRole::Use) && u.package.is_none() && !internal(&u.name))
        .filter(|u| spells(u.span, &u.name))
        .filter_map(|u| Some((u.name.clone(), value(&u.name)?)));
    let defined = (analysis.facts.colors.iter())
        .filter(|d| d.package.is_none() && !internal(&d.shown) && spells(d.span, &d.shown))
        .filter_map(|d| Some((d.shown.clone(), d.value.as_ref()?.rgb()?)));
    used.chain(defined).collect()
}

/// A fix that writes `c` into `def`'s own `\definecolor{name}{model}{spec}`, in its model. The
/// edit stands only where the spec written there is the color the run stored.
fn respec(report: &Report, def: &ColorDef, c: Rgb, description: String) -> Fix {
    let edits = (|| {
        let span = (def.certain && def.package.is_none()).then_some(def.span)?;
        let edits = report.edits();
        let (path, text) = edits.text(span.file)?;
        let mut scan = crate::linter::fix::Scan::at(&text, Pos::of(span), edits.cats(span.file))?;
        // the name's closing brace, then the model and the spec
        while !matches!(scan.advance()?.tok, crate::engine::tex::Tok::Chr(_, crate::engine::tex::Catcode::End)) {}
        let (model, spec) = (scan.group()?, scan.group()?);
        let model = crate::engine::value::Model::parse(text.slice(model.inner_start, model.inner_end)?.trim())?;
        let written = crate::engine::value::parse_color(model, text.slice(spec.inner_start, spec.inner_end)?)?;
        let (was, stored) = (written.rgb()?, def.value.as_ref()?.rgb()?);
        (0..3).all(|i| (was[i] - stored[i]).abs() <= SAME).then_some(())?;
        let gray = c[0] == c[1] && c[1] == c[2];
        let (_, replacement) =
            specs(c, true).into_iter().find(|(m, _)| *m == model.as_str() && (gray || *m != "gray"))?;
        Some(vec![Edit { path, start: spec.inner_start, end: spec.inner_end, replacement }])
    })();
    Fix::new(description, Applicability::Unsafe, edits)
}

/// The color of `c`'s hue nearest to it, towards black or white whichever `on` lets reach
/// further, with at least `need` of contrast against `on`.
fn reaching(c: Rgb, on: Rgb, need: f64) -> Option<Rgb> {
    let to = if contrast(BLACK, on) >= contrast(WHITE, on) { BLACK } else { WHITE };
    (1..=STEPS).map(|k| f64::from(k) / f64::from(STEPS)).find_map(|t| {
        let mixed = of_bytes(bytes([0, 1, 2].map(|i| c[i] + (to[i] - c[i]) * t)));
        (contrast(mixed, on) >= need).then_some(mixed)
    })
}

fn low_contrast_color(report: &mut Report) {
    let analysis = report.analysis();
    let limits = &analysis.settings.lints.low_contrast_color;
    let text = &analysis.facts.text;
    let states = &analysis.font_states;
    // A page `\pagecolor` or the output routine colored where satex cannot read it is a
    // background of unknown color.
    let paged = text.page_drawn
        || (analysis.facts.color_uses.iter()).any(|u| u.via.is_some_and(|v| analysis.interner.name(v) == "pagecolor"));
    let mut under: Vec<Option<Option<u16>>> = vec![None; text.at.len()];
    for &(start, end, fill) in &text.backgrounds {
        for slot in under.get_mut(start as usize..(end as usize).min(text.at.len())).into_iter().flatten() {
            slot.get_or_insert(fill);
        }
    }
    let chosen = author_colors(report);
    let mut last = None;
    for (i, (ch, spot)) in text.chars.chars().zip(&text.at).enumerate() {
        if ch.is_whitespace() {
            continue;
        }
        let Some(state) = states.get(usize::from(spot.font_state)) else { continue };
        let background = match under[i] {
            Some(Some(fill)) => color_of(analysis, fill),
            Some(None) => None,
            None if state.page.is_empty() => (!paged).then_some(WHITE),
            None => driver_rgb(&state.page),
        };
        let (Some(fg), Some(bg)) = (color_of(analysis, spot.font_state), background) else {
            last = None;
            continue;
        };
        let size = state.size.parse::<f64>().unwrap_or_default();
        let large = size >= limits.large || (state.series.starts_with('b') && size >= limits.large_bold);
        let key = (bytes(fg), bytes(bg), large);
        if last.replace(key) == Some(key) || !report.wanted(spot.span) {
            continue;
        }
        let need = if large { limits.large_ratio } else { limits.ratio };
        let ratio = contrast(fg, bg);
        if ratio >= need {
            continue;
        }
        // a name the author knows: one they chose, else the value (never pgf's `pgffillcolor`)
        let known = |c: Rgb| {
            (chosen.iter().find(|(_, n)| (0..3).all(|i| (n[i] - c[i]).abs() <= SAME)))
                .map_or_else(|| hex(c), |(name, _)| format!("`{name}`"))
        };
        let name = known(fg);
        let on = if under[i].is_none() && bg == WHITE { "the page".to_string() } else { known(bg) };
        let message = format!("text in {name} on {on} has a contrast of {ratio:.1}:1, below {need}:1");
        let severity = if spot.certain { Severity::Warning } else { Severity::Info };
        let shown = name.trim_matches('`').to_string();
        match reaching(fg, bg, need) {
            Some(better) => {
                let description = format!("change {name} to {} ({:.1}:1)", hex(better), contrast(better, bg));
                let fix = match definition(analysis, fg) {
                    Some(def) => respec(report, def, better, description),
                    None => Fix::new(description, Applicability::Unsafe, None),
                };
                report.add_fix_as(severity, spot.span, &shown, message, fix);
            }
            None => report.add_as(severity, spot.span, &shown, message, None),
        }
    }
}

/// The deficiencies under which `a` and `b`, apart for normal vision, come closer than `below`.
fn alike(a: Rgb, b: Rgb, apart: f64, below: f64) -> Vec<&'static str> {
    if distance(a, b) < apart {
        return Vec::new();
    }
    let close = |(_, m): &&(&str, [[f64; 3]; 3])| distance(simulate(a, m), simulate(b, m)) < below;
    DEFICIENCIES.iter().filter(close).map(|(name, _)| *name).collect()
}

fn indistinct_colors(report: &mut Report) {
    let analysis = report.analysis();
    let limits = &analysis.settings.lints.indistinct_colors;
    let at = |s: Span| (s.file, s.line, s.col);
    // Side by side: in one outermost environment below the document, or one paragraph's text.
    let regions = crate::linter::shared::environments(analysis, |key| key != "document");
    let mut outermost: Vec<(Span, Span)> = Vec::new();
    for (begin, end, _) in regions {
        if outermost.last().is_none_or(|last| last.1.file != begin.file || at(last.1) < at(begin)) {
            outermost.push((begin, end));
        }
    }
    let value = |name: &str| analysis.facts.colors.iter().rev().find(|d| d.name == name)?.value.as_ref()?.rgb();
    let mut uses: Vec<((bool, usize), Rgb, Span)> = Vec::new();
    for u in &analysis.facts.color_uses {
        if !matches!(u.role, CsnameRole::Use) || u.package.is_some() || internal(&u.name) {
            continue;
        }
        let inside = |r: &(Span, Span)| r.0.file == u.span.file && at(r.0) <= at(u.span) && at(u.span) <= at(r.1);
        if let (Some(scope), Some(c)) = (outermost.iter().position(inside), value(&u.name)) {
            uses.push(((true, scope), c, u.span));
        }
    }
    let mut colored: crate::fixed::HashSet<(usize, [u8; 3])> = crate::fixed::HashSet::default();
    for (i, spot) in text_spots(analysis) {
        let Some(c) = color_of(analysis, spot.font_state) else { continue };
        let par = analysis.facts.text.pars.partition_point(|&p| p as usize <= i);
        if colored.insert((par, bytes(c))) {
            uses.push(((false, par), c, spot.span));
        }
    }
    // a color the author chose (a theme's `alerted text.fg` beside its `example text.fg` is the theme's)
    let chosen = author_colors(report);
    let mine = |c: Rgb| chosen.iter().any(|(_, n)| (0..3).all(|i| (n[i] - c[i]).abs() <= SAME));
    let mut reported: crate::fixed::HashSet<([u8; 3], [u8; 3])> = crate::fixed::HashSet::default();
    let mut scopes: HashMap<(bool, usize), Vec<(Rgb, Span)>> = HashMap::default();
    for (scope, c, span) in uses {
        let seen = scopes.entry(scope).or_default();
        if seen.iter().any(|(s, _)| bytes(*s) == bytes(c)) {
            continue;
        }
        for &(other, _) in seen.iter() {
            let pair = (bytes(other).min(bytes(c)), bytes(other).max(bytes(c)));
            let names = alike(other, c, limits.apart, limits.below);
            if names.is_empty() || !(mine(other) || mine(c)) || !report.wanted(span) || !reported.insert(pair) {
                continue;
            }
            // the author's name for each, else the value: never a package's internal one, never one name twice
            let known = |c: Rgb| {
                (chosen.iter().find(|(_, n)| (0..3).all(|i| (n[i] - c[i]).abs() <= SAME)))
                    .map_or_else(|| hex(c), |(name, _)| format!("`{name}`"))
            };
            let (first, mut second) = (known(other), known(c));
            if first == second {
                second = hex(c);
            }
            let message = format!("{first} and {second} look alike with {}", names.join(" or "));
            let safe = |o: &&(&str, [u8; 3])| alike(other, of_bytes(o.1), limits.apart, limits.below).is_empty();
            let nearest = OKABE_ITO
                .iter()
                .filter(safe)
                .min_by(|x, y| distance(of_bytes(x.1), c).total_cmp(&distance(of_bytes(y.1), c)));
            let shown = second.trim_matches('`').to_string();
            match nearest {
                Some((name, b)) => {
                    let description = format!("use Okabe-Ito {name} ({}) for {second}", hex(of_bytes(*b)));
                    let fix = match definition(analysis, c) {
                        Some(def) => respec(report, def, of_bytes(*b), description),
                        None => Fix::new(description, Applicability::Unsafe, None),
                    };
                    report.add_fix(span, &shown, message, fix);
                }
                None => report.add(span, &shown, message, None),
            }
        }
        seen.push((c, span));
    }
}

/// The color of the font state `row`: black where no color was selected, `None` for the row of
/// a state the run did not know, or a color it cannot read.
fn color_of(analysis: &Analysis, row: u16) -> Option<Rgb> {
    let state = analysis.font_states.get(usize::from(row)).filter(|_| row != 0)?;
    if state.color.is_empty() { Some(BLACK) } else { driver_rgb(&state.color) }
}

/// The characters with ink the document set on every run, with their index in the text.
fn text_spots(analysis: &Analysis) -> impl Iterator<Item = (usize, &crate::analysis::facts::Spot)> {
    let text = &analysis.facts.text;
    text.chars
        .chars()
        .zip(&text.at)
        .enumerate()
        .filter(|(_, (c, s))| !c.is_whitespace() && s.certain)
        .map(|(i, (_, s))| (i, s))
}
