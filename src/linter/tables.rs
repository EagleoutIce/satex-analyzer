//! Tables: how their numbers line up and how they are ruled, read from the
//! source of each `tabular` the run entered, cell by cell as the column
//! specification and `&`, `\\` divide it.

use crate::engine::builtins::OccKind;
use crate::engine::tex::{Catcode, CatcodeTable, Span, Tok};
use crate::linter::fix::{Pos, Scan, Text};
use crate::linter::{Category, Ref, Report, Rule, Severity, Source};
use crate::plugins::augment::{Draft, Subject};

pub static RULES: &[Rule] = &[
    Rule {
        code: "numbers-not-aligned",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Info,
        summary: "numbers not aligned",
        explanation: "\
A column of numbers is read by comparing them, which works when each digit
stands under the digit of the same place: the units under the units, the
decimal marks under each other.  An `l` or `c` column sets numbers of
different lengths ragged, and an `r` column lines up only numbers with as many
decimals.  [siunitx](https://ctan.org/pkg/siunitx)'s `S` column aligns them at
the decimal mark (§ 7.12 of its manual). Its header, not a number, goes in
braces: `{Time}`.

The rule reads the table's source: a column whose cells below the header are
all numbers, three or more, set in a column that does not align them.",
        references: &[Ref { source: Source::Siunitx, at: "7.12" }],
        fixes: &[],
        engines: &[],
        run: numbers_not_aligned,
    },
    Rule {
        code: "inconsistent-precision",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Info,
        summary: "different decimals in a column",
        explanation: "\
`1.5`, `2.25` and `3` in one column claim three different precisions, and do
not line up even at their decimal mark.  Give them one precision.  An `S`
column of [siunitx](https://ctan.org/pkg/siunitx) rounds them with
`round-mode = places, round-precision = …`.",
        references: &[Ref { source: Source::Siunitx, at: "4.6" }],
        fixes: &[],
        engines: &[],
        run: inconsistent_precision,
    },
    Rule {
        code: "unit-in-cells",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Info,
        summary: "unit repeated in every cell",
        explanation: "\
A unit repeated in every cell of a column says the same thing on each line
and pushes the numbers out of alignment.  The
[SI brochure](https://www.bipm.org/en/publications/si-brochure) (9th ed.,
§ 5.4.1) heads such a column with the quantity and its unit, as
`Time / ms`. With [siunitx](https://ctan.org/pkg/siunitx), `{Time / \\unit{ms}}`
over an `S` column of bare numbers.",
        references: &[Ref { source: Source::SiBrochure, at: "5.4.1" }, Ref { source: Source::Siunitx, at: "\\unit" }],
        fixes: &[],
        engines: &[],
        run: unit_in_cells,
    },
    Rule {
        code: "table-rules",
        since: "0.2.0",
        category: Category::Style,
        severity: Severity::Info,
        summary: "vertical rules or hline in a table",
        explanation: "\
[booktabs](https://ctan.org/pkg/booktabs) gives a table three rules of
different weights, `\\toprule`, `\\midrule` and `\\bottomrule`, with space
around them, and asks for no vertical rules at all (§ 2 of its manual): the
columns are separated by their space, and `\\hline` sets every rule alike and
too close to the text.  A table of fewer rows than `lints.table-rules.rows`
(3) is left alone.",
        references: &[Ref { source: Source::Booktabs, at: "2" }],
        fixes: &[],
        engines: &[],
        run: table_rules,
    },
];

/// One table as its source has it.
struct Table {
    begin: Span,
    /// The column types as written, a package's with its `[…]` options.
    columns: Vec<String>,
    vertical: bool,
    hline: bool,
    /// The rows that are cells only, each cell's text and where it starts.
    rows: Vec<Vec<(String, Pos)>>,
    /// Every row with text, spanning cells or not.
    lines: usize,
}

/// A number as a table cell spells it.
#[derive(Debug, PartialEq)]
struct Number {
    digits: usize,
    decimals: Option<usize>,
    unit: Option<String>,
}

fn tables(report: &Report) -> Vec<Table> {
    let analysis = report.analysis();
    let mut out = Vec::new();
    for occurrence in &analysis.facts.occurrences {
        if occurrence.kind != OccKind::BeginEnvironment || !occurrence.certain {
            continue;
        }
        let Some(skip) = analysis.settings.conventions.tables.environments.get(&occurrence.key) else { continue };
        let Some((_, text)) = report.edits().text(occurrence.span.file) else { continue };
        if let Some(table) = read(&text, occurrence.span, *skip) {
            out.push(table);
        }
    }
    out
}

/// The table whose `\begin` stands at `begin`, when its source reads as one.
fn read(text: &Text, begin: Span, skip: usize) -> Option<Table> {
    let mut scan = Scan::at(text, Pos::new(begin.line, begin.col), CatcodeTable::latex())?;
    let head = scan.advance()?;
    if scan.name(&head)? != "begin" {
        return None;
    }
    scan.group()?;
    for _ in 0..skip {
        scan.group()?;
    }
    scan.optional();
    let spec = scan.group()?;
    let (columns, vertical) = columns(text.slice(spec.inner_start, spec.inner_end)?, 0)?;
    let mut table = Table { begin, columns, vertical, hline: false, rows: Vec::new(), lines: 0 };
    let (mut row, mut cell, mut depth, mut spans) = (Vec::new(), None::<Pos>, 0usize, false);
    // Environments a cell holds, whose `\end` ends no table.
    let mut nested = 0usize;
    while let Some(lexeme) = scan.advance() {
        match lexeme.tok {
            Tok::Chr(_, Catcode::Begin) => depth += 1,
            Tok::Chr(_, Catcode::End) if depth == 0 => break,
            Tok::Chr(_, Catcode::End) => depth -= 1,
            Tok::Chr(_, Catcode::Tab) if depth == 0 => {
                row.push(cell_text(text, cell.take(), lexeme.start));
                continue;
            }
            Tok::Cs(_) if depth == 0 => match scan.name(&lexeme)? {
                "begin" => nested += 1,
                "end" if nested > 0 => nested -= 1,
                "end" => {
                    row.push(cell_text(text, cell.take(), lexeme.start));
                    push_row(&mut table, std::mem::take(&mut row), spans);
                    return Some(table);
                }
                "\\" | "tabularnewline" => {
                    row.push(cell_text(text, cell.take(), lexeme.start));
                    push_row(&mut table, std::mem::take(&mut row), std::mem::take(&mut spans));
                    scan.optional();
                    continue;
                }
                // Rules stand between rows, and are no cell's text.
                "hline" | "toprule" | "midrule" | "bottomrule" | "cline" | "cmidrule" | "addlinespace"
                    if cell.is_none() =>
                {
                    table.hline |= scan.name(&lexeme)? == "hline";
                    if scan.peek().is_some_and(|l| l.tok == Tok::Chr('(', Catcode::Other)) {
                        while scan.advance().is_some_and(|l| l.tok != Tok::Chr(')', Catcode::Other)) {}
                    }
                    scan.optional();
                    if matches!(scan.name(&lexeme)?, "cline" | "cmidrule") {
                        scan.group();
                    }
                    continue;
                }
                "multicolumn" | "multirow" | "input" | "include" => spans = true,
                _ => {}
            },
            Tok::Chr(_, Catcode::Space) if cell.is_none() => continue,
            _ => {}
        }
        cell.get_or_insert(lexeme.start);
    }
    None
}

fn cell_text(text: &Text, start: Option<Pos>, end: Pos) -> (String, Pos) {
    let start = start.unwrap_or(end);
    (text.slice(start, end).unwrap_or_default().trim().to_string(), start)
}

/// A row whose cells line up with the columns: none spans several, and the
/// empty last line after a final `\\` is none.
fn push_row(table: &mut Table, row: Vec<(String, Pos)>, spans: bool) {
    let empty = row.iter().all(|(text, _)| text.is_empty());
    table.lines += usize::from(!empty);
    if !spans && !empty && row.len() <= table.columns.len() {
        table.rows.push(row);
    }
}

/// The column types of a specification and whether it has a vertical rule.
fn columns(spec: &str, depth: usize) -> Option<(Vec<String>, bool)> {
    let mut chars = spec.chars().peekable();
    let (mut out, mut vertical) = (Vec::new(), false);
    // `{…}` after the type letter, balanced.
    let group = |chars: &mut std::iter::Peekable<std::str::Chars>| -> Option<String> {
        while chars.peek().is_some_and(|c| c.is_whitespace()) {
            chars.next();
        }
        if chars.next()? != '{' {
            return None;
        }
        let (mut text, mut depth) = (String::new(), 1);
        for c in chars.by_ref() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
            if depth == 0 {
                return Some(text);
            }
            text.push(c);
        }
        None
    };
    while let Some(c) = chars.next() {
        match c {
            c if c.is_whitespace() => {}
            '|' => vertical = true,
            '@' | '!' | '>' | '<' => {
                group(&mut chars)?;
            }
            '*' if depth < 4 => {
                let times: usize = group(&mut chars)?.trim().parse().ok()?;
                let (inner, rule) = columns(&group(&mut chars)?, depth + 1)?;
                vertical |= rule;
                for _ in 0..times.min(64) {
                    out.extend(inner.iter().cloned());
                }
            }
            // `l`, a package's type with its `[…]` options, or one `\newcolumntype` made.
            // `p`, `m`, `b` take a width and dcolumn's `D` three arguments.
            _ => {
                let args = if c == 'D' { 3 } else { usize::from(matches!(c, 'p' | 'm' | 'b')) };
                for _ in 0..args {
                    group(&mut chars)?;
                }
                let mut kind = if matches!(c, 'm' | 'b') { 'p' } else { c }.to_string();
                if chars.peek() == Some(&'[') {
                    kind.extend(chars.by_ref().take_while(|c| *c != ']'));
                    kind.push(']');
                }
                out.push(kind);
            }
        }
    }
    Some((out, vertical))
}

/// A cell that is a number: digits, a decimal mark and more digits, a sign,
/// a unit after it; `$…$` around it as math.
fn number(cell: &str, unit_length: usize) -> Option<Number> {
    let cell = cell.strip_prefix('$').and_then(|c| c.strip_suffix('$')).unwrap_or(cell).trim();
    let cell = cell.strip_prefix(['-', '+', '\u{2212}']).unwrap_or(cell);
    let digits = cell.chars().take_while(char::is_ascii_digit).count();
    if digits == 0 {
        return None;
    }
    let mut rest = &cell[digits..];
    let mut decimals = None;
    if let Some(after) = rest.strip_prefix(['.', ',']) {
        let n = after.chars().take_while(char::is_ascii_digit).count();
        if n > 0 {
            decimals = Some(n);
            rest = &after[n..];
        }
    }
    let rest = rest.trim_start_matches([' ', '~']).trim_start_matches("\\,").trim_start_matches("\\ ").trim();
    let unit = (!rest.is_empty()).then(|| rest.replace("\\%", "%"));
    let unit_like =
        |u: &str| u.chars().all(|c| c.is_alphabetic() || "%°/^\u{b5}".contains(c)) && u.chars().count() <= unit_length;
    match &unit {
        Some(u) if !unit_like(u) => None,
        _ => Some(Number { digits, decimals, unit }),
    }
}

/// Each column of numbers: its number, type and numbers with where the
/// first stands, and its header. A column is one of numbers when, below the
/// rows heading it, every cell that has text is a number, two or more: two
/// can already stand apart.
fn number_columns(
    table: &Table,
    conv: &crate::project::config::Tables,
) -> Vec<(usize, String, Vec<Number>, Pos, String)> {
    let number = |text: &str| number(text, conv.unit_length);
    let mut out = Vec::new();
    for (j, kind) in table.columns.iter().enumerate() {
        let cells: Vec<&(String, Pos)> = table.rows.iter().filter_map(|row| row.get(j)).collect();
        let first = cells.iter().position(|(text, _)| number(text).is_some());
        let Some(first) = first else { continue };
        let numbers: Option<Vec<Number>> =
            cells[first..].iter().filter(|(text, _)| !text.is_empty()).map(|(text, _)| number(text)).collect();
        if let Some(numbers) = numbers.filter(|n| n.len() >= conv.numbers) {
            let header = cells[..first].iter().rev().find(|(text, _)| !text.is_empty());
            let header = header.map_or("Header", |(text, _)| text.as_str()).to_string();
            out.push((j + 1, kind.clone(), numbers, cells[first].1, header));
        }
    }
    out
}

fn span_at(table: &Table, at: Pos) -> Span {
    Span::new(table.begin.file, at.line, at.col)
}

fn all_equal<T: PartialEq>(mut values: impl Iterator<Item = T>) -> bool {
    let Some(first) = values.next() else { return true };
    values.all(|v| v == first)
}

fn numbers_not_aligned(report: &mut Report) {
    for table in tables(report) {
        for (column, kind, numbers, at, header) in
            number_columns(&table, &report.analysis().settings.conventions.tables)
        {
            if !matches!(kind.as_str(), "l" | "c" | "r" | "p" | "X") {
                continue;
            }
            let same_decimals = all_equal(numbers.iter().map(|n| n.decimals.unwrap_or(0)));
            let same_digits = all_equal(numbers.iter().map(|n| n.digits));
            if same_decimals && (kind == "r" || same_digits) {
                continue;
            }
            let decimals = numbers.iter().any(|n| n.decimals.is_some());
            let (message, fix) = match kind == "r" {
                true => (format!("numeric column {column} is right-aligned, not on the decimal separator"), None),
                false => (
                    format!("numeric column {column} is set `{kind}`: right-align it with `r`"),
                    Some(format!("set column {column} as `r` in the column specification")),
                ),
            };
            let subject = Subject::Alignment { kind: &kind, column, header: &header, decimals };
            report.add_reviewed(span_at(&table, at), "tabular", Draft { subject, message, fix, keep: true });
        }
    }
}

fn inconsistent_precision(report: &mut Report) {
    for table in tables(report) {
        for (column, kind, numbers, at, _) in number_columns(&table, &report.analysis().settings.conventions.tables) {
            let places = numbers.iter().map(|n| n.decimals.unwrap_or(0));
            let (low, high) = (places.clone().min().unwrap_or(0), places.max().unwrap_or(0));
            if low == high {
                continue;
            }
            let message = format!(
                "numeric column {column} mixes {low} to {high} decimal places: give every number one precision"
            );
            // A type the rule does not know may round: only a package that knows it keeps the finding.
            let keep = matches!(kind.as_str(), "l" | "c" | "r" | "p" | "X" | "D");
            let subject = Subject::Precision { kind: &kind, column, low, high };
            let fix = Some("round them by hand".to_string());
            report.add_reviewed(span_at(&table, at), "tabular", Draft { subject, message, fix, keep });
        }
    }
}

fn unit_in_cells(report: &mut Report) {
    for table in tables(report) {
        for (column, _, numbers, at, header) in number_columns(&table, &report.analysis().settings.conventions.tables) {
            let Some(unit) = numbers[0].unit.clone() else { continue };
            if numbers.iter().all(|n| n.unit.as_deref() == Some(unit.as_str())) {
                report.add(
                    span_at(&table, at),
                    "tabular",
                    format!("every number in column {column} repeats the unit `{unit}`: name it once in the header"),
                    Some(format!("head the column `{header} / {unit}` and leave bare numbers in the cells")),
                );
            }
        }
    }
}

fn table_rules(report: &mut Report) {
    let rows = report.analysis().settings.lints.table_rules.rows;
    for table in tables(report).into_iter().filter(|t| t.lines >= rows) {
        let how = match (table.vertical, table.hline) {
            (true, true) => "vertical rules and `\\hline`",
            (true, false) => "vertical rules",
            (false, true) => "`\\hline`",
            (false, false) => continue,
        };
        let drop = if table.vertical { "drop the `|` and " } else { "" };
        report.add(
            table.begin,
            "tabular",
            format!("{how} instead of booktabs rules: {drop}use `\\toprule`, `\\midrule`, `\\bottomrule`"),
            Some("add `\\usepackage{booktabs}`, put `\\toprule` above the header, `\\midrule` below it and `\\bottomrule` at the end".to_string()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_is_a_number_with_its_decimals_and_unit() {
        let n = |d, decimals, unit: Option<&str>| Some(Number { digits: d, decimals, unit: unit.map(str::to_string) });
        let number = |cell| number(cell, crate::project::config::Tables::default().unit_length);
        assert_eq!(number("12.5"), n(2, Some(1), None));
        assert_eq!(number("$-3,25$"), n(1, Some(2), None));
        assert_eq!(number("4 ms"), n(1, None, Some("ms")));
        assert_eq!(number("7\\,\\%"), n(1, None, Some("%")));
        assert_eq!(number("80%"), n(2, None, Some("%")));
        assert_eq!(number("Mean"), None);
        assert_eq!(number("3 apples and more"), None);
    }

    #[test]
    fn a_specification_gives_its_columns() {
        let kinds = |spec| columns(spec, 0).map(|(kinds, rule)| (kinds.join(" "), rule));
        assert_eq!(kinds("|l|c|r|"), Some(("l c r".into(), true)));
        assert_eq!(kinds("@{}lp{3cm}S@{}"), Some(("l p S".into(), false)));
        assert_eq!(
            kinds("S[round-mode=places]S[table-format=1.2]r"),
            Some(("S[round-mode=places] S[table-format=1.2] r".into(), false))
        );
        assert_eq!(kinds("*{3}{c}>{\\bfseries}l"), Some(("c c c l".into(), false)));
        assert_eq!(kinds("lY"), Some(("l Y".into(), false)));
    }
}
