//! `satex/commandText`: what a control word typesets on its own, for the echo
//! of a command typed into the PDF (`\TeX`, `\LaTeX`, a user's logo). A sandbox
//! of the run's end state sets `\hbox{\name}` and records each character with
//! where its list put it: the width before it on that list, the boxes around
//! it placed by their own list (`\raise`, `\lower`, a `\vbox`'s rows).

use super::Facts;
use crate::analysis::machine::Machine;
use crate::engine::mode::{Dim, Nest};
use crate::engine::tex::{Catcode, Span, Tok, Token};
use serde_json::{Value as Json, json};

/// The characters and boxes a sandbox run set, in the order TeX made them.
#[derive(Clone, Default)]
pub struct Recorder {
    /// Per character: its index in the run's text, the character, its place
    /// on its own list and its font's size (sp), `None` unknown.
    chars: Vec<(u32, char, Option<i64>, Option<i64>)>,
    /// Per box that ended: the text it holds, then where it went ([`Placed`]).
    boxes: Vec<(u32, u32, Placed)>,
    /// The last box's width, height and depth: the outermost `\hbox`'s at the end.
    size: [Option<i64>; 3],
}

#[derive(Clone)]
enum Placed {
    /// Onto a horizontal list: the width before it there, its shift down.
    Row { x: Option<i64>, shift: Option<i64> },
    /// Onto a vertical list: the height down to its baseline there, its shift right.
    Column { down: Option<i64>, shift: Option<i64> },
    /// A vertical list's box of this height ended: its rows' baselines are measured up from its own.
    Packed { height: Option<i64> },
    /// Into a register (`\sbox`): not on the list it was made in.
    Aside,
}

impl Machine<'_> {
    /// A character set at text index `before` and on (none where the run kept none).
    pub(crate) fn command_char(&mut self, c: char, before: usize) {
        if self.out.facts.text.at.len() <= before {
            return;
        }
        let x = self.natural.and_then(|n| n.along.known());
        let size = self.current_font().and_then(|f| self.font_size(f));
        if let Some(r) = self.command_text.as_deref_mut() {
            r.chars.push((before as u32, c, x, size));
        }
    }

    /// The box of group `nest` ended holding text `from..to`, `vertical` its own list
    /// vertical, `dims` its size: placed on the current list when `placed`, else set aside.
    pub(crate) fn command_box(&mut self, nest: &Nest, to: u32, vertical: bool, dims: [Dim; 3], placed: bool) {
        let along = self.natural.and_then(|n| n.along.known());
        let shift = nest.shift.known();
        let onto = match (placed, self.list_kind()) {
            (false, _) => Placed::Aside,
            (true, Some(true)) => Placed::Row { x: along, shift },
            (true, _) => Placed::Column { down: along.zip(dims[1].known()).map(|(a, h)| a + h), shift },
        };
        let Some(r) = self.command_text.as_deref_mut() else { return };
        r.size = dims.map(|d| d.known());
        if vertical {
            r.boxes.push((nest.from, to, Placed::Packed { height: dims[1].known() }));
        }
        r.boxes.push((nest.from, to, onto));
    }
}

/// One glyph of the answer: the character, x and y (up) in sizes of the
/// starting font (its em for Computer Modern), and its size against that font.
type Glyph = (char, f64, f64, f64);

impl Recorder {
    /// Each character's place from the outermost box's origin, `None` where
    /// any length on the way is unknown.
    fn glyphs(&self, em: i64) -> Option<Vec<Glyph>> {
        // x, y up, and for a row of a vertical list still open the height down to its baseline
        let mut at: Vec<(i64, i64, Option<i64>)> =
            self.chars.iter().map(|c| Some((c.2?, 0, None))).collect::<Option<_>>()?;
        // what the outermost box holds, never text after it
        let (f0, t0, _) = self.boxes.last()?;
        let mut aside: Vec<bool> = self.chars.iter().map(|c| !(*f0..*t0).contains(&c.0)).collect();
        for (from, to, placed) in &self.boxes {
            let inside = |i: usize| (*from..*to).contains(&self.chars[i].0);
            for i in (0..at.len()).filter(|&i| inside(i)) {
                let p = &mut at[i];
                match *placed {
                    Placed::Row { x, shift } => (p.0, p.1) = (p.0 + x?, p.1 - shift?),
                    Placed::Column { down, shift } => (p.0, p.2) = (p.0 + shift?, Some(down? - p.1)),
                    Placed::Packed { height } => {
                        if let Some(down) = p.2.take() {
                            p.1 = height? - down;
                        }
                    }
                    Placed::Aside => aside[i] = true,
                }
            }
        }
        let em_of = |v: i64| v as f64 / em as f64;
        let glyphs = self.chars.iter().zip(at).zip(aside).filter(|(_, a)| !a);
        glyphs.map(|((c, (x, y, _)), _)| Some((c.1, em_of(x), em_of(y), em_of(c.3?)))).collect()
    }
}

/// The answer to `{names}`: per name `{w, h, d, g}` in sizes of the font in
/// force (`1` its at-size), `g` its glyphs `[c, x, y, size]`; `null` where the command reads
/// an argument, raises an error, sets nothing or sets it at an unknown place.
pub fn answer(facts: &mut Facts, params: &Json) -> Json {
    let Some(names) = params["names"].as_array() else { return Json::Null };
    let word = |n: &&str| !n.is_empty() && n.chars().all(|c| c.is_ascii_alphabetic() || c == '@');
    let names = names.iter().map(|n| n.as_str().filter(word));
    Json::Array(names.map(|n| n.and_then(|n| typeset(facts, n)).unwrap_or(Json::Null)).collect())
}

fn typeset(facts: &Facts, name: &str) -> Option<Json> {
    let analysis = facts.analysis;
    let sym = analysis.interner.lookup(name).filter(|&s| analysis.env.is_defined(s))?;
    let hbox = analysis.interner.lookup("hbox")?;
    let cfg = crate::analysis::probe::sandbox_config(&analysis.settings);
    let mut m = Machine::sandbox(&cfg, analysis, crate::analysis::probe::State::End)?;
    let em = m.current_font().and_then(|f| m.font_size(f)).filter(|&q| q > 0)?;
    m.command_text = Some(Box::default());
    // `\hbox{\name}` as a probe call: what it sets is text of the run (a line run is a format's, setting none)
    let tok = |tok| Token::new(tok, Span::default());
    let args = [tok(Tok::Chr('{', Catcode::Begin)), tok(Tok::Cs(sym)), tok(Tok::Chr('}', Catcode::End))];
    let watch = m.run_call(hbox, &args, ".");
    let r = m.command_text.take()?;
    if watch.errors > 0 || !watch.raised.is_empty() || watch.exhausted || r.chars.is_empty() {
        return None;
    }
    let outer = r.boxes.last()?;
    let Placed::Column { .. } = outer.2 else { return None };
    let [w, h, d] = r.size;
    let g = r.glyphs(em)?;
    let em_of = |v: i64| v as f64 / em as f64;
    let g: Vec<Json> = g.iter().map(|g| json!([g.0.to_string(), g.1, g.2, g.3])).collect();
    Some(json!({ "w": em_of(w?), "h": em_of(h?), "d": em_of(d?), "g": g }))
}
