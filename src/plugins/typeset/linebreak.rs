//! `satex/break`: TeX's line breaking ([`crate::engine::linebreak::line_break`],
//! the breaker the run itself uses) of a paragraph's words. The list is built
//! as TeX's main loop builds it (§§ 1034-1044): each word's characters with
//! the font's ligatures and kerns, interword glue by space factor, a
//! discretionary after an explicit hyphen. Fonts come from the run's
//! paragraph at the given line, the parameters from where its lines broke.

use super::Facts;
use crate::analysis::facts::Text;
use crate::engine::builtins::CodeTable;
use crate::engine::linebreak::{Kp, Params, Tables, line_break, set_letters};
use crate::engine::value::{Glue, Scaled, Stretch};
use serde_json::{Value as Json, json};

/// Where a line ends: before character `at` of word `word` (0: at the glue
/// before it, else after a hyphen), and its glue set.
#[derive(Clone, Copy, PartialEq, Debug, serde::Serialize)]
pub struct Line {
    pub word: usize,
    pub at: usize,
    pub hyphen: bool,
    pub ratio: f64,
    pub order: u8,
}

/// The lines `words` break into at line `line` (0-based) of `path`, the first
/// line after a box `indent` wide, and each word's natural width; `None` where
/// something is not known.
pub fn lines(
    facts: &mut Facts,
    path: &str,
    line: Option<u32>,
    words: &[Word],
    indent: Scaled,
) -> Option<(Vec<Line>, Vec<Scaled>)> {
    let text = &facts.analysis.facts.text;
    let par = line.and_then(|l| paragraph(facts, path, l));
    let olds = par.map(|(from, to)| old_words(text, facts, from, to)).unwrap_or_default();
    // no paragraph of the run there: the font most of the document's text was set in
    let fallback = {
        let mut count = std::collections::HashMap::<u16, usize>::new();
        for spot in &text.at {
            *count.entry(spot.font_state).or_default() += 1;
        }
        let state =
            count.into_iter().filter(|&(s, _)| s != 0).max_by_key(|&(s, n)| (n, std::cmp::Reverse(s))).map(|(s, _)| s);
        state
            .and_then(|s| facts.analysis.font_states.get(usize::from(s)))
            .map(|s| u32::from(s.font))
            .filter(|&f| f != 0)
    };
    // the parameters the run broke the paragraph's lines with: the last record within it
    let here = |&&(at, _): &&(u32, Params)| par.is_some_and(|(from, to)| (from + 1..=to).contains(&(at as usize)));
    let params = match text.breaks.iter().rev().find(here) {
        Some(&(_, p)) => p,
        None => {
            let width = facts.dimen("hsize")?;
            let emergency = facts.dimen("emergencystretch");
            Params::read(width, emergency, facts, |f, n| f.int(n), |f, n| f.glue(n))?
        }
    };
    let [space_skip, xspace_skip] =
        ["spaceskip", "xspaceskip"].map(|n| facts.glue(n).filter(|g| *g != Glue::default()));
    let mut items = Vec::new();
    // per item: the word it belongs to and the offset of its first character in it
    let mut origin = Vec::new();
    // each word's natural width: its letters with their ligatures and kerns
    let mut widths: Vec<Scaled> = Vec::new();
    if indent != 0 {
        items.push(Kp::Box { w: indent, h: 0, d: 0, chr: None, lig: 0 });
        origin.push((0, 0));
    }
    let (mut cursor, mut font, mut sf) = (0, olds.first().map(|o| o.1).or(fallback)?, 1000);
    for (k, Word { text: word, width }) in words.iter().enumerate() {
        // the run's own word here: its font, and the font of the space after it
        let found = olds[cursor.min(olds.len())..].iter().position(|o| o.0 == *word).map(|i| cursor + i);
        let (f, space) = found.map_or((font, font), |i| (olds[i].1, olds[i].2));
        cursor = found.map_or(cursor, |i| i + 1);
        if k > 0 {
            let normal = match space_skip {
                Some(g) => g,
                None => {
                    let [w, st, sh] = [2, 3, 4].map(|p| facts.font_dimen(font, p));
                    Glue {
                        width: w?,
                        stretch: Stretch { amount: st?, order: 0 },
                        shrink: Stretch { amount: sh?, order: 0 },
                    }
                }
            };
            let extra = facts.font_dimen(font, 7)?;
            items.push(Kp::glue(crate::engine::mode::interword(normal, xspace_skip, extra, sf)));
            origin.push((k, 0));
        }
        let first = items.len();
        // a word the run's paragraph does not spell (a macro's output: a logo, `[2]`): as wide as the page set it
        match width.filter(|_| found.is_none()) {
            Some(w) => {
                items.push(Kp::Box { w, h: 0, d: 0, chr: None, lig: 0 });
                origin.push((k, 0));
            }
            None => push_letters(facts, f, word, k, &mut items, &mut origin)?,
        }
        widths.push(items[first..].iter().map(|k| k.width()).sum());
        for c in word.chars() {
            sf = crate::engine::mode::adjust_space_factor(facts.code(CodeTable::Space, c)?, Some(sf))?;
        }
        font = space;
    }
    let broken = line_break(items, 0, crate::engine::linebreak::DECENT, true, &params, facts)?;
    let mut out = Vec::new();
    for b in &broken.lines {
        let glues = broken.items[..b.at].iter().filter(|k| matches!(k, Kp::Glue { .. })).count();
        let (word, at, hyphen) = match broken.items.get(b.at) {
            None => (words.len(), 0, false),
            Some(Kp::Glue { .. }) => (glues + 1, 0, false),
            Some(Kp::Disc(..)) => {
                let at = match broken.points.iter().find(|p| p.0 == b.at) {
                    Some(&(_, s, j)) => origin[s].1 + j,
                    // an explicit hyphen's: after the letters before it
                    None => broken.items[..b.at]
                        .iter()
                        .rev()
                        .take_while(|k| !matches!(k, Kp::Glue { .. }))
                        .map(|k| match *k {
                            Kp::Box { chr: Some((_, c)), lig, .. } => letters(c, lig),
                            _ => 0,
                        })
                        .sum(),
                };
                (glues, at, true)
            }
            _ => return None,
        };
        out.push(Line { word, at, hyphen, ratio: b.set.0, order: b.set.1 });
    }
    Some((out, widths))
}

/// `word`'s characters in font `f` onto `items` as the main loop sets them (§§ 1034-1040), each item with its
/// word `k` and the offset of its first letter; `None` where the font lacks one or is not followed.
fn push_letters(
    facts: &mut Facts,
    f: u32,
    word: &str,
    k: usize,
    items: &mut Vec<Kp>,
    origin: &mut Vec<(usize, usize)>,
) -> Option<()> {
    let (m, z) = facts.metrics(f)?;
    if m.boundary_char().is_some() {
        return None;
    }
    let codes: Vec<u32> = word.chars().flat_map(source).map(|c| c as u32).collect();
    if codes.iter().any(|&c| !m.has_char(c)) || m.left_boundary(*codes.first()?, z).is_some() {
        return None;
    }
    let (set, _) = set_letters(&m, z, f, &codes)?;
    let hyphen = facts.hyphen_char(f)?;
    let mut at = 0;
    for item in set {
        items.push(item);
        origin.push((k, at));
        if let Kp::Box { chr: Some((_, c)), lig, .. } = item {
            at += letters(c, lig);
            // § 1039: a discretionary after the font's hyphen character
            if i64::from(c) == hyphen {
                items.push(Kp::Disc(None, 0, None));
                origin.push((k, at));
            }
        }
    }
    Some(())
}

/// A word of the request: its text and, for one the page shows, its width there (sp).
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    pub text: String,
    pub width: Option<Scaled>,
}

impl Word {
    /// A plain string, or `[text, width]`.
    fn of(json: &Json) -> Option<Word> {
        match json {
            Json::String(t) => Some(Word { text: t.clone(), width: None }),
            Json::Array(a) => {
                Some(Word { text: a.first()?.as_str()?.to_string(), width: a.get(1).and_then(Json::as_i64) })
            }
            _ => None,
        }
    }
}

/// The `satex/break` answer: the lines, `null` when not known.
pub fn answer(facts: &mut Facts, path: &str, params: &Json) -> Json {
    let words: Option<Vec<Word>> = params["words"].as_array().and_then(|ws| ws.iter().map(Word::of).collect());
    let Some(words) = words else { return Json::Null };
    let line = params["line"].as_u64().and_then(|l| u32::try_from(l).ok());
    // a box that wide, or `\\parindent` for `true`
    let indent = match &params["indent"] {
        Json::Bool(true) => facts.dimen("parindent"),
        v => Some(v.as_i64().unwrap_or(0)),
    };
    let Some(indent) = indent else { return Json::Null };
    // the file the line is in, when not the document's own
    let path = params["path"].as_str().map_or(path, |p| p.strip_prefix("file://").unwrap_or(p));
    match lines(facts, path, line, &words, indent) {
        Some((lines, widths)) => json!({ "lines": lines, "widths": widths }),
        None => Json::Null,
    }
}

/// How many letters a character box stands for: a ligature's all.
fn letters(c: u32, lig: u32) -> usize {
    crate::engine::linebreak::letters(c, lig).map_or(1, |l| l.len())
}

/// The characters TeX's text conventions type for one the PDF shows (`“` is
/// ` `` `): a ligature's own letters.
fn source(c: char) -> Vec<char> {
    let s = match c {
        '“' => "``",
        '”' => "''",
        '‘' => "`",
        '’' => "'",
        '–' => "--",
        '—' => "---",
        '¡' => "!`",
        '¿' => "?`",
        c => return vec![c],
    };
    s.chars().collect()
}

/// The characters (from, to) of the run's paragraph that line `line` of
/// `path` set: from its start to its `\par`.
fn paragraph(facts: &Facts, path: &str, line: u32) -> Option<(usize, usize)> {
    let a = facts.analysis;
    let text = &a.facts.text;
    // spans count lines from 1
    // the same file however the client names it: absolute, or relative to the project
    let same =
        |f| a.file_name(f) == path || std::path::Path::new(path).ends_with(a.file_name(f).trim_start_matches("./"));
    let i = text.site.iter().position(|s| s.line == line + 1 && same(s.file))?;
    let from = text.pars.iter().rev().find(|&&p| p as usize <= i).map_or(0, |&p| p as usize);
    let to = text.pars.iter().find(|&&p| p as usize > i).map_or(text.at.len(), |&p| p as usize);
    Some((from, to))
}

/// The words of characters `from..to`: each with its font and the font of the
/// space after it.
fn old_words(text: &Text, facts: &Facts, from: usize, to: usize) -> Vec<(String, u32, u32)> {
    let font = |i: usize| {
        let s = text.at[i].font_state;
        facts.analysis.font_states.get(usize::from(s)).map_or(0, |s| u32::from(s.font))
    };
    let mut out: Vec<(String, u32, u32)> = Vec::new();
    let mut open = false;
    for (i, c) in text.chars.chars().enumerate().skip(from).take(to - from) {
        match (c, open) {
            (' ', _) => {
                if let Some(last) = out.last_mut().filter(|_| open) {
                    last.2 = font(i);
                }
                open = false;
            }
            (c, true) => out.last_mut().expect("open").0.push(c),
            (c, false) => {
                out.push((c.to_string(), font(i), font(i)));
                open = true;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PT: Scaled = 1 << 16;

    /// A list of words `w` pt wide with glue 4pt stretching 6pt and shrinking 2pt between.
    fn words(widths: &[Scaled]) -> Vec<Kp> {
        let mut out = Vec::new();
        for (i, &w) in widths.iter().enumerate() {
            if i > 0 {
                out.push(Kp::Glue { w: 4 * PT, st: 6 * PT, sto: 0, sh: 2 * PT, sho: 0 });
            }
            out.push(Kp::Box { w: w * PT, h: 0, d: 0, chr: Some((1, u32::from(b'a'))), lig: 0 });
        }
        out
    }

    /// No hyphenation is asked of these lists: a pass that needs it is not followed.
    struct NoTables;
    impl Tables for NoTables {
        fn lower(&mut self, _: u32) -> Option<i64> {
            None
        }
        fn hyphen_char(&mut self, _: u32) -> Option<i64> {
            None
        }
        fn metrics(&mut self, _: u32) -> Option<(std::rc::Rc<crate::engine::tfm::Metrics>, Scaled)> {
            None
        }
        fn patterns(&mut self) -> Option<crate::engine::hyph::HyphTables> {
            None
        }
        fn mins(&mut self) -> (usize, usize) {
            (2, 3)
        }
    }

    fn params(width: Scaled) -> Params {
        let fill = Glue { width: 0, stretch: Stretch { amount: 1 << 16, order: 1 }, shrink: Stretch::default() };
        Params {
            width,
            dem: [10, 10000, 10000, 5000, 50, 50],
            tolerance: 200,
            pretolerance: 200,
            fill,
            ..Params::default()
        }
    }

    /// Intuition: TeX's breaker over a plain list: three words a line (98pt of 100pt, a little stretch), the
    /// rest on the last line, which `\\parfillskip`'s fil fills. Pins the pure breaker's line ends (each at
    /// glue, the last at the list's end) and glue set (within `\\tolerance`, the last line by fil).
    #[test]
    fn total_fit_breaks_a_list() {
        let list = words(&[30, 30, 30, 30, 30, 20, 30]);
        let b = line_break(list, 0, crate::engine::linebreak::DECENT, true, &params(100 * PT), &mut NoTables)
            .expect("a fit without hyphenation");
        let ends: Vec<usize> = b.lines.iter().map(|l| l.at).collect();
        assert_eq!(*ends.last().unwrap(), b.items.len());
        assert!(ends.iter().all(|&e| e == b.items.len() || matches!(b.items[e], Kp::Glue { .. })));
        let last = b.lines.last().unwrap();
        assert_eq!(last.set.1, 1, "the last line stretches by fil");
        // within `\\tolerance` 200: badness 100·|r|³ (§ 108)
        for l in &b.lines[..b.lines.len() - 1] {
            assert!(l.set.0 >= -1.0 && 100.0 * l.set.0.abs().powi(3) <= 200.5, "{l:?}");
        }
        assert!(b.lines.len() >= 2);
    }

    /// Intuition: a word wider than the line fits nowhere within `\\pretolerance`, so TeX's second pass
    /// hyphenates, and here the `\\lccode`s are not known: no answer, so the client falls back to its own fit. Pins
    /// a breaker that answers lines it could not have followed.
    #[test]
    fn unknown_hyphenation_is_no_answer() {
        let list = words(&[150, 10]);
        let p = Params { pretolerance: 100, ..params(100 * PT) };
        assert!(line_break(list, 0, crate::engine::linebreak::DECENT, true, &p, &mut NoTables).is_none());
    }

    /// Intuition: TeX's text conventions: a curly quote is two grave or two apostrophe characters, a dash two
    /// or three hyphens, as the font's ligature program makes them again. Pins words that arrive as the PDF shows them.
    #[test]
    fn source_letters_of_shown_glyphs() {
        assert_eq!(source('“'), vec!['`', '`']);
        assert_eq!(source('—'), vec!['-', '-', '-']);
        assert_eq!(source('a'), vec!['a']);
    }
}
