//! Lists: an `itemize` whose every `\item` carries its own label is the
//! list LaTeX already has for those labels. Counted labels (`1.`, `(b)`,
//! `iii)`) are an `enumerate`, labels of words a `description`. Read from the
//! source of each list the run entered.

use crate::engine::builtins::OccKind;
use crate::engine::tex::{Catcode, CatcodeTable, Span, Tok};
use crate::linter::fix::{Applicability, Edit, Fix, Pos, Scan, Text};
use crate::linter::{Category, Report, Rule, Severity};

pub static RULES: &[Rule] = &[
    Rule {
        code: "itemize-numbered",
        category: Category::Style,
        severity: Severity::Info,
        summary: "an `itemize` numbers its items by hand",
        explanation: "\
`\\item[1.]`, `\\item[2.]`, … in an `itemize` count by hand what `enumerate`
counts itself: an item added or moved leaves the numbers wrong, and
`\\label` cannot refer to them.  The fix turns the list into an `enumerate`
and drops the labels.  Labels of another shape (`(a)`, `iii)`) or a count
that does not start at 1 keep their look with
[enumitem](https://ctan.org/pkg/enumitem)'s `label` and `start` keys, and a
count that goes on from the list before with its `resume`.  A list of fewer
items than `lints.itemize-numbered.items` (2) is left alone.",
        references: &[],
        fixes: &[Applicability::Safe, Applicability::Unsafe],
        engines: &[],
        run: numbered,
    },
    Rule {
        code: "itemize-described",
        category: Category::Style,
        severity: Severity::Info,
        summary: "an `itemize` labels its items with words",
        explanation: "\
An `itemize` whose every `\\item[…]` names its item in words is a
`description`: that list sets each label in bold and hangs the text after
it, where `itemize` squeezes a long label into the space of a bullet.  A
label counts as words with at least `lints.itemize-described.letters` (4)
letters, set or not by a command, and no digit.",
        references: &[],
        fixes: &[Applicability::Unsafe],
        engines: &[],
        run: described,
    },
];

/// One list as its source has it.
struct List {
    begin: Span,
    /// The environment name inside `\begin{…}` and `\end{…}`.
    names: [(Pos, Pos); 2],
    end: Pos,
    /// Each `\item`'s optional label: the source of `[…]`, where its brackets start and end.
    items: Vec<Option<(String, Pos, Pos)>>,
    /// The `[…]` after `\begin{…}`.
    options: Option<String>,
}

/// Every list of environment `name` the run entered in a document file.
fn lists(report: &Report, name: &str) -> Vec<(List, std::rc::Rc<Text>, String)> {
    let analysis = report.analysis();
    let mut out = Vec::new();
    for o in &analysis.facts.occurrences {
        if o.kind != OccKind::BeginEnvironment || !o.certain || o.key != name {
            continue;
        }
        let Some((path, text)) = report.edits().text(o.span.file) else { continue };
        if let Some(list) = read(&text, o.span) {
            out.push((list, text, path));
        }
    }
    out
}

/// The list whose `\begin` stands at `begin`, its items at its own depth.
fn read(text: &Text, begin: Span) -> Option<List> {
    let mut scan = Scan::at(text, Pos::new(begin.line, begin.col), CatcodeTable::latex())?;
    let head = scan.advance()?;
    (scan.name(&head)? == "begin").then_some(())?;
    let env = scan.group()?;
    let options = scan.optional().and_then(|g| Some(text.slice(g.inner_start, g.inner_end)?.to_string()));
    let (mut items, mut depth, mut nested) = (Vec::new(), 0usize, 0usize);
    while let Some(lexeme) = scan.advance() {
        match lexeme.tok {
            Tok::Chr(_, Catcode::Begin) => depth += 1,
            Tok::Chr(_, Catcode::End) if depth == 0 => return None,
            Tok::Chr(_, Catcode::End) => depth -= 1,
            Tok::Cs(_) if depth == 0 => match scan.name(&lexeme)? {
                "begin" => nested += 1,
                "end" if nested > 0 => nested -= 1,
                "end" => {
                    let close = scan.group()?;
                    let names = [(env.inner_start, env.inner_end), (close.inner_start, close.inner_end)];
                    return Some(List { begin, names, end: close.end, items, options });
                }
                "item" if nested == 0 => {
                    let label = scan.optional();
                    items.push(
                        label.and_then(|g| Some((text.slice(g.inner_start, g.inner_end)?.to_string(), g.start, g.end))),
                    );
                }
                _ => {}
            },
            _ => {}
        }
    }
    None
}

/// What a label shows, command names left out, and where each shown
/// character stands in the label's source.
fn shown(label: &str) -> (Vec<char>, Vec<usize>) {
    let (mut chars, mut at, mut name) = (Vec::new(), Vec::new(), false);
    let mut iter = label.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        if c == '\\' {
            // `\foo` is a name; `\%` and its kin show their character.
            name = iter.peek().is_some_and(|(_, n)| n.is_ascii_alphabetic());
            if !name && let Some((j, n)) = iter.next() {
                chars.push(n);
                at.push(j);
            }
            continue;
        }
        if name && c.is_ascii_alphabetic() {
            continue;
        }
        name = false;
        if !matches!(c, '{' | '}' | '$' | '~') {
            chars.push(c);
            at.push(i);
        }
    }
    (chars, at)
}

/// A counter's styles: enumitem's starred command and how a value reads.
const STYLES: [&str; 5] = ["arabic", "roman", "Roman", "alph", "Alph"];

fn value(style: &str, s: &str) -> Option<u32> {
    let roman = |s: &str| {
        let digit = |c| match c {
            'i' => Some(1),
            'v' => Some(5),
            'x' => Some(10),
            'l' => Some(50),
            'c' => Some(100),
            _ => None,
        };
        let digits: Option<Vec<u32>> = s.chars().map(digit).collect();
        let digits = digits?;
        let n = digits.iter().enumerate().fold(0i64, |n, (k, &d)| {
            if digits.get(k + 1).is_some_and(|&next| next > d) { n - i64::from(d) } else { n + i64::from(d) }
        });
        // only the canonical spelling: `iiii` and `vx` are no numbers
        (n > 0 && to_roman(n as u32) == s).then_some(n as u32)
    };
    let alph = |s: &str| {
        let mut c = s.chars();
        let (first, rest) = (c.next()?, c.next());
        (rest.is_none() && first.is_ascii_lowercase()).then(|| first as u32 - 'a' as u32 + 1)
    };
    match style {
        "arabic" => s.bytes().all(|b| b.is_ascii_digit()).then(|| s.parse().ok()).flatten(),
        "roman" => roman(s),
        "Roman" => s.chars().all(|c| c.is_ascii_uppercase()).then(|| roman(&s.to_ascii_lowercase())).flatten(),
        "alph" => alph(s),
        _ => s.chars().all(|c| c.is_ascii_uppercase()).then(|| alph(&s.to_ascii_lowercase())).flatten(),
    }
}

fn to_roman(mut n: u32) -> String {
    let mut out = String::new();
    for (v, s) in [(100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")] {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

/// Labels that count: each the same text around a counter of one style,
/// one up from the label before. The enumitem label (`(\alph*)`) and the first value.
fn counted(labels: &[&str]) -> Option<(String, u32)> {
    let parts: Vec<_> = labels.iter().map(|l| (l, shown(l))).collect();
    STYLES.into_iter().find_map(|style| {
        let mut out: Option<(String, u32)> = None;
        for (k, (label, (chars, at))) in parts.iter().enumerate() {
            // the counter is the last run of letters and digits: `Step 7:` counts 7
            let hi = chars.len() - chars.iter().rev().position(char::is_ascii_alphanumeric)?;
            let lo = chars[..hi].iter().rposition(|c| !c.is_ascii_alphanumeric()).map_or(0, |p| p + 1);
            let n = value(style, &chars[lo..hi].iter().collect::<String>())?;
            let (start, end) = (at[lo], at[hi - 1] + chars[hi - 1].len_utf8());
            let shape = format!("{}\\{style}*{}", &label[..start], &label[end..]);
            match &out {
                None => out = Some((shape, n)),
                Some((s, first)) if *s == shape && n == first + k as u32 => {}
                _ => return None,
            }
        }
        out
    })
}

/// enumerate's own label at `depth` as the run left `\labelenumi` …
/// `\labelenumiv`, in enumitem's spelling: `\theenumi.` is `\arabic*.`.
fn default_label(analysis: &crate::analysis::machine::Analysis, depth: usize) -> Option<String> {
    let level = ["i", "ii", "iii", "iv"].get(depth)?;
    let text = |name: &str| match analysis.env.meaning(analysis.interner.lookup(name)?) {
        crate::engine::tex::Meaning::Macro(m) if m.arity() == 0 => {
            Some(crate::engine::tex::detokenize(&m.replacement_text, &analysis.interner).replace(' ', ""))
        }
        _ => None,
    };
    let mut label = text(&format!("labelenum{level}"))?;
    let the = format!("\\theenum{level}");
    if label.contains(&the) {
        label = label.replace(&the, &text(&the[1..])?);
    }
    for style in STYLES {
        label = label.replace(&format!("\\@{style}\\c@enum{level}"), &format!("\\{style}*"));
    }
    Some(label)
}

/// The value an `enumerate`'s last item shows, when its options tell.
fn last(list: &List) -> Option<u32> {
    let options = list.options.as_deref().unwrap_or_default();
    let start = options.split(',').find_map(|kv| kv.split_once('=').filter(|(k, _)| k.trim() == "start"));
    let start = start.map_or(Some(1), |(_, v)| v.trim().parse().ok())?;
    (!options.contains("resume")).then(|| start + list.items.len() as u32 - 1)
}

fn numbered(report: &mut Report) {
    let least = report.analysis().settings.lints.itemize_numbered.items;
    let enumerates = lists(report, "enumerate");
    for (list, text, path) in lists(report, "itemize") {
        // an itemize with keys of its own is set up on purpose
        let Some(labels) = labels(&list, least).filter(|_| list.options.is_none()) else { continue };
        let Some((shape, first)) = counted(&labels.iter().map(|(l, _, _)| l.as_str()).collect::<Vec<_>>()) else {
            continue;
        };
        let depth = |l: &List| enumerates.iter().filter(|(e, _, _)| inside(e, l)).count();
        let here = depth(&list);
        // the enumerate before at this depth that ends one short of `first`
        let resume = first > 1
            && (enumerates.iter().rev())
                .find(|(e, _, _)| e.begin.file == list.begin.file && pos(e.end) < key(list.begin) && depth(e) == here)
                .is_some_and(|(e, _, _)| last(e) == Some(first - 1));
        let mut keys = Vec::new();
        if default_label(report.analysis(), here).as_deref() != Some(shape.replace(' ', "").as_str()) {
            keys.push(format!("label={shape}"));
        }
        match (resume, first) {
            (true, _) => keys.push("resume".into()),
            (false, 1) => {}
            (false, n) => keys.push(format!("start={n}")),
        }
        let options = if keys.is_empty() { String::new() } else { format!("[{}]", keys.join(", ")) };
        let mut edits = rename(&list, &path, "enumerate");
        let close = Pos::new(list.names[0].1.line, list.names[0].1.col + 1);
        edits.push(Edit { path: path.clone(), start: close, end: close, replacement: options.clone() });
        for (_, start, end) in &labels {
            let tight = text.slice(*end, Pos::new(end.line, end.col + 1)).is_some_and(|c| !c.trim().is_empty());
            edits.push(Edit {
                path: path.clone(),
                start: *start,
                end: *end,
                replacement: if tight { " " } else { "" }.into(),
            });
        }
        let mut applicability = Applicability::Safe;
        if !keys.is_empty() {
            applicability = Applicability::Unsafe;
            edits.extend(report.edits().ensure_package("enumitem").unwrap_or_default());
        }
        let how = if keys.is_empty() { String::new() } else { format!(" with enumitem's `{options}`") };
        let fix =
            Fix::new(format!("use `enumerate`{how} and drop the hand-written labels"), applicability, Some(edits));
        report.add_fix(
            list.begin,
            "itemize",
            format!("this `itemize` counts its items by hand: use `enumerate`{how}"),
            fix,
        );
    }
}

fn described(report: &mut Report) {
    let settings = &report.analysis().settings.lints.itemize_described;
    let (least, letters) = (settings.items, settings.letters);
    for (list, _, path) in lists(report, "itemize") {
        let Some(labels) = labels(&list, least) else { continue };
        let words = |l: &str| {
            let (chars, _) = shown(l);
            !chars.iter().any(char::is_ascii_digit) && chars.iter().filter(|c| c.is_alphabetic()).count() >= letters
        };
        if !labels.iter().all(|(l, _, _)| words(l)) {
            continue;
        }
        let fix = Fix::new(
            "use `description`, which sets each label in bold before its text",
            Applicability::Unsafe,
            Some(rename(&list, &path, "description")),
        );
        report.add_fix(
            list.begin,
            "itemize",
            "every item of this `itemize` is labelled with words: use `description`".into(),
            fix,
        );
    }
}

/// Every item's label, when each item has one and there are `least`.
fn labels(list: &List, least: usize) -> Option<Vec<(String, Pos, Pos)>> {
    (list.items.len() >= least.max(1)).then_some(())?;
    list.items.iter().cloned().collect()
}

/// The edits that name both ends of `list` `name`.
fn rename(list: &List, path: &str, name: &str) -> Vec<Edit> {
    list.names.iter().map(|&(start, end)| Edit { path: path.into(), start, end, replacement: name.into() }).collect()
}

/// Whether `inner` stands inside `outer`.
fn inside(outer: &List, inner: &List) -> bool {
    outer.begin.file == inner.begin.file && key(outer.begin) < key(inner.begin) && pos(inner.end) < pos(outer.end)
}

fn key(span: Span) -> (u32, u32) {
    (span.line, span.col)
}

fn pos(p: Pos) -> (u32, u32) {
    (p.line, p.col)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each style counts, the text around the counter kept, formatting too.
    #[test]
    fn counted_labels_give_their_shape_and_start() {
        assert_eq!(counted(&["1.", "2.", "3."]), Some(("\\arabic*.".into(), 1)));
        assert_eq!(counted(&["(a)", "(b)"]), Some(("(\\alph*)".into(), 1)));
        assert_eq!(counted(&["iv)", "v)", "vi)"]), Some(("\\roman*)".into(), 4)));
        assert_eq!(counted(&["\\textbf{C.}", "\\textbf{D.}"]), Some(("\\textbf{\\Alph*.}".into(), 3)));
        assert_eq!(counted(&["Step 7:", "Step 8:"]), Some(("Step \\arabic*:".into(), 7)));
        // a gap, mixed shapes and words count nothing
        assert_eq!(counted(&["1.", "3."]), None);
        assert_eq!(counted(&["1.", "2)"]), None);
        assert_eq!(counted(&["Pro", "Con"]), None);
    }

    #[test]
    fn a_label_shows_its_text_without_command_names() {
        let s = |l| shown(l).0.into_iter().collect::<String>();
        assert_eq!(s("\\textbf{Input:}"), "Input:");
        assert_eq!(s("$\\bullet$"), "");
        assert_eq!(s("50\\%"), "50%");
    }
}
