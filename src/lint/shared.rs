use crate::fixed::{HashMap, HashSet};
use std::cell::{OnceCell, RefCell};
use std::collections::BTreeMap;

use crate::builtins::OccKind;
use crate::facts::{Load, Occurrence};
use crate::machine::{Analysis, Recursion};
use crate::tex::{FileId, Span, Sym};

/// Whether the message was raised by `\\errmessage`, under whatever name
/// the code called it (`\\tex_errmessage:D`, an alias): what stops a real
/// run (tex.web § 1283).
pub fn is_error(analysis: &Analysis, o: &crate::facts::Occurrence) -> bool {
    o.kind == OccKind::Message
        && o.detail.as_deref().and_then(|d| analysis.interner.lookup(d)).and_then(|d| analysis.env.meaning(d).prim())
            == Some(crate::builtins::Primitive::Message { error: true })
}

/// The first place before the `index`th occurrence in run order where the
/// run could not follow TeX (an `Unsupported` diagnostic: a limit, a cut,
/// an engine error on an approximated path).  An error raised after it may
/// be its echo rather than TeX's.
pub fn doubt(analysis: &Analysis, index: usize) -> Option<String> {
    gap_between(analysis, 0, index)
}

/// [`doubt`] between the `from`th and the `to`th occurrence: what the run
/// may have lost there (a closer an open operator waits for).
pub fn gap_between(analysis: &Analysis, from: usize, to: usize) -> Option<String> {
    let d = analysis
        .facts
        .diagnostics
        .iter()
        .filter(|d| d.severity == crate::facts::Severity::Unsupported && (from..=to).contains(&d.after))
        .min_by_key(|d| d.after)?;
    let at = d.origin.unwrap_or(d.span);
    Some(format!("the analysis gap at {}:{}:{} ({})", analysis.short_name(at.file), at.line, at.col, d.code))
}

/// Whether the document loaded a package.
pub fn loaded(analysis: &Analysis, name: &str) -> bool {
    analysis.facts.loads.iter().any(|load| load.kind.is_package() && load.name == name)
}

/// The text a parameterless macro stands for at the end of the run.
pub fn macro_text(analysis: &Analysis, name: &str) -> Option<String> {
    let sym = analysis.interner.lookup(name)?;
    match analysis.env.meaning(sym) {
        crate::tex::Meaning::Macro(m) if m.arity() == 0 => {
            Some(crate::tex::text_of(&m.replacement_text, &analysis.interner))
        }
        _ => None,
    }
}

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
    let mut marks: Vec<(Span, bool, &str)> = analysis
        .facts
        .occurrences
        .iter()
        .filter(|o| matches!(o.kind, OccKind::BeginEnvironment | OccKind::EndEnvironment) && kind(&o.key).is_some())
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
            out.push((b, span, kind(k).unwrap_or("table")));
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
/// list, built once per lint run and shared through [`crate::lint::Report`].
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
    sites: OnceCell<Sites>,
    origins: RefCell<Vec<Option<&'static str>>>,
    live: OnceCell<Vec<bool>>,
    objects: OnceCell<Vec<(Span, Span, &'static str)>>,
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
            sites: OnceCell::new(),
            origins: RefCell::new(vec![None; analysis.files.len()]),
            live: OnceCell::new(),
            objects: OnceCell::new(),
        }
    }

    /// [`written_definition`], against the prebuilt load lines.
    pub fn written_definition(&self, def: &crate::facts::Definition) -> bool {
        use crate::builtins::Primitive as P;
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

    /// [`crate::query::origin`], which compares whole paths, per file
    /// rather than per finding.
    pub fn origin(&self, file: FileId) -> &'static str {
        let mut origins = self.origins.borrow_mut();
        match origins.get_mut(file as usize) {
            Some(Some(known)) => known,
            Some(slot) => slot.insert(crate::query::origin(self.analysis, file)),
            None => crate::query::origin(self.analysis, file),
        }
    }

    /// Whether the document wrote `def`: made outside any package, by a
    /// command the document wrote, under a name its own files hold.
    pub fn own_definition(&self, def: &crate::facts::Definition) -> bool {
        def.package.is_none() && def.spelled && self.written_definition(def)
    }

    /// Whether what `def` made may be read: its graph vertex is live.
    pub fn used_definition(&self, def: &crate::facts::Definition) -> bool {
        let live = self.live.get_or_init(|| live_vertices(self));
        live.get(def.node as usize).copied().unwrap_or(true)
    }

    /// Whether another name the same call defined calls `def`, which makes
    /// `def` that name's implementation rather than a definition of its own:
    /// `\newcommand` writes both `\fe` and its `\\fe`.
    pub fn implements_sibling(&self, def: &crate::facts::Definition) -> bool {
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
    use crate::graph::{EdgeKind, VertexTag};
    let graph = &index.analysis.graph;
    let own: HashSet<crate::env::NodeId> =
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
pub fn own_runaway(analysis: &Analysis, d: &crate::facts::Diagnostic) -> Option<(Span, String)> {
    let at = d.origin.filter(|_| d.code == "runaway-argument" && !analysis.project_file(d.span.file))?;
    let text = crate::overlay::read_to_string(analysis.file_path(at.file)?).ok()?;
    let start = text.split_inclusive('\n').take(usize::try_from(at.line).ok()?.checked_sub(1)?).map(str::len).sum();
    let rest = &text[start + crate::source::byte_of_col(&text[start..], at.col)..];
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
    Some((at, format!("{first}: the argument of `\\{name}` opened here never closes, {end}")))
}
