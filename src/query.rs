//! Queries and filters over a finished analysis.
//! Filters: `tag=switch and package~^my` evaluated against record fields.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use regex::Regex;
use std::fmt::Write as _;

use serde_json::{json, Map, Value as Json};

use crate::builtins::{LoadKind, OccKind};
use crate::facts::{Load, LoadStatus};
use crate::env::NodeId;
use crate::graph::EdgeKind;
use crate::machine::Analysis;
use crate::tex::{detokenize, FileId, Meaning, Span, Sym};

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
        if let Some((symbol, op)) = OPERATORS.iter().find(|(symbol, _)| chars[i..].starts_with(&symbol.chars().collect::<Vec<_>>()[..])) {
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
}

impl Query {
    pub const ALL: [(&'static str, Query); 19] = [
        ("definitions", Query::Definitions),
        ("expansions", Query::Expansions),
        ("dependencies", Query::Dependencies),
        ("occurrences", Query::Occurrences),
        ("recursion", Query::Recursion),
        ("calls", Query::Calls),
        ("dependency-graph", Query::DependencyGraph),
        ("trace", Query::Trace),
        ("files", Query::Files),
        ("diagnostics", Query::Diagnostics),
        ("distribution", Query::Distribution),
        ("catcodes", Query::Catcodes),
        ("side-effects", Query::SideEffects),
        ("project", Query::Project),
        ("plugins", Query::Plugins),
        ("produces", Query::Produces),
        ("gaps", Query::Gaps),
        ("pgfkeys", Query::Pgfkeys),
        ("options", Query::Options),
    ];

    pub fn parse(name: &str) -> Option<Query> {
        Self::ALL.iter().find(|(n, _)| *n == name).map(|(_, q)| *q)
    }

    pub fn name(self) -> &'static str {
        Self::ALL.iter().find(|(_, q)| *q == self).map(|(n, _)| *n).unwrap_or("query")
    }
}

/// What the run could not follow, one row per cause with how often it
/// happened and where it first did.  Widening a recursion or analyzing both
/// arms of an undecided conditional is how satex stays sound, so neither is
/// here; what is here are the constructs it could not interpret at all.
fn gaps(analysis: &Analysis) -> Vec<Record> {
    let mut seen: Vec<(&'static str, usize, &crate::facts::Diagnostic)> = Vec::new();
    for diagnostic in &analysis.facts.diagnostics {
        if diagnostic.severity != crate::facts::Severity::Unsupported {
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
            record.insert("severity".into(), json!(crate::facts::Severity::Unsupported.as_str()));
            record.insert("message".into(), json!(first.message.clone()));
            record
        })
        .collect()
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
        // The text to look for arrives with `--text`, which `run` has no
        // room for: `cmd::query` calls `produces` directly.
        Query::Produces => Vec::new(),
        Query::Gaps => gaps(analysis),
        Query::Pgfkeys => pgfkeys(analysis, None),
        // The name to look up arrives with `--for`, which `run` has no room
        // for: `cmd::query` calls `options` directly.
        Query::Options => Vec::new(),
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
pub fn origin(analysis: &Analysis, file: crate::tex::FileId) -> &'static str {
    let path = |id: crate::tex::FileId| analysis.files.get(id as usize).map(|f| Path::new(&f.path));
    let project = path(analysis.main_file).and_then(Path::parent);
    let inside = match (path(file), project) {
        (Some(file), Some(root)) if !root.as_os_str().is_empty() => file.starts_with(root),
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
            record.insert("subject".into(), symbol(analysis, def.subject));
            record.insert("package".into(), symbol(analysis, def.package));
            record.insert("origin".into(), json!(origin(analysis, def.span.file)));
            record.insert("mode".into(), json!(def.mode.as_str()));
            record.insert("arity".into(), json!(def.arity()));
            record.insert("takes".into(), takes_of(analysis, def.name, def.mac.as_deref(), None));
            record.insert(
                "signature".into(),
                match def.mac.as_ref().and_then(|m| m.arg_spec.as_ref()) {
                    Some(spec) => json!(spec.raw),
                    None => Json::Null,
                },
            );
            record.insert(
                "parameters".into(),
                match &def.mac {
                    Some(m) => json!(m.parameter_text.render(&analysis.interner)),
                    None => Json::Null,
                },
            );
            record.insert("global".into(), json!(def.global));
            record.insert("certain".into(), json!(def.certain));
            record.insert("depth".into(), json!(def.depth));
            record.insert("node".into(), json!(def.node));
            record.insert(
                "body".into(),
                match &def.mac {
                    Some(m) => json!(detokenize(&m.replacement_text, &analysis.interner)),
                    None => Json::Null,
                },
            );
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
            record.insert("package".into(), symbol(analysis, use_.package));
            record.insert("within".into(), symbol(analysis, use_.within));
            record.insert("node".into(), json!(use_.node));
            record.insert("count".into(), json!(use_.count));
            record.insert("mode".into(), json!(use_.mode.names()));
            record.insert("arguments".into(), json!(use_.arguments.len()));
            record.insert(
                "argument".into(),
                json!(use_
                    .arguments
                    .first()
                    .map(|a| detokenize(a, &analysis.interner))
                    .unwrap_or_default()),
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
            record.insert("resolved".into(), json!(load.path.is_some()));
            record.insert("path".into(), json!(load.path));
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
    let defined: BTreeMap<Sym, Span> =
        analysis.facts.defs.iter().map(|d| (d.name, d.span)).collect();
    analysis
        .recursion()
        .into_iter()
        .filter(|r| defined.contains_key(&r.name))
        .map(|crate::machine::Recursion { name: sym, cycle, observed }| {
            let span = defined.get(&sym).copied().unwrap_or_default();
            let mut record = place(analysis, span);
            record.insert("name".into(), json!(analysis.interner.cs(sym)));
            record.insert("kind".into(), json!(if cycle.len() > 1 { "mutual" } else { "direct" }));
            record.insert("observed".into(), json!(observed));
            record.insert(
                "cycle".into(),
                json!(cycle.iter().map(|s| analysis.interner.cs(*s)).collect::<Vec<_>>()),
            );
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
                json!(analysis
                    .graph
                    .outgoing(id as NodeId)
                    .iter()
                    .map(|(to, kind)| json!({ "to": to, "types": kind.names() }))
                    .collect::<Vec<_>>()),
            );
            record.insert("controls".into(), json!(vertex.cds.len()));
            record
        })
        .collect()
}

fn trace(analysis: &Analysis) -> Vec<Record> {
    analysis
        .trace
        .iter()
        .map(|event| {
            let mut record = place(analysis, event.span);
            record.insert("index".into(), json!(event.index));
            record.insert("depth".into(), json!(event.depth));
            record.insert("step".into(), json!(event.kind.as_str()));
            record.insert("name".into(), json!(analysis.interner.cs(event.name)));
            record.insert("detail".into(), json!(event.detail.as_deref()));
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
            record.insert("file".into(), json!(analysis.short_name(id as crate::tex::FileId)));
            record.insert("path".into(), json!(info.path));
            record.insert("kind".into(), json!(info.kind.as_str()));
            record
        })
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
            record.insert(
                "detail".into(),
                json!(format!("while expanding at {}", caused_by.span)),
            );
            Some(record)
        })
        .collect()
}

fn catcodes(analysis: &Analysis) -> Vec<Record> {
    let base = crate::tex::CatcodeTable::latex();
    analysis
        .catcodes
        .diff(&base)
        .into_iter()
        .map(|(character, code)| {
            let mut record = Map::new();
            record.insert("key".into(), json!(character.to_string()));
            record.insert("code".into(), json!(code));
            record.insert(
                "kind".into(),
                json!(crate::tex::Catcode::from_u8(code).map(|c| format!("{c:?}"))),
            );
            record.insert(
                "detail".into(),
                json!(format!("was {:?}", base.get(character))),
            );
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
    first.insert("message".into(), json!(format!(
        "engine {} ({})",
        identity.engine, identity.engine_source
    )));
    let mut out = vec![first];
    for file in &analysis.project.files {
        let mut record = Map::new();
        record.insert("kind".into(), json!(file.tool));
        record.insert("name".into(), json!(file.path.display().to_string()));
        record.insert("detail".into(), json!(file
            .settings
            .iter()
            .map(|(key, value)| format!("{key}={value}"))
            .collect::<Vec<_>>()
            .join(", ")));
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
        let mut detail =
            paths[..shown].iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ");
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
            record
        })
        .collect()
}

/// Which way `satex slice` walks the [`crate::graph::DependencyGraph`] from its
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
            path.ends_with(wanted) || wanted.ends_with(path) || crate::config::names_file(analysis.file_name(id), &wanted.to_string_lossy())
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
            None => {
                name == wanted
                    || name.strip_prefix(crate::tex::ACTIVE).is_some_and(|c| c == wanted)
            }
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
        return (0..analysis.graph.vertices.len())
            .filter(|&id| {
                let span = analysis.graph.vertices[id].span;
                span.file == file && (span.line, span.col) >= start && (span.line, span.col) <= end
            })
            .map(|id| id as NodeId)
            .collect();
    }
    let on_line = || {
        analysis
            .graph
            .vertices
            .iter()
            .enumerate()
            .filter(move |(_, v)| v.span.file == file && v.span.line == at.line)
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

pub fn slice(
    analysis: &Analysis,
    names: &[String],
    at: Option<&At>,
    direction: Direction,
) -> Vec<Record> {
    slice_from(analysis, resolve(analysis, names, at), direction)
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
    analysis
        .graph
        .vertices
        .iter()
        .position(|v| v.span.file == main && v.span.line > last)
        .map_or(NodeId::MAX, |first| (first as NodeId).saturating_sub(1))
}

/// A backward slice from `nodes` that answers a body vertex to its own call.
fn backward(analysis: &Analysis, nodes: &[NodeId], limit: NodeId) -> Vec<NodeId> {
    let bound = crate::graph::SliceBound { limit, home: Some(analysis.main_file) };
    analysis.graph.slice_until(nodes, SLICE_EDGES, false, &bound)
}

fn slice_from(analysis: &Analysis, criteria: Vec<NodeId>, direction: Direction) -> Vec<Record> {
    let forward = direction == Direction::Forward;
    let bound = crate::graph::SliceBound {
        limit: if forward { NodeId::MAX } else { slice_limit(analysis, &criteria) },
        home: (!forward).then_some(analysis.main_file),
    };
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
    let reach = |ids: &mut dyn Iterator<Item = NodeId>| {
        let main = analysis.main_file;
        let last = ids
            .filter_map(|id| analysis.graph.vertex(id))
            .filter(|v| v.span.file == main && !frame.ends.contains(&v.span.line))
            .map(|v| v.span.line)
            .max();
        match last {
            Some(last) => analysis
                .graph
                .vertices
                .iter()
                .position(|v| v.span.file == main && v.span.line > last)
                .map_or(NodeId::MAX, |first| (first as NodeId).saturating_sub(1)),
            None => NodeId::MAX,
        }
    };
    let limit = reach(&mut nodes.iter().chain(&frame.nodes).copied());
    nodes.extend(backward(analysis, &frame.nodes, limit));
    let mut framing: BTreeMap<FileId, BTreeSet<u32>> = BTreeMap::new();
    framing.entry(analysis.main_file).or_default().extend(frame.lines.iter().copied());
    let lines = loop {
        let lines = kept_lines(analysis, &nodes, &framing, &mut texts);
        let extra: Vec<NodeId> = on_kept_lines(analysis, &lines, &frame.ends)
            .into_iter()
            .chain(enclosing_environments(analysis, &lines))
            .filter(|node| !nodes.contains(node))
            .collect();
        let groups = enclosing_groups(analysis, &lines);
        let conditionals = enclosing_conditionals(analysis, &lines);
        if extra.is_empty() && groups.is_empty() && conditionals.is_empty() {
            break lines;
        }
        for (file, line) in groups.into_iter().chain(conditionals) {
            framing.entry(file).or_default().insert(line);
        }
        let limit = reach(&mut nodes.iter().chain(&extra).copied());
        nodes.extend(extra.iter().copied());
        nodes.extend(backward(analysis, &extra, limit));
    };
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
    let mut current: Option<FileId> = None;
    for (_, file, number) in chunks {
        let Some(line) = texts.get(&file).and_then(|t| t.get(number as usize - 1)) else {
            continue;
        };
        if current != Some(file) && file != analysis.main_file {
            let _ = writeln!(out, "%% from {}", analysis.file_name(file));
        }
        current = Some(file);
        let _ = writeln!(out, "{line}");
    }
    out
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
}

fn frame(analysis: &Analysis, main: &[String]) -> Frame {
    let main_file = analysis.main_file;
    let here = || analysis.facts.occurrences.iter().filter(move |o| o.span.file == main_file);
    let document = |o: &&crate::facts::Occurrence| o.key == "document";
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
    let mut ends: Vec<u32> = here()
        .filter(|o| o.kind == OccKind::EndEnvironment && document(o))
        .map(|o| o.span.line)
        .collect();
    let library = analysis.files.get(main_file as usize).is_some_and(|f| f.kind.is_package());
    if lines.is_empty() && ends.is_empty() && !library
        && let Some(last) = main.iter().rposition(|line| {
            !line.split('%').next().unwrap_or_default().trim().is_empty()
        })
    {
        ends.push(last as u32 + 1);
    }
    lines.extend(ends.iter().copied());
    Frame { nodes, lines, ends }
}

/// Everything the run did on a kept line, read from the file itself rather
/// than produced by an expansion.
fn on_kept_lines(
    analysis: &Analysis,
    lines: &BTreeMap<FileId, BTreeSet<u32>>,
    ends: &[u32],
) -> Vec<NodeId> {
    let main = analysis.main_file;
    analysis
        .graph
        .vertices
        .iter()
        .enumerate()
        .filter(|(_, v)| {
            v.within.is_none()
                && !(v.span.file == main && ends.contains(&v.span.line))
                && lines.get(&v.span.file).is_some_and(|kept| kept.contains(&v.span.line))
        })
        .map(|(id, _)| id as NodeId)
        .collect()
}

/// Both ends of every environment that holds a kept line: a line kept
/// without the environment around it runs outside the group the environment
/// opens, and without whatever its `\begin` sets up.
fn enclosing_environments(analysis: &Analysis, lines: &BTreeMap<FileId, BTreeSet<u32>>) -> Vec<NodeId> {
    let mut out = Vec::new();
    for (&file, kept) in lines {
        let mut open: Vec<&crate::facts::Occurrence> = Vec::new();
        let mut marks: Vec<&crate::facts::Occurrence> = analysis
            .facts
            .occurrences
            .iter()
            .filter(|o| {
                o.span.file == file
                    && matches!(o.kind, OccKind::BeginEnvironment | OccKind::EndEnvironment)
            })
            .collect();
        marks.sort_by_key(|o| (o.span.line, o.span.col));
        for mark in marks {
            if mark.kind == OccKind::BeginEnvironment {
                open.push(mark);
                continue;
            }
            let Some(at) = open.iter().rposition(|b| b.key == mark.key) else { continue };
            let begin = open[at];
            open.truncate(at);
            if kept.range(begin.span.line..=mark.span.line).next().is_some() {
                out.extend([begin.node, mark.node]);
            }
        }
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
    framing: &BTreeMap<FileId, BTreeSet<u32>>,
    texts: &mut BTreeMap<FileId, Vec<String>>,
) -> BTreeMap<FileId, BTreeSet<u32>> {
    let mut wanted = framing.clone();
    for &node in nodes {
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
    home(analysis, load.span.file, load.span.line, load.span.line)
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

/// Whether a file's own lines go into the reconstruction: the main file,
/// and a file that one of those reads as input rather than as a package or
/// a class.
fn inlined(analysis: &Analysis, file: FileId) -> bool {
    let mut file = file;
    for _ in 0..analysis.files.len() {
        if file == analysis.main_file {
            return true;
        }
        // Installation content (the kernel, and every package and class it
        // ships) is never the document's own to inline: a file it loads
        // deep inside the kernel's own bootstrap (l3kernel pulling in its
        // backend, say) has nowhere precise to attribute the load to, so it
        // falls back to wherever the main file currently is — which would
        // otherwise read as "the main file `\input`s this" and paste a
        // library file's raw source into the reconstruction.

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
    let code = &line[..crate::tex::comment_start(line).unwrap_or(line.len())];
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

pub fn switches(analysis: &Analysis, all: bool) -> Vec<Record> {
    let main = analysis.main_file;
    let mut out = Vec::new();
    let mut seen: BTreeMap<String, ()> = BTreeMap::new();
    for def in &analysis.facts.defs {
        let name = analysis.interner.name(def.name);
        let Some(base) = name.strip_prefix("if") else { continue };
        if base.is_empty() || def.mac.is_some() {
            continue;
        }
        let dialect = analysis.interner.name(def.by);
        if !dialect.starts_with("new") && !dialect.starts_with("provide") {
            continue;
        }
        if !all && def.span.file != main {
            continue;
        }
        if seen.insert(name.to_string(), ()).is_some() {
            continue;
        }
        let value = match analysis.env.meaning(def.name).prim() {
            Some(crate::builtins::Primitive::If(crate::builtins::Cond::True)) => "true",
            Some(crate::builtins::Primitive::If(crate::builtins::Cond::False)) => "false",
            _ => "undecided",
        };
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
    let under = |cds: &[crate::graph::ControlDep]| {
        cds.iter()
            .any(|cd| analysis.graph.vertex(cd.on).is_some_and(|vertex| vertex.name == name))
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
                ["", ".sty", ".cls"]
                    .iter()
                    .any(|ext| name == format!("KV@{package}{ext}@{key}"))
            });
        if def.tag != "option" && !own_key {
            continue;
        }
        let name = analysis.interner.name(subject);
        // `\DeclareOption*` and the keys a package declares for itself are
        // not options a document can pass.
        let internal = name.is_empty()
            || name.contains('@')
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
        if occurrence.kind != OccKind::PassedOption
            || occurrence.detail.as_deref() != Some("default")
        {
            continue;
        }
        let package = occurrence
            .package
            .map(|sym| analysis.interner.name(sym).to_string())
            .unwrap_or_default();
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

pub fn controls(analysis: &Analysis, name: &str) -> Vec<Record> {
    let wanted = name.trim_start_matches('\\');
    let governed = |cds: &[crate::graph::ControlDep]| -> Option<String> {
        cds.iter().find_map(|cd| {
            let vertex = analysis.graph.vertex(cd.on)?;
            let rendered = analysis.interner.name(vertex.name);
            (rendered == wanted).then(|| format!("\\{rendered} = {}", cd.taken))
        })
    };
    let mut out = Vec::new();
    for def in &analysis.facts.defs {
        let Some(branch) = governed(&def.cds) else { continue };
        let mut record = place(analysis, def.span);
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
                crate::builtins::Primitive::Else
                    | crate::builtins::Primitive::Or
                    | crate::builtins::Primitive::Fi
                    | crate::builtins::Primitive::If(_)
            )
        ) {
            continue;
        }
        let mut record = place(analysis, expansion.span);
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
                node.identification = Some(
                    occurrence.detail.clone().unwrap_or_default().trim().to_string(),
                )
            }
            OccKind::Catcode => node.catcodes.push(format!(
                "{}={}",
                occurrence.key,
                occurrence.detail.clone().unwrap_or_default()
            )),
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
            && load.status == LoadStatus::Read {
                reader.entry(child).or_insert(load.span);
            }
    }
    let main = analysis.main_file;
    let mut open = Vec::new();
    attach(main, &mut by_file, &loads_in, &reader, &mut open)
        .unwrap_or_else(|| Node::empty("<input>".into(), "input"))
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
        let read_here =
            load.file.is_some_and(|child| reader.get(&child) == Some(&load.span) && !open.contains(&child));
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
    let class = analysis
        .facts
        .loads
        .iter()
        .find(|load| load.kind == LoadKind::Class && load.span.file == main);
    let body = analysis.document_depth.is_some();
    let context = analysis.plugins.kernel == crate::plugin::Kernel::Context;
    let kind = match analysis.files.get(main as usize).map(|file| file.kind) {
        // satex recognizes a ConTeXt document but does not interpret the
        // ConTeXt kernel, so it says no more than that it is one.
        _ if context => "ConTeXt document (experimental)",
        // A `.dtx` is documentation and code in one file, and a `.ins` is the
        // batch file that extracts the code: neither is a document, and what
        // satex read of a `.dtx` is what docstrip extracts from it.
        _ if analysis.literate.is_some() => {
            analysis.literate.map(crate::literate::Literate::as_str).unwrap_or_default()
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
        engine_source: analysis.plugins.source(crate::plugin::Kind::Engine),
        project: analysis.project.describe(),
    }
}

pub fn summary_json(analysis: &Analysis) -> Json {
    let root = summary(analysis);
    let identity = identity(analysis);
    json!({
        "identified": identity.kind,
        "profile": analysis.settings.profile.map(crate::config::Profile::as_str),
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

/// A name only a package's or the kernel's own internals would spell:
/// LaTeX2e's `@` convention, an expl3 function (`_..._:...`) whether public
/// or private, and expl3's own `__` prefix for what is private even among
/// those.
fn internal_name(name: &str) -> bool {
    // A name a `\csname`-style construction built out of another one's own
    // spelling (`\@testopt`'s internal partner for `\section`, say) carries
    // a literal backslash as its first character; no document ever writes
    // that out by hand.
    name.contains('@') || name.contains(':') || name.starts_with("__") || name.starts_with('\\')
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
pub fn scope(analysis: &Analysis, before: Option<(FileId, u32, u32)>, all: bool) -> Vec<Record> {
    let mut latest: BTreeMap<Sym, &crate::facts::Definition> = BTreeMap::new();
    for def in &analysis.facts.defs {
        let visible = match before {
            None => true,
            Some((file, line, col)) => analysis.visible_in(def.span, file, line, col),
        };
        if visible {
            latest.insert(def.name, def);
        }
    }
    let mut rows: Vec<(u8, Record)> = Vec::new();
    for def in latest.values() {
        let name = analysis.interner.name(def.name);
        if unprintable_name(name) || (!all && internal_name(name)) {
            continue;
        }
        let mut record = place(analysis, def.span);
        record.insert("name".into(), json!(analysis.interner.cs(def.name)));
        record.insert("tag".into(), json!(def.tag));
        record
            .insert("effective".into(), json!(effective_of(analysis, def.name, def.mac.as_deref(), None)));
        let (tier, origin) = origin_of(analysis, def.package, def.span.file);
        record.insert("package".into(), origin);
        rows.push((tier, record));
    }
    let recorded: BTreeSet<Sym> = analysis.facts.defs.iter().map(|d| d.name).collect();
    for i in 0..analysis.interner.len() as u32 {
        let sym = Sym(i);
        // A name the document defines later, but not yet at this position,
        // is not the kernel's meaning to show — `env.meaning` is the run's
        // final state, not this position's.
        if latest.contains_key(&sym) || recorded.contains(&sym) {
            continue;
        }
        let meaning = analysis.env.meaning(sym);
        if matches!(meaning, Meaning::Undefined) {
            continue;
        }
        let name = analysis.interner.name(sym);
        let tag = meaning.kind();
        // Without `--all`, only what reads as an invokable command: not the
        // registers and character meanings the kernel carries one of for
        // every number and character it has ever seen, which would
        // otherwise outnumber the useful primitives and macros many times
        // over.
        if unprintable_name(name)
            || (!all && (internal_name(name) || !matches!(tag, "primitive" | "macro")))
        {
            continue;
        }
        let mut record = Map::new();
        record.insert("name".into(), json!(analysis.interner.cs(sym)));
        record.insert("tag".into(), json!(tag));
        record.insert(
            "effective".into(),
            json!(effective_of(analysis, sym, meaning.as_macro().map(|m| &**m), None)),
        );
        record.insert("package".into(), json!("kernel"));
        rows.push((2, record));
    }
    rows.sort_by(|(ta, a), (tb, b)| {
        (ta, a.get("name").and_then(Json::as_str)).cmp(&(tb, b.get("name").and_then(Json::as_str)))
    });
    rows.into_iter().map(|(_, record)| record).collect()
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
fn observed_shape(analysis: &Analysis, sym: crate::tex::Sym) -> Option<String> {
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
/// ([`crate::probe`]) — at the end of the run, or at `\begin{document}`
/// with `preamble`.
pub fn signature(analysis: &Analysis, sym: crate::tex::Sym, mac: &crate::tex::MacroDef) -> Option<crate::tex::ArgSpec> {
    probed(analysis, sym, Some(mac), Some(false)).filter(|p| p.complete).map(|p| p.spec)
}

/// `view` is `None` where a listing reports many names at once and only
/// what the definition itself says is wanted: its ltcmd specification or
/// its parameter text.  Otherwise the probe says what a call reads.  Either
/// is written in one notation ([`crate::probe::raw_of`]).
fn probed(
    analysis: &Analysis,
    sym: crate::tex::Sym,
    mac: Option<&crate::tex::MacroDef>,
    view: Option<bool>,
) -> Option<crate::probe::Probed> {
    if let Some(spec) = mac.and_then(|m| m.arg_spec.as_ref()) {
        return Some(crate::probe::Probed { spec: spec.clone(), complete: true, error: None, forms: Vec::new() });
    }
    let own = mac
        .and_then(|m| crate::tex::ArgSpec::from_parameter_text(&m.parameter_text, &analysis.interner))
        .map(|spec| crate::tex::ArgSpec { raw: crate::probe::raw_of(&spec.items), ..spec });
    let Some(preamble) = view else {
        return own.map(|spec| crate::probe::Probed { spec, complete: true, error: None, forms: Vec::new() });
    };
    // Where the run could not say, what the parameter text reads is certain.
    crate::probe::probe(analysis, sym, preamble && analysis.preamble.is_some())
        .or_else(|| own.map(|spec| crate::probe::Probed { spec, complete: false, error: None, forms: Vec::new() }))
}

/// How many arguments a command consumes, as `{min, max}`: the mandatory
/// items of its signature, and all of them; `max` is `null` when it is
/// unbounded (`\csname`, `\halign`) or the probe could not see the end.
/// A primitive's bounds are what satex was seen to read for it, or its
/// catalog entry.
fn takes_of(analysis: &Analysis, sym: crate::tex::Sym, mac: Option<&crate::tex::MacroDef>, view: Option<bool>) -> Json {
    let (mut low, mut high) = match analysis.env.meaning(sym).prim() {
        Some(prim) => match analysis.facts.shapes.get(&sym) {
            Some(shape) => shape_bounds(shape),
            None => crate::builtins::takes(prim),
        },
        None => (0, Some(0)),
    };
    if let Some(mac) = mac {
        match probed(analysis, sym, Some(mac), view) {
            Some(p) => {
                use crate::tex::ArgType as A;
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
fn effective_of(analysis: &Analysis, sym: crate::tex::Sym, mac: Option<&crate::tex::MacroDef>, view: Option<bool>) -> String {
    use crate::tex::ArgType as A;
    let cs = analysis.interner.cs(sym);
    let mut body = String::new();
    let mut star = false;
    let mut n = 0u8;
    let primitive = analysis.env.meaning(sym).prim().filter(|_| mac.is_none());
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
                    A::Optional(Some(default)) => {
                        let _ = write!(body, "[{default}]");
                    }
                    A::TokenFlag(c) => {
                        let _ = write!(body, "{c}?");
                    }
                    A::Keyword { word, value } => {
                        let value = value.as_deref().map(|v| format!(" ⟨{v}⟩")).unwrap_or_default();
                        let _ = write!(body, " [{word}{value}]");
                    }
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
                        let word = cs.trim_start_matches('\\').chars().count() > 1
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
        (None, None, Some(prim)) => {
            let (low, high) = crate::builtins::takes(prim);
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
/// `calls` (the names its body merely names): every [`crate::facts::Expansion`]
/// recorded with this macro as its caller (`within`), deduplicated and
/// capped so the record stays a summary rather than a full trace.  Empty for
/// a macro this run never invoked, even one with calls in its body.
fn expands_of(analysis: &Analysis, sym: crate::tex::Sym) -> Vec<Json> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for expansion in &analysis.facts.expansions {
        if expansion.within != Some(sym) {
            continue;
        }
        let name = analysis.interner.cs(expansion.name);
        if !seen.insert(name.clone()) {
            continue;
        }
        out.push(json!(name));
        if out.len() >= 12 {
            break;
        }
    }
    out
}

/// `body` and, when the meaning is a primitive rather than a macro, the
/// `kind: "primitive"` marker: a primitive has no replacement text, so its
/// body is its own name, the way `\show` would report it (tex.web § 1294).
fn body_of(analysis: &Analysis, sym: crate::tex::Sym, mac: Option<&crate::tex::MacroDef>) -> (Json, bool) {
    match mac {
        Some(m) => (json!(detokenize(&m.replacement_text, &analysis.interner)), false),
        None => match analysis.env.meaning(sym) {
            Meaning::Primitive(_) => (json!(analysis.interner.cs(sym)), true),
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
    sym: crate::tex::Sym,
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
        let source = crate::builtins::reference(*p);
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

/// A manual on texdoc.org, which serves the same PDF a local `texdoc` opens.
fn texdoc(name: &str) -> String {
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
fn is_kernel_file(analysis: &Analysis, file: crate::tex::FileId) -> bool {
    analysis.format.as_ref().is_some_and(|f| analysis.file_name(file) == f.source)
}

/// A record for one recorded [`crate::facts::Definition`]: what it is, what
/// it takes, what it does — the branch `explain` uses when the run recorded
/// a `\def`-family site for the name.
/// Which part of the run a record describes: all of it, or the preamble or
/// the document body when the two see different meanings.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Window {
    All,
    Preamble,
    Document,
}

impl Window {
    fn view(self) -> Option<bool> {
        Some(self == Window::Preamble)
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
            (Window::Preamble, Some(b), Some(n)) => n < b,
            (Window::Document, Some(b), Some(n)) => n >= b,
            (_, Some(_), None) => false,
        })
        .map(|e| e.count)
        .sum()
}

fn record_for_def(analysis: &Analysis, def: &crate::facts::Definition, window: Window) -> Record {
    let mut record = place(analysis, def.span);
    record.insert("name".into(), json!(analysis.interner.cs(def.name)));
    record.insert("tag".into(), json!(def.tag));
    record.insert("by".into(), json!(analysis.interner.cs(def.by)));
    // A class is loaded the same way a package is (`def.package` names
    // either), but reads oddly as "package article" for a `\documentclass`;
    // the file it came from says which it really is.
    let kind = class_or_package(analysis, def.package, def.span.file);
    record.insert(kind.into(), symbol(analysis, def.package));
    record.insert("arity".into(), json!(def.arity()));
    record.insert("takes".into(), takes_of(analysis, def.name, def.mac.as_deref(), window.view()));
    record.insert(
        "signature".into(),
        match probed(analysis, def.name, def.mac.as_deref(), window.view()) {
            Some(p) if p.complete => json!(p.spec.raw),
            Some(p) => json!(format!("{}…", p.spec.raw)),
            None => Json::Null,
        },
    );
    if let Some(p) = probed(analysis, def.name, def.mac.as_deref(), window.view()).filter(|p| !p.forms.is_empty()) {
        record.insert("forms".into(), json!(p.forms.iter().map(|f| f.raw.clone()).collect::<Vec<_>>()));
    }
    record.insert("effective".into(), json!(effective_of(analysis, def.name, def.mac.as_deref(), window.view())));
    if let Some(error) = probed(analysis, def.name, def.mac.as_deref(), window.view()).and_then(|p| p.error) {
        record.insert("error".into(), json!(error));
    }
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
    let expanded = expands_of(analysis, def.name);
    if !expanded.is_empty() {
        record.insert("expands".into(), json!(expanded));
    }
    let package_name = def.package.map(|p| analysis.interner.name(p));
    let kernel = is_kernel_file(analysis, def.span.file);
    if let Some((doc, link)) = documentation(
        analysis,
        def.name,
        package_name,
        kind == "class",
        &analysis.env.meaning(def.name),
        kernel,
    ) {
        record.insert("documentation".into(), json!(doc));
        record.insert("reference".into(), json!(link));
    }
    record
}

/// A record for a name with no recorded definition site: a kernel primitive
/// or macro this run never redefined, reported from its final meaning —
/// the branch `explain` falls back to.
fn record_for_kernel(analysis: &Analysis, sym: Sym, window: Window) -> Option<Record> {
    let meaning = analysis.env.meaning(sym);
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
                let short = std::path::Path::new(&f.source)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&f.source);
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
        None => analysis
            .facts
            .shapes
            .get(&sym)
            .map(|shape| shape_bounds(shape).1.unwrap_or(0))
            .unwrap_or_else(|| {
                meaning
                    .prim()
                    .map_or(0, |p| crate::builtins::takes(p).1.unwrap_or(crate::builtins::takes(p).0))
            }),
    };
    record.insert("arity".into(), json!(arity));
    record.insert("context".into(), json!(where_label(analysis, &[Vec::new()])));
    record.insert("takes".into(), takes_of(analysis, sym, meaning.as_macro().map(|m| &**m), window.view()));
    record.insert(
        "signature".into(),
        match probed(analysis, sym, meaning.as_macro().map(|m| &**m), window.view()) {
            Some(p) if p.complete => json!(p.spec.raw),
            Some(p) => json!(format!("{}…", p.spec.raw)),
            None => Json::Null,
        },
    );
    record.insert("effective".into(), json!(effective_of(analysis, sym, meaning.as_macro().map(|m| &**m), window.view())));
    if let Some(error) = probed(analysis, sym, meaning.as_macro().map(|m| &**m), window.view()).and_then(|p| p.error) {
        record.insert("error".into(), json!(error));
    }
    let (body, primitive) = body_of(analysis, sym, meaning.as_macro().map(|m| &**m));
    record.insert("body".into(), body);
    if primitive {
        record.remove("tag");
        record.insert("kind".into(), json!("primitive"));
    }
    if meaning.as_macro().is_some() {
        let expanded = expands_of(analysis, sym);
        if !expanded.is_empty() {
            record.insert("expands".into(), json!(expanded));
        }
    }
    if let Some((doc, link)) = documentation(analysis, sym, None, false, &meaning, true) {
        record.insert("documentation".into(), json!(doc));
        record.insert("reference".into(), json!(link));
    }
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

/// The latest definition of `sym` at or before `boundary`; `None` for the
/// boundary means no cutoff, so the latest definition overall.
fn def_at(analysis: &Analysis, sym: Sym, boundary: Option<NodeId>) -> Option<&crate::facts::Definition> {
    analysis
        .facts
        .defs
        .iter()
        .filter(|d| d.name == sym)
        .filter(|d| boundary.is_none_or(|b| d.node <= b))
        .max_by_key(|d| d.node)
}

/// The environments open at `node`, outermost first, replayed from the
/// `BeginEnvironment`/`EndEnvironment` occurrences the run recorded up to
/// it: what a name was actually called under, generically, from running
/// the document rather than from a table of environment names.
fn context_at(analysis: &Analysis, node: NodeId) -> Vec<Sym> {
    let mut occurrences: Vec<&crate::facts::Occurrence> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| {
            matches!(o.kind, OccKind::BeginEnvironment | OccKind::EndEnvironment) && o.key != "document" && o.node <= node
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

/// Environments whose static reach (`crate::probe`'s call-graph closure)
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
    crate::probe::all_environments(analysis)
        .into_iter()
        .filter(|&env| env == via || crate::probe::closure_bounded(analysis, env, MECHANISM_DEPTH, MECHANISM_CAP).contains(&via))
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
fn meaning_identity(analysis: &Analysis, def: &crate::facts::Definition) -> String {
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
    let mut defs: Vec<&crate::facts::Definition> = analysis.facts.defs.iter().filter(|d| d.name == sym).collect();
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
    let mut groups: Vec<(String, Vec<&crate::facts::Definition>)> = Vec::new();
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
    let mut groups: Vec<(Option<String>, Vec<Vec<Sym>>)> = Vec::new();
    for ctx in contexts {
        let error = analysis
            .facts
            .raised
            .iter()
            .find(|r| r.name == sym && r.context.as_ref() == ctx.as_slice())
            .map(|r| r.text.clone());
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

/// Environment-context meanings for a name with no real use to read them
/// off of (a run with no `\item` inside a list, say): found by really
/// probing the command from inside every environment
/// [`crate::probe::probe_contexts`] found worth trying, and comparing what
/// each raises against `base`'s own (probed outside any environment)
/// error. `None` when none of them differed.
fn probe_context_meanings(analysis: &Analysis, sym: Sym, base: &Record) -> Option<Vec<Record>> {
    let mut groups: Vec<(Option<String>, Vec<Vec<Sym>>)> = Vec::new();
    for (env, error) in crate::probe::probe_contexts(analysis, sym) {
        let ctx = env.map_or_else(Vec::new, |e| vec![e]);
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
pub fn explain(
    analysis: &Analysis,
    names: &[String],
    all: bool,
) -> Result<(Vec<Record>, Vec<String>), String> {
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
        let name = raw.trim_start_matches('\\');
        let Some(sym) = analysis.interner.lookup(name) else { continue };
        let total = analysis.facts.defs.iter().filter(|d| d.name == sym).count();
        if let Some(at) = at {
            let boundary = explain_boundary(analysis, at)?;
            let window = if matches!(at, ExplainAt::Preamble) { Window::Preamble } else { Window::All };
            match def_at(analysis, sym, boundary) {
                Some(def) => {
                    let mut record = record_for_def(analysis, def, window);
                    record.insert("when".into(), json!(at.to_string()));
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
            let mut defs: Vec<&crate::facts::Definition> =
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
            let preamble_def = preamble_boundary.and_then(|b| def_at(analysis, sym, Some(b)));
            match (preamble_def, doc_def) {
                (None, None) => {
                    let base = record_for_kernel(analysis, sym, Window::All);
                    // Real uses in the run, when there were any of note;
                    // otherwise the same question asked of the probe, so a
                    // run with no `\item` inside a list still finds one.
                    let split = base.as_ref().and_then(|b| {
                        split_by_environment(analysis, sym, b).or_else(|| probe_context_meanings(analysis, sym, b))
                    });
                    match split {
                        Some(split) => out.extend(split),
                        None => out.extend(base),
                    }
                }
                // Only one of the two exists, or they agree: nothing to tell
                // apart, so `when` would only be noise on the one record shown.
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
            hints.push(format!("`\\{name}` has {total} definitions; `--all` lists every one"));
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
        if vertex.tag == crate::graph::VertexTag::VariableDefinition {
            sites.insert(vertex.name, vertex.span);
        }
    }
    for i in 0..analysis.interner.len() as u32 {
        let sym = Sym(i);
        let value = analysis.env.value(sym);
        if matches!(value, crate::value::Value::Unknown) {
            continue;
        }
        // A token register holds text rather than a number, and that text is
        // what it produces.
        let rendered = match &value {
            crate::value::Value::Toks(tokens) => detokenize(tokens, &analysis.interner),
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
        let argument = use_
            .arguments
            .iter()
            .map(|a| detokenize(a, &analysis.interner))
            .find(|a| a.contains(text));
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
            let comment = crate::tex::comment_start(line);
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
                (Some(m), "def") if m.replacement_text.get(1).and_then(|t| t.cs()).is_some_and(|s| analysis.interner.name(s) == "pgfkeys@was@choice") => "is choice",
                (Some(m), "edef") if m.replacement_text.get(1).and_then(|t| t.cs()).is_some_and(|s| analysis.interner.name(s) == "pgfkeysdefaultpath") => "is family",
                (Some(_), _) if key.args.is_some() => "code args",
                (Some(_), _) => "code",
            };
            let site = key.code.or(key.value).map_or(Span::default(), |sym| defined_at(analysis, sym));
            let mut record = place(analysis, site);
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
fn key_family(analysis: &Analysis, sym: Sym) -> Option<(KeyDialect<'_>, String)> {
    // `within` is what dynamically enclosed a call, but a common dispatch
    // idiom (`\@testopt`/`\@ifnextchar` re-invoking the command itself to
    // consume `[…]`) drops the chain — the re-invoked call is recorded with
    // no enclosing macro at all.  The *static* call graph (bodies scanned
    // for names, already transitive) has no such gap, so it is what this
    // walks to find what `sym`'s code can reach; what actually ran is still
    // what answers the question, via the family argument an expansion of
    // one of those names was recorded with.  Indexed by name once so the
    // walk stays linear in the run rather than rescanning every expansion
    // at every node.
    const MAX_VISITED: usize = 4_000;
    let mut by_name: HashMap<Sym, Vec<&crate::facts::Expansion>> = HashMap::new();
    for e in &analysis.facts.expansions {
        by_name.entry(e.name).or_default().push(e);
    }
    let mut seen: BTreeSet<Sym> = BTreeSet::from([sym]);
    let mut frontier = vec![sym];
    while let Some(current) = frontier.pop() {
        if let Some(dialect) = key_dispatch(analysis.interner.name(current)) {
            let found = by_name.get(&current).into_iter().flatten().find_map(|e| {
                let first = e.arguments.first()?;
                let text = detokenize(first, &analysis.interner);
                // `\pgfkeys`/`\pgfqkeys` take one argument that mixes the
                // path with the assignments under it (`/mytool,width=2cm`),
                // so only what precedes the first comma is the path;
                // `\setkeys`-style dispatchers pass the bare family instead,
                // where this is a no-op.
                let family =
                    text.split(',').next()?.trim().trim_start_matches('/').split('/').next()?;
                (!family.is_empty()).then(|| family.to_string())
            });
            if let Some(family) = found {
                return Some((dialect, family));
            }
        }
        let Some(index) = analysis.calls.get(current) else { continue };
        for target in &analysis.calls.edges[index] {
            let callee = analysis.calls.nodes[*target];
            if seen.len() >= MAX_VISITED {
                break;
            }
            if seen.insert(callee) {
                frontier.push(callee);
            }
        }
    }
    None
}

/// `satex query options`: the keys an environment or command's optional
/// argument accepts, from the key family its own code was observed to read
/// them under — `\begin{itemize}[…]` reads enumitem's `itemize` family once
/// enumitem is loaded, the kernel's own `\item[…]` otherwise (no keys: an
/// empty list, not a guess).  Falls back to the call shape `explain` would
/// show when the name takes an optional argument but not through a
/// recognized key dispatcher (`enumerate`'s label template, for one).
pub fn options(analysis: &Analysis, name: &str) -> Vec<Record> {
    let wanted = name.trim_start_matches('\\');
    let Some(sym) = analysis.interner.lookup(wanted) else { return Vec::new() };
    let Some((dialect, family)) = key_family(analysis, sym) else {
        let mac = analysis.env.meaning(sym).as_macro().map(|m| (**m).clone());
        let shape = effective_of(analysis, sym, mac.as_ref(), Some(false));
        if !shape.contains('[') {
            return Vec::new();
        }
        let mut record = Map::new();
        record.insert("name".into(), json!(analysis.interner.cs(sym)));
        record.insert("kind".into(), json!("positional"));
        record.insert(
            "detail".into(),
            json!(format!(
                "takes an optional argument ({shape}) not read through a recognized key family"
            )),
        );
        return vec![record];
    };
    let mut out = match dialect {
        KeyDialect::Pgf => pgfkeys(analysis, Some(&family)),
        KeyDialect::Keyval(prefix) => analysis
            .facts
            .defs
            .iter()
            .filter(|def| def.tag == "key")
            .filter_map(|def| {
                let subject = def.subject?;
                let key = analysis.interner.name(subject);
                (analysis.interner.name(def.name) == format!("{prefix}{family}@{key}")).then(|| {
                    let mut record = place(analysis, def.span);
                    record.insert("key".into(), json!(key));
                    record.insert("package".into(), symbol(analysis, def.package));
                    record
                })
            })
            .collect(),
    };
    for record in &mut out {
        record.insert("family".into(), json!(family));
    }
    out
}

/// Where a control sequence built by a library's own code was asked for: the
/// call read straight from a file that the definition happened inside, so a
/// key made by `\pgfkeys{…}` points at that `\pgfkeys` and not into
/// pgfkeys.code.tex.
fn defined_at(analysis: &Analysis, sym: Sym) -> Span {
    let Some(def) = analysis.facts.defs.iter().rev().find(|def| def.name == sym) else {
        return analysis.kernel_sites.get(&sym).copied().unwrap_or_default();
    };
    analysis
        .graph
        .maker(def.node)
        .and_then(|call| analysis.graph.vertices.get(call as usize))
        .map_or(def.span, |vertex| vertex.span)
}
