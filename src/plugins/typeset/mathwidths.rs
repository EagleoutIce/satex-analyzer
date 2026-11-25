//! `satex/mathwidths`: math symbols as the run's math fonts set them, for the
//! echo of typed math. A symbol is what the source types (`a`, `+`, `\alpha`):
//! its math code (`\mathcode`, or a `\mathchardef`'s value) names a family and
//! a slot (tex.web § 1151), the family's font of the size (`\textfont` …) its
//! metrics: width, height, depth and italic correction, in em of that font.

use super::Facts;
use crate::engine::builtins::CodeTable;
use crate::engine::linebreak::Tables;
use crate::engine::tex::{Meaning, RegKind};
use serde_json::{Value as Json, json};

/// The answer to `{symbols, size}` (`size` 0 text, 1 script, 2 scriptscript):
/// per symbol `[w, h, d, italic]` in em, `null` where not known.
pub fn answer(facts: &mut Facts, params: &Json) -> Json {
    let Some(symbols) = params["symbols"].as_array() else { return Json::Null };
    let size = params["size"].as_u64().unwrap_or(0).min(2) as usize;
    let table = ["textfont", "scriptfont", "scriptscriptfont"][size];
    Json::Array(
        symbols
            .iter()
            .map(|s| s.as_str().and_then(|s| metrics(facts, s, table)).map_or(Json::Null, |m| json!(m)))
            .collect(),
    )
}

/// `symbol`'s box in its family's font of table `table`, in em.
fn metrics(facts: &mut Facts, symbol: &str, table: &str) -> Option<[f64; 4]> {
    let code = match symbol.strip_prefix('\\') {
        // a `\mathchardef` (TeX82's packed code); any other meaning is no symbol of a font here
        Some(name) => {
            let a = facts.analysis;
            let sym = a.interner.lookup(name).filter(|&s| a.env.get(s).is_some())?;
            match a.env.meaning(sym) {
                Meaning::Register(RegKind::MathChar, code) => i64::from(code),
                _ => return None,
            }
        }
        None => {
            let mut cs = symbol.chars();
            let (c, None) = (cs.next()?, cs.next()) else { return None };
            facts.code(CodeTable::Math, c)?
        }
    };
    // `"8000`: active in math, its definition decides
    if !(0..0x8000).contains(&code) {
        return None;
    }
    let (fam, slot) = ((code >> 8) & 0xf, (code & 0xff) as u32);
    // the family's font as the run left it: a write through an unknown family number leaves every one unknown
    if facts.engine_value(&format!("{table}.{}any", crate::engine::commands::ENGINE)).is_some() {
        return None;
    }
    let font = facts.engine_value(&format!("{table}.{fam}")).map_or(Some(0), |v| v.as_int())?;
    let (m, z) = facts.metrics(u32::try_from(font).ok().filter(|&f| f != 0)?)?;
    let b = m.char_box(slot, z)?;
    let em = |s: i64| s as f64 / z as f64;
    (z > 0).then(|| [em(b.width), em(b.height), em(b.depth), em(b.italic)])
}
