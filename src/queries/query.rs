//! Queries and filters over a finished analysis.
//! Filters: `tag=switch and package~^my` evaluated against record fields.

use crate::fixed::HashMap;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use regex::Regex;
use std::fmt::Write as _;

use serde_json::{Map, Value as Json, json};

use crate::analysis::facts::{Load, LoadStatus};
use crate::analysis::graph::EdgeKind;
use crate::analysis::machine::Analysis;
use crate::engine::builtins::{LoadKind, OccKind};
use crate::engine::env::NodeId;
use crate::engine::tex::{FileId, Meaning, Span, Sym, detokenize};

pub type Record = Map<String, Json>;

/// A comparison a filter test uses: equals, matches a regex, numeric
/// ordering, and so on. Part of a [`Filter::Test`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Match,
    NotMatch,
    Lt,
    Le,
    Gt,
    Ge,
    Contains,
}

/// A boolean expression over a [`Record`]'s fields, as `satex query --filter`
/// and friends parse it: field tests combined with `and`/`or`/`not`.
#[derive(Debug)]
pub enum Filter {
    Always,
    Test { field: String, op: Op, operand: String, regex: Option<Regex> },
    And(Box<Filter>, Box<Filter>),
    Or(Box<Filter>, Box<Filter>),
    Not(Box<Filter>),
}

impl Filter {
    pub fn parse(text: &str) -> Result<Filter, String> {
        let tokens = lex_filter(text);
        let mut parser = FilterParser { tokens, pos: 0 };
        let filter = parser.disjunction()?;
        if parser.pos != parser.tokens.len() {
            return Err("trailing input after the filter expression".into());
        }
        Ok(filter)
    }

    pub fn accepts(&self, record: &Record) -> bool {
        match self {
            Filter::Always => true,
            Filter::And(a, b) => a.accepts(record) && b.accepts(record),
            Filter::Or(a, b) => a.accepts(record) || b.accepts(record),
            Filter::Not(a) => !a.accepts(record),
            Filter::Test { field, op, operand, regex } => {
                let Some(value) = record.get(field) else { return false };
                let text = render_field(value);
                match op {
                    Op::Eq => text == *operand,
                    Op::Ne => text != *operand,
                    Op::Contains => text.contains(operand.as_str()),
                    Op::Match | Op::NotMatch => {
                        let hit = regex.as_ref().is_some_and(|r| r.is_match(&text));
                        if *op == Op::Match { hit } else { !hit }
                    }
                    _ => {
                        let (Some(a), Some(b)) = (value.as_f64(), operand.parse::<f64>().ok()) else {
                            return false;
                        };
                        match op {
                            Op::Lt => a < b,
                            Op::Le => a <= b,
                            Op::Gt => a > b,
                            Op::Ge => a >= b,
                            _ => unreachable!("comparison operators only"),
                        }
                    }
                }
            }
        }
    }
}

fn render_field(value: &Json) -> String {
    match value {
        Json::String(s) => s.clone(),
        Json::Null => String::new(),
        other => other.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Lexeme {
    Word(String),
    Operator(Op),
    Open,
    Close,
}

const OPERATORS: [(&str, Op); 9] = [
    ("!~", Op::NotMatch),
    ("!=", Op::Ne),
    (">=", Op::Ge),
    ("<=", Op::Le),
    ("~=", Op::Contains),
    ("~", Op::Match),
    ("=", Op::Eq),
    (">", Op::Gt),
    ("<", Op::Lt),
];

fn lex_filter(text: &str) -> Vec<Lexeme> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '(' || c == ')' {
            out.push(if c == '(' { Lexeme::Open } else { Lexeme::Close });
            i += 1;
            continue;
        }
        if c == '"' || c == '\'' {
            let mut word = String::new();
            i += 1;
            while i < chars.len() && chars[i] != c {
                word.push(chars[i]);
                i += 1;
            }
            i += 1;
            out.push(Lexeme::Word(word));
            continue;
        }
        if let Some((symbol, op)) =
            OPERATORS.iter().find(|(symbol, _)| chars[i..].starts_with(&symbol.chars().collect::<Vec<_>>()[..]))
        {
            out.push(Lexeme::Operator(*op));
            i += symbol.chars().count();
            continue;
        }
        let mut word = String::new();
        while i < chars.len() {
            let c = chars[i];
            if c.is_whitespace()
                || c == '('
                || c == ')'
                || OPERATORS.iter().any(|(symbol, _)| chars[i..].starts_with(&symbol.chars().collect::<Vec<_>>()[..]))
            {
                break;
            }
            word.push(c);
            i += 1;
        }
        out.push(Lexeme::Word(word));
    }
    out
}

struct FilterParser {
    tokens: Vec<Lexeme>,
    pos: usize,
}

impl FilterParser {
    fn peek(&self) -> Option<&Lexeme> {
        self.tokens.get(self.pos)
    }

    fn eat_word(&mut self, word: &str) -> bool {
        if matches!(self.peek(), Some(Lexeme::Word(w)) if w.eq_ignore_ascii_case(word)) {
            self.pos += 1;
            return true;
        }
        false
    }

    fn eat(&mut self, lexeme: &Lexeme) -> bool {
        if self.peek() == Some(lexeme) {
            self.pos += 1;
            return true;
        }
        false
    }

    fn disjunction(&mut self) -> Result<Filter, String> {
        let mut left = self.conjunction()?;
        while self.eat_word("or") || self.eat_word("||") {
            left = Filter::Or(Box::new(left), Box::new(self.conjunction()?));
        }
        Ok(left)
    }

    fn conjunction(&mut self) -> Result<Filter, String> {
        let mut left = self.unary()?;
        while self.eat_word("and") || self.eat_word("&&") {
            left = Filter::And(Box::new(left), Box::new(self.unary()?));
        }
        Ok(left)
    }

    fn unary(&mut self) -> Result<Filter, String> {
        if self.eat_word("not") || self.eat_word("!") {
            return Ok(Filter::Not(Box::new(self.unary()?)));
        }
        if self.eat(&Lexeme::Open) {
            let inner = self.disjunction()?;
            if !self.eat(&Lexeme::Close) {
                return Err("missing `)`".into());
            }
            return Ok(inner);
        }
        let Some(Lexeme::Word(field)) = self.peek().cloned() else {
            return Err("expected a field name".into());
        };
        self.pos += 1;
        let op = match self.peek() {
            Some(Lexeme::Operator(op)) => *op,
            _ => {
                return Err(format!("`{field}` is not a test: expected field=value"));
            }
        };
        self.pos += 1;
        let Some(Lexeme::Word(operand)) = self.peek().cloned() else {
            return Err(format!("`{field}` has no value to compare against"));
        };
        self.pos += 1;
        let regex = matches!(op, Op::Match | Op::NotMatch)
            .then(|| Regex::new(&operand))
            .transpose()
            .map_err(|e| e.to_string())?;
        Ok(Filter::Test { field, op, operand, regex })
    }
}

/// One of the named views `satex query` can print: definitions, expansions,
/// dependencies, the dependency graph, and so on, each backed by its own
/// function in this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Query {
    Definitions,
    Expansions,
    Dependencies,
    Occurrences,
    Recursion,
    Calls,
    DependencyGraph,
    Trace,
    Files,
    Diagnostics,
    Distribution,
    Catcodes,
    SideEffects,
    Project,
    Plugins,
    /// Takes the text to look for, so [`run`] cannot answer it; see
    /// [`produces`].
    Produces,
    /// What this run could not follow, grouped by cause.
    Gaps,
    /// The pgfkeys registered when the run ended; see [`pgfkeys`].
    Pgfkeys,
    /// The keys an environment or command's optional argument accepts;
    /// takes a name, so [`run`] cannot answer it; see [`options`].
    Options,
    /// The text the run computes the document typesets; see [`typeset`].
    Typeset,
    /// The arrow tips and patterns the run declared, drawn by TeX; see
    /// [`crate::queries::preview::sheet`].
    Previews,
    /// The editor's type plugins, their types and features ([`crate::lsp::plugins`]).
    TypePlugins,
}

impl Query {
    /// Sorted by name: `--list`, the wiki and the playground show this order.
    pub const ALL: [(&'static str, Query); 22] = [
        ("calls", Query::Calls),
        ("catcodes", Query::Catcodes),
        ("definitions", Query::Definitions),
        ("dependencies", Query::Dependencies),
        ("dependency-graph", Query::DependencyGraph),
        ("diagnostics", Query::Diagnostics),
        ("distribution", Query::Distribution),
        ("expansions", Query::Expansions),
        ("files", Query::Files),
        ("gaps", Query::Gaps),
        ("occurrences", Query::Occurrences),
        ("options", Query::Options),
        ("pgfkeys", Query::Pgfkeys),
        ("plugins", Query::Plugins),
        ("previews", Query::Previews),
        ("produces", Query::Produces),
        ("project", Query::Project),
        ("recursion", Query::Recursion),
        ("side-effects", Query::SideEffects),
        ("trace", Query::Trace),
        ("type-plugins", Query::TypePlugins),
        ("typeset", Query::Typeset),
    ];

    pub fn parse(name: &str) -> Option<Query> {
        Self::ALL.iter().find(|(n, _)| *n == name).map(|(_, q)| *q)
    }

    pub fn name(self) -> &'static str {
        Self::ALL.iter().find(|(_, q)| *q == self).map(|(n, _)| *n).unwrap_or("query")
    }

    /// What the query answers, in a few words: the one place `--list`, the wiki and the playground read.
    pub fn about(self) -> &'static str {
        match self {
            Query::Definitions => "every macro, environment and register the document or its packages define",
            Query::Expansions => "how often each macro was expanded during the run",
            Query::Dependencies => "files, packages and classes read, and what loaded each",
            Query::Occurrences => "package banners, passed options and other one-off events",
            Query::Recursion => "macros that call themselves, directly or through others",
            Query::Calls => "which control sequences each macro's body invokes",
            Query::DependencyGraph => "the dependency graph between files and packages as records",
            Query::Trace => "every step: macro expansions, primitives run, changes made",
            Query::Files => "every file read, in the order it was opened",
            Query::Diagnostics => "what satex noticed while interpreting, apart from lint rules",
            Query::Distribution => "the detected TeX installation, version and searched trees",
            Query::Catcodes => "explicit \\catcode assignments the document makes",
            Query::SideEffects => "definitions and assignments a macro makes when called",
            Query::Project => "document class, engine and build settings detected or assumed",
            Query::Plugins => "what each plugin chose for this run, and why",
            Query::Produces => "where a given text can come from, expanded or not",
            Query::Gaps => "constructs the run could not interpret, grouped by cause",
            Query::Pgfkeys => "pgfkeys defined at the end of the run and what each does",
            Query::Options => "the keys an environment or command's optional argument accepts",
            Query::Typeset => "the text the document typesets, one row per paragraph",
            Query::Previews => "arrow tips and patterns the run declared, drawn by TeX",
            Query::TypePlugins => "the editor's type plugins: the types each triggers on and its features",
        }
    }
}

/// What the run could not follow, one row per cause with how often it
/// happened and where it first did.  Widening a recursion or analyzing both
/// arms of an undecided conditional is how satex stays sound, so neither is
/// here; what is here are the constructs it could not interpret at all.
fn gaps(analysis: &Analysis) -> Vec<Record> {
    let mut seen: Vec<(&'static str, usize, &crate::analysis::facts::Diagnostic)> = Vec::new();
    for diagnostic in &analysis.facts.diagnostics {
        if diagnostic.severity != crate::analysis::facts::Severity::Unsupported {
            continue;
        }
        match seen.iter_mut().find(|(code, _, _)| *code == diagnostic.code) {
            Some((_, count, _)) => *count += diagnostic.count as usize,
            None => seen.push((diagnostic.code, diagnostic.count as usize, diagnostic)),
        }
    }
    seen.sort_by_key(|(code, count, _)| (std::cmp::Reverse(*count), *code));
    seen.into_iter()
        .map(|(code, count, first)| {
            let mut record = place(analysis, first.span);
            record.insert("code".into(), json!(code));
            record.insert("count".into(), json!(count));
            record.insert("severity".into(), json!(crate::analysis::facts::Severity::Unsupported.as_str()));
            record.insert("message".into(), json!(first.message.clone()));
            record
        })
        .collect()
}

/// What the main file's top-level calls did, at most 8 lines: what a macro
/// inserts and what it defines, the text a primitive sets, and each
/// conditional's outcome. `rows` are trace records, `file` the main file's
/// short name.
pub fn compact_trace(rows: &[Record], file: &str, source: &str) -> Vec<String> {
    fn f<'a>(r: &'a Record, k: &str) -> &'a Json {
        static NULL: Json = Json::Null;
        r.get(k).unwrap_or(&NULL)
    }
    let lines: Vec<&str> = source.lines().collect();
    let ours: Vec<&Record> =
        rows.iter().filter(|r| f(r, "file") == file || f(r, "depth").as_u64() != Some(0)).collect();
    let short = |s: &str, n: usize| {
        if s.chars().count() > n { s.chars().take(n).collect::<String>() + "…" } else { s.to_string() }
    };
    // The call as written: from its column to the first space.
    let written = |r: &Record| {
        let (l, c) = (f(r, "line").as_u64().unwrap_or(0) as usize, f(r, "col").as_u64().unwrap_or(1) as usize);
        let text = lines.get(l.wrapping_sub(1)).and_then(|t| t.get(c.saturating_sub(1)..)).unwrap_or("");
        text.split_whitespace().next().unwrap_or("").trim_end_matches(',').to_string()
    };
    let mut out: Vec<String> = Vec::new();
    for (i, r) in ours.iter().enumerate() {
        if f(r, "file") != file || f(r, "depth").as_u64() != Some(0) {
            continue;
        }
        let name = f(r, "name").as_str().unwrap_or("");
        if matches!(
            name,
            "\\documentclass"
                | "\\makeatletter"
                | "\\makeatother"
                | "\\usepackage"
                | "\\begin"
                | "\\end"
                | "\\document"
        ) {
            continue;
        }
        let call = if f(r, "step") == "expand" { name.to_string() } else { written(r) };
        let at = format!("l.{} {call}", f(r, "line"));
        let then = match (f(r, "step").as_str(), f(r, "detail").as_str()) {
            (Some("condition"), Some("undecided")) => "undecided".to_string(),
            (Some("branch"), Some(arm)) if !out.last().is_some_and(|l| l.starts_with(&at)) => arm.to_string(),
            (Some("expand"), _) if r.get("inserts").is_some() => {
                let mut defines: Vec<String> = ours[i + 1..]
                    .iter()
                    .take_while(|n| f(n, "depth").as_u64() != Some(0))
                    .filter(|n| f(n, "step") == "define")
                    .filter_map(|n| f(n, "name").as_str().map(str::to_string))
                    .collect();
                let inserts = short(f(r, "inserts").as_str().unwrap_or(""), 40);
                let inserts = if inserts.is_empty() { "nothing".to_string() } else { inserts };
                defines.dedup();
                if defines.len() > 3 {
                    let more = format!("… {} more", defines.len() - 3);
                    defines.truncate(3);
                    defines.push(more);
                }
                match defines.is_empty() {
                    true => inserts,
                    false => format!("{inserts} (defines {})", defines.join(" ")),
                }
            }
            _ => match f(r, "sets").as_str() {
                Some(text) => short(text, 30),
                None => continue,
            },
        };
        let entry = format!("{at} ⇒ {then}");
        if out.last() != Some(&entry) {
            out.push(entry);
        }
    }
    if out.len() > 8 {
        let more = out.len() - 7;
        out.truncate(7);
        out.push(format!("… {more} more"));
    }
    out
}

/// The text the document typesets, one record per paragraph, then the
/// named assumptions it rests on. Text set under an undecided conditional
/// reads `{taken|not taken}`, one choice per conditional. Text set in a box
/// reads `⟦…⟧`: it reaches the page only if the box is used. `c…` is a
/// character a loop set any number of times. `·` is a gap where the line
/// breaker may set a hyphen (tex.web § 923). A run cut short ends in a
/// `⟨?⟩` paragraph with `cut` set: `kernel` when the kernel was read only
/// partly (it is then the only paragraph), `run` when the document was. A
/// paragraph no page break may part from what follows (a heading) has `kept`.
/// One whose end some path made and another may not have has `open`.
/// `│` is a thin `\vrule` (a table's `|`), `―` an `\hrule` (`\hline`),
/// `▬` a wider `\vrule` (`\rule`).
/// `keys` lists the labels, references and citations of the user's files
/// with the place in `detail` where each stands. `fonts` lists where the
/// font changes, as [`font_styles`] names it.
fn typeset(analysis: &Analysis) -> Vec<Record> {
    let text = &analysis.facts.text;
    let mut arms = text.arms.iter().peekable();
    let mut rules = text.rules.iter().peekable();
    let marks: Vec<String> = text.align.iter().map(|(_, mark)| mark.name()).collect();
    let mut aligns = text.align.iter().zip(&marks).peekable();
    let mut out = Vec::new();
    let (mut line, mut first, mut choice) = (String::new(), None, None::<Choice>);
    let mut causes: Vec<Cause> = Vec::new();
    let close = |line: &mut String, choice: &mut Option<Choice>, causes: &mut Vec<Cause>| {
        if let Some((on, site, [yes, no])) = choice.take() {
            // what both arms set is no choice
            if yes == no {
                line.push_str(&yes);
                return;
            }
            let _ = write!(line, "{}{yes}{}{no}{}", note("{"), note("|"), note("}"));
            causes.push(Cause::Choice(on, site, format!("{{{}|{}}}", unmarked(&yes), unmarked(&no))));
        }
    };
    let mut on_page = vec![false; text.at.len()];
    for &(from, to) in &text.placed {
        let end = on_page.len();
        on_page[(from as usize).min(end)..(to as usize).min(end)].fill(true);
    }
    // The labels, references and citations of the user's files, each set
    // as a `KEY` mark before the first character read past it.
    let mut named: Vec<(Span, &'static str, &str)> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| matches!(o.kind, OccKind::Label | OccKind::Ref | OccKind::Cite | OccKind::BibItem))
        .filter(|o| origin(analysis, o.span.file) == "document")
        .map(|o| (o.span, o.kind.as_str(), o.key.as_str()))
        .collect();
    named.sort_by_key(|(s, ..)| (s.file, s.line, s.col));
    let styles = font_styles(analysis);
    let mut style = "";
    let mut keys: Keys<'_> = Default::default();
    let mut next_key: std::collections::HashMap<FileId, usize> = Default::default();
    let (glyphs, joined) = glyphs(analysis);
    let math = math_atoms(text, &analysis.settings.conventions.math_symbols);
    let (empty, set): (Vec<Drawn>, Vec<Drawn>) = drawn_boxes(analysis).into_iter().partition(|d| d.4);
    let mut boxes = empty.into_iter().map(|(at, label, sketch, ..)| (at, label, sketch)).peekable();
    let pictures: Vec<(Span, Span, Json)> =
        set.into_iter().map(|(from, _, sketch, to, _)| (from, to, sketch)).collect();
    let mut sketches: Vec<Json> = Vec::new();
    // the environment of the user's text each character was set in (a figure, a listing): a paragraph
    // ends where it changes, and says which one it was set in
    let envs = environments(analysis);
    // looked up once per source line: the characters of a line share it
    let mut by_line: HashMap<(FileId, u32), Option<usize>> = HashMap::default();
    let mut env_at = |s: Span| {
        *by_line.entry((s.file, s.line)).or_insert_with(|| {
            let begun = envs.partition_point(|(f, ..)| (f.file, f.line) <= (s.file, s.line));
            envs[..begun].iter().rposition(|(f, t, _)| f.file == s.file && s.line <= t.line)
        })
    };
    // the innermost environment's name; the outermost one as `outer`: its name, the line it
    // begins on (one per instance, a frame) and the labels set in it
    let tag_env = |record: &mut Record, k: usize| {
        record.insert("env".into(), json!(envs[k].2));
        let within = |s: &Span, (f, t, _): &(Span, Span, String)| {
            s.file == f.file && (f.line, f.col) <= (s.line, s.col) && s.line <= t.line
        };
        let (from, ..) = &envs[k];
        let outer = envs.iter().find(|e| within(from, e)).unwrap_or(&envs[k]);
        let labels: Vec<&str> =
            named.iter().filter(|(s, kind, _)| *kind == "label" && within(s, outer)).map(|n| n.2).collect();
        record.insert("outer".into(), json!({"env": outer.2, "line": outer.0.line, "labels": labels}));
    };
    let mut env: Option<usize> = None;
    let mut foot = String::new();
    let push_foot = |out: &mut Vec<Record>, foot: &mut String, at: Span, keys: &mut Keys<'_>| {
        if foot.trim().is_empty() {
            foot.clear();
            return;
        }
        let mut record = paragraph(analysis, at, &std::mem::take(foot), Vec::new(), keys);
        record.insert("footnote".into(), json!(true));
        out.push(record);
    };
    // the source line the paragraph's text stands on now
    let mut on_line: Option<(FileId, u32)> = None;
    // the source site of the last character set: the next one marks its own unless it follows
    let mut last_site: Option<Span> = None;
    // where the paragraph being set began: its `\parskip` is the one in force there
    let mut began = 0usize;
    let mut flush = |i: usize, line: &mut String, keys: &mut std::collections::VecDeque<_>| {
        while let Some((_, mark)) = aligns.next_if(|a| a.0.0 as usize <= i) {
            line.push(KEY);
            keys.push_back((ALIGN, mark.as_str().into()));
        }
        while let Some(&(_, rule)) = rules.next_if(|r| r.0 as usize <= i) {
            line.push_str(&note(rule.glyph()));
        }
    };
    for (i, (c, spot)) in text.chars.chars().zip(&text.at).enumerate() {
        let site = Span {
            file: text.site.get(i).map_or(spot.span.file, |s| s.file),
            ..text.site.get(i).copied().unwrap_or(spot.span)
        };
        let from = *next_key.entry(site.file).or_insert_with(|| named.partition_point(|(s, ..)| s.file < site.file));
        let upto = from
            + named[from..]
                .iter()
                .take_while(|(s, ..)| s.file == site.file && (s.line, s.col) < (site.line, site.col))
                .count();
        for &(_, kind, key) in &named[from..upto] {
            keys.push_back((kind, key.into()));
            match choice.as_mut() {
                Some((_, _, arms)) => arms[0].push(KEY),
                None => line.push(KEY),
            }
        }
        next_key.insert(site.file, upto);
        // A picture or graphic the document set before this character.
        while let Some((_, label, sketch)) =
            boxes.next_if(|(at, ..)| (at.file, at.line, at.col) < (site.file, site.line, site.col))
        {
            line.push_str(&note(&format!("▢ {label}")));
            sketches.push(sketch);
        }
        let mut taken_by: Vec<bool> = Vec::new();
        let mut arm = None;
        while let Some(a) = arms.next_if(|a| a.0 as usize <= i) {
            if a.0 as usize == i && arm.is_none_or(|(_, on, _)| on == a.1) {
                arm = Some(*a);
                taken_by.push(a.2);
            }
        }
        let boxed = !on_page[i]
            && spot.modes.within(
                crate::engine::mode::Modes::INTERNAL_VERTICAL.union(crate::engine::mode::Modes::RESTRICTED_HORIZONTAL),
            );
        let unknown = crate::engine::tex::is_unknown_char(c);
        let mut c = if unknown { note("⟨?⟩") } else { glyphs[i].clone() };
        if unknown {
            causes.push(Cause::Unknown(site));
        }
        if text.repeats.binary_search(&(i as u32)).is_ok() {
            c = format!("{c}{}", note("…"));
        }
        if text.hyphens.binary_search(&(i as u32)).is_ok() {
            c = format!("{}{c}", note("·"));
        }
        if text.math.binary_search_by_key(&(i as u32), |m| m.0).is_ok() && c_is_space(&c) {
            c.clear();
        } else if let Some(&(tag, glyph)) = math.get(&i).filter(|_| !unknown) {
            c = format!(
                "{}{}{}",
                note(&format!("{MATH}{tag}")),
                glyph.map_or(c, String::from),
                note(&MATH.to_string())
            );
        }
        let piece = if boxed { format!("{}{c}{}", note("⟦"), note("⟧")) } else { c };
        // an insert's text (a footnote) goes to the foot, after the paragraph that set it
        if text.inserts.iter().any(|&(a, b)| (a..b).contains(&(i as u32))) {
            foot.push_str(&piece);
            continue;
        }
        let here = env_at(site);
        // the rules and marks closing an environment belong to it
        if env.is_some() && here != env {
            flush(i, &mut line, &mut keys);
        }
        if (text.pars.binary_search(&(i as u32)).is_ok() || here != env) && !line.trim().is_empty() {
            close(&mut line, &mut choice, &mut causes);
            let mut record = paragraph(
                analysis,
                first.take().unwrap_or(site),
                &std::mem::take(&mut line),
                std::mem::take(&mut causes),
                &mut keys,
            );
            if text.kept.binary_search(&(i as u32)).is_ok() {
                record.insert("kept".into(), json!(true));
            }
            if text.open.binary_search(&(i as u32)).is_ok() {
                record.insert("open".into(), json!(true));
            }
            if let Ok(k) = text.just.binary_search_by_key(&(began as u32), |s| s.0) {
                record.insert("just".into(), json!(text.just[k].1.name()));
            }
            if let Ok(k) = text.skips.binary_search_by_key(&(began as u32), |s| s.0) {
                record.insert("skip".into(), json!(text.skips[k].1 as f64 / f64::from(1 << 16)));
            }
            attach_sketches(&mut record, &mut sketches);
            if let Some(k) = env {
                tag_env(&mut record, k);
                attach_picture(analysis, &mut record, &pictures);
            }
            out.push(record);
            push_foot(&mut out, &mut foot, site, &mut keys);
            style = "";
            on_line = None;
        }
        if text.pars.binary_search(&(i as u32)).is_ok() {
            began = i;
        }
        env = here;
        if !piece.trim().is_empty() {
            first.get_or_insert(site);
        }
        flush(i, &mut line, &mut keys);
        match arm {
            Some((_, on, _)) => {
                if choice.as_ref().is_some_and(|(n, ..)| *n != on) {
                    close(&mut line, &mut choice, &mut causes);
                }
                let arms = &mut choice.get_or_insert_with(|| (on, site, Default::default())).2;
                for taken in taken_by {
                    arms[usize::from(!taken)].push_str(&piece);
                }
            }
            None => {
                close(&mut line, &mut choice, &mut causes);
                if !piece.trim().is_empty() {
                    let here = (site.file, site.line);
                    if on_line.is_some_and(|at| at != here) {
                        line.push(KEY);
                        keys.push_back((LINE, site.line.to_string().into()));
                    }
                    on_line = Some(here);
                    // a piece of several characters (a note, a math atom) shares its site
                    let single = unmarked(&piece).chars().count() == 1;
                    let follows =
                        last_site.is_some_and(|l| (l.file, l.line, l.col + 1) == (site.file, site.line, site.col));
                    if !(single && follows) {
                        line.push(KEY);
                        let file = analysis.file_name(site.file);
                        let step = if single { "" } else { ":0" };
                        // the site is where reading stood after the character: one column back is its own
                        // (a macro's text: the last letter of its name)
                        let col = site.col.saturating_sub(1).max(1);
                        keys.push_back((SITE, format!("{file}\u{1}{}:{col}{step}", site.line).into()));
                    }
                    last_site = single.then_some(site);
                }
                // a font set under an undecided conditional shows from the next sure character
                let now = styles.get(usize::from(spot.font_state)).map_or("", String::as_str);
                if now != style {
                    style = now;
                    line.push(KEY);
                    keys.push_back((FONT, now.into()));
                }
                if let Some(lig) = &joined[i] {
                    line.push(KEY);
                    keys.push_back((LIG, lig.as_str().into()));
                }
                line.push_str(&piece);
            }
        }
    }
    close(&mut line, &mut choice, &mut causes);
    for (_, label, sketch) in boxes {
        line.push_str(&note(&format!("▢ {label}")));
        sketches.push(sketch);
    }
    for (_, mark) in aligns {
        line.push(KEY);
        keys.push_back((ALIGN, mark.as_str().into()));
    }
    for &(_, rule) in rules {
        line.push_str(&note(rule.glyph()));
    }
    if !line.trim().is_empty() {
        let mut record = paragraph(analysis, first.unwrap_or_default(), &line, causes, &mut keys);
        if let Ok(k) = text.just.binary_search_by_key(&(began as u32), |s| s.0) {
            record.insert("just".into(), json!(text.just[k].1.name()));
        }
        if let Ok(k) = text.skips.binary_search_by_key(&(began as u32), |s| s.0) {
            record.insert("skip".into(), json!(text.skips[k].1 as f64 / f64::from(1 << 16)));
        }
        attach_sketches(&mut record, &mut sketches);
        if let Some(k) = env {
            tag_env(&mut record, k);
            attach_picture(analysis, &mut record, &pictures);
        }
        out.push(record);
    }
    push_foot(&mut out, &mut foot, first.unwrap_or_default(), &mut keys);
    // A kernel read only partly leaves the document names the engine's
    // format defines (`\documentclass` set as text), so none of the text is
    // sure; a run that stopped early never set the rest.
    let kernel_cut = analysis
        .facts
        .diagnostics
        .iter()
        .any(|d| matches!(d.code, "format-truncated" | "format-incomplete" | "preload-truncated"));
    if kernel_cut {
        out.clear();
    }
    if kernel_cut || analysis.exhausted {
        let mut rest = paragraph(
            analysis,
            Span::default(),
            &note("⟨?⟩"),
            vec![Cause::Unknown(Span::default())],
            &mut Default::default(),
        );
        rest.insert("cut".into(), json!(if kernel_cut { "kernel" } else { "run" }));
        out.push(rest);
    }
    let mut named: Vec<&str> = Vec::new();
    for d in analysis.facts.diagnostics.iter().filter(|d| d.severity == crate::analysis::facts::Severity::Assumption) {
        if !named.contains(&d.code) {
            named.push(d.code);
            let mut record = place(analysis, d.span);
            record.insert("kind".into(), json!("assumption"));
            record.insert("origin".into(), json!(origin(analysis, d.span.file)));
            record.insert("detail".into(), json!(d.code));
            out.push(record);
        }
    }
    out
}

/// How many characters of what a call typesets a hover shows.
const PREVIEW: usize = 100;

/// What the call at `at` typesets, as Markdown: the characters set while the
/// user's file was read at the one place past the call, in a code span; what
/// an undecided conditional sets either way as its alternatives, `a` / `b`;
/// at most [`PREVIEW`] characters, then `…`.
pub fn typeset_by(analysis: &Analysis, at: Span) -> Option<String> {
    let text = &analysis.facts.text;
    let pos = |s: &Span| (s.line, s.col);
    // the next call the file itself makes, not one inside this call's expansion: the reader stands at most there
    let next = analysis
        .facts
        .expansions
        .iter()
        .filter(|e| e.within.is_none() && e.span.file == at.file && pos(&e.span) > pos(&at))
        .map(|e| pos(&e.span))
        .min();
    let mut on_page = vec![false; text.at.len()];
    for &(from, to) in &text.placed {
        let end = on_page.len();
        on_page[(from as usize).min(end)..(to as usize).min(end)].fill(true);
    }
    // a box measured and never placed (`\settowidth`) is not what the reader sees
    let boxed = |i: usize| {
        !on_page[i]
            && text.at[i].modes.within(
                crate::engine::mode::Modes::INTERNAL_VERTICAL.union(crate::engine::mode::Modes::RESTRICTED_HORIZONTAL),
            )
    };
    // what it sets: the text this file's reader set right after it, and all read from other files (a `.toc`,
    // an `\input`) on the way
    let here = |i: usize| text.site[i].file == at.file;
    let n = text.site.len().min(text.at.len());
    let start = (0..n).rev().find(|&i| here(i) && pos(&text.site[i]) <= pos(&at)).map_or(0, |i| i + 1);
    let own = (start..n).find(|&i| here(i) && !boxed(i)).map(|i| pos(&text.site[i]))?;
    if next.is_some_and(|n| own > n) {
        return None;
    }
    let end = (start..n).find(|&i| here(i) && pos(&text.site[i]) > own).unwrap_or(n);
    // runs of plain text and of alternatives, each alternative's arms by the arm taken
    let mut runs: Vec<(Option<crate::engine::env::NodeId>, [String; 2])> = Vec::new();
    let mut shown = 0;
    for (i, c) in text.chars.chars().enumerate().take(end).skip(start) {
        if shown == PREVIEW {
            break;
        }
        if boxed(i) || here(i) && pos(&text.site[i]) != own {
            continue;
        }
        let c = if crate::engine::tex::is_unknown_char(c) { "⟨?⟩".to_string() } else { public(c) };
        // a new paragraph (each line of a table of contents) reads apart from the last
        let c = if i > start && text.pars.binary_search(&(i as u32)).is_ok() { format!(" {c}") } else { c };
        shown += c.chars().count();
        let arm = text.arms.binary_search_by_key(&(i as u32), |a| a.0).ok().map(|k| text.arms[k]);
        let on = arm.map(|(_, on, _)| on);
        if runs.last().is_none_or(|(o, _)| *o != on) {
            runs.push((on, Default::default()));
        }
        let last = runs.last_mut().expect("pushed");
        last.1[usize::from(arm.is_some_and(|(_, _, taken)| !taken))].push_str(&c);
    }
    let code = |s: &str| format!("`{}`", s.trim().replace('`', "ˋ"));
    let mut out: String = runs
        .iter()
        .filter(|(_, [a, b])| !a.trim().is_empty() || !b.trim().is_empty())
        .map(|(on, [a, b])| match on {
            None => code(a),
            Some(_) => format!("{} / {}", code(a), code(b)),
        })
        .collect::<Vec<_>>()
        .join(" ");
    if shown == PREVIEW {
        out.push_str(" …");
    }
    (!out.is_empty()).then_some(out)
}

/// A choice being set: where paths parted, the first user site it is set at, and each arm's text.
type Choice = (crate::engine::env::NodeId, Span, [String; 2]);

/// What made a character of the typeset text uncertain.
enum Cause {
    /// A `{a|b}` choice set where paths parted at this node (an undecided
    /// conditional or a split), first set at this place in the user's files.
    Choice(crate::engine::env::NodeId, Span, String),
    /// A character the run could not compute, set here.
    Unknown(Span),
}

/// A picture or graphic: where it begins, its label, its sketch (or `null`), where it ends, and
/// whether the run set no character in it.
type Drawn = (Span, String, Json, Span, bool);

/// What the document's own files set that holds no character the run saw:
/// an environment with none between its `\begin` and `\end` (a
/// `tikzpicture`), a graphic. Each where it begins, in reading order.
fn drawn_boxes(analysis: &Analysis) -> Vec<Drawn> {
    let text = &analysis.facts.text;
    let mut sites: Vec<(FileId, u32, u32)> = text.site.iter().map(|s| (s.file, s.line, s.col)).collect();
    sites.sort_unstable();
    let key = |s: Span| (s.file, s.line, s.col);
    let empty = |from: Span, to: Span| {
        let i = sites.partition_point(|s| *s < key(from));
        sites.get(i).is_none_or(|s| *s > key(to))
    };
    let mut out = Vec::new();
    let mut open: Vec<(&str, Span)> = Vec::new();
    let mut written = written_by(analysis);
    for o in analysis.facts.occurrences.iter().filter(|o| origin(analysis, o.span.file) == "document") {
        match o.kind {
            OccKind::BeginEnvironment => open.push((o.key.as_str(), o.span)),
            OccKind::EndEnvironment => {
                if let Some(k) = open.iter().rposition(|(name, _)| *name == o.key) {
                    let (name, from) = open.remove(k);
                    // What the picture drew, read from the literals the run wrote.
                    let sketch = match name == "tikzpicture" && from.file == o.span.file {
                        true => crate::queries::picture::draw(analysis, from, o.span).unwrap_or(Json::Null),
                        false => Json::Null,
                    };
                    // Text an environment reads back at its end (a frame) is placed after `\end{…}`.
                    let end = Span { col: o.span.col + format!("\\end{{{name}}}").len() as u32, ..o.span };
                    if from.file == o.span.file && written(name, from) && (empty(from, end) || !sketch.is_null()) {
                        // An empty one holding others (a figure of one picture) stands for them.
                        out.retain(|d: &Drawn| !(key(d.0) > key(from) && key(d.3) < key(o.span)));
                        out.push((from, name.to_string(), sketch, o.span, empty(from, o.span)));
                    }
                }
            }
            OccKind::Graphics => out.push((o.span, o.key.clone(), Json::Null, o.span, true)),
            _ => {}
        }
    }
    out.sort_by_key(|d| key(d.0));
    out.dedup_by(|a, b| a.0 == b.0 && a.1 == b.1);
    out
}

/// The sketches of the pictures a paragraph's `▢` notes stand for, in order, `null` for one without.
fn attach_sketches(record: &mut Record, sketches: &mut Vec<Json>) {
    let drawn = std::mem::take(sketches);
    if drawn.iter().any(|s| !s.is_null()) {
        record.insert("sketches".into(), Json::Array(drawn));
    }
}

/// The `sketch` of the picture a paragraph set inside, from the pictures with text.
fn attach_picture(analysis: &Analysis, record: &mut Record, pictures: &[(Span, Span, Json)]) {
    let at = |key: &str| record.get(key).and_then(Json::as_u64).unwrap_or(0) as u32;
    let (line, col) = (at("line"), at("col"));
    let path = record.get("path").and_then(Json::as_str).unwrap_or("");
    let inside = |(from, to, _): &&(Span, Span, Json)| {
        analysis.file_name(from.file) == path
            && (from.line, from.col) <= (line, col)
            && (line, col) <= (to.line, to.col)
    };
    if let Some((.., sketch)) = pictures.iter().find(inside) {
        record.insert("sketch".into(), sketch.clone());
    }
}

/// Whether the source begins environment `name` itself at `at`: beamer's own
/// (a `pgfpicture` run inside `\begin{frame}`) are reported at the frame's place.
fn written_by(analysis: &Analysis) -> impl FnMut(&str, Span) -> bool + '_ {
    let mut texts: HashMap<FileId, Option<String>> = HashMap::default();
    move |name: &str, at: Span| {
        let text = texts.entry(at.file).or_insert_with(|| std::fs::read_to_string(analysis.file_name(at.file)).ok());
        // a text read from no file (an unsaved buffer) cannot tell: kept
        let Some(text) = text.as_deref() else { return true };
        let line = text.lines().nth((at.line as usize).saturating_sub(1)).unwrap_or_default();
        let rest: String = line.chars().skip((at.col as usize).saturating_sub(1)).collect();
        rest.strip_prefix("\\begin").is_some_and(|r| r.trim_start().starts_with(&format!("{{{name}}}")))
    }
}

/// The environments the user's files begin themselves, `document` aside, as the places they begin and end.
pub(crate) fn environments(analysis: &Analysis) -> Vec<(Span, Span, String)> {
    let mut open: Vec<(&str, Span)> = Vec::new();
    let mut out = Vec::new();
    let mut written = written_by(analysis);
    for o in analysis.facts.occurrences.iter().filter(|o| origin(analysis, o.span.file) == "document") {
        match o.kind {
            OccKind::BeginEnvironment if o.key != "document" && written(&o.key, o.span) => {
                open.push((o.key.as_str(), o.span))
            }
            OccKind::EndEnvironment => {
                if let Some(k) = open.iter().rposition(|(name, _)| *name == o.key) {
                    let (name, from) = open.remove(k);
                    out.push((from, o.span, name.to_string()));
                }
            }
            _ => {}
        }
    }
    out.sort_by_key(|(f, ..)| (f.file, f.line, f.col));
    out
}

/// Each character as set: one a font's ligature program joins to the one
/// before (tex.web § 1035) is empty, and that one shows the ligature, as
/// TeX's text conventions write it in Unicode (`--` an en dash). Second, at
/// each ligature's first character, what the font joined and the glyph set.
fn glyphs(analysis: &Analysis) -> (Vec<String>, Vec<Option<String>>) {
    let text = &analysis.facts.text;
    let mut fonts = crate::linter::italics::Fonts::of(analysis);
    let slots = encoding_slots(analysis);
    let shown = |c: char, spot: &crate::analysis::facts::Spot| {
        let encoding = analysis.font_states.get(usize::from(spot.font_state)).map(|s| &*s.encoding);
        encoding.and_then(|e| slots.get(e)?.get(&(c as u32)).copied()).unwrap_or(c)
    };
    let mut out: Vec<String> = text.chars.chars().zip(&text.at).map(|(c, spot)| public(shown(c, spot))).collect();
    let mut seq: Vec<String> = out.clone();
    let mut held: Option<(usize, u32)> = None;
    for (i, (c, spot)) in text.chars.chars().zip(&text.at).enumerate() {
        let state = analysis.font_states.get(usize::from(spot.font_state)).map_or(0, |s| s.font);
        let joined = held.filter(|&(j, _)| {
            let prev = &text.at[j];
            prev.font_state == spot.font_state && prev.certain && spot.certain && !spot.broken
        });
        if let Some((j, left)) = joined
            && let Some((metrics, size)) = fonts.metrics(state)
            && let Some(crate::engine::tfm::LigKern::Lig { op: 0, char }) = metrics.lig_kern(left, c as u32, size)
        {
            seq[j].push(c);
            out[j] = match seq[j].as_str() {
                "--" => "–",
                "---" => "—",
                "``" => "“",
                "''" => "”",
                "!`" => "¡",
                "?`" => "¿",
                ",," => "„",
                "<<" => "«",
                ">>" => "»",
                s => s,
            }
            .to_string();
            out[i].clear();
            held = Some((j, char));
            continue;
        }
        held = Some((i, c as u32));
    }
    // each ligature: the characters the font joined and the glyph it set, at its first character
    let joined = seq.iter().zip(&out).map(|(s, o)| (s.chars().count() > 1).then(|| format!("{s}\u{1}{o}"))).collect();
    (out, joined)
}

/// The names allocated to each register, by the register as [`step_name`]
/// writes it (`\count188`): a `\countdef`-style name's own register, and
/// the box of a `\chardef` constant made by an allocation macro, the macro
/// that made such names (`\newbox` gives `\@tempboxa` its number so).
fn register_aliases(analysis: &Analysis) -> HashMap<String, Vec<String>> {
    use crate::engine::tex::RegKind;
    let meaning = |sym| analysis.env.meaning(sym);
    let register = |m: &Meaning| matches!(m, Meaning::Register(k, _) if !matches!(k, RegKind::Char | RegKind::MathChar | RegKind::UMathChar));
    let allocators: BTreeSet<Sym> =
        analysis.facts.defs.iter().filter(|d| register(&meaning(d.name))).filter_map(|d| d.via).collect();
    let boxes: BTreeSet<Sym> =
        analysis.facts.defs.iter().filter(|d| d.via.is_some_and(|v| allocators.contains(&v))).map(|d| d.name).collect();
    let mut out: HashMap<String, Vec<String>> = HashMap::default();
    for i in 0..analysis.interner.len() as u32 {
        let sym = Sym(i);
        let key = match meaning(sym) {
            Meaning::Register(RegKind::Char, n) if boxes.contains(&sym) => format!("\\{}{n}", RegKind::Box.as_str()),
            m @ Meaning::Register(kind, n) if register(&m) => format!("\\{}{n}", kind.as_str()),
            _ => continue,
        };
        out.entry(key).or_default().push(analysis.interner.cs(sym));
    }
    out
}

/// A trace step's name: a register addressed by number as TeX writes it
/// (`\count188`), not by the slot satex keeps its value in.
fn step_name(analysis: &Analysis, sym: crate::engine::tex::Sym) -> String {
    let name = analysis.interner.name(sym);
    match name.strip_prefix(crate::analysis::machine::REGISTER_PREFIX) {
        Some(rest)
            if rest.trim_end_matches(|c: char| c.is_ascii_digit()).chars().all(|c| c.is_ascii_lowercase())
                && rest.ends_with(|c: char| c.is_ascii_digit()) =>
        {
            format!("\\{rest}")
        }
        _ => analysis.interner.cs(sym),
    }
}

/// Per font encoding of the run (`OMS`), the character each slot it
/// declares a text symbol at stands for: `\DeclareTextSymbol` in the
/// encoding's `.def` file names the command, `tuenc.def` its code point.
/// `\{` sets slot 102 of an OMS font, which is `{`, not `f`.
fn encoding_slots(analysis: &Analysis) -> HashMap<String, HashMap<u32, char>> {
    let roots = &analysis.distribution.roots;
    let read =
        |name: &str| crate::caches::distribution::tex_file(roots, name).and_then(|p| std::fs::read_to_string(p).ok());
    let mut out = HashMap::default();
    let encodings: std::collections::BTreeSet<&str> =
        // OT1 has no one layout: cmtt sets `\` and braces where cmr sets quotes and dashes (fntguide § 4.2)
        analysis.font_states.iter().map(|s| &*s.encoding).filter(|e| !e.is_empty() && !matches!(*e, "TU" | "OT1")).collect();
    if encodings.is_empty() {
        return out;
    }
    let Some(unicode) = read("tuenc.def") else { return out };
    let code = regex::Regex::new(r#"\\DeclareUnicodeSymbol\{\\(\w+)\}\s*\{"([0-9A-Fa-f]+)\}"#).expect("tuenc regex");
    let points: HashMap<&str, char> = code
        .captures_iter(&unicode)
        .filter_map(|c| Some((c.get(1)?.as_str(), char::from_u32(u32::from_str_radix(&c[2], 16).ok()?)?)))
        .collect();
    for encoding in encodings {
        let Some(def) = read(&format!("{}enc.def", encoding.to_lowercase())) else { continue };
        let symbol = regex::Regex::new(&format!(
            r#"\\DeclareTextSymbol\{{\\(\w+)\}}\{{{}\}}\{{(\d+|"[0-9A-Fa-f]+|'[0-7]+)\}}"#,
            regex::escape(encoding)
        ))
        .expect("symbol regex");
        let slot = |n: &str| match n.as_bytes()[0] {
            b'"' => u32::from_str_radix(&n[1..], 16).ok(),
            b'\'' => u32::from_str_radix(&n[1..], 8).ok(),
            _ => n.parse().ok(),
        };
        let table: HashMap<u32, char> =
            symbol.captures_iter(&def).filter_map(|c| Some((slot(&c[2])?, *points.get(&c[1])?))).collect();
        out.insert(encoding.to_string(), table);
    }
    out
}

/// A character as typeset text: satex's private-use sentinels vanish.
fn public(c: char) -> String {
    match c {
        '\u{10FF00}'..='\u{10FFFF}' | '\u{E000}'..='\u{F8FF}' => String::new(),
        _ => c.to_string(),
    }
}

fn strip_private(s: &str) -> String {
    s.chars().map(public).collect()
}

/// `s` cut to about 60 characters.
fn clip(s: &str) -> String {
    if s.chars().count() <= 60 { s.to_string() } else { format!("{}…", s.chars().take(60).collect::<String>()) }
}

const NOTE: (char, char) = (NOTE_OPEN, NOTE_CLOSE);
const NOTE_OPEN: char = '\u{E000}';
const NOTE_CLOSE: char = '\u{E001}';

/// Where a key [`typeset`] lists stands in the text, removed by [`paragraph`].
const KEY: char = '\u{E002}';

/// The kind of a [`KEY`] that marks a font change, not a key.
const FONT: &str = "font";

/// The kind of a [`KEY`] that marks where the text moves on to another source line: the number.
const LINE: &str = "line";

/// The kind of a [`KEY`] before a character that does not follow the last one in the source:
/// its file, `\u{1}`, line and column. The characters after it count on from that column,
/// or with `:0` after the column (a piece of several characters) all stand there.
const SITE: &str = "site";

/// The marks waiting for their [`KEY`] in a paragraph's text: kind and value.
type Keys<'a> = std::collections::VecDeque<(&'a str, std::borrow::Cow<'a, str>)>;

/// The kind of a [`KEY`] that marks an alignment's part, not a key.
const ALIGN: &str = "align";

/// The kind of a [`KEY`] before a ligature: what the font joined, `\u{1}`, the glyph set.
const LIG: &str = "lig";

/// Each row of [`Analysis::font_states`] as its family, series and shape
/// among the kernel's defaults (`\sfdefault`, `\ttdefault`, `\bfdefault`,
/// `\itdefault`, `\sldefault`, `\scdefault`, fntguide § 2.5): `sf`, `tt`,
/// `bf`, `it`, `sl`, `sc`, space-separated; empty for the defaults or an
/// unknown font.
fn font_styles(analysis: &Analysis) -> Vec<String> {
    // A default's text up to the kernel's `\@empty` end marker (`b\@empty`).
    let axis = |names: &[&'static str]| -> Vec<(&'static str, String)> {
        let text = |n| Some(crate::linter::shared::macro_text(analysis, n)?.split('\\').next()?.trim().to_string());
        names.iter().filter_map(|n| Some((&n[..2], text(n)?))).collect()
    };
    // `\bfseries` sets the family's own bold series (`\bfseries@rm`, `bx`) when the kernel has one.
    let (family, series, shape) = (
        axis(&["sfdefault", "ttdefault"]),
        axis(&["bfdefault", "bfseries@rm", "bfseries@sf", "bfseries@tt"]),
        axis(&["itdefault", "sldefault", "scdefault"]),
    );
    let pick = |axis: &[(&'static str, String)], value: &str| axis.iter().find(|(_, d)| d == value).map(|(k, _)| *k);
    // a series is the default's when its weight (the first letter) is: `\fontseries{b}` is as bold as `bx`
    let weight = |value: &str| series.iter().find(|(_, d)| d.chars().next() == value.chars().next()).map(|(k, _)| *k);
    analysis
        .font_states
        .iter()
        .map(|s| {
            let style = [
                pick(&family, &s.family),
                pick(&series, &s.series).or_else(|| weight(&s.series)),
                pick(&shape, &s.shape),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
            match css_color(&s.color) {
                Some(color) => format!("{style}\u{1}{color}"),
                None => style,
            }
        })
        .collect()
}

/// The color a driver's `\current@color` sets as CSS `#rrggbb`, from the
/// numbers it carries: `0 1 0 rg 0 1 0 RG` (pdfTeX), `color push rgb 0 1 0`
/// (dvips), `pdf: bc [0 1 0]` (dvipdfmx). One number is a gray, three an
/// RGB color, four CMYK. `None` for black or no color.
fn css_color(spec: &str) -> Option<String> {
    let number = |t: &str| t.trim_matches(['[', ']']).parse::<f64>().ok();
    let words: Vec<&str> = spec.split_whitespace().collect();
    let start = words.iter().position(|t| number(t).is_some())?;
    let run: Vec<f64> = words[start..].iter().map_while(|t| number(t)).collect();
    let rgb = match run[..] {
        [g] => [g; 3],
        [r, g, b] => [r, g, b],
        [c, m, y, k] => [(1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k)],
        _ => return None,
    };
    (rgb != [0.0; 3]).then(|| {
        let [r, g, b] = rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
        format!("#{r:02x}{g:02x}{b:02x}")
    })
}

fn c_is_space(c: &str) -> bool {
    c == " "
}

/// The kind of a note that marks math: the class of the atom set next, `i` for
/// an italic letter, then an empty note closes it.
const MATH: char = '\u{6}';

/// The slot of the symbol family (OMS, family 2) a math code names, as the character it draws.
/// Math classes of an atom (tex.web § 1151).
const BINARY: u16 = 2;
const RELATION: u16 = 3;
const OPENING: u16 = 4;
const CLOSING: u16 = 5;
const PUNCTUATION: u16 = 6;
/// The class of a math code that takes its family from `\fam` (tex.web § 1151) acts as ordinary.
const VARIABLE: u16 = 7;

/// How each character set in math stands in its formula: the class of its atom and the glyph its
/// math code draws, when that is not the character. A binary operator at the start of a formula
/// or after another operator, relation, opening or punctuation, or before a relation, closing or
/// punctuation, is ordinary (tex.web § 728). A letter of the math italic family is tagged `i`.
fn math_atoms(
    text: &crate::analysis::facts::Text,
    symbols: &std::collections::BTreeMap<u32, char>,
) -> HashMap<usize, (char, Option<char>)> {
    let mut out = HashMap::default();
    let mut formulas: Vec<Vec<(usize, u16)>> = Vec::new();
    let chars: Vec<char> = text.chars.chars().collect();
    for &(at, code) in &text.math {
        let at = at as usize;
        match formulas.last_mut() {
            Some(f) if f.last().is_some_and(|&(p, _)| p + 1 == at) => f.push((at, code)),
            _ => formulas.push(vec![(at, code)]),
        }
    }
    for formula in formulas {
        // math ignores spaces (tex.web § 1045): they are no atoms
        let formula: Vec<(usize, u16)> = formula.into_iter().filter(|&(at, _)| chars[at] != ' ').collect();
        let classes: Vec<u16> = formula.iter().map(|&(_, c)| (c >> 12).min(VARIABLE)).collect();
        let mut before: Option<u16> = None;
        for (k, &(at, code)) in formula.iter().enumerate() {
            let mut class = if classes[k] == VARIABLE { 0 } else { classes[k] };
            let starts = before.is_none_or(|b| matches!(b, BINARY | RELATION | OPENING | PUNCTUATION));
            let ends = classes.get(k + 1).is_none_or(|n| matches!(*n, RELATION | CLOSING | PUNCTUATION));
            if class == BINARY && (starts || ends) {
                class = 0;
            }
            before = Some(class);
            let family = (code >> 8) & 0xf;
            let glyph = (family == 2).then(|| symbols.get(&u32::from(code & 0xff)).copied()).flatten();
            let tag = if class == 0 && family == 1 { 'i' } else { char::from(b'0' + class as u8) };
            out.insert(at, (tag, glyph));
        }
    }
    out
}

/// `s` as satex's own notation, not a typeset character.
fn note(s: &str) -> String {
    format!("{}{s}{}", NOTE.0, NOTE.1)
}

fn unmarked(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            // a math note holds no text of its own
            NOTE_OPEN if chars.peek() == Some(&MATH) => chars.by_ref().take_while(|&c| c != NOTE_CLOSE).for_each(drop),
            NOTE_OPEN | NOTE_CLOSE => {}
            c => out.push(c),
        }
    }
    out
}

/// A paragraph record: `detail` is plain, `marked` wraps the notation in
/// private-use characters, `causes` says why each choice or unknown is there.
fn paragraph(analysis: &Analysis, at: Span, marked: &str, causes: Vec<Cause>, keys: &mut Keys<'_>) -> Record {
    // marks (keys, an alignment's or a box's end) at the ends hold no space before or after them
    let ends = |chars: &mut dyn Iterator<Item = char>| {
        chars.take_while(|&c| c == KEY || c.is_whitespace()).filter(|&c| c == KEY).count()
    };
    let core = marked.trim_matches(|c: char| c == KEY || c.is_whitespace());
    let (head, tail) = (ends(&mut marked.chars()), if core.is_empty() { 0 } else { ends(&mut marked.chars().rev()) });
    let marked = tidy(&format!("{}{core}{}", KEY.to_string().repeat(head), KEY.to_string().repeat(tail)));
    let (mut placed, mut fonts, mut aligned, mut ligatures, mut lines, mut sites, mut offset) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), 0);
    let at_file = at.file;
    for c in unmarked(&marked).chars() {
        if c != KEY {
            offset += 1;
        } else if let Some((kind, key)) = keys.pop_front() {
            if kind == FONT {
                let (style, color) = key.split_once('\u{1}').unwrap_or((&key, ""));
                let mut font = json!({ "style": style, "at": offset });
                if !color.is_empty() {
                    font["color"] = json!(color);
                }
                fonts.push(font);
            } else if kind == SITE {
                let (file, at) = key.split_once('\u{1}').unwrap_or_default();
                let mut parts = at.split(':').map(|n| n.parse::<u32>().unwrap_or(0));
                let (line, col) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
                let mut site = json!({ "line": line, "col": col, "at": offset });
                if parts.next() == Some(0) {
                    site["same"] = json!(true);
                }
                if file != analysis.file_name(at_file) {
                    site["path"] = json!(file);
                }
                sites.push(site);
            } else if kind == LINE {
                lines.push(json!({ "line": key.parse::<u32>().unwrap_or(0), "at": offset }));
            } else if kind == ALIGN {
                aligned.push(json!({ "mark": key, "at": offset }));
            } else if kind == LIG {
                let (from, to) = key.split_once('\u{1}').unwrap_or((&key, &key));
                ligatures.push(json!({ "from": from, "to": to, "at": offset }));
            } else {
                placed.push(json!({ "kind": kind, "key": key, "at": offset }));
            }
        }
    }
    let marked: String = marked.chars().filter(|&c| c != KEY).collect();
    let mut record = place(analysis, at);
    if !placed.is_empty() {
        record.insert("keys".into(), json!(placed));
    }
    if !lines.is_empty() {
        record.insert("lines".into(), json!(lines));
    }
    if !sites.is_empty() {
        record.insert("sites".into(), json!(sites));
    }
    if fonts.iter().any(|f| f["style"] != "" || f.get("color").is_some()) {
        record.insert("fonts".into(), json!(fonts));
    }
    if !aligned.is_empty() {
        record.insert("align".into(), json!(aligned));
    }
    if !ligatures.is_empty() {
        record.insert("ligatures".into(), json!(ligatures));
    }
    record.insert("kind".into(), json!("paragraph"));
    record.insert("detail".into(), json!(unmarked(&marked)));
    record.insert("marked".into(), json!(marked));
    let mut seen: Vec<(String, u32)> = Vec::new();
    let mut why = Vec::new();
    for cause in causes {
        let (mark, line, test, reads, parted) = match cause {
            Cause::Choice(on, set, mark) => {
                let vertex = analysis.graph.vertices.get(on as usize);
                let site = vertex.map(|v| v.span).unwrap_or_default();
                let (test, reads) = test_text(analysis, site);
                // A test in library code is named by what it tested, at the user's line that led there.
                match origin(analysis, site.file) == "document" && !test.is_empty() {
                    true => (mark, site.line, test, reads, String::new()),
                    false => {
                        // a name the run made up (an unknown value's) says less than its place
                        let name = vertex.map(|v| analysis.interner.name(v.name)).unwrap_or_default();
                        let named = !name.is_empty() && name.chars().all(|c| c.is_ascii_alphabetic() || c == '@');
                        let file = analysis.file_name(site.file);
                        let file = Path::new(file).file_name().map_or(file.into(), |f| f.to_string_lossy());
                        let parted = if named { format!("`\\{name}`") } else { format!("{file}:{}", site.line) };
                        (mark, set.line, String::new(), Vec::new(), parted)
                    }
                }
            }
            Cause::Unknown(site) => ("⟨?⟩".to_string(), site.line, String::new(), Vec::new(), String::new()),
        };
        if !seen.contains(&(mark.clone(), line)) {
            seen.push((mark.clone(), line));
            let mut cause = json!({"mark": clip(&strip_private(&mark)), "line": line, "test": clip(&strip_private(&test)), "reads": reads});
            if !parted.is_empty() {
                cause["parted"] = json!(strip_private(&parted));
            }
            why.push(cause);
        }
    }
    if !why.is_empty() {
        record.insert("causes".into(), json!(why));
    }
    record
}

/// The source of the undecided test of the conditional at `site`, and the
/// control sequences it reads.
fn test_text(analysis: &Analysis, site: Span) -> (String, Vec<String>) {
    let Some(cond) = analysis.facts.conditionals.iter().find(|c| c.at == site) else { return Default::default() };
    let Ok(source) = std::fs::read_to_string(analysis.file_name(site.file)) else { return Default::default() };
    let Some(row) = source.lines().nth((site.line as usize).saturating_sub(1)) else { return Default::default() };
    let end = cond
        .test_end
        .filter(|e| e.file == site.file && e.line == site.line && e.col > site.col)
        .map(|e| e.col as usize - 1);
    let from = site.col as usize - 1;
    let row: String = row.chars().skip(from).take(end.map_or(usize::MAX, |e| e - from)).collect();
    let test = row.split('%').next().unwrap_or("").trim().to_string();
    let reads = regex::Regex::new(r"\\(?:[A-Za-z@]+|.)")
        .map(|re| re.find_iter(&test).skip(1).map(|m| m.as_str().to_string()).collect())
        .unwrap_or_default();
    (test, reads)
}

/// Adjacent box marks and unknown characters merged. A box that ends where
/// the next begins is one box, the position marks between them kept inside.
fn tidy(line: &str) -> String {
    let (open, shut) = NOTE;
    let (end, begin) = (format!("{open}⟧{shut}"), format!("{open}⟦{shut}"));
    let mut joined = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(at) = rest.find(&end) {
        let after = &rest[at + end.len()..];
        let keys = after.chars().take_while(|&c| c == KEY).count();
        joined.push_str(&rest[..at]);
        match after[keys * KEY.len_utf8()..].starts_with(&begin) {
            true => {
                joined.push_str(&after[..keys * KEY.len_utf8()]);
                rest = &after[keys * KEY.len_utf8() + begin.len()..];
            }
            false => {
                joined.push_str(&end);
                rest = after;
            }
        }
    }
    joined.push_str(rest);
    let line = joined.replace(&format!("⟨?⟩{shut}{open}⟨?⟩"), "⟨?⟩").replace(&format!("⟨?⟩{shut}{open}⟨?⟩"), "⟨?⟩");
    regex::Regex::new(&format!("{open}⟦{shut}\\s*{open}⟧{shut}"))
        .map_or(line.clone(), |blank| blank.replace_all(&line, "").into_owned())
        .replace(&format!("{open}{shut}"), "")
        .trim()
        .to_string()
}

/// The paragraphs [`typeset`] computes (text, and whether it is `open`:
/// on some path it goes on into the next) paired in order with the ones an
/// engine set. A computed one that may be empty and does not match the next
/// set one stands alone. An open one that does not cover the next set one
/// is taken with the ones it may go on into. Each pair holds the set text
/// (empty: none) and the range of computed paragraphs.
pub fn pair_paragraphs(computed: &[(String, bool)], set: &[String]) -> Vec<(String, std::ops::Range<usize>)> {
    let text = |r: std::ops::Range<usize>| computed[r].iter().map(|c| c.0.as_str()).collect::<Vec<_>>().join(" ");
    let mut pairs = Vec::new();
    let mut engine = set.iter().peekable();
    let mut k = 0;
    while k < computed.len() {
        let e = engine.peek().map(|e| e.as_str());
        let mut end = k + 1;
        if let Some(e) = e {
            let mut j = k + 1;
            while typeset_verdict(&text(k..j), e) == "DIFFERS" && computed[j - 1].1 && j < computed.len() {
                j += 1;
            }
            if typeset_verdict(&text(k..j), e) != "DIFFERS" {
                end = j;
            }
        }
        let c = text(k..end);
        let alone = typeset_verdict(&c, "") != "DIFFERS" && e.is_none_or(|e| typeset_verdict(&c, e) == "DIFFERS");
        // Past the last set one, an open paragraph's next one may be its own rest.
        if let Some(last) = pairs.last_mut().filter(|_| e.is_none() && !alone && computed[k - 1].1) {
            let (set, range): &mut (String, std::ops::Range<usize>) = last;
            if typeset_verdict(&text(range.start..end), set) != "DIFFERS" {
                range.end = end;
                k = end;
                continue;
            }
        }
        let e = if alone { String::new() } else { engine.next().cloned().unwrap_or_default() };
        pairs.push((e, k..end));
        k = end;
    }
    pairs.extend(engine.map(|e| (e.clone(), computed.len()..computed.len())));
    pairs
}

/// How the text [`typeset`] computes compares with what an engine set,
/// whitespace aside: `equal`, `covers` (the engine's text is one of the
/// choices, so sound), or `DIFFERS` (unsound).
pub fn typeset_verdict(computed: &str, typeset: &str) -> &'static str {
    // Rules are drawn, not characters an engine's log lists.
    let squeeze = |s: &str| {
        s.replace("⟨?⟩", "\u{10FFFF}")
            .chars()
            .filter(|c| !c.is_whitespace() && !matches!(c, '│' | '―' | '▬'))
            .collect::<String>()
    };
    let (computed, typeset) = (squeeze(computed), squeeze(typeset));
    if computed == typeset {
        return "equal";
    }
    let mut pattern = String::from("^");
    for c in computed.chars() {
        match c {
            '{' | '⟦' => pattern.push_str("(?:"),
            '|' => pattern.push('|'),
            '}' => pattern.push(')'),
            '⟧' => pattern.push_str(")?"),
            crate::engine::tex::UNKNOWN_CHAR => pattern.push_str(".+?"),
            '…' => pattern.push('+'),
            '·' => pattern.push_str("-?"),
            c => pattern.push_str(&regex::escape(&c.to_string())),
        }
    }
    pattern.push('$');
    match regex::Regex::new(&pattern) {
        Ok(re) if re.is_match(&typeset) => "covers",
        _ => "DIFFERS",
    }
}

pub fn run(analysis: &Analysis, query: Query, filter: &Filter) -> Vec<Record> {
    let all = match query {
        Query::Definitions => definitions(analysis),
        Query::Expansions => expansions(analysis),
        Query::Dependencies => dependencies(analysis),
        Query::Occurrences => occurrences(analysis),
        Query::Recursion => recursion(analysis),
        Query::Calls => calls(analysis),
        Query::DependencyGraph => dependency_graph(analysis),
        Query::Trace => trace(analysis),
        Query::Files => files(analysis),
        Query::Diagnostics => diagnostics(analysis),
        Query::Distribution => distribution(analysis),
        Query::Catcodes => catcodes(analysis),
        Query::SideEffects => side_effects(analysis),
        Query::Project => project(analysis),
        Query::Plugins => plugins(analysis),
        Query::TypePlugins => crate::lsp::plugins::list(&analysis.settings.type_plugins),
        // The text to look for arrives with `--text`, which `run` has no
        // room for: `cmd::query` calls `produces` directly.
        Query::Produces => Vec::new(),
        Query::Gaps => gaps(analysis),
        Query::Pgfkeys => pgfkeys(analysis, None),
        // The name to look up arrives with `--for`, which `run` has no room
        // for: `cmd::query` calls `options` directly.
        Query::Options => Vec::new(),
        Query::Typeset => typeset(analysis),
        Query::Previews => crate::queries::preview::sheet(analysis)
            .into_iter()
            .map(|(name, svg)| Record::from_iter([("name".to_string(), json!(name)), ("svg".to_string(), json!(svg))]))
            .collect(),
    };
    all.into_iter().filter(|r| filter.accepts(r)).collect()
}

pub fn place(analysis: &Analysis, span: Span) -> Record {
    let mut record = Map::new();
    record.insert("file".into(), json!(analysis.short_name(span.file)));
    record.insert("path".into(), json!(analysis.file_name(span.file)));
    record.insert("line".into(), json!(span.line));
    record.insert("col".into(), json!(span.col));
    record
}

fn symbol(analysis: &Analysis, sym: Option<Sym>) -> Json {
    match sym {
        Some(s) => json!(analysis.interner.name(s)),
        None => Json::Null,
    }
}

/// `document` for the main file and the files beside or below it, the
/// project; `library` for everything else the run read.
pub fn origin(analysis: &Analysis, file: crate::engine::tex::FileId) -> &'static str {
    let path = |id: crate::engine::tex::FileId| analysis.files.get(id as usize).map(|f| Path::new(&f.path));
    let inside = match (path(file), path(analysis.main_file)) {
        (Some(file), Some(main)) => crate::project::paths::within(&crate::project::paths::dir_of(main), file),
        _ => false,
    };
    if file == analysis.main_file || inside { "document" } else { "library" }
}

fn definitions(analysis: &Analysis) -> Vec<Record> {
    analysis
        .facts
        .defs
        .iter()
        .map(|def| {
            let mut record = place(analysis, def.span);
            record.insert("name".into(), json!(analysis.interner.cs(def.name)));
            record.insert("tag".into(), json!(def.tag));
            record.insert("by".into(), json!(analysis.interner.cs(def.by)));
            insert_comment(&mut record, analysis, def.span);
            record.insert("subject".into(), symbol(analysis, def.subject));
            record.insert("package".into(), symbol(analysis, def.package));
            record.insert("origin".into(), json!(origin(analysis, def.span.file)));
            record.insert("mode".into(), json!(def.mode.as_str()));
            record.insert("arity".into(), json!(def.arity()));
            let mac = def.mac.as_deref();
            record.insert("takes".into(), takes_of(analysis, def.name, mac, None));
            record.insert("signature".into(), json!(mac.and_then(|m| m.arg_spec.as_ref()).map(|s| &s.raw)));
            record.insert("parameters".into(), json!(mac.map(|m| m.parameter_text.render(&analysis.interner))));
            record.insert("global".into(), json!(def.global));
            record.insert("certain".into(), json!(def.certain));
            record.insert("depth".into(), json!(def.depth));
            record.insert("node".into(), json!(def.node));
            record.insert("body".into(), json!(mac.map(|m| detokenize(&m.replacement_text, &analysis.interner))));
            record
        })
        .collect()
}

fn expansions(analysis: &Analysis) -> Vec<Record> {
    analysis
        .facts
        .expansions
        .iter()
        .map(|use_| {
            let mut record = place(analysis, use_.span);
            record.insert("name".into(), json!(analysis.interner.cs(use_.name)));
            record.insert("meaning".into(), json!(use_.meaning.as_str()));
            // The `node` of the definition this call resolved to.
            record.insert(
                "definition".into(),
                match use_.meaning {
                    crate::analysis::facts::MeaningKind::Macro(def) => json!(def),
                    _ => Json::Null,
                },
            );
            record.insert("package".into(), symbol(analysis, use_.package));
            record.insert("within".into(), symbol(analysis, use_.within));
            record.insert("node".into(), json!(use_.node));
            record.insert("count".into(), json!(use_.count));
            record.insert("mode".into(), json!(use_.mode.names()));
            // The texts its parameters took: none for a macro that reads
            // what follows itself (`\documentclass` through `\@ifnextchar`).
            record.insert(
                "arguments".into(),
                json!(use_.arguments.iter().map(|a| detokenize(a, &analysis.interner)).collect::<Vec<_>>()),
            );
            record.insert("conditional".into(), json!(!use_.cds.is_empty()));
            record
        })
        .collect()
}

fn dependencies(analysis: &Analysis) -> Vec<Record> {
    analysis
        .facts
        .loads
        .iter()
        .map(|load| {
            let mut record = place(analysis, load.span);
            record.insert("name".into(), json!(load.name));
            record.insert("kind".into(), json!(load.kind.as_str()));
            record.insert("options".into(), json!(load.options));
            record.insert("by".into(), symbol(analysis, load.by));
            record.insert("status".into(), json!(load.status.as_str()));
            record.insert("depth".into(), json!(load.depth));
            record.insert("certain".into(), json!(load.certain));
            record.insert("resolved".into(), json!(load.path.is_some()));
            record.insert("path".into(), json!(load.path));
            // `\ProvidesPackage{p}[2019/01/01 v1.0 …]`: what the file says it is.
            record.insert(
                "provided".into(),
                json!(identification(analysis, load.file).or_else(|| load.provided.clone())),
            );
            // The TeX Live package depp would name for the file.
            let texlive = load.path.as_deref().and_then(|p| analysis.settings.depp.package_of(p));
            record.insert("texlive".into(), json!(texlive));
            record
        })
        .collect()
}

fn occurrences(analysis: &Analysis) -> Vec<Record> {
    analysis
        .facts
        .occurrences
        .iter()
        .map(|occ| {
            let mut record = place(analysis, occ.span);
            record.insert("kind".into(), json!(occ.kind.as_str()));
            record.insert("key".into(), json!(occ.key));
            record.insert("detail".into(), json!(occ.detail));
            record.insert("expanded".into(), occ.expanded.map_or(Json::Null, |s| Json::Object(place(analysis, s))));
            record.insert("section".into(), json!(occ.section));
            record.insert("package".into(), symbol(analysis, occ.package));
            record.insert("node".into(), json!(occ.node));
            record
        })
        .collect()
}

fn recursion(analysis: &Analysis) -> Vec<Record> {
    let defined: BTreeMap<Sym, Span> = analysis.facts.defs.iter().map(|d| (d.name, d.span)).collect();
    analysis
        .recursion()
        .into_iter()
        .filter(|r| defined.contains_key(&r.name))
        .map(|crate::analysis::machine::Recursion { name: sym, cycle, observed }| {
            let span = defined.get(&sym).copied().unwrap_or_default();
            let mut record = place(analysis, span);
            record.insert("name".into(), json!(analysis.interner.cs(sym)));
            record.insert("kind".into(), json!(if cycle.len() > 1 { "mutual" } else { "direct" }));
            record.insert("observed".into(), json!(observed));
            record.insert("cycle".into(), json!(cycle.iter().map(|s| analysis.interner.cs(*s)).collect::<Vec<_>>()));
            record
        })
        .collect()
}

fn calls(analysis: &Analysis) -> Vec<Record> {
    let mut out = Vec::new();
    for (i, caller) in analysis.calls.nodes.iter().enumerate() {
        for &target in &analysis.calls.edges[i] {
            let mut record = Map::new();
            record.insert("name".into(), json!(analysis.interner.cs(*caller)));
            record.insert("callee".into(), json!(analysis.interner.cs(analysis.calls.nodes[target])));
            out.push(record);
        }
    }
    out
}

fn dependency_graph(analysis: &Analysis) -> Vec<Record> {
    analysis
        .graph
        .vertices
        .iter()
        .enumerate()
        .map(|(id, vertex)| {
            let mut record = place(analysis, vertex.span);
            record.insert("node".into(), json!(id));
            record.insert("tag".into(), json!(vertex.tag.as_str()));
            record.insert("name".into(), json!(vertex.render(&analysis.interner)));
            record.insert(
                "edges".into(),
                json!(
                    analysis
                        .graph
                        .outgoing(id as NodeId)
                        .iter()
                        .map(|(to, kind)| json!({ "to": to, "types": kind.names() }))
                        .collect::<Vec<_>>()
                ),
            );
            record.insert("controls".into(), json!(vertex.cds.len()));
            record
        })
        .collect()
}

/// Each step, with what a macro call inserts and the text a step set at
/// its own position (`\meaning`, `\the`, a character).
fn trace(analysis: &Analysis) -> Vec<Record> {
    let aliases = register_aliases(analysis);
    let mut set: HashMap<Span, String> = HashMap::default();
    for (c, spot) in analysis.facts.text.chars.chars().zip(&analysis.facts.text.at) {
        set.entry(spot.span).or_default().push(c);
    }
    let calls: HashMap<_, _> = analysis.facts.expansions.iter().map(|e| ((e.span, e.name), e)).collect();
    let defs: HashMap<_, _> = analysis.facts.defs.iter().map(|d| (d.node, d)).collect();
    analysis
        .trace
        .iter()
        .map(|event| {
            let mut record = place(analysis, event.span);
            record.insert("index".into(), json!(event.index));
            record.insert("depth".into(), json!(event.depth));
            record.insert("step".into(), json!(event.kind.as_str()));
            let name = match (event.name, event.kind) {
                (Sym(0), crate::analysis::machine::Step::OpenGroup) => "{".to_string(),
                (Sym(0), crate::analysis::machine::Step::CloseGroup) => "}".to_string(),
                (sym, _) => step_name(analysis, sym),
            };
            record.insert("name".into(), json!(name));
            let expand = matches!(event.kind, crate::analysis::machine::Step::Expand);
            record.insert("detail".into(), json!(if expand { None } else { event.detail.as_deref() }));
            let call = expand.then(|| calls.get(&(event.span, event.name))).flatten();
            let def = call.and_then(|call| match call.meaning {
                crate::analysis::facts::MeaningKind::Macro(n) => {
                    defs.get(&n).and_then(|d| Some((*call, d.mac.as_ref()?)))
                }
                _ => None,
            });
            if let Some((call, m)) = def {
                record.insert("parameters".into(), json!(m.parameter_text.render(&analysis.interner)));
                record.insert("body".into(), json!(detokenize(&m.replacement_text, &analysis.interner)));
                let args: Vec<String> = call.arguments.iter().map(|a| detokenize(a, &analysis.interner)).collect();
                record.insert("arguments".into(), json!(args));
            }
            if expand && let Some(body) = event.detail.as_deref() {
                record.insert("inserts".into(), json!(body));
            } else if let Some((call, m)) = def {
                let args: Vec<Vec<_>> = call.arguments.iter().map(|a| a.to_vec()).collect();
                if let Some(body) = crate::analysis::machine::substitute(m, &args, usize::MAX) {
                    record.insert("inserts".into(), json!(detokenize(&body, &analysis.interner)));
                }
            }
            if !event.fork.is_empty() {
                record.insert("fork".into(), json!(event.fork.as_ref()));
            }
            if let Some(text) = set.get(&event.span) {
                record.insert("sets".into(), json!(text));
            }
            record.insert("ahead".into(), json!(event.ahead.as_deref()));
            if !event.queued.is_empty() {
                record.insert(
                    "tokens".into(),
                    json!(event.queued.iter().map(|(t, c)| json!([t.as_ref(), c])).collect::<Vec<_>>()),
                );
            }
            if !event.meanings.is_empty() {
                let meanings: Map<String, Json> =
                    event.meanings.iter().map(|(s, m)| (step_name(analysis, *s), json!(m.as_ref()))).collect();
                record.insert("meanings".into(), Json::Object(meanings));
            }
            let named: Map<String, Json> = std::iter::once(event.name)
                .chain(event.meanings.iter().map(|(s, _)| *s))
                .map(|s| step_name(analysis, s))
                .filter_map(|n| Some((n.clone(), json!(aliases.get(&n)?))))
                .collect();
            if !named.is_empty() {
                record.insert("aliases".into(), Json::Object(named));
            }
            record
        })
        .collect()
}

fn files(analysis: &Analysis) -> Vec<Record> {
    analysis
        .files
        .iter()
        .enumerate()
        .map(|(id, info)| {
            let mut record = Map::new();
            record.insert("id".into(), json!(id));
            record.insert("file".into(), json!(analysis.short_name(id as crate::engine::tex::FileId)));
            record.insert("path".into(), json!(info.path));
            record.insert("kind".into(), json!(info.kind.as_str()));
            record
        })
        .chain(analysis.facts.absent.iter().map(|name| {
            let mut record = Map::new();
            record.insert("file".into(), json!(name));
            record.insert("kind".into(), json!("absent"));
            record
        }))
        .collect()
}

fn side_effects(analysis: &Analysis) -> Vec<Record> {
    analysis
        .graph
        .vertices
        .iter()
        .enumerate()
        .filter_map(|(id, vertex)| {
            let caller = analysis
                .graph
                .outgoing(id as NodeId)
                .iter()
                .find(|(_, kind)| kind.intersects(EdgeKind::SIDE_EFFECT_ON_CALL))
                .map(|(to, _)| *to)?;
            let caused_by = analysis.graph.vertex(caller)?;
            let mut record = place(analysis, vertex.span);
            record.insert("name".into(), json!(vertex.render(&analysis.interner)));
            record.insert("tag".into(), json!(vertex.tag.as_str()));
            record.insert("by".into(), json!(caused_by.render(&analysis.interner)));
            record.insert("detail".into(), json!(format!("while expanding at {}", caused_by.span)));
            Some(record)
        })
        .collect()
}

fn catcodes(analysis: &Analysis) -> Vec<Record> {
    let base = crate::engine::tex::CatcodeTable::latex();
    analysis
        .catcodes
        .diff(&base)
        .into_iter()
        .map(|(character, code)| {
            let mut record = Map::new();
            record.insert("key".into(), json!(character.to_string()));
            record.insert("code".into(), json!(code));
            record.insert("kind".into(), json!(crate::engine::tex::Catcode::from_u8(code).map(|c| format!("{c:?}"))));
            record.insert("detail".into(), json!(format!("was {:?}", base.get(character))));
            record
        })
        .collect()
}

fn distribution(analysis: &Analysis) -> Vec<Record> {
    let found = &analysis.distribution;
    let mut record = Map::new();
    record.insert("discovery".into(), json!(found.discovery.as_str()));
    record.insert("year".into(), json!(found.year));
    record.insert("format".into(), json!(found.format_version));
    record.insert("indexed".into(), json!(found.indexed));
    record.insert("message".into(), json!(found.describe()));
    let mut out = vec![record];
    for root in &found.roots {
        let mut record = Map::new();
        record.insert("kind".into(), json!("texmf-tree"));
        record.insert("root".into(), json!(root.display().to_string()));
        out.push(record);
    }
    // What the index holds: each directory of a tree with how many files.
    let mut dirs: std::collections::BTreeMap<(String, String), usize> = Default::default();
    for (_, path) in crate::caches::distribution::files_with(&found.roots, "") {
        let Some(root) = found.roots.iter().find(|r| path.starts_with(r)) else { continue };
        let dir = path.parent().and_then(|d| d.strip_prefix(root).ok()).map(|d| d.display().to_string());
        *dirs.entry((root.display().to_string(), dir.unwrap_or_default())).or_default() += 1;
    }
    for ((root, dir), count) in dirs {
        let mut record = Map::new();
        record.insert("kind".into(), json!("directory"));
        record.insert("root".into(), json!(root));
        record.insert("name".into(), json!(dir));
        record.insert("count".into(), json!(count));
        out.push(record);
    }
    out
}

fn plugins(analysis: &Analysis) -> Vec<Record> {
    analysis
        .plugins
        .rows()
        .into_iter()
        .map(|(kind, value, source)| {
            let mut record = Map::new();
            record.insert("kind".into(), json!(kind.as_str()));
            record.insert("name".into(), json!(value));
            record.insert("detail".into(), json!(source));
            record
        })
        .collect()
}

fn project(analysis: &Analysis) -> Vec<Record> {
    let identity = identity(analysis);
    let mut first = Map::new();
    first.insert("kind".into(), json!("document"));
    first.insert("detail".into(), json!(identity.kind));
    first.insert("name".into(), json!(identity.class));
    first.insert("message".into(), json!(format!("engine {} ({})", identity.engine, identity.engine_source)));
    let mut out = vec![first];
    for file in &analysis.project.files {
        let mut record = Map::new();
        record.insert("kind".into(), json!(file.tool));
        record.insert("name".into(), json!(file.path.display().to_string()));
        record.insert(
            "detail".into(),
            json!(file.settings.iter().map(|(key, value)| format!("{key}={value}")).collect::<Vec<_>>().join(", ")),
        );
        out.push(record);
    }
    // What the walk of the project directory found, which is where a run
    // with no file named on the command line starts from.  A tree can hold
    // thousands of them, and `count` is what the row is really for.
    for (kind, paths) in analysis.plugins.discovery.rows() {
        if paths.is_empty() {
            continue;
        }
        let mut record = Map::new();
        record.insert("kind".into(), json!(kind));
        record.insert("count".into(), json!(paths.len()));
        let shown = paths.len().min(FOUND_SHOWN);
        let mut detail = paths[..shown].iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ");
        if shown < paths.len() {
            detail.push_str(&format!(", +{}", paths.len() - shown));
        }
        record.insert("detail".into(), json!(detail));
        out.push(record);
    }
    out
}

fn diagnostics(analysis: &Analysis) -> Vec<Record> {
    analysis
        .facts
        .diagnostics
        .iter()
        .map(|d| {
            let mut record = place(analysis, d.span);
            record.insert("severity".into(), json!(d.severity.as_str()));
            record.insert("code".into(), json!(d.code));
            record.insert("message".into(), json!(d.message));
            record.insert("count".into(), json!(d.count));
            let origin = d.origin.map(|o| format!("{}:{}:{}", analysis.short_name(o.file), o.line, o.col));
            record.insert("origin".into(), json!(origin));
            record
        })
        .collect()
}

/// Which way `satex slice` walks the [`crate::analysis::graph::DependencyGraph`] from its
/// criteria: backward to what they depend on, or forward to what depends on
/// them.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    Backward,
    Forward,
}

const SLICE_EDGES: EdgeKind = EdgeKind(
    EdgeKind::READS.0
        | EdgeKind::DEFINED_BY.0
        | EdgeKind::EXPANDS.0
        | EdgeKind::ARGUMENT.0
        | EdgeKind::SIDE_EFFECT_ON_CALL.0
        | EdgeKind::CONTROL.0,
);

/// Where `satex slice --at` points: the main file unless a file is named,
/// which is how a position in an `\input`ed file is reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct At {
    pub file: Option<String>,
    pub line: u32,
    pub col: u32,
    /// The end of a range, inclusive: `LINE:COL`, with `u32::MAX` as the
    /// column when only a line was given, meaning the end of that line.
    pub end: Option<(u32, u32)>,
}

impl At {
    pub fn here(line: u32, col: u32) -> At {
        At { file: None, line, col, end: None }
    }

    /// The file the position is in.  The run records a file under the path
    /// it resolved it to, which may be relative or absolute, so either name
    /// may be the longer one and a common tail is enough to identify it.
    pub fn file(&self, analysis: &Analysis) -> Option<FileId> {
        let Some(wanted) = &self.file else { return Some(analysis.main_file) };
        let wanted = std::path::Path::new(wanted);
        (0..analysis.files.len() as FileId).find(|&id| {
            let path = std::path::Path::new(analysis.file_name(id));
            path.ends_with(wanted)
                || wanted.ends_with(path)
                || crate::project::config::names_file(analysis.file_name(id), &wanted.to_string_lossy())
        })
    }
}

impl std::fmt::Display for At {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `u32::MAX` stands for "wherever on the line", the column `explain`
        // fills in for a `@line`/`@file:line` suffix that named no column.
        let col = (self.col != u32::MAX).then_some(self.col);
        match (&self.file, col) {
            (Some(file), Some(col)) => write!(f, "{file}:{}:{col}", self.line),
            (Some(file), None) => write!(f, "{file}:{}", self.line),
            (None, Some(col)) => write!(f, "{}:{col}", self.line),
            (None, None) => write!(f, "{}", self.line),
        }?;
        match self.end {
            Some((line, u32::MAX)) => write!(f, "-{line}"),
            Some((line, col)) => write!(f, "-{line}:{col}"),
            None => Ok(()),
        }
    }
}

fn resolve(analysis: &Analysis, names: &[String], at: Option<&At>) -> Vec<NodeId> {
    // `\~` names the control symbol and `~` names the name or, for a single
    // character, the active character, which lives apart from it (tex.web § 222).
    let named = |name: &str| {
        names.iter().any(|wanted| match wanted.strip_prefix('\\') {
            Some(cs) => name == cs,
            None => name == wanted || name.strip_prefix(crate::engine::tex::ACTIVE).is_some_and(|c| c == wanted),
        })
    };
    let mut out: Vec<NodeId> = analysis
        .graph
        .vertices
        .iter()
        .enumerate()
        .filter(|(_, vertex)| named(analysis.interner.name(vertex.name)))
        .map(|(id, _)| id as NodeId)
        .collect();
    if let Some(at) = at {
        out.extend(at_position(analysis, at));
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// What `--at` names.  A [`Span`] is a point, so the construct at a position
/// is the last one that begins at or before the column; a position that
/// precedes everything on the line — a leading brace, say — names the first
/// construct there rather than nothing.
fn at_position(analysis: &Analysis, at: &At) -> Vec<NodeId> {
    let Some(file) = at.file(analysis) else { return Vec::new() };
    if let Some(end) = at.end {
        let start = (at.line, at.col);
        let pos = |id: usize| {
            let span = analysis.graph.vertices[id].span;
            (span.file == file).then_some((span.line, span.col))
        };
        let inside: Vec<NodeId> = (0..analysis.graph.vertices.len())
            .filter(|&id| pos(id).is_some_and(|p| p >= start && p <= end))
            .map(|id| id as NodeId)
            .collect();
        if !inside.is_empty() {
            return inside;
        }
        // Plain text names no construct: the last one before it reads it.
        let last = (0..analysis.graph.vertices.len()).filter_map(pos).filter(|&p| p < start).max();
        return (0..analysis.graph.vertices.len())
            .filter(|&id| last.is_some() && pos(id) == last)
            .map(|id| id as NodeId)
            .collect();
    }
    let on_line = || {
        analysis.graph.vertices.iter().enumerate().filter(move |(_, v)| v.span.file == file && v.span.line == at.line)
    };
    let Some(col) = on_line()
        .map(|(_, v)| v.span.col)
        .filter(|start| *start <= at.col)
        .max()
        .or_else(|| on_line().map(|(_, v)| v.span.col).min())
    else {
        return Vec::new();
    };
    on_line().filter(|(_, v)| v.span.col == col).map(|(id, _)| id as NodeId).collect()
}

pub fn slice(analysis: &Analysis, names: &[String], at: Option<&At>, direction: Direction) -> Vec<Record> {
    // A point on a line no construct stands for (plain text, a macro body's
    // line): the brace-balanced lines around it, as a range. A line of only
    // a comment stays empty.
    let region = at.filter(|a| a.end.is_none()).and_then(|a| {
        let file = a.file(analysis)?;
        if analysis.graph.vertices.iter().any(|v| v.span.file == file && v.span.line == a.line) {
            return None;
        }
        let text = std::fs::read_to_string(analysis.file_name(file)).ok()?;
        let lines: Vec<&str> = text.lines().collect();
        let region = balanced_lines(&lines, &BTreeSet::from([a.line]));
        let code = |l: &u32| {
            let t = lines.get(*l as usize - 1).copied().unwrap_or_default();
            !t[..crate::engine::tex::comment_start(t).unwrap_or(t.len())].trim().is_empty()
        };
        let (first, last) = (*region.first()?, *region.last()?);
        region.iter().any(code).then(|| At { line: first, col: 1, end: Some((last, u32::MAX)), ..a.clone() })
    });
    let at = region.as_ref().or(at);
    let mut records = slice_from(analysis, resolve(analysis, names, at), direction);
    // A range of lines keeps its lines, also the text no vertex stands for.
    if let Some((at, file, (last, _))) = at.and_then(|at| Some((at, at.file(analysis)?, at.end?))) {
        // An environment begun there is one box: its body sets its size.
        let last = environments(analysis)
            .into_iter()
            .filter(|(begin, end, _)| begin.file == file && end.file == file && (at.line..=last).contains(&begin.line))
            .map(|(_, end, _)| end.line)
            .fold(last, u32::max);
        // The run's end ships out the last page, and with it all that is on it.
        let main = analysis.main_file;
        let ends = frame(analysis, &[]).ends;
        let first = match file == main && (at.line..=last).any(|l| ends.contains(&l)) {
            true => 1,
            false => at.line,
        };
        for line in first..=last {
            let mut record = place(analysis, Span::new(file, line, 1));
            record.insert("tag".into(), json!("line"));
            record.insert("file_id".into(), json!(file));
            records.push(record);
        }
    }
    records
}

/// Slice on every occurrence or definition `filter` accepts, for example
/// `kind=begin-environment and key=figure` to slice to every figure: the
/// criteria are a query over the same records `satex query occurrences` and
/// `satex query definitions` print, not a name or a position.
pub fn slice_matching(analysis: &Analysis, filter: &Filter, direction: Direction) -> Vec<Record> {
    let criteria: Vec<NodeId> = occurrences(analysis)
        .into_iter()
        .chain(definitions(analysis))
        .filter(|record| filter.accepts(record))
        .filter_map(|record| record.get("node").and_then(Json::as_u64))
        .map(|node| node as NodeId)
        .collect();
    slice_from(analysis, criteria, direction)
}

/// The last vertex a backward slice may reach: the one before the first
/// construct of the main file that follows every criterion.  No limit when a
/// criterion lies outside the main file.
fn slice_limit(analysis: &Analysis, criteria: &[NodeId]) -> NodeId {
    let main = analysis.main_file;
    let spans: Vec<_> = criteria.iter().filter_map(|&id| analysis.graph.vertex(id)).map(|v| v.span).collect();
    if spans.is_empty() || spans.len() != criteria.len() || spans.iter().any(|s| s.file != main) {
        return NodeId::MAX;
    }
    // Whole lines: what stands after a criterion on its line is part of the same statement.
    let last = spans.iter().map(|s| s.line).max().unwrap_or(0);
    let limit = analysis
        .graph
        .vertices
        .iter()
        .position(|v| v.span.file == main && v.span.line > last)
        .map_or(NodeId::MAX, |first| (first as NodeId).saturating_sub(1));
    // A token stored on its line and run later (a key's value) is reached past that line: its run bounds it.
    criteria.iter().copied().fold(limit, NodeId::max)
}

/// A backward slice from `nodes` that answers a body vertex to its own call.
fn backward(analysis: &Analysis, nodes: &[NodeId], limit: NodeId) -> Vec<NodeId> {
    let bound = slice_bound(analysis, limit, Some(analysis.main_file));
    analysis.graph.slice_until(nodes, SLICE_EDGES, false, &bound)
}

/// Trimmed (`slice.trim`), a load and `\begin{document}` are opaque: what
/// reads their effects depends on the definitions their files made, not on
/// the whole call, so a package nothing uses stays out.
fn slice_bound(analysis: &Analysis, limit: NodeId, home: Option<FileId>) -> crate::analysis::graph::SliceBound {
    let mut opaque = crate::fixed::HashSet::default();
    if analysis.settings.slice.trim {
        let main = analysis.main_file;
        let calls: BTreeSet<Span> = analysis
            .facts
            .loads
            .iter()
            .filter(|load| {
                matches!(load.kind, LoadKind::Package | LoadKind::Class | LoadKind::Inherited | LoadKind::Input)
            })
            .filter(|load| load.file.is_some_and(|file| !inlined(analysis, file)))
            .filter_map(|load| load.call.or(Some(load.span)))
            .filter(|span| span.file == main)
            .collect();
        opaque.extend(
            (0..analysis.graph.vertices.len() as NodeId)
                .filter(|&id| straight(analysis, &analysis.graph.vertices[id as usize]))
                .filter(|&id| calls.contains(&analysis.graph.vertices[id as usize].span)),
        );
        opaque.extend(frame_construct(analysis, &frame(analysis, &[])));
    }
    crate::analysis::graph::SliceBound { limit, home, opaque }
}

/// The vertices of `\begin{document}`, from its backslash to its closing brace.
fn frame_construct(analysis: &Analysis, frame: &Frame) -> crate::fixed::HashSet<NodeId> {
    let spans: Vec<(Span, u32)> = frame
        .nodes
        .iter()
        .filter_map(|&id| analysis.graph.vertex(id))
        .filter_map(|v| Some((v.span, *frame.widths.get(&v.span)?)))
        .collect();
    (0..analysis.graph.vertices.len() as NodeId)
        .filter(|&id| {
            let v = &analysis.graph.vertices[id as usize];
            straight(analysis, v)
                && spans.iter().any(|(s, w)| {
                    s.file == v.span.file && s.line == v.span.line && (s.col..s.col + w).contains(&v.span.col)
                })
        })
        .collect()
}

/// What the criteria run besides themselves: every vertex of a file one of
/// them inputs, and of the files that one inputs in turn.
fn runs(analysis: &Analysis, criteria: &[NodeId]) -> Vec<NodeId> {
    let spans: BTreeSet<Span> = criteria.iter().filter_map(|&id| analysis.graph.vertex(id)).map(|v| v.span).collect();
    let mut files: BTreeSet<FileId> = analysis
        .facts
        .loads
        .iter()
        .filter(|load| load.kind == LoadKind::Input && spans.contains(&load.call.unwrap_or(load.span)))
        .filter_map(|load| load.file)
        .collect();
    loop {
        let more: Vec<FileId> = analysis
            .facts
            .loads
            .iter()
            .filter(|load| load.kind == LoadKind::Input && files.contains(&load.span.file))
            .filter_map(|load| load.file)
            .filter(|file| !files.contains(file))
            .collect();
        if more.is_empty() {
            break;
        }
        files.extend(more);
    }
    let vertices = analysis.graph.vertices.iter().enumerate();
    criteria
        .iter()
        .copied()
        .chain(vertices.filter(|(_, v)| files.contains(&v.span.file)).map(|(id, _)| id as NodeId))
        .collect()
}

fn slice_from(analysis: &Analysis, criteria: Vec<NodeId>, direction: Direction) -> Vec<Record> {
    let forward = direction == Direction::Forward;
    let bound = slice_bound(
        analysis,
        if forward { NodeId::MAX } else { slice_limit(analysis, &criteria) },
        (!forward).then_some(analysis.main_file),
    );
    let criteria = runs(analysis, &criteria);
    analysis
        .graph
        .slice_until(&criteria, SLICE_EDGES, forward, &bound)
        .into_iter()
        .filter_map(|id| {
            let vertex = analysis.graph.vertex(id)?;
            let mut record = place(analysis, vertex.span);
            record.insert("node".into(), json!(id));
            record.insert("tag".into(), json!(vertex.tag.as_str()));
            record.insert("name".into(), json!(vertex.render(&analysis.interner)));
            Some(record)
        })
        .collect()
}

/// The source the slice keeps, in the order the run read it: a file of the
/// same kind as the one sliced, which runs and gives the criterion the same
/// value.
///
/// Every kept line is grown to the smallest brace-balanced region around it,
/// so that a definition whose body runs over several lines comes out whole.
/// A line of an `\input` file is laid out where the file became visible; a
/// line of a package or a class is represented by the line that loads it,
/// since its own lines only make sense inside it.  The text is kept a line
/// at a time, so everything the run did on a kept line is part of the slice
/// too, and so are the frame the file runs in — its class, its `document`
/// environment, the end of a plain run — the category codes its lines were
/// read under, and the environments around the lines it keeps.
pub fn reconstruct(analysis: &Analysis, slice: &[Record], source: &str) -> String {
    reconstruct_mapped(analysis, slice, source).0
}

/// [`reconstruct`], with the line of the document each line of it comes
/// from: `(file, line)`, `None` for one it adds.
pub fn reconstruct_mapped(analysis: &Analysis, slice: &[Record], source: &str) -> (String, Vec<Option<(String, u32)>>) {
    let mut nodes: BTreeSet<NodeId> = slice
        .iter()
        .filter_map(|record| record.get("node").and_then(Json::as_u64))
        .map(|node| node as NodeId)
        .collect();
    let mut texts: BTreeMap<FileId, Vec<String>> = BTreeMap::new();
    texts.insert(analysis.main_file, source.lines().map(str::to_string).collect());
    let frame = frame(analysis, &texts[&analysis.main_file]);
    // The frame reads what the whole run wrote (`\begin{document}` reads the
    // `.aux` file), and so does an environment's end: what stands after the
    // slice is kept out of what they pull in.
    // The statement of the document's own files the run reached last, to
    // its end: the vertex before the next line of that file.
    let reach = |ids: &mut dyn Iterator<Item = NodeId>| {
        let main = analysis.main_file;
        let last = ids
            .filter_map(|id| Some((id, analysis.graph.vertex(id)?)))
            .filter(|(_, v)| inlined(analysis, v.span.file))
            .filter(|(_, v)| !(v.span.file == main && frame.ends.contains(&v.span.line)))
            .max_by_key(|(id, _)| *id);
        match last {
            Some((id, last)) => analysis.graph.vertices[id as usize..]
                .iter()
                .position(|v| v.span.file == last.span.file && v.span.line > last.span.line)
                .map_or(NodeId::MAX, |next| (id + next as NodeId).saturating_sub(1)),
            None => NodeId::MAX,
        }
    };
    // Trimmed, the frame keeps its lines but not what it reads: what only
    // `\begin{document}` reads (package hooks, the `.aux` file) is no
    // dependency of the criterion.
    // The document's own catcode changes stay either way: they decide how
    // every later line reads. A package's stay inside its load.
    let trim = analysis.settings.slice.trim;
    let construct = frame_construct(analysis, &frame);
    let loads = slice_bound(analysis, NodeId::MAX, None).opaque;
    let from: Vec<NodeId> =
        frame.nodes.iter().copied().filter(|n| !trim || !construct.contains(n) && !loads.contains(n)).collect();
    let limit = reach(&mut nodes.iter().chain(&from).copied());
    nodes.extend(backward(analysis, &from, limit));
    let mut framing: BTreeMap<FileId, BTreeSet<u32>> = BTreeMap::new();
    for record in slice.iter().filter(|r| r.get("tag").and_then(Json::as_str) == Some("line")) {
        let (Some(file), Some(line)) =
            (record.get("file_id").and_then(Json::as_u64), record.get("line").and_then(Json::as_u64))
        else {
            continue;
        };
        if let Some((file, lines)) = home(analysis, file as FileId, line as u32, line as u32) {
            framing.entry(file).or_default().extend(lines);
        }
    }
    framing.entry(analysis.main_file).or_default().extend(frame.lines.iter().copied());
    framing.entry(analysis.main_file).or_default().extend(option_lines(analysis));
    // The magic comments pick the build (`% !TeX program = tex`): the slice asks for the same one.
    let magic = crate::plugins::magic::scan_with(source, &analysis.settings.magic);
    framing.entry(analysis.main_file).or_default().extend(magic.iter().map(|m| m.line as u32));
    let peeks = peeks(analysis);
    let lines = loop {
        let lines = kept_lines(analysis, &nodes, &peeks, &framing, &mut texts);
        let extra: Vec<NodeId> = on_kept_lines(analysis, &lines, &frame.ends)
            .into_iter()
            .chain(enclosing_environments(analysis, &lines))
            .chain(written_back(analysis, &nodes))
            .filter(|node| !nodes.contains(node))
            .filter(|node| !trim || !construct.contains(node))
            .collect();
        let groups = enclosing_groups(analysis, &lines);
        let conditionals = enclosing_conditionals(analysis, &lines);
        let peeked = unanswered_peeks(analysis, &nodes, &peeks, &lines, &texts);
        let paragraphs = enclosing_paragraphs(analysis, &lines, &frame.lines);
        if extra.is_empty()
            && groups.is_empty()
            && conditionals.is_empty()
            && peeked.is_empty()
            && paragraphs.is_empty()
        {
            break lines;
        }
        for (file, line) in groups.into_iter().chain(conditionals).chain(peeked).chain(paragraphs) {
            framing.entry(file).or_default().insert(line);
        }
        let limit = reach(&mut nodes.iter().chain(&extra).copied());
        nodes.extend(extra.iter().copied());
        nodes.extend(backward(analysis, &extra, limit));
    };
    let lines = whole_inputs(analysis, lines, &mut texts);
    // Sorted by where the line belongs in the main file: a line of the main
    // file by its own number, a line of an `\input` file by the position
    // where that file became visible.
    let mut chunks: Vec<((u32, u32, u32), FileId, u32)> = Vec::new();
    for (&file, numbers) in &lines {
        let key = |line: u32| {
            if file == analysis.main_file {
                return (line, 0, 0);
            }
            match analysis.entry.get(file as usize).copied().flatten() {
                Some(entry) => (entry.line, entry.col, u32::from(file) + 1),
                None => (0, 0, u32::from(file) + 1),
            }
        };
        for &number in numbers {
            // An `\input` line whose file is inlined right where it stands
            // is not run itself: running it too would read the file a
            // second time, on top of the copy already in the slice.
            if let Some(target) = input_stand_in(analysis, file, number)
                && lines.contains_key(&target)
            {
                continue;
            }
            chunks.push((key(number), file, number));
        }
    }
    chunks.sort_by_key(|(key, file, number)| (*key, *file, *number));
    let mut out = String::new();
    let mut origin = Vec::new();
    let mut current: Option<FileId> = None;
    for (_, file, number) in chunks {
        let Some(line) = texts.get(&file).and_then(|t| t.get(number as usize - 1)) else {
            continue;
        };
        if current != Some(file) && file != analysis.main_file {
            let _ = writeln!(out, "%% from {}", analysis.file_name(file));
            origin.push(None);
        }
        current = Some(file);
        let _ = writeln!(out, "{line}");
        origin.push(Some((analysis.file_name(file).to_string(), number)));
    }
    if !analysis.settings.slice.standalone || frame.class.is_empty() {
        return (out, origin);
    }
    let out = standalone(analysis, &out, &lines);
    let added = out.lines().count().saturating_sub(origin.len());
    (out, std::iter::repeat_n(None, added).chain(origin).collect())
}

/// An `\input` file every code line of which is kept is read by its own
/// load line instead of being inlined: it runs the same, and stays where
/// the run put it.  Inner files first, so a whole chain folds up.
fn whole_inputs(
    analysis: &Analysis,
    mut lines: BTreeMap<FileId, BTreeSet<u32>>,
    texts: &mut BTreeMap<FileId, Vec<String>>,
) -> BTreeMap<FileId, BTreeSet<u32>> {
    let code = |line: &String| !line[..crate::engine::tex::comment_start(line).unwrap_or(line.len())].trim().is_empty();
    loop {
        let whole = lines.iter().find_map(|(&file, kept)| {
            let load = analysis.facts.loads.iter().find(|l| l.kind == LoadKind::Input && l.file == Some(file))?;
            let text = texts.get(&file)?;
            let nested =
                lines.keys().any(|&f| f != file && loaded_by(analysis, f).is_some_and(|l| l.span.file == file));
            let all = (1..=text.len() as u32).all(|n| kept.contains(&n) || !code(&text[n as usize - 1]));
            (file != analysis.main_file && !nested && all && inlined(analysis, load.span.file))
                .then_some((file, load.span))
        });
        let Some((file, at)) = whole else { return lines };
        if let std::collections::btree_map::Entry::Vacant(e) = texts.entry(at.file) {
            let Ok(text) = std::fs::read_to_string(analysis.file_name(at.file)) else { return lines };
            e.insert(text.lines().map(str::to_string).collect());
        }
        lines.remove(&file);
        lines.entry(at.file).or_default().insert(at.line);
    }
}

/// `slice.standalone`: the reconstruction as one file that compiles in an
/// empty directory.  Every local file it reads goes in front of it as
/// `filecontents*`, and every graphic becomes `example-image` with the
/// bounding box of the original, so it takes the same room.
fn standalone(analysis: &Analysis, text: &str, inlined: &BTreeMap<FileId, BTreeSet<u32>>) -> String {
    let main = Path::new(analysis.file_name(analysis.main_file));
    let main_dir = main.parent().unwrap_or_else(|| Path::new(""));
    let root = std::fs::canonicalize(main).ok().and_then(|p| Some(p.parent()?.to_path_buf())).unwrap_or_default();
    let local = |path: &Path| !analysis.distribution.roots.iter().any(|root| path.starts_with(root));
    let written =
        |path: &Path| analysis.facts.written_files.iter().any(|w| Path::new(w).file_name() == path.file_name());
    let mut files: Vec<(PathBuf, PathBuf)> = project_files(analysis, text);
    // A file whose lines are in the text already needs no copy.
    let named = |name: &str| text.contains(name) || name.strip_suffix(".tex").is_some_and(|stem| text.contains(stem));
    files.extend(analysis.facts.loads.iter().filter(|load| named(&load.name)).filter_map(|load| {
        let file = load.file.filter(|f| !inlined.contains_key(f))?;
        let path = std::fs::canonicalize(analysis.file_name(file)).ok()?;
        let relative = path.strip_prefix(&root).map(Path::to_path_buf).ok()?;
        Some((path, relative))
    }));
    let mut head = String::new();
    let mut seen = BTreeSet::new();
    for (path, relative) in files {
        if !local(&path) || written(&path) || !seen.insert(relative.clone()) {
            continue;
        }
        let Ok(contents) = std::fs::read_to_string(&path) else { continue };
        let name = relative.to_string_lossy();
        let _ = writeln!(
            head,
            "\\begin{{filecontents*}}[overwrite]{{{name}}}\n{}\\end{{filecontents*}}",
            contents.trim_end().to_string() + "\n"
        );
    }
    let graphic = Regex::new(r"\\includegraphics(\*?)\s*(?:\[([^\]]*)\])?\s*\{([^{}]*)\}").expect("a valid pattern");
    let body = graphic.replace_all(text, |c: &regex::Captures| {
        let options = c.get(2).map_or("", |m| m.as_str());
        match example_image(main_dir, &c[3], options) {
            Some(options) => format!("\\includegraphics{}[{options}]{{example-image}}", &c[1]),
            None => c[0].to_string(),
        }
    });
    head + &body
}

/// The options that draw `example-image` as large as `file` with `options`:
/// its natural size as the bounding box, a trim as the viewport it leaves,
/// and no page, since the example has one.
fn example_image(dir: &Path, file: &str, options: &str) -> Option<String> {
    let path = std::iter::once(dir.join(file))
        .chain(["pdf", "png", "jpg", "jpeg"].iter().map(|ext| dir.join(format!("{file}.{ext}"))))
        .find(|p| p.is_file())?;
    let bytes = std::fs::read(&path).ok()?;
    let pdf = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"));
    let mut keys: Vec<(String, String)> = options
        .split(',')
        .filter_map(|o| {
            let (k, v) = o.split_once('=').unwrap_or((o, ""));
            (!k.trim().is_empty()).then(|| (k.trim().to_string(), v.trim().to_string()))
        })
        .collect();
    let page = keys.iter().find(|(k, _)| k == "page").and_then(|(_, v)| v.parse().ok()).unwrap_or(1);
    let (w, h) = match pdf {
        true => crate::engine::pdfbox::page_box(&bytes, "crop", page)?,
        false => crate::engine::pdfbox::raster_size(&bytes, 72)?,
    };
    // The box in scaled points, as the engine measures the file itself.
    let bb = format!("bb=0 0 {w}sp {h}sp");
    let bp = |sp: i64| sp as f64 / 65536.0 * 72.0 / 72.27;
    let (w, h) = (bp(w), bp(h));
    keys.retain(|(k, _)| k != "page");
    for (k, v) in &mut keys {
        if k != "trim" {
            continue;
        }
        let t: Vec<f64> = v.split_whitespace().filter_map(|n| n.parse().ok()).collect();
        let [l, b, r, top] = t[..] else { return None };
        (*k, *v) = ("viewport".into(), format!("{l} {b} {} {}", w - r, h - top));
    }
    let rest: Vec<String> = keys.into_iter().map(|(k, v)| if v.is_empty() { k } else { format!("{k}={v}") }).collect();
    Some(std::iter::once(bb).chain(rest).collect::<Vec<_>>().join(","))
}

/// The project's own files a reconstruction needs beside it to compile: a
/// local package or class (found beside the document rather than in the
/// distribution) and the graphics and bibliography files it names, each with
/// the path an exported project keeps it at, relative to the main file's own
/// directory. Only files the reconstructed text actually names are listed,
/// so a slice that never reaches a figure does not pull its image along.
pub fn project_files(analysis: &Analysis, text: &str) -> Vec<(PathBuf, PathBuf)> {
    let main_dir = Path::new(analysis.file_name(analysis.main_file)).parent().unwrap_or_else(|| Path::new(""));
    let local = |path: &Path| !analysis.distribution.roots.iter().any(|root| path.starts_with(root));
    let relative_to_main = |path: &Path| -> PathBuf {
        path.strip_prefix(main_dir)
            .map(Path::to_path_buf)
            .unwrap_or_else(|_| PathBuf::from(path.file_name().unwrap_or_default()))
    };
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut out = Vec::new();
    for load in &analysis.facts.loads {
        if !matches!(load.kind, LoadKind::Package | LoadKind::Class | LoadKind::Inherited) {
            continue;
        }
        let Some(path) = load.path.as_deref().map(Path::new) else { continue };
        if !local(path) || !text.contains(&load.name) || !seen.insert(path.to_path_buf()) {
            continue;
        }
        out.push((path.to_path_buf(), relative_to_main(path)));
    }
    for occurrence in &analysis.facts.occurrences {
        if !matches!(occurrence.kind, OccKind::Graphics | OccKind::Bibliography) {
            continue;
        }
        for key in occurrence.key.split(',').map(str::trim).filter(|k| !k.is_empty()) {
            if !text.contains(key) {
                continue;
            }
            let candidates: Vec<PathBuf> = if occurrence.kind == OccKind::Bibliography {
                vec![main_dir.join(format!("{key}.bib"))]
            } else if Path::new(key).extension().is_some() {
                vec![main_dir.join(key)]
            } else {
                ["pdf", "png", "jpg", "jpeg", "eps"].iter().map(|ext| main_dir.join(format!("{key}.{ext}"))).collect()
            };
            let Some(found) = candidates.into_iter().find(|p| p.is_file()) else { continue };
            if seen.insert(found.clone()) {
                let relative = relative_to_main(&found);
                out.push((found, relative));
            }
        }
    }
    out
}

/// What every run of the main file needs whatever the criterion: the class
/// it is set in, its `document` environment, its identification when it is
/// a package or a class, the category codes it changes, and what ends the
/// run: `\end{document}`, or the last line of a plain run.
struct Frame {
    nodes: Vec<NodeId>,
    lines: Vec<u32>,
    /// The lines that end the run.  What they read is kept out of the slice:
    /// nothing after the criterion can change what it printed.
    ends: Vec<u32>,
    /// How many columns `\begin{document}` takes, by its span.
    widths: BTreeMap<Span, u32>,
    /// The lines that load the class: none in plain TeX or a package.
    class: Vec<u32>,
}

fn frame(analysis: &Analysis, main: &[String]) -> Frame {
    let main_file = analysis.main_file;
    let here = || analysis.facts.occurrences.iter().filter(move |o| o.span.file == main_file);
    let document = |o: &&crate::analysis::facts::Occurrence| o.key == "document";
    let widths = here()
        .filter(|o| o.kind == OccKind::BeginEnvironment && document(o))
        .map(|o| (o.span, format!("\\begin{{{}}}", o.key).len() as u32))
        .collect();
    let nodes: Vec<NodeId> = here()
        .filter(|o| match o.kind {
            OccKind::Catcode | OccKind::Identification => true,
            OccKind::BeginEnvironment => document(o),
            _ => false,
        })
        .map(|o| o.node)
        .collect();
    let mut lines: Vec<u32> = analysis
        .facts
        .loads
        .iter()
        .filter(|load| load.kind == LoadKind::Class && load.span.file == main_file)
        .map(|load| load.span.line)
        .collect();
    let mut ends: Vec<u32> =
        here().filter(|o| o.kind == OccKind::EndEnvironment && document(o)).map(|o| o.span.line).collect();
    let library = analysis.files.get(main_file as usize).is_some_and(|f| f.kind.is_package());
    if lines.is_empty()
        && ends.is_empty()
        && !library
        && let Some(last) = main.iter().rposition(|line| !line.split('%').next().unwrap_or_default().trim().is_empty())
    {
        ends.push(last as u32 + 1);
    }
    let class = lines.clone();
    // A run that never reached `\end{document}` says nothing about what
    // follows where it stopped: the whole file stays.
    if !class.is_empty() && ends.is_empty() {
        lines.extend(1..=main.len() as u32);
    }
    lines.extend(ends.iter().copied());
    Frame { nodes, lines, ends, widths, class }
}

/// A package or a class sliced on its own: the lines that declare its
/// options and what of its own reads them (`\ProcessOptions`).  Its options
/// are its input, and a real caller passes some this run never saw.
fn option_lines(analysis: &Analysis) -> Vec<u32> {
    let main = analysis.main_file;
    if !analysis.files.get(main as usize).is_some_and(|f| f.kind.is_package()) {
        return Vec::new();
    }
    let options: Vec<NodeId> =
        analysis.facts.defs.iter().filter(|d| d.tag == "option" && d.span.file == main).map(|d| d.node).collect();
    let bound = crate::analysis::graph::SliceBound { limit: NodeId::MAX, home: None, opaque: Default::default() };
    analysis
        .graph
        .slice_until(&options, SLICE_EDGES, true, &bound)
        .into_iter()
        .filter_map(|id| analysis.graph.vertex(id))
        .filter(|v| straight(analysis, v) && v.span.file == main)
        .map(|v| v.span.line)
        .collect()
}

/// Everything the run did on a kept line, read from the file itself rather
/// than produced by an expansion.
fn on_kept_lines(analysis: &Analysis, lines: &BTreeMap<FileId, BTreeSet<u32>>, ends: &[u32]) -> Vec<NodeId> {
    let main = analysis.main_file;
    analysis
        .graph
        .vertices
        .iter()
        .enumerate()
        // A token a library macro expands (`\state` inside `\typeout`) is
        // read on its line too, so not only straight vertices count.
        .filter(|(_, v)| {
            !(v.span.file == main && ends.contains(&v.span.line))
                && lines.get(&v.span.file).is_some_and(|kept| kept.contains(&v.span.line))
        })
        .map(|(id, _)| id as NodeId)
        .collect()
}

/// The writes a kept line read back depends on: a vertex of a file the run
/// wrote itself (the `.aux`) stands for every call of the document's own
/// files that wrote a line, since which of them wrote it is not tracked.
/// The run before wrote the whole file, so a write after the reader counts.
fn written_back(analysis: &Analysis, nodes: &BTreeSet<NodeId>) -> Vec<NodeId> {
    let written = |file: FileId| {
        let name = Path::new(analysis.file_name(file)).file_name();
        analysis.facts.written_files.iter().any(|w| Path::new(w).file_name() == name)
    };
    if !nodes.iter().filter_map(|&id| analysis.graph.vertex(id)).any(|v| written(v.span.file)) {
        return Vec::new();
    }
    let calls: BTreeSet<Span> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.kind == OccKind::Write)
        .filter_map(|o| o.call.map(|(_, span)| span))
        .filter(|span| inlined(analysis, span.file))
        .collect();
    (0..analysis.graph.vertices.len() as NodeId)
        .filter(|&id| straight(analysis, &analysis.graph.vertices[id as usize]))
        .filter(|&id| calls.contains(&analysis.graph.vertices[id as usize].span))
        .collect()
}

/// Both ends of every environment that holds a kept line: a line kept
/// without the environment around it runs outside the group the environment
/// opens, and without whatever its `\begin` sets up.
fn enclosing_environments(analysis: &Analysis, lines: &BTreeMap<FileId, BTreeSet<u32>>) -> Vec<NodeId> {
    let mut out = Vec::new();
    for (&file, kept) in lines {
        for (begin, end) in environment_pairs(analysis, file) {
            if kept.range(begin.span.line..=end.span.line).next().is_some() {
                out.extend([begin.node, end.node]);
            }
        }
    }
    out
}

/// The `\begin`/`\end` pairs of `file`, matched by name.
fn environment_pairs(
    analysis: &Analysis,
    file: FileId,
) -> Vec<(&crate::analysis::facts::Occurrence, &crate::analysis::facts::Occurrence)> {
    let mut open: Vec<&crate::analysis::facts::Occurrence> = Vec::new();
    let mut marks: Vec<&crate::analysis::facts::Occurrence> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| o.span.file == file && matches!(o.kind, OccKind::BeginEnvironment | OccKind::EndEnvironment))
        .collect();
    marks.sort_by_key(|o| (o.span.line, o.span.col));
    let mut out = Vec::new();
    for mark in marks {
        if mark.kind == OccKind::BeginEnvironment {
            open.push(mark);
            continue;
        }
        let Some(at) = open.iter().rposition(|b| b.key == mark.key) else { continue };
        out.push((open[at], mark));
        open.truncate(at);
    }
    out
}

/// Both ends of every `\begingroup … \endgroup` that holds a kept line and
/// is not kept whole yet: without them what the group rolls back would stay.
fn enclosing_groups(analysis: &Analysis, lines: &BTreeMap<FileId, BTreeSet<u32>>) -> Vec<(FileId, u32)> {
    let mut out = Vec::new();
    for (open, close) in &analysis.facts.groups {
        let Some(kept) = lines.get(&open.file) else { continue };
        if kept.range(open.line..=close.line).next().is_none() {
            continue;
        }
        for line in [open.line, close.line] {
            if !kept.contains(&line) {
                out.push((open.file, line));
            }
        }
    }
    out
}

/// Every line of a paragraph that a kept line sets a character of: TeX
/// breaks a paragraph into lines as a whole, so where a character lands
/// depends on all of it.  The paragraphs are the run's (`\par`), each the
/// lines of its file from its first character to its last.
fn enclosing_paragraphs(
    analysis: &Analysis,
    lines: &BTreeMap<FileId, BTreeSet<u32>>,
    frame: &[u32],
) -> Vec<(FileId, u32)> {
    let text = &analysis.facts.text;
    // A space shows only when its glue stays (tex.web § 816): one a
    // paragraph's end removes is no part of it.
    let shows: Vec<bool> = text
        .chars
        .chars()
        .zip(&text.at)
        .map(|(c, s)| !c.is_whitespace() || analysis.facts.kept_spaces.contains(&s.span))
        .collect();
    let mut out = Vec::new();
    let mut start = 0usize;
    for end in text.pars.iter().map(|&p| p as usize).chain([text.at.len()]) {
        let range = start.min(end)..end;
        start = end;
        let spots: Vec<&crate::analysis::facts::Spot> =
            range.filter(|&i| shows.get(i) == Some(&true)).map(|i| &text.at[i]).collect();
        let mut spans: BTreeMap<FileId, (u32, u32)> = BTreeMap::new();
        for spot in spots.iter().filter(|s| inlined(analysis, s.span.file)) {
            let range = spans.entry(spot.span.file).or_insert((spot.span.line, spot.span.line));
            *range = (range.0.min(spot.span.line), range.1.max(spot.span.line));
        }
        // The frame's lines are kept whatever the criterion.
        let mine =
            |s: &crate::analysis::facts::Spot| !(s.span.file == analysis.main_file && frame.contains(&s.span.line));
        let touched =
            spots.iter().filter(|s| mine(s)).any(|s| lines.get(&s.span.file).is_some_and(|k| k.contains(&s.span.line)));
        if touched {
            for (&file, &(a, b)) in &spans {
                let kept = lines.get(&file);
                out.extend((a..=b).filter(|l| !kept.is_some_and(|k| k.contains(l))).map(|l| (file, l)));
            }
        }
    }
    out
}

/// Every line of every evaluated `\if…\fi` that holds a kept line of a
/// package or a class being sliced on its own: analyzed alone,
/// `\ProcessOptions` sees none of a real caller's options, so a switch a
/// `\DeclareOption` sets may be decided the wrong way for whoever loads the
/// slice for real.  The arm this run did not take is no dependency it can
/// find, so the whole conditional is kept wherever anything of it is.  A
/// document sliced from itself makes the same decision every time.
fn enclosing_conditionals(analysis: &Analysis, lines: &BTreeMap<FileId, BTreeSet<u32>>) -> Vec<(FileId, u32)> {
    let library = analysis.files.get(analysis.main_file as usize).is_some_and(|f| f.kind.is_package());
    if !library {
        return Vec::new();
    }
    let mut out = Vec::new();
    for cond in &analysis.facts.conditionals {
        let Some(fi) = cond.fi else { continue };
        let Some(kept) = lines.get(&cond.at.file) else { continue };
        let range = cond.at.line..=fi.line;
        if kept.range(range.clone()).next().is_some() {
            out.extend(range.filter(|line| !kept.contains(line)).map(|line| (cond.at.file, line)));
        }
    }
    out
}

/// The lines each file keeps for `nodes`: a line of a package or a class
/// stands for the line that loads it, and every kept line is grown until its
/// braces balance.  `texts` gathers the text of every file that keeps a line.
fn kept_lines(
    analysis: &Analysis,
    nodes: &BTreeSet<NodeId>,
    peeks: &crate::fixed::HashSet<NodeId>,
    framing: &BTreeMap<FileId, BTreeSet<u32>>,
    texts: &mut BTreeMap<FileId, Vec<String>>,
) -> BTreeMap<FileId, BTreeSet<u32>> {
    let mut wanted = framing.clone();
    for &node in nodes.iter().filter(|node| !peeks.contains(node)) {
        let Some(vertex) = analysis.graph.vertex(node) else { continue };
        let line = vertex.span.line;
        // A command whose arguments ran past its own line keeps them all.
        let last = analysis.graph.extent(vertex.span).unwrap_or(line).max(line);
        let Some((file, lines)) = home(analysis, vertex.span.file, line, last) else { continue };
        wanted.entry(file).or_default().extend(lines);
    }
    let mut out = BTreeMap::new();
    for (file, lines) in wanted {
        if let std::collections::btree_map::Entry::Vacant(e) = texts.entry(file) {
            let Ok(text) = std::fs::read_to_string(analysis.file_name(file)) else { continue };
            e.insert(text.lines().map(str::to_string).collect());
        }
        let text: Vec<&str> = texts[&file].iter().map(String::as_str).collect();
        out.insert(file, balanced_lines(&text, &lines));
    }
    out
}

/// Tokens of the document only looked at by a lookahead made elsewhere
/// (`\@ifnextchar` lets `\@let@token` to the next token): what the peek
/// reads is whichever token comes next, not the line it stands on.
fn peeks(analysis: &Analysis) -> crate::fixed::HashSet<NodeId> {
    let graph = &analysis.graph;
    (0..graph.vertices.len() as NodeId)
        .flat_map(|from| graph.outgoing(from).iter().map(move |&(to, kind)| (from, to, kind)))
        .filter(|&(from, to, kind)| {
            let (by, read) = (&graph.vertices[from as usize], &graph.vertices[to as usize]);
            kind.intersects(EdgeKind::DEFINED_BY)
                && by.tag == crate::analysis::graph::VertexTag::VariableDefinition
                && read.tag == crate::analysis::graph::VertexTag::Use
                && straight(analysis, read)
                && (by.span.file, by.span.line) != (read.span.file, read.span.line)
                && inlined(analysis, read.span.file)
        })
        .map(|(_, to, _)| to)
        .collect()
}

/// The lines of the peeked tokens whose place the slice gives to a token
/// that may answer the peek differently.  A macro call answers it like any
/// other: a lookahead tests for a character or an implicit one, never for
/// one particular macro (a convention).
fn unanswered_peeks(
    analysis: &Analysis,
    nodes: &BTreeSet<NodeId>,
    peeks: &crate::fixed::HashSet<NodeId>,
    lines: &BTreeMap<FileId, BTreeSet<u32>>,
    texts: &BTreeMap<FileId, Vec<String>>,
) -> Vec<(FileId, u32)> {
    let call_at = |file: FileId, line: u32, col: u32| {
        let text = texts.get(&file).and_then(|t| t.get(line as usize - 1)).map_or("", String::as_str);
        let starts = text.get(col as usize - 1..).is_some_and(|rest| {
            rest.strip_prefix('\\').is_some_and(|r| r.starts_with(|c: char| c.is_ascii_alphabetic()))
        });
        starts
            && analysis.graph.vertices.iter().any(|v| {
                straight(analysis, v)
                    && v.tag == crate::analysis::graph::VertexTag::MacroCall
                    && (v.span.file, v.span.line, v.span.col) == (file, line, col)
            })
    };
    nodes
        .iter()
        .filter(|node| peeks.contains(node))
        .filter_map(|&node| analysis.graph.vertex(node).map(|v| v.span))
        .filter(|span| !lines.get(&span.file).is_some_and(|kept| kept.contains(&span.line)))
        .filter(|span| {
            let next = lines.get(&span.file).and_then(|kept| {
                kept.range(span.line + 1..).find_map(|&line| {
                    let text = texts.get(&span.file)?.get(line as usize - 1)?;
                    let code = &text[..crate::engine::tex::comment_start(text).unwrap_or(text.len())];
                    let col = code.len() - code.trim_start().len();
                    (!code.trim().is_empty()).then_some((line, col as u32 + 1))
                })
            });
            !(call_at(span.file, span.line, span.col) && next.is_some_and(|(line, col)| call_at(span.file, line, col)))
        })
        .map(|span| (span.file, span.line))
        .collect()
}

/// Where lines `first..=last` of `file` go in the reconstruction: in the
/// file itself when it is the main file or one that it, or a file it
/// inlines, `\input`s; at the line that loads it otherwise, since the lines
/// of a package or a class only make sense inside it; nowhere when the run
/// began with it already there.
fn home(analysis: &Analysis, file: FileId, first: u32, last: u32) -> Option<(FileId, Vec<u32>)> {
    if inlined(analysis, file) {
        return Some((file, (first..=last).collect()));
    }
    let load = loaded_by(analysis, file)?;
    // A load made under a call of the document's own (`\usetikzlibrary`) stands at that call.
    let at = load.call.filter(|call| call.file != file && inlined(analysis, call.file)).unwrap_or(load.span);
    home(analysis, at.file, at.line, at.line)
}

/// The file an `\input` at `(file, line)` reads, when that file's own lines
/// are inlined right there instead of being represented by the load line:
/// running the load for real, on top of the inlined copy, would read it
/// twice.
fn input_stand_in(analysis: &Analysis, file: FileId, line: u32) -> Option<FileId> {
    analysis.facts.loads.iter().find_map(|load| {
        (load.kind == LoadKind::Input && load.span.file == file && load.span.line == line)
            .then_some(load.file)
            .flatten()
            .filter(|&target| inlined(analysis, target))
    })
}

fn loaded_by(analysis: &Analysis, file: FileId) -> Option<&Load> {
    analysis.facts.loads.iter().find(|load| load.file == Some(file) && load.span.file != file)
}

/// Whether the run read `v` straight from its file: no expansion made it,
/// or, in an `\input` file, only the library macro that reads the file.
fn straight(analysis: &Analysis, v: &crate::analysis::graph::Vertex) -> bool {
    let input = v.span.file != analysis.main_file && inlined(analysis, v.span.file);
    v.within.and_then(|w| analysis.graph.vertex(w)).is_none_or(|w| input && !inlined(analysis, w.span.file))
}

/// Whether a file's own lines go into the reconstruction: the main file,
/// and a file that one of those reads as input rather than as a package or
/// a class.
fn inlined(analysis: &Analysis, file: FileId) -> bool {
    let mut file = file;
    for _ in 0..analysis.files.len() {
        if file == analysis.main_file {
            return true;
        }
        // Installation files are never the document's to inline: a load deep
        // in the kernel's bootstrap would otherwise be pinned on the main file.

        let Some(load) = loaded_by(analysis, file) else { return false };
        if matches!(load.kind, LoadKind::Package | LoadKind::Class | LoadKind::Inherited) {
            return false;
        }
        file = load.span.file;
    }
    false
}

/// The wanted lines grown until their braces balance: for each one, the
/// smallest run of lines around it that begins and ends at brace depth zero.
///
/// Only braces count.  A group opened with `\begingroup` is not closed by
/// anything on the same line often enough for counting it to pay: a macro
/// body that opens one and leaves its `\endgroup` to another macro would
/// drag the rest of the file into every slice below it.
fn balanced_lines(text: &[&str], wanted: &BTreeSet<u32>) -> BTreeSet<u32> {
    let mut before = Vec::with_capacity(text.len());
    let mut depth = 0i32;
    for line in text {
        before.push(depth);
        depth += brace_delta(line);
    }
    let after = |i: usize| before.get(i + 1).copied().unwrap_or(depth);
    let mut out = BTreeSet::new();
    for &number in wanted {
        let Some(i) = (number as usize).checked_sub(1) else { continue };
        if i >= before.len() {
            continue;
        }
        let lo = (0..=i).rev().find(|&j| before[j] == 0).unwrap_or(0);
        let hi = (i..before.len()).find(|&k| after(k) == 0).unwrap_or(before.len() - 1);
        out.extend((lo..=hi).map(|j| j as u32 + 1));
    }
    out
}

/// How much one line changes the brace depth: `\{` is no group, and a
/// comment adds nothing (The TeXbook, ch. 2).
fn brace_delta(line: &str) -> i32 {
    let code = &line[..crate::engine::tex::comment_start(line).unwrap_or(line.len())];
    let mut delta = 0;
    let mut chars = code.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '{' => delta += 1,
            '}' => delta -= 1,
            _ => {}
        }
    }
    delta
}

/// Records in the order the run meets them, the document's own first.
pub fn in_document_order(analysis: &Analysis, records: &mut [Record]) {
    let main = analysis.file_name(analysis.main_file).to_string();
    records.sort_by_key(|record| {
        let path = record.get("file").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        let line = record.get("line").and_then(serde_json::Value::as_u64).unwrap_or(0);
        let col = record.get("col").and_then(serde_json::Value::as_u64).unwrap_or(0);
        (u8::from(!main.ends_with(&path)), path, line, col)
    });
}

/// Whether `name` now means `\iftrue` or `\iffalse`: a switch's value.
fn switch_truth(analysis: &Analysis, name: Sym) -> Option<&'static str> {
    match analysis.env.meaning(name).prim() {
        Some(crate::engine::builtins::Primitive::If(crate::engine::builtins::Cond::True)) => Some("true"),
        Some(crate::engine::builtins::Primitive::If(crate::engine::builtins::Cond::False)) => Some("false"),
        _ => None,
    }
}

/// A macro that only expands to `\iftrue` or `\iffalse`: a toggle (etoolbox's `\newtoggle`).
fn macro_switch(analysis: &Analysis, def: &crate::analysis::facts::Definition) -> Option<&'static str> {
    let mac = def.mac.as_ref().filter(|mac| mac.arity() == 0)?;
    match &*mac.replacement_text {
        [one] => switch_truth(analysis, one.cs()?),
        _ => None,
    }
}

pub fn switches(analysis: &Analysis, all: bool) -> Vec<Record> {
    let main = analysis.main_file;
    let mut out = Vec::new();
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    for def in &analysis.facts.defs {
        let name = analysis.interner.name(def.name);
        let dialect = analysis.interner.name(def.by);
        let truth = match &def.mac {
            Some(_) => macro_switch(analysis, def),
            None => switch_truth(analysis, def.name),
        };
        if def.mac.is_some() && truth.is_none() {
            continue;
        }
        // a conditional made by `\newif`, or any name that now means `\iftrue` or `\iffalse`
        // (`\let\my@ifdraft\iffalse`, an expl3 bool), whatever it is called
        let declared = (dialect.starts_with("new") || dialect.starts_with("provide"))
            && (name.starts_with("if") || dialect.contains("if"));
        if truth.is_none() && !declared {
            continue;
        }
        if !all && def.span.file != main {
            continue;
        }
        if seen.insert(name.to_string(), ()).is_some() {
            continue;
        }
        let value = truth.unwrap_or("undecided");
        let governs = governed_count(analysis, def.name);
        let mut record = place(analysis, def.span);
        record.insert("tag".into(), json!("switch"));
        record.insert("name".into(), json!(analysis.interner.cs(def.name)));
        record.insert("by".into(), json!(format!("\\{dialect}")));
        record.insert("value".into(), json!(value));
        record.insert("governs".into(), json!(governs));
        out.push(record);
    }
    for load in &analysis.facts.loads {
        if load.options.is_empty() || (!all && load.span.file != main) {
            continue;
        }
        let mut record = place(analysis, load.span);
        record.insert("tag".into(), json!("option"));
        record.insert("name".into(), json!(load.name.clone()));
        record.insert("by".into(), json!(load.kind.as_str()));
        record.insert("value".into(), json!(load.options.join(", ")));
        out.push(record);
    }
    out.extend(offered_options(analysis, all));
    out
}

/// How much a switch decides: the definitions and uses recorded under it.
fn governed_count(analysis: &Analysis, name: Sym) -> usize {
    let under = |cds: &[crate::analysis::graph::ControlDep]| {
        cds.iter().any(|cd| analysis.graph.vertex(cd.on).is_some_and(|vertex| vertex.name == name))
    };
    analysis.facts.defs.iter().filter(|def| under(&def.cds)).count()
        + analysis.facts.expansions.iter().filter(|use_| under(&use_.cds)).count()
}

/// How many options a package's row names before it is cut short.
const OFFERED_SHOWN: usize = 8;

/// How many paths a `project` row names before it is cut short.
const FOUND_SHOWN: usize = 8;

/// What each package accepts: its `\DeclareOption` and `\define@key` names,
/// the ones the document passes first.
fn offered_options(analysis: &Analysis, all: bool) -> Vec<Record> {
    let mut by_package: BTreeMap<String, (Vec<String>, Span)> = BTreeMap::new();
    for def in &analysis.facts.defs {
        let Some(subject) = def.subject else { continue };
        // A key in the family named after the package that declares it is a
        // package option: kvoptions' family is `\@currname` and xkeyval's
        // `\DeclareOptionX` uses `\@currname.\@currext` (their manuals).
        let own_key = def.tag == "key"
            && def.package.is_some_and(|package| {
                let package = analysis.interner.name(package);
                let name = analysis.interner.name(def.name);
                let key = analysis.interner.name(subject);
                ["", ".sty", ".cls"].iter().any(|ext| name == format!("KV@{package}{ext}@{key}"))
            });
        if def.tag != "option" && !own_key {
            continue;
        }
        let name = analysis.interner.name(subject);
        // `\DeclareOption*` and the keys a package declares for itself are
        // not options a document can pass.
        let internal = name.is_empty()
            || analysis.settings.conventions.is_internal(name)
            || !name.starts_with(|c: char| c.is_ascii_alphanumeric());
        if internal && !all {
            continue;
        }
        let package = def
            .package
            .map(|sym| analysis.interner.name(sym).to_string())
            .unwrap_or_else(|| analysis.file_name(def.span.file).to_string());
        let entry = by_package.entry(package).or_insert_with(|| (Vec::new(), def.span));
        let name = analysis.interner.name(subject).to_string();
        if !entry.0.contains(&name) {
            entry.0.push(name);
        }
    }
    // What the package switches on by itself, before the document says
    // anything (`\ExecuteOptions`).
    let mut defaults: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for occurrence in &analysis.facts.occurrences {
        if occurrence.kind != OccKind::PassedOption || occurrence.detail.as_deref() != Some("default") {
            continue;
        }
        let package = occurrence.package.map(|sym| analysis.interner.name(sym).to_string()).unwrap_or_default();
        defaults.entry(package).or_default().push(occurrence.key.clone());
    }

    let mut out = Vec::new();
    for (package, (mut names, span)) in by_package {
        let passed: Vec<String> = analysis
            .facts
            .loads
            .iter()
            .filter(|load| load.name == package)
            .flat_map(|load| load.options.clone())
            .collect();
        let empty = Vec::new();
        let default = defaults.get(&package).unwrap_or(&empty);
        names.sort_by_key(|name| (!passed.contains(name), !default.contains(name), name.clone()));
        let names: Vec<String> = names
            .iter()
            .map(|name| match (passed.contains(name), default.contains(name)) {
                (true, _) => format!("{name} (given)"),
                (_, true) => format!("{name} (default)"),
                _ => name.clone(),
            })
            .collect();
        let shown = if all { names.len() } else { names.len().min(OFFERED_SHOWN) };
        let mut value = names[..shown].join(", ");
        if shown < names.len() {
            value.push_str(&format!(", +{}", names.len() - shown));
        }
        let mut record = place(analysis, span);
        record.insert("tag".into(), json!("offers"));
        record.insert("name".into(), json!(package));
        record.insert("count".into(), json!(names.len()));
        record.insert("value".into(), json!(value));
        out.push(record);
    }
    out
}

/// The values a load option `key=` takes, read off the code of the package and of the packages it
/// passes options to: what its options set the key to (`a4paper` is `paper=a4`), and the words its
/// key's code compares the value with (`\Ifstr{#1}{landscape}`).
pub fn option_values(analysis: &Analysis, package: &str, key: &str) -> Vec<String> {
    let set =
        Regex::new(&format!(r"(?:^|[,{{\s]){}\s*=\s*([A-Za-z0-9][\w.:-]*)", regex::escape(key))).expect("valid regex");
    let mut packages = BTreeSet::from([package.to_string()]);
    packages.extend(option_handler(analysis, package).1);
    // and what those load: KOMA's `paper=` values live in typearea
    let stem = |n: &str| n.trim_end_matches(".sty").trim_end_matches(".cls").to_string();
    loop {
        let more: Vec<String> = analysis
            .facts
            .loads
            .iter()
            .filter(|l| l.by.is_some_and(|b| packages.iter().any(|q| stem(q) == stem(analysis.interner.name(b)))))
            .map(|l| stem(&l.name))
            .filter(|n| !packages.iter().any(|q| stem(q) == *n))
            .collect();
        if more.is_empty() {
            break;
        }
        packages.extend(more);
    }
    let mut out = BTreeSet::new();
    for def in &analysis.facts.defs {
        let Some(mac) = def.mac.as_ref() else { continue };
        let body = detokenize(&mac.replacement_text, &analysis.interner);
        let name = analysis.interner.name(def.name);
        let ours = def.package.is_some_and(|p| {
            packages.iter().any(|q| {
                q.trim_end_matches(".sty").trim_end_matches(".cls")
                    == analysis.interner.name(p).trim_end_matches(".sty").trim_end_matches(".cls")
            })
        });
        if !ours {
            continue;
        }
        out.extend(set.captures_iter(&body).map(|c| c[1].to_string()));
        if name.ends_with(&format!("@{key}")) && name.contains("KV@") {
            out.extend(handler_values(analysis, &body));
            // `paper={a4paper}` handing its value to `\setkeys`: the family's flag keys are its values
            if body.contains("\\setkeys") && body.contains("{#1}") {
                out.extend(
                    package_options(analysis, package)
                        .into_iter()
                        .filter(|(_, d)| d.as_deref() == Some("true"))
                        .map(|(k, _)| k),
                );
            }
        }
    }
    out.into_iter().collect()
}

/// The words a key handler's `body` selects its value among: those it compares `#1` with
/// (`\Ifstr{#1}{landscape}`), the cases of a switch (`\lstKV@SwitchCases{#1}{fixed:…\\flexible:…}`),
/// and the names that follow the prefix of a `\csname⟨prefix⟩#1\endcsname` it calls (`lst@column@`:
/// the macros the run defined by that prefix).
fn handler_values(analysis: &Analysis, body: &str) -> BTreeSet<String> {
    static COMPARED: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static CASES: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static SELECT: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static CS: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let compared = COMPARED.get_or_init(|| Regex::new(r"\{#1\}\s*\{([A-Za-z][\w.-]*)\}").expect("valid regex"));
    let cases = CASES.get_or_init(|| Regex::new(r"SwitchCases\s*\{#+\d\}\s*\{").expect("valid regex"));
    let select = SELECT.get_or_init(|| {
        Regex::new(r"\\csname\s*((?:[^#\\{}]|\\[A-Za-z@]+\s?)*)#+\d\s*\\endcsname").expect("valid regex")
    });
    let cs = CS.get_or_init(|| Regex::new(r"\\([A-Za-z@]+)\s?").expect("valid regex"));
    let mut out: BTreeSet<String> = compared.captures_iter(body).map(|c| c[1].to_string()).collect();
    for m in cases.find_iter(body) {
        let (mut depth, rest) = (1, &body[m.end()..]);
        let end = rest.char_indices().find_map(|(i, c)| {
            depth += (c == '{') as i32 - (c == '}') as i32;
            (depth == 0).then_some(i)
        });
        let text = end.map_or(rest, |e| &rest[..e]);
        let word = |c: &str| c.split_once(':').map(|(w, _)| w.trim().to_string());
        out.extend(text.split("\\\\").filter_map(word).filter(|w| w.chars().all(|c| c.is_alphanumeric() || c == '-')));
    }
    for c in select.captures_iter(body) {
        // the prefix with the macros in it read as the text they hold (`\@lst` is `lst`)
        let mut known = true;
        let prefix = cs.replace_all(&c[1], |m: &regex::Captures| {
            let text = analysis.interner.lookup(&m[1]).and_then(|s| {
                Some(detokenize(&analysis.env.meaning(s).as_macro()?.replacement_text, &analysis.interner))
            });
            known &= text.is_some();
            text.unwrap_or_default()
        });
        let prefix: String = prefix.split_whitespace().collect();
        if !known || prefix.is_empty() {
            continue;
        }
        out.extend(analysis.facts.defs.iter().filter_map(|d| {
            let rest = analysis.interner.name(d.name).strip_prefix(&prefix)?;
            (!rest.is_empty() && rest.chars().all(|c| c.is_alphanumeric() || c == '-')).then(|| rest.to_string())
        }));
    }
    out
}

/// The values `cmd`'s key `key` takes when its handler is a keyval macro, as [`handler_values`]
/// reads them off the code the run stored for it.
pub fn key_values(analysis: &Analysis, cmd: &str, key: &str) -> Vec<String> {
    let records = options(analysis, cmd);
    let store = records.iter().find_map(|r| {
        (r.get("key")?.as_str()? == key).then_some(())?;
        Some(format!("{}{key}", r.get("store")?.as_str()?))
    });
    let Some(store) = store.filter(|s| s != key) else { return Vec::new() };
    let bodies = analysis.facts.defs.iter().filter(|d| analysis.interner.name(d.name) == store);
    let bodies = bodies.filter_map(|d| Some(detokenize(&d.mac.as_ref()?.replacement_text, &analysis.interner)));
    bodies.flat_map(|b| handler_values(analysis, &b)).collect::<BTreeSet<_>>().into_iter().collect()
}

/// The options `package` (a class or package name) declares, each with its
/// default when one is known: `\DeclareOption`'s, and the keys of the
/// family named after the package (kvoptions uses `\@currname`, xkeyval's
/// `\DeclareOptionX` `\@currname.\@currext`).
pub fn package_options(analysis: &Analysis, package: &str) -> Vec<(String, Option<String>)> {
    let mut out: BTreeMap<String, Option<String>> = BTreeMap::new();
    options_into(analysis, package, &mut BTreeSet::new(), &mut out);
    out.into_iter().collect()
}

/// `package`'s options into `out`, with those of every package its
/// `\DeclareOption*` handler passes `\CurrentOption` on to (`seen` breaks
/// cycles).
fn options_into(
    analysis: &Analysis,
    package: &str,
    seen: &mut BTreeSet<String>,
    out: &mut BTreeMap<String, Option<String>>,
) {
    if !seen.insert(package.to_string()) {
        return;
    }
    for def in &analysis.facts.defs {
        if def.tag == "option" && def.package.is_some_and(|p| analysis.interner.name(p) == package) {
            let name = def.subject.map(|s| analysis.interner.name(s)).unwrap_or_default();
            if name.starts_with(|c: char| c.is_ascii_alphanumeric()) && !analysis.settings.conventions.is_internal(name)
            {
                out.entry(name.to_string()).or_default();
            }
        }
    }
    let (families, passed) = option_handler(analysis, package);
    let own = ["", ".sty", ".cls"].map(|ext| format!("{package}{ext}"));
    for family in own.into_iter().chain(families).chain(member_families(analysis, package)) {
        for (key, _, default) in keyval_keys(analysis, &format!("KV@{family}@")) {
            out.entry(key).or_insert(default);
        }
    }
    for target in passed {
        options_into(analysis, &target, seen, out);
    }
}

/// The keyval families of a family `package` is a member of: scrbase's
/// `\DefineFamilyKey` stores `paper` of scrartcl as `\KV@KOMA.typearea.sty@paper`,
/// and the class processes every member of `KOMA`, the family its own
/// `KOMA.scrartcl.cls` names.
fn member_families(analysis: &Analysis, package: &str) -> BTreeSet<String> {
    let families: BTreeSet<&str> = analysis
        .facts
        .defs
        .iter()
        .filter_map(|d| analysis.interner.name(d.name).strip_prefix("KV@")?.split_once('@').map(|(f, _)| f))
        .collect();
    let own = [".sty", ".cls"].map(|ext| format!(".{package}{ext}"));
    let parents: BTreeSet<&str> = families.iter().filter_map(|f| own.iter().find_map(|o| f.strip_suffix(o))).collect();
    families
        .into_iter()
        .filter(|f| parents.iter().any(|p| f.strip_prefix(p).is_some_and(|m| m.starts_with('.'))))
        .map(str::to_string)
        .collect()
}

/// Where a package's unnamed options go through its `\DeclareOption*`
/// handler (`\default@ds`, clsguide § 4.5): the key families a dispatcher
/// in the package's code is handed `\CurrentOption`, or a macro the
/// handler names, for, and the packages or classes the handler passes
/// `\CurrentOption` on to. biblatex sets `blx@opt@ldt` keys at once and
/// collects the rest in `\blx@theoptions` for `blx@opt@pre`; graphicx
/// passes its options to graphics.
fn option_handler(analysis: &Analysis, package: &str) -> (Vec<String>, Vec<String>) {
    static NAME: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static DISPATCH: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static PASS: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    let name = NAME.get_or_init(|| Regex::new(r"\\[A-Za-z@]+").expect("valid regex"));
    let dispatch =
        DISPATCH.get_or_init(|| Regex::new(r"\\[A-Za-z@]*setkeys\s*\{([^{}]*)\}\s*\{([^{}]*)\}").expect("valid regex"));
    let pass = PASS.get_or_init(|| {
        Regex::new(r"\\PassOptionsTo(?:Package|Class)\s*(?:\{\s*\\CurrentOption\s*\}|\\CurrentOption)\s*\{([^{}]*)\}")
            .expect("valid regex")
    });
    let own = analysis.facts.defs.iter().filter(|d| d.package.is_some_and(|p| analysis.interner.name(p) == package));
    let bodies: Vec<(&str, String)> = own
        .filter_map(|d| {
            Some((analysis.interner.name(d.name), detokenize(&d.mac.as_ref()?.replacement_text, &analysis.interner)))
        })
        .collect();
    let mut named: BTreeSet<&str> = BTreeSet::from(["\\CurrentOption"]);
    let mut passed: Vec<String> = Vec::new();
    for (_, body) in bodies.iter().filter(|(n, _)| *n == "default@ds") {
        named.extend(name.find_iter(body).map(|m| m.as_str()));
        passed.extend(
            pass.captures_iter(body).flat_map(|c| c[1].split(',').map(|t| t.trim().to_string()).collect::<Vec<_>>()),
        );
    }
    // A package that declares no option of its own reads its option list itself (geometry's
    // `\ProcessOptionsKV{Gm}`): its options are the keys of the keyval families it defines
    let own: Vec<&crate::analysis::facts::Definition> = analysis
        .facts
        .defs
        .iter()
        .filter(|d| d.package.is_some_and(|p| analysis.interner.name(p) == package))
        .collect();
    let reads_options = !own.iter().any(|d| d.tag == "option");
    let mut out: Vec<String> = Vec::new();
    if reads_options {
        for d in &own {
            let family =
                analysis.interner.name(d.name).strip_prefix("KV@").and_then(|r| r.split_once('@')).map(|(f, _)| f);
            if let Some(family) = family.filter(|f| !out.iter().any(|o| o == f)) {
                out.push(family.to_string());
            }
        }
    }
    if named.len() == 1 {
        return (out, passed);
    }
    for (_, body) in &bodies {
        for c in dispatch.captures_iter(body) {
            let family = c[1].trim().to_string();
            if name.find_iter(&c[2]).any(|m| named.contains(m.as_str())) && !out.contains(&family) {
                out.push(family);
            }
        }
    }
    (out, passed)
}

/// `satex controls`: what each of `names` governs, or with none named the
/// document's switches (every one with `all`), in document order.  Several
/// names answer in one table: each row keeps the switch or option it
/// belongs to in `for`, so one answer is still traceable to the name that
/// asked for it.
pub fn controls_of(analysis: &Analysis, names: &[String], all: bool) -> Vec<Record> {
    let mut records = if names.is_empty() {
        switches(analysis, all)
    } else {
        let mut out = Vec::new();
        for name in names {
            for mut record in controls(analysis, name) {
                record.insert("for".into(), json!(crate::engine::tex::bare_name(name)));
                out.push(record);
            }
        }
        out
    };
    in_document_order(analysis, &mut records);
    records
}

pub fn controls(analysis: &Analysis, name: &str) -> Vec<Record> {
    let wanted = crate::analysis::facts::switches_named(
        &analysis.interner,
        &analysis.facts.defs,
        crate::engine::tex::bare_name(name),
    );
    let governed = |cds: &[crate::analysis::graph::ControlDep]| -> Option<String> {
        cds.iter().find_map(|cd| {
            let vertex = analysis.graph.vertex(cd.on)?;
            let rendered = analysis.interner.name(vertex.name);
            wanted.iter().any(|(w, _)| w == rendered).then(|| format!("\\{rendered} = {}", cd.taken))
        })
    };
    // What the name was taken to mean, said on every row it answers.
    let means = |detail: &str| -> Option<String> {
        let switch = detail.trim_start_matches('\\').split(' ').next()?;
        let (_, how) = wanted.iter().find(|(w, _)| w == switch).filter(|(_, how)| !how.is_empty())?;
        Some(format!("\\{switch}, {how}"))
    };
    let mut out = Vec::new();
    for def in &analysis.facts.defs {
        let Some(branch) = governed(&def.cds) else { continue };
        let mut record = place(analysis, def.span);
        if let Some(means) = means(&branch) {
            record.insert("means".into(), json!(means));
        }
        record.insert("tag".into(), json!(def.tag));
        record.insert("name".into(), json!(analysis.interner.cs(def.name)));
        record.insert("detail".into(), json!(branch));
        out.push(record);
    }
    for expansion in &analysis.facts.expansions {
        let Some(branch) = governed(&expansion.cds) else { continue };
        if matches!(
            analysis.env.meaning(expansion.name).prim(),
            Some(
                crate::engine::builtins::Primitive::Else
                    | crate::engine::builtins::Primitive::Or
                    | crate::engine::builtins::Primitive::Fi
                    | crate::engine::builtins::Primitive::If(_)
            )
        ) {
            continue;
        }
        let mut record = place(analysis, expansion.span);
        if let Some(means) = means(&branch) {
            record.insert("means".into(), json!(means));
        }
        record.insert("tag".into(), json!("use"));
        record.insert("name".into(), json!(analysis.interner.cs(expansion.name)));
        record.insert("detail".into(), json!(branch));
        out.push(record);
    }
    out
}

/// One file in the `satex summary` tree: what it defines, provides and
/// costs, with its `\input`/`\usepackage` children nested inside.
pub struct Node {
    pub name: String,
    pub path: String,
    pub kind: &'static str,
    pub status: &'static str,
    pub options: Vec<String>,
    pub identification: Option<String>,
    pub span: Option<Span>,
    pub provides: BTreeMap<&'static str, Vec<String>>,
    pub catcodes: Vec<String>,
    pub tokens: u64,
    pub millis: f64,
    /// Whether the tokens and time above are what a package-cache replay
    /// stands for rather than what this run counted token by token.
    pub cached: bool,
    pub children: Vec<Node>,
}

impl Node {
    fn empty(name: String, kind: &'static str) -> Node {
        Node {
            name,
            path: String::new(),
            kind,
            status: "read",
            options: Vec::new(),
            identification: None,
            span: None,
            provides: BTreeMap::new(),
            catcodes: Vec::new(),
            tokens: 0,
            millis: 0.0,
            cached: false,
            children: Vec::new(),
        }
    }

    fn to_json(&self) -> Json {
        json!({
            "name": self.name,
            "path": self.path,
            "kind": self.kind,
            "status": self.status,
            "options": self.options,
            "identification": self.identification,
            "provides": self.provides,
            "catcodes": self.catcodes,
            "tokens": self.tokens,
            "millis": self.millis,
            "cached": self.cached,
            "requires": self.children.iter().map(Node::to_json).collect::<Vec<_>>(),
        })
    }
}

pub fn summary(analysis: &Analysis) -> Node {
    let mut by_file: BTreeMap<FileId, Node> = BTreeMap::new();
    for (id, info) in analysis.files.iter().enumerate() {
        let id = id as FileId;
        let mut node = Node::empty(analysis.short_name(id).to_string(), info.kind.as_str());
        node.path = info.path.clone();
        node.cached = analysis.cached_files.contains(&id);
        by_file.insert(id, node);
    }

    for def in &analysis.facts.defs {
        let Some(node) = by_file.get_mut(&def.span.file) else { continue };
        let shown = def
            .subject
            .map(|s| analysis.interner.name(s).to_string())
            .unwrap_or_else(|| analysis.interner.cs(def.name));
        let slot = node.provides.entry(def.tag).or_default();
        if !slot.contains(&shown) {
            slot.push(shown);
        }
    }
    for occurrence in &analysis.facts.occurrences {
        let Some(node) = by_file.get_mut(&occurrence.span.file) else { continue };
        match occurrence.kind {
            OccKind::Identification => {
                node.identification = Some(occurrence.detail.clone().unwrap_or_default().trim().to_string())
            }
            OccKind::Catcode => {
                node.catcodes.push(format!("{}={}", occurrence.key, occurrence.detail.clone().unwrap_or_default()))
            }
            OccKind::Label | OccKind::Ref | OccKind::Cite | OccKind::BeginEnvironment => {
                let tag = match occurrence.kind {
                    OccKind::Label => "labels",
                    OccKind::Ref => "references",
                    OccKind::Cite => "citations",
                    _ => "environments used",
                };
                let slot = node.provides.entry(tag).or_default();
                if !slot.contains(&occurrence.key) {
                    slot.push(occurrence.key.clone());
                }
            }
            _ => {}
        }
    }
    for (id, elapsed, tokens) in analysis.timings.by_file() {
        if let Some(node) = by_file.get_mut(&id) {
            node.tokens = tokens;
            node.millis = elapsed.as_secs_f64() * 1000.0;
        }
    }

    let mut loads_in: BTreeMap<FileId, Vec<&Load>> = BTreeMap::new();
    for load in &analysis.facts.loads {
        loads_in.entry(load.span.file).or_default().push(load);
    }
    let mut reader: BTreeMap<FileId, Span> = BTreeMap::new();
    for load in &analysis.facts.loads {
        if let Some(child) = load.file
            && load.status == LoadStatus::Read
        {
            reader.entry(child).or_insert(load.span);
        }
    }
    let main = analysis.main_file;
    let mut open = Vec::new();
    attach(main, &mut by_file, &loads_in, &reader, &mut open)
        .unwrap_or_else(|| Node::empty(crate::analysis::machine::NO_FILE.into(), "input"))
}

fn attach(
    file: FileId,
    by_file: &mut BTreeMap<FileId, Node>,
    loads_in: &BTreeMap<FileId, Vec<&Load>>,
    reader: &BTreeMap<FileId, Span>,
    open: &mut Vec<FileId>,
) -> Option<Node> {
    let mut node = by_file.remove(&file)?;
    open.push(file);
    for load in loads_in.get(&file).into_iter().flatten() {
        let read_here = load.file.is_some_and(|child| reader.get(&child) == Some(&load.span) && !open.contains(&child));
        let mut child = match load.file.filter(|_| read_here) {
            Some(id) => attach(id, by_file, loads_in, reader, open),
            None => None,
        }
        .unwrap_or_else(|| Node::empty(load.name.clone(), load.kind.as_str()));
        child.options = load.options.clone();
        child.status = load.status.as_str();
        child.span = Some(load.span);
        child.kind = load.kind.as_str();
        node.children.push(child);
    }
    open.pop();
    Some(node)
}

fn identification(analysis: &Analysis, file: Option<FileId>) -> Option<String> {
    let file = file?;
    analysis
        .facts
        .occurrences
        .iter()
        .find(|occurrence| occurrence.kind == OccKind::Identification && occurrence.span.file == file)
        .and_then(|occurrence| occurrence.detail.clone())
        .map(|detail| detail.trim().to_string())
}

/// What kind of TeX file this is and what it was built with: document, class
/// or package, the `\documentclass` in force, and the engine `summary`
/// reports.
pub struct Identity<'a> {
    pub kind: &'static str,
    pub class: Option<String>,
    pub class_options: Vec<String>,
    pub engine: &'a str,
    pub engine_source: &'static str,
    pub project: Vec<String>,
}

pub fn identity(analysis: &Analysis) -> Identity<'_> {
    let main = analysis.main_file;
    let class = analysis.facts.loads.iter().find(|load| load.kind == LoadKind::Class && load.span.file == main);
    let body = analysis.document_depth.is_some();
    let context = analysis.plugins.kernel == crate::plugins::Kernel::Context;
    let kind = match analysis.files.get(main as usize).map(|file| file.kind) {
        // satex recognizes a ConTeXt document but does not interpret the
        // ConTeXt kernel, so it says no more than that it is one.
        _ if context => "ConTeXt document (experimental)",
        // A `.dtx` is documentation and code in one file, and a `.ins` is the
        // batch file that extracts the code: neither is a document, and what
        // satex read of a `.dtx` is what docstrip extracts from it.
        _ if analysis.literate.is_some() => {
            analysis.literate.map(crate::project::literate::Literate::as_str).unwrap_or_default()
        }
        Some(LoadKind::Package) => "LaTeX2e package",
        Some(LoadKind::Class) => "LaTeX2e document class",
        _ => match (class.is_some(), body) {
            (true, true) => "LaTeX2e document",
            (true, false) => "LaTeX2e preamble",
            (false, true) => "LaTeX2e document without \\documentclass",
            (false, false) => "TeX input",
        },
    };
    Identity {
        kind,
        class: class.map(|load| match identification(analysis, load.file) {
            Some(provided) => format!("{} [{provided}]", load.name),
            None => load.name.clone(),
        }),
        class_options: class.map(|load| load.options.clone()).unwrap_or_default(),
        engine: analysis.plugins.program.as_str(),
        engine_source: analysis.plugins.source(crate::plugins::Kind::Engine),
        project: analysis.project.describe(),
    }
}

pub fn summary_json(analysis: &Analysis) -> Json {
    let root = summary(analysis);
    let identity = identity(analysis);
    json!({
        "identified": identity.kind,
        "profile": analysis.settings.profile.map(crate::project::config::Profile::as_str),
        "class": identity.class,
        "class_options": identity.class_options,
        // The document properties, as the summary shows them and as the
        // token list they were made from.
        "metadata": analysis.metadata.iter().map(|entry| json!({
            "field": entry.field, "text": entry.text, "source": entry.source,
        })).collect::<Vec<_>>(),
        "engine": { "name": identity.engine, "from": identity.engine_source },
        "plugins": analysis.plugins.rows().iter().map(|(kind, value, source)| json!({
            "kind": kind.as_str(), "value": value, "from": source,
        })).collect::<Vec<_>>(),
        "project": identity.project,
        "distribution": analysis.distribution.describe(),
        "format": analysis.format.as_ref().map(|f| json!({
            "source": f.source, "definitions": f.definitions, "cached": f.cached,
        })),
        "files": analysis.files.len(),
        "tokens": analysis.steps,
        "document": root.to_json(),
    })
}

pub fn render_json(records: &[Record]) -> String {
    serde_json::to_string_pretty(records).unwrap_or_else(|e| e.to_string())
}

/// Whether a name reads as satex's own internal bookkeeping rather than
/// something a document ever spells: empty (a placeholder symbol with
/// nothing to show), or holding a character that cannot print on a line of
/// its own.
fn unprintable_name(name: &str) -> bool {
    name.is_empty() || name.chars().any(|c| c.is_control())
}

/// A name only a package's or the kernel's own internals would spell, by
/// `conventions.internal` (LaTeX2e's `@`, expl3's `:` and `__`).
fn internal_name(analysis: &Analysis, name: &str) -> bool {
    // A name a `\csname`-style construction built out of another one's own
    // spelling (`\@testopt`'s internal partner for `\section`, say) carries
    // a literal backslash as its first character; no document ever writes
    // that out by hand.
    // So does the robust partner `\X␣` of `\DeclareRobustCommand`.
    analysis.settings.conventions.is_internal(name) || name.starts_with('\\') || name.ends_with(' ')
}

/// Where a name in scope came from, for `scope`'s ordering and its
/// `package` field: the document's own project first, then the class and
/// packages it loaded, then the kernel — `tier` is the sort key, `origin`
/// what `package` shows.
fn origin_of(analysis: &Analysis, package: Option<Sym>, file: FileId) -> (u8, Json) {
    if origin(analysis, file) == "document" {
        return (0, json!("document"));
    }
    match package {
        Some(sym) => (1, json!(analysis.interner.name(sym))),
        // Installation content with no package attributed to it (a
        // package's own sub-file, an encoding definition file): named by
        // the file it came from rather than the bare word "package".
        None => (1, json!(analysis.short_name(file))),
    }
}

/// `satex scope`: the control sequences a completion at this position could
/// offer, one row per name — the meaning in force there, not a log of
/// everything the run ever touched. `all` adds the names a document never
/// spells: package and kernel internals (`internal_name`), and the ones the
/// tag whitelist does not recognize as a name a document would look up
/// (registers and the like, which crowd out the useful hundred with the
/// tens of thousands the kernel allocates for its own bookkeeping).
/// `does` is the one-line description of a documented kernel command.
pub fn scope(analysis: &Analysis, before: Option<(FileId, u32, u32)>, all: bool) -> Vec<Record> {
    scope_named(analysis, before, all, |_| true)
}

/// [`scope`] of the names `named` keeps.
pub fn scope_named(
    analysis: &Analysis,
    before: Option<(FileId, u32, u32)>,
    all: bool,
    named: impl Fn(&str) -> bool,
) -> Vec<Record> {
    let rows = analysis.scope_rows.get_or_init(|| crate::util::progress::unpaused(|| scope_rows(analysis)));
    let mut latest: BTreeMap<Sym, usize> = BTreeMap::new();
    // A definition in a file no user file reads (the kernel's, run by
    // `\begin{document}`) has no place there: it holds once the run has
    // passed the first call made at or after the position.
    let placed = |f: FileId| f == analysis.main_file || analysis.entry.get(f as usize).copied().flatten().is_some();
    let cut = before.map(|(file, line, col)| {
        analysis
            .facts
            .expansions
            .iter()
            .filter(|e| e.span.file == file && !e.span.before(line, col))
            .filter_map(|e| e.node)
            .min()
    });
    for (i, def) in analysis.facts.defs.iter().enumerate() {
        let visible = match before {
            None => true,
            Some((file, line, col)) if placed(def.span.file) => analysis.visible_in(def.span, file, line, col),
            Some(_) => cut.flatten().is_none_or(|cut| def.node < cut),
        };
        if visible && named(analysis.interner.name(def.name)) {
            latest.insert(def.name, i);
        }
    }
    let shown = |row: &&ScopeRow| all || row.plain;
    let row = |i: usize| {
        rows.defs[i].get_or_init(|| crate::util::progress::unpaused(|| def_row(analysis, &analysis.facts.defs[i])))
    };
    let defs = latest.values().filter_map(|&i| row(i).as_ref()).filter(shown);
    let rest = rows.rest.iter().filter(|r| named(&r.name)).filter(shown);
    let mut out: Vec<&ScopeRow> = defs.chain(rest).collect();
    out.sort_by(|a, b| {
        (a.tier, a.record.get("name").and_then(Json::as_str))
            .cmp(&(b.tier, b.record.get("name").and_then(Json::as_str)))
    });
    out.into_iter().map(|row| row.record.clone()).collect()
}

/// One name [`scope`] may show, its tier, and whether a document spells it.
#[derive(Clone)]
pub struct ScopeRow {
    name: String,
    tier: u8,
    record: Record,
    plain: bool,
}

/// What [`scope`] shows of each definition, made when a position first
/// shows it, then of each name only the run's end state holds: kept per
/// analysis, cut to a position per request.
#[derive(Clone)]
pub struct ScopeRows {
    defs: Vec<std::sync::OnceLock<Option<ScopeRow>>>,
    rest: Vec<ScopeRow>,
}

fn def_row(analysis: &Analysis, def: &crate::analysis::facts::Definition) -> Option<ScopeRow> {
    let name = analysis.interner.name(def.name);
    if unprintable_name(name) {
        return None;
    }
    let mut record = place(analysis, def.span);
    record.insert("name".into(), json!(analysis.interner.cs(def.name)));
    record.insert("tag".into(), json!(def.tag));
    record.insert("effective".into(), json!(effective_of(analysis, def.name, def.mac.as_deref(), None)));
    let (tier, origin) = origin_of(analysis, def.package, def.span.file);
    record.insert("package".into(), origin);
    let file = analysis.files.get(usize::from(def.span.file)).map(|f| std::path::PathBuf::from(&f.path));
    if let Some(does) = scope_does(analysis, name, &analysis.env.meaning(def.name), file) {
        record.insert("does".into(), json!(does));
    }
    Some(ScopeRow { name: name.to_string(), tier, record, plain: !internal_name(analysis, name) })
}

fn scope_rows(analysis: &Analysis) -> ScopeRows {
    let defs = analysis.facts.defs.iter().map(|_| std::sync::OnceLock::new()).collect();
    let recorded: BTreeSet<Sym> = analysis.facts.defs.iter().map(|d| d.name).collect();
    let mut rest = Vec::new();
    for i in 0..analysis.interner.len() as u32 {
        let sym = Sym(i);
        // A name the document defines later, but not yet at this position,
        // is not the kernel's meaning to show — `env.meaning` is the run's
        // final state, not this position's.
        if recorded.contains(&sym) {
            continue;
        }
        let meaning = analysis.env.meaning(sym);
        if matches!(meaning, Meaning::Undefined) {
            continue;
        }
        let name = analysis.interner.name(sym);
        let tag = meaning.kind();
        // Without `all`, only what reads as an invokable command: not the
        // registers and character meanings the kernel carries one of for
        // every number and character it has ever seen, which would
        // otherwise outnumber the useful primitives and macros many times
        // over.
        if unprintable_name(name) {
            continue;
        }
        let plain = !internal_name(analysis, name) && matches!(tag, "primitive" | "macro");
        // `\X` and `\endX` both macros: an environment the format made
        let base = name.strip_prefix("end").filter(|b| !b.is_empty()).unwrap_or(name);
        let partner = if base.len() == name.len() { format!("end{name}") } else { base.to_string() };
        let paired = tag == "macro"
            && analysis.interner.lookup(&partner).is_some_and(|p| analysis.env.meaning(p).as_macro().is_some());
        let mut record = Map::new();
        record.insert("name".into(), json!(analysis.interner.cs(sym)));
        record.insert("tag".into(), json!(if paired { "environment" } else { tag }));
        record.insert("effective".into(), json!(effective_of(analysis, sym, meaning.as_macro().map(|m| &**m), None)));
        record.insert("package".into(), json!("kernel"));
        if let Some(does) = scope_does(analysis, name, &meaning, None) {
            record.insert("does".into(), json!(does));
        }
        rest.push(ScopeRow { name: name.to_string(), tier: 2, record, plain });
    }
    ScopeRows { defs, rest }
}

/// How many arguments a command consumes, as a range: what its parameter
/// text says, plus what it reaches for when it dispatches on a star or on the
/// next character.  `max` is `null` when it is unbounded (`\csname`,
/// `\halign`); `render::plain` turns this back into `1-3` or `1+` for text,
/// CSV and Markdown output.
/// What this command was seen to read, numbered in the order it read it:
/// `{1}` a mandatory argument, `[2]` an optional one, `*` a star form.  The
/// interpreter records it while the command runs, so it describes what the
/// command really consumes rather than what a table says it should.
fn observed_shape(analysis: &Analysis, sym: crate::engine::tex::Sym) -> Option<String> {
    let shape = analysis.facts.shapes.get(&sym)?;
    let mut out = String::new();
    let mut items = shape.chars();
    let mut n = 0u8;
    while let Some(open) = items.next() {
        if open == '*' {
            out.push('*');
            continue;
        }
        let close = items.next()?;
        n += 1;
        let _ = write!(out, "{open}{n}{close}");
    }
    Some(out)
}

/// How many items a shape holds: a `*` stands on its own, and every other
/// item is the pair of delimiters around it, whatever they are — `xparse`
/// takes `D<>{…}` as readily as `O{…}`.
fn shape_items(shape: &str) -> usize {
    let stars = shape.matches('*').count();
    stars + (shape.chars().count() - stars) / 2
}

/// `min` counts the mandatory arguments in an observed shape, `max` every
/// item in it.
fn shape_bounds(shape: &str) -> (u8, Option<u8>) {
    let mandatory = shape.matches('{').count() as u8;
    (mandatory, Some(shape_items(shape) as u8))
}

/// What a macro takes: its own `ArgSpec` when satex reads it by one, and
/// otherwise what running it in a sandbox shows it consumes
/// ([`crate::analysis::probe`]) — at the end of the run, or at `\begin{document}`
/// with `preamble`.
pub fn signature(
    analysis: &Analysis,
    sym: crate::engine::tex::Sym,
    mac: &crate::engine::tex::MacroDef,
) -> Option<crate::engine::tex::ArgSpec> {
    probed(analysis, sym, Some(mac), Some(crate::analysis::probe::State::End)).filter(|p| p.complete).map(|p| p.spec)
}

/// What a call of the macro `sym` reads as the probe found it (`\\greet[hi]{2}`), for one name
/// where a listing gave only its definition's own text.
pub fn effective_probed(analysis: &Analysis, sym: crate::engine::tex::Sym) -> String {
    let meaning = analysis.env.meaning(sym);
    effective_of(analysis, sym, meaning.as_macro().map(|m| &**m), Some(crate::analysis::probe::State::End))
}

/// `view` is `None` where a listing reports many names at once and only
/// what the definition itself says is wanted: its ltcmd specification or
/// its parameter text.  Otherwise the probe says what a call reads.  Either
/// is written in one notation ([`crate::analysis::probe::raw_of`]).
fn probed(
    analysis: &Analysis,
    sym: crate::engine::tex::Sym,
    mac: Option<&crate::engine::tex::MacroDef>,
    view: Option<crate::analysis::probe::State>,
) -> Option<crate::analysis::probe::Probed> {
    if let Some(spec) = mac.and_then(|m| m.arg_spec.as_ref()) {
        return Some(crate::analysis::probe::Probed {
            spec: spec.clone(),
            complete: true,
            error: None,
            forms: Vec::new(),
        });
    }
    let own = mac
        .and_then(|m| crate::engine::tex::ArgSpec::from_parameter_text(&m.parameter_text, &analysis.interner))
        .map(|spec| crate::engine::tex::ArgSpec { raw: crate::analysis::probe::raw_of(&spec.items), ..spec });
    let Some(at) = view else {
        return own.map(|spec| crate::analysis::probe::Probed { spec, complete: true, error: None, forms: Vec::new() });
    };
    // A state the run did not keep: the end of the run's.
    let kept = match at {
        crate::analysis::probe::State::Preamble => analysis.preamble.is_some(),
        crate::analysis::probe::State::Start => analysis.preamble.is_some() && analysis.kernel.is_some(),
        crate::analysis::probe::State::End => true,
    };
    let at = if kept { at } else { crate::analysis::probe::State::End };
    // Where the run could not say, what the parameter text reads is certain.
    crate::analysis::probe::probe(analysis, sym, at).or_else(|| {
        own.map(|spec| crate::analysis::probe::Probed { spec, complete: false, error: None, forms: Vec::new() })
    })
}

/// How many arguments a command consumes, as `{min, max}`: the mandatory
/// items of its signature, and all of them; `max` is `null` when it is
/// unbounded (`\csname`, `\halign`) or the probe could not see the end.
/// A primitive's bounds are what satex was seen to read for it, or its
/// catalog entry.
/// The fewest arguments `def` takes.
pub fn takes_min(analysis: &Analysis, def: &crate::analysis::facts::Definition) -> Option<u8> {
    Some(takes_of(analysis, def.name, def.mac.as_deref(), None).get("min")?.as_u64()? as u8)
}

fn takes_of(
    analysis: &Analysis,
    sym: crate::engine::tex::Sym,
    mac: Option<&crate::engine::tex::MacroDef>,
    view: Option<crate::analysis::probe::State>,
) -> Json {
    let (mut low, mut high) = match analysis.env.meaning(sym).prim() {
        Some(prim) => match analysis.facts.shapes.get(&sym) {
            Some(shape) => shape_bounds(shape),
            None => crate::engine::builtins::takes(prim),
        },
        None => (0, Some(0)),
    };
    if let Some(mac) = mac {
        match probed(analysis, sym, Some(mac), view) {
            Some(p) => {
                use crate::engine::tex::ArgType as A;
                let arguments = p.spec.items.iter().filter(|i| !matches!(i, A::Literal(_)));
                low = arguments
                    .clone()
                    .filter(|i| {
                        matches!(i, A::Mandatory | A::Until(_) | A::Quantity(_) | A::Delimited { required: true, .. })
                    })
                    .count() as u8;
                high = p.complete.then_some(arguments.count() as u8);
            }
            None => {
                low = mac.parameter_text.arity;
                high = Some(low);
            }
        }
        // A call that was seen to read more than that settles it.
        if let Some((seen_low, seen_high)) = analysis.facts.shapes.get(&sym).map(|s| shape_bounds(s)) {
            low = low.max(seen_low);
            high = match (high, seen_high) {
                (Some(h), Some(seen)) => Some(h.max(seen)),
                _ => None,
            };
        }
    }
    json!({ "min": low, "max": high })
}

/// What a call to this command looks like: `*?` when it takes a star,
/// `[n]` for an optional argument (`[default]` when the default shows),
/// `{n}` for a mandatory one, `…` where the probe could not see the end.
/// A primitive the probe finds nothing for shows the bounds
/// `builtins::takes` gives it, mandatory arguments first.
fn effective_of(
    analysis: &Analysis,
    sym: crate::engine::tex::Sym,
    mac: Option<&crate::engine::tex::MacroDef>,
    view: Option<crate::analysis::probe::State>,
) -> String {
    use crate::engine::tex::ArgType as A;
    let cs = analysis.interner.cs(sym);
    let mut body = String::new();
    let mut star = false;
    let mut n = 0u8;
    let primitive = analysis.env.meaning(sym).prim().filter(|_| mac.is_none());
    // A definer reads a parameter text, which no probe can count: its
    // syntax is The TeXbook's.
    let definer = matches!(primitive, Some(crate::engine::builtins::Primitive::Def { .. }));
    if let Some(rest) = definer.then(|| crate::lsp::primitives::usage(crate::engine::tex::bare_name(&cs))).flatten() {
        return format!("{cs}{rest}");
    }
    let found = match primitive {
        // A primitive's syntax is not all arguments (`\def`, `\count0=`):
        // the probe counts only when it read some.
        Some(_) => probed(analysis, sym, None, view).filter(|p| p.complete && !p.spec.items.is_empty()),
        None => mac.and_then(|m| probed(analysis, sym, Some(m), view)),
    };
    match (found, mac, primitive) {
        (Some(p), ..) => {
            for item in &p.spec.items {
                match item {
                    A::Star => star = true,
                    A::Mandatory => {
                        n += 1;
                        let _ = write!(body, "{{{n}}}");
                    }
                    // its default shows, and it takes its parameter number all the same:
                    // `#1` of the types is the optional argument
                    A::Optional(Some(default)) => {
                        n += 1;
                        let _ = write!(body, "[{default}]");
                    }
                    A::TokenFlag(c) => {
                        let _ = write!(body, "{c}?");
                    }
                    A::Keyword { word, value } => {
                        let value = value.as_deref().map(|v| format!(" ⟨{v}⟩")).unwrap_or_default();
                        let _ = write!(body, " [{word}{value}]");
                    }
                    A::Quantity(value) if value == crate::analysis::probe::SWITCH => body.push_str(value),
                    A::Quantity(value) => {
                        let _ = write!(body, "⟨{value}⟩");
                    }
                    A::Delimited { open, close, required, default } => {
                        n += 1;
                        let mark = if *required { "" } else { "?" };
                        let _ = match default {
                            Some(d) => write!(body, "{open}{d}{close}{mark}"),
                            None => write!(body, "{open}{n}{close}{mark}"),
                        };
                    }
                    A::Embellishment(e) => {
                        for c in e.chars() {
                            n += 1;
                            let _ = write!(body, "{c}{{{n}}}?");
                        }
                    }
                    A::Optional(None) => {
                        n += 1;
                        let _ = write!(body, "[{n}]");
                    }
                    // `#{`: the `{` stays, for what follows to read.
                    A::Until(delimiter) => {
                        n += 1;
                        let delimiter = if delimiter == "{" { "" } else { delimiter };
                        let _ = write!(body, "{{{n}}}{delimiter}");
                    }
                    // A control word's name ends before the text, which
                    // the call writes after a space.
                    A::Literal(text) => {
                        let word = crate::engine::tex::bare_name(&cs).chars().count() > 1
                            || cs.chars().last().is_some_and(char::is_alphabetic);
                        if body.is_empty() && word && text.starts_with(char::is_alphabetic) {
                            body.push(' ');
                        }
                        body.push_str(text);
                    }
                }
            }
            if !p.complete {
                body.push('…');
            }
        }
        (None, Some(mac), _) => {
            for _ in 0..mac.parameter_text.arity {
                n += 1;
                let _ = write!(body, "{{{n}}}");
            }
        }
        // no complete probe: The TeXbook's syntax where one is known, not a count of arguments
        (None, None, Some(_)) if crate::lsp::primitives::usage(crate::engine::tex::bare_name(&cs)).is_some() => {
            return format!(
                "{cs}{}",
                crate::lsp::primitives::usage(crate::engine::tex::bare_name(&cs)).unwrap_or_default()
            );
        }
        (None, None, Some(prim)) => {
            let (low, high) = crate::engine::builtins::takes(prim);
            for i in 1..=low {
                let _ = write!(body, "{{{i}}}");
            }
            match high {
                Some(high) => {
                    for i in (low + 1)..=high {
                        let _ = write!(body, "[{i}]");
                    }
                }
                None => body.push_str("[..]"),
            }
        }
        (None, None, None) => {}
    }
    // A star that only some calls use is optional, not part of every call,
    // so it reads as `*?` (the way an optional argument reads as `[…]`).
    let declared = format!("{}{body}", if star { "*?" } else { "" });
    // What the command was seen to read wins when it read more than that.
    let seen = analysis.facts.shapes.get(&sym);
    let stated = declared.chars().filter(|c| matches!(c, '{' | '[' | '(' | '*')).count();
    if let (Some(shape), Some(observed)) = (seen, observed_shape(analysis, sym))
        && shape_items(shape) > stated
    {
        return format!("{cs}{observed}");
    }
    format!("{cs}{declared}")
}

/// What this macro's body actually expanded while it ran, distinct from
/// `calls` (the names its body merely names): every [`crate::analysis::facts::Expansion`]
/// recorded with this macro as its caller (`within`), deduplicated and
/// capped so the record stays a summary rather than a full trace.  Empty for
/// a macro this run never invoked, even one with calls in its body.  Split by
/// the body's conditional nesting, read from meanings: a name outside every
/// `\if…` arm always runs (`expands`), the outermost tests too, one inside
/// an arm may (`may_expand`).
fn insert_expands(record: &mut Record, analysis: &Analysis, sym: Sym, mac: &crate::engine::tex::MacroDef) {
    use crate::engine::builtins::Primitive as P;
    let mut always = BTreeSet::new();
    let mut depth = 0usize;
    for token in mac.replacement_text.iter() {
        let crate::engine::tex::Tok::Cs(name) = token.tok else { continue };
        match analysis.env.meaning(name) {
            Meaning::Primitive(P::If(_)) => {
                if depth == 0 {
                    always.insert(name);
                }
                depth += 1;
            }
            Meaning::Primitive(P::Fi) => depth = depth.saturating_sub(1),
            Meaning::Primitive(P::Else | P::Or) => {}
            _ if depth == 0 => {
                always.insert(name);
            }
            _ => {}
        }
    }
    let mut seen = BTreeSet::new();
    let (mut definite, mut potential) = (Vec::new(), Vec::new());
    for expansion in &analysis.facts.expansions {
        // A test is a callee; the arms' ends are not.
        let test = matches!(analysis.env.meaning(expansion.name), Meaning::Primitive(P::Else | P::Or | P::Fi));
        // a value the run did not know (`\year`'s digits) stands where a token was: no callee
        let value = analysis.interner.name(expansion.name).starts_with(crate::engine::tex::UNKNOWN_PREFIX);
        if expansion.within != Some(sym) || test || value || !seen.insert(expansion.name) || seen.len() > 12 {
            continue;
        }
        let list = if always.contains(&expansion.name) { &mut definite } else { &mut potential };
        list.push(json!(analysis.interner.cs(expansion.name)));
    }
    for (key, list) in [("expands", definite), ("may_expand", potential)] {
        if !list.is_empty() {
            record.insert(key.into(), json!(list));
        }
    }
}

/// The names the document's calls of `sym` define and leave defined after
/// the call: made globally or at the call's own group level (`\pgfmathparse`
/// leaves `\pgfmathresult`). `creates` lists the public ones, `creates_internal`
/// the internal ones by the naming conventions.
/// Up to how many created names a record lists, per kind.
const CREATED: usize = 24;

fn insert_creates(record: &mut Record, analysis: &Analysis, sym: Sym) {
    // A call that loads files is left out: what they define is theirs.
    let loading: BTreeSet<(FileId, u32, u32)> =
        analysis.facts.loads.iter().filter_map(|l| l.call).map(|c| (c.file, c.line, c.col)).collect();
    let mut calls: Vec<(crate::engine::env::NodeId, Sym, u16)> = analysis
        .facts
        .expansions
        .iter()
        .filter(|e| origin(analysis, e.span.file) == "document")
        .filter_map(|e| Some((e.node?, e.name, e.depth, (e.span.file, e.span.line, e.span.col))))
        .filter(|(.., at)| !loading.contains(at))
        .map(|(node, name, depth, _)| (node, name, depth))
        .collect();
    calls.sort_by_key(|c| c.0);
    let (mut public, mut internal) = (BTreeSet::new(), BTreeSet::new());
    for def in &analysis.facts.defs {
        let Some(&(_, call, depth)) = calls.get(calls.partition_point(|c| c.0 <= def.node).wrapping_sub(1)) else {
            continue;
        };
        if call != sym || def.name == sym || !(def.global || def.depth <= depth) {
            continue;
        }
        let name = analysis.interner.name(def.name);
        // A name no document spells as a control word or symbol (`\OT1/cmr/m/n/10`,
        // a font the selection loaded) is the code's own, like an internal one.
        let spelled = name.chars().count() == 1 || name.chars().all(char::is_alphabetic);
        match analysis.settings.conventions.is_internal(name) || !spelled {
            true => internal.insert(analysis.interner.cs(def.name)),
            false => public.insert(analysis.interner.cs(def.name)),
        };
    }
    for (key, names) in [("creates", public), ("creates_internal", internal)] {
        if !names.is_empty() {
            record.insert(key.into(), json!(names.into_iter().take(CREATED).collect::<Vec<_>>()));
        }
    }
}

/// `body` and, when the meaning is a primitive rather than a macro, the
/// `kind: "primitive"` marker: a primitive has no replacement text, so its
/// body is its own name, the way `\show` would report it (tex.web § 1294).
fn body_of(
    analysis: &Analysis,
    sym: crate::engine::tex::Sym,
    mac: Option<&crate::engine::tex::MacroDef>,
) -> (Json, bool) {
    match mac {
        Some(m) => (json!(detokenize(&m.replacement_text, &analysis.interner)), false),
        None => match analysis.env.meaning(sym) {
            // A name `\let` to a primitive (a `\newif` switch is `\iffalse`)
            // shows the primitive by its own name.
            Meaning::Primitive(p) => {
                let names = crate::engine::builtins::primitive_names(analysis.plugins.engine);
                let own = |n: &str| names.get(n) == Some(&p);
                let name = analysis.interner.name(sym);
                let shown = match own(name) {
                    true => name.to_string(),
                    false => names.keys().filter(|n| own(n)).min().cloned().unwrap_or_else(|| name.to_string()),
                };
                (json!(format!("\\{shown}")), true)
            }
            _ => (Json::Null, false),
        },
    }
}

/// Where a command is documented and a link that opens it: expl3 names in
/// interface3, a package's own manual on CTAN, a LaTeX2e command in the
/// kernel sources, and a primitive in the TeX reference manual, which has an
/// anchor per control sequence.  `kernel` is true for a name with no
/// definition site of its own (the interpreted kernel set) or one defined
/// inside the format file.
/// `"class"` for a name defined in a file `\documentclass` loaded, `"package"`
/// otherwise — the field `explain` labels the defining package or class
/// with, and what its `documentation` line calls it.
fn class_or_package(analysis: &Analysis, package: Option<Sym>, file: FileId) -> &'static str {
    match (package, analysis.files.get(file as usize).map(|f| f.kind)) {
        (Some(_), Some(LoadKind::Class)) => "class",
        _ => "package",
    }
}

fn documentation(
    analysis: &Analysis,
    sym: crate::engine::tex::Sym,
    package: Option<&str>,
    is_class: bool,
    meaning: &Meaning,
    kernel: bool,
) -> Option<(String, String)> {
    let name = analysis.interner.name(sym);
    if name.contains(':') {
        return Some(("interface3".into(), texdoc("interface3")));
    }
    if let Some(pkg) = package {
        let what = if is_class { "class" } else { "package" };
        return Some((format!("{what} {pkg}"), format!("https://ctan.org/pkg/{pkg}")));
    }
    if let Meaning::Primitive(p) = meaning {
        let source = crate::engine::builtins::reference(*p);
        // Every TeX primitive is named in lowercase (The TeXbook, chapter 3),
        // so a mixed-case name is a kernel command satex happens to model
        // with a primitive, and tex.web has no entry to point at.
        if source == "tex.web" && name.chars().any(char::is_uppercase) {
            return Some(("The LaTeX2e sources, source2e".into(), texdoc("source2e")));
        }
        return Some((source.to_string(), reference_link(source, name)));
    }
    if kernel {
        return Some(("The LaTeX2e sources, source2e".into(), texdoc("source2e")));
    }
    None
}

/// One line per primitive and documented kernel command: what it is for.
const DESCRIPTIONS: &str = include_str!("../data/describe.tsv");

/// The one-line description of a kernel or primitive command, by name
/// without the backslash.
pub fn describe(name: &str) -> Option<&'static str> {
    static TABLE: std::sync::OnceLock<HashMap<&'static str, &'static str>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| DESCRIPTIONS.lines().filter_map(|l| l.split_once('\t')).collect()).get(name).copied()
}

/// What `name` with this meaning does: a kernel macro that took a
/// primitive's name (`\end`) has a row of its own, `latex:end`; the
/// primitive's row never describes it.
fn describe_meaning(analysis: &Analysis, name: &str, meaning: &Meaning) -> Option<&'static str> {
    if meaning.as_macro().is_none() {
        return describe(name);
    }
    describe(&format!("latex:{name}")).or_else(|| {
        (!crate::engine::builtins::primitive_names(analysis.plugins.engine).contains_key(name))
            .then(|| describe(name))
            .flatten()
    })
}

/// What [`scope`] says `name` does: its description, else what the manual
/// of the package defining it (`file`, else the format) says, else what
/// the run made it: `\theX` prints counter `X` where `\c@X` is one.
fn scope_does(analysis: &Analysis, name: &str, meaning: &Meaning, file: Option<std::path::PathBuf>) -> Option<String> {
    if let Some(does) = describe_meaning(analysis, name, meaning) {
        return Some(does.to_string());
    }
    let file = file.or_else(|| crate::caches::distribution::tex_file(&analysis.distribution.roots, "latex.ltx"));
    let mined = file
        .filter(|_| analysis.settings.plugins.docs && meaning.as_macro().is_some())
        .and_then(|f| crate::plugins::docs::index(&analysis.settings.docs).lookup_now(&f, name)?.text);
    mined.or_else(|| {
        let counter = name.strip_prefix("the")?;
        let reg = analysis.interner.lookup(&format!("c@{counter}"))?;
        matches!(analysis.env.meaning(reg), Meaning::Register(crate::engine::tex::RegKind::Count, _))
            .then(|| format!("The printed value of counter {counter}."))
    })
}

/// Adds the `documentation`/`reference` link and, when the name has one, the
/// `does` description.
fn document(
    record: &mut Record,
    analysis: &Analysis,
    sym: crate::engine::tex::Sym,
    package: Option<&str>,
    is_class: bool,
    meaning: &Meaning,
    kernel: bool,
) {
    if let Some((doc, link)) = documentation(analysis, sym, package, is_class, meaning, kernel) {
        record.insert("documentation".into(), json!(doc));
        record.insert("reference".into(), json!(link));
        if let Some(text) = describe_meaning(analysis, analysis.interner.name(sym), meaning) {
            record.insert("does".into(), json!(text));
        }
    }
}

/// A manual on texdoc.org, which serves the same PDF a local `texdoc` opens.
pub fn texdoc(name: &str) -> String {
    format!("https://texdoc.org/serve/{name}/0")
}

/// The page that documents a reference satex names, with the anchor for this
/// command where the source has one per control sequence.
fn reference_link(source: &str, name: &str) -> String {
    match source {
        // The TeX reference manual gives every primitive its own anchor.
        "tex.web" => format!("https://www.tug.org/utilities/plain/cseq.html#{name}-rp"),
        "The TeXbook, chapter 20" => "https://www.tug.org/texbook.html".into(),
        "The eTeX manual" => texdoc("etex_man"),
        "interface3" => texdoc("interface3"),
        "The keyval and l3keys documentation" => texdoc("keyval"),
        _ => texdoc("source2e"),
    }
}

/// Whether `file` is the format's own source, so a definition found there is
/// the kernel's, not the document's or a package's.
fn is_kernel_file(analysis: &Analysis, file: crate::engine::tex::FileId) -> bool {
    analysis.format.as_ref().is_some_and(|f| analysis.file_name(file) == f.source)
}

/// A record for one recorded [`crate::analysis::facts::Definition`]: what it is, what
/// it takes, what it does — the branch `explain` uses when the run recorded
/// a `\def`-family site for the name.
/// Which part of the run a record describes: all of it, or the preamble or
/// the document body when the two see different meanings.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Window {
    All,
    Preamble,
    Document,
    /// The document's start, where a name means what the format made it.
    Start,
}

impl Window {
    fn view(self) -> Option<crate::analysis::probe::State> {
        use crate::analysis::probe::State;
        Some(match self {
            Window::Preamble => State::Preamble,
            Window::Start => State::Start,
            Window::All | Window::Document => State::End,
        })
    }
}

/// How often `sym` was expanded within `window`.
fn uses_in(analysis: &Analysis, sym: Sym, window: Window) -> u32 {
    let boundary = explain_boundary(analysis, &ExplainAt::Preamble).ok().flatten();
    analysis
        .facts
        .expansions
        .iter()
        .filter(|e| e.name == sym)
        .filter(|e| match (window, boundary, e.node) {
            (Window::All, ..) | (_, None, _) => true,
            (Window::Preamble | Window::Start, Some(b), Some(n)) => n < b,
            (Window::Document, Some(b), Some(n)) => n >= b,
            (_, Some(_), None) => false,
        })
        .map(|e| e.count)
        .sum()
}

/// The other names the call that made `def` defined: the names a
/// `\newcounter{x}` (`\c@x`, `\thex`, …) or a `\newtcolorbox` makes
/// together. Package code's definitions stand at the document's call that
/// ran it, so they share its place; the document's own `\def`s each stand
/// where they are written.
fn defined_together(analysis: &Analysis, def: &crate::analysis::facts::Definition) -> Vec<String> {
    let mut out: Vec<String> = analysis
        .facts
        .defs
        .iter()
        .filter(|d| d.name != def.name && d.span == def.span)
        .map(|d| analysis.interner.cs(d.name))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// What the comment block above a definition says, as `comment` and per
/// argument `arguments` (`% #1 is the name`).
fn insert_comment(record: &mut Record, analysis: &Analysis, at: Span) {
    let Some(doc) = source_comment(analysis, at) else { return };
    record.insert("comment".into(), json!(doc.text));
    let args: Vec<String> = doc.params.iter().map(|(n, text)| format!("⟨arg{n}⟩ {text}")).collect();
    if !args.is_empty() {
        record.insert("arguments".into(), json!(args));
    }
}

/// The comment block above a definition in the project's own files, read
/// from the source.
fn source_comment(analysis: &Analysis, at: Span) -> Option<crate::lsp::doccomment::DocComment> {
    if !analysis.project_file(at.file) {
        return None;
    }
    let path = analysis.files.get(usize::from(at.file))?.path.clone();
    let text = crate::project::overlay::read_to_string(Path::new(&path)).ok()?;
    crate::lsp::doccomment::above(&text, at.line as usize)
}

fn record_for_def(analysis: &Analysis, def: &crate::analysis::facts::Definition, window: Window) -> Record {
    let mut record = place(analysis, def.span);
    record.insert("name".into(), json!(analysis.interner.cs(def.name)));
    record.insert("tag".into(), json!(def.tag));
    record.insert("by".into(), json!(analysis.interner.cs(def.by)));
    insert_comment(&mut record, analysis, def.span);
    let together = defined_together(analysis, def);
    if !together.is_empty() {
        record.insert("together".into(), json!(together));
    }
    if analysis.facts.defs.iter().any(|d| d.name == def.name && d.via == Some(def.name) && d.node > def.node) {
        record.insert("once".into(), json!(true));
    }
    // A class is loaded the same way a package is (`def.package` names
    // either), but reads oddly as "package article" for a `\documentclass`;
    // the file it came from says which it really is.
    let kind = class_or_package(analysis, def.package, def.span.file);
    record.insert(kind.into(), symbol(analysis, def.package));
    record.insert("arity".into(), json!(def.arity()));
    insert_call_shape(&mut record, analysis, def.name, def.mac.as_deref(), window.view(), true);
    record.insert("certain".into(), json!(def.certain));
    record.insert("context".into(), json!(def_context_label(analysis, &[(def.context.to_vec(), def.via)])));
    let (body, primitive) = body_of(analysis, def.name, def.mac.as_deref());
    record.insert("body".into(), body);
    if primitive {
        record.insert("kind".into(), json!("primitive"));
    }
    record.insert("uses".into(), json!(uses_in(analysis, def.name, window)));
    // What this run actually expanded while executing the body, not what
    // the body merely names (that duplicated this and is what the `calls`
    // query is for).
    if let Some(mac) = def.mac.as_deref() {
        insert_expands(&mut record, analysis, def.name, mac);
        insert_creates(&mut record, analysis, def.name);
    }
    let package_name = def.package.map(|p| analysis.interner.name(p));
    let kernel = is_kernel_file(analysis, def.span.file);
    document(&mut record, analysis, def.name, package_name, kind == "class", &analysis.env.meaning(def.name), kernel);
    record
}

/// The numbered arguments of a signature, by their place in the usage line
/// (`effective_of` counts the same).
fn argument_slots(items: &[crate::engine::tex::ArgType]) -> Vec<(usize, &crate::engine::tex::ArgType)> {
    use crate::engine::tex::ArgType as A;
    let mut n = 0;
    let mut out = Vec::new();
    for item in items {
        match item {
            A::Mandatory | A::Optional(_) | A::Delimited { .. } | A::Until(_) => {
                n += 1;
                out.push((n, item));
            }
            A::Embellishment(e) => n += e.chars().count(),
            _ => {}
        }
    }
    out
}

/// What a call of `sym` reads: `takes`, `signature`, `effective`, the
/// `error` the probe raised if any, and with `forms` the forms it found.
fn insert_call_shape(
    record: &mut Record,
    analysis: &Analysis,
    sym: Sym,
    mac: Option<&crate::engine::tex::MacroDef>,
    view: Option<crate::analysis::probe::State>,
    forms: bool,
) {
    record.insert("takes".into(), takes_of(analysis, sym, mac, view));
    // A primitive the TeXbook gives a syntax for shows it; one that reads a
    // parameter text (`\def`) has no argument shape a probe could find.
    let name = analysis.interner.name(sym);
    let syntax =
        crate::lsp::primitives::usage(name).filter(|_| matches!(analysis.env.meaning(sym), Meaning::Primitive(_)));
    if let Some(rest) = syntax {
        record.insert("syntax".into(), json!(format!("\\{name}{rest}")));
    }
    let probe = probed(analysis, sym, mac, view).filter(|_| !syntax.is_some_and(|s| s.contains("⟨parameter text⟩")));
    record.insert(
        "signature".into(),
        match &probe {
            Some(p) if p.complete => json!(p.spec.raw),
            Some(p) => json!(format!("{}…", p.spec.raw)),
            None => Json::Null,
        },
    );
    if let Some(p) = probe.as_ref().filter(|p| forms && !p.forms.is_empty()) {
        record.insert("forms".into(), json!(p.forms.iter().map(|f| f.raw.clone()).collect::<Vec<_>>()));
    }
    let roles = crate::analysis::roles::roles(analysis, analysis.interner.name(sym));
    // Roles and types name the braced arguments; the record names every
    // argument by its place in the usage line.
    let slots = argument_slots(probe.as_ref().map_or(&[][..], |p| &p.spec.items[..]));
    // A braced group stands for the next item that reads one: a numbered
    // argument, or a name the call defines (`\newcommand{\x}`), which has
    // no number and takes no type here.
    let braced_items: Vec<Option<usize>> = probe
        .as_ref()
        .map_or(&[][..], |p| &p.spec.items[..])
        .iter()
        .filter_map(|item| {
            use crate::engine::tex::ArgType as A;
            match item {
                A::Mandatory | A::Until(_) => slots.iter().find(|(_, i)| std::ptr::eq(*i, item)).map(|(n, _)| Some(*n)),
                A::Quantity(k) if k == "cs" => Some(None),
                _ => None,
            }
        })
        .collect();
    let place_of_braced = |braced: usize| braced_items.get(braced).copied().unwrap_or(Some(braced + 1));
    let place = |braced: usize| place_of_braced(braced).unwrap_or(0);
    let named: Vec<String> = roles
        .iter()
        .filter(|r| !matches!(r.kind, crate::analysis::roles::RoleKind::Type(_)))
        .map(|r| {
            let at = place_of_braced(r.arg).map_or("⟨cs⟩".to_string(), |n| format!("#{n}"));
            ToString::to_string(r).replacen(&format!("#{}", r.arg + 1), &at, 1)
        })
        .collect();
    if !named.is_empty() {
        record.insert("roles".into(), json!(named));
    }
    use crate::analysis::roles::{RoleKind, Ty};
    let mut types: Vec<(usize, Ty)> = crate::analysis::roles::arg_types(&roles)
        .into_iter()
        .filter_map(|(arg, t)| Some((place_of_braced(arg)?, t)))
        .collect();
    // What the run read in the `[…]` arguments, each at its optional slot.
    let optional: Vec<usize> = slots
        .iter()
        .filter(|(_, i)| {
            matches!(
                i,
                crate::engine::tex::ArgType::Optional(_)
                    | crate::engine::tex::ArgType::Delimited { required: false, .. }
            )
        })
        .map(|(n, _)| *n)
        .collect();
    for (j, t) in crate::analysis::roles::optional_types(analysis, analysis.interner.name(sym)) {
        types.extend(optional.get(j).map(|n| (*n, t)));
    }
    // The options a loading command takes before the name it loads.
    for r in roles.iter().filter(|r| {
        matches!(
            r.kind,
            RoleKind::Load(crate::engine::builtins::LoadKind::Package | crate::engine::builtins::LoadKind::Class)
        )
    }) {
        let name = place(r.arg);
        if let Some((n, _)) = slots.iter().rev().find(|(n, i)| {
            *n < name
                && matches!(
                    i,
                    crate::engine::tex::ArgType::Optional(_)
                        | crate::engine::tex::ArgType::Delimited { required: false, .. }
                )
        }) {
            types.push((*n, Ty::Options));
        }
    }
    // The release a command asks for in the optional argument its options do not fill
    // (`\documentclass[⟨options⟩]{⟨class⟩}[⟨date⟩]`, `\NeedsTeXFormat{LaTeX2e}[⟨date⟩]`).
    if crate::analysis::roles::takes_release(analysis, analysis.interner.name(sym))
        && let Some(n) = optional.iter().rev().find(|n| !types.iter().any(|(m, _)| m == *n))
    {
        types.push((*n, Ty::Date));
    }
    // A control sequence the call defines as a conditional (`\newif`).
    let mut place_of = 0;
    for item in probe.as_ref().map_or(&[][..], |p| &p.spec.items[..]) {
        use crate::engine::tex::ArgType as A;
        match item {
            A::Quantity(k) if k == crate::analysis::probe::SWITCH => types.push((place_of + 1, Ty::Switch)),
            A::Mandatory | A::Optional(_) | A::Delimited { .. } | A::Until(_) => place_of += 1,
            A::Embellishment(e) => place_of += e.chars().count(),
            _ => {}
        }
    }
    types.sort_by_key(|(n, _)| *n);
    types.dedup_by_key(|(n, _)| *n);
    if !types.is_empty() {
        record.insert("types".into(), json!(types.iter().map(|(arg, t)| format!("#{arg} {t}")).collect::<Vec<_>>()));
    }
    record.insert("effective".into(), json!(effective_of(analysis, sym, mac, view)));
    // what the expansion gives where TeX reads a quantity (`\the\c@page`: a number)
    if let Some(ty) = mac.and_then(|m| crate::analysis::roles::yields(analysis, m)) {
        record.insert("yields".into(), json!(ty.to_string()));
    }
    if let Some(error) = probe.and_then(|p| p.error) {
        record.insert("error".into(), json!(error));
    }
}

/// The run called `sym` and no call raised an error: its uses decide, not a
/// probe that calls it in text mode outside any environment, where
/// `\mathcal` errs though every use, in math, did not.
fn went_through(analysis: &Analysis, sym: Sym) -> bool {
    analysis.facts.expansions.iter().any(|e| e.name == sym && e.node.is_some())
        && !analysis.facts.raised.iter().any(|r| r.name == sym)
}

/// A record for a name with no recorded definition site: a kernel primitive
/// or macro this run never redefined, reported from its final meaning —
/// the branch `explain` falls back to.
fn record_for_kernel(analysis: &Analysis, sym: Sym, window: Window) -> Option<Record> {
    record_for_meaning(analysis, sym, window, analysis.env.meaning(sym))
}

/// [`record_for_kernel`] of the meaning `sym` has somewhere else than at
/// the end of the run.
fn record_for_meaning(analysis: &Analysis, sym: Sym, window: Window, meaning: Meaning) -> Option<Record> {
    if matches!(meaning, Meaning::Undefined) {
        return None;
    }
    let mut record = Map::new();
    record.insert("name".into(), json!(analysis.interner.cs(sym)));
    record.insert("tag".into(), json!(meaning.kind()));
    record.insert("package".into(), json!("kernel"));
    // Where the kernel defined it, when the format remembers; otherwise the
    // format's own source, which is all satex knows for a name it models
    // itself.
    match analysis.kernel_sites.get(&sym) {
        Some(span) => {
            let mut place = place(analysis, *span);
            record.append(&mut place);
        }
        None => match analysis.format.as_ref() {
            Some(f) => {
                record.insert("path".into(), json!(f.source));
                let short = std::path::Path::new(&f.source).file_name().and_then(|n| n.to_str()).unwrap_or(&f.source);
                record.insert("file".into(), json!(short));
            }
            None => {
                record.insert("path".into(), Json::Null);
            }
        },
    }
    // The arguments the command reads itself: a macro's parameter text, or,
    // for a primitive, what satex was seen to read for it.  `takes` adds
    // what the commands it dispatches to read on top of that.
    let arity = match meaning.as_macro() {
        Some(mac) => mac.arity(),
        None => analysis.facts.shapes.get(&sym).map(|shape| shape_bounds(shape).1.unwrap_or(0)).unwrap_or_else(|| {
            meaning
                .prim()
                .map_or(0, |p| crate::engine::builtins::takes(p).1.unwrap_or(crate::engine::builtins::takes(p).0))
        }),
    };
    record.insert("arity".into(), json!(arity));
    record.insert("context".into(), json!(where_label(analysis, &[Vec::new()])));
    insert_call_shape(&mut record, analysis, sym, meaning.as_macro().map(|m| &**m), window.view(), false);
    let (body, primitive) = body_of(analysis, sym, meaning.as_macro().map(|m| &**m));
    record.insert("body".into(), body);
    if primitive {
        record.remove("tag");
        record.insert("kind".into(), json!("primitive"));
    }
    if let Some(mac) = meaning.as_macro() {
        insert_expands(&mut record, analysis, sym, mac);
        insert_creates(&mut record, analysis, sym);
    }
    document(&mut record, analysis, sym, None, false, &meaning, true);
    Some(record)
}

/// A `satex explain` location suffix, `NAME@LOCATION`: which of a name's
/// possibly several definitions to report, the way `latexdef` lets a
/// document pick where it asks.  `explain_at` resolves it to a graph node;
/// definitions at or before that node are in force there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExplainAt {
    /// `document`: the meaning at the end of the run — the same one shown
    /// without a suffix when there is only one, asked for explicitly.
    Doc,
    /// `preamble`: the meaning right before `\begin{document}`.
    Preamble,
    /// `after:NAME`: the meaning right after this package or class
    /// finished contributing what it defined.
    After(String),
    /// `LINE` or `FILE:LINE`: the meaning in force at that position,
    /// the main file unless another one is named.
    At(At),
}

impl std::fmt::Display for ExplainAt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExplainAt::Doc => write!(f, "document"),
            ExplainAt::Preamble => write!(f, "preamble"),
            ExplainAt::After(name) => write!(f, "after {name}"),
            ExplainAt::At(at) => write!(f, "{at}"),
        }
    }
}

/// Where `explain --at` asks: `preamble`, `document`, a line of the main
/// file, `FILE:LINE`, or `after:PACKAGE`.
pub fn parse_explain_at(loc: &str) -> Result<ExplainAt, String> {
    let at = match loc {
        "document" | "doc" => Some(ExplainAt::Doc),
        "preamble" => Some(ExplainAt::Preamble),
        _ if loc.starts_with("after:") && loc.len() > 6 => Some(ExplainAt::After(loc["after:".len()..].to_string())),
        _ if !loc.is_empty() && loc.chars().all(|c| c.is_ascii_digit()) => {
            loc.parse().ok().map(|line| ExplainAt::At(At { file: None, line, col: u32::MAX, end: None }))
        }
        _ => loc.rsplit_once(':').and_then(|(file, line)| {
            (!file.is_empty())
                .then(|| line.parse().ok())
                .flatten()
                .map(|line| ExplainAt::At(At { file: Some(file.to_string()), line, col: u32::MAX, end: None }))
        }),
    };
    at.ok_or_else(|| format!("`{loc}` is no place: use preamble, document, LINE, FILE:LINE or after:PACKAGE"))
}

/// The graph node a location names: every definition at or before it is in
/// force there.  `Ok(None)` means no cutoff — the end of the run, where
/// every recorded definition is in force.
fn explain_boundary(analysis: &Analysis, at: &ExplainAt) -> Result<Option<NodeId>, String> {
    match at {
        ExplainAt::Doc => Ok(None),
        ExplainAt::Preamble => analysis
            .facts
            .occurrences
            .iter()
            .find(|o| o.kind == OccKind::BeginEnvironment && o.key == "document")
            .map(|o| Some(o.node))
            .ok_or_else(|| "no `\\begin{document}` in this run, so `--at preamble` names no position".into()),
        ExplainAt::After(pkg) => {
            let name_is_pkg = |p: Option<Sym>| p.is_some_and(|p| analysis.interner.name(p) == pkg);
            analysis
                .facts
                .defs
                .iter()
                .filter(|d| name_is_pkg(d.package))
                .map(|d| d.node)
                .chain(analysis.facts.occurrences.iter().filter(|o| name_is_pkg(o.package)).map(|o| o.node))
                .max()
                .map(Some)
                .ok_or_else(|| format!("`{pkg}` was not loaded in this run, so `--at after:{pkg}` names no position"))
        }
        ExplainAt::At(at) => {
            let nodes = at_position(analysis, at);
            nodes.into_iter().min().map(Some).ok_or_else(|| format!("nothing at {at}"))
        }
    }
}

/// The definitions of `sym` the use at `node` read on some path, in order:
/// more than one where paths that defined it differently joined.
fn defs_reaching(analysis: &Analysis, sym: Sym, node: NodeId) -> Vec<&crate::analysis::facts::Definition> {
    let read: std::collections::HashSet<NodeId> = analysis
        .graph
        .outgoing(node)
        .iter()
        .filter(|(_, kind)| kind.intersects(crate::analysis::graph::EdgeKind::READS))
        .map(|(n, _)| *n)
        .collect();
    let mut defs: Vec<_> = analysis.facts.defs.iter().filter(|d| d.name == sym && read.contains(&d.node)).collect();
    defs.sort_by_key(|d| d.node);
    defs
}

fn def_body(analysis: &Analysis, def: &crate::analysis::facts::Definition) -> String {
    def.mac.as_ref().map(|m| detokenize(&m.replacement_text, &analysis.interner)).unwrap_or_default()
}

/// The latest definition of `sym` at or before `boundary`; `None` for the
/// boundary means no cutoff, so the latest definition overall.
fn def_at(analysis: &Analysis, sym: Sym, boundary: Option<NodeId>) -> Option<&crate::analysis::facts::Definition> {
    let defs = || analysis.facts.defs.iter().filter(|d| d.name == sym).filter(|d| boundary.is_none_or(|b| d.node <= b));
    defs().filter(|d| d.via != Some(sym)).max_by_key(|d| d.node).or_else(|| defs().max_by_key(|d| d.node))
}

/// The environments open at `node`, outermost first, replayed from the
/// `BeginEnvironment`/`EndEnvironment` occurrences the run recorded up to
/// it: what a name was actually called under, generically, from running
/// the document rather than from a table of environment names.
fn context_at(analysis: &Analysis, node: NodeId) -> Vec<Sym> {
    let mut occurrences: Vec<&crate::analysis::facts::Occurrence> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| {
            matches!(o.kind, OccKind::BeginEnvironment | OccKind::EndEnvironment)
                && o.key != "document"
                && o.node <= node
        })
        .collect();
    occurrences.sort_by_key(|o| o.node);
    let mut stack: Vec<Sym> = Vec::new();
    for o in occurrences {
        let Some(sym) = analysis.interner.lookup(&o.key) else { continue };
        match o.kind {
            OccKind::BeginEnvironment => stack.push(sym),
            OccKind::EndEnvironment => {
                if let Some(pos) = stack.iter().rposition(|s| *s == sym) {
                    stack.remove(pos);
                }
            }
            _ => {}
        }
    }
    stack
}

/// One environment stack, as `explain` shows it: the names it holds,
/// outermost to innermost.
fn context_path(analysis: &Analysis, ctx: &[Sym]) -> String {
    ctx.iter().map(|s| analysis.interner.name(*s)).collect::<Vec<_>>().join(" > ")
}

/// Several environment stacks a meaning holds under, as one label, hedged
/// with `"e.g."` — this is what specific calls or probes actually saw,
/// never the full story a name's *definitions* can give (`def_context_label`
/// derives that from the mechanism instead): `"outside environments"`,
/// `"e.g. in enumerate, itemize"`, or a mix of both.
fn where_label(analysis: &Analysis, contexts: &[Vec<Sym>]) -> String {
    let outside = contexts.iter().any(|c| c.is_empty());
    let rest: Vec<String> = contexts.iter().filter(|c| !c.is_empty()).map(|c| context_path(analysis, c)).collect();
    match (outside, rest.is_empty()) {
        (true, true) => "outside environments".to_string(),
        (true, false) => format!("outside environments, e.g. in {}", rest.join(", ")),
        (false, _) => format!("e.g. in {}", rest.join(", ")),
    }
}

/// Environments whose static reach (`crate::analysis::probe`'s call-graph closure)
/// includes `via` — every environment that could run the code that made a
/// definition while `via` was expanding, generically: `\list` is reached by
/// `itemize`, `enumerate`, `description`, and whatever else is built on it,
/// found from the run's own call graph, never a table of environment names.
/// How far an environment's begin-code is walked to see whether it reaches
/// `via`: shallow, so a mechanism specific to a few environments (`\list`)
/// is told apart from generic machinery nearly everything reaches
/// eventually (`\@ifdefinable`, on the way to almost any `\newcommand`).
const MECHANISM_DEPTH: usize = 2;
const MECHANISM_CAP: usize = 200;

fn reaching_environments(analysis: &Analysis, via: Sym) -> Vec<Sym> {
    crate::analysis::probe::all_environments(analysis)
        .into_iter()
        .filter(|&env| {
            env == via
                || crate::analysis::probe::closure_bounded(analysis, env, MECHANISM_DEPTH, MECHANISM_CAP).contains(&via)
        })
        .collect()
}

/// How many other environments a `"via"` mechanism names before `"…"`.
const MECHANISM_SHOWN: usize = 3;

/// One definition's context as `explain` names it: the mechanism behind it
/// when the macro expanding at the time (`via`) is itself reached by more
/// than one environment (`"environments via \list: itemize, enumerate,
/// description, …"`), that macro alone when it is the only environment
/// known to reach it (`"loud"`), or, lacking either, the plain environment
/// stack that was open (`ctx`) — `None` for "outside any environment".
fn context_piece(analysis: &Analysis, ctx: &[Sym], via: Option<Sym>) -> Option<String> {
    if ctx.is_empty() {
        return None;
    }
    if let Some(via) = via {
        let mut reaching = reaching_environments(analysis, via);
        reaching.retain(|&e| e != via);
        if !reaching.is_empty() {
            let shown: Vec<&str> = reaching.iter().take(MECHANISM_SHOWN).map(|s| analysis.interner.name(*s)).collect();
            let mut label = format!("environments via \\{}: {}", analysis.interner.name(via), shown.join(", "));
            if reaching.len() > MECHANISM_SHOWN {
                label.push_str(", …");
            }
            return Some(label);
        }
    }
    Some(context_path(analysis, ctx))
}

/// Several (context, mechanism) pairs a *definition* holds under, as one
/// label: `def_context_label` derives the fullest story it can — a
/// mechanism (`"in environments via \list: …"`) where the run's own call
/// graph gives one, the plain environment otherwise — unlike [`where_label`],
/// which only ever reports the one instance a call or probe happened to see.
fn def_context_label(analysis: &Analysis, entries: &[(Vec<Sym>, Option<Sym>)]) -> String {
    let mut outside = false;
    let mut pieces: Vec<String> = Vec::new();
    for (ctx, via) in entries {
        match context_piece(analysis, ctx, *via) {
            None => outside = true,
            Some(piece) => {
                let piece = format!("in {piece}");
                if !pieces.contains(&piece) {
                    pieces.push(piece);
                }
            }
        }
    }
    match (outside, pieces.is_empty()) {
        (true, true) => "outside environments".to_string(),
        (true, false) => format!("outside environments, {}", pieces.join(", ")),
        (false, _) => pieces.join(", "),
    }
}

/// What a `\def`-family definition's replacement amounts to, for telling
/// two definitions with the same meaning apart from two with different
/// ones: a macro's own body, or a coarse tag for anything else (a register,
/// a switch) satex does not detokenize.
fn meaning_identity(analysis: &Analysis, def: &crate::analysis::facts::Definition) -> String {
    match &def.mac {
        Some(m) => detokenize(&m.replacement_text, &analysis.interner),
        None => format!("<{}>", def.tag),
    }
}

/// A name's distinct meanings, told apart by the environment context each
/// definition was made in, when that context varies — `None` when every
/// definition of `sym` was made in the same place, so the preamble/document
/// split already tells its (at most two) meanings apart.  Definitions that
/// read the same are one meaning, shown once, with every context it holds
/// under.
fn context_meanings(analysis: &Analysis, sym: Sym) -> Option<Vec<Record>> {
    let mut defs: Vec<&crate::analysis::facts::Definition> =
        analysis.facts.defs.iter().filter(|d| d.name == sym).collect();
    if defs.is_empty() {
        return None;
    }
    defs.sort_by_key(|d| d.node);
    let all_same = defs.iter().all(|d| d.context.as_ref() == defs[0].context.as_ref());
    let has_outside = defs.iter().any(|d| d.context.is_empty());
    // One definition made outside any environment is exactly what the
    // preamble/document split already covers.
    if defs.len() < 2 && has_outside {
        return None;
    }
    if defs.len() >= 2 && all_same {
        return None;
    }
    let mut groups: Vec<(String, Vec<&crate::analysis::facts::Definition>)> = Vec::new();
    for def in &defs {
        let id = meaning_identity(analysis, def);
        match groups.iter_mut().find(|(k, _)| *k == id) {
            Some((_, ds)) => ds.push(def),
            None => groups.push((id, vec![def])),
        }
    }
    let mut records: Vec<Record> = groups
        .into_iter()
        .map(|(_, ds)| {
            let mut record = record_for_def(analysis, ds[ds.len() - 1], Window::All);
            let entries: Vec<(Vec<Sym>, Option<Sym>)> = ds.iter().map(|d| (d.context.to_vec(), d.via)).collect();
            record.insert("context".into(), json!(def_context_label(analysis, &entries)));
            record
        })
        .collect();
    // Every recorded definition sits inside some environment — kernel code
    // (`\list`'s own `\def\@itemlabel{#1}`) re-run at the same site never
    // shows what it means outside one — so the baseline the probe finds
    // there, outside any environment, is a meaning of its own.
    if !has_outside && let Some(base) = record_for_kernel(analysis, sym, Window::All) {
        records.push(base);
    }
    Some(records)
}

/// Environment-context meanings for a name with no redefinition of its
/// own, when calling it did something different depending on what was
/// open at the time (`\item` outside a list versus inside one built on
/// `\trivlist`): one record per distinct error the run actually raised —
/// or did not — across every context the name was really called in.
/// `None` when the run only ever called it in one context, so `base` is
/// the whole story.
fn split_by_environment(analysis: &Analysis, sym: Sym, base: &Record) -> Option<Vec<Record>> {
    let mut contexts: Vec<Vec<Sym>> = vec![Vec::new()];
    for e in analysis.facts.expansions.iter().filter(|e| e.name == sym) {
        if let Some(node) = e.node {
            let ctx = context_at(analysis, node);
            if !contexts.contains(&ctx) {
                contexts.push(ctx);
            }
        }
    }
    if contexts.len() <= 1 {
        return None;
    }
    let errors = contexts.into_iter().map(|ctx| {
        let error = analysis
            .facts
            .raised
            .iter()
            .find(|r| r.name == sym && r.context.as_ref() == ctx.as_slice())
            .map(|r| r.text.clone());
        (error, ctx)
    });
    meanings_by_error(analysis, base, errors)
}

/// Environment-context meanings for a name with no real use to read them
/// off of (a run with no `\item` inside a list, say): found by really
/// probing the command from inside every environment
/// [`crate::analysis::probe::probe_contexts`] found worth trying, and comparing what
/// each raises against `base`'s own (probed outside any environment)
/// error. `None` when none of them differed.
fn probe_context_meanings(analysis: &Analysis, sym: Sym, base: &Record) -> Option<Vec<Record>> {
    // The bare call hands its `.` to a mandatory argument: what it raises
    // then is about that argument (`\end{.}`), as in the signature probe.
    if base.get("takes").and_then(|t| t.get("min")).and_then(Json::as_u64).unwrap_or(0) > 0 {
        return None;
    }
    let errors = crate::analysis::probe::probe_contexts(analysis, sym)
        .into_iter()
        .map(|(env, error)| (error, env.map_or_else(Vec::new, |e| vec![e])));
    meanings_by_error(analysis, base, errors)
}

/// One copy of `base` per distinct error among `errors`, each labelled with
/// every context that raised it.  `None` when they all raised the same.
fn meanings_by_error(
    analysis: &Analysis,
    base: &Record,
    errors: impl Iterator<Item = (Option<String>, Vec<Sym>)>,
) -> Option<Vec<Record>> {
    let mut groups: Vec<(Option<String>, Vec<Vec<Sym>>)> = Vec::new();
    for (error, ctx) in errors {
        match groups.iter_mut().find(|(e, _)| *e == error) {
            Some((_, ctxs)) => ctxs.push(ctx),
            None => groups.push((error, vec![ctx])),
        }
    }
    if groups.len() <= 1 {
        return None;
    }
    Some(
        groups
            .into_iter()
            .map(|(error, ctxs)| {
                let mut record = base.clone();
                match error {
                    Some(text) => {
                        record.insert("error".into(), json!(text));
                    }
                    None => {
                        record.remove("error");
                    }
                }
                record.insert("context".into(), json!(where_label(analysis, &ctxs)));
                record
            })
            .collect(),
    )
}

/// `satex explain`: what one or more names mean, from the recorded
/// definition site closest to where it is asked about, or the kernel's own
/// meaning for a name this run never redefined.
///
/// Without a `@location` suffix and without `all`, a name with several
/// definitions is shown twice at most: its meaning right before
/// `\begin{document}` and its meaning at the end of the run, when those
/// differ (a package's definition survives to become the one shown alone
/// far more often than not, so the common case is still one record).  A
/// `@location` suffix asks for one specific meaning instead; `all` lists
/// every recorded definition.  Returns the records plus a hint per name
/// that had definitions left out, for the caller to show as a footer.
pub fn explain(analysis: &Analysis, names: &[String], all: bool) -> Result<(Vec<Record>, Vec<String>), String> {
    explain_at(analysis, names, all, None)
}

/// [`explain`] at one place instead of the preamble/document view.
pub fn explain_at(
    analysis: &Analysis,
    names: &[String],
    all: bool,
    at: Option<&ExplainAt>,
) -> Result<(Vec<Record>, Vec<String>), String> {
    let mut out = Vec::new();
    let mut hints = Vec::new();
    for raw in names {
        let name = crate::engine::tex::bare_name(raw);
        let Some(sym) = analysis.interner.lookup(name) else { continue };
        let total = analysis.facts.defs.iter().filter(|d| d.name == sym).count();
        if let Some(at) = at {
            let boundary = explain_boundary(analysis, at)?;
            let window = if matches!(at, ExplainAt::Preamble) { Window::Preamble } else { Window::All };
            let nodes = match at {
                ExplainAt::At(a) => at_position(analysis, a),
                _ => boundary.into_iter().collect(),
            };
            let mut reached: Vec<_> = nodes.into_iter().flat_map(|n| defs_reaching(analysis, sym, n)).collect();
            reached.sort_by_key(|d| d.node);
            reached.dedup_by_key(|d| d.node);
            // The definition the use read beats the latest one before it: a group may have restored it.
            match reached.last().copied().or_else(|| def_at(analysis, sym, boundary)) {
                Some(def) => {
                    let mut record = record_for_def(analysis, def, window);
                    record.insert("when".into(), json!(at.to_string()));
                    if reached.len() > 1 {
                        let one_of: Vec<serde_json::Value> = reached
                            .iter()
                            .map(|d| json!({ "line": d.span.line, "body": def_body(analysis, d) }))
                            .collect();
                        record.insert("one_of".into(), json!(one_of));
                    }
                    out.push(record);
                }
                // No definition at that point: the kernel's own meaning is
                // constant throughout the run, so it still answers.
                None => {
                    if let Some(mut record) = record_for_kernel(analysis, sym, window) {
                        record.insert("when".into(), json!(at.to_string()));
                        out.push(record);
                    }
                }
            }
            continue;
        }
        if all {
            let mut defs: Vec<&crate::analysis::facts::Definition> =
                analysis.facts.defs.iter().filter(|d| d.name == sym).collect();
            defs.sort_by_key(|d| d.node);
            if defs.is_empty() {
                out.extend(record_for_kernel(analysis, sym, Window::All));
            } else {
                out.extend(defs.into_iter().map(|def| record_for_def(analysis, def, Window::All)));
            }
            continue;
        }
        let before = out.len();
        // Definitions made in different environments are different
        // meanings on their own axis; the preamble/document split below is
        // only reached when that axis has nothing to say.
        if let Some(meanings) = context_meanings(analysis, sym) {
            out.extend(meanings);
        } else {
            let doc_def = def_at(analysis, sym, None);
            let preamble_boundary = explain_boundary(analysis, &ExplainAt::Preamble).ok().flatten();
            let preamble_call = preamble_boundary.and_then(|b| {
                analysis
                    .facts
                    .expansions
                    .iter()
                    .filter(|e| e.name == sym && e.span.file == analysis.main_file)
                    .filter_map(|e| e.node)
                    .filter(|n| *n <= b)
                    .max()
            });
            let preamble_def = preamble_boundary.and_then(|b| def_at(analysis, sym, Some(preamble_call.unwrap_or(b))));
            match (preamble_def, doc_def) {
                (None, None) => {
                    let mut base = record_for_kernel(analysis, sym, Window::All);
                    // Real uses in the run, when there were any of note;
                    // otherwise the same question asked of the probe, so a
                    // run with no `\item` inside a list still finds one, and
                    // its error stands only where no use decided.
                    let decided = went_through(analysis, sym);
                    if decided && let Some(b) = &mut base {
                        b.remove("error");
                    }
                    let split = base.as_ref().and_then(|b| {
                        split_by_environment(analysis, sym, b)
                            .or_else(|| (!decided).then(|| probe_context_meanings(analysis, sym, b)).flatten())
                    });
                    match split {
                        Some(split) => out.extend(split),
                        None => out.extend(base),
                    }
                }
                // Only one of the two exists, or they agree: nothing to tell
                // apart, so `when` would only be noise on the one record shown.
                // Used in the preamble with the meaning the format gave it,
                // which the run later replaced (`\documentclass` by an
                // error): both, each where it holds.
                (None, Some(doc))
                    if preamble_call.is_some()
                        && analysis.kernel.as_ref().is_some_and(|kernel| {
                            !matches!(kernel.meaning(sym), Meaning::Undefined)
                                && doc.mac.as_deref() != kernel.meaning(sym).as_macro().map(|m| &**m)
                        }) =>
                {
                    let kernel = analysis.kernel.as_ref().map(|k| k.meaning(sym)).unwrap_or(Meaning::Undefined);
                    if let Some(mut record) = record_for_meaning(analysis, sym, Window::Start, kernel) {
                        record.insert("when".into(), json!("preamble"));
                        out.push(record);
                    }
                    let mut record = record_for_def(analysis, doc, Window::Document);
                    record.insert("when".into(), json!("document"));
                    out.push(record);
                }
                (None, Some(def)) | (Some(def), None) => out.push(record_for_def(analysis, def, Window::All)),
                (Some(a), Some(b)) if a.node == b.node => out.push(record_for_def(analysis, b, Window::All)),
                (Some(preamble), Some(doc)) => {
                    let mut record = record_for_def(analysis, preamble, Window::Preamble);
                    record.insert("when".into(), json!("preamble"));
                    out.push(record);
                    let mut record = record_for_def(analysis, doc, Window::Document);
                    record.insert("when".into(), json!("document"));
                    out.push(record);
                }
            }
        }
        let shown = out.len() - before;
        if total > shown {
            if let Some(first) = out.get_mut(before) {
                first.insert("redefined".into(), json!(redefinitions(analysis, sym, total)));
            }
            hints.push(format!("`\\{name}` has {total} definitions. `--all` lists every one"));
        }
    }
    Ok((out, hints))
}

/// `N× [file:line, …]`: where a name was defined, the first few places.
fn redefinitions(analysis: &Analysis, sym: Sym, total: usize) -> String {
    const SHOWN: usize = 3;
    let mut places: Vec<String> = Vec::new();
    for def in analysis.facts.defs.iter().filter(|d| d.name == sym) {
        let place = format!("{}:{}", analysis.short_name(def.span.file), def.span.line);
        if !places.contains(&place) {
            places.push(place);
        }
    }
    let more = if places.len() > SHOWN { ", …" } else { "" };
    places.truncate(SHOWN);
    format!("{total}× [{}{more}]", places.join(", "))
}

/// How much of the text a record matched in its `detail` shows.
const EXCERPT: usize = 120;

/// A one-line, bounded view of the text a match sits in.
fn excerpt(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(EXCERPT) {
        Some((at, _)) => format!("{}…", &flat[..at]),
        None => flat,
    }
}

/// What the documentation of each package and class the run read says
/// about its commands ([`crate::plugins::docs`]): a record per entry, `dir`
/// the documentation directory mined for `package_file`. The playground's build keeps them as
/// `.satex-docs` files there, for a bundle without the sources.
pub fn docs(analysis: &Analysis) -> Vec<Record> {
    use crate::plugins::docs::{Dirs, PLUGINS, mine, mined};
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    for file in &analysis.files {
        let path = Path::new(&file.path);
        if !path.extension().is_some_and(|e| e == "sty" || e == "cls") {
            continue;
        }
        let Some(dirs) = Dirs::of(&analysis.settings.docs, path).filter(|d| seen.insert(d.clone())) else { continue };
        let Some(dir) = dirs.doc.clone().or_else(|| dirs.source.clone()) else { continue };
        for entry in mine(PLUGINS, &analysis.settings.docs, &dirs).values() {
            let Json::Object(mut record) = mined::row(entry) else { continue };
            record.insert("dir".into(), json!(dir.display().to_string()));
            record.insert("package_file".into(), json!(file.path));
            out.push(record);
        }
    }
    let key = |r: &Record| {
        (r["dir"].as_str().unwrap_or_default().to_string(), r["name"].as_str().unwrap_or_default().to_string())
    };
    out.sort_by_key(key);
    out
}

/// Each name (with or without its backslash) and the package or class of the
/// installation that defines it loading the fewest files
/// ([`crate::caches::exports::Exports::providers`]); a name no file defines is left out.
pub fn providers(analysis: &Analysis, names: &[String]) -> Vec<Record> {
    let exports = crate::caches::exports::Exports::of(
        &analysis.distribution.roots,
        crate::caches::format::default_cache_dir().as_deref(),
        &crate::caches::exports::definers(analysis, &[]),
    );
    names
        .iter()
        .filter_map(|name| {
            let bare = name.trim_start_matches('\\').to_string();
            let (file, _) = exports.providers(std::slice::from_ref(&bare)).into_iter().next()?;
            let mut record = Record::new();
            record.insert("name".into(), json!(format!("\\{bare}")));
            record.insert("package".into(), json!(file.trim_end_matches(".sty")));
            Some(record)
        })
        .collect()
}

/// Everything that can produce a piece of text, whatever it takes to get
/// there: `satex query produces --text 1.5cm`.
///
/// The search text is a literal, case-sensitive substring.  `1.5cm`,
/// `\textbf` and `(a|b)` all stand for themselves — no metacharacters, no
/// word boundaries — so a magic number reads here the way it reads in the
/// source.  `--filter` narrows the records afterwards, and its `~` operator
/// is where a regular expression belongs.
///
/// `kind` says how the text comes about:
///
/// * `source` — it stands in a file the run read, expansion or no expansion;
///   `tag` says what kind of file, or `comment` where the line comments it out.
/// * `macro body` — a replacement text carries it, reported at the definition.
/// * `value` — a register, counter or length holds it, as the value prints:
///   a dimension in points, whatever unit was written.
/// * `occurrence` — a recorded key or detail is it: a label, a citation, a
///   file name, a section title.
/// * `metadata` — the expanded `\title`, `\author` or `\date`.
/// * `expansion` — a call the run expanded is it, or took it as an argument.
pub fn produces(analysis: &Analysis, source: &str, text: &str) -> Vec<Record> {
    if text.is_empty() {
        return Vec::new();
    }
    let mut out = literal_source(analysis, source, text);
    // Where each name was last given what it holds: an assignment where the
    // graph recorded one (`\setlength` makes no definition), the definition
    // site otherwise.
    let mut sites: BTreeMap<Sym, Span> = BTreeMap::new();
    for def in &analysis.facts.defs {
        sites.insert(def.name, def.span);
        let Some(mac) = &def.mac else { continue };
        if unprintable_name(analysis.interner.name(def.name)) {
            continue;
        }
        let body = detokenize(&mac.replacement_text, &analysis.interner);
        if !body.contains(text) {
            continue;
        }
        let mut record = place(analysis, def.span);
        record.insert("kind".into(), json!("macro body"));
        record.insert("tag".into(), json!(def.tag));
        record.insert("name".into(), json!(analysis.interner.cs(def.name)));
        record.insert("detail".into(), json!(excerpt(&body)));
        out.push(record);
    }
    for vertex in &analysis.graph.vertices {
        if vertex.tag == crate::analysis::graph::VertexTag::VariableDefinition {
            sites.insert(vertex.name, vertex.span);
        }
    }
    for i in 0..analysis.interner.len() as u32 {
        let sym = Sym(i);
        let value = analysis.env.value(sym);
        if matches!(value, crate::engine::value::Value::Unknown) {
            continue;
        }
        // A token register holds text rather than a number, and that text is
        // what it produces.
        let rendered = match &value {
            crate::engine::value::Value::Toks(tokens) => detokenize(tokens, &analysis.interner),
            other => other.render(),
        };
        if !rendered.contains(text) {
            continue;
        }
        let mut record = match sites.get(&sym) {
            Some(span) => place(analysis, *span),
            None => Map::new(),
        };
        record.insert("kind".into(), json!("value"));
        record.insert("tag".into(), json!(analysis.env.meaning(sym).kind()));
        record.insert("name".into(), json!(analysis.interner.cs(sym)));
        record.insert("detail".into(), json!(rendered));
        out.push(record);
    }
    for occurrence in &analysis.facts.occurrences {
        let detail = occurrence.detail.as_deref().unwrap_or_default();
        let matched = if occurrence.key.contains(text) {
            &occurrence.key
        } else if detail.contains(text) {
            detail
        } else {
            continue;
        };
        let mut record = place(analysis, occurrence.span);
        record.insert("kind".into(), json!("occurrence"));
        record.insert("tag".into(), json!(occurrence.kind.as_str()));
        record.insert("name".into(), json!(occurrence.key));
        record.insert("detail".into(), json!(excerpt(matched)));
        out.push(record);
    }
    for entry in &analysis.metadata {
        if !entry.text.contains(text) && !entry.source.contains(text) {
            continue;
        }
        // `\title` and friends store their argument in `\@title`, which is
        // where the document said it.
        let field = entry.field;
        let site = analysis.interner.lookup(&format!("@{field}")).and_then(|sym| sites.get(&sym));
        let mut record = match site {
            Some(span) => place(analysis, *span),
            None => Map::new(),
        };
        record.insert("kind".into(), json!("metadata"));
        record.insert("tag".into(), json!(field));
        record.insert("name".into(), json!(format!("\\{field}")));
        record.insert("detail".into(), json!(excerpt(&entry.text)));
        // What the text was made of, so the rendering loses nothing.
        record.insert("source".into(), json!(excerpt(&entry.source)));
        out.push(record);
    }
    for use_ in &analysis.facts.expansions {
        let name = analysis.interner.cs(use_.name);
        let argument = use_.arguments.iter().map(|a| detokenize(a, &analysis.interner)).find(|a| a.contains(text));
        let matched = match (name.contains(text), argument) {
            (_, Some(argument)) => argument,
            (true, None) => name.clone(),
            (false, None) => continue,
        };
        let mut record = place(analysis, use_.span);
        record.insert("kind".into(), json!("expansion"));
        record.insert("tag".into(), json!(use_.meaning.as_str()));
        record.insert("name".into(), json!(name));
        record.insert("detail".into(), json!(excerpt(&matched)));
        out.push(record);
    }
    out
}

/// Where the text stands in the source itself, in every file the run read.
/// The main file's text is the one the run was given, which may have come
/// from standard input; the rest are read back from disk.
fn literal_source(analysis: &Analysis, source: &str, text: &str) -> Vec<Record> {
    let mut out = Vec::new();
    for id in 0..analysis.files.len() as FileId {
        let read;
        let body = if id == analysis.main_file && !source.is_empty() {
            source
        } else {
            match std::fs::read_to_string(analysis.file_name(id)) {
                Ok(content) => {
                    read = content;
                    &read
                }
                Err(_) => continue,
            }
        };
        let kind = analysis.files[id as usize].kind.as_str();
        for (number, line) in body.lines().enumerate() {
            let comment = crate::engine::tex::comment_start(line);
            let mut from = 0;
            while let Some(found) = line[from..].find(text) {
                let at = from + found;
                let col = line[..at].chars().count() as u32 + 1;
                let mut record = place(analysis, Span::new(id, number as u32 + 1, col));
                record.insert("kind".into(), json!("source"));
                record.insert(
                    "tag".into(),
                    json!(if comment.is_some_and(|start| at >= start) { "comment" } else { kind }),
                );
                record.insert("detail".into(), json!(excerpt(line)));
                out.push(record);
                from = at + text.len();
            }
        }
    }
    out
}

/// The pgfkeys a run left registered.  pgfkeys keeps a key as control
/// sequences named after its path (pgfmanual, "Key Management"): the value
/// of `/a/b` in `\pgfk@/a/b`, its code in `\pgfk@/a/b/.@cmd` and its default
/// in `\pgfk@/a/b/.@def`.  The handler that made the key is read off the code
/// pgfkeys stored for it: `.style` stores `\pgfkeysalso{…}`, `.is if` a call
/// of `\pgfkeys@handle@boolean`, `.is choice` code that defines
/// `\pgfkeys@was@choice`, `.is family` code that sets `\pgfkeysdefaultpath`.
pub fn pgfkeys(analysis: &Analysis, prefix: Option<&str>) -> Vec<Record> {
    const STORE: &str = "pgfk@";
    #[derive(Default)]
    struct Key {
        value: Option<Sym>,
        code: Option<Sym>,
        default: Option<Sym>,
        args: Option<Sym>,
    }
    let mut keys: BTreeMap<String, Key> = BTreeMap::new();
    for index in 0..analysis.interner.len() as u32 {
        let sym = Sym(index);
        let Some(stored) = analysis.interner.name(sym).strip_prefix(STORE) else { continue };
        if !stored.starts_with('/') || !matches!(analysis.env.meaning(sym), Meaning::Macro(_)) {
            continue;
        }
        // `\newif\ifpgfk@/a/familyactive` makes the setters
        // `\pgfk@/a/familyactivetrue` and `…false`, which are not keys.
        let setter = ["true", "false"].iter().any(|suffix| {
            stored.strip_suffix(suffix).is_some_and(|stem| {
                analysis.interner.lookup(&format!("if{STORE}{stem}")).is_some_and(|s| analysis.env.is_defined(s))
            })
        });
        if setter {
            continue;
        }
        let (path, part) = match stored.rsplit_once("/.@") {
            Some((path, part)) => (path, Some(part)),
            None => (stored, None),
        };
        if prefix.is_some_and(|prefix| !path.starts_with(prefix)) {
            continue;
        }
        let key = keys.entry(path.to_string()).or_default();
        match part {
            None => key.value = Some(sym),
            Some("cmd") => key.code = Some(sym),
            Some("def") => key.default = Some(sym),
            Some("args") => key.args = Some(sym),
            Some(_) => {}
        }
    }
    let body = |sym: Sym| match analysis.env.meaning(sym) {
        Meaning::Macro(m) => Some(m),
        _ => None,
    };
    let text = |sym: Option<Sym>| match sym.and_then(body) {
        Some(m) => json!(detokenize(&m.replacement_text, &analysis.interner)),
        None => Json::Null,
    };
    keys.into_iter()
        .filter(|(_, key)| key.value.is_some() || key.code.is_some())
        .map(|(path, key)| {
            let code = key.code.and_then(body);
            let first = code.as_ref().and_then(|m| m.replacement_text.first().and_then(|t| t.cs()));
            let first = first.map(|s| analysis.interner.name(s)).unwrap_or_default();
            let handler = match (&code, first) {
                (None, _) => "initial",
                (Some(_), "pgfkeysalso") => "style",
                (Some(_), "pgfkeys@handle@boolean") => "is if",
                (Some(m), "def")
                    if m.replacement_text
                        .get(1)
                        .and_then(|t| t.cs())
                        .is_some_and(|s| analysis.interner.name(s) == "pgfkeys@was@choice") =>
                {
                    "is choice"
                }
                (Some(m), "edef")
                    if m.replacement_text
                        .get(1)
                        .and_then(|t| t.cs())
                        .is_some_and(|s| analysis.interner.name(s) == "pgfkeysdefaultpath") =>
                {
                    "is family"
                }
                (Some(_), _) if key.args.is_some() => "code args",
                (Some(_), _) => "code",
            };
            let sym = key.code.or(key.value);
            let site = sym.map_or(Span::default(), |sym| defined_at(analysis, sym));
            let mut record = place(analysis, site);
            // where the key's code itself is defined, in the library: what go-to-definition opens
            if let Some(def) = sym.and_then(|sym| analysis.facts.defs.iter().rev().find(|d| d.name == sym)) {
                record.insert("defined".into(), json!(place(analysis, def.span)));
            }
            record.insert("key".into(), json!(path));
            record.insert("kind".into(), json!(handler));
            record.insert(
                "value".into(),
                match handler {
                    "initial" => text(key.value),
                    _ => text(key.code),
                },
            );
            // A key with code can hold a value as well, as `.is family` does.
            record.insert("detail".into(), if handler == "initial" { Json::Null } else { text(key.value) });
            record.insert("default".into(), text(key.default));
            record
        })
        .collect()
}

/// The keys l3keys holds (`\keys_define:nn`): each `key code > ⟨path⟩`
/// macro, with its type (`choice`) and default, as [`pgfkeys`] gives pgf's.
fn l3keys(analysis: &Analysis, prefix: Option<&str>) -> Vec<Record> {
    let text = |root: &str, path: &str| {
        let sym = analysis.interner.lookup(&format!("key {root} > {path}"))?;
        let m = analysis.env.meaning(sym).as_macro()?.clone();
        Some(detokenize(&m.replacement_text, &analysis.interner))
    };
    (0..analysis.interner.len() as u32)
        .map(Sym)
        .filter_map(|sym| {
            let path = analysis.interner.name(sym).strip_prefix("key code > ")?;
            if prefix.is_some_and(|p| !path.starts_with(p)) || analysis.env.meaning(sym).as_macro().is_none() {
                return None;
            }
            let mut record = place(analysis, defined_at(analysis, sym));
            record.insert("key".into(), json!(path));
            record.insert("kind".into(), json!(text("type", path).unwrap_or_else(|| "code".into())));
            record.insert("default".into(), text("default", path).map_or(Json::Null, |d| json!(d)));
            Some(record)
        })
        .collect()
}

/// The keys pgfkeys and l3keys hold under `prefix`: a path with `/`
/// between its parts, from `/` for pgf's.
pub fn stored_keys(analysis: &Analysis, prefix: Option<&str>) -> Vec<Record> {
    let mut keys = pgfkeys(analysis, prefix);
    keys.extend(l3keys(analysis, prefix));
    keys
}

/// Whether a called name is a key dispatcher, and if so, the prefix the
/// same dialect stores its keys under: pgfkeys and l3keys keep their own
/// store ([`pgfkeys`] reads it), and every keyval dialect — keyval itself,
/// xkeyval, and a package's own copy — installs a key's code under a prefix
/// tied to its setter's own name: keyval's `\setkeys` stores `KV@…`
/// (`offered_options` relies on the same convention for package options),
/// and a private copy renamed to fit its package, like enumitem's
/// `\enitkv@setkeys`, stores `enitkv@…` right alongside it.  Recognizing the
/// convention rather than any one package's spelling of it is what keeps
/// this generic instead of a table of packages.
fn key_dispatch(name: &str) -> Option<KeyDialect<'_>> {
    match name {
        "pgfkeys" | "pgfqkeys" | "keys_set:nn" => Some(KeyDialect::Pgf),
        _ => name.strip_suffix("setkeys").map(|prefix| match prefix.is_empty() {
            true => KeyDialect::Keyval("KV@"),
            false => KeyDialect::Keyval(prefix),
        }),
    }
}

enum KeyDialect<'a> {
    Pgf,
    Keyval(&'a str),
}

/// The key family and dialect a name's code was observed reading its
/// optional argument with: the first argument of a call [`key_dispatch`]
/// recognizes, made somewhere in what that name's own code called while it
/// ran — read from the call the interpreter recorded, not looked up in a
/// table.  enumitem's `itemize` reads its keys several helpers deep
/// (`\itemize` → `\enit@itemize` → … → `\enitkv@setkeys{enumitem}{…}`), so
/// this walks the dynamic call tree breadth-first from `sym` rather than
/// looking only at its immediate calls; a handful of levels covers every
/// dialect this run has been seen to nest that deep.  An environment is
/// asked for by its base name (`itemize`, not `\begin{itemize}`):
/// `\newenvironment` makes the code that reads `[…]` the environment's own
/// macro.
fn key_family(analysis: &Analysis, sym: Sym) -> Vec<(KeyDialect<'_>, Vec<String>)> {
    // The static call graph, not `within`: `\@testopt`-style dispatch
    // re-invokes the command with no enclosing macro. What ran still answers,
    // via the family argument recorded with an expansion.
    const MAX_VISITED: usize = 4_000;
    let mut by_name: HashMap<Sym, Vec<&crate::analysis::facts::Expansion>> = HashMap::default();
    for e in &analysis.facts.expansions {
        by_name.entry(e.name).or_default().push(e);
    }
    // A name the run gave many different meanings is scratch storage
    // (`\reserved@a`, `\next`): an edge through it links unrelated code, so
    // the walk does not pass it. Nor does it go further than a call chain
    // a command's own options plausibly travel.
    const SCRATCH_DEFS: usize = 4;
    const MAX_DEPTH: usize = 6;
    let mut bodies: HashMap<Sym, BTreeSet<String>> = HashMap::default();
    for d in &analysis.facts.defs {
        bodies.entry(d.name).or_default().insert(meaning_identity(analysis, d));
    }
    let defined: HashMap<Sym, usize> = bodies.into_iter().map(|(k, v)| (k, v.len())).collect();
    let mut candidates: Vec<(usize, KeyDialect<'_>, Vec<String>)> = Vec::new();
    let mut seen: BTreeSet<Sym> = BTreeSet::from([sym]);
    let mut frontier = vec![(sym, 0usize)];
    let mut parent: HashMap<Sym, Sym> = HashMap::default();
    while let Some((current, depth)) = frontier.pop() {
        if let Some(dialect) = key_dispatch(analysis.interner.name(current)) {
            // The call this name's own code made comes first: the library
            // calls the same dispatcher for its own keys.
            let mut calls: Vec<_> = by_name.get(&current).into_iter().flatten().collect();
            calls.sort_by_key(|e| !e.within.is_some_and(|w| seen.contains(&w)));
            // A dispatcher whose argument the run did not record is read
            // off the body of the macro that called it on the way here:
            // `\def\fa[#1]{\pgfkeys{/fa/.cd,#1}}`, `\tikzset`'s `\pgfqkeys{/tikz}`.
            let spelled = format!("\\{} {{", analysis.interner.name(current));
            let from_body = parent.get(&current).into_iter().find_map(|&s| {
                let m = analysis.env.meaning(s).as_macro()?.clone();
                let body = detokenize(&m.replacement_text, &analysis.interner);
                let at = body.find(&spelled)? + spelled.len();
                Some(body[at..].split(['}', ',']).next()?.to_string())
            });
            let texts = calls.into_iter().filter_map(|e| Some(detokenize(e.arguments.first()?, &analysis.interner)));
            let mut found: Vec<String> = texts
                .chain(from_body)
                .filter_map(|text| {
                    // `\pgfkeys`/`\pgfqkeys` take one argument that mixes the
                    // path with the assignments under it (`/mytool/.cd,width=2cm`),
                    // so only what precedes the first comma is the path;
                    // `\setkeys`-style dispatchers pass the bare family instead.
                    let path = text.split(',').next()?.trim();
                    let family = match dialect {
                        KeyDialect::Pgf if path.starts_with('/') => path.trim_end_matches("/.cd").to_string(),
                        _ => path.trim_start_matches('/').split('/').next()?.to_string(),
                    };
                    (!family.is_empty()).then_some(family)
                })
                .collect();
            // Every family the code reads, in the order it was seen to
            // (enumitem reads `enumitem-resume` and `enumitem`).
            let mut known = BTreeSet::new();
            found.retain(|f| known.insert(f.clone()));
            if !found.is_empty() {
                candidates.push((depth, dialect, found));
            }
        }
        let Some(index) = analysis.calls.get(current).filter(|_| depth < MAX_DEPTH) else { continue };
        for target in &analysis.calls.edges[index] {
            let callee = analysis.calls.nodes[*target];
            if seen.len() >= MAX_VISITED {
                break;
            }
            if defined.get(&callee).copied().unwrap_or(0) > SCRATCH_DEFS {
                continue;
            }
            if seen.insert(callee) {
                parent.insert(callee, current);
                frontier.push((callee, depth + 1));
            }
        }
    }
    // Nearest first: the caller tries them until one names keys the run defined.
    candidates.sort_by_key(|(depth, ..)| *depth);
    candidates.into_iter().map(|(_, dialect, found)| (dialect, found)).collect()
}

/// `satex query options`: the keys an environment or command's optional
/// argument accepts, from the key family its own code was observed to read
/// them under — `\begin{itemize}[…]` reads enumitem's `itemize` family once
/// enumitem is loaded, the kernel's own `\item[…]` otherwise (no keys: an
/// empty list, not a guess).  Falls back to the call shape `explain` would
/// show when the name takes an optional argument but not through a
/// recognized key dispatcher (`enumerate`'s label template, for one).
pub fn options(analysis: &Analysis, name: &str) -> Vec<Record> {
    let wanted = crate::engine::tex::bare_name(name);
    let Some(sym) = analysis.interner.lookup(wanted) else { return Vec::new() };
    // The nearest key family whose keys the run defined: a wrapper named like a dispatcher
    // (`\enit@setkeys`) passes no family keys are stored under.
    let found = key_family(analysis, sym)
        .into_iter()
        .map(|(dialect, families)| family_keys(analysis, dialect, families))
        .find(|keys| !keys.is_empty());
    let Some(keys) = found else {
        let mac = analysis.env.meaning(sym).as_macro().map(|m| (**m).clone());
        let shape = effective_of(analysis, sym, mac.as_ref(), Some(crate::analysis::probe::State::End));
        if !shape.contains('[') {
            return Vec::new();
        }
        let mut record = Map::new();
        record.insert("name".into(), json!(analysis.interner.cs(sym)));
        record.insert("kind".into(), json!("positional"));
        record.insert(
            "detail".into(),
            json!(format!("takes an optional argument ({shape}) not read through a recognized key family")),
        );
        return vec![record];
    };
    keys
}

/// The stored key `key` of the list `cmd` reads (`thick` in `\draw[thick]`,
/// `every node/.style` in `\tikzset{…}`): the record [`options`] gives it,
/// with `defined` moved to the key's own line and that line's `comment`.
/// A pgf key no family stores goes to the family's `.unknown` handler
/// (`red`, `->`), marked `unknown`.
pub fn resolve_key(analysis: &Analysis, cmd: &str, key: &str) -> Option<Record> {
    // A handler after the key defines it: `every node/.style`.
    let key = key.split("/.").next()?.trim();
    let text = |r: &Record, f: &str| r.get(f).and_then(Json::as_str).unwrap_or_default().to_string();
    let options = options(analysis, cmd);
    let named = |r: &&Record| {
        let full = text(r, "key");
        full == key || full.strip_prefix(&text(r, "family")).map(|k| k.trim_start_matches('/')) == Some(key)
    };
    let mut record = match options.iter().find(named) {
        Some(r) => r.clone(),
        // a path names its key whatever families the command reads (`\pgfkeys{/demo}`)
        None if key.starts_with('/') => stored_keys(analysis, Some(key)).into_iter().find(|r| text(r, "key") == key)?,
        None => {
            let mut families: Vec<String> = options.iter().map(|r| text(r, "family")).collect();
            families.dedup();
            families.into_iter().filter(|f| f.starts_with('/')).find_map(|family| {
                let handler = format!("{family}/.unknown");
                let mut r = pgfkeys(analysis, Some(&handler)).into_iter().find(|r| text(r, "key") == handler)?;
                r.insert("family".into(), json!(family));
                r.insert("unknown".into(), json!(true));
                Some(r)
            })?
        }
    };
    let call = record.get("defined").and_then(Json::as_object).cloned();
    let name = text(&record, "key").rsplit('/').next().unwrap_or(key).to_string();
    let site = call.as_ref().filter(|_| !record.contains_key("unknown")).and_then(|c| key_line(c, &name));
    if let Some(site) = &site {
        record.insert("defined".into(), Json::Object(site.clone()));
    }
    // The comment above the key's line, or above the `\tikzset{` line it is the first key under.
    let line = |r: &Record| r.get("line").and_then(Json::as_u64).map(|n| n as usize);
    let doc = call.as_ref().and_then(|call| {
        let text = crate::project::overlay::read_to_string(Path::new(call.get("path")?.as_str()?)).ok()?;
        let at = site.as_ref().and_then(line).unwrap_or(line(call)?);
        let opener = text
            .lines()
            .nth(at.checked_sub(2)?)
            .map(str::trim)
            .is_some_and(|l| l.starts_with('\\') && l.ends_with('{'));
        crate::lsp::doccomment::above(&text, at)
            .or_else(|| opener.then(|| crate::lsp::doccomment::above(&text, at - 1)).flatten())
    });
    if let Some(doc) = doc.filter(|d| !d.text.is_empty()) {
        record.insert("comment".into(), json!(doc.text));
    }
    Some(record)
}

/// Where the call `call` places defines `key` with a handler (`key/.style`)
/// or a value (`key=`), within the braces the call opens on its line.
fn key_line(call: &Record, key: &str) -> Option<Record> {
    let first = call.get("line")?.as_u64()? as usize;
    let text = crate::project::overlay::read_to_string(Path::new(call.get("path")?.as_str()?)).ok()?;
    let mut depth = 0i64;
    for (i, line) in text.lines().enumerate().skip(first.checked_sub(1)?) {
        let found = line.match_indices(key).find(|(at, _)| {
            let rest = line[at + key.len()..].trim_start();
            let before = line[..*at].trim_end();
            (before.is_empty() || before.ends_with(['{', ',', '[', '/']))
                && (rest.starts_with("/.") || rest.starts_with('='))
        });
        if let Some((at, _)) = found {
            let mut site = call.clone();
            site.insert("line".into(), json!(i + 1));
            site.insert("col".into(), json!(line[..at].chars().count() + 1));
            return Some(site);
        }
        depth += line.matches('{').count() as i64 - line.matches('}').count() as i64;
        if depth <= 0 {
            return None;
        }
    }
    None
}

/// The keys stored under `families` in `dialect`, each with its family.
fn family_keys(analysis: &Analysis, dialect: KeyDialect<'_>, families: Vec<String>) -> Vec<Record> {
    let out: Vec<Record> = match dialect {
        KeyDialect::Pgf => {
            // `.search also` stores the paths it adds in the family's
            // `.unknown` handler; they follow the family's own keys.
            let mut families = families;
            let mut out = Vec::new();
            let also = Regex::new(r"/[A-Za-z][^,{}\s\\]*").expect("valid regex");
            let mut rank = 0;
            while rank < families.len() && rank < 8 {
                let path = format!("{}/", families[rank]);
                let keys = stored_keys(analysis, Some(&path));
                // A choice's values are no keys of their own (`mode/fast`).
                let choices: Vec<String> = keys
                    .iter()
                    .filter(|r| r.get("kind").and_then(Json::as_str).is_some_and(|k| k.contains("choice")))
                    .filter_map(|r| Some(format!("{}/", r.get("key")?.as_str()?)))
                    .collect();
                for mut record in keys {
                    let key = record.get("key").and_then(Json::as_str).unwrap_or_default().to_string();
                    if key == format!("{path}.unknown") {
                        let code = record.get("value").and_then(Json::as_str).unwrap_or_default();
                        for m in also.find_iter(code) {
                            if !families.iter().any(|f| f == m.as_str()) {
                                families.push(m.as_str().to_string());
                            }
                        }
                    }
                    if key[path.len()..].contains("/.")
                        || key[path.len()..].starts_with('.')
                        || key.ends_with("/unknown")
                        || choices.iter().any(|c| key.starts_with(c.as_str()))
                    {
                        continue;
                    }
                    record.insert("rank".into(), json!(rank));
                    record.insert("family".into(), json!(families[rank]));
                    record.insert(
                        "type".into(),
                        json!(crate::analysis::roles::Ty::Key(crate::analysis::roles::KeyKind::Pgf).to_string()),
                    );
                    out.push(record);
                }
                rank += 1;
            }
            return out;
        }
        KeyDialect::Keyval(prefix) => families
            .iter()
            .flat_map(|family| {
                keyval_keys(analysis, &format!("{prefix}{family}@")).into_iter().map(move |k| (family, k))
            })
            .map(|(family, (key, def, default))| {
                let mut record = place(analysis, def.span);
                record.insert("family".into(), json!(family));
                record.insert("key".into(), json!(key));
                record.insert("store".into(), json!(format!("{prefix}{family}@")));
                record.insert(
                    "type".into(),
                    json!(crate::analysis::roles::Ty::Key(crate::analysis::roles::KeyKind::Option).to_string()),
                );
                record.insert("default".into(), default.map_or(Json::Null, |d| json!(d)));
                record.insert("package".into(), symbol(analysis, def.package));
                record
            })
            .collect(),
    };
    out
}

/// The keys a keyval-style family holds: every macro defined as
/// `⟨store⟩⟨key⟩` (keyval's `\define@key` convention, which xkeyval,
/// kvoptions and private copies share), with the default its
/// `⟨store⟩⟨key⟩@default` passes when one was given.
pub fn keyval_keys<'a>(
    analysis: &'a Analysis,
    store: &str,
) -> Vec<(String, &'a crate::analysis::facts::Definition, Option<String>)> {
    let mut keys: BTreeMap<&str, &crate::analysis::facts::Definition> = BTreeMap::new();
    let mut defaults: HashMap<&str, String> = HashMap::default();
    for def in &analysis.facts.defs {
        let Some(key) = analysis.interner.name(def.name).strip_prefix(store) else { continue };
        let Some(mac) = def.mac.as_ref().filter(|_| !key.is_empty()) else { continue };
        if let Some(base) = key.strip_suffix("@default") {
            // `\KV@fam@key{⟨default⟩}`: the braced part is the value.
            let body = detokenize(&mac.replacement_text, &analysis.interner);
            if let (Some(a), Some(b)) = (body.find('{'), body.rfind('}')) {
                defaults.insert(base, body[a + 1..b].to_string());
            }
        } else if !key.contains('@') {
            keys.insert(key, def);
        }
    }
    keys.into_iter().map(|(key, def)| (key.to_string(), def, defaults.get(key).cloned())).collect()
}

/// Where a control sequence built by a library's own code was asked for: the
/// call read straight from a file that the definition happened inside, so a
/// key made by `\pgfkeys{…}` points at that `\pgfkeys` and not into
/// pgfkeys.code.tex.
pub(crate) fn defined_at(analysis: &Analysis, sym: Sym) -> Span {
    let Some(def) = analysis.facts.defs.iter().rev().find(|def| def.name == sym) else {
        return analysis.kernel_sites.get(&sym).copied().unwrap_or_default();
    };
    analysis
        .graph
        .maker(def.node)
        .and_then(|call| analysis.graph.vertices.get(call as usize))
        .map_or(def.span, |vertex| vertex.span)
}
