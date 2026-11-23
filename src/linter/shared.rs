use crate::fixed::{HashMap, HashSet};
use std::cell::{OnceCell, RefCell};
use std::collections::BTreeMap;

use crate::analysis::facts::{Load, Occurrence, Rest};
use crate::analysis::machine::{Analysis, Recursion};
use crate::engine::builtins::OccKind;
use crate::engine::tex::{FileId, Span, Sym};

/// Whether the author can write `name` as a command in the document: its
/// characters are letters there (catcode 11), or it is one character
/// (`\\,`). `@` (`\\reserved@a`), `_` and `:` (expl3's `\\l__foo_tl`) are
/// letters only in package code, so a finding or fix meant for the author
/// never names such a command.
pub fn writable(name: &str) -> bool {
    name.chars().count() == 1 || name.chars().all(char::is_alphabetic)
}

/// `text` as the author reads it: a name the run could not compute is
/// held under a mark of satex's own (`\u{2}unknown name:r@sec:…`), raw in
/// TeX's messages or `^^B` as TeX prints it. Each becomes its known head and
/// `…` (`\\r@sec:…`), an unknown text `…` alone.
pub fn readable(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    loop {
        let raw = rest.find("\u{2}unknown").map(|at| (at, 1));
        let printed = rest.find("^^Bunknown").map(|at| (at, 3));
        let Some((at, mark)) = [raw, printed].into_iter().flatten().min() else { break };
        out.push_str(&rest[..at]);
        // the mark (`unknown name:`), an unknown text's kind (`text of:`), then the name to a space or quote
        let body = &rest[at + mark..];
        let body = body.split_once(':').map_or(body, |(_, b)| b);
        let kind = [crate::analysis::machine::TYPED_TEXT, crate::analysis::machine::explore::ONE_OF]
            .into_iter()
            .find(|k| body.starts_with(k));
        let body = &body[kind.map_or(0, str::len)..];
        let end = body.find(|c: char| c.is_whitespace() || matches!(c, '`' | '\'' | '"')).unwrap_or(body.len());
        let head: String = match kind {
            None => body[..end].chars().take_while(|c| !c.is_control() && *c != '^').collect(),
            Some(_) => String::new(),
        };
        if kind.is_some() && out.ends_with('\\') {
            out.pop();
        }
        out.push_str(&head);
        out.push('…');
        rest = &body[end..];
    }
    out.push_str(rest);
    // a typed unknown digit's name (`\\-16384..16383#8970.8059.0`), a private-use mark (U+F0000 on), an unknown
    // text's digest (32 hex digits): each the `…` of a value the run did not know, one `…` for a run of them
    static RAW: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\\?-?\d+\.\.-?\d+#[0-9.]*\d|[\u{F0000}-\u{10FFFF}]|\b[0-9a-f]{32}\b")
            .expect("a valid pattern")
    });
    static RUN: std::sync::LazyLock<regex::Regex> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"…(?:[\s.,\\]*…)+").expect("a valid pattern"));
    let out = RAW.replace_all(&out, "…");
    RUN.replace_all(&out, "…").into_owned()
}

/// Whether the message was raised by `\\errmessage`, under whatever name
/// the code called it (`\\tex_errmessage:D`, an alias): what stops a real
/// run (tex.web § 1283).
pub fn is_error(analysis: &Analysis, o: &crate::analysis::facts::Occurrence) -> bool {
    o.kind == OccKind::Message
        && o.detail.as_deref().and_then(|d| analysis.interner.lookup(d)).and_then(|d| analysis.env.meaning(d).prim())
            == Some(crate::engine::builtins::Primitive::Message { error: true })
}

/// Why a finding is not in every run of the document.
#[derive(Clone, PartialEq, Debug)]
pub enum Unsure {
    /// On a path the run could not decide.
    Undecided,
    /// After a row the run rests on ([`Diagnostic::rest`]), which this names.
    After(Rest, String),
}

impl Unsure {
    /// What a finding that would be an error is instead: possible where
    /// TeX may raise it, a note where the analysis may have caused it.
    pub fn severity(&self) -> crate::linter::Severity {
        match self {
            Unsure::After(Rest::Gap, _) => crate::linter::Severity::Info,
            _ => crate::linter::Severity::Warning,
        }
    }

    /// What ends the message of a finding that is not certain.
    pub fn note(&self) -> String {
        match self {
            Unsure::Undecided => " (on a path the run could not decide: TeX may raise it)".into(),
            Unsure::After(Rest::Assumed, cause) => format!(" (holds if {cause} holds)"),
            Unsure::After(Rest::Gap, cause) => format!(" (may follow from {cause}: check that first)"),
        }
    }
}

/// The first row before the `index`th occurrence in run order that the run
/// rests on from there ([`Diagnostic::rest`]: a limit, a cut, an engine
/// error on an approximated path, an assumption about lost state).  An
/// error raised after it may be its echo rather than TeX's.
pub fn doubt(analysis: &Analysis, index: usize) -> Option<Unsure> {
    gap_between(analysis, 0, index)
}

/// Diagnostics of code the run did not follow: an undecided conditional
/// whose arms were skipped, a call or loop summarized, an expansion cut.
/// What that code wrote out is not known.
const SKIPS: &[&str] = &[
    "undecided-condition",
    "paths-budget",
    "site-forks-widened",
    "recursion-widened",
    "region-skipped",
    "fault-contained",
];

/// Whether code the run did not follow ([`SKIPS`]) lies between the
/// `from`th and the `to`th occurrence: what it wrote there is unknown.
pub fn skipped_between(analysis: &Analysis, from: usize, to: usize) -> bool {
    analysis.facts.diagnostics.iter().any(|d| SKIPS.contains(&d.code) && (from..=to).contains(&d.after))
}

/// [`doubt`] between the `from`th and the `to`th occurrence: what the run
/// may have lost there (a closer an open operator waits for). A gap before
/// an assumption.
pub fn gap_between(analysis: &Analysis, from: usize, to: usize) -> Option<Unsure> {
    let rows = analysis.facts.diagnostics.iter().filter(|d| (from..=to).contains(&d.after));
    let (rest, d) =
        rows.filter_map(|d| Some((d.rest()?, d))).min_by_key(|(rest, d)| (std::cmp::Reverse(*rest), d.after))?;
    let (at, what) = (d.origin.unwrap_or(d.span), if rest == Rest::Gap { "analysis gap" } else { "assumption" });
    let place = format!("{}:{}:{}", analysis.short_name(at.file), at.line, at.col);
    Some(Unsure::After(rest, format!("the {what} at {place} ({})", d.code)))
}

pub use crate::analysis::machine::{loaded, macro_text};

pub fn first_loads(analysis: &Analysis) -> HashMap<&str, (Vec<String>, Span)> {
    let mut seen: HashMap<&str, (Vec<String>, Span)> = HashMap::default();
    for load in &analysis.facts.loads {
        if load.kind.is_package() {
            seen.entry(&load.name).or_insert_with(|| (load.options.clone(), load.span));
        }
    }
    seen
}

pub fn definition_order(analysis: &Analysis) -> HashMap<Sym, Span> {
    let mut first: HashMap<Sym, Span> = HashMap::default();
    for def in &analysis.facts.defs {
        first.entry(def.name).or_insert(def.span);
    }
    first
}

pub fn labels(analysis: &Analysis) -> BTreeMap<&str, &Occurrence> {
    let mut out = BTreeMap::new();
    for occurrence in &analysis.facts.occurrences {
        if occurrence.kind == OccKind::Label {
            out.entry(occurrence.key.as_str()).or_insert(occurrence);
        }
    }
    out
}

pub fn document_start(analysis: &Analysis) -> Option<Span> {
    analysis
        .facts
        .occurrences
        .iter()
        .find(|o| o.kind == OccKind::BeginEnvironment && o.key == "document")
        .map(|o| o.span)
}

/// The tables and figures the run entered and left, outermost first.
fn objects(analysis: &Analysis) -> Vec<(Span, Span, &'static str)> {
    let kind = |key: &str| match key.trim_end_matches('*') {
        "table" | "sidewaystable" | "longtable" | "tabular" | "tabularx" => Some("table"),
        "figure" | "sidewaysfigure" => Some("figure"),
        _ => None,
    };
    let found = environments(analysis, |key| kind(key).is_some());
    found.into_iter().map(|(b, e, key)| (b, e, kind(key).unwrap_or("table"))).collect()
}

/// The environments named as `keep` wants that the run entered and left: where each began and
/// ended, outermost first.
pub(crate) fn environments(analysis: &Analysis, keep: impl Fn(&str) -> bool) -> Vec<(Span, Span, &str)> {
    let mut marks: Vec<(Span, bool, &str)> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| matches!(o.kind, OccKind::BeginEnvironment | OccKind::EndEnvironment) && keep(&o.key))
        .map(|o| (o.span, o.kind == OccKind::BeginEnvironment, o.key.as_str()))
        .collect();
    marks.sort_unstable();
    marks.dedup();
    let (mut open, mut out) = (Vec::new(), Vec::new());
    for (span, begin, key) in marks {
        if begin {
            open.push((span, key));
        } else if let Some(i) = open.iter().rposition(|(b, k): &(Span, &str)| *k == key && b.file == span.file) {
            let (b, k) = open.remove(i);
            out.push((b, span, k));
        }
    }
    out.sort_unstable();
    out
}

/// A set of [`Sym`]s as a bitmap: the rules ask after names far more often
/// than the run makes them.
#[derive(Default)]
pub struct Names(Vec<bool>);

impl Names {
    fn of(syms: impl Iterator<Item = Sym> + Clone) -> Names {
        let size = syms.clone().map(|s| s.0 as usize + 1).max().unwrap_or(0);
        let mut bits = vec![false; size];
        for sym in syms {
            bits[sym.0 as usize] = true;
        }
        Names(bits)
    }

    pub fn contains(&self, sym: Sym) -> bool {
        self.0.get(sym.0 as usize).copied().unwrap_or(false)
    }
}

/// The maps the rules would otherwise each rebuild over the whole fact
/// list, built once per lint run and shared through [`crate::linter::Report`].
/// Lazy wherever a map costs more than the rules that want it.
pub struct Index<'a> {
    analysis: &'a Analysis,
    /// Leftmost column of a load on each line it stands on.
    load_col: HashMap<(FileId, u32), u32>,
    expanded: OnceCell<Names>,
    first_loads: OnceCell<HashMap<&'a str, (Vec<String>, Span)>>,
    definition_order: OnceCell<HashMap<Sym, Span>>,
    labels: OnceCell<BTreeMap<&'a str, &'a Occurrence>>,
    bodies: OnceCell<HashSet<Span>>,
    document_start: OnceCell<Option<Span>>,
    recursion: OnceCell<Vec<Recursion>>,
    load_spans: OnceCell<HashSet<Span>>,
    error_spans: OnceCell<HashSet<Span>>,
    unsure: OnceCell<HashMap<Span, Option<Unsure>>>,
    sites: OnceCell<Sites>,
    origins: RefCell<Vec<Option<&'static str>>>,
    live: OnceCell<Vec<bool>>,
    objects: OnceCell<Vec<(Span, Span, &'static str)>>,
    /// The `.bib` databases and the style's model, for the rules on them.
    pub(crate) bib: OnceCell<crate::linter::bibdb::Bib>,
}

impl<'a> Index<'a> {
    pub fn of(analysis: &'a Analysis) -> Index<'a> {
        let mut load_col: HashMap<(FileId, u32), u32> = HashMap::default();
        for load in &analysis.facts.loads {
            load_col
                .entry((load.span.file, load.span.line))
                .and_modify(|col| *col = (*col).min(load.span.col))
                .or_insert(load.span.col);
        }
        Index {
            analysis,
            load_col,
            expanded: OnceCell::new(),
            first_loads: OnceCell::new(),
            definition_order: OnceCell::new(),
            labels: OnceCell::new(),
            bodies: OnceCell::new(),
            document_start: OnceCell::new(),
            recursion: OnceCell::new(),
            load_spans: OnceCell::new(),
            error_spans: OnceCell::new(),
            unsure: OnceCell::new(),
            sites: OnceCell::new(),
            origins: RefCell::new(vec![None; analysis.files.len()]),
            live: OnceCell::new(),
            objects: OnceCell::new(),
            bib: OnceCell::new(),
        }
    }

    /// [`written_definition`], against the prebuilt load lines.
    pub fn written_definition(&self, def: &crate::analysis::facts::Definition) -> bool {
        use crate::engine::builtins::Primitive as P;
        if self.load_col.get(&(def.span.file, def.span.line)).is_some_and(|col| *col <= def.span.col) {
            return false;
        }
        matches!(self.analysis.env.meaning(def.by).prim(), None | Some(P::Def { .. }))
    }

    /// Every name the run expanded.
    pub fn expanded(&self) -> &Names {
        self.expanded.get_or_init(|| Names::of(self.analysis.facts.expansions.iter().map(|e| e.name)))
    }

    pub fn first_loads(&self) -> &HashMap<&'a str, (Vec<String>, Span)> {
        self.first_loads.get_or_init(|| first_loads(self.analysis))
    }

    /// Each package load after that package's first, with the options the
    /// first load gave, where it stands, and that load itself.
    pub fn reloads(&'a self) -> impl Iterator<Item = (&'a Load, &'a [String], Span, Option<&'a Load>)> {
        let loads = &self.analysis.facts.loads;
        loads.iter().filter(|load| load.kind.is_package()).filter_map(|load| {
            let (options, at) = self.first_loads().get(load.name.as_str()).filter(|(_, at)| *at != load.span)?;
            let first = loads.iter().find(|l| l.span == *at && l.name == load.name);
            Some((load, options.as_slice(), *at, first))
        })
    }

    pub fn definition_order(&self) -> &HashMap<Sym, Span> {
        self.definition_order.get_or_init(|| definition_order(self.analysis))
    }

    pub fn labels(&self) -> &BTreeMap<&'a str, &'a Occurrence> {
        self.labels.get_or_init(|| labels(self.analysis))
    }

    /// The document call an occurrence came out of, when its own text is
    /// not the document's: a macro body or a package's code.
    pub fn call(&self, o: &Occurrence) -> Option<(Sym, Span)> {
        let bodies = self.bodies.get_or_init(|| {
            let defs = self.analysis.facts.defs.iter().filter(|d| self.origin(d.span.file) == "document");
            defs.filter_map(|d| d.mac.as_ref()).flat_map(|m| m.replacement_text.iter().map(|t| t.span)).collect()
        });
        let foreign = self.origin(o.span.file) != "document" || bodies.contains(&o.span);
        // The innermost command of the call's own text (`\begin{nb}` inside
        // `\sbox{…}{…}`) is closer than the call, for an occurrence at it too.
        let outer = o.call.is_some_and(|(_, at)| at == o.span);
        let inner = o.site.filter(|(_, at)| (foreign || outer) && self.origin(at.file) == "document");
        inner.or(o.call.filter(|(_, at)| foreign && *at != o.span && self.origin(at.file) == "document"))
    }

    pub fn document_start(&self) -> Option<Span> {
        *self.document_start.get_or_init(|| document_start(self.analysis))
    }

    /// The outermost table or figure around `span`, as `table@line:col`:
    /// findings on one object share it.
    pub fn group(&self, span: Span) -> Option<String> {
        let objects = self.objects.get_or_init(|| objects(self.analysis));
        let (begin, _, kind) = objects.iter().find(|(b, e, _)| b.file == span.file && (*b..=*e).contains(&span))?;
        Some(format!("{kind}@{}:{}", begin.line, begin.col))
    }

    /// [`Analysis::recursion`] runs two strongly-connected-component passes
    /// over the call graph, and several rules want the same answer.
    pub fn recursion(&self) -> &[Recursion] {
        self.recursion.get_or_init(|| self.analysis.recursion())
    }

    pub fn load_spans(&self) -> &HashSet<Span> {
        self.load_spans.get_or_init(|| self.analysis.facts.loads.iter().map(|l| l.span).collect())
    }

    /// Where an `\errmessage`-family occurrence stands.
    pub fn error_spans(&self) -> &HashSet<Span> {
        self.error_spans.get_or_init(|| {
            self.analysis.facts.occurrences.iter().filter(|o| is_error(self.analysis, o)).map(|o| o.span).collect()
        })
    }

    /// Why an error at `span` is not in every run, if it is not. The facts
    /// the run recorded there say: an occurrence, an engine error's row, an
    /// undefined name. Each is certain or not ([`Unsure`]). An error fact
    /// that is certain backs every finding at its place.
    pub fn unsure(&self, span: Span) -> Option<&Unsure> {
        let a = self.analysis;
        let build = || {
            // The first gap and the first assumption in run order. A finding
            // at such a row's own place reports the row: it does not rest on it.
            let first = |rest| {
                let rows = a.facts.diagnostics.iter().filter(|d| d.rest() == Some(rest));
                let d = rows.min_by_key(|d| d.after)?;
                let row = a.facts.diagnostics.iter().position(|r| std::ptr::eq(r, d))?;
                Some(((d.after, row), [Some(d.span), d.origin], gap_between(a, 0, d.after)?))
            };
            let firsts = [first(Rest::Gap), first(Rest::Assumed)];
            let mut at: HashMap<Span, Option<Unsure>> = HashMap::default();
            // `index`: the occurrences before the fact, and for a row its place among the rows.
            let mut fact = |span: Span, error: bool, certain: bool, index: (usize, Option<usize>)| {
                let rests = |((after, row), own, _): &&((usize, usize), [Option<Span>; 2], Unsure)| {
                    index.1.map_or(*after <= index.0, |at| *row < at) && !own.contains(&Some(span))
                };
                let after = || firsts.iter().flatten().find(rests).map(|(_, _, u)| u.clone());
                match if certain { after() } else { Some(Unsure::Undecided) } {
                    None if error => _ = at.insert(span, None),
                    None => {}
                    unsure => _ = at.entry(span).or_insert(unsure),
                }
            };
            let occurrences = &a.facts.occurrences;
            for (i, o) in occurrences.iter().enumerate() {
                fact(o.span, is_error(a, o), o.certain, (i, None));
            }
            let exact =
                |d: &&crate::analysis::facts::Diagnostic| d.severity == crate::analysis::facts::Severity::Warning;
            for (row, d) in a.facts.diagnostics.iter().enumerate().filter(|(_, d)| exact(d)) {
                [Some(d.span), d.origin]
                    .into_iter()
                    .flatten()
                    .for_each(|span| fact(span, true, true, (d.after, Some(row))));
            }
            use crate::analysis::facts::MeaningKind::Undefined;
            // Expansions and vertices are made in run order, and an undefined
            // name has no vertex: the occurrences before the next expansion
            // that has one bound those before it. An expansion under an
            // undecided conditional is a fact too: its place is not every run's.
            let mut index = usize::MAX;
            for e in a.facts.expansions.iter().rev() {
                index = e.node.map_or(index, |n| occurrences.partition_point(|o| o.node <= n));
                fact(e.span, e.meaning == Undefined, e.cds.is_empty(), (index, None));
            }
            at
        };
        self.unsure.get_or_init(build).get(&span)?.as_ref()
    }

    /// [`crate::queries::query::origin`], which compares whole paths, per file
    /// rather than per finding.
    pub fn origin(&self, file: FileId) -> &'static str {
        let mut origins = self.origins.borrow_mut();
        match origins.get_mut(file as usize) {
            Some(Some(known)) => known,
            Some(slot) => slot.insert(crate::queries::query::origin(self.analysis, file)),
            None => crate::queries::query::origin(self.analysis, file),
        }
    }

    /// Whether the document wrote `def`: made outside any package, by a
    /// command the document wrote, under a name its own files hold.
    pub fn own_definition(&self, def: &crate::analysis::facts::Definition) -> bool {
        def.package.is_none() && def.spelled && self.written_definition(def)
    }

    /// Whether what `def` made may be read: its graph vertex is live.
    pub fn used_definition(&self, def: &crate::analysis::facts::Definition) -> bool {
        let live = self.live.get_or_init(|| live_vertices(self));
        live.get(def.node as usize).copied().unwrap_or(true)
    }

    /// Whether another name the same call defined calls `def`, which makes
    /// `def` that name's implementation rather than a definition of its own:
    /// `\newcommand` writes both `\fe` and its `\\fe`.
    pub fn implements_sibling(&self, def: &crate::analysis::facts::Definition) -> bool {
        let sites = self.sites.get_or_init(|| Sites::of(self.analysis));
        sites.group(def.span, def.by).any(|i| {
            let other = &self.analysis.facts.defs[i];
            other.name != def.name
                && other.mac.as_ref().is_some_and(|m| m.replacement_text.iter().any(|t| t.cs() == Some(def.name)))
        })
    }
}

/// The definitions grouped by the call that made them, as a chain through
/// `next` so that the common one-definition site costs no allocation.
struct Sites {
    head: HashMap<(Span, Sym), u32>,
    next: Vec<u32>,
}

impl Sites {
    fn of(analysis: &Analysis) -> Sites {
        let defs = &analysis.facts.defs;
        let mut head: HashMap<(Span, Sym), u32> = HashMap::with_capacity_and_hasher(defs.len(), Default::default());
        let mut next = vec![u32::MAX; defs.len()];
        for (i, def) in defs.iter().enumerate().rev() {
            next[i] = head.insert((def.span, def.by), i as u32).unwrap_or(u32::MAX);
        }
        Sites { head, next }
    }

    fn group(&self, span: Span, by: Sym) -> impl Iterator<Item = usize> {
        let mut at = self.head.get(&(span, by)).copied().unwrap_or(u32::MAX);
        std::iter::from_fn(move || {
            let i = (at != u32::MAX).then_some(at as usize)?;
            at = self.next[i];
            Some(i)
        })
    }
}

/// The dependency-graph vertices a real read reaches: the least fixpoint of
/// "live" over the graph's edges (a reader depends on what it reads).
///
/// A call, a test and a value are live.  A definition is live when a live
/// vertex reads it, and then so is what it is defined by: its body, whose
/// mentions it reads, and for `\let` the read of the meaning it copies.  A
/// definition the document did not write is live whatever reads it, since
/// the package that made it may read it later (`\lstset`'s literate text).
fn live_vertices(index: &Index) -> Vec<bool> {
    use crate::analysis::graph::{EdgeKind, VertexTag};
    let graph = &index.analysis.graph;
    let own: HashSet<crate::engine::env::NodeId> =
        index.analysis.facts.defs.iter().filter(|def| index.own_definition(def)).map(|def| def.node).collect();
    let tag = |v: u32| graph.vertex(v).map(|v| v.tag);
    let definition = |v: u32| tag(v) == Some(VertexTag::VariableDefinition);
    // The reads a definition is made of: live only with it.
    let mut made_of = vec![false; graph.len()];
    for v in (0..graph.len() as u32).filter(|&v| definition(v)) {
        for &(to, kind) in graph.outgoing(v) {
            if kind.intersects(EdgeKind::DEFINED_BY) {
                made_of[to as usize] = true;
            }
        }
    }
    let mut live = vec![false; graph.len()];
    let mut work: Vec<u32> = (0..graph.len() as u32)
        .filter(|&v| match tag(v) {
            Some(VertexTag::VariableDefinition) => !own.contains(&v),
            Some(VertexTag::MacroDefinition) | None => false,
            Some(_) => !made_of[v as usize],
        })
        .collect();
    for &v in &work {
        live[v as usize] = true;
    }
    while let Some(v) = work.pop() {
        // A definition passes life on to what makes it, a reader to what it reads.
        let follow = if definition(v) { EdgeKind::DEFINED_BY } else { EdgeKind::READS };
        for &(to, kind) in graph.outgoing(v) {
            if kind.intersects(follow) && !std::mem::replace(&mut live[to as usize], true) {
                work.push(to);
            }
        }
    }
    live
}

/// A runaway (tex.web §§ 336, 396) in library code whose unclosed text is
/// the document's own: the call at the diagnostic's origin opens a `{`
/// that the file or the paragraph ends inside. The message says so, at
/// that call.
pub fn own_runaway(analysis: &Analysis, d: &crate::analysis::facts::Diagnostic) -> Option<(Span, String)> {
    let at = d.origin.filter(|_| d.code == "runaway-argument" && !analysis.project_file(d.span.file))?;
    let text = crate::project::overlay::read_to_string(analysis.file_path(at.file)?).ok()?;
    let start = text.split_inclusive('\n').take(usize::try_from(at.line).ok()?.checked_sub(1)?).map(str::len).sum();
    let rest = &text[start + crate::engine::source::byte_of_col(&text[start..], at.col)..];
    let name: String = rest.chars().skip(1).take_while(|c| c.is_ascii_alphabetic() || *c == '@').collect();
    let mut chars = rest.chars().skip(1 + name.len());
    // Its arguments: `[…]` and `{…}` groups, up to a `}` that closes the
    // last one opened. Only a macro not `\long` ends at a blank line.
    let par = d.message.starts_with("Paragraph ended");
    let (mut depth, mut blank) = (0u32, false);
    let end = loop {
        let Some(c) = chars.next() else {
            if depth == 0 {
                return None;
            }
            break "the file ends inside it";
        };
        match c {
            '\n' if depth > 0 && blank && par => break "the paragraph ends inside it",
            '%' => {
                chars.by_ref().find(|&c| c == '\n');
                blank = true;
                continue;
            }
            '\n' => blank = true,
            c if c.is_whitespace() => continue,
            '[' if depth == 0 => {
                chars.by_ref().find(|&c| c == ']');
            }
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            '\\' if depth > 0 => {
                chars.next();
            }
            _ if depth == 0 => return None,
            _ => {}
        }
        blank &= c == '\n';
    };
    let first = d.message.split(':').next().unwrap_or(&d.message);
    Some((at, format!("{}: the argument of `\\{name}` opened here never closes, {end}", first.trim_end())))
}

/// The installed package that, with what it loads, defines `name` (no
/// backslash), by the cached index of what each file defines
/// ([`crate::caches::exports`]): a proposal for a fix the user accepts, never applied
/// on its own. The index is built once per process and kept in the cache. It is the
/// package completion offers ([`crate::caches::exports::Exports::package_for`]): of
/// several, the one most loaded, so `\includegraphics` is graphicx's.
pub fn providing_package(analysis: &Analysis, name: &str) -> Option<String> {
    let index = exports(analysis)?;
    let pick = index.package_for(name);
    // a package that defines it with a definer this run never saw (url's `\DeclareUrlCommand`):
    // its own run says, at most `limits.provider_probes` of them, most loaded first
    let tries = analysis.settings.limits.provider_probes as usize;
    let proposed = index.proposed(name, pick.as_deref());
    proposed.into_iter().take(tries).find(|p| own_names(analysis, p).contains(name)).or(pick)
}

/// The names `package` defines in its own files and holds at the end of a run that loads it
/// ([`probe`]).
fn own_names(analysis: &Analysis, package: &str) -> std::sync::Arc<HashSet<String>> {
    probe(analysis, &[], package, "").own
}

/// What one probe run said: whether its call ran clean, and the names the probed package's own
/// files define (the package, or a file it reads itself, never one another package reads) and
/// the run still holds at its end.
#[derive(Clone)]
struct Probed {
    clean: bool,
    /// The package's file was not there to load (a bundle that fetches on demand): the call untried.
    absent: bool,
    own: std::sync::Arc<HashSet<String>>,
    /// Every name the package's files defined during the run, in a group too.
    made: std::sync::Arc<HashSet<String>>,
}

/// A document of the run's class that loads `with`, then `package`, and makes `call`, run once per
/// process for that text and the settings it runs under. It runs without wall-clock limit: what
/// it says depends on the installation and the settings, never on the machine's load. Its step
/// limits bound it.
/// The file a LaTeX "File `⟨name⟩' not found" error names: a lookup that found nothing, not a
/// fault of the code that made it.
pub(crate) fn missing_file(message: &str) -> Option<&str> {
    let rest = &message[message.find("File `")? + "File `".len()..];
    Some(&rest[..rest.find("' not found")?])
}

/// Whether one of the packages `chosen` defines `name` while `call` runs under it (tikz's `\\node`, set
/// inside the `tikzpicture` only): the name is that package's, though no index lists it as its export.
pub fn defined_by(analysis: &Analysis, chosen: &[String], name: &str, call: &str) -> bool {
    chosen.iter().any(|p| probe(analysis, &[], p, call).made.contains(name))
}

fn probe(analysis: &Analysis, with: &[String], package: &str, call: &str) -> Probed {
    use crate::engine::builtins::LoadKind;
    type Runs = HashMap<(u64, Vec<std::path::PathBuf>, String), Probed>;
    static RUNS: std::sync::Mutex<Option<Runs>> = std::sync::Mutex::new(None);
    // LaTeX's standard class where the run names none
    let class = analysis.facts.loads.iter().find(|l| l.kind == LoadKind::Class).map_or("article", |l| l.name.as_str());
    let loads: String = with.iter().filter(|p| *p != package).map(|p| format!("\\usepackage{{{p}}}\n")).collect();
    let text = format!(
        "\\documentclass{{{class}}}\n{loads}\\usepackage{{{package}}}\n\\begin{{document}}\n{call}\n\\end{{document}}\n"
    );
    let mut settings = analysis.settings.clone();
    settings.limits.seconds = 0;
    let digest = crate::caches::format::digest(serde_json::to_string(&settings).unwrap_or_default().as_bytes());
    let key = (digest, analysis.distribution.roots.clone(), text);
    if let Some(p) = RUNS.lock().ok().and_then(|r| r.as_ref()?.get(&key).cloned()) {
        return p;
    }
    let run = crate::analyze(&key.2, None, &settings);
    // a file the probe's document does not have (`example-image` where the playground's bundle
    // has not fetched it) says nothing about whether the package takes the call
    let fault = |e: &crate::analysis::facts::RaisedError| e.certain && missing_file(&e.text).is_none();
    let clean = !run.facts.raised.iter().any(fault) && !run.facts.diagnostics.iter().any(|d| d.code == "missing-end");
    let base =
        |file: FileId| run.files.get(usize::from(file)).map_or("", |f| f.path.rsplit('/').next().unwrap_or(&f.path));
    // a file is the package's own when the nearest package or class up its chain of loads is it
    let own = format!("{package}.sty");
    let mine = |mut file: FileId| {
        for _ in 0..run.files.len() {
            let name = base(file);
            if name.ends_with(".sty") || name.ends_with(".cls") {
                return name == own;
            }
            let Some(load) = run.facts.loads.iter().find(|l| l.file == Some(file)) else { return false };
            file = load.span.file;
        }
        false
    };
    let made: HashSet<Sym> = run.facts.defs.iter().filter(|d| mine(d.span.file)).map(|d| d.name).collect();
    let all = std::sync::Arc::new(made.iter().map(|&s| run.interner.name(s).to_string()).collect::<HashSet<_>>());
    let held = made.into_iter().filter(|&s| !matches!(run.env.meaning(s), crate::engine::tex::Meaning::Undefined));
    let own = std::sync::Arc::new(held.map(|s| run.interner.name(s).to_string()).collect::<HashSet<_>>());
    let absent =
        run.facts.loads.iter().any(|l| {
            l.name == package && l.kind.is_package() && l.status == crate::analysis::facts::LoadStatus::NotFound
        });
    let probed = Probed { clean, absent, own, made: all };
    if let Ok(mut runs) = RUNS.lock() {
        runs.get_or_insert_with(HashMap::default).insert(key, probed.clone());
    }
    probed
}

/// The installed package to load for `name`: a code file's own package, else the one that defines
/// it itself, the most loaded of several ([`crate::caches::exports::Exports::package_for`]).
pub fn provider(analysis: &Analysis, name: &str) -> Option<String> {
    // a name a code file defines is its package's (`tikzpicture` is tikz's, though forest `\let`s it)
    exports(analysis)?.package_for(name)
}

/// The installed package to load for `call`, a call of `name` the run lacks: of the packages that
/// may provide it ([`crate::caches::exports::Exports::candidates`]), the most loaded under which the
/// call runs clean, at most `limits.provider_probes` tried; else [`provider`]. Clean: a document of
/// the run's class that loads it and makes the call reaches `\end{document}` with no certain error,
/// TeX's verdict on the package's real code (graphics' `\includegraphics[width=1cm]{x}` reads to the
/// end for `[llx,lly]`'s comma). `with`: the packages the same write loads for the call's other names.
pub fn provider_for(analysis: &Analysis, name: &str, call: &str, with: &[String]) -> Option<String> {
    let tries = analysis.settings.limits.provider_probes as usize;
    let first = provider(analysis, name);
    let others =
        exports(analysis)?.candidates(name).into_iter().filter_map(|f| f.strip_suffix(".sty").map(str::to_string));
    // The usual provider first: the others are tried only when the call fails under it.
    let packages = first.clone().into_iter().chain(others.filter(|p| Some(p) != first.as_ref())).take(tries);
    // The first the call runs clean under; else the first that could not be tried (not in a bundle
    // yet) before one under which it failed: an untried package may take it, a failed one does not.
    let mut untried = None;
    for p in packages {
        let run = probe(analysis, with, &p, call);
        // alone, when another name's provider beside it spoiled the call (the index names
        // pgflibraryplotmarks for tikz's `\\node`; loaded first it broke the picture, and tcolorbox won)
        if run.clean || !with.is_empty() && probe(analysis, &[], &p, call).clean {
            return Some(p);
        }
        if run.absent && untried.is_none() {
            untried = Some(p);
        }
    }
    untried.or(first)
}

/// Names the installed packages define that `keep` takes (`env`: environments only), the `n` best by `rank`
/// (lowest first), each with the package to load for it ([`crate::caches::exports::Exports::package_for`]).
pub fn offered<K: Ord>(
    analysis: &Analysis,
    keep: impl Fn(&str) -> bool,
    env: bool,
    rank: impl Fn(&str) -> K,
    n: usize,
) -> Vec<(String, String)> {
    let Some(index) = exports(analysis) else { return Vec::new() };
    let mut names = index.named(keep, env);
    names.sort_by_cached_key(|n| (rank(n), n.clone()));
    names.into_iter().take(n).filter_map(|n| Some((index.package_for(&n)?, n)).map(|(p, n)| (n, p))).collect()
}

/// Whether a meaning of `name` requests packages: its replacement text names the kernel's
/// `\@pkgextension`, as `\usepackage` and `\RequirePackage` do (latex.ltx, ltclass.dtx).
pub fn requests_packages(analysis: &Analysis, name: Sym) -> bool {
    let Some(extension) = analysis.interner.lookup("@pkgextension") else { return false };
    analysis.facts.defs.iter().any(|def| {
        def.name == name
            && def.mac.as_ref().is_some_and(|m| m.replacement_text.iter().any(|t| t.cs() == Some(extension)))
    })
}

/// The names a document can write, each with the file to load for it
/// ([`crate::caches::exports::Exports::names`]): `satex query exports`, which the site build ships.
pub fn export_names(analysis: &Analysis) -> Vec<(String, String)> {
    exports(analysis).map(|e| e.names(analysis.settings.limits.provider_probes as usize)).unwrap_or_default()
}

/// Whether package `name` is installed: the installation's index lists `name.sty`.
pub fn installed_package(analysis: &Analysis, name: &str) -> bool {
    exports(analysis).is_some_and(|e| e.files.iter().any(|f| f == &format!("{name}.sty")))
}

/// Whether installed package `a` loads package `b`, itself or through what it loads.
pub fn package_loads(analysis: &Analysis, a: &str, b: &str) -> bool {
    exports(analysis).is_some_and(|e| e.reaches(&format!("{a}.sty"), &format!("{b}.sty")))
}

/// The load of the pgf or TikZ library that declares `name`, an arrow tip,
/// shape or key a library error names: `\usetikzlibrary{arrows.meta}`.
pub fn providing_library(analysis: &Analysis, name: &str) -> Option<String> {
    let tikz = loaded(analysis, "tikz");
    exports(analysis)?.providers(&[name.to_string()]).into_iter().find_map(|(file, _)| library_load(&file, tikz))
}

/// The `kind` names (`arrow`) the installation's pgf and TikZ libraries declare, each with the load
/// that brings it.
pub fn library_declared(analysis: &Analysis, kind: &str) -> Vec<(String, String)> {
    let Some(index) = exports(analysis) else { return Vec::new() };
    let tikz = loaded(analysis, "tikz");
    index
        .declared(kind)
        .into_iter()
        .filter_map(|(name, files)| Some((name, files.iter().find_map(|f| library_load(f, tikz))?)))
        .collect()
}

/// The load of a library file: `\usetikzlibrary` reads pgf's libraries too, so a TikZ document
/// asks for them with it.
fn library_load(file: &str, tikz: bool) -> Option<String> {
    let stem = file.strip_suffix(".code.tex")?;
    let (lib, pgf) = match stem.strip_prefix("tikzlibrary") {
        Some(lib) => (lib, false),
        None => (stem.strip_prefix("pgflibrary")?, !tikz),
    };
    Some(format!("\\use{}library{{{lib}}}", if pgf { "pgf" } else { "tikz" }))
}

/// A load string's command and item: `\\usetikzlibrary{calc}` -> (`usetikzlibrary`, `calc`).
pub fn load_parts(load: &str) -> Option<(&str, &str)> {
    let (cmd, rest) = load.trim().strip_prefix('\\')?.split_once('{')?;
    Some((cmd, rest.strip_suffix('}')?))
}

/// `text` with every comment blanked (same bytes, a `%` to the line's end as spaces): offsets stay.
fn uncommented(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let cut = line.match_indices('%').find(|(i, _)| !line[..*i].ends_with('\\')).map_or(line.len(), |(i, _)| i);
        out.push_str(&line[..cut]);
        let rest = &line[cut..];
        out.extend(rest.chars().map(|c| if c == '\n' { "\n".to_string() } else { " ".repeat(c.len_utf8()) }));
    }
    out
}

/// Where the load of `items` by `\\cmd` (`usepackage`, `usetikzlibrary`, `usepgflibrary`) goes in `text`'s preamble, as
/// (byte offset, text to insert); `None` when its calls list every item already. Packages are seen by `\\usepackage` and
/// `\\RequirePackage`, libraries by either library command (TikZ's reads pgf's). A library missing joins the last call of
/// its command, comma-separated in the list's own style (`{calc}` -> `{calc,positioning}`); else, and for a package, the
/// call goes on a line of its own before `\\begin{document}`. `Err` when `text` has no `\\begin{document}` and no call to
/// join: an `\\input` file, whose loads go in the root's preamble.
pub fn preamble_load(text: &str, cmd: &str, items: &[&str], options: &str) -> Result<Option<(usize, String)>, String> {
    let code = uncommented(text);
    let begin = code.find("\\begin{document}");
    let pre = &code[..begin.unwrap_or(code.len())];
    let package = cmd == "usepackage";
    let kinds: &[&str] = if package { &["usepackage", "RequirePackage"] } else { &["usetikzlibrary", "usepgflibrary"] };
    let (mut have, mut last) = (Vec::new(), None);
    for k in kinds {
        for (i, _) in pre.match_indices(&format!("\\{k}")) {
            let rest = &pre[i + 1 + k.len()..];
            if rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
                continue;
            }
            let mut r = rest.trim_start();
            if let Some(o) = r.strip_prefix('[') {
                r = o.find(']').map_or("", |j| &o[j + 1..]).trim_start();
            }
            let Some(list) = r.strip_prefix('{') else { continue };
            let Some(close) = list.find('}') else { continue };
            have.extend(list[..close].split(',').map(str::trim));
            if *k == cmd && !package {
                last = Some((pre.len() - list.len() + close, &list[..close]));
            }
        }
    }
    let mut missing: Vec<&str> = Vec::new();
    for i in items.iter().map(|i| i.trim()).filter(|i| !i.is_empty() && !have.contains(i)) {
        if !missing.contains(&i) {
            missing.push(i);
        }
    }
    if missing.is_empty() {
        return Ok(None);
    }
    if let Some((close, list)) = last {
        let sep = if list.contains(", ") { ", " } else { "," };
        let lead = if list.trim().is_empty() { "" } else { sep };
        return Ok(Some((close, format!("{lead}{}", missing.join(sep)))));
    }
    let call = format!(
        "\\{cmd}{}{{{}}}",
        if options.is_empty() { String::new() } else { format!("[{options}]") },
        missing.join(",")
    );
    let begin = begin.ok_or_else(|| format!("needs {call} in the preamble"))?;
    Ok(Some((code[..begin].rfind('\n').map_or(0, |i| i + 1), format!("{call}\n"))))
}

/// The installation's index of what each file defines, built once and cached. The playground's
/// bundle holds a few packages only: it reads the names of every installed one from its
/// `exports.tsv`, which the site build ships beside `ls-R`.
fn exports(analysis: &Analysis) -> Option<std::sync::Arc<crate::caches::exports::Exports>> {
    static INDEX: std::sync::Mutex<Option<(Vec<std::path::PathBuf>, std::sync::Arc<crate::caches::exports::Exports>)>> =
        std::sync::Mutex::new(None);
    let roots = &analysis.distribution.roots;
    let mut held = INDEX.lock().ok()?;
    if let Some((_, exports)) = held.as_ref().filter(|(r, _)| r == roots) {
        return Some(exports.clone());
    }
    let cfg = &analysis.settings;
    let cache = crate::caches::format::enabled_cache_dir(cfg);
    let shipped = (analysis.distribution.bundle.as_ref())
        .and_then(|bundle| crate::project::overlay::read_to_string(&bundle.join("exports.tsv")).ok());
    let exports = std::sync::Arc::new(match shipped {
        Some(text) => crate::caches::exports::Exports::of_names(&text),
        None => {
            let definers = crate::caches::exports::definers(analysis, &cfg.conventions.definers);
            crate::caches::exports::Exports::of(roots, cache.as_deref(), &definers)
        }
    });
    *held = Some((roots.clone(), exports.clone()));
    Some(exports)
}

/// The document's files that only define (`stats.tex`, the numbers a script writes for the text to
/// pick from): nine in ten of its non-blank, non-comment lines state a definition. Their unused
/// names are their data, their repeated lines a table of values.
pub fn definition_files(
    analysis: &crate::analysis::machine::Analysis,
    sources: &crate::linter::fix::Edits,
) -> crate::fixed::HashSet<crate::engine::tex::FileId> {
    let mut lines: crate::fixed::HashMap<crate::engine::tex::FileId, crate::fixed::HashSet<u32>> =
        crate::fixed::HashMap::default();
    for def in analysis.facts.defs.iter().filter(|d| d.stated && d.spelled) {
        lines.entry(def.span.file).or_default().insert(def.span.line);
    }
    lines
        .into_iter()
        .filter(|(file, defined)| {
            let Some((_, text)) = sources.text(*file) else { return false };
            let body = (1..=text.line_count() as u32)
                .filter(|&l| {
                    let t = text.line(l).trim();
                    !t.is_empty() && !t.starts_with('%')
                })
                .count();
            defined.len() >= 20 && defined.len() * 10 >= body * 9
        })
        .map(|(file, _)| file)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::preamble_load;

    /// Intuition: a library joins the last list of its command in the list's own style, listed ones (by either library
    /// command, a commented one not) are not repeated, a package or a first library goes on a line before
    /// `\\begin{document}`, and a file without one (an `\\input` file) is an `Err`: the user's rule for every writer.
    #[test]
    fn loads_join_the_preamble_s_list() {
        let doc = |pre: &str| format!("\\documentclass{{article}}\n{pre}\\begin{{document}}\nx\n\\end{{document}}\n");
        let at = |text: &str, cmd: &str, items: &[&str]| {
            preamble_load(text, cmd, items, "").map(|e| e.map(|(at, t)| format!("{}|{t}|{}", &text[..at], &text[at..])))
        };
        let t = doc("\\usetikzlibrary{calc}\n");
        let joined = at(&t, "usetikzlibrary", &["positioning", "calc", "positioning"]).unwrap().unwrap();
        assert!(joined.contains("\\usetikzlibrary{calc|,positioning|}"), "{joined}");
        let spaced = doc("\\usetikzlibrary{calc, arrows.meta}\n");
        assert!(at(&spaced, "usetikzlibrary", &["matrix"]).unwrap().unwrap().contains("arrows.meta|, matrix|}"));
        assert_eq!(at(&doc("\\usepgflibrary{arrows.meta}\n"), "usetikzlibrary", &["arrows.meta"]), Ok(None));
        let commented = doc("% \\usetikzlibrary{calc}\n");
        assert!(
            at(&commented, "usetikzlibrary", &["calc"])
                .unwrap()
                .unwrap()
                .ends_with("|\\usetikzlibrary{calc}\n|\\begin{document}\nx\n\\end{document}\n")
        );
        let pkg = at(&doc("\\usepackage{amsmath,array}\n"), "usepackage", &["array"]);
        assert_eq!(pkg, Ok(None));
        assert!(
            at(&doc(""), "usepackage", &["array"])
                .unwrap()
                .unwrap()
                .contains("|\\usepackage{array}\n|\\begin{document}")
        );
        assert!(at("\\begin{tabular}{l}\n\\end{tabular}\n", "usetikzlibrary", &["calc"]).is_err());
    }
}

#[cfg(test)]
mod readable_tests {
    use super::readable;

    /// Intuition: what the run did not know shows as one `…`, never as satex's own names for it. Pins
    /// fse-2027's "Suggested extra height (\-16384..16383#8970.8059.0… .\… pt)" and color models listed
    /// as 32-digit digests.
    #[test]
    fn unknown_values_read_as_one_ellipsis() {
        let raw = "Suggested extra height (\\-16384..16383#8970.8059.0\u{2}unknown name:text of:x .\\\u{2}unknown name:text of:y pt)";
        assert_eq!(readable(raw), "Suggested extra height (… pt)");
        let hashed = "Undefined color model `\u{2}unknown name:text of:a 498e155c8bbc32aad8afc2b7edaa6b2b, 553a8a6ec0bbc32aad8afc2b7edaa6b2'";
        assert_eq!(readable(hashed), "Undefined color model `…'");
        assert_eq!(readable("operator `\u{10fffc}'"), "operator `…'");
        assert_eq!(readable("width 3.5pt, 42 items"), "width 3.5pt, 42 items");
    }
}
