use std::cell::{OnceCell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};

use crate::builtins::OccKind;
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
    let mut seen: HashMap<&str, (Vec<String>, Span)> = HashMap::new();
    for load in &analysis.facts.loads {
        if load.kind.is_package() {
            seen.entry(&load.name).or_insert_with(|| (load.options.clone(), load.span));
        }
    }
    seen
}

pub fn definition_order(analysis: &Analysis) -> HashMap<Sym, Span> {
    let mut first: HashMap<Sym, Span> = HashMap::new();
    for def in &analysis.facts.defs {
        first.entry(def.name).or_insert(def.span);
    }
    first
}

pub fn labels(analysis: &Analysis) -> BTreeMap<&str, Span> {
    let mut out = BTreeMap::new();
    for occurrence in &analysis.facts.occurrences {
        if occurrence.kind == OccKind::Label {
            out.entry(occurrence.key.as_str()).or_insert(occurrence.span);
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
    labels: OnceCell<BTreeMap<&'a str, Span>>,
    document_start: OnceCell<Option<Span>>,
    recursion: OnceCell<Vec<Recursion>>,
    load_spans: OnceCell<HashSet<Span>>,
    error_spans: OnceCell<HashSet<Span>>,
    sites: OnceCell<Sites>,
    origins: RefCell<Vec<Option<&'static str>>>,
}

impl<'a> Index<'a> {
    pub fn of(analysis: &'a Analysis) -> Index<'a> {
        let mut load_col: HashMap<(FileId, u32), u32> = HashMap::new();
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
            document_start: OnceCell::new(),
            recursion: OnceCell::new(),
            load_spans: OnceCell::new(),
            error_spans: OnceCell::new(),
            sites: OnceCell::new(),
            origins: RefCell::new(vec![None; analysis.files.len()]),
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

    pub fn definition_order(&self) -> &HashMap<Sym, Span> {
        self.definition_order.get_or_init(|| definition_order(self.analysis))
    }

    pub fn labels(&self) -> &BTreeMap<&'a str, Span> {
        self.labels.get_or_init(|| labels(self.analysis))
    }

    pub fn document_start(&self) -> Option<Span> {
        *self.document_start.get_or_init(|| document_start(self.analysis))
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
        let mut head: HashMap<(Span, Sym), u32> = HashMap::with_capacity(defs.len());
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
