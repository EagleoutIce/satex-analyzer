//! Heading styles and page options for the page view: the font each heading level ends the run
//! with, the preamble's call that sets it (KOMA's `\setkomafont`/`\addtokomafont`, titlesec's
//! `\titleformat`), KOMA's alignment macros, `secnumdepth` and `\pagecolor`. The view edits the
//! calls' texts with the switches listed here; the snapshot printer writes them.

use super::Cx;
use super::page::{Call, calls, count};
use crate::analysis::machine::Analysis;
use crate::linter::shared::loaded;
use lsp_types::Position;
use serde_json::{Value as Json, json};

const LEVELS: [&str; 4] = ["chapter", "section", "subsection", "subsubsection"];
/// NFSS's switches by what they set (fntguide § 4.2 and § 4.3): `[field, [value, command]]`.
const SWITCHES: [(&str, &[(&str, &str)]); 3] = [
    ("family", &[("serif", "rmfamily"), ("sans", "sffamily"), ("mono", "ttfamily")]),
    ("series", &[("normal", "mdseries"), ("bold", "bfseries")]),
    ("shape", &[("upright", "upshape"), ("italic", "itshape"), ("slanted", "slshape"), ("small caps", "scshape")]),
];
const SIZES: [&str; 10] =
    ["tiny", "scriptsize", "footnotesize", "small", "normalsize", "large", "Large", "LARGE", "huge", "Huge"];
/// Alignment as KOMA's `\raggedsection` holds it, as titlesec's format writes it.
const ALIGN: [(&str, &str, &str); 3] =
    [("left", "raggedright", "filright"), ("center", "centering", "filcenter"), ("right", "raggedleft", "filleft")];

/// The text of the first `{…}` group in `t` after `skip` groups, and what follows it.
fn group(t: &str, skip: usize) -> Option<&str> {
    let (mut depth, mut start, mut seen) = (0, None, 0);
    for (i, c) in t.char_indices() {
        if t[..i].ends_with('\\') && !t[..i].ends_with("\\\\") {
            continue;
        }
        match c {
            '{' => {
                if depth == 0 {
                    start = Some(i + 1);
                }
                depth += 1;
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    if seen == skip {
                        return t.get(start?..i);
                    }
                    seen += 1;
                }
            }
            _ => {}
        }
    }
    None
}

/// The text of the parameterless macro `\name` as `\show` prints it.
pub(super) fn macro_text(a: &Analysis, name: &str) -> Option<String> {
    match a.env.meaning(a.interner.lookup(name)?) {
        crate::engine::tex::Meaning::Macro(m) if m.arity() == 0 => {
            Some(crate::engine::tex::detokenize(&m.replacement_text, &a.interner))
        }
        _ => None,
    }
}

/// The text of macro `\name`, followed while it is one other macro (`\scr@fnt@disposition` is `\sectfont`).
fn resolved(a: &Analysis, name: &str) -> Option<String> {
    let mut t = macro_text(a, name)?;
    for _ in 0..4 {
        let one = t.trim().strip_prefix('\\').filter(|n| n.chars().all(|c| c.is_ascii_alphabetic() || c == '@'));
        match one.and_then(|n| macro_text(a, n)) {
            Some(next) => t = next,
            None => break,
        }
    }
    Some(t)
}

/// The preamble's last call of `\name` whose first `{…}` reads `first`, as a snapshot.
fn last(cx: &Cx, names: &[&str], first: &str, begin: usize) -> Option<Json> {
    let text = cx.doc.body();
    let mut found: Vec<Call> = names
        .iter()
        .flat_map(|n| calls(text, n, 0, begin))
        .filter(|(_, g, _)| g.iter().find(|g| g.open == '{').is_some_and(|g| text[g.inner.clone()].trim() == first))
        .collect();
    found.sort_by_key(|c| c.0);
    plain(cx.doc, found.last()?.0)
}

/// The call at `at` as a snapshot, its arguments text.
fn plain(doc: &crate::text::Text, at: usize) -> Option<Json> {
    serde_json::to_value(super::super::snapshot::read(doc, at, &[], false)?).ok()
}

/// Heading levels with the font each ends the run with and where a change goes; a reason when
/// the class's headings are not this dialog's to write.
pub(super) fn headings(cx: &Cx, family: &str, begin: usize, insert: Position) -> Json {
    let a = cx.analysis;
    let koma = a.defined("setkomafont");
    let titlesec = loaded(a, "titlesec");
    let startsection = |n: &str| macro_text(a, n).filter(|t| t.trim_start().starts_with("\\@startsection"));
    // memoir first: titlesec loaded beside it does not make its headings titlesec's
    let scheme = if family == "memoir" {
        let why = "memoir sets its headings with its own commands (\\setsecheadstyle …): not written here";
        return json!({ "refuse": why, "insert": insert });
    } else if koma {
        "koma"
    } else if titlesec || (family == "basic" && startsection("section").is_some()) {
        "titlesec"
    } else {
        return json!({ "refuse": "the class defines its own headings: not written here", "insert": insert });
    };
    let mut levels: Vec<Json> = Vec::new();
    for name in LEVELS.into_iter().filter(|n| a.defined(n)) {
        // the level's number: titlesec's, KOMA's, the class's `\@startsection{name}{level}`
        let num = (macro_text(a, &format!("ttll@{name}")).or_else(|| macro_text(a, &format!("{name}numdepth"))))
            .or_else(|| startsection(name).and_then(|t| group(&t, 1).map(str::to_string)))
            .and_then(|t| t.trim().parse::<i64>().ok());
        let (font, call, why) = if scheme == "koma" {
            // KOMA applies preamble fonts at `\begin{document}`: the disposition, the element's own
            // (`\size@chapter`, which `\setkomafont` replaces), then the call's
            let call = last(cx, &["setkomafont", "addtokomafont"], name, begin);
            let own = call.as_ref().and_then(|c| c["args"][1]["text"].as_str()).unwrap_or_default();
            let set = call.as_ref().is_some_and(|c| c["name"] == "setkomafont");
            let element = if set { Some(String::new()) } else { resolved(a, &format!("scr@fnt@{name}")) };
            let font = resolved(a, "scr@fnt@disposition").zip(element).map(|(d, e)| format!("{d}{e}{own}"));
            (font, call, None)
        } else if titlesec {
            // `\ttlf@name` is `shape{format}{label}…` (titlesec.sty `\ttl@format@si`)
            let font = macro_text(a, &format!("ttlf@{name}")).and_then(|t| group(&t, 0).map(str::to_string));
            let why = font.is_none().then_some("titlesec keeps the class's head here: write \\titleformat for it");
            (font, last(cx, &["titleformat"], &format!("\\{name}"), begin), why)
        } else {
            let font = startsection(name).and_then(|t| group(&t, 5).map(str::to_string));
            let why =
                font.is_none().then_some("titlesec would replace the class's whole head: write \\titleformat for it");
            (font, None, why)
        };
        let why = why.or_else(|| font.is_none().then_some("the run left its font open"));
        let ragged = (scheme == "koma").then(|| match name {
            "chapter" if a.defined("raggedchapter") => "raggedchapter",
            _ => "raggedsection",
        });
        levels.push(json!({ "name": name, "level": num, "font": font, "call": call, "refuse": why, "ragged": ragged }));
    }
    let ragged: serde_json::Map<String, Json> = ["raggedchapter", "raggedsection"]
        .into_iter()
        .filter(|n| scheme == "koma" && a.defined(n))
        .map(|n| {
            let call = last(cx, &["renewcommand"], &format!("\\{n}"), begin);
            (n.to_string(), json!({ "text": resolved(a, n), "call": call }))
        })
        .collect();
    let switches: serde_json::Map<String, Json> = SWITCHES
        .iter()
        .map(|(field, vals)| {
            let defined: Vec<Json> =
                vals.iter().filter(|(_, c)| a.defined(c)).map(|(v, c)| json!([v, format!("\\{c}")])).collect();
            (field.to_string(), json!(defined))
        })
        .collect();
    let align: Vec<Json> = ALIGN
        .iter()
        .map(|(v, k, t)| {
            let (write, other) = if scheme == "koma" { (k, t) } else { (t, k) };
            json!([v, format!("\\{write}"), format!("\\{other}")])
        })
        .collect();
    json!({
        "scheme": scheme,
        "levels": levels,
        "ragged": ragged,
        "switches": switches,
        "sizes": SIZES.iter().filter(|s| a.defined(s)).map(|s| format!("\\{s}")).collect::<Vec<_>>(),
        "align": align,
        "secnumdepth": { "value": count(a, "c@secnumdepth"), "call": last(cx, &["setcounter"], "secnumdepth", begin) },
        "insert": insert,
    })
}

/// `\pagecolor`: its last call in the preamble, the colors defined there.
pub(super) fn pagecolor(cx: &Cx, begin: usize) -> Json {
    let text = cx.doc.body();
    let call = calls(text, "pagecolor", 0, begin).pop().and_then(|c| plain(cx.doc, c.0));
    let at = cx.doc.pos(begin);
    json!({ "call": call, "colors": super::color::palette(cx.analysis, cx.path, at), "defined": cx.analysis.defined("pagecolor") })
}

/// The switch `\if<name>` as the run ended it, none when left open.
pub(super) fn flag(a: &Analysis, name: &str) -> Option<bool> {
    use crate::engine::builtins::{Cond, Primitive};
    match a.env.meaning(a.interner.lookup(name)?) {
        crate::engine::tex::Meaning::Primitive(Primitive::If(Cond::True)) => Some(true),
        crate::engine::tex::Meaning::Primitive(Primitive::If(Cond::False)) => Some(false),
        _ => None,
    }
}
