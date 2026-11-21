//! Conversions between array tables (`tabular`, `tabularx`, `longtable`) and tabularray's (`tblr`,
//! `longtblr`) on the grid model of [`super::tabular`]. A conversion keeps the cells, the column
//! alignments, the spans and the rules; where the other form cannot say the same it is refused with
//! the reason. Rules stay in the body where tabularray takes them as written (`\hline`, `\cline`,
//! booktabs' through its library); rules on every row and column become `hlines`, `vlines`.

use super::tabular::{Cell, Col, Inner, Row, Table, group_end, key_value, top_items};

/// booktabs' rules: tabularray takes them with `\UseTblrLibrary{booktabs}`.
const BOOKTABS: [&str; 7] =
    ["\\toprule", "\\midrule", "\\bottomrule", "\\cmidrule", "\\addlinespace", "\\specialrule", "\\morecmidrules"];
/// The row commands of longtable that have no tabularray form but a row head.
const LONG_ROWS: [&str; 3] = ["\\endfirsthead", "\\endfoot", "\\endlastfoot"];

/// A paragraph column's alignment command (`>{\raggedleft\arraybackslash}`) and the letter tabularray says it with.
const RAGGED: [(&str, &str); 3] = [("\\raggedright", "l"), ("\\centering", "c"), ("\\raggedleft", "r")];

/// The braced groups `{…}` at the start of `s` (spaces between allowed), up to `n`, and the bytes they take.
fn groups(s: &str, n: usize) -> Option<(Vec<String>, usize)> {
    let (mut out, mut i) = (Vec::new(), 0);
    while out.len() < n {
        i += s[i..].len() - s[i..].trim_start().len();
        let e = group_end(s, i).filter(|_| s[i..].starts_with('{'))?;
        out.push(s[i + 1..e - 1].to_string());
        i = e;
    }
    Some((out, i))
}

/// `text` starting with `\name` and `n` braced arguments: the arguments and the text after them.
fn call<'a>(text: &'a str, name: &str, n: usize) -> Option<(Vec<String>, &'a str)> {
    let rest = text.trim_start().strip_prefix(name)?;
    if rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    let (args, k) = groups(rest, n)?;
    Some((args, &rest[k..]))
}

/// A conversion between the two forms.
pub type Conversion = fn(&Table) -> Result<Table, String>;

/// The environment the other form writes a table of `env` as: tabularray's for array's and back.
pub fn other_form(env: &str) -> Option<&'static str> {
    match env {
        "tabular" | "tabularx" => Some("tblr"),
        "longtable" => Some("longtblr"),
        "tblr" => Some("tabular"),
        "longtblr" => Some("longtable"),
        _ => None,
    }
}

/// The libraries the tabularray table's body needs (`booktabs` for `\toprule`).
pub fn libraries(t: &Table) -> Vec<&'static str> {
    let rules = t.rows.iter().flat_map(|r| &r.before).chain(&t.after);
    let booktabs = rules.into_iter().any(|r| BOOKTABS.iter().any(|b| rule_is(r, b)));
    if booktabs { vec!["booktabs"] } else { Vec::new() }
}

fn rule_is(item: &str, name: &str) -> bool {
    item.strip_prefix(name).is_some_and(|r| !r.starts_with(|c: char| c.is_ascii_alphabetic()))
}

/// Whether every row has `rule` alone before it and after the last: a rule on every row line.
fn every(t: &Table, rule: &str) -> bool {
    let only = |items: &[String]| items.len() == 1 && items[0] == rule;
    !t.rows.is_empty() && t.rows.iter().all(|r| only(&r.before)) && only(&t.after)
}

/// The table as tabularray writes it: `tblr` or `longtblr`, its columns as `colspec`.
pub fn to_tblr(t: &Table) -> Result<Table, String> {
    if t.inner.is_some() {
        return Err("this is a tabularray table already".into());
    }
    let env = match other_form(&t.env) {
        Some(e @ ("tblr" | "longtblr")) => e,
        _ => return Err(format!("{} has no tabularray form here", t.env)),
    };
    let mut out = Table { env: env.into(), width: None, pos: None, ..t.clone() };
    let mut keys: Vec<String> = Vec::new();
    if t.env == "tabularx" {
        keys.push(format!("width={}", t.width.clone().unwrap_or_default()));
    }
    match (env, t.pos.as_deref()) {
        (_, None) => {}
        ("tblr", Some(p @ ("t" | "b" | "c"))) => out.pos = Some(p.into()),
        (_, Some(p)) => return Err(format!("the position `[{p}]` has no tabularray form here")),
    }
    out.cols = t.cols.iter().map(column).collect::<Result<_, _>>()?;
    for s in &t.seps {
        if !matches!(s.as_str(), "" | "|" | "||" | "@{}") {
            return Err(format!("`{s}` between columns has no tabularray form"));
        }
    }
    if t.seps.iter().all(|s| s == "|") {
        out.seps = vec![String::new(); t.seps.len()];
        keys.push("vlines".into());
    }
    // longtable's head rows and caption
    let mut head = None;
    let mut outer: Vec<String> = Vec::new();
    let mut rows: Vec<Row> = Vec::new();
    for (k, r) in t.rows.iter().enumerate() {
        let mut r = r.clone();
        let single = r.cells.len() == 1;
        if env == "longtblr"
            && let Some(first) = r.cells.first_mut()
        {
            if let Some(no) = LONG_ROWS.iter().find(|n| first.text.contains(*n)) {
                return Err(format!("{no} has no tabularray form: a row head is `rowhead`"));
            }
            if let Some(rest) = first.text.strip_prefix("\\endhead") {
                if head.is_some() || !r.before.is_empty() {
                    return Err("rules or a second \\endhead around a row head".into());
                }
                head = Some(rows.len());
                first.text = rest.trim_start().to_string();
                r.raw.clear();
            }
            if k == 0
                && single
                && let Some((cap, rest)) = call(&first.text, "\\caption", 1)
            {
                if !t.rows[0].before.is_empty() {
                    return Err("a rule above the caption".into());
                }
                outer.push(format!("caption={{{}}}", cap[0]));
                match call(rest, "\\label", 1) {
                    Some((l, rest)) if rest.trim().is_empty() => outer.push(format!("label={{{}}}", l[0])),
                    None if rest.trim().is_empty() => {}
                    _ => return Err("text beside the caption".into()),
                }
                continue;
            }
        }
        if r.cells.len() == 1
            && r.cells[0].text.trim().is_empty()
            && head.is_some()
            && head == Some(rows.len())
            && r.end.is_empty()
        {
            // `\endhead` alone on the last line: no row
            continue;
        }
        r.cells = cells(&r.cells, t)?;
        r.end = row_end(&r.end);
        r.before = r.before.iter().map(|b| rule(b)).collect();
        if r.cells != t.rows[k].cells {
            r.raw.clear();
        }
        rows.push(r);
    }
    if let Some(h) = head {
        keys.push(format!("rowhead={h}"));
    }
    out.rows = rows;
    out.after = t.after.iter().map(|b| rule(b)).collect();
    if every(&out, "\\hline") {
        for r in &mut out.rows {
            (r.before, r.inline) = (Vec::new(), false);
        }
        out.after.clear();
        keys.push("hlines".into());
    }
    if !outer.is_empty() {
        out.pos = Some(outer.join(", "));
    }
    let after = keys.iter().map(|k| format!(", {k}")).collect::<String>();
    out.inner = Some(Inner { before: "colspec={".into(), after: format!("}}{after}"), implied: None });
    out.style.inline_rules &= !out.rows.iter().all(|r| r.before.is_empty());
    Ok(out)
}

/// booktabs' `\cmidrule(lr){…}` as tabularray's `\cmidrule[lr]{…}`; other rules as written.
fn rule(item: &str) -> String {
    match item.strip_prefix("\\cmidrule(").and_then(|r| r.split_once(')')) {
        Some((trim, rest)) => format!("\\cmidrule[{trim}]{rest}"),
        None => item.to_string(),
    }
}

/// `\\*` keeps a page from breaking in longtable; tabularray's `\\` with `[skip]` alike.
fn row_end(end: &str) -> String {
    end.replacen("\\\\*", "\\\\", 1)
}

/// An array column as tabularray's: a paragraph's alignment command as `Q[r, wd=…]`, `X[c]`.
fn column(c: &Col) -> Result<Col, String> {
    let mut pre = c.pre.clone();
    let mut halign = None;
    for (cmd, letter) in RAGGED {
        for form in [format!(">{{{cmd}\\arraybackslash}}"), format!(">{{{cmd}}}")] {
            if pre.contains(&form) {
                pre = pre.replacen(&form, "", 1);
                halign = Some(letter);
            }
        }
    }
    if pre.contains("\\arraybackslash") {
        return Err("a column's `>{…}` with \\arraybackslash".into());
    }
    let (align, arg) = match (c.align.as_str(), halign) {
        ("l" | "c" | "r", None) => (c.align.clone(), String::new()),
        ("p" | "m" | "b", None) => (c.align.clone(), c.arg.clone()),
        ("p" | "m" | "b", Some(h)) => {
            let v = match c.align.as_str() {
                "p" => "t",
                v => v,
            };
            ("Q".into(), format!("{h},{v},wd={}", c.arg))
        }
        ("X", h) => ("X".into(), h.unwrap_or_default().to_string()),
        ("S", _) => {
            return Err("siunitx S columns: tabularray needs \\UseTblrLibrary{siunitx} and braced headers".into());
        }
        (a, _) => return Err(format!("the column `{a}` (a \\newcolumntype?) has no tabularray form")),
    };
    Ok(Col { align, arg, pre, post: c.post.clone(), package: None })
}

/// A row's cells as tabularray's: `\multicolumn{n}{c}{x}` as `\SetCell[c=n]{c} x` and the cells it covers,
/// `\multirow{n}{*}{x}` as `\SetCell[r=n]{…} x`, `\cellcolor{red}` as `bg=red`.
fn cells(cells: &[Cell], t: &Table) -> Result<Vec<Cell>, String> {
    let mut out = Vec::new();
    let mut col = 0;
    for c in cells {
        let mut text = c.text.clone();
        let mut spec: Vec<String> = Vec::new();
        if c.span > 1 || !c.spec.is_empty() {
            let letter = c.spec.replace(['|', ' '], "").replace("@{}", "");
            if !matches!(letter.as_str(), "l" | "c" | "r") {
                return Err(format!("a \\multicolumn of `{}`: only l, c, r spans have a tabularray form", c.spec));
            }
            let ruled = |k: usize| t.seps.get(k).is_some_and(|s| s.contains('|'));
            let (left, right) = (c.spec.trim().starts_with('|'), c.spec.trim().ends_with('|'));
            // a span's rules are its own in array: the table's must agree (its first column's left one only there)
            if (col == 0 && left != ruled(0)) || right != ruled(col + c.span) || (col > 0 && left) {
                return Err("a \\multicolumn with vertical rules of its own".into());
            }
            spec.push(letter);
        }
        if text.contains("\\rowcolor") {
            return Err("\\rowcolor: tabularray colours a row by `row{…}={bg=…}`".into());
        }
        if let Some((args, rest)) = call(&text, "\\cellcolor", 1) {
            spec.push(format!("bg={}", args[0]));
            text = rest.trim_start().to_string();
        }
        let mut down = None;
        if let Some((args, _)) = call(&text, "\\multirow", 3).filter(|(_, r)| r.trim().is_empty()) {
            let n: usize =
                args[0].trim().parse().map_err(|_| format!("\\multirow{{{}}}: rows counted down", args[0]))?;
            down = Some(n);
            if spec.is_empty() {
                spec.push(
                    t.cols
                        .get(col)
                        .map(|c| c.align.clone())
                        .filter(|a| matches!(a.as_str(), "l" | "c" | "r"))
                        .unwrap_or_else(|| "l".into()),
                );
            }
            text = args[2].clone();
        }
        if text.contains("\\multicolumn") || text.contains("\\multirow") || text.contains("\\cellcolor") {
            return Err("a \\multicolumn, \\multirow or \\cellcolor inside a cell's text".into());
        }
        match down {
            // a span down is a `\SetCell` written out: the grid models spans across only
            Some(n) => {
                let across = if c.span > 1 { format!(",c={}", c.span) } else { String::new() };
                out.push(Cell {
                    text: format!("\\SetCell[r={n}{across}]{{{}}} {text}", spec.join(", ")),
                    span: 1,
                    spec: String::new(),
                });
                out.extend((1..c.span).map(|_| Cell { text: String::new(), span: 1, spec: String::new() }));
            }
            None => out.push(Cell { text, span: c.span.max(1), spec: spec.join(", ") }),
        }
        col += c.span.max(1);
    }
    Ok(out)
}

/// The tabularray table as an array one: `tabular` (`tabularx` with `X` columns), `longtable`.
pub fn to_tabular(t: &Table) -> Result<Table, String> {
    let Some(inner) = &t.inner else {
        // a tblr with a column spec of its own: no key list
        return to_tabular(&Table {
            inner: Some(Inner { before: String::new(), after: String::new(), implied: None }),
            ..t.clone()
        });
    };
    let env = match other_form(&t.env) {
        Some(e @ ("tabular" | "longtable")) => e,
        _ => return Err(format!("{} has no array form here", t.env)),
    };
    let mut out = Table { env: env.into(), inner: None, width: None, pos: None, ..t.clone() };
    let (mut hlines, mut vlines, mut head) = (false, false, None);
    let rest = format!("{}{}", inner.before.trim_end_matches("colspec={"), inner.after.trim_start_matches('}'));
    let rest = if inner.implied.is_some() { inner.implied.clone().unwrap_or_default() } else { rest };
    for (a, b) in top_items(&rest) {
        let item = rest[a..b].trim();
        match key_value(item) {
            ("hlines", None) => hlines = true,
            ("vlines", None) => vlines = true,
            ("width", Some((w, _))) => out.width = Some(w.to_string()),
            ("rowhead", Some((n, _))) if env == "longtable" => head = n.trim().parse::<usize>().ok().filter(|&n| n > 0),
            _ => return Err(format!("`{item}` has no tabular form")),
        }
    }
    match (env, t.pos.as_deref().map(str::trim)) {
        (_, None) => {}
        ("tabular", Some(p @ ("t" | "b" | "c"))) => out.pos = Some(p.into()),
        ("longtable", Some(p)) => {
            let mut caption = Vec::new();
            for (a, b) in top_items(p) {
                match key_value(&p[a..b]) {
                    ("caption", Some((c, _))) => caption.insert(0, format!("\\caption{{{c}}}")),
                    ("label", Some((l, _))) => caption.push(format!("\\label{{{l}}}")),
                    _ => return Err(format!("`{}` has no longtable form", p[a..b].trim())),
                }
            }
            if !caption.is_empty() {
                let cell = Cell { text: caption.join(""), span: 1, spec: String::new() };
                out.rows.insert(0, Row { cells: vec![cell], end: "\\\\".into(), ..Default::default() });
            }
        }
        (_, Some(p)) => return Err(format!("the outer spec `[{p}]` has no tabular form")),
    }
    let shift = out.rows.len() - t.rows.len();
    out.cols = t.cols.iter().map(array_column).collect::<Result<_, _>>()?;
    if out.cols.iter().any(|c| c.align == "X") {
        out.env = match env {
            "tabular" => "tabularx".into(),
            _ => return Err("X columns in a long table".into()),
        };
        out.width.get_or_insert_with(|| "\\linewidth".into());
    } else if out.width.is_some() {
        return Err("a width without X columns: tabular has none".into());
    }
    if vlines {
        out.seps = vec!["|".into(); out.cols.len() + 1];
    }
    for (k, r) in t.rows.iter().enumerate() {
        let row = &mut out.rows[k + shift];
        row.cells = array_cells(&r.cells, &out.cols, &out.seps)?;
        if row.cells != r.cells {
            row.raw.clear();
        }
        for b in &mut row.before {
            *b = array_rule(b)?;
        }
        if hlines {
            row.before = vec!["\\hline".into()];
            row.inline = false;
        }
    }
    for b in &mut out.after {
        *b = array_rule(b)?;
    }
    if hlines {
        out.after = vec!["\\hline".into()];
        if let Some(r) = out.rows.get_mut(shift) {
            r.before = vec!["\\hline".into()];
        }
    }
    if let Some(h) = head {
        let r = out.rows.get_mut(shift + h).ok_or("rowhead past the last row")?;
        let first = r.cells.first_mut().ok_or("an empty row")?;
        first.text = format!("\\endhead\n{}", first.text);
        r.raw.clear();
    }
    Ok(out)
}

/// A tabularray rule as array's: options (`\hline[2pt]`) have no form there.
fn array_rule(item: &str) -> Result<String, String> {
    if item.starts_with("\\hline[") || item.starts_with("\\cline[") {
        return Err(format!("`{item}`: a rule with options"));
    }
    Ok(match item.strip_prefix("\\cmidrule[").and_then(|r| r.split_once(']')) {
        Some((trim, rest)) => format!("\\cmidrule({trim}){rest}"),
        None => item.to_string(),
    })
}

/// A tabularray column as array's: `Q[r, wd=3cm]` as `>{\raggedleft\arraybackslash}p{3cm}`.
fn array_column(c: &Col) -> Result<Col, String> {
    let opts: Vec<String> = c.arg.split(',').map(|o| o.trim().to_string()).filter(|o| !o.is_empty()).collect();
    let (mut h, mut v, mut wd) = (None, None, None);
    for o in &opts {
        match key_value(o) {
            (x @ ("l" | "c" | "r" | "j"), None) => h = Some(x.to_string()),
            (x @ ("t" | "m" | "b" | "p"), None) => v = Some(x.to_string()),
            ("wd", Some((w, _))) => wd = Some(w.to_string()),
            _ => return Err(format!("the column option `{o}` has no array form")),
        }
    }
    let ragged = |h: &Option<String>| {
        RAGGED.iter().find(|(_, l)| Some(*l) == h.as_deref()).map(|(cmd, _)| format!(">{{{cmd}\\arraybackslash}}"))
    };
    let (align, arg, pre) = match c.align.as_str() {
        a @ ("l" | "c" | "r") if opts.is_empty() => (a.to_string(), String::new(), String::new()),
        a @ ("p" | "m" | "b") => (a.to_string(), c.arg.clone(), String::new()),
        "X" if wd.is_none() && v.is_none() => ("X".into(), String::new(), ragged(&h).unwrap_or_default()),
        "Q" => match wd {
            None if v.is_none() => (h.unwrap_or_else(|| "l".into()).replace('j', "l"), String::new(), String::new()),
            Some(w) => {
                let letter = match v.as_deref() {
                    Some("m") => "m",
                    Some("b") => "b",
                    _ => "p",
                };
                (letter.into(), w, ragged(&h).unwrap_or_default())
            }
            None => return Err("a Q column aligned vertically without a width".into()),
        },
        a => return Err(format!("the column `{a}[{}]` has no array form", c.arg)),
    };
    Ok(Col { align, arg, pre: format!("{pre}{}", c.pre), post: c.post.clone(), package: None })
}

/// A row's cells as array's: `\SetCell[c=n]{c} x` and the empty cells it covers as `\multicolumn`.
fn array_cells(cells: &[Cell], cols: &[Col], seps: &[String]) -> Result<Vec<Cell>, String> {
    let mut out: Vec<Cell> = Vec::new();
    let mut skip = 0;
    for c in cells {
        if skip > 0 {
            if !c.text.trim().is_empty() {
                return Err("text in a cell a span covers".into());
            }
            skip -= 1;
            continue;
        }
        let modelled = c.span > 1 || !c.spec.is_empty();
        let rest = c.text.trim_start().strip_prefix("\\SetCell").filter(|_| !modelled);
        if rest.is_none()
            && (c.text.contains("\\SetCell") || c.text.contains("\\SetRow") || c.text.contains("\\SetHline"))
        {
            return Err("a \\Set… command inside a cell's text".into());
        }
        if rest.is_none() && !modelled {
            out.push(c.clone());
            continue;
        }
        // a span the grid models, or a `\SetCell` written out (one with `r=`)
        let (mut opts, mut i, rest) = (format!("c={}", c.span), 0, rest.unwrap_or_default());
        let (spec, text) = match modelled {
            true => (vec![c.spec.clone()], c.text.clone()),
            false => {
                if rest.starts_with('[') {
                    let e = group_end(rest, 0).ok_or("an open [")?;
                    opts = rest[1..e - 1].to_string();
                    i = e;
                }
                let (spec, j) = groups(&rest[i..], 1).ok_or("\\SetCell without its {…}")?;
                (spec, rest[i + j..].trim().to_string())
            }
        };
        let (mut span, mut down) = (1, None);
        for o in opts.split(',').filter(|o| !o.trim().is_empty()) {
            match key_value(o) {
                ("c", Some((n, _))) => span = n.trim().parse().map_err(|_| format!("c={n}"))?,
                ("r", Some((n, _))) => down = Some(n.trim().to_string()),
                _ => return Err(format!("\\SetCell option `{o}` has no array form")),
            }
        }
        let (mut letter, mut bg) = (None, None);
        for o in spec[0].split(',').filter(|o| !o.trim().is_empty()) {
            match key_value(o) {
                (x @ ("l" | "c" | "r"), None) => letter = Some(x.to_string()),
                ("bg", Some((c, _))) => bg = Some(c.to_string()),
                _ => return Err(format!("\\SetCell `{}` has no array form", o.trim())),
            }
        }
        let mut text = text;
        if let Some(n) = down {
            text = format!("\\multirow{{{n}}}{{*}}{{{text}}}");
        }
        if let Some(c) = bg {
            text = format!("\\cellcolor{{{c}}}{text}");
        }
        let col = out.iter().map(|c| c.span.max(1)).sum::<usize>();
        let own = cols.get(col).map(|c| c.align.clone());
        // a span's spec replaces the rules around it: the table's, its left one only in the first column
        let rules = |k: usize| seps.get(k).map_or(String::new(), |s| s.chars().filter(|&c| c == '|').collect());
        let spec = |l: String| format!("{}{l}{}", if col == 0 { rules(0) } else { String::new() }, rules(col + span));
        match (span > 1, letter) {
            (true, l) => out.push(Cell { text, span, spec: spec(l.or(own).unwrap_or_else(|| "l".into())) }),
            (false, Some(l)) if Some(&l) != own.as_ref() => out.push(Cell { text, span: 1, spec: spec(l) }),
            _ => out.push(Cell { text, span: 1, spec: String::new() }),
        }
        skip = if modelled { 0 } else { span - 1 };
    }
    if skip > 0 {
        return Err("a span past the row's last cell".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::tabular::{emit, parse, parse_with};
    use super::*;

    fn tblr(src: &str) -> String {
        emit(&to_tblr(&parse(src).unwrap()).unwrap())
    }

    fn back(src: &str) -> String {
        emit(&to_tabular(&parse_with(src, &|k| k == "hlines").unwrap()).unwrap())
    }

    /// Intuition: a plain tabular becomes a tblr with its columns as `colspec` and every rule where it
    /// stood; rules on every row line and column become `hlines` and `vlines`. Pins the conversion's
    /// text and that converting back gives the original table.
    #[test]
    fn a_tabular_becomes_a_tblr_and_back() {
        let ruled =
            "\\begin{tabular}{|l|r|}\n  \\hline\n  a & b \\\\\n  \\hline\n  c & d \\\\\n  \\hline\n\\end{tabular}";
        let t = tblr(ruled);
        assert_eq!(t, "\\begin{tblr}{colspec={lr}, vlines, hlines}\n  a & b \\\\\n  c & d \\\\\n\\end{tblr}");
        assert_eq!(back(&t), ruled);
        let plain = "\\begin{tabular}{lc}\n  Method & Score \\\\ \\hline\n  Ours & 0.92 \\\\\n\\end{tabular}";
        let t = tblr(plain);
        assert_eq!(t, "\\begin{tblr}{colspec={lc}}\n  Method & Score \\\\ \\hline\n  Ours & 0.92 \\\\\n\\end{tblr}");
        assert_eq!(back(&t), plain);
    }

    /// Intuition: spans, paragraph alignments and booktabs rules have tabularray forms: `\SetCell[c=2]{c}`
    /// with the covered cell left empty, `Q[r,t,wd=…]`, `\cmidrule[lr]` (tabularray's trim), and the
    /// library they need. Pins each mapping, and the reverse giving back the same table.
    #[test]
    fn spans_paragraphs_and_booktabs_convert() {
        let src = "\\begin{tabular}{l>{\\raggedleft\\arraybackslash}p{2cm}}\n\\toprule\n\\multicolumn{2}{c}{Both} \\\\\n\\cmidrule(lr){1-2}\n\\multirow{2}{*}{x} & 1 \\\\\n & 2 \\\\\n\\bottomrule\n\\end{tabular}";
        let t = to_tblr(&parse(src).unwrap()).unwrap();
        assert_eq!(libraries(&t), ["booktabs"]);
        let out = emit(&t);
        assert_eq!(
            out,
            "\\begin{tblr}{colspec={lQ[r,t,wd=2cm]}}\n\\toprule\n\\SetCell[c=2]{c} Both & \\\\\n\\cmidrule[lr]{1-2}\n\\SetCell[r=2]{l} x & 1 \\\\\n & 2 \\\\\n\\bottomrule\n\\end{tblr}"
        );
        assert_eq!(back(&out), src);
    }

    /// Intuition: in array a `\multicolumn`'s spec replaces the vertical rules around the cells it
    /// spans, so a span in a `vlines` tblr keeps them by writing them into its spec. Pins the lost rules
    /// of `\multicolumn{2}{c}{Both}` in a ruled table (seen in the typeset comparison).
    #[test]
    fn a_span_keeps_the_rules_around_it() {
        let t = "\\begin{tblr}{colspec={lrc}, vlines}\n\\SetCell[c=2]{c} Both & & x \\\\\na & 1 & \\SetCell{l} y \\\\\n\\end{tblr}";
        let out = back(t);
        assert!(out.contains("\\multicolumn{2}{|c|}{Both} & x"), "{out}");
        assert!(out.contains("a & 1 & \\multicolumn{1}{l|}{y}"), "{out}");
        assert_eq!(tblr(&out), t);
    }

    /// Intuition: what one form cannot say is refused with the reason, never written half right: an S
    /// column, a `\newcolumntype` letter, text in `@{…}`, a tblr key with no tabular form.
    #[test]
    fn what_has_no_other_form_is_refused() {
        let no = |src: &str| to_tblr(&parse(src).unwrap()).unwrap_err();
        assert!(no("\\begin{tabular}{lS}\na & 1\\\\\n\\end{tabular}").contains("S columns"));
        assert!(no("\\begin{tabular}{lY}\na & 1\\\\\n\\end{tabular}").contains("`Y`"));
        assert!(no("\\begin{tabular}{l@{ -- }l}\na & 1\\\\\n\\end{tabular}").contains("@{ -- }"));
        let keyed = |k: &str| k == "hlines";
        let t = parse_with("\\begin{tblr}{colspec={ll}, row{1}={font=\\bfseries}}\na & b\\\\\n\\end{tblr}", &keyed)
            .unwrap();
        assert!(to_tabular(&t).unwrap_err().contains("row{1}"));
    }

    /// Intuition: a longtable's caption row and `\endhead` are longtblr's `caption`, `label` and `rowhead`;
    /// `tabularx` keeps its width as `width=`. Pins both directions of each.
    #[test]
    fn long_tables_and_widths_convert() {
        let src = "\\begin{longtable}{ll}\n\\caption{Long}\\label{tab:l} \\\\\nA & B \\\\\n\\endhead\nx & y \\\\\n\\end{longtable}";
        let out = tblr(src);
        assert_eq!(
            out,
            "\\begin{longtblr}[caption={Long}, label={tab:l}]{colspec={ll}, rowhead=1}\nA & B \\\\\nx & y \\\\\n\\end{longtblr}"
        );
        let x = "\\begin{tabularx}{\\linewidth}{lX}\na & b \\\\\n\\end{tabularx}";
        let out = tblr(x);
        assert_eq!(out, "\\begin{tblr}{colspec={lX}, width=\\linewidth}\na & b \\\\\n\\end{tblr}");
        assert_eq!(back(&out), x);
        // a key-list spec without colspec: the default Q columns, written back as they were
        let implied = "\\begin{tblr}{hlines}\na & b \\\\\n\\end{tblr}";
        let t = parse_with(implied, &|k| k == "hlines").unwrap();
        assert_eq!((t.cols.len(), emit(&t).as_str()), (2, implied));
    }
}
