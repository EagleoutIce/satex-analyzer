//! Tables: how their numbers line up and how they are ruled, read from the
//! source of each `tabular` the run entered, cell by cell as the column
//! specification and `&`, `\\` divide it.

use crate::builtins::OccKind;
use crate::lint::fix::{Pos, Scan, Text};
use crate::lint::{Category, Ref, Report, Rule, Severity, Source};
use crate::tex::{Catcode, CatcodeTable, Span, Tok};

pub static RULES: &[Rule] = &[
    Rule {
        code: "numbers-not-aligned",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a table column of numbers does not line them up at their decimal mark",
        explanation: "\
A column of numbers is read by comparing them, which works when each digit
stands under the digit of the same place: the units under the units, the
decimal marks under each other.  An `l` or `c` column sets numbers of
different lengths ragged, and an `r` column lines up only numbers with as many
decimals.  [siunitx](https://ctan.org/pkg/siunitx)'s `S` column aligns them at
the decimal mark (§ 7.12 of its manual); its header, not a number, goes in
braces: `{Time}`.

The rule reads the table's source: a column whose cells below the header are
all numbers, three or more, set in a column that does not align them.",
        references: &[Ref { source: Source::Siunitx, at: "7.12" }],
        fixes: &[],
        run: numbers_not_aligned,
    },
    Rule {
        code: "inconsistent-precision",
        category: Category::Style,
        severity: Severity::Info,
        summary: "the numbers of a table column give different numbers of decimals",
        explanation: "\
`1.5`, `2.25` and `3` in one column claim three different precisions, and do
not line up even at their decimal mark.  Give them one precision; an `S`
column of [siunitx](https://ctan.org/pkg/siunitx) rounds them with
`round-mode = places, round-precision = …`.",
        references: &[Ref { source: Source::Siunitx, at: "4.6" }],
        fixes: &[],
        run: inconsistent_precision,
    },
    Rule {
        code: "unit-in-cells",
        category: Category::Style,
        severity: Severity::Info,
        summary: "every number of a table column carries the same unit",
        explanation: "\
A unit repeated in every cell of a column says the same thing on each line
and pushes the numbers out of alignment.  The
[SI brochure](https://www.bipm.org/en/publications/si-brochure) (9th ed.,
§ 5.4.1) heads such a column with the quantity and its unit, as
`Time / ms`; with [siunitx](https://ctan.org/pkg/siunitx), `{Time / \\unit{ms}}`
over an `S` column of bare numbers.",
        references: &[Ref { source: Source::SiBrochure, at: "5.4.1" }, Ref { source: Source::Siunitx, at: "\\unit" }],
        fixes: &[],
        run: unit_in_cells,
    },
    Rule {
        code: "table-rules",
        category: Category::Style,
        severity: Severity::Info,
        summary: "a table is ruled with vertical rules or `\\hline`",
        explanation: "\
[booktabs](https://ctan.org/pkg/booktabs) gives a table three rules of
different weights, `\\toprule`, `\\midrule` and `\\bottomrule`, with space
around them, and asks for no vertical rules at all (§ 2 of its manual): the
columns are separated by their space, and `\\hline` sets every rule alike and
too close to the text.",
        references: &[Ref { source: Source::Booktabs, at: "2" }],
        fixes: &[],
        run: table_rules,
    },
];

/// The environments read as tables, and how many arguments come before the
/// column specification.
const TABLES: [(&str, usize); 5] = [("tabular", 0), ("tabular*", 1), ("tabularx", 1), ("longtable", 0), ("array", 0)];

/// One table as its source has it.
struct Table {
    begin: Span,
    /// The column types, `?` for one the rule does not know.
    columns: Vec<char>,
    vertical: bool,
    hline: bool,
    /// The rows that are cells only, each cell's text and where it starts.
    rows: Vec<Vec<(String, Pos)>>,
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
        let Some((_, skip)) = TABLES.iter().find(|(name, _)| *name == occurrence.key) else { continue };
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
    let mut table = Table { begin, columns, vertical, hline: false, rows: Vec::new() };
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
    if !spans && !empty && row.len() <= table.columns.len() {
        table.rows.push(row);
    }
}

/// The column types of a specification and whether it has a vertical rule.
fn columns(spec: &str, depth: usize) -> Option<(Vec<char>, bool)> {
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
            'l' | 'c' | 'r' | 'X' | 'S' => out.push(c),
            'p' | 'm' | 'b' => {
                group(&mut chars)?;
                out.push('p');
            }
            'D' => {
                for _ in 0..3 {
                    group(&mut chars)?;
                }
                out.push('D');
            }
            '*' if depth < 4 => {
                let times: usize = group(&mut chars)?.trim().parse().ok()?;
                let (inner, rule) = columns(&group(&mut chars)?, depth + 1)?;
                vertical |= rule;
                for _ in 0..times.min(64) {
                    out.extend(&inner);
                }
            }
            // A type `\newcolumntype` made, or one this rule does not know.
            _ => out.push('?'),
        }
    }
    Some((out, vertical))
}

/// A cell that is a number: digits, a decimal mark and more digits, a sign,
/// a unit after it; `$…$` around it as math.
fn number(cell: &str) -> Option<Number> {
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
        |u: &str| u.chars().all(|c| c.is_alphabetic() || "%°/^\u{b5}".contains(c)) && u.chars().count() <= 6;
    match &unit {
        Some(u) if !unit_like(u) => None,
        _ => Some(Number { digits, decimals, unit }),
    }
}

/// Each column of numbers: its number, type and numbers with where the
/// first stands. A column is one of numbers when, below the rows heading
/// it, every cell that has text is a number, two or more: two can
/// already stand apart.
fn number_columns(table: &Table) -> Vec<(usize, char, Vec<Number>, Pos)> {
    let mut out = Vec::new();
    for (j, &kind) in table.columns.iter().enumerate() {
        let cells: Vec<&(String, Pos)> = table.rows.iter().filter_map(|row| row.get(j)).collect();
        let first = cells.iter().position(|(text, _)| number(text).is_some());
        let Some(first) = first else { continue };
        let numbers: Option<Vec<Number>> =
            cells[first..].iter().filter(|(text, _)| !text.is_empty()).map(|(text, _)| number(text)).collect();
        if let Some(numbers) = numbers.filter(|n| n.len() >= 2) {
            out.push((j + 1, kind, numbers, cells[first].1));
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
        for (column, kind, numbers, at) in number_columns(&table) {
            if !matches!(kind, 'l' | 'c' | 'r' | 'p' | 'X') {
                continue;
            }
            let same_decimals = all_equal(numbers.iter().map(|n| n.decimals.unwrap_or(0)));
            let same_digits = all_equal(numbers.iter().map(|n| n.digits));
            if same_decimals && (kind == 'r' || same_digits) {
                continue;
            }
            let decimal = numbers.iter().any(|n| n.decimals.is_some());
            let (message, fix) = match decimal {
                true => (
                    format!("the numbers of column {column} do not line up at their decimal mark"),
                    "use siunitx's `S` column (`\\usepackage{siunitx}`), with the header in braces: `{Header}`",
                ),
                false => (
                    format!("the numbers of column {column} do not line up at their last digit"),
                    "right-align the column (`r`), or use siunitx's `S` column",
                ),
            };
            report.add(span_at(&table, at), "tabular", message, Some(fix.to_string()));
        }
    }
}

fn inconsistent_precision(report: &mut Report) {
    for table in tables(report) {
        for (column, _, numbers, at) in number_columns(&table) {
            if !numbers.iter().any(|n| n.decimals.is_some()) {
                continue;
            }
            let places: Vec<usize> = numbers.iter().map(|n| n.decimals.unwrap_or(0)).collect();
            let (low, high) = (places.iter().min().copied().unwrap_or(0), places.iter().max().copied().unwrap_or(0));
            if low == high {
                continue;
            }
            report.add(
                span_at(&table, at),
                "tabular",
                format!("the numbers of column {column} give {low} to {high} decimals"),
                Some("give them one precision; an `S` column rounds with `round-mode = places`".to_string()),
            );
        }
    }
}

fn unit_in_cells(report: &mut Report) {
    for table in tables(report) {
        for (column, _, numbers, at) in number_columns(&table) {
            let Some(unit) = numbers[0].unit.clone() else { continue };
            if numbers.iter().all(|n| n.unit.as_deref() == Some(unit.as_str())) {
                report.add(
                    span_at(&table, at),
                    "tabular",
                    format!("every number of column {column} carries the unit `{unit}`"),
                    Some(format!(
                        "name the unit once, in the header (`Quantity / {unit}`), and give the numbers alone"
                    )),
                );
            }
        }
    }
}

fn table_rules(report: &mut Report) {
    for table in tables(report) {
        let how = match (table.vertical, table.hline) {
            (true, true) => "vertical rules and `\\hline`",
            (true, false) => "vertical rules",
            (false, true) => "`\\hline`",
            (false, false) => continue,
        };
        report.add(
            table.begin,
            "tabular",
            format!("the table is ruled with {how}"),
            Some(
                "rule it with booktabs's `\\toprule`, `\\midrule` and `\\bottomrule`, and no vertical rules"
                    .to_string(),
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cell_is_a_number_with_its_decimals_and_unit() {
        let n = |d, decimals, unit: Option<&str>| Some(Number { digits: d, decimals, unit: unit.map(str::to_string) });
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
        assert_eq!(columns("|l|c|r|", 0), Some((vec!['l', 'c', 'r'], true)));
        assert_eq!(columns("@{}lp{3cm}S@{}", 0), Some((vec!['l', 'p', 'S'], false)));
        assert_eq!(columns("*{3}{c}>{\\bfseries}l", 0), Some((vec!['c', 'c', 'c', 'l'], false)));
        assert_eq!(columns("lY", 0), Some((vec!['l', '?'], false)));
    }
}
