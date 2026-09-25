//! Quick-fixes: the text edits that repair a finding.  A span only says where
//! a command starts, so the extent of what to change is read from the source
//! there, tokenized the way TeX's mouth tokenizes it.

use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::path::Path;
use std::rc::Rc;

use serde_json::{json, Value as Json};

use crate::builtins::{LoadKind, Primitive};
use crate::facts::{CsnameRole, Definition, Load};
use crate::machine::Analysis;
use crate::tex::{Catcode, CatcodeTable, FileId, Interner, Mouth, Span, Sym, Tok};

/// Whether a fix can be applied without a reader checking it: `Safe` keeps
/// what the document means, `Unsafe` changes it in a way that is usually
/// intended but may not be (a package deleted for its side effects, say).
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Applicability {
    Safe,
    Unsafe,
}

impl Applicability {
    pub fn as_str(self) -> &'static str {
        match self {
            Applicability::Safe => "safe",
            Applicability::Unsafe => "unsafe",
        }
    }

    pub fn parse(text: &str) -> Option<Applicability> {
        match text {
            "safe" => Some(Applicability::Safe),
            "unsafe" => Some(Applicability::Unsafe),
            _ => None,
        }
    }
}

/// A place in a file: 1-based line, and 1-based column counted in characters,
/// as every [`Span`] counts them.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub struct Pos {
    pub line: u32,
    pub col: u32,
}

impl Pos {
    pub fn new(line: u32, col: u32) -> Pos {
        Pos { line, col }
    }

    fn of(span: Span) -> Pos {
        Pos::new(span.line, span.col)
    }
}

/// Replace the text from `start` up to, not including, `end`.  An insertion
/// has `start == end`, a deletion an empty `replacement`.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Edit {
    pub path: String,
    pub start: Pos,
    pub end: Pos,
    pub replacement: String,
}

impl Edit {
    pub fn to_json(&self) -> Json {
        json!({
            "path": self.path,
            "start": { "line": self.start.line, "col": self.start.col },
            "end": { "line": self.end.line, "col": self.end.col },
            "replacement": self.replacement,
        })
    }

    pub fn from_json(value: &Json) -> Option<Edit> {
        let pos = |key: &str| {
            let at = value.get(key)?;
            Some(Pos::new(at.get("line")?.as_u64()? as u32, at.get("col")?.as_u64()? as u32))
        };
        Some(Edit {
            path: value.get("path")?.as_str()?.to_string(),
            start: pos("start")?,
            end: pos("end")?,
            replacement: value.get("replacement")?.as_str()?.to_string(),
        })
    }
}

/// What a rule suggests: the prose, and the edits that carry it out when the
/// fix is mechanical.
pub struct Fix {
    pub description: String,
    pub applicability: Applicability,
    pub edits: Vec<Edit>,
}

impl Fix {
    pub fn new(description: impl Into<String>, applicability: Applicability, edits: Option<Vec<Edit>>) -> Fix {
        Fix { description: description.into(), applicability, edits: edits.unwrap_or_default() }
    }
}

/// A file's text with its line starts, for moving between [`Pos`]itions,
/// byte offsets and the UTF-16 columns editors count in.
pub struct Text {
    body: String,
    starts: Vec<usize>,
}

impl Text {
    pub fn new(body: String) -> Text {
        let mut starts = vec![0];
        starts.extend(body.match_indices('\n').map(|(i, _)| i + 1));
        Text { body, starts }
    }

    pub fn body(&self) -> &str {
        &self.body
    }

    fn line_range(&self, line: u32) -> Option<(usize, usize)> {
        let index = line.checked_sub(1)? as usize;
        let start = *self.starts.get(index)?;
        let end = self.starts.get(index + 1).map_or(self.body.len(), |next| next - 1);
        Some((start, end))
    }

    /// The line without its end-of-line character.
    pub fn line(&self, line: u32) -> &str {
        self.line_range(line).map_or("", |(start, end)| &self.body[start..end])
    }

    /// The byte offset of a position.  One column past the last character
    /// is the end of the line, and a position on the line after the last is
    /// the end of the file.
    pub fn offset(&self, pos: Pos) -> Option<usize> {
        let (start, end) = self.line_range(pos.line)?;
        let skip = pos.col.checked_sub(1)? as usize;
        match self.body[start..end].char_indices().nth(skip) {
            Some((i, _)) => Some(start + i),
            None if self.body[start..end].chars().count() == skip => Some(end),
            None => None,
        }
    }

    pub fn pos(&self, offset: usize) -> Pos {
        let index = self.starts.partition_point(|start| *start <= offset).max(1) - 1;
        let col = self.body[self.starts[index]..offset].chars().count() as u32 + 1;
        Pos::new(index as u32 + 1, col)
    }

    pub fn end(&self) -> Pos {
        self.pos(self.body.len())
    }

    pub fn slice(&self, start: Pos, end: Pos) -> Option<&str> {
        let (a, b) = (self.offset(start)?, self.offset(end)?);
        (a <= b).then(|| &self.body[a..b])
    }

    /// The position as the Language Server Protocol counts it: 0-based line,
    /// and the column in UTF-16 code units.
    pub fn utf16(&self, pos: Pos) -> Option<(u32, u32)> {
        let (start, _) = self.line_range(pos.line)?;
        let offset = self.offset(pos)?;
        Some((pos.line - 1, self.body[start..offset].encode_utf16().count() as u32))
    }
}

/// The files the fixes read, each read once: through the overlay, so that a
/// rerun after an edit sees the edited text.
#[derive(Default)]
pub struct Sources {
    texts: RefCell<HashMap<String, Option<Rc<Text>>>>,
}

impl Sources {
    pub fn get(&self, path: &str) -> Option<Rc<Text>> {
        self.texts
            .borrow_mut()
            .entry(path.to_string())
            .or_insert_with(|| crate::overlay::read_to_string(Path::new(path)).ok().map(|t| Rc::new(Text::new(t))))
            .clone()
    }
}

/// A file a fix may change: one of the project's, beside or below the main
/// file, not in the TeX installation, and read as it is written (a `.dtx`
/// is read the way docstrip reads it, so its positions are not the file's).
pub fn editable(analysis: &Analysis, path: &str) -> bool {
    let path = Path::new(path);
    if crate::literate::Literate::of(path).is_some() {
        return false;
    }
    let Ok(canonical) = path.canonicalize() else { return false };
    let installed = analysis
        .distribution
        .roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| canonical.starts_with(root));
    let main = Path::new(analysis.file_name(analysis.main_file)).canonicalize();
    let project = main.ok().and_then(|main| main.parent().map(Path::to_path_buf));
    !installed && project.is_some_and(|root| canonical.starts_with(root))
}

/// One token as read from the source, with where it starts and ends.
#[derive(Clone, Debug)]
pub struct Lexeme {
    pub tok: Tok,
    pub start: Pos,
    pub end: Pos,
}

/// `{…}` or `[…]`: where the delimiters are, and the tokens between them.
pub struct Group {
    pub start: Pos,
    pub end: Pos,
    pub inner_start: Pos,
    pub inner_end: Pos,
    pub inner: Vec<Lexeme>,
}

/// TeX's mouth, started at a position of a file instead of its beginning.
pub struct Scan {
    mouth: Mouth,
    cats: CatcodeTable,
    interner: Interner,
    origin: Pos,
    ahead: VecDeque<Lexeme>,
}

const SPACE: Catcode = Catcode::Space;

impl Scan {
    pub fn at(text: &Text, pos: Pos, cats: CatcodeTable) -> Option<Scan> {
        let offset = text.offset(pos)?;
        Some(Scan {
            mouth: Mouth::new(&text.body[offset..], 0),
            cats,
            interner: Interner::default(),
            origin: pos,
            ahead: VecDeque::new(),
        })
    }

    fn translate(&self, line: u32, col: u32) -> Pos {
        match line {
            1 => Pos::new(self.origin.line, self.origin.col + col - 1),
            _ => Pos::new(self.origin.line + line - 1, col),
        }
    }

    fn lex(&mut self) -> Option<Lexeme> {
        let token = self.mouth.next(&self.cats, &mut self.interner)?;
        let here = self.mouth.here();
        Some(Lexeme {
            tok: token.tok,
            start: self.translate(token.span.line, token.span.col),
            end: self.translate(here.line, here.col),
        })
    }

    pub fn advance(&mut self) -> Option<Lexeme> {
        self.ahead.pop_front().or_else(|| self.lex())
    }

    pub fn peek(&mut self) -> Option<&Lexeme> {
        if self.ahead.is_empty() {
            let lexeme = self.lex()?;
            self.ahead.push_back(lexeme);
        }
        self.ahead.front()
    }

    pub fn name(&self, lexeme: &Lexeme) -> Option<&str> {
        match lexeme.tok {
            Tok::Cs(sym) => Some(self.interner.name(sym)),
            _ => None,
        }
    }

    pub fn skip_spaces(&mut self) {
        while self.peek().is_some_and(|l| matches!(l.tok, Tok::Chr(_, SPACE))) {
            self.advance();
        }
    }

    fn peek_is(&mut self, ch: char, cat: Catcode) -> bool {
        self.peek().is_some_and(|l| l.tok == Tok::Chr(ch, cat))
    }

    /// A balanced `{…}`, after the spaces TeX skips before an argument.
    pub fn group(&mut self) -> Option<Group> {
        self.skip_spaces();
        let open = self.advance()?;
        if !matches!(open.tok, Tok::Chr(_, Catcode::Begin)) {
            return None;
        }
        self.until(open, |_, _| false)
    }

    /// `[…]` when one follows, the way `\@ifnextchar[` looks for it.
    pub fn optional(&mut self) -> Option<Group> {
        self.skip_spaces();
        if !self.peek_is('[', Catcode::Other) {
            return None;
        }
        let open = self.advance()?;
        self.until(open, |tok, depth| depth == 0 && tok == Tok::Chr(']', Catcode::Other))
    }

    pub fn star(&mut self) -> bool {
        self.skip_spaces();
        let star = self.peek_is('*', Catcode::Other);
        if star {
            self.advance();
        }
        star
    }

    /// An undelimited argument: a group, or one token.
    pub fn argument(&mut self) -> Option<Group> {
        self.skip_spaces();
        if self.peek().is_some_and(|l| matches!(l.tok, Tok::Chr(_, Catcode::Begin))) {
            return self.group();
        }
        let token = self.advance()?;
        if matches!(token.tok, Tok::Chr(_, Catcode::End)) {
            return None;
        }
        Some(Group { start: token.start, end: token.end, inner_start: token.start, inner_end: token.end, inner: vec![token] })
    }

    fn until(&mut self, open: Lexeme, closes: impl Fn(Tok, u32) -> bool) -> Option<Group> {
        let mut depth = 0u32;
        let mut inner = Vec::new();
        loop {
            let lexeme = self.advance()?;
            if closes(lexeme.tok, depth) {
                return Some(Group { start: open.start, end: lexeme.end, inner_start: open.end, inner_end: lexeme.start, inner });
            }
            match lexeme.tok {
                Tok::Chr(_, Catcode::Begin) => depth += 1,
                Tok::Chr(_, Catcode::End) if depth == 0 => {
                    return matches!(open.tok, Tok::Chr(_, Catcode::Begin)).then(|| Group {
                        start: open.start,
                        end: lexeme.end,
                        inner_start: open.end,
                        inner_end: lexeme.start,
                        inner,
                    });
                }
                Tok::Chr(_, Catcode::End) => depth -= 1,
                _ => {}
            }
            inner.push(lexeme);
        }
    }
}

/// The items of a comma list such as `\usepackage{a,b}`'s, with where each
/// one starts and ends.  `None` when an item is not plain text.
fn list_items(group: &Group) -> Option<Vec<(Pos, Pos, String)>> {
    let mut items = Vec::new();
    let mut current: Option<(Pos, Pos, String)> = None;
    for lexeme in &group.inner {
        match lexeme.tok {
            Tok::Chr(',', Catcode::Other) => items.extend(current.take()),
            Tok::Chr(_, SPACE) => {}
            Tok::Chr(c, Catcode::Letter | Catcode::Other) => {
                let item = current.get_or_insert_with(|| (lexeme.start, lexeme.end, String::new()));
                item.1 = lexeme.end;
                item.2.push(c);
            }
            _ => return None,
        }
    }
    items.extend(current);
    Some(items)
}

/// Builds the edits for one finding, against the analysis that produced it.
pub struct Edits<'a> {
    pub analysis: &'a Analysis,
    pub sources: &'a Sources,
}

impl<'a> Edits<'a> {
    /// A file's path and text, when a fix may change it.
    pub fn text(&self, file: FileId) -> Option<(String, Rc<Text>)> {
        let path = self.analysis.file_name(file).to_string();
        self.path(&path)
    }

    /// Insert `%` right after the last character of a line — the fix for a
    /// line end that fell into a macro body as a stray space token (tex.web
    /// § 347: end-of-line becomes a space in state M; `%` would have
    /// swallowed it instead).
    pub fn insert_percent(&self, span: Span) -> Option<Vec<Edit>> {
        let (path, _) = self.text(span.file)?;
        let pos = Pos::of(span);
        Some(vec![Edit { path, start: pos, end: pos, replacement: "%".to_string() }])
    }

    pub fn path(&self, path: &str) -> Option<(String, Rc<Text>)> {
        if !editable(self.analysis, path) {
            return None;
        }
        Some((path.to_string(), self.sources.get(path)?))
    }

    /// The category codes a file is read with: LaTeX's, with `@` a letter
    /// in a package or class, which `\usepackage` and `\documentclass` make
    /// it (source2e, ltclass.dtx).
    fn cats(&self, file: FileId) -> CatcodeTable {
        let mut cats = CatcodeTable::latex();
        let kind = self.analysis.files.get(file as usize).map(|f| f.kind);
        if matches!(kind, Some(LoadKind::Package | LoadKind::Class)) {
            cats.set('@', Catcode::Letter);
        }
        cats
    }

    /// The source at a span, and the control sequence that starts there.
    fn command(&self, span: Span) -> Option<(String, Rc<Text>, Scan, Lexeme)> {
        let (path, text) = self.text(span.file)?;
        let mut scan = Scan::at(&text, Pos::of(span), self.cats(span.file))?;
        let cs = scan.advance()?;
        scan.name(&cs)?;
        Some((path, text, scan, cs))
    }

    fn meaning(&self, name: &str) -> Option<Primitive> {
        self.analysis.env.meaning(self.analysis.interner.lookup(name)?).prim()
    }

    fn defined(&self, name: &str) -> bool {
        self.analysis.interner.lookup(name).is_some_and(|sym| self.analysis.env.is_defined(sym))
    }

    /// Delete `\usepackage[…]{name}`, or only `name` when the command loads
    /// others too.
    pub fn delete_load(&self, load: &Load) -> Option<Vec<Edit>> {
        let (path, text, mut scan, cs) = self.command(load.span)?;
        scan.optional();
        let list = scan.group()?;
        let items = list_items(&list)?;
        let index = items.iter().position(|(_, _, name)| *name == load.name)?;
        let edit = match (index.checked_sub(1), items.get(index + 1)) {
            (_, _) if items.len() == 1 => return Some(vec![removal(&text, &path, cs.start, list.end)]),
            (_, Some(next)) => Edit { path, start: items[index].0, end: next.0, replacement: String::new() },
            (Some(previous), None) => {
                Edit { path, start: items[previous].1, end: items[index].1, replacement: String::new() }
            }
            (None, None) => return None,
        };
        Some(vec![edit])
    }

    /// The options and names of the `\cs[…]{a,b}` call at `span`.
    pub fn load_request(&self, span: Span) -> Option<(Vec<String>, Vec<String>)> {
        let (_, text, mut scan, _) = self.command(span)?;
        let options = match scan.optional() {
            Some(group) => crate::tex::comma_split(text.slice(group.inner_start, group.inner_end)?),
            None => Vec::new(),
        };
        let names = list_items(&scan.group()?)?.into_iter().map(|(_, _, name)| name).collect();
        Some((options, names))
    }

    /// Give a load the options it lacks, when it loads nothing else.
    pub fn add_options(&self, load: &Load, extra: &[&str]) -> Option<Vec<Edit>> {
        let (path, _, mut scan, cs) = self.command(load.span)?;
        let options = scan.optional();
        let list = scan.group()?;
        if list_items(&list)?.len() != 1 {
            return None;
        }
        let extra = extra.join(",");
        let edit = match options {
            None => Edit { path, start: cs.end, end: cs.end, replacement: format!("[{extra}]") },
            Some(group) => match group.inner.iter().rev().find(|l| !matches!(l.tok, Tok::Chr(_, SPACE))) {
                None => Edit { path, start: group.inner_start, end: group.inner_end, replacement: extra },
                Some(last) => Edit { path, start: last.end, end: last.end, replacement: format!(",{extra}") },
            },
        };
        Some(vec![edit])
    }

    /// Move a load that stands after `\begin{document}` to the line before it.
    pub fn move_to_preamble(&self, load: &Load, begin: Span) -> Option<Vec<Edit>> {
        if begin.file != load.span.file {
            return None;
        }
        let (path, text, mut scan, cs) = self.command(load.span)?;
        let options = scan.optional();
        let options = match &options {
            Some(group) => text.slice(group.start, group.end)?,
            None => "",
        };
        let name = scan.name(&cs)?.to_string();
        let mut edits = self.insert_before(begin, &format!("\\{name}{options}{{{}}}", load.name))?;
        edits.extend(self.delete_load(load)?);
        let _ = path;
        Some(edits)
    }

    /// Insert a line of its own before the line `at` starts, when `at` is
    /// the first thing on it.
    pub fn insert_before(&self, at: Span, line: &str) -> Option<Vec<Edit>> {
        let (path, text) = self.text(at.file)?;
        let before: String = text.line(at.line).chars().take(at.col.saturating_sub(1) as usize).collect();
        if !before.trim().is_empty() {
            return None;
        }
        let start = Pos::new(at.line, 1);
        Some(vec![Edit { path, start, end: start, replacement: format!("{line}\n") }])
    }

    /// Replace the name of the command at `span` by what `rename` makes of
    /// it: `\newcommand` by `\renewcommand`, say.
    pub fn rename_command(&self, span: Span, rename: impl Fn(&str) -> Option<String>) -> Option<Vec<Edit>> {
        let (path, _, scan, cs) = self.command(span)?;
        let renamed = rename(scan.name(&cs)?)?;
        Some(vec![Edit { path, start: cs.start, end: cs.end, replacement: format!("\\{renamed}") }])
    }

    /// The same command with its `new` turned into `renew` or back, when
    /// the document has a command of that name: LaTeX pairs its defining
    /// commands this way (`\newcommand`, `\NewDocumentCommand`, …).
    pub fn counterpart(&self, name: &str, from: &str, to: &str) -> Option<String> {
        let capitalized = |s: &str| {
            let mut chars = s.chars();
            chars.next().map(|c| c.to_uppercase().chain(chars).collect::<String>()).unwrap_or_default()
        };
        [(from.to_string(), to.to_string()), (capitalized(from), capitalized(to))]
            .into_iter()
            .find_map(|(from, to)| name.strip_prefix(&from).map(|rest| format!("{to}{rest}")))
            .filter(|renamed| self.defined(renamed))
    }

    /// The source a quantity was set from, up to where the observed unit
    /// text is complete.  `via` is the control sequence the file called
    /// where the unit was set.
    pub fn quantity(
        &self,
        span: Span,
        number: &str,
        unit: &str,
        via: Option<&str>,
        replacement: &str,
    ) -> Option<Vec<Edit>> {
        /// Lexemes a unit may take before the source is given up on.
        const STEPS: usize = 8;
        let (path, text) = self.text(span.file)?;
        let start = Pos::of(span);
        let mut scan = Scan::at(&text, start, self.cats(span.file))?;
        let mut digits = String::new();
        loop {
            let Some(Tok::Chr(c, Catcode::Other)) = scan.peek().map(|l| l.tok) else { break };
            if !(c.is_ascii_digit() || (matches!(c, '.' | ',') && !digits.is_empty())) {
                break;
            }
            digits.push(c);
            scan.advance();
        }
        if digits != number {
            return None;
        }
        let mut seen = String::new();
        let mut end = None;
        for _ in 0..STEPS {
            if seen == unit {
                break;
            }
            let lexeme = scan.advance()?;
            match lexeme.tok {
                Tok::Chr(c, Catcode::Letter | Catcode::Other) => seen.push(c),
                Tok::Chr(_, Catcode::Active) => {}
                Tok::Cs(_) => {
                    let name = scan.name(&lexeme)?.to_string();
                    if matches!(scan.peek().map(|l| l.tok), Some(Tok::Chr(_, Catcode::Begin))) {
                        let group = scan.group()?;
                        for inner in &group.inner {
                            match inner.tok {
                                Tok::Chr(c, Catcode::Letter | Catcode::Other) => seen.push(c),
                                _ => return None,
                            }
                        }
                        end = Some(group.end);
                        continue;
                    }
                    if name == "%" {
                        seen.push('%');
                    } else if via == Some(name.as_str()) {
                        seen = unit.to_string();
                    }
                }
                _ => return None,
            }
            end = Some(lexeme.end);
        }
        if seen != unit {
            return None;
        }
        Some(vec![Edit { path, start, end: end?, replacement: replacement.to_string() }])
    }

    /// Delete a command whose one argument reads `key`: `\label{sec:x}`.
    pub fn delete_command(&self, span: Span, key: &str) -> Option<Vec<Edit>> {
        let (path, text, mut scan, cs) = self.command(span)?;
        let group = scan.group()?;
        if text.slice(group.inner_start, group.inner_end)?.trim() != key {
            return None;
        }
        Some(vec![removal(&text, &path, cs.start, group.end)])
    }

    /// Replace the argument of `\end{old}`-like commands by `new`.
    pub fn replace_argument(&self, span: Span, old: &str, new: &str) -> Option<Vec<Edit>> {
        let (path, text, mut scan, _) = self.command(span)?;
        let group = scan.group()?;
        if text.slice(group.inner_start, group.inner_end)?.trim() != old {
            return None;
        }
        Some(vec![Edit { path, start: group.inner_start, end: group.inner_end, replacement: new.to_string() }])
    }

    /// Delete a definition that has lines of its own.  `\def` reads a name,
    /// the parameter text up to `{` and the replacement text (The TeXbook,
    /// ch. 20).  A macro the file called for it, such as `\newcommand`,
    /// stands before the name on its line and reads the arguments that
    /// follow the name.
    pub fn delete_definition(&self, def: &Definition) -> Option<Vec<Edit>> {
        let by = self.analysis.interner.name(def.by);
        let name = self.analysis.interner.name(def.name);
        let primitive = self.analysis.env.meaning(def.by).prim();
        let at = match primitive {
            Some(_) => def.span,
            None => self.call_before(def.span, by)?,
        };
        let (path, text, mut scan, cs) = self.command(at)?;
        if scan.name(&cs)? != by {
            return None;
        }
        let body = match primitive {
            Some(Primitive::Def { .. }) => {
                let target = scan.advance()?;
                if scan.name(&target)? != name {
                    return None;
                }
                while !scan.peek().is_some_and(|l| matches!(l.tok, Tok::Chr(_, Catcode::Begin))) {
                    scan.advance()?;
                }
                scan.group()?
            }
            Some(_) => return None,
            None => {
                scan.star();
                let target = scan.argument()?;
                match target.inner.as_slice() {
                    [only] if scan.name(only) == Some(name) => {}
                    _ => return None,
                }
                let mut last = target;
                loop {
                    if let Some(optional) = scan.optional() {
                        last = optional;
                    } else if scan.peek().is_some_and(|l| matches!(l.tok, Tok::Chr(_, Catcode::Begin))) {
                        last = scan.group()?;
                    } else {
                        break;
                    }
                }
                last
            }
        };
        whole_lines(&text, &path, cs.start, body.end).map(|edit| vec![edit])
    }

    /// Where `\⟨command⟩` stands last before `span` on its line.
    fn call_before(&self, span: Span, command: &str) -> Option<Span> {
        let (_, text) = self.text(span.file)?;
        let end = text.offset(Pos::of(span))?;
        let line = text.offset(Pos::new(span.line, 1))?;
        let at = text.body()[line..end].rfind(&format!("\\{command}"))? + line;
        let pos = text.pos(at);
        Some(Span::new(span.file, pos.line, pos.col))
    }

    /// `\csname…\endcsname` in the role the run saw it in, spelled with the
    /// command that does the same: `\@nameuse{…}`, `\csdef{…}`, …  Only
    /// when that command's name can be written in the file, which for a
    /// name with `@` means a package or class.
    pub fn csname(&self, span: Span, role: CsnameRole, alternative: Sym) -> Option<Vec<Edit>> {
        let (path, text) = self.text(span.file)?;
        let cats = self.cats(span.file);
        let alternative = self.analysis.interner.name(alternative);
        if alternative.chars().any(|c| cats.get(c) != Catcode::Letter) {
            return None;
        }
        let start = match role {
            CsnameRole::Use => Pos::of(span),
            CsnameRole::Define { .. } | CsnameRole::Let => {
                let mut scan = Scan::at(&text, Pos::new(span.line, 1), cats.clone())?;
                let mut lexemes = Vec::new();
                while let Some(lexeme) = scan.advance() {
                    if lexeme.start >= Pos::of(span) {
                        break;
                    }
                    lexemes.push((scan.name(&lexeme).map(str::to_string), lexeme.start));
                }
                let [.., (Some(expandafter), at), (Some(definer), _)] = lexemes.as_slice() else { return None };
                if self.meaning(expandafter) != Some(Primitive::ExpandAfter) {
                    return None;
                }
                let expected = match role {
                    CsnameRole::Define { global, expand } => Primitive::Def { global, expand },
                    _ => Primitive::Let { global: false, future: false },
                };
                if self.meaning(definer) != Some(expected) {
                    return None;
                }
                // A prefix before `\expandafter` would apply to the new command.
                if let [.., (Some(prefix), _), _, _] = lexemes.as_slice()
                    && matches!(self.meaning(prefix), Some(Primitive::Prefix(_)))
                {
                    return None;
                }
                *at
            }
            _ => return None,
        };
        let mut scan = Scan::at(&text, Pos::of(span), cats)?;
        let csname = scan.advance()?;
        if self.meaning(scan.name(&csname)?) != Some(Primitive::Csname) {
            return None;
        }
        let mut first = None;
        let mut depth = 0i32;
        let endcsname = loop {
            let lexeme = scan.advance()?;
            if scan.name(&lexeme).is_some_and(|n| self.meaning(n) == Some(Primitive::Endcsname)) {
                break lexeme;
            }
            match lexeme.tok {
                Tok::Chr(_, Catcode::Begin) => depth += 1,
                Tok::Chr(_, Catcode::End) => depth -= 1,
                _ => {}
            }
            if depth < 0 {
                return None;
            }
            first.get_or_insert(lexeme.start);
        };
        if depth != 0 {
            return None;
        }
        let inner = text.slice(first.unwrap_or(endcsname.start), endcsname.start)?;
        if role == CsnameRole::Let && scan.peek().is_some_and(|l| l.tok == Tok::Chr('=', Catcode::Other)) {
            return None;
        }
        let mut replacement = format!("\\{alternative}{{{inner}}}");
        // TeX skipped the spaces after `\endcsname`; after `}` they would count.
        let end = match scan.peek() {
            Some(next) if next.start.line == endcsname.end.line => next.start,
            _ => {
                let rest: String =
                    text.line(endcsname.end.line).chars().skip(endcsname.end.col as usize - 1).collect();
                let blank = rest.chars().take_while(|c| cats_space(*c)).count();
                if !rest[rest.char_indices().nth(blank).map_or(rest.len(), |(i, _)| i)..].starts_with('%') {
                    replacement.push('%');
                }
                Pos::new(endcsname.end.line, endcsname.end.col + blank as u32)
            }
        };
        Some(vec![Edit { path, start, end, replacement }])
    }

    /// Append a package to a dependency file, with the directive of the
    /// entry before it.
    pub fn add_dependency(&self, file: &Path, package: &str, directive: Option<&str>) -> Option<Vec<Edit>> {
        let (path, text) = self.path(file.to_str()?)?;
        let line = match directive {
            Some(directive) => format!("{directive} {package}\n"),
            None => format!("{package}\n"),
        };
        let at = text.end();
        let replacement = match text.body().is_empty() || text.body().ends_with('\n') {
            true => line,
            false => format!("\n{line}"),
        };
        Some(vec![Edit { path, start: at, end: at, replacement }])
    }

    /// Remove a package from its line of a dependency file, or the line when
    /// nothing else is on it.
    pub fn drop_dependency(&self, file: &Path, line: usize, package: &str) -> Option<Vec<Edit>> {
        let (path, text) = self.path(file.to_str()?)?;
        let line = line as u32;
        let words = words(text.line(line));
        let index = words.iter().position(|(_, word)| *word == package)?;
        let directive = |word: &str| matches!(word, "hard" | "soft");
        let others = words.iter().filter(|(_, word)| *word != package && !directive(word)).count();
        if others == 0 {
            let end = if (line as usize) < text.starts.len() { Pos::new(line + 1, 1) } else { text.end() };
            return Some(vec![Edit { path, start: Pos::new(line, 1), end, replacement: String::new() }]);
        }
        let (col, word) = words[index];
        let start = Pos::new(line, col);
        let end = match words.get(index + 1) {
            Some((next, _)) => Pos::new(line, *next),
            None => Pos::new(line, col + word.chars().count() as u32),
        };
        let start = match (words.get(index + 1), index.checked_sub(1)) {
            (None, Some(previous)) => Pos::new(line, words[previous].0 + words[previous].1.chars().count() as u32),
            _ => start,
        };
        Some(vec![Edit { path, start, end, replacement: String::new() }])
    }
}

fn cats_space(c: char) -> bool {
    CatcodeTable::latex().get(c) == SPACE
}

/// Whitespace-separated words of a line with the column each starts at.
fn words(line: &str) -> Vec<(u32, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (col, (i, c)) in line.char_indices().enumerate() {
        match (c.is_whitespace(), start) {
            (false, None) => start = Some((col, i)),
            (true, Some((col0, i0))) => {
                out.push((col0 as u32 + 1, &line[i0..i]));
                start = None;
            }
            _ => {}
        }
    }
    if let Some((col0, i0)) = start {
        out.push((col0 as u32 + 1, &line[i0..]));
    }
    out
}

/// The lines from `start` to `end` when nothing else stands on them but
/// blanks and a comment, deleted with their line ends.
fn whole_lines(text: &Text, path: &str, start: Pos, end: Pos) -> Option<Edit> {
    let before: String = text.line(start.line).chars().take(start.col as usize - 1).collect();
    let after: String = text.line(end.line).chars().skip(end.col as usize - 1).collect();
    let after = after.trim_start();
    if !before.trim().is_empty() || !(after.is_empty() || after.starts_with('%')) {
        return None;
    }
    let last = text.end();
    let end = if end.line < last.line { Pos::new(end.line + 1, 1) } else { last };
    Some(Edit { path: path.to_string(), start: Pos::new(start.line, 1), end, replacement: String::new() })
}

/// Delete a command: its lines when it has them to itself, else just it.
fn removal(text: &Text, path: &str, start: Pos, end: Pos) -> Edit {
    whole_lines(text, path, start, end)
        .unwrap_or_else(|| Edit { path: path.to_string(), start, end, replacement: String::new() })
}
