//! Quick fixes for TeX's own errors: one table from the error's headline to
//! the edit that repairs it, read off the source where the run raised it.
//! A fix that gives the output TeX's recovery gives is safe; one that guesses
//! what the text meant needs a review.

use crate::engine::builtins::{LoadKind, expandable};
use crate::engine::tex::{Catcode, Meaning, Span, Tok};
use crate::linter::fix::{Applicability, Edit, Edits, Fix, Pos, Scan};

type Repair = fn(&Edits, Span, &str) -> Option<Vec<Fix>>;

/// Each headline TeX (`print_err`) or LaTeX (`\@latex@error`) starts the
/// error with, and its repair.
const REPAIRS: [(&str, Repair); 16] = [
    ("File ended while scanning use of", close_argument),
    ("Paragraph ended before", close_argument),
    ("Too many }'s", drop_brace),
    ("Missing $ inserted", wrap_math),
    ("Extra alignment tab", add_column),
    ("Misplaced alignment tab character", escape_tab),
    ("Misplaced \\noalign", end_row),
    ("Misplaced \\omit", end_row),
    ("LaTeX Error: Missing \\begin{document}", begin_document),
    ("LaTeX Error: Lonely \\item", wrap_list),
    ("LaTeX Error: There's no line here to end", drop_newline),
    ("Illegal unit of measure", add_unit),
    ("Missing number", add_unit),
    ("Missing \\endcsname inserted", end_csname),
    ("Double s", brace_script),
    ("Missing } inserted", close_group),
];

/// The fixes for the error `message` raised at `span`, the likeliest first.
pub(crate) fn repairs(edits: &Edits, span: Span, message: &str) -> Vec<Fix> {
    REPAIRS
        .iter()
        .filter(|(head, _)| message.starts_with(head))
        .find_map(|(_, r)| r(edits, span, message))
        .unwrap_or_default()
}

fn insert(path: &str, at: Pos, text: &str) -> Edit {
    Edit { path: path.into(), start: at, end: at, replacement: text.into() }
}

fn one(description: String, applicability: Applicability, edits: Vec<Edit>) -> Option<Vec<Fix>> {
    Some(vec![Fix::new(description, applicability, Some(edits))])
}

/// The character at `span`, with the file's path, when it is `want`.
fn char_at(edits: &Edits, span: Span, want: char) -> Option<(String, Pos, Pos)> {
    let (path, text) = edits.text(span.file)?;
    let (start, end) = (Pos::of(span), Pos::new(span.line, span.col + 1));
    (text.slice(start, end)? == want.to_string()).then_some((path, start, end))
}

/// How far the text on from a scan reaches: to the last token before a
/// `\par` (tex.web § 392: an argument that is not `\long` stops there) or a
/// command the run saw end a paragraph ([`crate::analysis::facts::Facts::paragraph_enders`]), an
/// `\end` whose `\begin` came before (environments nest as groups), a `}`
/// closing a group opened before (`closed`), or the file's end.  With the
/// groups still open there, and where its first line ends.
struct Reach {
    end: Pos,
    depth: usize,
    line: Option<(Pos, usize)>,
    closed: bool,
    par: bool,
}

fn reach(edits: &Edits, scan: &mut Scan, mut last: Pos) -> Reach {
    let (mut depth, mut envs, mut line, mut closed, mut par) = (0usize, 0usize, None, false, false);
    loop {
        let lexeme = scan.advance();
        if line.is_none() && lexeme.as_ref().is_none_or(|l| l.start.line > last.line) {
            line = Some((last, depth));
        }
        let Some(l) = lexeme else { break };
        match (l.tok, scan.name(&l)) {
            (_, Some(name)) if edits.ends_paragraph(name) => {
                par = true;
                break;
            }
            (_, Some("begin")) => envs += 1,
            (_, Some("end")) if envs == 0 => break,
            (_, Some("end")) => envs -= 1,
            (Tok::Chr(_, Catcode::Space), _) => continue,
            (Tok::Chr(_, Catcode::Begin), _) => depth += 1,
            (Tok::Chr(_, Catcode::End), _) if depth == 0 => {
                closed = true;
                break;
            }
            (Tok::Chr(_, Catcode::End), _) => depth -= 1,
            _ => {}
        }
        last = l.end;
    }
    Reach { end: last, depth, line, closed, par }
}

/// A runaway argument (tex.web §§ 396, 338): the call's `{` never closes.
/// It is closed, with every group still open, at its first line's end, or
/// where its text reaches, the paragraph's end.
fn close_argument(edits: &Edits, span: Span, _: &str) -> Option<Vec<Fix>> {
    let (path, _, mut scan, cs) = edits.command(span)?;
    let name = scan.name(&cs)?.to_string();
    // the arguments before it close; `[…]` are optional ones
    let r = loop {
        if scan.optional().is_some() {
            continue;
        }
        let open = scan.advance()?;
        if !matches!(open.tok, Tok::Chr(_, Catcode::Begin)) {
            return None;
        }
        let r = reach(edits, &mut scan, open.end);
        if !r.closed {
            break r;
        }
    };
    let close = |at: Pos, depth: usize, place: &str| {
        let description = format!("close the argument of `\\{name}` at the end of the {place}");
        Fix::new(description, Applicability::Unsafe, Some(vec![insert(&path, at, &"}".repeat(depth + 1))]))
    };
    // The line's end first: the paragraph's may lie past commands that end a
    // paragraph through their own code (`\section`), which this scan does not see.
    let mut fixes: Vec<Fix> =
        r.line.filter(|(at, _)| *at != r.end).map(|(at, depth)| close(at, depth, "line")).into_iter().collect();
    fixes.push(close(r.end, r.depth, "paragraph"));
    Some(fixes)
}

/// A `{` still open at a token that ends a larger group (`\right`, `$`,
/// `\endgroup` of an `\end`): TeX inserts the `}` right before it (tex.web
/// § 1065 `off_save`).  Written out before a primitive or a character that
/// is what TeX reads; before a macro it closes before the macro's own code
/// runs, which needs a review.
fn close_group(edits: &Edits, span: Span, _: &str) -> Option<Vec<Fix>> {
    let (path, text) = edits.text(span.file)?;
    let mut scan = Scan::at(&text, Pos::of(span), edits.cats(span.file))?;
    let token = scan.advance()?;
    let meaning =
        scan.name(&token).and_then(|n| edits.analysis.interner.lookup(n)).map(|s| edits.analysis.env.meaning(s));
    let safety = match meaning {
        None | Some(Meaning::Primitive(_)) => Applicability::Safe,
        _ => Applicability::Unsafe,
    };
    let what = text.slice(token.start, token.end)?;
    one(format!("close the group before `{what}`"), safety, vec![insert(&path, token.start, "}")])
}

/// TeX ignores a `}` that closes no group (tex.web § 1069): deleting it
/// changes nothing.
fn drop_brace(edits: &Edits, span: Span, _: &str) -> Option<Vec<Fix>> {
    let (path, start, end) = char_at(edits, span, '}')?;
    one(
        "delete the `}` that closes no group".into(),
        Applicability::Safe,
        vec![Edit { path, start, end, replacement: String::new() }],
    )
}

/// A math-only token in text (tex.web § 1047 `insert_dollar_sign`): the
/// token with its nucleus and its scripts,
/// or the command with its arguments, set as a formula, in the delimiters
/// of [`crate::linter::shared::inline_math`]. A command whose arguments are
/// all `*`, `[…]` and `{…}` that the run knows it reads is set alone, the
/// text around it left text: safe. A nucleus found in the source needs a review.
fn wrap_math(edits: &Edits, span: Span, _: &str) -> Option<Vec<Fix>> {
    use crate::engine::tex::ArgType as A;
    let (path, text) = edits.text(span.file)?;
    let mut scan = Scan::at(&text, Pos::of(span), edits.cats(span.file))?;
    let first = scan.advance()?;
    let mut end = first.end;
    let mut safety = Applicability::Unsafe;
    let start = match first.tok {
        // the nucleus: the atom right before the script (tex.web § 1176)
        Tok::Chr(_, Catcode::Sup | Catcode::Sub) => {
            let line = text.line(span.line);
            let before: Vec<char> = line[..crate::engine::source::byte_of_col(line, span.col)].chars().collect();
            Pos::new(span.line, atom_start(&before).unwrap_or(before.len()) as u32 + 1)
        }
        Tok::Cs(_) => {
            let analysis = edits.analysis;
            let sym = analysis.interner.lookup(scan.name(&first)?)?;
            let meaning = analysis.env.meaning(sym);
            let mac = meaning.as_macro();
            // a macro without parameters whose text ends in a letter, `}` or other character reads no further
            let ends = |m: &crate::engine::tex::MacroDef| {
                m.arity() == 0
                    && m.replacement_text
                        .last()
                        .is_some_and(|t| matches!(t.tok, Tok::Chr(_, Catcode::Letter | Catcode::Other | Catcode::End)))
            };
            // none read is known only of a command that cannot read on (`\\sqrt` is `\\protect\\sqrt␣`)
            let known = crate::queries::query::call_items(analysis, sym, mac.map(|m| &**m), false)
                .filter(|items| !items.is_empty() || mac.is_none_or(|m| ends(m)))
                .or_else(|| mac.filter(|m| ends(m)).map(|_| Vec::new()))
                .filter(|items| items.iter().all(|i| matches!(i, A::Star | A::Optional(_) | A::Mandatory)));
            if known.is_some() || matches!(meaning, Meaning::Register(..)) {
                safety = Applicability::Safe;
            }
            let guess = known.is_none();
            for item in known.unwrap_or_else(|| vec![A::Mandatory; mac.map_or(0, |m| m.arity().into())]) {
                match item {
                    A::Star => {
                        scan.skip_spaces();
                        if scan.peek().is_some_and(|l| l.tok == Tok::Chr('*', Catcode::Other)) {
                            end = scan.advance()?.end;
                        }
                    }
                    A::Optional(_) => end = scan.optional().map_or(end, |g| g.end),
                    _ => end = scan.argument()?.end,
                }
            }
            // arguments not known: the `[…]` and `{…}` right after it, for a review
            while guess
                && scan.peek().is_some_and(|l| {
                    l.start == end && matches!(l.tok, Tok::Chr('[', Catcode::Other) | Tok::Chr(_, Catcode::Begin))
                })
            {
                end = scan.optional().or_else(|| scan.group())?.end;
            }
            first.start
        }
        _ => return None,
    };
    if !matches!(first.tok, Tok::Cs(_)) {
        end = scan.argument()?.end;
    }
    // further scripts on the same nucleus
    while scan.peek().is_some_and(|l| matches!(l.tok, Tok::Chr(_, Catcode::Sup | Catcode::Sub)) && l.start == end) {
        scan.advance();
        end = scan.argument()?.end;
    }
    let (open, close) = crate::linter::shared::inline_math(edits.analysis, text.body());
    let edits = vec![insert(&path, start, open), insert(&path, end, close)];
    one(format!("set it as a formula, `{open}…{close}`"), safety, edits)
}

/// The table environment last begun before `span` in its file, when it has
/// not ended there.
fn open_table<'a>(edits: &Edits<'a>, span: Span) -> Option<&'a crate::analysis::facts::Occurrence> {
    use crate::engine::builtins::OccKind;
    let tables = &edits.analysis.settings.conventions.tables.environments;
    let before = |o: &&crate::analysis::facts::Occurrence| {
        o.certain && o.span.file == span.file && (o.span.line, o.span.col) < (span.line, span.col)
    };
    let occurrences = &edits.analysis.facts.occurrences;
    let begin = (occurrences.iter().filter(before))
        .filter(|o| o.kind == OccKind::BeginEnvironment && tables.contains_key(&o.key))
        .max_by_key(|o| (o.span.line, o.span.col))?;
    let ended = (occurrences.iter().filter(before)).any(|o| {
        o.kind == OccKind::EndEnvironment
            && o.key == begin.key
            && (o.span.line, o.span.col) > (begin.span.line, begin.span.col)
    });
    (!ended).then_some(begin)
}

/// A `\noalign` (`\hline`, `\bottomrule`) or `\omit` in a cell of a table
/// (tex.web § 1129): the row lacks its end before it.
fn end_row(edits: &Edits, span: Span, _: &str) -> Option<Vec<Fix>> {
    open_table(edits, span)?;
    let (path, _) = edits.text(span.file)?;
    one("end the row before it".into(), Applicability::Unsafe, vec![insert(&path, Pos::of(span), "\\\\ ")])
}

/// An `&` past the columns of the row (tex.web § 792): the preamble's last
/// column once more, in the alignment it is in.
fn add_column(edits: &Edits, span: Span, _: &str) -> Option<Vec<Fix>> {
    let analysis = edits.analysis;
    let tables = &analysis.settings.conventions.tables.environments;
    let begin = open_table(edits, span)?;
    let (path, text) = edits.text(span.file)?;
    let mut scan = Scan::at(&text, Pos::of(begin.span), edits.cats(span.file))?;
    scan.advance()?;
    scan.group()?;
    for _ in 0..tables[&begin.key] {
        scan.group()?;
    }
    scan.optional();
    let spec = scan.group()?;
    // the last column: a letter at the top level and the groups right after it (`p{3cm}`)
    let (mut depth, mut column, mut taking) = (0usize, None::<(Pos, Pos)>, false);
    for l in &spec.inner {
        match l.tok {
            Tok::Chr(_, Catcode::Begin) => depth += 1,
            Tok::Chr(_, Catcode::End) => {
                depth -= 1;
                if depth == 0 && taking {
                    column = column.map(|(start, _)| (start, l.end));
                }
            }
            _ if depth > 0 => {}
            Tok::Chr(_, Catcode::Letter) => (column, taking) = (Some((l.start, l.end)), true),
            _ => taking = false,
        }
    }
    let (start, end) = column?;
    let kind = text.slice(start, end)?;
    one(format!("add a `{kind}` column to the table"), Applicability::Unsafe, vec![insert(&path, end, kind)])
}

/// An `&` outside any alignment (tex.web § 1128) is most likely meant as
/// the character.
fn escape_tab(edits: &Edits, span: Span, _: &str) -> Option<Vec<Fix>> {
    let (path, start, end) = char_at(edits, span, '&')?;
    one("write `\\&`".into(), Applicability::Unsafe, vec![Edit { path, start, end, replacement: "\\&".into() }])
}

/// Text before a `\begin{document}` the file never has, and no package
/// loaded after it: the document starts there.
fn begin_document(edits: &Edits, span: Span, _: &str) -> Option<Vec<Fix>> {
    let facts = &edits.analysis.facts;
    let begun = facts
        .occurrences
        .iter()
        .any(|o| o.kind == crate::engine::builtins::OccKind::BeginEnvironment && o.key == "document");
    if begun
        || facts
            .loads
            .iter()
            .any(|l| l.kind == LoadKind::Package && l.span.file == span.file && l.span.line >= span.line)
    {
        return None;
    }
    let edits = edits.insert_before(span, "\\begin{document}")?;
    one("begin the document here".into(), Applicability::Unsafe, edits)
}

/// `\item` outside a list: this item and the lonely ones the run met right
/// after it, each to where its text reaches, in an `itemize`.
fn wrap_list(edits: &Edits, span: Span, message: &str) -> Option<Vec<Fix>> {
    let (path, text) = edits.text(span.file)?;
    // Inside `\begin{X}` of an undefined X (a misspelled list): the fix is X's name, not a new list.
    // Its error is the last environment event before the item.
    use crate::engine::builtins::OccKind;
    let before = (edits.analysis.facts.occurrences.iter())
        .filter(|o| o.span.file == span.file && o.span < span)
        .filter(|o| {
            matches!(o.kind, OccKind::BeginEnvironment | OccKind::EndEnvironment) || o.key.contains("undefined")
        })
        .max_by_key(|o| o.span);
    if before.is_some_and(|o| o.key.starts_with("LaTeX Error: Environment ") && o.key.contains(" undefined")) {
        return None;
    }
    let head = message.split('.').next()?;
    let lonely: Vec<Pos> = (edits.analysis.facts.occurrences.iter())
        .filter(|o| o.span.file == span.file && o.key.starts_with(head))
        .map(|o| Pos::of(o.span))
        .collect();
    let mut begin = edits.insert_before(span, "\\begin{itemize}")?;
    let mut scan = Scan::at(&text, Pos::of(span), edits.cats(span.file))?;
    let mut end = scan.advance()?.end;
    loop {
        let r = reach(edits, &mut scan, end);
        end = r.end;
        // past a paragraph's end, the next lonely item goes on the list
        let next = loop {
            let Some(l) = scan.peek().cloned().filter(|_| r.par) else { break None };
            if !matches!(l.tok, Tok::Chr(_, Catcode::Space)) && scan.name(&l) != Some("par") {
                break Some(l);
            }
            scan.advance();
        };
        match next.filter(|l| lonely.contains(&l.start)) {
            Some(l) => end = scan.advance().map_or(l.end, |l| l.end),
            None => break,
        }
    }
    begin.push(insert(&path, end, "\n\\end{itemize}"));
    one("put the items in an `itemize`".into(), Applicability::Unsafe, begin)
}

/// `\\` where no line is open: TeX drops it, so deleting it is safe.  A
/// space it asks for (`\\[1em]`) is most likely meant as `\vspace`.
fn drop_newline(edits: &Edits, span: Span, _: &str) -> Option<Vec<Fix>> {
    let (path, text, mut scan, cs) = edits.command(span)?;
    if scan.name(&cs)? != "\\" {
        return None;
    }
    scan.star();
    let Some(space) = scan.optional() else {
        let line = text.line(cs.end.line);
        let rest = &line[crate::engine::source::byte_of_col(line, cs.end.col)..];
        let end = Pos::new(cs.end.line, cs.end.col + (rest.len() - rest.trim_start().len()) as u32);
        return one(
            "delete the `\\\\`".into(),
            Applicability::Safe,
            vec![Edit { path, start: cs.start, end, replacement: String::new() }],
        );
    };
    let length = text.slice(space.inner_start, space.inner_end)?;
    let edit = Edit { path, start: cs.start, end: space.end, replacement: format!("\\vspace{{{length}}}") };
    one(format!("write `\\vspace{{{length}}}`"), Applicability::Unsafe, vec![edit])
}

/// A length argument that is a bare number gets TeX's `pt` (tex.web § 459),
/// an empty one `0pt` (§ 415): what TeX's recovery reads, written out.
fn add_unit(edits: &Edits, span: Span, message: &str) -> Option<Vec<Fix>> {
    let (path, text, mut scan, _) = edits.command(span)?;
    scan.star();
    while scan.optional().is_some() {}
    let empty = message.starts_with("Missing number");
    let mut out = Vec::new();
    for _ in 0..4 {
        let Some(group) = scan.group() else { break };
        let inner = text.slice(group.inner_start, group.inner_end)?.trim();
        let number = inner.trim_start_matches(['+', '-']);
        let bare = !number.is_empty()
            && number.chars().all(|c| c.is_ascii_digit() || c == '.')
            && number.matches('.').count() <= 1;
        match (empty, inner.is_empty(), bare) {
            (true, true, _) => out.push(Edit {
                path: path.clone(),
                start: group.inner_start,
                end: group.inner_end,
                replacement: "0pt".into(),
            }),
            (false, _, true) => out.push(insert(&path, group.inner_end, "pt")),
            _ => {}
        }
    }
    if out.is_empty() {
        return None;
    }
    one(if empty { "write `0pt`" } else { "write the unit TeX assumes, `pt`" }.into(), Applicability::Safe, out)
}

/// `\csname` meets a command that does not expand before its `\endcsname`
/// (tex.web § 373):
/// TeX inserts the `\endcsname` right before it, written out here.
fn end_csname(edits: &Edits, span: Span, _: &str) -> Option<Vec<Fix>> {
    let (path, _, mut scan, cs) = edits.command(span)?;
    if scan.name(&cs)? != "csname" {
        return None;
    }
    loop {
        let lexeme = scan.advance()?;
        let Tok::Cs(_) = lexeme.tok else { continue };
        let name = scan.name(&lexeme)?;
        let sym = edits.analysis.interner.lookup(name)?;
        match edits.analysis.env.meaning(sym) {
            Meaning::Primitive(p) if !expandable(p) && name != "endcsname" => {
                let edit = insert(&path, lexeme.start, "\\endcsname");
                return one("insert the `\\endcsname` TeX assumes".into(), Applicability::Safe, vec![edit]);
            }
            Meaning::Primitive(p) if expandable(p) => {}
            _ => return None,
        }
    }
}

/// A second `^` (or `_`) on one nucleus (tex.web § 1177): the first scripted nucleus braced,
/// `x^2^3` as `{x^2}^3`, one reading of an ambiguous formula.
fn brace_script(edits: &Edits, span: Span, _: &str) -> Option<Vec<Fix>> {
    let (path, text) = edits.text(span.file)?;
    let line = text.line(span.line);
    let before: Vec<char> = line[..crate::engine::source::byte_of_col(line, span.col)].chars().collect();
    let script =
        line[crate::engine::source::byte_of_col(line, span.col)..].chars().next().filter(|c| "^_".contains(*c))?;
    let end = before.iter().rposition(|c| !c.is_whitespace())? + 1;
    let arg = atom_start(&before[..end])?;
    let at = before[..arg].iter().rposition(|c| !c.is_whitespace())?;
    if before[at] != script {
        return None;
    }
    let nucleus = atom_start(&before[..at])?;
    let col = |i: usize| Pos::new(span.line, i as u32 + 1);
    let edits = vec![insert(&path, col(nucleus), "{"), insert(&path, col(end), "}")];
    one(format!("brace the first scripted part: `{{…{script}…}}{script}`"), Applicability::Unsafe, edits)
}

/// Where the math atom ending `chars` starts: a `{…}` group, a command or one character.
fn atom_start(chars: &[char]) -> Option<usize> {
    let end = chars.iter().rposition(|c| !c.is_whitespace())?;
    if chars[end] == '}' {
        let mut depth = 0;
        for i in (0..=end).rev() {
            match chars[i] {
                '}' => depth += 1,
                '{' if depth == 1 => return Some(i),
                '{' => depth -= 1,
                _ => {}
            }
        }
        return None;
    }
    let word = chars[..=end].iter().rev().take_while(|c| c.is_ascii_alphabetic()).count();
    match end + 1 - word {
        i if word > 0 && i > 0 && chars[i - 1] == '\\' => Some(i - 1),
        _ => Some(end),
    }
}
