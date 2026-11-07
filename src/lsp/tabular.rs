//! The table plugin's model: a `tabular`'s text as the grid its dialog
//! edits, and the grid written back. A row the dialog left as it was keeps
//! its own text; new and edited rows follow the table's style.

use serde::{Deserialize, Serialize};

/// A column: its letter (`l`, `p`, `S`, a `\newcolumntype`), its argument
/// (`p`'s width, `S`'s options), what `>{…}` and `<{…}` add.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Col {
    pub align: String,
    #[serde(default)]
    pub arg: String,
    #[serde(default)]
    pub pre: String,
    #[serde(default)]
    pub post: String,
    /// The package that gives the letter, for when the run lacks it (the dialog's menu says).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

/// A cell: its text, and for a `\multicolumn` the columns it spans and its spec.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Cell {
    pub text: String,
    #[serde(default = "one")]
    pub span: usize,
    #[serde(default)]
    pub spec: String,
}

fn one() -> usize {
    1
}

/// A row with the rules and comments before it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Row {
    #[serde(default)]
    pub before: Vec<String>,
    /// Whether `before` stood on the previous row's line.
    #[serde(default)]
    pub inline: bool,
    pub cells: Vec<Cell>,
    /// `\\` with its `*` or `[skip]`, empty on a last row without one.
    #[serde(default)]
    pub end: String,
    #[serde(default)]
    pub comment: String,
    /// The row's own text, kept while its cells are as parsed from it.
    #[serde(default)]
    pub raw: String,
}

/// How the table is written: indentation, `&` with spaces, padded columns,
/// rules on the row line before them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Style {
    pub indent: String,
    pub spaced: bool,
    pub aligned: bool,
    pub inline_rules: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Table {
    pub env: String,
    /// The braced argument before the spec (`tabular*`'s, `tabularx`'s width).
    #[serde(default)]
    pub width: Option<String>,
    #[serde(default)]
    pub pos: Option<String>,
    pub cols: Vec<Col>,
    /// What stands between columns (`|`, `@{}`): one more than `cols`.
    pub seps: Vec<String>,
    pub rows: Vec<Row>,
    #[serde(default)]
    pub after: Vec<String>,
    #[serde(default)]
    pub after_inline: bool,
    pub style: Style,
    /// The rest of the `\begin` line (a comment), and the `\end`'s indentation.
    #[serde(default)]
    pub head_tail: String,
    #[serde(default)]
    pub end_indent: String,
}

/// Rules a row may have before it.
const RULES: &[&str] =
    &["hline", "toprule", "midrule", "bottomrule", "cline", "cmidrule", "addlinespace", "specialrule", "morecmidrules"];

/// The end of the group opening at `s[i] == open`, past its close.
pub(super) fn group_end(s: &str, i: usize) -> Option<usize> {
    let b = s.as_bytes();
    let (open, close) = match b.get(i)? {
        b'{' => (b'{', b'}'),
        b'[' => (b'[', b']'),
        b'(' => (b'(', b')'),
        _ => return None,
    };
    let mut depth = 0usize;
    let mut j = i;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 1,
            b'%' => {
                while j < b.len() && b[j] != b'\n' {
                    j += 1;
                }
            }
            c if c == open => depth += 1,
            c if c == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(j + 1);
                }
            }
            b'{' if open != b'{' => j = group_end(s, j)? - 1,
            _ => {}
        }
        j += 1;
    }
    None
}

fn skip_ws(s: &str, mut i: usize) -> usize {
    while s[i..].starts_with([' ', '\t']) {
        i += 1;
    }
    i
}

/// The table environments the run knows: `\begin{name}` runs into `\@array`, not through a saved copy.
pub(super) fn is_table(analysis: &crate::analysis::machine::Analysis, name: &str) -> bool {
    let array = analysis.interner.lookup("@array");
    array.is_some_and(|a| super::handlers::body_reach(analysis, name, 400, false).contains(&a))
}

/// The `\begin{name}` at or before `at` on its line (else after it), and
/// the offset past its matching `\end{name}`.
pub(super) fn extent(text: &str, at: usize, name: &str) -> Option<(usize, usize)> {
    let begin = format!("\\begin{{{name}}}");
    let line = text[..at.min(text.len())].rfind('\n').map_or(0, |n| n + 1);
    let line_end = text[line..].find('\n').map_or(text.len(), |n| line + n);
    let mut upto = (at + begin.len()).min(line_end);
    while !text.is_char_boundary(upto) {
        upto -= 1;
    }
    let start = text[line..upto]
        .rfind(&begin)
        .map(|n| line + n)
        .or_else(|| text[line..line_end].find(&begin).map(|n| line + n))?;
    let end_tag = format!("\\end{{{name}}}");
    let (mut depth, mut j) = (0usize, start);
    while j < text.len() {
        let rest = &text[j..];
        if rest.starts_with('%') {
            j += rest.find('\n').unwrap_or(rest.len());
            continue;
        }
        if rest.starts_with("\\\\") || rest.starts_with("\\%") {
            j += 2;
            continue;
        }
        if rest.starts_with(&begin) {
            depth += 1;
        } else if rest.starts_with(&end_tag) {
            depth -= 1;
            if depth == 0 {
                return Some((start, j + end_tag.len()));
            }
        }
        j += rest.chars().next().map_or(1, char::len_utf8);
    }
    None
}

/// The columns and separators of a spec (`|l|c@{}S[table-format=1.2]`).
pub fn parse_spec(spec: &str) -> (Vec<Col>, Vec<String>) {
    let (mut cols, mut seps) = (Vec::new(), vec![String::new()]);
    let mut pre = String::new();
    let mut i = 0;
    let s = spec;
    while i < s.len() {
        let c = s[i..].chars().next().unwrap_or(' ');
        let arg = |from: usize| group_end(s, from).map(|e| (s[from + 1..e - 1].to_string(), e));
        match c {
            ' ' | '\t' | '\n' => i += 1,
            '|' | ':' => {
                seps.last_mut().unwrap().push(c);
                i += 1;
            }
            '@' | '!' => {
                let Some((a, e)) = arg(i + 1) else { break };
                seps.last_mut().unwrap().push_str(&format!("{c}{{{a}}}"));
                i = e;
            }
            '>' => {
                let Some((a, e)) = arg(i + 1) else { break };
                pre.push_str(&format!(">{{{a}}}"));
                i = e;
            }
            '<' => {
                let Some((a, e)) = arg(i + 1) else { break };
                if let Some(col) = cols.last_mut() {
                    Col::post_push(col, &a);
                }
                i = e;
            }
            '*' => {
                let Some((n, e)) = arg(i + 1) else { break };
                let Some((body, e)) = arg(e) else { break };
                let (more, more_seps) = parse_spec(&body);
                for _ in 0..n.trim().parse::<usize>().unwrap_or(1) {
                    seps.last_mut().unwrap().push_str(&more_seps[0]);
                    for (k, col) in more.iter().enumerate() {
                        cols.push(col.clone());
                        seps.push(more_seps[k + 1].clone());
                    }
                }
                i = e;
            }
            _ => {
                i += c.len_utf8();
                let mut col = Col { align: c.to_string(), pre: std::mem::take(&mut pre), ..Default::default() };
                let (open, wants) = (s[i..].chars().next(), matches!(c, 'p' | 'm' | 'b' | 'w' | 'W'));
                if ((wants && open == Some('{')) || (c == 'S' && open == Some('[')))
                    && let Some((a, e)) = arg(i)
                {
                    col.arg = a;
                    i = e;
                }
                cols.push(col);
                seps.push(String::new());
            }
        }
    }
    (cols, seps)
}

pub fn spec_text(cols: &[Col], seps: &[String]) -> String {
    let mut out = seps.first().cloned().unwrap_or_default();
    for (k, c) in cols.iter().enumerate() {
        out.push_str(&c.pre);
        out.push_str(&col_text(c));
        out.push_str(&c.post);
        out.push_str(seps.get(k + 1).map_or("", String::as_str));
    }
    out
}

impl Col {
    fn post_push(&mut self, a: &str) {
        self.post.push_str(&format!("<{{{a}}}"));
    }
}

fn col_text(c: &Col) -> String {
    match (c.align.as_str(), c.arg.is_empty()) {
        ("S", false) => format!("S[{}]", c.arg),
        ("p" | "m" | "b" | "w" | "W", _) => format!("{}{{{}}}", c.align, c.arg),
        (a, _) => a.to_string(),
    }
}

/// A rule command at the start of `s`, with its arguments.
fn rule_at(s: &str) -> Option<usize> {
    let name = s.strip_prefix('\\')?;
    let len = name.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(name.len());
    if !RULES.contains(&&name[..len]) {
        return None;
    }
    let mut i = 1 + len;
    while let Some(e) = s[i..].starts_with(['[', '(', '{']).then(|| group_end(s, i)).flatten() {
        i = e;
    }
    Some(i)
}

/// A cell's text as a cell: a `\multicolumn{n}{spec}{text}` spans.
fn cell(text: &str) -> Cell {
    let t = text.trim();
    let multi = t.strip_prefix("\\multicolumn").and_then(|r| {
        let at = t.len() - r.len();
        let mut groups = Vec::new();
        let mut i = skip_ws(t, at);
        while groups.len() < 3 {
            let e = group_end(t, i).filter(|_| t[i..].starts_with('{'))?;
            groups.push(t[i + 1..e - 1].to_string());
            i = skip_ws(t, e);
        }
        (i == t.len()).then_some(groups)
    });
    match multi {
        Some(g) => Cell { span: g[0].trim().parse().unwrap_or(1), spec: g[1].clone(), text: g[2].clone() },
        None => Cell { text: t.to_string(), span: 1, spec: String::new() },
    }
}

fn cell_text(c: &Cell) -> String {
    match c.span > 1 || !c.spec.is_empty() {
        true => format!("\\multicolumn{{{}}}{{{}}}{{{}}}", c.span, c.spec, c.text),
        false => c.text.clone(),
    }
}

/// The table at `text[start..end]` (from `\begin` to past `\end`).
pub fn parse(text: &str) -> Option<Table> {
    let s = text;
    let name_end = group_end(s, "\\begin".len())?;
    let env = s["\\begin{".len()..name_end - 1].to_string();
    let (mut groups, mut pos, mut i) = (Vec::new(), None, name_end);
    loop {
        let j = skip_ws(s, i);
        match s[j..].chars().next() {
            Some('{') if groups.len() < 2 => {
                let e = group_end(s, j)?;
                groups.push(s[j + 1..e - 1].to_string());
                i = e;
            }
            Some('[') if pos.is_none() => {
                let e = group_end(s, j)?;
                pos = Some(s[j + 1..e - 1].to_string());
                i = e;
            }
            _ => break,
        }
        // a table's spec is its last braced argument: one, or two after a width
        if groups.len() == 2 || (groups.len() == 1 && !env.ends_with('*') && !env.ends_with('x')) {
            break;
        }
    }
    let spec = groups.pop()?;
    let width = groups.pop();
    let (cols, seps) = parse_spec(&spec);
    let end_at = s.rfind("\\end{")?;
    let line_end = s[i..end_at].find('\n').map_or(end_at, |n| i + n);
    let head_tail = s[i..line_end].to_string();
    let end_indent: String = s[..end_at].rsplit('\n').next().unwrap_or("").to_string();
    let end_indent = if end_indent.trim().is_empty() { end_indent } else { String::new() };
    let body = &s[(line_end + 1).min(end_at)..end_at - end_indent.len()];
    let mut t = Table { env, width, pos, cols, seps, head_tail, end_indent, ..Default::default() };
    body_rows(body, &mut t);
    Some(t)
}

/// Splits the body into rows at `\\`, cells at `&`, outside groups.
fn body_rows(b: &str, t: &mut Table) {
    let mut row = Row::default();
    let (mut cur, mut cells) = (String::new(), Vec::<String>::new());
    let (mut row_start, mut i, mut line_has_row) = (None::<usize>, 0, false);
    let mut indent = None;
    let mut seen_inline = false;
    while i < b.len() {
        let rest = &b[i..];
        let empty = cells.is_empty() && cur.trim().is_empty();
        if empty {
            if rest.starts_with(['\n']) {
                line_has_row = false;
                cur.clear();
                i += 1;
                continue;
            }
            if rest.starts_with([' ', '\t']) {
                cur.push_str(&rest[..1]);
                i += 1;
                continue;
            }
            if let Some(n) =
                rule_at(rest).or_else(|| rest.starts_with('%').then(|| rest.find('\n').unwrap_or(rest.len())))
            {
                if !line_has_row && indent.is_none() {
                    indent = Some(cur.clone());
                }
                if line_has_row && row.before.is_empty() && !rest.starts_with('%') {
                    row.inline = true;
                    seen_inline = true;
                }
                row.before.push(rest[..n].to_string());
                cur.clear();
                i += n;
                continue;
            }
        }
        if row_start.is_none() {
            if indent.is_none() && !line_has_row {
                indent = Some(cur.clone());
            }
            // a row opening with an empty cell (under a `\multirow`) keeps its spaces past the indent
            let extra = match rest.starts_with('&') {
                true => cur.len().saturating_sub(indent.as_ref().map_or(0, String::len)),
                false => 0,
            };
            cur.clear();
            row_start = Some(i - extra);
        }
        let c = rest.chars().next().unwrap_or(' ');
        match c {
            '{' => {
                let e = group_end(b, i).unwrap_or(b.len());
                cur.push_str(&b[i..e]);
                i = e;
            }
            '&' => {
                cells.push(std::mem::take(&mut cur));
                i += 1;
            }
            '%' => {
                let e = rest.find('\n').map_or(b.len(), |n| i + n + 1);
                cur.push_str(&b[i..e]);
                i = e;
            }
            '\\' if rest.starts_with("\\\\") => {
                let mut e = i + 2;
                if b[e..].starts_with('*') {
                    e += 1;
                }
                let k = skip_ws(b, e);
                if b[k..].starts_with('[') {
                    e = group_end(b, k).unwrap_or(e);
                }
                cells.push(std::mem::take(&mut cur));
                row.end = b[i..e].to_string();
                let k = skip_ws(b, e);
                if b[k..].starts_with('%') {
                    let ce = b[k..].find('\n').map_or(b.len(), |n| k + n);
                    row.comment = b[k..ce].to_string();
                    e = ce;
                }
                row.raw = b[row_start.take().unwrap_or(i)..e].to_string();
                row.cells = cells.drain(..).map(|c| cell(&c)).collect();
                t.rows.push(std::mem::take(&mut row));
                line_has_row = true;
                i = e;
            }
            '\\' => {
                let n = 1 + rest[1..].chars().next().map_or(0, char::len_utf8);
                cur.push_str(&rest[..n]);
                i += n;
            }
            _ => {
                cur.push(c);
                i += c.len_utf8();
            }
        }
    }
    if !cells.is_empty() || !cur.trim().is_empty() {
        cells.push(cur);
        row.raw = b[row_start.unwrap_or(0)..].trim_end().to_string();
        row.cells = cells.into_iter().map(|c| cell(&c)).collect();
        t.rows.push(row);
    } else {
        t.after_inline = row.inline;
        t.after = row.before;
    }
    t.style.indent = indent.unwrap_or_default();
    t.style.inline_rules = seen_inline;
    t.style.spaced = t.rows.iter().any(|r| r.raw.contains(" & ")) || !t.rows.iter().any(|r| r.raw.contains('&'));
    t.style.aligned = aligned(&t.rows);
}

/// Whether the rows pad their cells so the `&`s line up.
fn aligned(rows: &[Row]) -> bool {
    let amps = |r: &Row| -> Option<Vec<usize>> {
        (r.cells.iter().all(|c| c.span == 1) && !r.raw.contains('\n'))
            .then(|| r.raw.char_indices().filter(|(_, c)| *c == '&').map(|(k, _)| r.raw[..k].chars().count()).collect())
    };
    let all: Vec<Vec<usize>> = rows.iter().filter_map(amps).filter(|a| !a.is_empty()).collect();
    all.len() >= 2 && all.windows(2).all(|w| w[0] == w[1]) && rows.iter().any(|r| r.raw.contains("  &"))
}

/// The row's text: its own while its cells are as it has them.
fn row_text(r: &Row, widths: &[usize], style: &Style) -> String {
    if !r.raw.is_empty()
        && let Some(old) = parse_row(&r.raw)
        && old.cells == r.cells
        && old.end == r.end
        && old.comment == r.comment
    {
        return r.raw.clone();
    }
    let amp = if style.spaced { " & " } else { "&" };
    let mut out = String::new();
    let mut col = 0;
    let n = r.cells.len();
    for (k, c) in r.cells.iter().enumerate() {
        let text = cell_text(c);
        out.push_str(&text);
        if k + 1 < n {
            if style.aligned && c.span == 1 {
                let w = widths.get(col).copied().unwrap_or(0);
                out.push_str(&" ".repeat(w.saturating_sub(text.chars().count())));
            }
            out.push_str(amp);
        }
        col += c.span.max(1);
    }
    if !r.end.is_empty() {
        out.push_str(if style.spaced { " " } else { "" });
        out.push_str(&r.end);
    }
    if !r.comment.is_empty() {
        out.push(' ');
        out.push_str(&r.comment);
    }
    out
}

/// One row's text read back as a row, to tell an edited row from its text.
fn parse_row(raw: &str) -> Option<Row> {
    let mut t = Table::default();
    body_rows(raw, &mut t);
    (t.rows.len() == 1).then(|| t.rows.remove(0))
}

/// The table's text, from `\begin` to past `\end`.
pub fn emit(t: &Table) -> String {
    let mut out = format!("\\begin{{{}}}", t.env);
    if let Some(w) = &t.width {
        out.push_str(&format!("{{{w}}}"));
    }
    if let Some(p) = &t.pos {
        out.push_str(&format!("[{p}]"));
    }
    out.push_str(&format!("{{{}}}{}", spec_text(&t.cols, &t.seps), t.head_tail));
    let ncols = t.cols.len();
    let mut widths = vec![0; ncols];
    for r in t.rows.iter().filter(|r| r.cells.iter().all(|c| c.span == 1)) {
        for (k, c) in r.cells.iter().enumerate().filter(|(k, _)| k + 1 < r.cells.len()) {
            if let Some(w) = widths.get_mut(k) {
                *w = (*w).max(c.text.chars().count());
            }
        }
    }
    let ind = &t.style.indent;
    let rules = |out: &mut String, items: &[String], inline: bool| {
        let inline = inline && items.iter().all(|r| !r.starts_with('%')) && !out.ends_with('\n');
        for r in items {
            if inline {
                out.push(' ');
            } else {
                out.push('\n');
                out.push_str(ind);
            }
            out.push_str(r);
        }
    };
    for r in &t.rows {
        rules(&mut out, &r.before, r.inline);
        out.push('\n');
        out.push_str(ind);
        out.push_str(&row_text(r, &widths, &t.style));
    }
    rules(&mut out, &t.after, t.after_inline);
    out.push('\n');
    out.push_str(&t.end_indent);
    out.push_str(&format!("\\end{{{}}}", t.env));
    out
}

/// booktabs' rules for the table's: `\toprule` first, `\midrule` between,
/// `\bottomrule` last, `\cmidrule` for `\cline`; no vertical rules.
pub fn booktabs(t: &mut Table) {
    let n = t.rows.len();
    let map = |items: &mut Vec<String>, rule: &str| {
        let mut out: Vec<String> = Vec::new();
        let mut put = false;
        for it in items.drain(..) {
            if it.starts_with("\\hline")
                || it.starts_with("\\toprule")
                || it.starts_with("\\midrule")
                || it.starts_with("\\bottomrule")
            {
                if !put {
                    out.push(rule.to_string());
                    put = true;
                }
            } else if let Some(arg) = it.strip_prefix("\\cline") {
                out.push(format!("\\cmidrule(lr){arg}"));
            } else {
                out.push(it);
            }
        }
        *items = out;
    };
    for (k, r) in t.rows.iter_mut().enumerate() {
        map(&mut r.before, if k == 0 { "\\toprule" } else { "\\midrule" });
    }
    map(&mut t.after, "\\bottomrule");
    let has = |items: &[String], rule: &str| items.iter().any(|r| r == rule);
    if n > 0 && !has(&t.rows[0].before, "\\toprule") {
        t.rows[0].before.insert(0, "\\toprule".into());
    }
    if n > 1 && !t.rows[1..].iter().any(|r| has(&r.before, "\\midrule")) {
        t.rows[1].before.push("\\midrule".into());
        t.rows[1].inline = t.style.inline_rules;
    }
    if !has(&t.after, "\\bottomrule") {
        t.after.push("\\bottomrule".into());
    }
    for s in &mut t.seps {
        *s = s.replace('|', "");
    }
}

/// `tabularx` for a `tabular` with `X` columns (they need a width); whether it switched.
pub fn to_tabularx(t: &mut Table) -> bool {
    let switch = t.env == "tabular" && t.cols.iter().any(|c| c.align == "X");
    if switch {
        t.env = "tabularx".into();
        t.width = Some("\\linewidth".into());
    }
    switch
}

/// A number siunitx reads: digits, a decimal mark, an exponent, an uncertainty.
fn number(s: &str) -> Option<(usize, usize)> {
    let s = s.trim().trim_start_matches(['+', '-', '−']);
    let (mant, _) = s.split_once(['e', 'E']).map_or((s, ""), |(m, e)| (m, e));
    let mant = mant.split(['±']).next()?.trim();
    let (int, dec) = mant.split_once(['.', ',']).unwrap_or((mant, ""));
    let digits = |x: &str| x.chars().all(|c| c.is_ascii_digit());
    (!(int.is_empty() && dec.is_empty()) && digits(int) && digits(dec)).then_some((int.len().max(1), dec.len()))
}

/// The columns whose cells below a header are all numbers.
pub fn numeric_columns(t: &Table) -> Vec<(usize, String)> {
    let mut out = Vec::new();
    for k in 0..t.cols.len() {
        if !matches!(t.cols[k].align.as_str(), "l" | "c" | "r") {
            continue;
        }
        let (mut int, mut dec, mut nums, mut ok, mut header) = (0, 0, 0, true, true);
        for r in &t.rows {
            let Some(c) = at_col(r, k) else { continue };
            let text = c.text.trim();
            match number(text) {
                Some((i, d)) => {
                    (int, dec, nums, header) = (int.max(i), dec.max(d), nums + 1, false);
                }
                None if text.is_empty() || header => {}
                None => ok = false,
            }
        }
        if ok && nums > 0 {
            out.push((k, format!("table-format={int}.{dec}")));
        }
    }
    out
}

/// The cell starting at column `k`, if one does and spans only it.
fn at_col(r: &Row, k: usize) -> Option<&Cell> {
    let mut col = 0;
    for c in &r.cells {
        if col == k {
            return (c.span == 1).then_some(c);
        }
        col += c.span.max(1);
    }
    None
}

/// siunitx `S` columns for the numeric ones; a text cell in them is braced.
pub fn siunitx(t: &mut Table) -> bool {
    let cols = numeric_columns(t);
    for (k, format) in &cols {
        t.cols[*k].align = "S".into();
        t.cols[*k].arg = format.clone();
        t.cols[*k].package = Some("siunitx".into());
    }
    protect_s(t);
    !cols.is_empty()
}

/// Braces the text cells of the S columns (a header `Score`): siunitx reads every other cell as a number.
pub fn protect_s(t: &mut Table) {
    for r in &mut t.rows {
        let mut col = 0;
        for c in &mut r.cells {
            let text = c.text.trim();
            let s = t.cols.get(col).is_some_and(|k| k.align == "S");
            if s && c.span == 1 && !text.is_empty() && number(text).is_none() && !text.starts_with('{') {
                c.text = format!("{{{text}}}");
            }
            col += c.span.max(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAPER: &str = "\\begin{tabular}{lc}\n    Method & Score \\\\ \\hline\n    Ours     & 0.92  \\\\\n    Baseline & 1.5   \\\\\n  \\end{tabular}";

    #[test]
    fn a_table_round_trips_its_text() {
        let cases = [
            PAPER,
            "\\begin{tabular}{|l|r|}\n  \\hline\n  a & b \\\\\n  \\hline\n  c & d \\\\\n  \\hline\n\\end{tabular}",
            "\\begin{tabular*}{\\linewidth}[t]{@{}l@{\\extracolsep{\\fill}}S[table-format=1.2]@{}} % note\n\\toprule\nx & {y} \\\\ % head\n\\midrule\n\\multicolumn{2}{c}{both} \\\\[2pt]\na\\&b & {1 & 2} \\\\\n% a full-line comment\nlast & 1 \\\\\n\\bottomrule\n\\end{tabular*}",
        ];
        for c in cases {
            let t = parse(c).unwrap();
            assert_eq!(emit(&t), c, "{t:#?}");
        }
    }

    #[test]
    fn cells_split_outside_groups_and_comments() {
        let t = parse("\\begin{tabular}{ll}\n{a & b} & c % x & y\n\\\\ \\multicolumn{2}{|c|}{m}\\\\\n\\end{tabular}")
            .unwrap();
        assert_eq!(t.rows[0].cells.len(), 2);
        assert_eq!(t.rows[0].cells[0].text, "{a & b}");
        assert_eq!((t.rows[1].cells[0].span, t.rows[1].cells[0].spec.as_str()), (2, "|c|"));
    }

    #[test]
    fn edited_rows_follow_the_style() {
        let mut t = parse(PAPER).unwrap();
        t.rows[1].cells[1].text = "0.95".into();
        t.rows.push(Row {
            cells: vec![
                Cell { text: "New".into(), span: 1, spec: String::new() },
                Cell { text: "2".into(), span: 1, spec: String::new() },
            ],
            end: "\\\\".into(),
            ..Default::default()
        });
        let out = emit(&t);
        assert!(out.contains("    Ours & 0.95 \\\\\n    Baseline & 1.5   \\\\\n    New & 2 \\\\\n"), "{out}");
    }

    #[test]
    fn booktabs_and_siunitx_rewrite_rules_and_columns() {
        let mut t = parse("\\begin{tabular}{|l|c|}\n\\hline\nMethod & Score \\\\ \\hline\\hline\nOurs & 0.92 \\\\ \\cline{1-2}\nBase & 11.5 \\\\\n\\hline\n\\end{tabular}").unwrap();
        booktabs(&mut t);
        assert!(siunitx(&mut t));
        let out = emit(&t);
        assert_eq!(
            out,
            "\\begin{tabular}{lS[table-format=2.2]}\n\\toprule\nMethod & {Score} \\\\ \\midrule\nOurs & 0.92 \\\\ \\cmidrule(lr){1-2}\nBase & 11.5 \\\\\n\\bottomrule\n\\end{tabular}"
        );
    }

    #[test]
    fn x_columns_switch_to_tabularx_and_multirow_cells_round_trip() {
        let mut t = parse("\\begin{tabular}{lX}\n\\multirow{2}{*}{a} & b \\\\\n & c \\\\\n\\end{tabular}").unwrap();
        assert_eq!(t.rows[0].cells[0].text, "\\multirow{2}{*}{a}");
        assert!(to_tabularx(&mut t) && !to_tabularx(&mut t));
        assert_eq!(
            emit(&t),
            "\\begin{tabularx}{\\linewidth}{lX}\n\\multirow{2}{*}{a} & b \\\\\n & c \\\\\n\\end{tabularx}"
        );
    }

    #[test]
    fn specs_expand_and_keep_their_parts() {
        let (cols, seps) = parse_spec("*{2}{c|}>{\\bfseries}p{3cm}<{x}S[table-format=1.2]");
        assert_eq!(cols.len(), 4);
        assert_eq!(spec_text(&cols, &seps), "c|c|>{\\bfseries}p{3cm}<{x}S[table-format=1.2]");
    }
}
