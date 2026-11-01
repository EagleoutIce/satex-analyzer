//! A rough sketch of a TikZ picture, read from its source: the nodes as
//! boxes (centimetres, y up) and the paths between them as lines.
//!
//! Positions come from `at (x,y)`, from `right=of a` and `right of=a` against
//! the `node distance`, from a matrix's grid, or from the node before it.
//! Calculations (`$(a)+(1,0)$`), `\foreach` and styles are not followed.

use regex::Regex;
use serde_json::{Value, json};
use std::sync::LazyLock;

const DEFAULT_DISTANCE: f64 = 1.0;
const GAP: f64 = 0.6;
const CHAR_WIDTH: f64 = 0.17;
const PADDING: f64 = 0.5;
const LINE_HEIGHT: f64 = 0.4;
const MIN_WIDTH: f64 = 0.6;
const MIN_HEIGHT: f64 = 0.6;
const TEXT_CLIP: usize = 28;
const CM_PER_PT: f64 = 1.0 / 28.452_756;

static LENGTH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\s*([+-]?\d*\.?\d+)\s*(cm|mm|pt|in|em|ex)?\s*$").unwrap());
static PLACE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(?P<d>(?:above|below)(?: (?:left|right))?|left|right)\s*(?:=\s*(?:(?P<g>\S+)\s+)?of\s+(?P<n>\S+)|\s+of\s*=\s*(?P<m>\S+))$",
    )
    .unwrap()
});
static TIP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^([<>A-Za-z|*]*)-([<>A-Za-z|*]*)$").unwrap());
static BRACKETS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[[^\]]*\]|[{}]").unwrap());
static MACRO: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\[A-Za-z]+\*?").unwrap());
static CELL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?s)\\node\s*(?:\[[^\]]*\])?\s*(?:\(([^)]*)\))?\s*(?:\[[^\]]*\])?\s*\{(.*)\}").unwrap()
});

/// A centimetre length to the millimetre.
fn mm(cm: f64) -> f64 {
    (cm * 10.0).round() / 10.0
}

/// A length in centimetres, `cm` when it has no unit.
fn length(text: &str) -> Option<f64> {
    let m = LENGTH.captures(text)?;
    let n: f64 = m[1].parse().ok()?;
    Some(match m.get(2).map_or("cm", |u| u.as_str()) {
        "mm" => n / 10.0,
        "pt" => n * CM_PER_PT,
        "in" => n * 2.54,
        "em" | "ex" => n * 10.0 * CM_PER_PT,
        _ => n,
    })
}

/// `x,y` in lengths or `angle:radius`.
fn point(text: &str) -> Option<(f64, f64)> {
    if let Some((x, y)) = text.split_once(',') {
        return Some((length(x)?, length(y)?));
    }
    let (angle, radius) = text.split_once(':')?;
    let (a, r) = (angle.trim().parse::<f64>().ok()?.to_radians(), length(radius)?);
    Some((r * a.cos(), r * a.sin()))
}

/// The text of a node as plain words: macros and math marks dropped.
fn plain(text: &str) -> String {
    let spaced = text.replace("\\\\", " ");
    let words = MACRO.replace_all(&spaced, "");
    let kept: String = words.chars().filter(|c| !"${}\\".contains(*c)).collect();
    let kept = kept.split_whitespace().collect::<Vec<_>>().join(" ");
    match kept.char_indices().nth(TEXT_CLIP) {
        Some((at, _)) => format!("{}…", &kept[..at]),
        None => kept,
    }
}

/// The options split at top-level commas.
fn items(text: &str) -> Vec<String> {
    let (mut out, mut depth, mut item) = (Vec::new(), 0i32, String::new());
    for c in text.chars() {
        match c {
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut item).trim().to_string());
                continue;
            }
            _ => {}
        }
        item.push(c);
    }
    out.push(item.trim().to_string());
    out.retain(|i| !i.is_empty());
    out
}

fn option<'a>(opts: &'a [String], key: &str) -> Option<&'a str> {
    opts.iter().find_map(|o| o.strip_prefix(key)?.trim_start().strip_prefix('=').map(str::trim))
}

/// Whether the options put an arrow tip at the start and at the end.
fn arrow(opts: &[String]) -> Option<(bool, bool)> {
    opts.iter().find_map(|o| {
        let bare = BRACKETS.replace_all(o, "").into_owned();
        let m = TIP.captures(&bare)?;
        Some((!m[1].is_empty(), !m[2].is_empty()))
    })
}

#[derive(Clone)]
enum End {
    Name(String),
    At(f64, f64),
    Lost,
}

struct Node {
    name: String,
    text: String,
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    round: bool,
    drawn: bool,
}

/// A character cursor over the source.
struct Cur {
    s: Vec<char>,
    i: usize,
}

impl Cur {
    fn new(s: &str) -> Self {
        Cur { s: s.chars().collect(), i: 0 }
    }
    fn peek(&self) -> Option<char> {
        self.s.get(self.i).copied()
    }
    fn ws(&mut self) {
        while self.peek().is_some_and(char::is_whitespace) {
            self.i += 1;
        }
    }
    fn eat(&mut self, word: &str) -> bool {
        let n = word.chars().count();
        let hit = self.s.get(self.i..self.i + n).is_some_and(|t| t.iter().copied().eq(word.chars()));
        self.i += if hit { n } else { 0 };
        hit
    }
    fn letters(&mut self) -> String {
        let from = self.i;
        while self.peek().is_some_and(|c| c.is_alphabetic() || c == '@') {
            self.i += 1;
        }
        self.s[from..self.i].iter().collect()
    }
    /// The inside of the group opening here, past its close.
    fn group(&mut self, open: char, close: char) -> String {
        let (mut depth, mut out) = (0, String::new());
        while let Some(c) = self.peek() {
            self.i += 1;
            if c == open {
                depth += 1;
                if depth == 1 {
                    continue;
                }
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    break;
                }
            }
            out.push(c);
        }
        out
    }
    /// The next path word: letters, a run of dashes, bars and dots, or one character.
    fn word(&mut self) -> String {
        let from = self.i;
        if self.peek().is_some_and(char::is_alphabetic) {
            self.letters();
        } else if self.peek().is_some_and(|c| "-|.<>".contains(c)) {
            while self.peek().is_some_and(|c| "-|.<>".contains(c)) {
                self.i += 1;
            }
        } else {
            self.i += 1;
        }
        self.s[from..self.i].iter().collect()
    }
}

/// The path being walked: its options, where it is, a connector waiting for its end.
#[derive(Default)]
struct Path {
    arrow: Option<(bool, bool)>,
    cur: Option<End>,
    pending: Option<Option<(bool, bool)>>,
    relative: Option<bool>,
}

struct Pic {
    nodes: Vec<Node>,
    edges: Vec<(End, End, (bool, bool))>,
    vertical: f64,
    horizontal: f64,
}

impl Pic {
    fn find(&self, name: &str) -> Option<&Node> {
        self.nodes.iter().rfind(|n| n.name == name)
    }

    fn spot(&self, end: &End) -> Option<(f64, f64)> {
        match end {
            End::At(x, y) => Some((*x, *y)),
            End::Name(n) => self.find(n).map(|n| (n.x, n.y)),
            End::Lost => None,
        }
    }

    fn end(&self, text: &str) -> End {
        let text = text.trim();
        if let Some((x, y)) = point(text) {
            return End::At(x, y);
        }
        let name = text.split('.').next().unwrap_or("");
        if !name.is_empty() && !name.contains(|c: char| c.is_whitespace() || "$,:()+*!|".contains(c)) {
            return End::Name(name.to_string());
        }
        End::Lost
    }

    /// Where `right=of a`, `below left=1cm of a` or `right of=a` puts a node of this size.
    fn relative(&self, opts: &[String], (w, h): (f64, f64)) -> Option<(f64, f64)> {
        opts.iter().find_map(|o| {
            let m = PLACE.captures(o)?;
            let anchor = self.find(m.name("n").or(m.name("m"))?.as_str().trim_matches(|c| c == '(' || c == ')'))?;
            let gap = m.name("g").and_then(|g| length(g.as_str()));
            let dir = &m["d"];
            let (sx, sy) = (
                f64::from(i8::from(dir.ends_with("right")) - i8::from(dir.ends_with("left"))),
                f64::from(i8::from(dir.starts_with("above")) - i8::from(dir.starts_with("below"))),
            );
            let (gx, gy) = (gap.unwrap_or(self.horizontal), gap.unwrap_or(self.vertical));
            let (dx, dy) = if m.name("m").is_some() {
                (gx.max((anchor.w + w) / 2.0 + GAP / 2.0), gy.max((anchor.h + h) / 2.0 + GAP / 2.0))
            } else {
                ((anchor.w + w) / 2.0 + gx, (anchor.h + h) / 2.0 + gy)
            };
            Some((anchor.x + sx * dx, anchor.y + sy * dy))
        })
    }

    /// The next free place to the right of the last node.
    fn beside(&self, (w, _): (f64, f64)) -> (f64, f64) {
        self.nodes.last().map_or((0.0, 0.0), |n| (n.x + (n.w + w) / 2.0 + GAP, n.y))
    }

    fn place(&self, opts: &[String], anchor: Option<(f64, f64)>, size: (f64, f64)) -> (f64, f64) {
        let shift = |key| option(opts, key).and_then(length).unwrap_or(0.0);
        let (x, y) = anchor.or_else(|| self.relative(opts, size)).unwrap_or_else(|| self.beside(size));
        (x + shift("xshift"), y + shift("yshift"))
    }

    fn size(opts: &[String], text: &str) -> (f64, f64) {
        let lines = text.matches('\n').count() as f64 + 1.0;
        let least = |key| option(opts, key).and_then(length);
        let square = least("minimum size");
        let w = (text.chars().count() as f64 * CHAR_WIDTH + PADDING).max(MIN_WIDTH);
        let h = (lines * LINE_HEIGHT + 0.2).max(MIN_HEIGHT);
        let (w, h) = (
            w.max(least("minimum width").or(square).unwrap_or(0.0)),
            h.max(least("minimum height").or(square).unwrap_or(0.0)),
        );
        if opts.iter().any(|o| o == "circle") { (w.max(h), w.max(h)) } else { (w, h) }
    }

    fn add(
        &mut self,
        name: Option<String>,
        text: &str,
        opts: &[String],
        anchor: Option<(f64, f64)>,
        drawn: bool,
    ) -> End {
        let (w, h) = if drawn { Self::size(opts, text) } else { (0.0, 0.0) };
        let (x, y) = self.place(opts, anchor, (w, h));
        let name = name.unwrap_or_else(|| format!("#{}", self.nodes.len()));
        let round = opts.iter().any(|o| o == "circle" || o == "ellipse");
        self.nodes.push(Node { name: name.clone(), text: text.to_string(), x, y, w, h, round, drawn });
        End::Name(name)
    }

    /// A `node`, `coordinate` or `matrix` met on a path.
    fn thing(&mut self, kind: &str, c: &mut Cur, path: &mut Path) {
        let (mut opts, mut name, mut at) = (Vec::new(), None, None);
        loop {
            c.ws();
            match c.peek() {
                Some('[') => opts.extend(items(&c.group('[', ']'))),
                Some('(') => name = Some(c.group('(', ')').trim().to_string()),
                Some('a') if c.s.get(c.i + 2).is_none_or(|n| !n.is_alphabetic()) && c.eat("at") => {
                    c.ws();
                    at = Some(self.end(&c.group('(', ')')));
                }
                _ => break,
            }
        }
        c.ws();
        let body = if c.peek() == Some('{') { c.group('{', '}') } else { String::new() };
        let anchor = at.as_ref().or(path.cur.as_ref()).and_then(|e| self.spot(e));
        let label =
            at.is_none() && (path.pending.is_some() || (name.is_none() && matches!(path.cur, Some(End::Name(_)))));
        if kind == "node" && label {
            return;
        }
        let here = match kind {
            "matrix" => self.matrix(name, &body, &opts, anchor),
            "coordinate" => self.add(name, "", &opts, anchor, false),
            _ => self.add(name, &plain(&body), &opts, anchor, true),
        };
        path.cur = Some(here);
    }

    /// A matrix's cells as a grid of nodes around its anchor, the matrix itself an unseen node.
    fn matrix(&mut self, name: Option<String>, body: &str, opts: &[String], anchor: Option<(f64, f64)>) -> End {
        let rows: Vec<Vec<(Option<String>, String)>> = body
            .split("\\\\")
            .map(|row| {
                row.split('&')
                    .map(|cell| match CELL.captures(cell) {
                        Some(m) => (m.get(1).map(|n| n.as_str().to_string()), plain(&m[2])),
                        None => (None, plain(cell)),
                    })
                    .collect()
            })
            .filter(|row: &Vec<_>| row.iter().any(|(_, t)| !t.is_empty()))
            .collect();
        let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
        let width =
            |c: usize| rows.iter().filter_map(|r| r.get(c)).map(|(_, t)| Self::size(&[], t).0).fold(0.0, f64::max);
        let (widths, row_h) = ((0..columns).map(width).collect::<Vec<_>>(), MIN_HEIGHT + LINE_HEIGHT);
        let total = (widths.iter().sum::<f64>() + GAP * columns.saturating_sub(1) as f64, row_h * rows.len() as f64);
        let (cx, cy) = self.place(opts, anchor, total);
        let whole = self.add(name.clone(), "", &[], Some((cx, cy)), false);
        if let Some(n) = self.nodes.last_mut() {
            (n.w, n.h) = total;
        }
        for (r, row) in rows.iter().enumerate() {
            let mut x = cx - total.0 / 2.0;
            for (c, (cell, text)) in row.iter().enumerate() {
                let mid = x + widths[c] / 2.0;
                x += widths[c] + GAP;
                if text.is_empty() {
                    continue;
                }
                let at = (mid, cy + total.1 / 2.0 - row_h * (r as f64 + 0.5));
                let id = cell.clone().or_else(|| name.as_ref().map(|n| format!("{n}-{}-{}", r + 1, c + 1)));
                self.add(id, text, &[], Some(at), true);
            }
        }
        whole
    }

    fn reach(&mut self, path: &mut Path, end: End) {
        let end = match (path.relative.take(), path.cur.as_ref().and_then(|c| self.spot(c)), &end) {
            (Some(_), Some((x, y)), End::At(dx, dy)) => End::At(x + dx, y + dy),
            (Some(_), _, _) => End::Lost,
            _ => end,
        };
        if let (Some(tip), Some(from)) = (path.pending.take(), path.cur.clone()) {
            self.edges.push((from, end.clone(), tip.or(path.arrow).unwrap_or_default()));
        }
        path.cur = Some(end);
    }

    /// One statement: `\node`, `\matrix`, `\coordinate` or a path command.
    fn statement(&mut self, text: &str) {
        let mut c = Cur::new(text);
        loop {
            c.ws();
            if !(c.eat("\\begin") || c.eat("\\end")) {
                break;
            }
            c.group('{', '}');
            c.ws();
            if c.peek() == Some('[') {
                c.group('[', ']');
            }
        }
        if !c.eat("\\") {
            return;
        }
        let command = c.letters();
        let mut path = Path::default();
        match command.as_str() {
            "node" | "matrix" | "coordinate" => self.thing(&command, &mut c, &mut path),
            "draw" | "path" | "fill" | "filldraw" | "shade" | "shadedraw" | "clip" => {}
            _ => return,
        }
        loop {
            c.ws();
            let Some(ch) = c.peek() else { break };
            match ch {
                '[' => {
                    let opts = items(&c.group('[', ']'));
                    match (&mut path.pending, &path.cur) {
                        (Some(tip), _) => *tip = arrow(&opts).or(*tip),
                        (None, None) => path.arrow = arrow(&opts),
                        _ => {}
                    }
                }
                '(' => {
                    let end = self.end(&c.group('(', ')'));
                    self.reach(&mut path, end);
                }
                '{' => {
                    c.group('{', '}');
                }
                _ => match c.word().as_str() {
                    "--" | "-|" | "|-" | "to" | "edge" => path.pending = Some(None),
                    "+" => path.relative = Some(false),
                    "++" => path.relative = Some(true),
                    kind @ ("node" | "coordinate" | "matrix") => self.thing(kind, &mut c, &mut path),
                    _ => {}
                },
            }
        }
    }

    /// The nodes drawn and the edges between nodes or points that can be placed.
    fn json(&self) -> Option<Value> {
        let nodes: Vec<Value> = self
            .nodes
            .iter()
            .filter(|n| n.drawn)
            .map(
                |n| json!({ "text": n.text, "x": mm(n.x), "y": mm(n.y), "w": mm(n.w), "h": mm(n.h), "round": n.round }),
            )
            .collect();
        let edges: Vec<Value> = self
            .edges
            .iter()
            .filter_map(|(a, b, (start, end))| {
                let (p, q) = (self.spot(a)?, self.spot(b)?);
                let (p, q) = (self.clip(a, p, q), self.clip(b, q, p));
                let tip = match (start, end) {
                    (true, true) => "both",
                    (true, false) => "start",
                    (false, true) => "end",
                    _ => "none",
                };
                (p != q).then(|| json!({ "x1": mm(p.0), "y1": mm(p.1), "x2": mm(q.0), "y2": mm(q.1), "arrow": tip }))
            })
            .collect();
        (!nodes.is_empty() || !edges.is_empty()).then(|| json!({ "nodes": nodes, "edges": edges }))
    }

    /// Where the line from `from` toward `to` leaves the box of the node at its start.
    fn clip(&self, end: &End, from: (f64, f64), to: (f64, f64)) -> (f64, f64) {
        let End::Name(name) = end else { return from };
        let Some(n) = self.find(name).filter(|n| n.drawn) else { return from };
        let (dx, dy) = (to.0 - from.0, to.1 - from.1);
        let reach = |half: f64, d: f64| if d == 0.0 { f64::INFINITY } else { half / d.abs() };
        let t = reach(n.w / 2.0, dx).min(reach(n.h / 2.0, dy));
        if t < 1.0 { (from.0 + dx * t, from.1 + dy * t) } else { from }
    }
}

/// The statements of a picture's body, split at top-level semicolons.
fn statements(body: &str) -> Vec<String> {
    let (mut out, mut depth, mut cur) = (Vec::new(), 0i32, String::new());
    for c in body.chars() {
        match c {
            '{' | '[' | '(' => depth += 1,
            '}' | ']' | ')' => depth -= 1,
            ';' if depth == 0 => {
                out.push(std::mem::take(&mut cur));
                continue;
            }
            _ => {}
        }
        cur.push(c);
    }
    out.push(cur);
    out
}

fn uncommented(source: &str) -> String {
    source
        .lines()
        .map(|line| {
            let mut end = line.len();
            let mut escaped = false;
            for (i, c) in line.char_indices() {
                if c == '%' && !escaped {
                    end = i;
                    break;
                }
                escaped = c == '\\' && !escaped;
            }
            &line[..end]
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// The sketch of the picture whose source, `\begin{tikzpicture}` to `\end{tikzpicture}`, this is.
pub fn sketch(source: &str) -> Option<Value> {
    let source = uncommented(source);
    let mut c = Cur::new(&source);
    c.eat("\\begin");
    c.group('{', '}');
    c.ws();
    let picture = if c.peek() == Some('[') { items(&c.group('[', ']')) } else { Vec::new() };
    let rest: String = c.s[c.i..].iter().collect();
    let body = rest.rfind("\\end{").map_or(rest.as_str(), |at| &rest[..at]);
    let (vertical, horizontal) = match option(&picture, "node distance") {
        Some(d) => match d.split_once("and") {
            Some((v, h)) => (length(v), length(h)),
            None => (length(d), length(d)),
        },
        None => (None, None),
    };
    let mut pic = Pic {
        nodes: Vec::new(),
        edges: Vec::new(),
        vertical: vertical.unwrap_or(DEFAULT_DISTANCE),
        horizontal: horizontal.unwrap_or(DEFAULT_DISTANCE),
    };
    for statement in statements(body) {
        pic.statement(&statement);
    }
    pic.json()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nodes_and_arrows() {
        let s = sketch(
            "\\begin{tikzpicture}[node distance=2cm]\n\\node (a) {A};\n\\node (b) [right=of a] {B};\n\\node (c) at (0,-2) {C};\n\\draw[->] (a) -- (b);\n\\draw (a) edge[<->] (c);\n\\end{tikzpicture}",
        )
        .unwrap();
        let nodes = s["nodes"].as_array().unwrap();
        assert_eq!(nodes.len(), 3);
        assert!(nodes[1]["x"].as_f64().unwrap() > 2.0);
        assert_eq!(nodes[2]["y"].as_f64().unwrap(), -2.0);
        let arrows: Vec<_> = s["edges"].as_array().unwrap().iter().map(|e| e["arrow"].as_str().unwrap()).collect();
        assert_eq!(arrows, ["end", "both"]);
    }

    #[test]
    fn matrix_cells_are_named() {
        let s = sketch(
            "\\begin{tikzpicture}\\matrix (m) {A & B \\\\ C & D \\\\};\\draw[->] (m-1-1) -- (m-2-2);\\end{tikzpicture}",
        )
        .unwrap();
        assert_eq!(s["nodes"].as_array().unwrap().len(), 4);
        assert_eq!(s["edges"].as_array().unwrap().len(), 1);
    }
}
