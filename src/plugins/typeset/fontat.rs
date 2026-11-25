//! The font the run set text in at a place: for the playground's echo of letters typed in a
//! command's argument (`\textbf{…}`, a user's `\kw{…}`) before a build sets them.

use super::Facts;
use crate::analysis::machine::{Analysis, macro_text};
use serde_json::{Value as Json, json};

/// `satex/fontAt` (`{textDocument, position, char}`): the character the run set just before
/// `position` (0-based line and column), `char` when given, as its font's style among the
/// kernel's defaults (`bf it`, as the typeset view's font marks) and its NFSS axes. `null` where
/// the run set no such character there: text it has not seen yet.
pub fn answer(facts: &mut Facts, path: &str, params: &Json) -> Json {
    let a = facts.analysis;
    let at = |k: &str| params["position"][k].as_u64().and_then(|v| u32::try_from(v).ok());
    let (Some(line), Some(col)) = (at("line"), at("character")) else { return Json::Null };
    let want = params["char"].as_str().and_then(|c| c.chars().next());
    let path = params["path"].as_str().map_or(path, |p| p.strip_prefix("file://").unwrap_or(p));
    let same =
        |f| a.file_name(f) == path || std::path::Path::new(path).ends_with(a.file_name(f).trim_start_matches("./"));
    let text = &a.facts.text;
    // spans count lines and columns from 1: the character before the place stands at its column
    let hit = (text.chars.chars().zip(&text.at))
        .filter(|(c, s)| {
            (s.span.line, s.span.col) == (line + 1, col)
                && s.font_state != 0
                && want.is_none_or(|w| w == *c)
                && same(s.span.file)
        })
        .last();
    let Some((_, spot)) = hit else { return Json::Null };
    let Some(state) = a.font_states.get(usize::from(spot.font_state)) else { return Json::Null };
    let styles = font_styles(a);
    let style = styles.get(usize::from(spot.font_state)).map_or("", String::as_str);
    json!({ "style": style, "family": &*state.family, "series": &*state.series, "shape": &*state.shape })
}

/// Each row of [`Analysis::font_states`] as its family, series and shape
/// among the kernel's defaults (`\sfdefault`, `\ttdefault`, `\bfdefault`,
/// `\itdefault`, `\sldefault`, `\scdefault`, fntguide § 2.5): `sf`, `tt`,
/// `bf`, `it`, `sl`, `sc`, space-separated; empty for the defaults or an
/// unknown font.
pub fn font_styles(analysis: &Analysis) -> Vec<String> {
    // A default's text up to the kernel's `\@empty` end marker (`b\@empty`).
    let axis = |names: &[&'static str]| -> Vec<(&'static str, String)> {
        let text = |n| Some(macro_text(analysis, n)?.split('\\').next()?.trim().to_string());
        names.iter().filter_map(|n| Some((&n[..2], text(n)?))).collect()
    };
    // `\bfseries` sets the family's own bold series (`\bfseries@rm`, `bx`) when the kernel has one.
    let (family, series, shape) = (
        axis(&["sfdefault", "ttdefault"]),
        axis(&["bfdefault", "bfseries@rm", "bfseries@sf", "bfseries@tt"]),
        axis(&["itdefault", "sldefault", "scdefault"]),
    );
    let pick = |axis: &[(&'static str, String)], value: &str| axis.iter().find(|(_, d)| d == value).map(|(k, _)| *k);
    // a series is the default's when its weight (the first letter) is: `\fontseries{b}` is as bold as `bx`
    let weight = |value: &str| series.iter().find(|(_, d)| d.chars().next() == value.chars().next()).map(|(k, _)| *k);
    analysis
        .font_states
        .iter()
        .map(|s| {
            [pick(&family, &s.family), pick(&series, &s.series).or_else(|| weight(&s.series)), pick(&shape, &s.shape)]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect()
}
