//! A picture as the run drew it. pgf's driver draws through `\pdfliteral`,
//! so the run holds the picture's PDF content operators (PDF 1.7 § 8.5),
//! read here the way a viewer reads them: the graphics state stack
//! (`q`, `Q`, `cm`, `w`, the colors `g`/`G`, `rg`/`RG`, `k`/`K`), path construction (`m`, `l`, `c`, `v`, `y`, `re`,
//! `h`) and painting (`S`, `s`, `f`, `F`, `f*`, `B`, `B*`, `b`, `b*`, `n`).
//! pgf typesets a node's text before it draws the node's frame, so text
//! stands at the middle of the next shape painted after it, else where
//! the drawing stands at the end. A literal with a number the run did not know is left
//! out, and the picture says it is approximate.

use serde_json::{Value, json};

use crate::analysis::machine::Analysis;
use crate::engine::tex::Span;

/// A color's operands as `#rrggbb`: one gray, three RGB or four CMYK values in 0..1.
fn hex(c: &[f64]) -> String {
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let [r, g, b] = match c {
        [g] => [*g; 3],
        [r, g, b] => [*r, *g, *b],
        [c, m, y, k] => [(1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k)],
        _ => [0.0; 3],
    };
    format!("#{:02x}{:02x}{:02x}", byte(r), byte(g), byte(b))
}

/// A transformation `[a b c d e f]` (PDF 1.7 § 8.3.4).
type Matrix = [f64; 6];

const IDENTITY: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

fn times(m: Matrix, n: Matrix) -> Matrix {
    [
        m[0] * n[0] + m[1] * n[2],
        m[0] * n[1] + m[1] * n[3],
        m[2] * n[0] + m[3] * n[2],
        m[2] * n[1] + m[3] * n[3],
        m[4] * n[0] + m[5] * n[2] + n[4],
        m[4] * n[1] + m[5] * n[3] + n[5],
    ]
}

fn apply(m: Matrix, x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

/// What one picture draws: its painted paths and placed texts, in points
/// with y growing downwards as SVG has it.
#[derive(Default)]
struct Canvas {
    ctm: Matrix,
    width: f64,
    /// The stroke and the fill color, `#rrggbb`, `None` for the default black.
    colors: [Option<String>; 2],
    /// The colors `\pdfcolorstack` pushes saved.
    color_stack: Vec<[Option<String>; 2]>,
    saved: Vec<(Matrix, f64, [Option<String>; 2])>,
    path: String,
    /// The bounding box of the path being built.
    bounds: Option<[f64; 4]>,
    paths: Vec<Value>,
    texts: Vec<Value>,
    /// Text typeset that waits for the shape it goes into, with its color.
    pending: Vec<(String, Option<String>)>,
    box_all: Option<[f64; 4]>,
    approximate: bool,
}

impl Canvas {
    fn point(&mut self, x: f64, y: f64) -> String {
        let (x, y) = apply(self.ctm, x, y);
        let b = self.bounds.get_or_insert([x, y, x, y]);
        *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
        format!("{x:.2} {:.2}", -y)
    }

    fn paint(&mut self, stroke: bool, fill: bool) {
        if let Some(b) = self.bounds.take()
            && (stroke || fill)
        {
            self.paths.push(json!({ "d": self.path.trim(), "stroke": stroke, "fill": fill, "width": self.width }));
            self.place((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
            let all = self.box_all.get_or_insert(b);
            *all = [all[0].min(b[0]), all[1].min(b[1]), all[2].max(b[2]), all[3].max(b[3])];
        }
        self.path.clear();
    }

    /// One literal's operators, operands before each. A color stack's
    /// push saves the colors before its operators set new ones, its pop
    /// brings them back.
    fn run(&mut self, text: &str) {
        if text == crate::analysis::observe::POP_COLOR {
            self.colors = self.color_stack.pop().unwrap_or_default();
            return;
        }
        let text = match text.strip_prefix(crate::analysis::observe::PUSH_COLOR) {
            Some(ops) => {
                self.color_stack.push(self.colors.clone());
                ops
            }
            None => text,
        };
        let mut operands: Vec<f64> = Vec::new();
        let mut array = 0;
        for word in text.split_whitespace() {
            // A dash array `[ … ] phase d` is a stroke style, not a path.
            match word {
                "[" => array += 1,
                "]" => array -= 1,
                _ if array > 0 => {}
                _ => match word.parse::<f64>() {
                    Ok(n) => operands.push(n),
                    Err(_) => {
                        if !self.op(word, &operands) {
                            self.approximate = true;
                        }
                        operands.clear();
                    }
                },
            }
        }
    }

    /// Whether `op` was read: an unknown operator or wrong operands are not.
    fn op(&mut self, op: &str, a: &[f64]) -> bool {
        let take = |n: usize| (a.len() >= n).then(|| &a[a.len() - n..]);
        match op {
            "q" => self.saved.push((self.ctm, self.width, self.colors.clone())),
            "Q" => {
                let last = (IDENTITY, self.width, self.colors.clone());
                (self.ctm, self.width, self.colors) = self.saved.pop().unwrap_or(last);
            }
            // Gray, RGB and CMYK (PDF 1.7 § 8.6.8): upper case strokes, lower case fills.
            "G" | "g" | "RG" | "rg" | "K" | "k" => {
                let n = match op.to_ascii_lowercase().as_str() {
                    "g" => 1,
                    "rg" => 3,
                    _ => 4,
                };
                let Some(c) = take(n) else { return false };
                self.colors[usize::from(op.chars().all(|c| c.is_ascii_lowercase()))] = Some(hex(c));
            }
            "cm" => match take(6) {
                Some(m) => self.ctm = times([m[0], m[1], m[2], m[3], m[4], m[5]], self.ctm),
                None => return false,
            },
            "w" => match take(1) {
                Some(w) => self.width = w[0],
                None => return false,
            },
            "m" | "l" => match take(2) {
                Some(p) => {
                    let at = self.point(p[0], p[1]);
                    self.path.push_str(&format!("{} {at} ", if op == "m" { "M" } else { "L" }));
                }
                None => return false,
            },
            "c" | "v" | "y" => match take(if op == "c" { 6 } else { 4 }) {
                Some(p) if op == "c" => {
                    let (p1, p2, p3) = (self.point(p[0], p[1]), self.point(p[2], p[3]), self.point(p[4], p[5]));
                    self.path.push_str(&format!("C {p1} {p2} {p3} "));
                }
                Some(p) => {
                    let (p1, p2) = (self.point(p[0], p[1]), self.point(p[2], p[3]));
                    self.path.push_str(&format!("Q {p1} {p2} "));
                }
                None => return false,
            },
            "re" => match take(4) {
                Some(r) => {
                    let corners = [(r[0], r[1]), (r[0] + r[2], r[1]), (r[0] + r[2], r[1] + r[3]), (r[0], r[1] + r[3])];
                    let points: Vec<String> = corners.iter().map(|&(x, y)| self.point(x, y)).collect();
                    self.path.push_str(&format!("M {} L {} L {} L {} Z ", points[0], points[1], points[2], points[3]));
                }
                None => return false,
            },
            "h" => self.path.push_str("Z "),
            "S" => self.paint(true, false),
            "s" => {
                self.path.push_str("Z ");
                self.paint(true, false)
            }
            "f" | "F" | "f*" => self.paint(false, true),
            "B" | "B*" => self.paint(true, true),
            "b" | "b*" => {
                self.path.push_str("Z ");
                self.paint(true, true)
            }
            "n" => self.paint(false, false),
            // Colors, line caps and joins, dashes, clipping and the like
            // change no geometry.
            "J" | "j" | "d" | "M" | "W" | "W*" | "cs" | "CS" | "sc" | "SC" | "scn" | "SCN" | "gs" | "ri" | "i" => {}
            _ => return false,
        }
        true
    }

    fn text(&mut self, text: &str) {
        let text = text.trim();
        if !text.is_empty() {
            self.pending.push((text.to_string(), self.colors[1].clone()));
        }
    }

    /// The waiting text, at `(x, y)`.
    fn place(&mut self, x: f64, y: f64) {
        let round = |v: f64| (v * 100.0).round() / 100.0;
        for (text, color) in std::mem::take(&mut self.pending) {
            self.texts.push(json!({ "text": text, "x": round(x), "y": round(-y), "color": color }));
        }
    }
}

/// What the picture from `from` to `to` drew: `{paths, texts, box, approximate}`,
/// lengths in points, `None` when it drew nothing the run knew.
pub fn draw(analysis: &Analysis, from: Span, to: Span) -> Option<Value> {
    let key = |s: Span| (s.line, s.col);
    let inside = |s: Span| s.file == from.file && key(s) >= key(from) && key(s) <= key(to);
    let literals: Vec<_> = analysis.facts.literals.iter().filter(|l| inside(l.at)).collect();
    let chars: Vec<char> = analysis.facts.text.chars.chars().collect();
    let mut canvas = Canvas { ctm: IDENTITY, width: 0.4, ..Canvas::default() };
    for (i, literal) in literals.iter().enumerate() {
        if literal.text.contains(|c: char| crate::engine::tex::is_unknown_char(c) || c == '\u{2}') {
            canvas.approximate = true;
            continue;
        }
        // What was typeset since the literal before goes into the next shape.
        let before = i.checked_sub(1).map_or(literal.chars, |j| literals[j].chars) as usize;
        let set: String = chars.get(before..literal.chars as usize).unwrap_or_default().iter().collect();
        canvas.text(&set);
        canvas.run(&literal.text);
    }
    let (x, y) = apply(canvas.ctm, 0.0, 0.0);
    canvas.place(x, y);
    let b = canvas.box_all?;
    Some(json!({
        "paths": canvas.paths,
        "texts": canvas.texts,
        "box": [b[0], -b[3], b[2] - b[0], b[3] - b[1]],
        "approximate": canvas.approximate,
    }))
}
