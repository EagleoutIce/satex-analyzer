//! Package caches: what reading a `\usepackage` or `\documentclass` did,
//! kept as a chain of segments and replayed by any later run whose state
//! agrees with what each segment looked at.
//!
//! While a package is read, its file is cut into segments at *checkpoints*:
//! the start of a line of the package's own file, nothing pending on the
//! input stack, the interpreter otherwise at rest. Each segment records its
//! *reads* (every part of a binding it looked at while unchanged since
//! load, hooks looked at, packages asked about, options given) and its
//! *effect* (the bindings, hooks, facts, vertices and edges it made, in
//! order, and the state it left).
//!
//! A later load walks the segments in order and checks each one's reads
//! against the current state. A segment that agrees is installed from the
//! cache; at the first that doesn't, the file is read on from the
//! checkpoint before it, and stored as another branch of the package's
//! tree. What a package does before it looks at its options or context —
//! typically everything up to `\ProcessOptions` — is thus shared by every
//! document.
//!
//! Register allocation is an effect, not a read: a replay allocates again
//! from this run's counters, and every register the package allocated is
//! renumbered to match. Names and files are written against each segment's
//! own tables ([`crate::tex::codec`]), so a segment installs into a run
//! that numbers them differently. Every file a segment opened is stamped,
//! and every file name it looked up must still resolve the same.

use crate::fixed::{HashMap, HashSet};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use super::{Carried, CarriedDelta, Frame, Machine, Metadata, RECENT_FILE_SPANS, Site};
use crate::builtins::{DefMode, LoadKind, OccKind};
use crate::env::{Binding, NodeId, ReadKind};
use crate::facts::{
    ColorDef, ColorStack, ColorUse, CsnameRole, Definition, Diagnostic, Expansion, Load, LoadStatus, MeaningKind,
    Occurrence, StackOp,
};
use crate::format::{Fnv, SEVERITIES, leak};
use crate::graph::{ControlDep, EdgeKind, Vertex, VertexTag};
use crate::tex::codec::{self, Mode};
use crate::tex::{CatcodeTable, EndLineChar, FileId, LineMark, MacroDef, Meaning, Mouth, RegKind, Span, Sym, Token};
use crate::value::Value;

const ENCODING_VERSION: u32 = 4;
/// The file number that stands for "the site of this load" in a cached span.
const LOAD_SITE: FileId = FileId::MAX;
/// A file number no run hands out: a file a segment names that this run has
/// not read.
const UNREAD: FileId = FileId::MAX - 1;
/// How many segments one package's trees keep, over every context read.
const MAX_NODES: usize = 200_000;
/// How many trees a package keeps: one per engine, configuration and
/// category-code regime it was read under.
const MAX_TREES: usize = 8;
const MAIN: &str = "<main>";
const SITE: &str = "<site>";

const TAGS: [VertexTag; 5] =
    [VertexTag::Value, VertexTag::Use, VertexTag::MacroCall, VertexTag::VariableDefinition, VertexTag::MacroDefinition];
const STATUSES: [LoadStatus; 8] = [
    LoadStatus::Read,
    LoadStatus::NotFollowed,
    LoadStatus::Skipped,
    LoadStatus::AlreadyLoaded,
    LoadStatus::TooDeep,
    LoadStatus::NotFound,
    LoadStatus::Unreadable,
    LoadStatus::CaseMismatch { on_disk: Sym(0) },
];
const STACK_OPS: [StackOp; 4] = [StackOp::Push, StackOp::Pop, StackOp::Set, StackOp::Current];
const REGISTERS: [RegKind; 11] = [
    RegKind::Count,
    RegKind::Dimen,
    RegKind::Skip,
    RegKind::MuSkip,
    RegKind::Toks,
    RegKind::Box,
    RegKind::Read,
    RegKind::Write,
    RegKind::Char,
    RegKind::MathChar,
    RegKind::UMathChar,
];

fn index_of<T: PartialEq>(all: &[T], item: &T) -> u8 {
    all.iter().position(|x| x == item).unwrap_or(0) as u8
}

/// [`index_of`] compares by value, so it would miss `CaseMismatch {
/// on_disk }` against the placeholder `on_disk: Sym(0)` in [`STATUSES`]
/// unless the real symbol is zero too, silently miscoding the load as
/// `Read`. Match by variant instead; the payload is carried separately in
/// `CachedLoad::case_mismatch_on_disk`.
fn status_index(item: &LoadStatus) -> u8 {
    STATUSES.iter().position(|x| std::mem::discriminant(x) == std::mem::discriminant(item)).unwrap_or(0) as u8
}

/// Which node of a tree this is, as a digest of what it holds. Kept for the
/// node a replay stopped at, so a later store can tell it is still the one read.
fn node_id(node: &Node) -> u64 {
    let mut hash = Fnv::new();
    hash.put(&(node.end, node.last, node.steps));
    hash.write(&node.reads);
    hash.write(&node.effect);
    hash.0
}

/// What a store says when the file changed under it.
const DEGRADED: &str = "the run lowered its precision to finish";
const REWRITTEN: &str = "not stored: another instance rewrote it first";

/// Where a recording may be appended: the tree whose root is `root`, if the
/// node its chain hangs from is still the one that was read. The file may
/// have changed since this run read it — another process storing the same
/// package inserts its tree at the front, shifting every later index, and
/// `MAX_TREES` drops the oldest — so the kept index can't be trusted.
/// `None` means the recording belongs nowhere now: written on a stale
/// index it would hang under someone else's root, and a later run would
/// replay it for a state it was never recorded in.
fn append_target(
    file: &mut CacheFile,
    root: &[(String, u64)],
    parent: Option<u32>,
    parent_id: Option<u64>,
) -> Option<usize> {
    match file.trees.iter().position(|t| t.root == root) {
        Some(t) => {
            let same = match parent {
                None => true,
                Some(k) => file.trees[t].nodes.get(k as usize).map(node_id) == parent_id,
            };
            same.then_some(t)
        }
        None => {
            // A chain that hangs from a node cannot be moved to an empty tree.
            if parent.is_some() {
                return None;
            }
            file.trees.insert(0, Tree { root: root.to_vec(), nodes: Vec::new() });
            file.trees.truncate(MAX_TREES);
            Some(0)
        }
    }
}

/// What a test hands [`BEFORE_STORE`]: the cache file is its to rewrite.
#[cfg(test)]
pub(super) type Rewrite = Box<dyn Fn(&Path) + Send>;

/// What one store decided: the package, the node its chain hung from, the
/// tree it went to, and how long the chain was.
#[cfg(test)]
pub(super) type Decided = (String, Option<u32>, Option<usize>, usize);

/// A test's chance to rewrite the cache file in the window a second process
/// writes in: after the read that chose a segment, before the store that
/// appends to it. Test builds only; release builds have no hook.
#[cfg(test)]
pub(super) static BEFORE_STORE: std::sync::Mutex<Option<Rewrite>> = std::sync::Mutex::new(None);

#[cfg(test)]
fn rewrite_before_store(path: &Path) {
    let hook = BEFORE_STORE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(hook) = hook.as_ref() {
        hook(path);
    }
}

#[cfg(not(test))]
fn rewrite_before_store(_path: &Path) {}

/// Last store's decision, for testing whether the window was exercised.  Test builds only.
#[cfg(test)]
pub(super) static LAST_APPEND: std::sync::Mutex<Vec<Decided>> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
fn note_append(name: &str, parent: Option<u32>, target: Option<usize>, chain: usize) {
    LAST_APPEND.lock().unwrap_or_else(|e| e.into_inner()).push((name.to_string(), parent, target, chain));
}

#[cfg(not(test))]
fn note_append(_name: &str, _parent: Option<u32>, _target: Option<usize>, _chain: usize) {}

/// Per cacheable load's end, its [`Carried`] state, in a fixed order.
#[cfg(test)]
pub(super) static LOAD_ENDS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

#[cfg(test)]
pub(super) fn note_load_end(carried: &Carried, guarded_lines: usize) {
    let state = format!("{:?} lines {guarded_lines}", carried.delta(&Carried::default()));
    LOAD_ENDS.lock().unwrap_or_else(|e| e.into_inner()).push(state);
}

/// A vertex the segment refers to: one in its table or before the load (found by name).
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
enum VRef {
    Table(u32),
    Before(Sym),
}

#[derive(Serialize, Deserialize, Clone)]
struct CachedVertex {
    tag: u8,
    name: Sym,
    key: bool,
    /// Made by this segment, not reached by it.
    new: bool,
    /// The call the load is read on behalf of (where the segment is replayed).
    reader: bool,
    span: Span,
    cds: Vec<(VRef, bool)>,
    within: Option<VRef>,
}

#[derive(Serialize, Deserialize)]
struct CachedBinding {
    meaning: Meaning,
    value: Value,
    defs: Vec<VRef>,
    certain: bool,
    global: bool,
    /// Where the old text goes in the new meaning (stored without the old).
    splice: Option<u32>,
}

#[derive(Serialize, Deserialize)]
struct CachedDefinition {
    /// `redefines` is decided by the initial state.
    relative: bool,
    name: Sym,
    by: Sym,
    tag: String,
    subject: Option<Sym>,
    mode: DefMode,
    span: Span,
    package: Option<Sym>,
    node: VRef,
    mac: Option<Rc<MacroDef>>,
    /// As [`CachedBinding::splice`], for `mac`.
    splice: Option<u32>,
    depth: u16,
    global: bool,
    redefines: bool,
    certain: bool,
    cds: Vec<(VRef, bool)>,
}

/// A call site the segment reached: the fact it makes and the calls there.
#[derive(Serialize, Deserialize)]
struct CachedExpansion {
    name: Sym,
    span: Span,
    fact: Option<CachedFact>,
    count: u32,
    /// Modes of the segment's calls there.
    mode: crate::mode::Modes,
    repeats: u32,
    /// Whether the last call read nothing new (what the next call compares).
    current: bool,
}

#[derive(Serialize, Deserialize)]
struct CachedFact {
    package: Option<Sym>,
    node: Option<VRef>,
    meaning: (u8, Option<VRef>),
    within: Option<Sym>,
    arguments: Vec<Box<[Token]>>,
    cds: Vec<(VRef, bool)>,
}

#[derive(Serialize, Deserialize)]
struct CachedLoad {
    name: String,
    kind: LoadKind,
    options: Vec<String>,
    span: Span,
    by: Option<Sym>,
    path: Option<String>,
    status: u8,
    required: Option<String>,
    provided: Option<String>,
    depth: u16,
    case_mismatch_on_disk: Option<Sym>,
}

#[derive(Serialize, Deserialize)]
struct CachedOccurrence {
    kind: OccKind,
    key: String,
    detail: Option<String>,
    span: Span,
    package: Option<Sym>,
    node: VRef,
    section: Option<String>,
    certain: bool,
}

/// What one segment read from the initial state.
#[derive(Serialize, Deserialize, Default)]
struct Reads {
    /// Names read: their binding parts and digest.
    bindings: Vec<(Sym, u8, u64)>,
    /// Gaps found already reported.
    gaps: Vec<(String, Sym)>,
    /// Text copies (by `[ReadKind::Copy]`): name, original length, growth room.
    copies: Vec<(Sym, u32, i64)>,
}

/// Checkpoint state that changes on almost every line.
#[derive(Serialize, Deserialize, Default)]
struct Scalars {
    /// Steps, progress, assignments since the load began.
    steps: u64,
    progress: u64,
    assignments: u64,
    recent: Vec<Sym>,
    after_assignment: Option<Token>,
    last_file: Option<Span>,
    file_call: Option<(Sym, Span)>,
    identifying: Option<i64>,
    call_shape: Option<(Sym, Span, String, usize)>,
    /// Segment completed the pending call at its start.
    finished_call: bool,
    last_named_cs: Option<Sym>,
    /// File spans yielded back (lookahead from files); before are the run's own.
    recent_file_spans: Vec<Span>,
    /// Whether material may have reached the page (tex.web § 986).
    page_touched: bool,
}

/// The state a checkpoint leaves that changes now and then: stored only
/// when it differs from the checkpoint before.
#[derive(Serialize, Deserialize, Clone, PartialEq, Default)]
struct Lists {
    catcodes: Vec<(char, u8)>,
    end_line: i32,
    loaded: Vec<String>,
    class_options: Vec<String>,
    verbatim: Vec<String>,
    section: Option<String>,
    lua: Vec<PathBuf>,
    counter_namespace: Option<String>,
    streams: Vec<(i64, Vec<String>, usize)>,
    outputs: Vec<(String, Option<String>)>,
    aliases: Vec<(Sym, Sym)>,
    wild: crate::machine::Wild,
}

/// One cached diagnostic: severity, code, message, span, and the gap it names, if any.
type CachedDiagnostic = (u8, String, String, Span, Option<(String, Sym)>, u32);

/// A color definition: name, value, model, where defined and via which command.
#[derive(Serialize, Deserialize)]
struct CachedColorDef {
    name: String,
    value: Option<crate::value::Colors>,
    declared: Option<(crate::value::Model, String)>,
    spec_span: Option<Span>,
    span: Span,
    via: Option<Sym>,
    package: Option<Sym>,
    certain: bool,
}

/// A `\color@⟨name⟩` the segment formed and whether the color was defined.
#[derive(Serialize, Deserialize)]
struct CachedColorUse {
    name: String,
    role: (u8, bool, bool),
    defined: bool,
    span: Span,
    via: Option<Sym>,
    package: Option<Sym>,
    certain: bool,
}

/// A `\pdfcolorstack` the segment executed: stack, operation index, span.
type CachedColorStack = (Option<i64>, u8, Span, bool);

/// What a load changed in one field of [`Carried`]: a cache stores the
/// change, a replay applies it. Changes are sorted, so their bytes do not
/// depend on hashing.
pub(crate) trait Delta {
    type Change;
    fn delta(&self, before: &Self) -> Self::Change;
    fn apply(&mut self, change: &Self::Change);
}

/// Members added (`true`) and removed (`false`).
impl<T: std::hash::Hash + Ord + Clone> Delta for HashSet<T> {
    type Change = Vec<(T, bool)>;
    fn delta(&self, before: &Self) -> Self::Change {
        let mut change: Vec<_> = self.symmetric_difference(before).map(|t| (t.clone(), self.contains(t))).collect();
        change.sort();
        change
    }
    fn apply(&mut self, change: &Self::Change) {
        for (t, added) in change {
            if *added {
                self.insert(t.clone())
            } else {
                self.remove(t)
            };
        }
    }
}

/// Entries set to a new value, or removed (`None`).
impl<K: std::hash::Hash + Ord + Clone, V: PartialEq + Clone> Delta for HashMap<K, V> {
    type Change = Vec<(K, Option<V>)>;
    fn delta(&self, before: &Self) -> Self::Change {
        let keys = self.keys().chain(before.keys().filter(|k| !self.contains_key(*k)));
        let mut change: Vec<_> =
            keys.filter(|k| self.get(*k) != before.get(*k)).map(|k| (k.clone(), self.get(k).cloned())).collect();
        change.sort_by(|a, b| a.0.cmp(&b.0));
        change
    }
    fn apply(&mut self, change: &Self::Change) {
        for (k, v) in change {
            if let Some(v) = v {
                self.insert(k.clone(), v.clone())
            } else {
                self.remove(k)
            };
        }
    }
}

/// A scalar: its new value, when it changed.
macro_rules! scalar_delta {
    ($($ty:ty),*) => {$(
        impl Delta for $ty {
            type Change = Option<Self>;
            fn delta(&self, before: &Self) -> Self::Change {
                (self != before).then(|| self.clone())
            }
            fn apply(&mut self, change: &Self::Change) {
                if let Some(value) = change {
                    *self = value.clone();
                }
            }
        }
    )*};
}
scalar_delta!(u32, Option<Span>, Option<(i64, usize, usize, u64)>);

/// Head of an [`Effect`]: what the first pass needs to relocate the segment's names.
/// Decoded with `take_from_bytes` so the rest is read once.
#[derive(Deserialize)]
struct EffectHead {
    names: Vec<Sym>,
    files: Vec<(String, LoadKind, Option<Span>)>,
    moved: Vec<(u8, i64, i64, Vec<VRef>)>,
    origins: Vec<((RegKind, u16), u8)>,
}

/// What one segment did. Its bulk lists (skipped here) travel in [`Sections`].
#[derive(Serialize, Deserialize, Default)]
struct Effect {
    /// Names in order asked for.
    names: Vec<Sym>,
    files: Vec<(String, LoadKind, Option<Span>)>,
    /// Allocation counters: which, from, to, and assignments that set them.
    moved: Vec<(u8, i64, i64, Vec<VRef>)>,
    /// Which counter handed out each register the segment named.
    origins: Vec<((RegKind, u16), u8)>,
    #[serde(skip)]
    vertices: Vec<CachedVertex>,
    /// Table entries reached, in order, with the file read call.
    reached: Vec<(u32, Option<VRef>)>,
    /// Every edge added, in order, with the charged call.
    #[serde(skip)]
    edges: Vec<(VRef, VRef, u16, Option<VRef>)>,
    extents: Vec<(Span, u32)>,
    calls: Vec<crate::graph::CallEvent>,
    observed: Vec<crate::graph::CallEvent>,
    /// Names assigned: current binding and bits the load wrote.
    bindings: Vec<(Sym, Option<CachedBinding>, u8)>,
    #[serde(skip)]
    definitions: Vec<CachedDefinition>,
    #[serde(skip)]
    expansions: Vec<CachedExpansion>,
    loads: Vec<CachedLoad>,
    /// `\Provides…` banners, relative to this load's own (-1).
    identifications: Vec<(i64, Option<String>)>,
    occurrences: Vec<CachedOccurrence>,
    /// Color facts the segment recorded (replay reports without re-running).
    colors: Vec<CachedColorDef>,
    color_uses: Vec<CachedColorUse>,
    color_stack: Vec<CachedColorStack>,
    diagnostics: Vec<CachedDiagnostic>,
    metadata: Vec<(String, String, String)>,
    csnames: Vec<(Span, u8, bool, bool)>,
    shapes: Vec<(Sym, String)>,
    gaps: Vec<(String, Sym)>,
    scalars: Scalars,
    lists: Option<Lists>,
    carried: CarriedDelta,
}

/// An [`Effect`] with its bulk lists cut into chunks, so a replay decodes them in parallel.
#[derive(Serialize, Deserialize)]
struct Sections<'a> {
    #[serde(borrow, with = "serde_bytes")]
    rest: Cow<'a, [u8]>,
    #[serde(borrow)]
    vertices: Vec<&'a serde_bytes::Bytes>,
    #[serde(borrow)]
    edges: Vec<&'a serde_bytes::Bytes>,
    #[serde(borrow)]
    definitions: Vec<&'a serde_bytes::Bytes>,
    #[serde(borrow)]
    expansions: Vec<&'a serde_bytes::Bytes>,
}

/// Effect bytes below this decode on one thread.
const PARALLEL_BYTES: usize = 1 << 20;
const MAX_THREADS: usize = 8;

/// A value moved whole to another thread.
struct Moved<T>(T);
// SAFETY: only freshly decoded values cross, so no `Rc` in one is shared with another thread.
unsafe impl<T> Send for Moved<T> {}

fn chunked<T: Serialize>(items: &[T], per: usize) -> Result<Vec<Vec<u8>>, postcard::Error> {
    items.chunks(per).map(postcard::to_allocvec).collect()
}

fn encode_effect(e: &Effect) -> Result<Vec<u8>, postcard::Error> {
    let rest = postcard::to_allocvec(e)?;
    let (vertices, edges) = (chunked(&e.vertices, 8192)?, chunked(&e.edges, 131_072)?);
    let (definitions, expansions) = (chunked(&e.definitions, 1024)?, chunked(&e.expansions, 4096)?);
    fn of(v: &[Vec<u8>]) -> Vec<&serde_bytes::Bytes> {
        v.iter().map(|b| serde_bytes::Bytes::new(b)).collect()
    }
    postcard::to_allocvec(&Sections {
        rest: Cow::Owned(rest),
        vertices: of(&vertices),
        edges: of(&edges),
        definitions: of(&definitions),
        expansions: of(&expansions),
    })
}

fn decode_part<T: serde::de::DeserializeOwned>(bytes: &[u8], syms: &[Sym], files: &[FileId]) -> Option<T> {
    codec::with(Mode::Decode { syms: syms.to_vec(), files: files.to_vec() }, || postcard::from_bytes(bytes)).0.ok()
}

/// One decoded chunk of a bulk list.
enum Chunk {
    Vertices(Vec<CachedVertex>),
    Edges(Vec<(VRef, VRef, u16, Option<VRef>)>),
    Definitions(Vec<CachedDefinition>),
    Expansions(Vec<CachedExpansion>),
}

fn decode_chunk(kind: u8, bytes: &[u8], syms: &[Sym], files: &[FileId]) -> Option<Chunk> {
    Some(match kind {
        0 => Chunk::Vertices(decode_part(bytes, syms, files)?),
        1 => Chunk::Edges(decode_part(bytes, syms, files)?),
        2 => Chunk::Definitions(decode_part(bytes, syms, files)?),
        _ => Chunk::Expansions(decode_part(bytes, syms, files)?),
    })
}

fn decode_effect(bytes: &[u8], syms: &[Sym], files: &[FileId]) -> Option<Effect> {
    let s: Sections = postcard::from_bytes(bytes).ok()?;
    let mut e: Effect = decode_part(&s.rest, syms, files)?;
    let jobs: Vec<(u8, &[u8])> = [&s.vertices, &s.edges, &s.definitions, &s.expansions]
        .iter()
        .enumerate()
        .flat_map(|(kind, list)| list.iter().map(move |b| (kind as u8, &***b)))
        .collect();
    let threads = if bytes.len() < PARALLEL_BYTES {
        1
    } else {
        std::thread::available_parallelism().map_or(1, |n| n.get()).min(MAX_THREADS).min(jobs.len())
    };
    let next = std::sync::atomic::AtomicUsize::new(0);
    let run = crate::budget::Run::current();
    let work = || {
        let _entered = run.as_ref().map(crate::budget::Run::enter);
        let mut done = Vec::new();
        loop {
            let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let Some(&(kind, bytes)) = jobs.get(i) else { break };
            done.push((i, Moved(decode_chunk(kind, bytes, syms, files))));
        }
        done
    };
    let mut done = if threads <= 1 {
        work()
    } else {
        std::thread::scope(|sc| {
            let helpers: Vec<_> = (1..threads).map(|_| sc.spawn(work)).collect();
            let mut all = work();
            for h in helpers {
                all.extend(h.join().ok()?);
            }
            Some(all)
        })?
    };
    done.sort_by_key(|(i, _)| *i);
    for (_, Moved(chunk)) in done {
        match chunk? {
            Chunk::Vertices(v) => e.vertices.extend(v),
            Chunk::Edges(v) => e.edges.extend(v),
            Chunk::Definitions(v) => e.definitions.extend(v),
            Chunk::Expansions(v) => e.expansions.extend(v),
        }
    }
    Some(e)
}

/// One segment of a package's tree. Read from a file, its byte fields
/// borrow the file's bytes: the file is held once, not decoded into a copy.
#[derive(Clone, Serialize, Deserialize)]
struct Node<'a> {
    parent: Option<u32>,
    /// Checkpoint marking where the segment ends.
    end: LineMark,
    /// Whether this is the file's end.
    last: bool,
    /// Tokens the segment read.
    steps: u64,
    /// Every file the segment opened, as [`crate::format::Roots`] writes
    /// it, with its [`crate::format::content_hash`].
    stamps: Vec<(String, u64)>,
    lookups: Vec<(String, LoadKind, Option<PathBuf>)>,
    names: Vec<String>,
    paths: Vec<String>,
    /// Via `serde_bytes` for bulk copy (postcard's derived visit is per-byte).
    #[serde(borrow, with = "serde_bytes")]
    reads: Cow<'a, [u8]>,
    #[serde(borrow, with = "serde_bytes")]
    effect: Cow<'a, [u8]>,
}

/// The segments of one package read under one engine, configuration and
/// category-code regime.
#[derive(Serialize, Deserialize)]
struct Tree<'a> {
    root: Vec<(String, u64)>,
    #[serde(borrow)]
    nodes: Vec<Node<'a>>,
}

#[derive(Serialize, Deserialize)]
struct CacheFile<'a> {
    version: u32,
    #[serde(borrow)]
    trees: Vec<Tree<'a>>,
}

/// Interpreter logs beyond checkpoint state, while a package is recorded.
#[derive(Clone, Default)]
pub(super) struct Log {
    /// Definition facts in order, including pre-existing sites; whether redefinition was in initial state.
    pub(super) definitions: Vec<(Definition, bool)>,
    /// Call sites this run already had: the fact and count.
    pub(super) touched: HashMap<(Span, Sym), (Expansion, u32)>,
    /// Call sites reached since last checkpoint, in order, with prior count.
    sites: Vec<((Span, Sym), u32)>,
    sites_seen: HashSet<(Span, Sym)>,
    /// Modes of the calls at each site.
    site_modes: HashMap<(Span, Sym), crate::mode::Modes>,
    /// Sites first reached (for identifying a site's first segment).
    first_seen: HashSet<(Span, Sym)>,
    pub(super) shapes: Vec<(Sym, String)>,
    pub(super) gaps_seen: Vec<(&'static str, Sym)>,
    pub(super) gaps_new: Vec<(&'static str, Sym, usize)>,
    csnames: Vec<(Span, CsnameRole)>,
    csname_spans: HashSet<Span>,
    origins: Vec<((RegKind, u16), u8)>,
    identifications: Vec<(usize, Option<String>)>,
    opened: Vec<String>,
    /// Set if the package changed pre-segment state (cannot be expressed): stops recording.
    spoiled: Option<&'static str>,
    /// Definitions with text from before the segment: position and length.
    def_splices: HashMap<usize, (usize, usize)>,
}

/// Initial state of a load; reads are compared against it.
#[derive(Clone)]
struct Before {
    slots: Vec<Option<Binding>>,
    aliases: HashMap<Sym, Sym>,
    /// Name of each pre-load definition vertex.
    def_owner: HashMap<NodeId, Sym>,
    vertices: usize,
    loads: usize,
    counters: Vec<(u8, Option<i64>)>,
    steps: u64,
    progress: u64,
    assignments: u64,
    recent_file_spans: Vec<Span>,
    /// Call the load is read on behalf of.
    reader: Option<NodeId>,
    carried: Carried,
}

/// Where each log stood at the last checkpoint.
#[derive(Default, Clone)]
struct Marks {
    names: usize,
    reached: usize,
    edges: usize,
    links: usize,
    extents: usize,
    calls: usize,
    observed: usize,
    definitions: usize,
    shapes: usize,
    gaps_seen: usize,
    gaps_new: usize,
    csnames: usize,
    origins: usize,
    identifications: usize,
    opened: usize,
    loads: usize,
    occurrences: usize,
    colors: usize,
    color_uses: usize,
    color_stack: usize,
    diagnostics: usize,
    metadata: usize,
    files: usize,
    steps: u64,
}

/// Interpreter state that must match for a checkpoint (at package file push).
#[derive(Clone, PartialEq, Debug)]
struct Rest {
    input: usize,
    depth: usize,
    conds: usize,
    cds: usize,
    testing: usize,
    prefixes: (bool, bool, bool, bool),
    long_argument: bool,
    runaway: crate::machine::Runaway,
    within: usize,
    held: usize,
    edef_depth: usize,
    scanning: usize,
    scan_undecided: bool,
    pdf_string: bool,
    def_body_depth: Option<usize>,
    noexpanded: bool,
    choices: usize,
    scans: usize,
    packages: usize,
    file_depth: u16,
    aux: (bool, usize),
    pinned: usize,
    docstrip: bool,
    budget: Option<u64>,
    forking: bool,
    halted: bool,
    command_level: bool,
    held_tokens: usize,
    reader: bool,
}

/// A package being recorded.
#[derive(Clone)]
pub(super) struct Recorder {
    pub(super) file: FileId,
    pub(super) expansions_start: usize,
    pub(super) log: Log,
    frame: usize,
    name: String,
    cache: PathBuf,
    site: Span,
    root: Vec<(String, u64)>,
    parent: Option<u32>,
    /// The parent node ID (index alone insufficient; another process may write the file).
    parent_id: Option<u64>,
    before: Before,
    rest: Rest,
    marks: Marks,
    last_line: u32,
    /// `class article` or `package tikz`, for the progress bar.
    label: String,
    started: std::time::Instant,
    chain: Vec<Node<'static>>,
    lists: Option<Lists>,
    lookups: HashSet<(String, LoadKind)>,
    /// Initial segment state; reads of load changes compare against it.
    seg_gaps: HashSet<(&'static str, Sym)>,
    seg_vertices: NodeId,
    /// Shape being read at segment start; completion finishes this run's shape.
    pub(super) seg_call: Option<(Sym, Span)>,
    seg_call_finished: bool,
    /// Names with copied text at segment start (compare reads against).
    seg_start: HashMap<Sym, Option<Binding>>,
    /// Names the load has assigned before this segment, as they stood when it
    /// began: a read of a part not yet assigned sees them as a replay does.
    seg_prior: HashMap<Sym, Option<Binding>>,
    /// Names assigned by segments encoded since the last start.
    seg_assigned: Vec<Sym>,
    /// Texts the segment copied: original length and growth room.
    seg_copies: HashMap<Sym, (usize, i64)>,
    /// Tokens served from cache before recording.
    served: u64,
}

fn family(name: &str) -> String {
    let safe: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    format!("package-{safe}-v{ENCODING_VERSION}-")
}

/// Delete the caches no build can use and bound the rest: see
/// [`crate::format::prune_caches`].
pub fn prune(dir: &Path, bound: u64) -> crate::format::Pruned {
    crate::format::prune_caches(dir, ENCODING_VERSION, bound)
}

fn cache_file(dir: &Path, name: &str, key: u64) -> PathBuf {
    dir.join(format!("{}{}-{key:016x}.postcard", family(name), crate::format::cache_key()))
}

fn span_out(span: Span, site: Span) -> Span {
    if span == site { Span::new(LOAD_SITE, 0, 0) } else { span }
}

fn span_in(span: Span, site: Span) -> Span {
    if span.file == LOAD_SITE { site } else { span }
}

fn role_out(role: CsnameRole) -> (u8, bool, bool) {
    match role {
        CsnameRole::Use => (0, false, false),
        CsnameRole::Define { global, expand } => (1, global, expand),
        CsnameRole::Let => (2, false, false),
        CsnameRole::LetTo => (3, false, false),
        CsnameRole::Test => (4, false, false),
    }
}

fn role_in(kind: u8, global: bool, expand: bool) -> CsnameRole {
    match kind {
        1 => CsnameRole::Define { global, expand },
        2 => CsnameRole::Let,
        3 => CsnameRole::LetTo,
        4 => CsnameRole::Test,
        _ => CsnameRole::Use,
    }
}

/// The digest of a binding as the read set compares it: its meaning, value
/// and alias, with every name and file written out, and an undefined name
/// the same whether or not it has a slot.
fn binding_digest(binding: Option<&Binding>, alias: Option<&str>, kinds: u8) -> u64 {
    let mut hash = Fnv::new();
    // Which definitions a name has is what the dependency graph links to; a
    // replay links that again, so a meaning read does not look at it.
    if kinds & MEANING != 0 {
        match binding {
            Some(b) if b.meaning != Meaning::Undefined => hash.put(&b.meaning),
            _ => hash.write(&[0]),
        }
        hash.put(&alias);
    }
    if kinds & VALUE != 0 {
        hash.put(&binding.map_or(&Value::Unknown, |b| &b.value));
    }
    // What assigning a value keeps: an empty slot becomes a new register
    // slot.
    if kinds & DEFS != 0 {
        hash.put(&binding.is_some_and(|b| !b.defs.is_empty()));
    }
    if kinds & SKIP != 0 {
        hash.put(&crate::machine::CondDepth::role(binding.and_then(|b| b.meaning.prim())));
    }
    if kinds & DEFINED != 0 {
        hash.put(&binding.is_some_and(|b| b.meaning != Meaning::Undefined));
    }
    if kinds & SHAPE != 0 {
        match binding.map(|b| &b.meaning) {
            Some(Meaning::Macro(_)) => hash.write(&[1]),
            Some(meaning) if *meaning != Meaning::Undefined => {
                hash.put(meaning);
                hash.put(&alias);
            }
            _ => hash.write(&[0]),
        }
    }
    if kinds & COPY != 0 {
        hash.put(&binding.is_some_and(|b| crate::env::copied_text(&b.meaning).is_some()));
    }
    if kinds & KEPT != 0 {
        match binding {
            Some(b) => hash.put(&(&b.meaning, b.defs.is_empty(), b.certain)),
            None => hash.put(&(&Meaning::Unknown, true, true)),
        }
    }
    hash.0
}

const MEANING: u8 = 1;
const VALUE: u8 = 2;
const KEPT: u8 = 4;
const DEFINED: u8 = 8;
const SKIP: u8 = 16;
const DEFS: u8 = 32;
const COPY: u8 = 64;
const SHAPE: u8 = 128;

/// tex.web § 241: the parameters a job sets from the clock. Their values
/// are the run's, not the document's: a segment that read one is keyed on
/// the read, not the value, and replays the effect of the run that made it,
/// as that run's clock had it. A replay says so as an assumption.
fn is_clock(name: &str) -> bool {
    matches!(name, "time" | "day" | "month" | "year")
}

/// The parts of a read of `name` its digest covers.
fn digested_kinds(name: &str, kinds: u8) -> u8 {
    if is_clock(name) { kinds & !VALUE } else { kinds }
}

/// What each kind bit reads, lowest bit first, for saying which read a stale
/// segment made.
const KINDS: [&str; 8] =
    ["meaning", "value", "kept part", "definedness", "conditional role", "definition sites", "copied text", "shape"];

/// The codec mode that writes names and paths out in full.
fn hash_mode(m: &Machine) -> Mode {
    let roots = m.cache_roots();
    let files = (0..m.out.files.len())
        .map(|i| match i as FileId == m.out.main_file {
            true => Rc::from(MAIN),
            false => Rc::from(roots.canonical(&m.out.files[i].path)),
        })
        .collect();
    Mode::Hash { names: m.out.interner.names(), files }
}

/// `\count10` to `\count19`: the last register of each kind allocated,
/// which `\newcount` and its relatives advance (The TeXbook, appendix B;
/// `latex.ltx`, `\e@alloc`).
const ALLOCATION_COUNTERS: std::ops::RangeInclusive<i64> = 10..=19;

/// The kinds of register each allocation counter hands out: `\count14`
/// boxes, `\count16` and `\count17` streams, `\count18` languages and
/// `\count19` inserts, which are a box, a count, a dimen and a skip at once
/// — and a `\chardef` of the number for the ones that are not registers.
fn allocates(counter: u8) -> &'static [RegKind] {
    match counter {
        10 => &[RegKind::Count],
        11 => &[RegKind::Dimen],
        12 => &[RegKind::Skip],
        13 => &[RegKind::MuSkip],
        14 => &[RegKind::Box, RegKind::Char],
        15 => &[RegKind::Toks],
        16 => &[RegKind::Read, RegKind::Char],
        17 => &[RegKind::Write, RegKind::Char],
        18 => &[RegKind::Char],
        _ => &[RegKind::Box, RegKind::Count, RegKind::Dimen, RegKind::Skip, RegKind::Char],
    }
}

/// `text` with every register reference printed in it, as `\\count24`,
/// renumbered by `relocate`.
fn relocate_text(text: &str, relocate: &dyn Fn(RegKind, i64) -> i64) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('\\') {
        out.push_str(&rest[..=at]);
        rest = &rest[at + 1..];
        let Some(kind) =
            REGISTERS[..6].iter().copied().filter(|k| rest.starts_with(k.as_str())).max_by_key(|k| k.as_str().len())
        else {
            continue;
        };
        let digits = rest[kind.as_str().len()..].bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            continue;
        }
        let end = kind.as_str().len() + digits;
        let index: i64 = rest[kind.as_str().len()..end].parse().unwrap_or(0);
        out.push_str(kind.as_str());
        out.push_str(&relocate(kind, index).to_string());
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// The register a storage name stands for, `\count45` for its slot.
fn storage_register(name: &str) -> Option<(RegKind, i64)> {
    let rest = name.strip_prefix(super::REGISTER)?;
    REGISTERS.iter().find_map(|kind| {
        let digits = rest.strip_prefix(kind.as_str())?;
        if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        Some((*kind, digits.parse().ok()?))
    })
}

impl Machine<'_> {
    /// The roots cached paths are written against: this run's TeX trees
    /// and the document's directory.
    pub(crate) fn cache_roots(&self) -> crate::format::Roots {
        crate::format::Roots::new(
            &self.out.distribution.roots,
            self.out.distribution.bundle.as_deref(),
            Some(&self.base),
        )
    }

    /// Whether the file can be served from or recorded into cache: outer-level package/class, no conditionals.
    fn cacheable_at_push(&self, kind: LoadKind) -> bool {
        let reason = if !matches!(kind, LoadKind::Package | LoadKind::Class)
            || !self.cfg.cache.enabled
            || self.cfg.trace
            || self.budget.is_some()
            || self.package_capture.is_some()
            || self.file_depth != self.document_file_depth + 1
        {
            return false;
        } else if self.on_paths() || !self.cds.is_empty() || self.conds.iter().any(|c| c.dep) {
            "it is loaded under a conditional"
        } else if self.env.depth() != 0 {
            "it is loaded inside a group"
        } else if !self.held.is_empty() || self.edef_depth != 0 || self.scanning != 0 {
            "it is loaded from inside an expansion"
        } else if !self.testing.is_empty() || self.pdf_string {
            "it is loaded while a test is read"
        } else if self.halted || self.out.exhausted {
            "the analysis ran out of budget"
        } else if self.pressure > 0 {
            DEGRADED
        } else {
            return true;
        };
        if self.cfg.verbose >= 1 {
            crate::progress::note!(
                "satex: a load is not cached: {reason} ({} conditionals open, {} undecided, branch depth {})",
                self.conds.len(),
                self.cds.len(),
                self.env.forking()
            );
        }
        false
    }

    /// State every package sees, no read set tracks: config, engine, prefixes, first expansion repeats.
    fn fixed_state(&self, site: Span) -> Vec<(String, u64)> {
        let (digest, _) = codec::with(hash_mode(self), || {
            let mut parts: Vec<(String, u64)> = Vec::new();
            let mut hash = Fnv::new();
            hash.put(&(crate::format::cache_key(), self.out.plugins.engine.as_str()));
            parts.push(("engine".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            let mut settings = self.cfg.clone();
            settings.source = None;
            settings.cache =
                crate::config::Cache { enabled: true, dir: None, rebuild: false, index: Default::default() };
            // Only what changes the interpretation keys a cache: not how a
            // run reports, lints or traces, not its resource limits, and not
            // the engine as configured (the engine in effect is keyed above).
            let defaults = crate::config::Config::default();
            settings.verbose = 0;
            settings.timings = false;
            settings.gaps = defaults.gaps.clone();
            settings.lint_off = Vec::new();
            settings.lints = defaults.lints.clone();
            settings.latex_alternatives = Default::default();
            settings.profile = None;
            settings.profiles = defaults.profiles.clone();
            settings.depp = defaults.depp.clone();
            settings.lsp = defaults.lsp.clone();
            settings.engine = None;
            // Where the trees are is no part of what a package does: each
            // segment checks its lookups and file contents itself.
            let roots = self.cache_roots();
            settings.texlive.root = None;
            settings.texmf.bundle = None;
            settings.texmf.roots.clear();
            let local = |p: &PathBuf| PathBuf::from(roots.canonical(&p.display().to_string()));
            settings.search_paths = settings.search_paths.iter().map(local).collect();
            settings.preload = settings.preload.as_ref().map(local);
            (settings.trace, settings.trace_lines, settings.trace_col, settings.trace_file) = (false, None, 0, None);
            (settings.trace_end_col, settings.trace_steps) = (0, None);
            let l = &mut settings.limits;
            (l.seconds, l.memory, l.threads, l.cache_size) = (0, Default::default(), 0, Default::default());
            (l.degrade_at_percent, l.facts, l.vertices, l.trace, l.steps) = (0, 0, 0, 0, 0);
            hash.write(serde_json::to_string(&settings).unwrap_or_default().as_bytes());
            parts.push(("configuration".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            hash.put(&(self.prefixes.global, self.prefixes.long, self.prefixes.outer, self.prefixes.protected));
            hash.put(&self.after_assignment);
            hash.put(&(self.long_argument, self.runaway, self.out.document_depth, self.docstrip));
            parts.push(("prefixes and flags".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            hash.put(&self.section.as_deref());
            let mut streams: Vec<_> = self.streams.iter().collect();
            streams.sort();
            hash.put(&streams);
            let mut outputs: Vec<_> = self.outputs.iter().collect();
            outputs.sort();
            hash.put(&outputs);
            hash.put(&self.docstrip_dir.as_ref().map(|d| roots.canonical(d)));
            parts.push(("section and streams".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            let mut pinned: Vec<Sym> = self.pinned.iter().copied().collect();
            pinned.sort();
            hash.put(&pinned);
            parts.push(("pinned names".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            let open: Vec<(Sym, u16)> =
                self.expanding.iter().enumerate().filter(|(_, n)| **n > 0).map(|(i, n)| (Sym(i as u32), *n)).collect();
            hash.put(&open);
            parts.push(("open expansions".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            // Enclosing expansions the package's vertices hang from.
            let within: Vec<Sym> = self.within.iter().map(|(sym, _)| *sym).collect();
            hash.put(&within);
            parts.push(("enclosing expansions".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            let mut recent: Vec<_> = self
                .expansion_sites
                .iter()
                .filter(|(_, s)| s.progress == self.site_progress())
                .map(|((span, sym), s)| (span_out(*span, site), *sym, s.repeats))
                .collect();
            recent.sort_by_key(|(span, sym, _)| (span.file, span.line, span.col, sym.0));
            hash.put(&recent);
            parts.push(("recent call sites".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            // Decided conditionals the load stands in (latex.ltx reads from `\IfFileExists` arm).
            let conds: Vec<(u8, Option<i8>, Span)> =
                self.conds.iter().map(|c| (c.limit as u8, c.kind, span_out(c.at, site))).collect();
            hash.put(&conds);
            parts.push(("the conditionals open".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            // Counter namespace known: decides whether package's first counter probes.
            hash.put(&self.counter_namespace);
            parts.push(("the counter namespace".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            parts
        });
        digest
    }

    /// Tree selector: state every segment assumes (beyond read checks).
    fn root_state(&self, site: Span, path: &str) -> Vec<(String, u64)> {
        let mut parts = self.fixed_state(site);
        let mut hash = Fnv::new();
        hash.put(&(
            CatcodeTable::latex().diff(&self.catcodes),
            self.catcodes.diff(&CatcodeTable::latex()),
            self.end_line.0,
        ));
        parts.push(("category codes".into(), hash.0));
        let roots = self.cache_roots();
        let mut hash = Fnv::new();
        hash.put(&(roots.canonical(path), crate::format::content_hash(Path::new(path))));
        parts.push(("the package file".into(), hash.0));
        let mut hash = Fnv::new();
        let lua: Vec<String> = self.lua.files().iter().map(|f| roots.canonical(&f.display().to_string())).collect();
        hash.put(&lua);
        parts.push(("the Lua files read".into(), hash.0));
        parts
    }

    /// The state a load begins in, which its reads are compared with.
    fn before_state(&self) -> Before {
        let slots: Vec<Option<Binding>> =
            (0..self.out.interner.len() as u32).map(|i| self.env.get(Sym(i)).cloned()).collect();
        let mut def_owner = HashMap::default();
        for (i, slot) in slots.iter().enumerate() {
            for def in slot.iter().flat_map(|b| b.defs.iter()) {
                def_owner.entry(*def).or_insert(Sym(i as u32));
            }
        }
        let graph = &self.out.graph;
        let counters = ALLOCATION_COUNTERS
            .map(|n| {
                (n as u8, self.register_sym_peek(n).and_then(|sym| self.env.get(sym)).and_then(|b| b.value.as_int()))
            })
            .collect();
        Before {
            slots,
            aliases: self.env.aliases.clone(),
            def_owner,
            vertices: graph.len(),
            reader: graph.reader,
            loads: self.out.facts.loads.len(),
            counters,
            steps: self.out.steps,
            progress: self.source_progress,
            assignments: self.env.assignments,
            recent_file_spans: self.recent_file_spans.clone(),
            carried: self.carried.clone(),
        }
    }

    /// Interpreter state that must match for checkpoints. Every machine field is listed with its cache role.
    fn rest_state(&self, input: usize) -> Rest {
        let Machine {
            // Their own roles: [`Carried`] in the effect, [`Scratch`] forgotten at the end.
            carried: _,
            scratch: _,
            // Same throughout a load.
            cfg: _,
            checkpoints: _,
            limits: _,
            load_start: _,
            pressure: _,
            hyph_cache: _,
            kp: _,
            degraded_at: _,
            base: _,
            resolver: _,
            dont_expand: _,
            unknown: _,
            call_runs: _,
            call_recs: _,
            guard_seen: _,
            job_ended: _,
            checked_at: _,
            unknown_digits: _,
            unknown_more: _,
            typed_syms: _,
            clock: _,
            clock_read_at: _,
            clock_cached: _,
            stubs: _,
            stub_said: _,
            // Recorded (facts, graph, names, files, bindings).
            out: _,
            env,
            definition_sites: _,
            expansion_sites: _,
            shape_memo: _,
            cond_sites: _,
            // In segment's lists and scalars.
            catcodes: _,
            lua: _,
            end_line: _,
            after_assignment: _,
            verbatim_environments: _,
            wild: _,
            section: _,
            streams: _,
            outputs: _,
            loaded: _,
            class_options: _,
            identifying: _,
            last_named_cs: _,
            font_watch: _,
            source_progress: _,
            lines_read: _,
            call_shape: _,
            gaps: _,
            last_file: _,
            read: _,
            may_have_ended: _,
            recent_file_spans: _,
            file_call: _,
            project_site: _,
            built_name: _,
            file_calls: _,
            counter_namespace: _,
            name_token: _,
            own_files: _,
            gaps_raised: _,
            tex_errors: _,
            join_commands: _,
            join_hooks: _,
            at_join: _,
            fi_mark: _,
            dropping: _,
            dropping_possible: _,
            dropping_breaks: _,
            keep_errors: _,
            approximated: _,
            cut_taint: _,
            gap_depth: _,
            // Part of tree root.
            docstrip_dir: _,
            // Follows input stack (expansion counted while tokens on it).
            expanding: _,
            // Compared at each checkpoint.
            input: _,
            prefixes,
            long_argument,
            runaway,
            scan_abandoned: _,
            scan_fuzzy: _,
            delimiter_met: _,
            arm_assumed: _,
            unknown_chars: _,
            cds,
            conds,
            testing,
            testing_kind: _,
            mode: _,
            list: _,
            material: _,
            page_touched: _,
            page_grew: _,
            natural: _,
            main_tail: _,
            main_depth: _,
            docstrip,
            pinned,
            within,
            source: _,
            packages,
            file_depth,
            choices,
            scans,
            unit: _,
            lone: _,
            script: _,
            runs: _,
            replayable: _,
            infeasible: _,
            undo: _,
            waiting: _,
            parted: _,
            abs: _,
            abs_exact: _,
            abs_odd: _,
            graph_full_noted: _,
            output_active: _,
            abs_parts: _,
            abs_from: _,
            abs_whole: _,
            ranged: _,
            narrowing: _,
            halted,
            budget,
            format_progress: _,
            region: _,
            guard: _,
            scan_starts: _,
            scan_cut: _,
            cached_lines: _,
            // Only a cell compares them, and no package loaded in a group
            // (as a cell is) is cached.
            align_state: _,
            cell_base: _,
            held_tokens,
            noexpanded,
            def_body_depth,
            edef_depth,
            scanning,
            scan_undecided,
            pdf_string,
            written_lines,
            name_reads,
            line_sandbox: _,
            line_defined,
            stream_files,
            call_parts: _,
            body: _,
            key_names,
            held,
            command_level: _,
            // Cache and run bookkeeping.
            package_capture: _,
            // Only during `\edef` body scan.
            copy_target: _,
            copies: _,
            document_file_depth: _,
            popping_in_read: _,
            reading_command: _,
            trace_full: _,
            started: _,
            step_depth: _,
            stack_base: _,
            scanner: _,
            pseudo_file: _,
            probe: _,
            // Used at next step, never across load.
            cacheable_load: _,
            par_inserted: _,
            // Output bookkeeping, not runtime state.
            diag_seen: _,
            diag_floor: _,
            quantity: _,
            text_broken: _,
            box_chars: _,
            fetched: _,
            carried_chars: _,
            redefines_hint: _,
            text_apart: _,
            // Scratch of one widened conditional.
            skip_log: _,
        } = self;
        Rest {
            input,
            depth: env.depth(),
            conds: conds.len(),
            cds: cds.len(),
            testing: testing.len(),
            prefixes: (prefixes.global, prefixes.long, prefixes.outer, prefixes.protected),
            long_argument: *long_argument,
            runaway: *runaway,
            within: within.len(),
            held: held.len(),
            edef_depth: *edef_depth,
            scanning: *scanning,
            scan_undecided: *scan_undecided,
            pdf_string: *pdf_string,
            def_body_depth: *def_body_depth,
            noexpanded: noexpanded.is_some(),
            choices: choices.len(),
            scans: scans.len(),
            packages: packages.len(),
            file_depth: *file_depth,
            aux: (
                line_defined.is_some(),
                written_lines.len() + name_reads.len() + stream_files.len() + key_names.len(),
            ),
            pinned: pinned.len(),
            docstrip: *docstrip,
            budget: *budget,
            forking: env.forking(),
            halted: *halted,
            command_level: false,
            held_tokens: *held_tokens,
            reader: self.out.graph.reader.is_some(),
        }
    }

    /// Mouth for a package file: resuming where cache left off, after installing fitting segments.
    /// The package a cache is currently being recorded for, so the run
    /// loop can draw a progress spinner without seeing into [`Recorder`].
    /// The package being cached and the input frame of its file.
    pub(super) fn recording_package(&self) -> Option<(&str, usize)> {
        let rec = self.package_capture.as_deref()?;
        Some((&rec.label, rec.frame))
    }

    pub(super) fn open_package(&mut self, source: &str, id: FileId, name: &str, kind: LoadKind, span: Span) -> Mouth {
        if !self.cacheable_at_push(kind) {
            return Mouth::file(source, id);
        }
        self.cacheable_load = Some(id);
        self.guard.held = (std::mem::take(&mut self.guard.lines), std::mem::take(&mut self.guard.forks));
        let Some(dir) = self.cache_dir() else { return Mouth::file(source, id) };
        let path = self.out.files[id as usize].path.clone();
        if kind == LoadKind::Class {
            self.out.class_caches.insert(name.to_string());
        }
        crate::format::load_hashes(&dir);
        let roots = self.cache_roots();
        let mut key = Fnv::new();
        key.put(&(crate::format::cache_key(), name, kind, roots.canonical(&path)));
        // A segment that read the clock replays that run's clock: only
        // under the same `clock:` setting.
        key.put(&self.cfg.clock.to_string());
        let cache = cache_file(&dir, name, key.0);
        let started = std::time::Instant::now();
        let root = self.root_state(span, &path);
        let before = self.before_state();
        let rest = self.rest_state(self.input.len() + 1);
        let base = self.base.clone();
        let mut at: Option<u32> = None;
        let mut parent_id: Option<u64> = None;
        let mut end: Option<(LineMark, bool)> = None;
        let mut served = 0;
        let mut reason: Option<String> = None;
        let mut written: Vec<(Sym, u8)> = Vec::new();
        let mut origins: Vec<((RegKind, u16), u8)> = Vec::new();
        let mut carried: Vec<Sym> = Vec::new();
        let replaying = !self.cfg.cache.rebuild && !self.cfg.cache.index.refresh;
        let bytes = replaying.then(|| std::fs::read(&cache).ok()).flatten();
        match bytes.as_deref().map(|bytes| decode(&cache, bytes)) {
            None | Some(Err(None)) => {}
            Some(Err(Some(why))) => reason = Some(why),
            Some(Ok(file)) => match file.trees.iter().position(|t| t.root == root) {
                None if file.trees.is_empty() => {}
                None => {
                    let differs = file.trees[0]
                        .root
                        .iter()
                        .zip(&root)
                        .find(|(a, b)| a != b)
                        .map_or("its root", |(a, _)| a.0.as_str());
                    reason = Some(match differs {
                        "the package file" => format!("content changed: {path}"),
                        _ => format!("root differs: {differs}"),
                    });
                }
                Some(t) => {
                    let tree = &file.trees[t];
                    let mut children: HashMap<Option<u32>, Vec<u32>> = HashMap::default();
                    for (i, node) in tree.nodes.iter().enumerate() {
                        children.entry(node.parent).or_default().push(i as u32);
                    }
                    while let Some(kids) = children.get(&at) {
                        let mut chosen = None;
                        for &k in kids {
                            match self.segment_fits(&tree.nodes[k as usize], &base) {
                                Ok(clock) => {
                                    chosen = Some((k, clock));
                                    break;
                                }
                                Err(why) => {
                                    reason.get_or_insert(why);
                                }
                            }
                        }
                        let Some((k, clock)) = chosen else { break };
                        reason = None;
                        if clock {
                            self.clock_read(span);
                            self.clock_cached.get_or_insert_with(|| name.to_string());
                        }
                        let node = &tree.nodes[k as usize];
                        if self.out.steps + node.steps >= self.cfg.limits.steps {
                            reason = Some("the run could not afford it within its budgets".into());
                            break;
                        }
                        match self.apply_segment(node, span, &before, &mut origins, &mut carried) {
                            Some(marks) => written.extend(marks),
                            None => {
                                reason = Some("a segment cannot be decoded".into());
                                break;
                            }
                        }
                        served += node.steps;
                        at = Some(k);
                        end = Some((node.end, node.last));
                        if node.last {
                            break;
                        }
                    }
                    parent_id = at.and_then(|k| tree.nodes.get(k as usize)).map(node_id);
                }
            },
        }
        if served > 0 {
            self.out.timings.credit_tokens(id, served);
            self.out.cached_files.insert(id);
            // The lines the cache stands for count as read (progress).
            self.cached_lines += match end {
                Some((mark, false)) => u64::from(mark.line),
                _ => source.lines().count() as u64,
            };
        }
        let cache_text = cache.display().to_string();
        let mouth = match end {
            Some((mark, _)) => Mouth::resumed(source, id, mark),
            None => Mouth::file(source, id),
        };
        if end.is_some_and(|(_, last)| last) {
            if self.cfg.verbose >= 1 {
                crate::progress::note!("satex: {name} from its cache in {:?}", started.elapsed());
            }
            self.out.package_caches.push((name.to_string(), cache_text, "cached".into()));
            return mouth;
        }
        if let Some(why) = &reason {
            if self.cfg.verbose >= 1 {
                let line = end.map_or(1, |(mark, _)| mark.line);
                crate::progress::note!(
                    "satex: {name}: {served} tokens from its cache; {why}, so it is read on from line {line}"
                );
            }
            self.out.package_caches.push((name.to_string(), cache_text.clone(), format!("stale: {why}")));
        }
        if !self.cfg.cache.index.auto && !self.cfg.cache.index.building {
            return mouth;
        }
        let lookups = self.resolver.lookups().map(|(key, _)| key.clone()).collect();
        let marks = Marks { steps: self.out.steps, ..Marks::default() };
        let mut recorder = Recorder {
            file: id,
            expansions_start: self.out.facts.expansions.len(),
            log: Log::default(),
            frame: self.input.len(),
            name: name.to_string(),
            cache,
            site: span,
            root,
            parent: at,
            parent_id,
            before,
            rest,
            marks,
            last_line: end.map_or(0, |(mark, _)| mark.line),
            label: format!("{} {name}", if kind == LoadKind::Class { "class" } else { "package" }),
            started: std::time::Instant::now(),
            chain: Vec::new(),
            lists: None,
            lookups,
            seg_gaps: self.gaps.clone(),
            seg_vertices: 0,
            seg_call: None,
            seg_call_finished: false,
            seg_start: HashMap::default(),
            seg_prior: HashMap::default(),
            seg_assigned: Vec::new(),
            seg_copies: HashMap::default(),
            served,
        };
        recorder.log.origins = origins;
        self.package_capture = Some(Box::new(recorder));
        self.env.track_reads(true);
        for (sym, bits) in written {
            self.env.mark_written(sym, bits);
        }
        for sym in carried {
            self.env.note_carrying(sym);
        }
        self.out.interner.record_touches(true);
        self.out.graph.record_touches(true);
        self.out.graph.record_log(true);
        self.out.calls.record(true);
        self.out.observed.record(true);
        self.start_segment();
        mouth
    }

    /// Whether segment's files are unchanged and this run agrees with what it read.
    /// Whether the segment can be replayed here, and if so whether it read
    /// the clock ([`is_clock`]); else why not.
    fn segment_fits(&self, node: &Node, base: &Path) -> Result<bool, String> {
        let roots = self.cache_roots();
        for (path, hash) in &node.stamps {
            let path = roots.decode(path);
            if crate::format::content_hash(Path::new(&path)) != Some(*hash) {
                return Err(format!("content changed: {path}"));
            }
        }
        for (looked, kind, found) in &node.lookups {
            let found = found.as_ref().map(|f| PathBuf::from(roots.decode(&f.display().to_string())));
            if self.resolver.search_uncached(looked, *kind, base) != found {
                return Err(format!("lookup changed: {looked}"));
            }
        }
        // Decoded with segment's name numbers (may not exist here; looked up, not interned).
        let local: Vec<Sym> = (0..node.names.len() as u32).map(Sym).collect();
        let files = self.lookup_files(&node.paths);
        let (decoded, _) =
            codec::with(Mode::Decode { syms: local, files }, || postcard::from_bytes::<Reads>(&node.reads));
        let reads = decoded.map_err(|e| format!("cannot decode it: {e}"))?;
        let here = |sym: Sym| node.names.get(sym.0 as usize).and_then(|n| self.out.interner.lookup(n));
        for (code, sym) in &reads.gaps {
            let sym = here(*sym);
            if !self.gaps.iter().any(|(c, s)| c == code && Some(*s) == sym) {
                return Err("a gap it relied on being reported is not".into());
            }
        }
        let name_of = |sym: Sym| node.names.get(sym.0 as usize).map_or("", String::as_str);
        let clock = reads.bindings.iter().any(|(sym, kinds, _)| kinds & VALUE != 0 && is_clock(name_of(*sym)));
        let (mismatch, _) = codec::with(hash_mode(self), || {
            for (sym, kinds, digest) in &reads.bindings {
                let digested = digested_kinds(name_of(*sym), *kinds);
                let sym = here(*sym);
                let binding = sym.and_then(|sym| self.env.get(sym));
                let alias =
                    sym.and_then(|sym| self.env.aliases.get(&sym)).map(|a| self.out.interner.name(*a).to_string());
                if binding_digest(binding, alias.as_deref(), digested) != *digest {
                    let name = sym.map_or_else(|| "a name".to_string(), |sym| self.out.interner.cs(sym));
                    let read: Vec<&str> =
                        KINDS.iter().enumerate().filter(|(bit, _)| kinds >> bit & 1 != 0).map(|(_, k)| *k).collect();
                    return Some(format!("digest changed: {name} (its {} was read)", read.join(", ")));
                }
            }
            None
        });
        if let Some(reason) = mismatch {
            return Err(reason);
        }
        // Copied text may grow up to the limits its copies allow.
        for (sym, len, room) in &reads.copies {
            let Some(sym) = here(*sym) else { return Err("a text it copied is gone".into()) };
            let Some(text) = self.env.slot(sym).and_then(|b| crate::env::copied_text(&b.meaning)) else {
                return Err("a text it copied is gone".into());
            };
            if text.len() as i64 - i64::from(*len) > *room {
                return Err(format!("{} is too long to copy", self.out.interner.cs(sym)));
            }
        }
        Ok(clock)
    }

    /// This run's file numbers for a segment's paths (unregistered).
    fn lookup_files(&self, paths: &[String]) -> Vec<FileId> {
        let roots = self.cache_roots();
        paths
            .iter()
            .map(|p| match p.as_str() {
                SITE => LOAD_SITE,
                MAIN => self.out.main_file,
                path => {
                    let path = roots.decode(path);
                    self.out.files.iter().position(|f| f.path == path).map_or(UNREAD, |i| i as FileId)
                }
            })
            .collect()
    }

    /// The storage symbol of `\\count⟨n⟩`, without interning it.
    fn register_sym_peek(&self, n: i64) -> Option<Sym> {
        self.out.interner.lookup(&format!("{}{}{n}", super::REGISTER, RegKind::Count.as_str()))
    }
}

impl Machine<'_> {
    /// Install one segment: names, files, vertices, edges, bindings, facts, state.
    /// Returns names written and their parts, or `None` if uncodeably invalid.
    fn apply_segment(
        &mut self,
        node: &Node,
        site: Span,
        before: &Before,
        origins: &mut Vec<((RegKind, u16), u8)>,
        carried: &mut Vec<Sym>,
    ) -> Option<Vec<(Sym, u8)>> {
        // Read with segment's numbers (for allocations and name order).
        let local: Vec<Sym> = (0..node.names.len() as u32).map(Sym).collect();
        let (provisional, _) = codec::with(Mode::Decode { syms: local, files: self.lookup_files(&node.paths) }, || {
            postcard::from_bytes::<Sections>(&node.effect)
                .and_then(|s| postcard::take_from_bytes::<EffectHead>(&s.rest).map(|(head, _)| head))
        });
        let provisional = provisional.ok()?;
        let mut shift: HashMap<u8, i64> = HashMap::default();
        for (counter, from, ..) in &provisional.moved {
            let now = before.counters.iter().find(|(c, _)| c == counter).and_then(|(_, v)| *v)?;
            shift.insert(*counter, now - from);
        }
        let mut known = origins.clone();
        known.extend(provisional.origins.iter().copied());
        let relocate = |kind: RegKind, index: i64| -> i64 {
            match origin_of(&known, kind, index) {
                Some(counter) => index + shift.get(&counter).copied().unwrap_or(0),
                None => index,
            }
        };
        let names: Vec<String> = node
            .names
            .iter()
            .map(|n| match storage_register(n) {
                Some((kind, index)) => format!("{}{}{}", super::REGISTER, kind.as_str(), relocate(kind, index)),
                None => relocate_text(n, &relocate),
            })
            .collect();
        let mut syms: Vec<Option<Sym>> = vec![None; names.len()];
        for local in &provisional.names {
            let i = local.0 as usize;
            if let Some(name) = names.get(i) {
                syms[i] = Some(self.out.interner.intern(name));
            }
        }
        let syms: Vec<Sym> = syms
            .into_iter()
            .enumerate()
            .map(|(i, s)| s.unwrap_or_else(|| self.out.interner.intern(&names[i])))
            .collect();
        let roots = self.cache_roots();
        for (path, kind, _) in &provisional.files {
            self.register_file(roots.decode(path), *kind);
        }
        // Segment's files registered above (in order read); unread files stay unread.
        let files = self.lookup_files(&node.paths);
        let st = decode_effect(&node.effect, &syms, &files)?;
        // Texts the segment copied, as this run has them at segment start.
        let mut origs: HashMap<Sym, Rc<[Token]>> = HashMap::default();
        let spliced =
            st.bindings.iter().filter(|(_, b, _)| b.as_ref().is_some_and(|b| b.splice.is_some())).map(|(sym, ..)| *sym);
        for sym in spliced.chain(st.definitions.iter().filter(|d| d.splice.is_some()).map(|d| d.name)) {
            let text = self.env.slot(sym).and_then(|b| crate::env::copied_text(&b.meaning)).cloned()?;
            origs.insert(sym, text);
        }
        origins.extend(st.origins.iter().copied());
        for (path, _, entry) in &st.files {
            let path = roots.decode(path);
            if let Some(id) = self.out.files.iter().position(|f| f.path == path)
                && let Some(slot) = self.out.entry.get_mut(id)
            {
                *slot = entry.map(|s| span_in(s, site));
            }
        }

        // Call the segment completed (as read).
        let pending = self.call_shape.as_deref().map(|(sym, span, ..)| (*sym, *span));
        if st.scalars.finished_call {
            self.finish_shape();
        }
        // Vertices, in the order the segment first reached them.
        let mut ids: Vec<Option<NodeId>> = vec![None; st.vertices.len()];
        let reached: HashSet<u32> = st.reached.iter().map(|(i, _)| *i).collect();
        for (i, reader) in &st.reached {
            let reader =
                reader.and_then(|r| self.resolve_vertex(&st, &mut ids, &reached, before, site, r, 0).first().copied());
            let charged = std::mem::replace(&mut self.out.graph.reader, reader);
            self.resolve_vertex(&st, &mut ids, &reached, before, site, VRef::Table(*i), 0);
            self.out.graph.reader = charged;
        }
        // Table entries already made resolve without a lookup.
        fn made(ids: &[Option<NodeId>], r: &VRef) -> Option<NodeId> {
            match r {
                VRef::Table(i) => ids.get(*i as usize).copied().flatten(),
                VRef::Before(_) => None,
            }
        }
        for (from, to, kind, reader) in &st.edges {
            let (many_from, many_to);
            let one_from;
            let froms: &[NodeId] = match made(&ids, from) {
                Some(id) => {
                    one_from = [id];
                    &one_from
                }
                None => {
                    many_from = self.resolve_vertex(&st, &mut ids, &reached, before, site, *from, 0);
                    &many_from
                }
            };
            let one_to;
            let tos: &[NodeId] = match made(&ids, to) {
                Some(id) => {
                    one_to = [id];
                    &one_to
                }
                None => {
                    many_to = self.resolve_vertex(&st, &mut ids, &reached, before, site, *to, 0);
                    &many_to
                }
            };
            let kind = EdgeKind(*kind);
            // Definition maker (in edge order).
            if kind == EdgeKind::default() {
                for d in froms {
                    if let Some(c) = tos.first() {
                        self.out.graph.made_by(*d, *c);
                    }
                }
                continue;
            }
            if kind.intersects(EdgeKind::READS) {
                let reader = reader.and_then(|r| {
                    made(&ids, &r)
                        .or_else(|| self.resolve_vertex(&st, &mut ids, &reached, before, site, r, 0).first().copied())
                });
                let charged = std::mem::replace(&mut self.out.graph.reader, reader);
                for f in froms {
                    for t in tos {
                        self.out.graph.edge(*f, *t, kind);
                    }
                }
                self.out.graph.reader = charged;
            } else {
                for f in froms {
                    for t in tos {
                        self.out.graph.add_raw_edge(*f, *t, kind);
                    }
                }
            }
        }
        for (span, line) in &st.extents {
            self.out.graph.note_extent(span_in(*span, site), *line);
        }
        for event in &st.observed {
            self.out.observed.replay(*event);
        }
        for event in &st.calls {
            self.out.calls.replay(*event);
        }

        // Bindings with renumbered registers.
        let lasts: Vec<(i64, i64)> =
            st.moved.iter().map(|(c, _, after, _)| (*after, shift.get(c).copied().unwrap_or(0))).collect();
        let mut written = Vec::with_capacity(st.bindings.len());
        for (sym, binding, bits) in &st.bindings {
            written.push((*sym, *bits));
            let Some(b) = binding else { continue };
            let mut meaning = match (b.splice, origs.get(sym)) {
                (Some(at), Some(orig)) => with_text(&b.meaning, |text| splice_in(text, at as usize, orig))?,
                _ => b.meaning.clone(),
            };
            if b.splice.is_some() {
                carried.push(*sym);
            }
            let mut value = b.value.clone();
            if let Meaning::Register(kind, index) = meaning {
                let new = relocate(kind, i64::from(index));
                if let Value::Int(v) = value
                    && v == i64::from(index)
                {
                    value = Value::Int(new);
                }
                meaning = Meaning::Register(kind, u16::try_from(new).unwrap_or(index));
            } else if let (Some((RegKind::Count, _)), Value::Int(v)) =
                (storage_register(self.out.interner.name(*sym)), &value)
                && let [(after, delta)] = lasts.iter().filter(|(after, _)| after == v).copied().collect::<Vec<_>>()[..]
            {
                value = Value::Int(after + delta);
            }
            let mut defs = Vec::new();
            for d in &b.defs {
                defs.extend(self.resolve_vertex(&st, &mut ids, &reached, before, site, *d, 0));
            }
            let binding = Binding { meaning, value, defs, certain: b.certain, global: b.global, may: None };
            self.env.set(*sym, binding, b.global);
        }
        for (counter, _, after, defs) in &st.moved {
            let sym = self.register_sym(RegKind::Count, i64::from(*counter));
            let mut binding = self.env.get(sym).cloned().unwrap_or_else(|| Binding::builtin(Meaning::Unknown));
            binding.value = Value::Int(after + shift.get(counter).copied().unwrap_or(0));
            let mut resolved = Vec::new();
            for d in defs {
                resolved.extend(self.resolve_vertex(&st, &mut ids, &reached, before, site, *d, 0));
            }
            binding.defs = resolved;
            self.env.set(sym, binding, true);
        }

        // Facts in order made.
        for d in &st.definitions {
            let mac = match (d.splice, &d.mac, origs.get(&d.name)) {
                (Some(at), Some(mac), Some(orig)) => {
                    let mut mac = (**mac).clone();
                    mac.replacement_text = splice_in(&mac.replacement_text, at as usize, orig)?;
                    Some(Rc::new(mac))
                }
                _ => d.mac.clone(),
            };
            let Some(node_id) = self.resolve_vertex(&st, &mut ids, &reached, before, site, d.node, 0).first().copied()
            else {
                continue;
            };
            let span = span_in(d.span, site);
            if let Some(&i) = self.definition_sites.get(&(span, d.name)) {
                let previous = &mut self.out.facts.defs[i];
                previous.certain &= d.certain;
                previous.mac = mac;
                continue;
            }
            let cds = self.resolve_cds(&st, &mut ids, &reached, before, site, &d.cds);
            self.definition_sites.insert((span, d.name), self.out.facts.defs.len());
            self.out.facts.defs.push(Definition {
                name: d.name,
                by: d.by,
                tag: leak(&d.tag),
                subject: d.subject,
                mode: d.mode,
                span,
                package: d.package,
                node: node_id,
                mac,
                depth: d.depth,
                global: d.global,
                redefines: if d.relative {
                    before
                        .slots
                        .get(d.name.0 as usize)
                        .and_then(|b| b.as_ref())
                        .is_some_and(|b| b.meaning != Meaning::Undefined)
                } else {
                    d.redefines
                },
                certain: d.certain,
                spelled: false,
                cds,
                // Outside environment (not persisted; replay matches).
                context: Rc::from(Vec::new()),
                via: None,
            });
        }
        let mut sites = Vec::with_capacity(st.expansions.len());
        for e in &st.expansions {
            let key = (span_in(e.span, site), e.name);
            if let Some(existing) = self.expansion_sites.get(&key) {
                let fact = existing.fact;
                let previous = &mut self.out.facts.expansions[fact];
                previous.count = previous.count.saturating_add(e.count);
                previous.mode = previous.mode.union(e.mode);
                sites.push((key, fact, e.repeats, e.current));
                continue;
            }
            let Some(f) = &e.fact else { continue };
            let meaning = match f.meaning {
                (0, _) => MeaningKind::Primitive,
                (1, Some(n)) => self
                    .resolve_vertex(&st, &mut ids, &reached, before, site, n, 0)
                    .first()
                    .copied()
                    .map_or(MeaningKind::Unknown, MeaningKind::Macro),
                (2, _) => MeaningKind::Register,
                (3, _) => MeaningKind::Char,
                (4, _) => MeaningKind::Undefined,
                (6, _) => MeaningKind::UndefinedApproximated,
                _ => MeaningKind::Unknown,
            };
            let node_id =
                f.node.and_then(|n| self.resolve_vertex(&st, &mut ids, &reached, before, site, n, 0).first().copied());
            let cds = self.resolve_cds(&st, &mut ids, &reached, before, site, &f.cds);
            let fact = self.out.facts.expansions.len();
            self.out.facts.expansions.push(Expansion {
                name: e.name,
                span: key.0,
                package: f.package,
                node: node_id,
                meaning,
                within: f.within,
                arguments: f.arguments.clone(),
                cds,
                count: e.count,
                mode: e.mode,
            });
            self.expansion_sites.insert(
                key,
                Site {
                    fact,
                    repeats: e.repeats,
                    progress: 0,
                    size: usize::MAX,
                    source: 0,
                    unread: 1,
                    ..Site::fresh(fact)
                },
            );
            sites.push((key, fact, e.repeats, e.current));
        }
        let base_loads = before.loads as i64;
        for l in &st.loads {
            let file = l
                .path
                .as_ref()
                .map(|p| roots.decode(p))
                .and_then(|p| self.out.files.iter().position(|f| f.path == p))
                .map(|i| i as FileId);
            self.out.facts.loads.push(Load {
                name: l.name.clone(),
                kind: l.kind,
                options: l.options.clone(),
                span: span_in(l.span, site),
                by: l.by,
                file,
                path: l.path.as_ref().map(|p| roots.decode(p)),
                status: {
                    let mut s = STATUSES[l.status as usize];
                    if let (LoadStatus::CaseMismatch { on_disk: od }, Some(actual_on_disk)) =
                        (&mut s, l.case_mismatch_on_disk)
                    {
                        *od = actual_on_disk;
                    }
                    s
                },
                required: l.required.clone(),
                provided: l.provided.clone(),
                depth: l.depth,
            });
        }
        for (load, provided) in &st.identifications {
            if let Some(entry) = usize::try_from(base_loads + load).ok().and_then(|i| self.out.facts.loads.get_mut(i)) {
                entry.provided = provided.clone();
            }
        }
        // The cache keeps no order between occurrences and diagnostics:
        // a replayed diagnostic counts as raised before the load's occurrences.
        let replayed_from = self.out.facts.occurrences.len();
        for o in &st.occurrences {
            let Some(node_id) = self.resolve_vertex(&st, &mut ids, &reached, before, site, o.node, 0).first().copied()
            else {
                continue;
            };
            self.out.facts.occurrences.push(Occurrence {
                kind: o.kind,
                key: relocate_text(&o.key, &relocate),
                detail: o.detail.as_deref().map(|d| relocate_text(d, &relocate)),
                span: span_in(o.span, site),
                expanded: None,
                call: None,
                package: o.package,
                node: node_id,
                section: o.section.as_deref().map(Rc::from),
                certain: o.certain && self.exact(),
            });
        }
        for c in &st.colors {
            self.out.facts.colors.push(ColorDef {
                name: c.name.clone(),
                value: c.value.clone(),
                declared: c.declared.clone(),
                spec_span: c.spec_span.map(|s| span_in(s, site)),
                span: span_in(c.span, site),
                via: c.via,
                package: c.package,
                certain: c.certain && self.exact(),
            });
        }
        for u in &st.color_uses {
            let (kind, global, expand) = u.role;
            self.out.facts.color_uses.push(ColorUse {
                name: u.name.clone(),
                role: role_in(kind, global, expand),
                defined: u.defined,
                span: span_in(u.span, site),
                via: u.via,
                package: u.package,
                certain: u.certain && self.exact(),
            });
        }
        for (stack, op, span, certain) in &st.color_stack {
            self.out.facts.color_stack.push(ColorStack {
                stack: *stack,
                op: STACK_OPS[usize::from(*op).min(STACK_OPS.len() - 1)],
                span: span_in(*span, site),
                certain: *certain && self.exact(),
            });
        }
        for (severity, code, message, span, gap, count) in &st.diagnostics {
            // A gap is reported once per run: not again if this run already
            // has.
            if let Some((gap_code, sym)) = gap
                && self.gaps.iter().any(|(c, s)| c == gap_code && s == sym)
            {
                continue;
            }
            let span = span_in(*span, site);
            // Loading stops the origin search: only an own file is one.
            let origin = self.own_file(span.file).then_some(span);
            self.out.facts.diagnostics.push(Diagnostic {
                severity: SEVERITIES[*severity as usize],
                code: leak(code),
                message: relocate_text(message, &relocate),
                span,
                count: *count,
                origin,
                after: replayed_from,
            });
        }
        self.diag_floor = self.out.facts.diagnostics.len();
        for (code, sym) in &st.gaps {
            self.gaps.insert((leak(code), *sym));
        }
        for (field, text, source) in &st.metadata {
            self.out.metadata.push(Metadata { field: leak(field), text: text.clone(), source: source.clone() });
        }
        for (span, kind, global, expand) in &st.csnames {
            self.out.facts.csnames.entry(span_in(*span, site)).or_insert(role_in(*kind, *global, *expand));
        }
        for (sym, shape) in &st.shapes {
            let slot = self.out.facts.shapes.entry(*sym).or_default();
            if shape.len() > slot.len() {
                *slot = shape.clone();
            }
        }

        // Restore checkpoint state.
        if let Some(lists) = &st.lists {
            let mut catcodes = CatcodeTable::latex();
            catcodes.apply(&lists.catcodes);
            self.catcodes = catcodes;
            self.end_line = EndLineChar(lists.end_line);
            self.loaded = Rc::new(lists.loaded.clone());
            self.class_options = lists.class_options.clone();
            self.wild = lists.wild.clone();
            self.verbatim_environments = lists.verbatim.iter().cloned().collect();
            self.section = lists.section.as_deref().map(Rc::from);
            self.lua
                .set_files(lists.lua.iter().map(|f| PathBuf::from(roots.decode(&f.display().to_string()))).collect());
            self.counter_namespace = lists.counter_namespace.clone();
            self.streams =
                Rc::new(lists.streams.iter().map(|(n, lines, at)| (*n, (Rc::new(lines.clone()), *at))).collect());
            self.outputs = Rc::new(lists.outputs.iter().cloned().collect());
            self.env.aliases = lists.aliases.iter().copied().collect();
        }
        // Changes count from the load's start, like the scalars.
        self.carried = before.carried.clone();
        self.carried.apply(&st.carried);
        let s = &st.scalars;
        self.after_assignment = s.after_assignment;
        self.last_file = s.last_file.map(|span| span.file);
        self.file_call = s.file_call.map(|(sym, span)| (sym, span_in(span, site)));
        self.identifying = s.identifying.and_then(|i| usize::try_from(base_loads + i).ok());
        // Unfinished pending call remains this run's own.
        if s.call_shape.is_some() || st.scalars.finished_call || pending.is_none() {
            self.call_shape =
                s.call_shape.clone().map(|(sym, span, shape, depth)| Rc::new((sym, span_in(span, site), shape, depth)));
        }
        self.last_named_cs = s.last_named_cs;
        self.page_touched |= s.page_touched;
        // What the replayed code put on the main vertical list is not kept.
        self.main_tail = crate::mode::Last::UNKNOWN;
        self.main_depth = crate::mode::Dim::ANY;
        let mut recent = before.recent_file_spans.clone();
        recent.extend(s.recent_file_spans.iter().map(|span| span_in(*span, site)));
        let excess = recent.len().saturating_sub(RECENT_FILE_SPANS);
        recent.drain(..excess);
        self.recent_file_spans = recent;
        self.out.steps = before.steps + s.steps;
        self.skip_replayed_steps(s.steps);
        self.source_progress = before.progress + s.progress;
        self.env.assignments = before.assignments + s.assignments;
        self.env.set_recent(s.recent.clone());
        let now = self.site_progress();
        for (key, fact, repeats, current) in sites {
            let progress = if current { now } else { !now };
            self.expansion_sites.insert(
                key,
                Site { fact, repeats, progress, size: usize::MAX, source: 0, unread: 1, ..Site::fresh(fact) },
            );
        }
        Some(written)
    }

    /// Vertices a reference stands for: table entry (found/made) or pre-load definitions.
    #[allow(clippy::too_many_arguments)]
    fn resolve_vertex(
        &mut self,
        st: &Effect,
        ids: &mut [Option<NodeId>],
        reached: &HashSet<u32>,
        before: &Before,
        site: Span,
        r: VRef,
        depth: usize,
    ) -> Vec<NodeId> {
        let i = match r {
            VRef::Before(sym) => {
                return before
                    .slots
                    .get(sym.0 as usize)
                    .and_then(|b| b.as_ref())
                    .map(|b| b.defs.clone())
                    .unwrap_or_default();
            }
            VRef::Table(i) => i as usize,
        };
        if let Some(id) = ids.get(i).copied().flatten() {
            return vec![id];
        }
        let Some(v) = st.vertices.get(i) else { return Vec::new() };
        if v.reader {
            ids[i] = before.reader;
            return before.reader.into_iter().collect();
        }
        let span = span_in(v.span, site);
        let tag = TAGS[v.tag as usize];
        if let Some(id) = self.out.graph.find(span, v.name, tag) {
            ids[i] = Some(id);
            return vec![id];
        }
        // Unreached or too-deep vertices won't be made by reading the package.
        if !reached.contains(&(i as u32)) || depth > st.vertices.len() {
            return Vec::new();
        }
        let within =
            v.within.and_then(|w| self.resolve_vertex(st, ids, reached, before, site, w, depth + 1).first().copied());
        let mut cds = Vec::new();
        for (on, taken) in &v.cds {
            for on in self.resolve_vertex(st, ids, reached, before, site, *on, depth + 1) {
                cds.push(ControlDep { on, taken: *taken });
            }
        }
        let fresh = self.out.graph.len() as NodeId;
        let id = self.out.graph.push_vertex(Vertex { tag, name: v.name, key: v.key, span, cds: cds.clone(), within });
        // Pre-existing vertices lack edges; new ones get them as if read.
        if id == fresh && !v.new {
            for cd in &cds {
                self.out.graph.add_raw_edge(id, cd.on, EdgeKind::CONTROL);
            }
            if let Some(parent) = within {
                let from = self.out.graph.reader.unwrap_or(parent);
                self.out.graph.add_raw_edge(from, id, EdgeKind::EXPANDS);
            }
        }
        ids[i] = Some(id);
        vec![id]
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_cds(
        &mut self,
        st: &Effect,
        ids: &mut [Option<NodeId>],
        reached: &HashSet<u32>,
        before: &Before,
        site: Span,
        refs: &[(VRef, bool)],
    ) -> Vec<ControlDep> {
        let mut out = Vec::new();
        for (on, taken) in refs {
            for on in self.resolve_vertex(st, ids, reached, before, site, *on, 0) {
                out.push(ControlDep { on, taken: *taken });
            }
        }
        out
    }
}

/// Minimum tokens per segment (unless file ends).
/// Bounds divergence costs; fewer checkpoints keep cache small.
/// Measured on TikZ (8·10⁶ tokens) and amsmath (8·10⁴).
const SEGMENT_TOKENS: u64 = 20_000;

impl Machine<'_> {
    /// Begin a segment: log counts from here, reads compare with this state.
    fn start_segment(&mut self) {
        let Some(rec) = self.package_capture.as_ref() else { return };
        let log = self.out.graph.log();
        let marks = Marks {
            names: self.out.interner.touches().len(),
            reached: self.out.graph.touches().len(),
            edges: log.map_or(0, |l| l.edges.len()),
            links: log.map_or(0, |l| l.links.len()),
            extents: log.map_or(0, |l| l.extents.len()),
            calls: self.out.calls.recorded().len(),
            observed: self.out.observed.recorded().len(),
            definitions: rec.log.definitions.len(),
            shapes: rec.log.shapes.len(),
            gaps_seen: rec.log.gaps_seen.len(),
            gaps_new: rec.log.gaps_new.len(),
            csnames: rec.log.csnames.len(),
            origins: rec.log.origins.len(),
            identifications: rec.log.identifications.len(),
            opened: rec.log.opened.len(),
            loads: self.out.facts.loads.len(),
            occurrences: self.out.facts.occurrences.len(),
            colors: self.out.facts.colors.len(),
            color_uses: self.out.facts.color_uses.len(),
            color_stack: self.out.facts.color_stack.len(),
            diagnostics: {
                self.diag_floor = self.out.facts.diagnostics.len();
                self.diag_floor
            },
            metadata: self.out.metadata.len(),
            files: self.out.files.len(),
            steps: self.out.steps,
        };
        let seg_gaps = self.gaps.clone();
        let seg_vertices = self.out.graph.len() as NodeId;
        let seg_call = self.call_shape.as_deref().map(|(sym, span, ..)| (*sym, *span));
        let _ = self.env.take_read_marks();
        let dirty = self.env.take_dirty();
        self.env.begin_segment();
        let seg_start = self.env.carrying().into_iter().map(|sym| (sym, self.env.slot(sym).cloned())).collect();
        let Some(rec) = self.package_capture.as_mut() else { return };
        let assigned: Vec<Sym> =
            std::mem::take(&mut rec.seg_assigned).into_iter().chain(dirty.iter().map(|d| d.0)).collect();
        for sym in assigned {
            rec.seg_prior.insert(sym, self.env.slot(sym).cloned());
        }
        rec.seg_start = seg_start;
        rec.seg_copies.clear();
        rec.marks = marks;
        rec.seg_gaps = seg_gaps;
        rec.seg_vertices = seg_vertices;
        rec.seg_call = seg_call;
        rec.seg_call_finished = false;
        rec.log.sites.clear();
        rec.log.sites_seen.clear();
        rec.log.site_modes.clear();
    }

    /// Checkpoint when the package's file is at line start and interpreter matches initial state.
    pub(super) fn maybe_checkpoint(&mut self) {
        let Some(rec) = self.package_capture.as_ref() else { return };
        if rec.frame + 1 != self.input.len() || rec.log.spoiled.is_some() {
            return;
        }
        let Some(Frame::File { mouth, .. }) = self.input.last() else { return };
        if mouth.file != rec.file {
            return;
        }
        let Some(mark) = mouth.line_mark() else { return };
        let long_enough = self.out.steps - rec.marks.steps >= SEGMENT_TOKENS;
        if mark.line <= rec.last_line || !(long_enough || mark.ended) {
            return;
        }
        if self.rest_state(rec.frame + 1) != rec.rest {
            return;
        }
        self.checkpoint(mark);
    }

    /// Whether defining `name` redefines it, decided by initial state (no read; replay re-decides).
    pub(crate) fn redefines(&self, name: Sym) -> (bool, bool) {
        if self.package_capture.is_some() && !self.env.meaning_written(name) {
            let defined = self.env.slot(name).is_some_and(|b| b.meaning != Meaning::Undefined);
            return (defined, true);
        }
        (self.env.is_defined(name), false)
    }

    /// Texts copied into `meaning`, where old text goes (so replay can re-make the definition).
    pub(crate) fn resolve_copies(
        &mut self,
        name: Sym,
        meaning: &Meaning,
        copies: &[crate::machine::CopyEvent],
        global: bool,
    ) -> Option<(usize, usize)> {
        let [copy] = copies else {
            for copy in copies {
                self.env.note_read(copy.sym, ReadKind::Meaning);
            }
            return None;
        };
        let text = meaning.as_macro().map(|m| m.replacement_text.clone());
        let old = self.env.slot(name).and_then(|b| crate::env::copied_text(&b.meaning)).cloned();
        let kept = match (&text, &old) {
            (Some(text), Some(old)) => {
                copy.len == old.len()
                    && text.get(copy.at..copy.at + copy.len).is_some_and(|slice| same_tokens(slice, old))
            }
            _ => false,
        };
        let splice = (kept && global && copy.fresh && !self.on_paths() && self.package_capture.is_some())
            .then(|| self.env.splice_of(name, copy.len))
            .flatten();
        let (Some((start, len)), Some(text)) = (splice, text) else {
            self.env.note_read(name, ReadKind::Meaning);
            return None;
        };
        self.env.note_carrying(name);
        let room = copy.room.min(self.cfg.limits.expansion_tokens as i64 - 1 - text.len() as i64);
        let rec = self.package_capture.as_mut()?;
        let entry = rec.seg_copies.entry(name).or_insert((len, room));
        entry.1 = entry.1.min(room);
        Some((copy.at + start, len))
    }

    /// Definition used text from start of segment at offset `at`, length `len`.
    pub(crate) fn note_copied(&mut self, name: Sym, at: usize, len: usize) {
        self.env.set_splice(name, at, len);
        let Some(rec) = self.package_capture.as_mut() else { return };
        let last = rec.log.definitions.len().wrapping_sub(1);
        if rec.log.definitions.last().is_some_and(|(d, _)| d.name == name) {
            rec.log.def_splices.insert(last, (at, len));
        }
    }

    /// Call shape completed; if pending at segment start, replay completes it too.
    pub(crate) fn note_call_finished(&mut self, sym: Sym, span: Span) {
        if let Some(rec) = self.package_capture.as_mut()
            && rec.seg_call == Some((sym, span))
        {
            rec.seg_call_finished = true;
        }
    }

    /// `from` reads what defines `name`: recorded as read if pre-load definitions unchanged; replay links.
    pub(crate) fn link_reads(&mut self, from: NodeId, name: Sym, defs: &[NodeId]) {
        self.link_edges(from, name, defs, EdgeKind::READS);
    }

    /// Macro body `from` mentions `name`: depends on its definition; no call charged.
    pub(crate) fn link_mentions(&mut self, from: NodeId, name: Sym, defs: &[NodeId]) {
        self.link_edges(from, name, defs, EdgeKind(EdgeKind::READS.0 | EdgeKind::MENTION.0));
    }

    fn link_edges(&mut self, from: NodeId, name: Sym, defs: &[NodeId], kind: EdgeKind) {
        let linked = self.package_capture.is_some() && !self.env.meaning_written(name);
        if linked {
            self.out.graph.begin_link(from, name, kind);
        }
        for def in defs {
            self.out.graph.edge(from, *def, kind);
        }
        if linked {
            self.out.graph.end_link();
        }
    }

    /// Last checkpoint at package file end when interpreter is at rest.
    /// The read already counted the token a replay will count again.
    pub(super) fn checkpoint_at_end(&mut self) {
        let Some(rec) = self.package_capture.as_ref() else { return };
        if rec.frame + 1 != self.input.len() || rec.log.spoiled.is_some() {
            return;
        }
        let Some(Frame::File { mouth, .. }) = self.input.last() else { return };
        if mouth.file != rec.file {
            return;
        }
        let Some(mark) = mouth.line_mark().filter(|m| m.ended) else { return };
        if self.rest_state(rec.frame + 1) != rec.rest {
            return;
        }
        self.out.steps -= 1;
        self.checkpoint(mark);
        self.out.steps += 1;
    }

    /// Call site about to expand: note it and call count on first segment reach.
    pub(super) fn note_site(&mut self, span: Span, name: Sym) {
        let mode = self.mode;
        let Some(rec) = self.package_capture.as_mut() else { return };
        let modes = rec.log.site_modes.entry((span, name)).or_default();
        *modes = modes.union(mode);
        if !rec.log.sites_seen.insert((span, name)) {
            return;
        }
        let count =
            self.expansion_sites.get(&(span, name)).map_or(0, |site| self.out.facts.expansions[site.fact].count);
        rec.log.sites.push(((span, name), count));
    }

    /// File opened during recording: part of segment dependency.
    pub(crate) fn note_opened(&mut self, path: &str) {
        if let Some(rec) = self.package_capture.as_mut() {
            rec.log.opened.push(path.to_string());
        }
    }

    /// `\Provides…` set the banner of a load.
    pub(crate) fn note_identification(&mut self, load: usize, provided: Option<String>) {
        if let Some(rec) = self.package_capture.as_mut() {
            rec.log.identifications.push((load, provided));
        }
    }

    /// Register given a name: remember which allocation counter handed it out.
    pub(crate) fn note_register_origin(&mut self, kind: RegKind, index: u16) {
        let Some(rec) = self.package_capture.as_ref() else { return };
        if index == crate::tex::UNNUMBERED {
            return;
        }
        let tracking = self.env.track_reads(false);
        let mut holders: Vec<u8> = Vec::new();
        for (counter, before) in &rec.before.counters {
            if !allocates(*counter).contains(&kind) {
                continue;
            }
            let now = self.register_sym_peek(i64::from(*counter)).and_then(|sym| self.env.slot_value(sym));
            if now == Some(i64::from(index)) && now != *before {
                holders.push(*counter);
            }
        }
        self.env.track_reads(tracking);
        // Multiple holders: the first to hand out this kind is the primary.
        if holders.len() > 1
            && let Some(primary) = holders.iter().copied().find(|c| allocates(*c).first() == Some(&kind))
        {
            holders = vec![primary];
        }
        if let &[counter] = &holders[..] {
            if let Some(rec) = self.package_capture.as_mut() {
                rec.log.origins.push(((kind, index), counter));
            }
            // Allocated register holds nothing from before (regardless of renumbering).
            let storage = self.register_sym(kind, i64::from(index));
            self.env.mark_written(storage, u8::MAX);
        }
    }

    /// `\csname` at `span`: first recorded role stands.
    pub(crate) fn note_csname(&mut self, span: Span, role: CsnameRole) {
        if let Some(rec) = &mut self.package_capture
            && rec.log.csname_spans.insert(span)
        {
            rec.log.csnames.push((span, role));
        }
        self.out.facts.csnames.entry(span).or_insert(role);
    }

    /// End the segment at `mark` and begin the next.
    fn checkpoint(&mut self, mark: LineMark) {
        // A degraded run's state is one no full-precision run reaches: its
        // segments would be keyed by a root they were not recorded against.
        if self.pressure > 0 {
            if let Some(rec) = self.package_capture.as_mut() {
                rec.log.spoiled.get_or_insert(DEGRADED);
            }
            return;
        }
        let Some(node) = self.encode_segment(mark) else {
            if let Some(rec) = self.package_capture.as_mut() {
                rec.log.spoiled.get_or_insert("a segment could not be written");
            }
            return;
        };
        let Some(rec) = self.package_capture.as_mut() else { return };
        rec.last_line = mark.line;
        rec.chain.push(node);
        self.start_segment();
    }

    /// Package file ended or recording stopped: store and finish.
    pub(super) fn finish_recording(&mut self) {
        let Some(rec) = self.package_capture.take() else { return };
        crate::progress::clear();
        self.env.end_tracking();
        self.out.interner.record_touches(false);
        self.out.graph.record_touches(false);
        self.out.graph.record_log(false);
        self.out.calls.record(false);
        self.out.observed.record(false);
        if let Some(why) = rec.log.spoiled
            && self.cfg.verbose >= 1
        {
            crate::progress::note!("satex: {} is recorded only up to line {}: {why}", rec.name, rec.last_line);
        }
        let total = self.out.steps - rec.before.steps;
        if rec.chain.is_empty() {
            if rec.served == 0 {
                let why = rec.log.spoiled.unwrap_or("no line of it was at rest");
                self.out.package_caches.push((
                    rec.name.clone(),
                    rec.cache.display().to_string(),
                    format!("not stored: {why}"),
                ));
            }
            return;
        }
        let Some(dir) = rec.cache.parent() else { return };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        let Some(_lock) = crate::format::StoreLock::try_take(&rec.cache) else {
            self.out.package_caches.push((
                rec.name.clone(),
                rec.cache.display().to_string(),
                "not stored: another instance is storing it".into(),
            ));
            return;
        };
        rewrite_before_store(&rec.cache);
        let bytes = std::fs::read(&rec.cache).unwrap_or_default();
        let mut file = decode(&rec.cache, &bytes).unwrap_or(CacheFile { version: ENCODING_VERSION, trees: Vec::new() });
        let target = append_target(&mut file, &rec.root, rec.parent, rec.parent_id);
        note_append(&rec.name, rec.parent, target, rec.chain.len());
        let Some(tree) = target else {
            self.out.package_caches.push((rec.name.clone(), rec.cache.display().to_string(), REWRITTEN.into()));
            return;
        };
        let nodes = &mut file.trees[tree].nodes;
        let mut parent = rec.parent;
        for mut node in rec.chain {
            node.parent = parent;
            parent = Some(nodes.len() as u32);
            nodes.push(node);
        }
        if file.trees.iter().map(|t| t.nodes.len()).sum::<usize>() > MAX_NODES {
            file.trees.truncate(1);
        }
        // Streamed: the file is held as read, plus the chain, not a third copy.
        let temporary = crate::format::temporary_for(&rec.cache);
        let written = std::fs::File::create(&temporary).ok().and_then(|out| {
            let mut out = std::io::BufWriter::new(out);
            postcard::to_io(&file, &mut out).ok()?;
            std::io::Write::flush(&mut out).ok()
        });
        drop(file);
        drop(bytes);
        let stored = written.is_some() && std::fs::rename(&temporary, &rec.cache).is_ok();
        if !stored {
            let _ = std::fs::remove_file(&temporary);
        }
        if stored {
            crate::progress::done(&format!("cached {}", rec.label), rec.started.elapsed());
            crate::format::save_hashes(dir, self.cfg.limits.cache_size.as_u64());
            crate::format::prune_family(
                dir,
                &family(&rec.name),
                crate::format::cache_key(),
                self.cfg.limits.cache_size.as_u64(),
            );
            prune(dir, self.cfg.limits.cache_size.as_u64());
            let cache_text = rec.cache.display().to_string();
            let state = match rec.served {
                0 => "stored".to_string(),
                served => {
                    // Replay entry says why it stopped.
                    let why = match self.out.package_caches.last() {
                        Some((name, cache, why))
                            if *name == rec.name && *cache == cache_text && why.starts_with("stale: ") =>
                        {
                            let why = format!(" ({})", why.trim_start_matches("stale: "));
                            self.out.package_caches.pop();
                            why
                        }
                        _ => String::new(),
                    };
                    format!("{} % cached{why}, rest stored", served * 100 / total.max(1))
                }
            };
            self.out.package_caches.push((rec.name.clone(), cache_text, state));
        }
    }

    /// Segment from last checkpoint to `mark`, encoded against its own tables.
    fn encode_segment(&mut self, mark: LineMark) -> Option<Node<'static>> {
        let roots = self.cache_roots();
        let mut reads = self.env.take_read_marks();
        let dirty = self.env.take_dirty();
        if let Some(rec) = self.package_capture.as_mut() {
            rec.seg_assigned.extend(dirty.iter().map(|d| d.0));
        }
        let rec = self.package_capture.as_ref()?;
        // Names with copied text at segment start, and names stored whole despite holding one.
        let seg_binding = |sym: Sym| -> Option<&Binding> {
            match rec.seg_start.get(&sym) {
                Some(b) => b.as_ref(),
                None => match rec.seg_prior.get(&sym) {
                    Some(b) => b.as_ref(),
                    None => rec.before.slots.get(sym.0 as usize).and_then(|b| b.as_ref()),
                },
            }
        };
        let seg_text = |sym: Sym| seg_binding(sym).and_then(|b| crate::env::copied_text(&b.meaning)).cloned();
        let whole: std::cell::RefCell<Vec<Sym>> = Default::default();
        let m = rec.marks.clone();
        let site = rec.site;
        let main = self.out.main_file;
        let before = &rec.before;
        let graph = &self.out.graph;
        let glog = graph.log()?;

        // Vertex table: reached vertices stand for their site; pre-load vertices stand for their definitions.
        let first_new = rec.seg_vertices;
        let table: std::cell::RefCell<(HashMap<NodeId, u32>, Vec<NodeId>)> = Default::default();
        let reached_all: HashSet<NodeId> = graph.touches().iter().copied().collect();
        let vref = |id: NodeId| -> VRef {
            if (id as usize) < before.vertices
                && !reached_all.contains(&id)
                && let Some(owner) = before.def_owner.get(&id)
            {
                return VRef::Before(*owner);
            }
            let mut t = table.borrow_mut();
            if let Some(i) = t.0.get(&id) {
                return VRef::Table(*i);
            }
            let i = t.1.len() as u32;
            t.0.insert(id, i);
            t.1.push(id);
            VRef::Table(i)
        };
        let reached: Vec<(u32, Option<VRef>)> = graph.touches()[m.reached..]
            .iter()
            .zip(&graph.touch_readers()[m.reached..])
            .filter_map(|(id, reader)| match vref(*id) {
                VRef::Table(i) => Some((i, reader.map(vref))),
                VRef::Before(_) => None,
            })
            .collect();
        // Reads of pre-load definitions are replayed as reads (charged to the call that made them there).
        let logged = &glog.edges[m.edges..];
        let mut edges: Vec<(VRef, VRef, u16, Option<VRef>)> = Vec::with_capacity(logged.len());
        let mut links = glog.links[m.links..].iter().peekable();
        for (i, (from, to, kind, reader)) in logged.iter().enumerate() {
            while let Some((_, lfrom, name, lreader, lkind)) = links.next_if(|(at, ..)| *at <= m.edges + i) {
                edges.push((vref(*lfrom), VRef::Before(*name), lkind.0, lreader.map(vref)));
            }
            let to_ref = vref(*to);
            // Read's charge to maker is made again when replayed.
            if *kind == EdgeKind::SIDE_EFFECT_ON_CALL
                && logged.get(i + 1).is_some_and(|(next_from, _, k, next_reader)| {
                    k.intersects(EdgeKind::READS) && next_reader.unwrap_or(*next_from) == *from
                })
            {
                continue;
            }
            edges.push((vref(*from), to_ref, kind.0, reader.map(vref)));
        }
        for (_, lfrom, name, lreader, lkind) in links {
            edges.push((vref(*lfrom), VRef::Before(*name), lkind.0, lreader.map(vref)));
        }
        let extents: Vec<(Span, u32)> =
            glog.extents[m.extents..].iter().map(|(s, l)| (span_out(*s, site), *l)).collect();
        let calls = self.out.calls.recorded()[m.calls..].to_vec();
        let observed = self.out.observed.recorded()[m.observed..].to_vec();

        // Bindings: every part the load wrote.
        let counters: Vec<(u8, Sym, Option<i64>)> = before
            .counters
            .iter()
            .filter_map(|(c, v)| Some((*c, self.register_sym_peek(i64::from(*c))?, *v)))
            .collect();
        let mut moved: Vec<(u8, i64, i64, Vec<VRef>)> = Vec::new();
        for (counter, sym, from) in &counters {
            let binding = self.env.slot(*sym);
            let now = binding.and_then(|b| b.value.as_int());
            if now == *from {
                continue;
            }
            let (Some(from), Some(now)) = (*from, now) else { return None };
            let defs = binding.map(|b| b.defs.iter().map(|d| vref(*d)).collect()).unwrap_or_default();
            moved.push((*counter, from, now, defs));
        }
        let moved_syms: HashSet<Sym> =
            counters.iter().filter(|(c, ..)| moved.iter().any(|(m, ..)| m == c)).map(|(_, s, _)| *s).collect();
        let bindings: Vec<(Sym, Option<CachedBinding>, u8)> = dirty
            .iter()
            .filter(|(sym, bits)| *bits != 0 && !moved_syms.contains(sym))
            .map(|(sym, bits)| {
                let own = before.slots.get(sym.0 as usize).and_then(|b| b.as_ref()).map(|b| &b.defs);
                let spliced = self.env.slot(*sym).and_then(|b| {
                    let (at, len) = self.env.splice(*sym)?;
                    let orig = seg_text(*sym)?;
                    Some((with_text(&b.meaning, |text| cut(text, at, len, &orig))?, at as u32))
                });
                if spliced.is_none() && self.env.carries(*sym) {
                    whole.borrow_mut().push(*sym);
                }
                let b = self.env.slot(*sym).map(|b| CachedBinding {
                    splice: spliced.as_ref().map(|(_, at)| *at),
                    meaning: spliced.as_ref().map_or_else(|| b.meaning.clone(), |(m, _)| m.clone()),
                    value: b.value.clone(),
                    // Pre-load definitions stand for what defines it where replayed.
                    defs: {
                        let mut defs: Vec<VRef> = b
                            .defs
                            .iter()
                            .map(|d| match own {
                                Some(own)
                                    if (*d as usize) < before.vertices
                                        && own.contains(d)
                                        && !reached_all.contains(d) =>
                                {
                                    VRef::Before(*sym)
                                }
                                _ => vref(*d),
                            })
                            .collect();
                        defs.dedup();
                        defs
                    },
                    certain: b.certain,
                    global: b.global,
                });
                (*sym, b, *bits)
            })
            .collect();

        // Facts: convert control deps.
        let cds = |cds: &[ControlDep]| -> Vec<(VRef, bool)> { cds.iter().map(|cd| (vref(cd.on), cd.taken)).collect() };
        let definitions: Vec<CachedDefinition> = rec.log.definitions[m.definitions..]
            .iter()
            .enumerate()
            .map(|(i, (d, relative))| {
                let splice = rec.log.def_splices.get(&(m.definitions + i));
                let spliced = splice.and_then(|&(at, len)| {
                    let orig = seg_text(d.name)?;
                    let mac = d.mac.as_ref()?;
                    let text = cut(&mac.replacement_text, at, len, &orig)?;
                    let mut mac = (**mac).clone();
                    mac.replacement_text = text;
                    Some((Rc::new(mac), at as u32))
                });
                if spliced.is_none() && splice.is_some() {
                    whole.borrow_mut().push(d.name);
                }
                (d, relative, spliced)
            })
            .map(|(d, relative, spliced)| CachedDefinition {
                splice: spliced.as_ref().map(|(_, at)| *at),
                relative: *relative,
                name: d.name,
                by: d.by,
                tag: d.tag.to_string(),
                subject: d.subject,
                mode: d.mode,
                span: span_out(d.span, site),
                package: d.package,
                node: vref(d.node),
                mac: spliced.map_or_else(|| d.mac.clone(), |(mac, _)| Some(mac)),
                depth: d.depth,
                global: d.global,
                redefines: d.redefines,
                certain: d.certain,
                cds: cds(&d.cds),
            })
            .collect();
        let now = self.site_progress();
        let mut expansions = Vec::new();
        for (key, base) in &rec.log.sites {
            let Some(site_state) = self.expansion_sites.get(key) else { continue };
            let current = &self.out.facts.expansions[site_state.fact];
            let template = match rec.log.touched.get(key) {
                Some((fact, _)) => fact,
                None => current,
            };
            let first = !rec.log.first_seen.contains(key);
            let fact = first.then(|| CachedFact {
                package: template.package,
                node: template.node.map(vref),
                meaning: match template.meaning {
                    MeaningKind::Primitive => (0, None),
                    MeaningKind::Macro(id) => (1, Some(vref(id))),
                    MeaningKind::Register => (2, None),
                    MeaningKind::Char => (3, None),
                    MeaningKind::Undefined => (4, None),
                    MeaningKind::Unknown => (5, None),
                    MeaningKind::UndefinedApproximated => (6, None),
                },
                within: template.within,
                arguments: template.arguments.clone(),
                cds: cds(&template.cds),
            });
            expansions.push(CachedExpansion {
                name: key.1,
                span: span_out(key.0, site),
                fact,
                count: current.count.saturating_sub(*base),
                mode: rec.log.site_modes.get(key).copied().unwrap_or_default(),
                repeats: site_state.repeats,
                current: site_state.progress == now,
            });
        }
        let occurrences: Vec<CachedOccurrence> = self.out.facts.occurrences
            [m.occurrences.min(self.out.facts.occurrences.len())..]
            .iter()
            .map(|o| CachedOccurrence {
                kind: o.kind,
                key: o.key.clone(),
                detail: o.detail.clone(),
                span: span_out(o.span, site),
                package: o.package,
                node: vref(o.node),
                section: o.section.as_deref().map(str::to_string),
                certain: o.certain,
            })
            .collect();
        let colors: Vec<CachedColorDef> = self.out.facts.colors[m.colors.min(self.out.facts.colors.len())..]
            .iter()
            .map(|c| CachedColorDef {
                name: c.name.clone(),
                value: c.value.clone(),
                declared: c.declared.clone(),
                spec_span: c.spec_span.map(|s| span_out(s, site)),
                span: span_out(c.span, site),
                via: c.via,
                package: c.package,
                certain: c.certain,
            })
            .collect();
        let color_uses: Vec<CachedColorUse> = self.out.facts.color_uses
            [m.color_uses.min(self.out.facts.color_uses.len())..]
            .iter()
            .map(|u| CachedColorUse {
                name: u.name.clone(),
                role: role_out(u.role),
                defined: u.defined,
                span: span_out(u.span, site),
                via: u.via,
                package: u.package,
                certain: u.certain,
            })
            .collect();
        let color_stack: Vec<CachedColorStack> = self.out.facts.color_stack
            [m.color_stack.min(self.out.facts.color_stack.len())..]
            .iter()
            .map(|e| (e.stack, index_of(&STACK_OPS, &e.op), span_out(e.span, site), e.certain))
            .collect();
        let gap_of: HashMap<usize, (&'static str, Sym)> =
            rec.log.gaps_new[m.gaps_new..].iter().map(|(code, sym, at)| (*at, (*code, *sym))).collect();
        let diagnostics = self.out.facts.diagnostics[m.diagnostics..]
            .iter()
            .enumerate()
            .map(|(i, d)| {
                (
                    index_of(&SEVERITIES, &d.severity),
                    d.code.to_string(),
                    d.message.clone(),
                    span_out(d.span, site),
                    gap_of.get(&(m.diagnostics + i)).map(|(code, sym)| (code.to_string(), *sym)),
                    d.count,
                )
            })
            .collect();
        let loads = self.out.facts.loads[m.loads..]
            .iter()
            .map(|l| {
                let case_mismatch_on_disk = match l.status {
                    LoadStatus::CaseMismatch { on_disk } => Some(on_disk),
                    _ => None,
                };
                CachedLoad {
                    name: l.name.clone(),
                    kind: l.kind,
                    options: l.options.clone(),
                    span: span_out(l.span, site),
                    by: l.by,
                    path: l.path.as_ref().map(|p| roots.encode(p)),
                    status: status_index(&l.status),
                    required: l.required.clone(),
                    provided: l.provided.clone(),
                    depth: l.depth,
                    case_mismatch_on_disk,
                }
            })
            .collect();
        let identifications = rec.log.identifications[m.identifications..]
            .iter()
            .map(|(load, provided)| (*load as i64 - before.loads as i64, provided.clone()))
            .collect();
        let lists = Lists {
            catcodes: self.catcodes.diff(&CatcodeTable::latex()),
            end_line: self.end_line.0,
            loaded: self.loaded.to_vec(),
            class_options: self.class_options.clone(),
            wild: self.wild.clone(),
            verbatim: {
                let mut v: Vec<_> = self.verbatim_environments.iter().cloned().collect();
                v.sort();
                v
            },
            section: self.section.as_deref().map(str::to_string),
            lua: self.lua.files().iter().map(|f| PathBuf::from(roots.encode(&f.display().to_string()))).collect(),
            counter_namespace: self.counter_namespace.clone(),
            streams: {
                let mut s: Vec<_> =
                    self.streams.iter().map(|(n, (lines, at))| (*n, lines.as_ref().clone(), *at)).collect();
                s.sort_by_key(|(n, ..)| *n);
                s
            },
            outputs: {
                let mut o: Vec<_> = self.outputs.iter().map(|(n, t)| (n.clone(), t.clone())).collect();
                o.sort();
                o
            },
            aliases: {
                let mut a: Vec<_> = self.env.aliases.iter().map(|(k, v)| (*k, *v)).collect();
                a.sort();
                a
            },
        };
        let lists = (rec.lists.as_ref() != Some(&lists)).then_some(lists);
        let identifying = self.identifying.map(|i| i as i64 - before.loads as i64);
        let scalars = Scalars {
            steps: self.out.steps - before.steps,
            progress: self.source_progress - before.progress,
            assignments: self.env.assignments - before.assignments,
            recent: self.env.recent(),
            after_assignment: self.after_assignment,
            last_file: self.last_file.map(|f| Span::new(f, 0, 0)),
            file_call: self.file_call.map(|(sym, span)| (sym, span_out(span, site))),
            identifying,
            call_shape: self
                .call_shape
                .as_deref()
                .cloned()
                .filter(|(sym, span, ..)| rec.seg_call != Some((*sym, *span)))
                .map(|(sym, span, shape, depth)| (sym, span_out(span, site), shape, depth)),
            finished_call: rec.seg_call_finished,
            last_named_cs: self.last_named_cs,
            page_touched: self.page_touched,
            recent_file_spans: {
                let before = &rec.before.recent_file_spans;
                let kept = (0..=before.len())
                    .find(|e| self.recent_file_spans.starts_with(&before[*e..]))
                    .map_or(0, |e| before.len() - e);
                self.recent_file_spans[kept.min(self.recent_file_spans.len())..]
                    .iter()
                    .map(|s| span_out(*s, site))
                    .collect()
            },
        };
        let mut vertices = Vec::new();
        let mut i = 0;
        while i < table.borrow().1.len() {
            let id = table.borrow().1[i];
            let v = &graph.vertices[id as usize];
            vertices.push(CachedVertex {
                tag: index_of(&TAGS, &v.tag),
                name: v.name,
                key: v.key,
                new: id >= first_new,
                reader: before.reader == Some(id),
                span: span_out(v.span, site),
                cds: cds(&v.cds),
                within: v.within.map(vref),
            });
            i += 1;
        }
        let effect = Effect {
            names: self.out.interner.touches()[m.names..].to_vec(),
            files: self.out.files[m.files..]
                .iter()
                .enumerate()
                .map(|(i, f)| {
                    (
                        roots.encode(&f.path),
                        f.kind,
                        self.out.entry.get(m.files + i).copied().flatten().map(|s| span_out(s, site)),
                    )
                })
                .collect(),
            vertices,
            reached,
            edges,
            extents,
            calls,
            observed,
            bindings,
            moved,
            origins: rec.log.origins[m.origins..].to_vec(),
            definitions,
            expansions,
            loads,
            identifications,
            occurrences,
            colors,
            color_uses,
            color_stack,
            diagnostics,
            metadata: self.out.metadata[m.metadata..]
                .iter()
                .map(|x| (x.field.to_string(), x.text.clone(), x.source.clone()))
                .collect(),
            csnames: rec.log.csnames[m.csnames..]
                .iter()
                .map(|(span, role)| {
                    let (kind, global, expand) = role_out(*role);
                    (span_out(*span, site), kind, global, expand)
                })
                .collect(),
            shapes: rec.log.shapes[m.shapes..].to_vec(),
            gaps: rec.log.gaps_new[m.gaps_new..].iter().map(|(code, sym, _)| (code.to_string(), *sym)).collect(),
            scalars,
            lists: lists.clone(),
            carried: self.carried.delta(&before.carried),
        };

        // Reads against initial state (allocation counters are no dependencies; replay uses its own).
        for sym in whole.into_inner() {
            match reads.iter_mut().find(|(s, _)| *s == sym) {
                Some((_, bits)) => *bits |= MEANING,
                None => reads.push((sym, MEANING)),
            }
        }
        let reads: Vec<(Sym, u8)> = reads.into_iter().filter(|(sym, _)| !moved_syms.contains(sym)).collect();
        let mut copies: Vec<(Sym, u32, i64)> =
            rec.seg_copies.iter().map(|(sym, (len, room))| (*sym, *len as u32, *room)).collect();
        copies.sort_by_key(|(sym, ..)| *sym);
        let hash = hash_mode(self);
        let (read_bindings, _) = codec::with(hash, || {
            reads
                .iter()
                .map(|(sym, bits)| {
                    let before_binding = seg_binding(*sym);
                    let alias = before.aliases.get(sym).map(|a| self.out.interner.name(*a).to_string());
                    let digested = digested_kinds(self.out.interner.name(*sym), *bits);
                    (*sym, *bits, binding_digest(before_binding, alias.as_deref(), digested))
                })
                .collect::<Vec<(Sym, u8, u64)>>()
        });
        let gaps = rec.log.gaps_seen[m.gaps_seen..]
            .iter()
            .filter(|g| rec.seg_gaps.contains(g))
            .map(|(code, sym)| (code.to_string(), *sym))
            .collect();
        let reads = Reads { copies, bindings: read_bindings, gaps };

        // Files opened and new ones, and first-time name lookups.
        let mut stamps = Vec::new();
        let mut seen = HashSet::default();
        let opened = rec.log.opened[m.opened..].iter().cloned();
        let new_files = self.out.files[m.files..].iter().map(|f| f.path.clone());
        for path in opened.chain(new_files) {
            if path.starts_with('<') || !seen.insert(path.clone()) {
                continue;
            }
            let hash = crate::format::content_hash(Path::new(&path))?;
            stamps.push((roots.encode(&path), hash));
        }
        let lookups: Vec<(String, LoadKind, Option<PathBuf>)> = self
            .resolver
            .lookups()
            .filter(|(key, _)| !rec.lookups.contains(*key))
            .map(|((name, kind), found)| {
                (name.clone(), *kind, found.as_ref().map(|f| PathBuf::from(roots.encode(&f.display().to_string()))))
            })
            .collect();
        let steps = self.out.steps - m.steps;

        let encode = || Mode::Encode {
            syms: HashMap::default(),
            sym_order: Vec::new(),
            files: HashMap::default(),
            file_order: Vec::new(),
        };
        let ((reads, effect), mode) = codec::with(encode(), || (postcard::to_allocvec(&reads), encode_effect(&effect)));
        let Mode::Encode { sym_order, file_order, .. } = mode else { return None };
        let names = sym_order.iter().map(|i| self.out.interner.name(Sym(*i)).to_string()).collect();
        let paths = file_order
            .iter()
            .map(|f| match *f {
                LOAD_SITE => SITE.to_string(),
                f if f == main => MAIN.to_string(),
                f => self.out.files.get(f as usize).map_or_else(String::new, |i| roots.encode(&i.path)),
            })
            .collect();
        let node = Node {
            parent: None,
            end: mark,
            last: mark.ended,
            steps,
            stamps,
            lookups: lookups.clone(),
            names,
            paths,
            reads: Cow::Owned(reads.ok()?),
            effect: Cow::Owned(effect.ok()?),
        };
        let rec = self.package_capture.as_mut()?;
        for (name, kind, _) in lookups {
            rec.lookups.insert((name, kind));
        }
        rec.log.site_modes.clear();
        for (key, _) in std::mem::take(&mut rec.log.sites) {
            rec.log.first_seen.insert(key);
        }
        if lists.is_some() {
            rec.lists = lists;
        }
        Some(node)
    }
}

/// Counter that handed out a register: recorded one, or the one its name came from.
fn origin_of(origins: &[((RegKind, u16), u8)], kind: RegKind, index: i64) -> Option<u8> {
    origins
        .iter()
        .find(|((k, i), _)| *k == kind && i64::from(*i) == index)
        .or_else(|| origins.iter().find(|((_, i), c)| i64::from(*i) == index && allocates(*c).contains(&kind)))
        .map(|(_, c)| *c)
}

/// The cached trees in `bytes`, read from `file`, or why unusable; `None`
/// when the file was empty.
fn decode<'a>(file: &Path, bytes: &'a [u8]) -> Result<CacheFile<'a>, Option<String>> {
    if bytes.is_empty() {
        return Err(None);
    }
    let cached: CacheFile = postcard::from_bytes(bytes).map_err(|e| Some(format!("cannot decode it: {e}")))?;
    if cached.version != ENCODING_VERSION {
        return Err(Some("interpreter version: written in another encoding".into()));
    }
    crate::format::touch(file);
    Ok(cached)
}

fn same_tokens(a: &[Token], b: &[Token]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.tok == y.tok && x.span == y.span)
}

/// `text` with `len` tokens at `at` removed (if they match `orig`).
fn cut(text: &[Token], at: usize, len: usize, orig: &[Token]) -> Option<Rc<[Token]>> {
    let slice = text.get(at..at + len)?;
    if !same_tokens(slice, orig) {
        return None;
    }
    Some(text[..at].iter().chain(&text[at + len..]).copied().collect())
}

/// `text` with `orig` put in at `at`.
fn splice_in(text: &[Token], at: usize, orig: &[Token]) -> Option<Rc<[Token]>> {
    let head = text.get(..at)?;
    Some(head.iter().chain(orig).chain(&text[at..]).copied().collect())
}

/// `meaning` with its replacement text changed by `f`.
fn with_text(meaning: &Meaning, f: impl FnOnce(&[Token]) -> Option<Rc<[Token]>>) -> Option<Meaning> {
    let m = meaning.as_macro()?;
    let mut m = (**m).clone();
    m.replacement_text = f(&m.replacement_text)?;
    Some(Meaning::Macro(Rc::new(m)))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::config::Config;
    use crate::machine::{Analysis, Machine};

    fn node(steps: u64) -> Node<'static> {
        Node {
            parent: None,
            end: LineMark { pos: 0, line: 0, col: 0, force_eof: None, ended: false },
            last: false,
            steps,
            stamps: Vec::new(),
            lookups: Vec::new(),
            names: Vec::new(),
            paths: Vec::new(),
            reads: Cow::Owned(Vec::new()),
            effect: Cow::Owned(vec![steps as u8]),
        }
    }

    fn tree<'a>(root: &str, nodes: Vec<Node<'a>>) -> Tree<'a> {
        Tree { root: vec![(root.to_string(), 1)], nodes }
    }

    fn root_of(name: &str) -> Vec<(String, u64)> {
        vec![(name.to_string(), 1)]
    }

    #[test]
    fn a_recording_is_appended_to_the_tree_its_root_names() {
        let mut file =
            CacheFile { version: ENCODING_VERSION, trees: vec![tree("other", vec![]), tree("mine", vec![node(1)])] };
        let id = Some(node_id(&node(1)));
        // The index the run kept is 1 and still is: the append goes there.
        assert_eq!(append_target(&mut file, &root_of("mine"), Some(0), id), Some(1));
        // A run that replayed nothing may always start a chain of its own.
        assert_eq!(append_target(&mut file, &root_of("mine"), None, None), Some(1));
        // A root nothing holds gets a tree of its own, at the front.
        assert_eq!(append_target(&mut file, &root_of("new"), None, None), Some(0));
        assert_eq!(file.trees[0].root, root_of("new"));
    }

    #[test]
    fn a_tree_inserted_under_a_run_moves_its_index_and_the_recording_is_dropped() {
        let id = Some(node_id(&node(1)));
        // The run read a file whose tree 0 was `mine`; another process then
        // put its own tree in front, so index 0 is now someone else's.
        let mut file = CacheFile {
            version: ENCODING_VERSION,
            trees: vec![tree("theirs", vec![node(9)]), tree("mine", vec![node(1)])],
        };
        assert_eq!(append_target(&mut file, &root_of("mine"), Some(0), id), Some(1), "found by root, not by index");
        assert_eq!(file.trees[0].nodes.len(), 1, "the other tree was not written into");
        // And when the node at that index is no longer the node that was
        // replayed, there is nowhere sound to put the chain.
        let mut file = CacheFile { version: ENCODING_VERSION, trees: vec![tree("mine", vec![node(7)])] };
        assert_eq!(append_target(&mut file, &root_of("mine"), Some(0), id), None);
    }

    #[test]
    fn a_tree_truncated_away_under_a_run_drops_the_recording() {
        let id = Some(node_id(&node(1)));
        // MAX_TREES pushed `mine` off the end while this run was reading.
        let trees = (0..MAX_TREES).map(|i| tree(&format!("other{i}"), vec![node(9)])).collect();
        let mut file = CacheFile { version: ENCODING_VERSION, trees };
        assert_eq!(append_target(&mut file, &root_of("mine"), Some(0), id), None);
        assert_eq!(file.trees.len(), MAX_TREES, "nothing was inserted");
        assert!(file.trees.iter().all(|t| t.nodes.len() == 1), "no tree was written into");
    }

    /// The hook and the kernel cache are process-wide: these run one at a time.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Clears the hook however the test ends.
    struct Hooked;

    impl Drop for Hooked {
        fn drop(&mut self) {
            *BEFORE_STORE.lock().unwrap_or_else(|e| e.into_inner()) = None;
        }
    }

    fn hook(f: impl Fn(&Path) + Send + 'static) -> Hooked {
        *BEFORE_STORE.lock().unwrap_or_else(|e| e.into_inner()) = Some(Box::new(f));
        Hooked
    }

    fn installed() -> bool {
        crate::paths::which("kpsewhich").is_ok()
    }

    /// One kernel cache for every test here, as `tests/it/package_cache.rs` does.
    fn kernel() -> &'static Path {
        static KERNEL: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        KERNEL.get_or_init(|| {
            let dir = std::env::temp_dir().join(format!("satex-graft-kernel-{}", std::process::id()));
            let cfg = Config {
                cache: crate::config::Cache { dir: Some(dir.clone()), ..Default::default() },
                ..Config::default()
            };
            Machine::analyze("\\relax\n", Some(&dir.join("kernel.tex")), &cfg);
            dir
        })
    }

    /// A package whose last lines read a name the preamble may define, so a
    /// second run replays the front of it and records the rest: that is the
    /// run that holds an index into a tree someone else can move.
    const GUARD: &str = "\\ProvidesPackage{guard}[2020/01/01 v1]\n\\def\\guardone{1}\n\\relax\n\
\\def\\guardtwo{2}\n\\relax\n\\ifdefined\\userflag\\def\\guardflagged{}\\fi\n\\def\\guardthree{3}\n";

    fn project(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("satex-graft-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cache = dir.join("cache");
        std::fs::create_dir_all(&cache).expect("temporary directory");
        std::fs::write(dir.join("guard.sty"), GUARD).expect("guard.sty");
        for entry in std::fs::read_dir(kernel()).expect("kernel cache").flatten() {
            let name = entry.file_name();
            if name.to_string_lossy().starts_with("format-") {
                std::fs::copy(entry.path(), cache.join(&name)).expect("kernel cache copied");
            }
        }
        dir
    }

    fn analyze(dir: &Path, source: &str) -> Analysis {
        let cfg = Config {
            cache: crate::config::Cache { dir: Some(dir.join("cache")), ..Default::default() },
            ..Config::default()
        };
        let path = dir.join("doc.tex");
        std::fs::write(&path, source).expect("doc.tex");
        Machine::analyze(source, Some(&path), &cfg)
    }

    const PLAIN: &str = "\\documentclass{article}\n\\usepackage{guard}\n\\begin{document}\\relax\\end{document}\n";
    const FLAGGED: &str =
        "\\documentclass{article}\n\\def\\userflag{}\n\\usepackage{guard}\n\\begin{document}\\relax\\end{document}\n";

    /// The cache file the run used for `package`: never its lock or a
    /// temporary, which share its name's start.
    fn cache_of(analysis: &Analysis, package: &str) -> Option<PathBuf> {
        analysis.package_caches.iter().find(|(name, ..)| name == package).map(|(_, path, _)| PathBuf::from(path))
    }

    fn state_of(analysis: &Analysis, package: &str) -> Option<String> {
        analysis.package_caches.iter().rev().find(|(name, ..)| name == package).map(|(.., state)| state.clone())
    }

    /// Rewrite cache as a second process would: insert `bogus` trees, optionally keep existing.
    fn sabotage(path: &Path, bogus: usize, keep: bool) {
        let Ok(bytes) = std::fs::read(path) else { return };
        let Ok(mut file) = postcard::from_bytes::<CacheFile>(&bytes) else { return };
        if !keep {
            file.trees.clear();
        }
        for i in 0..bogus {
            file.trees.insert(0, tree(&format!("bogus{i}"), vec![node(9)]));
        }
        file.trees.truncate(MAX_TREES);
        if let Ok(bytes) = postcard::to_allocvec(&file) {
            let _ = std::fs::write(path, bytes);
        }
    }

    /// Facts a run learned (for comparing sabotaged cache vs. direct read).
    fn facts(a: &Analysis) -> Vec<String> {
        let mut out: Vec<String> = a
            .facts
            .defs
            .iter()
            .map(|d| format!("{} {}:{}", a.interner.name(d.name), a.file_name(d.span.file), d.span.line))
            .collect();
        out.extend(
            a.facts.diagnostics.iter().map(|d| format!("{} {}:{}", d.code, a.file_name(d.span.file), d.span.line)),
        );
        out
    }

    /// Insert `bogus` trees while storing (optionally keep existing tree).
    /// Tests the window between segment selection and store.
    fn grafting(name: &str, bogus: usize, keep: bool) -> (Option<u32>, Option<usize>) {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = project(name);
        // First run records package; second replays what fits, records rest (holds tree index).
        let first = analyze(&dir, PLAIN);
        assert_eq!(state_of(&first, "guard").as_deref(), Some("stored"), "{:?}", first.package_caches);
        let target_file = cache_of(&first, "guard").expect("a cache for guard");
        let once = std::sync::atomic::AtomicBool::new(false);
        LAST_APPEND.lock().unwrap_or_else(|e| e.into_inner()).clear();
        let _hooked = hook(move |path: &Path| {
            if path.file_name().is_some_and(|n| n.to_string_lossy().starts_with("package-guard-"))
                && !once.swap(true, std::sync::atomic::Ordering::SeqCst)
            {
                sabotage(path, bogus, keep);
            }
        });
        let second = analyze(&dir, FLAGGED);
        drop(_hooked);
        let seen = LAST_APPEND.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let (_, parent, target, chain) =
            seen.iter().rev().find(|(n, ..)| n == "guard").cloned().expect("guard reached the store");
        assert!(chain > 0, "the run recorded something to store");

        // Bogus trees keep their original node; nothing unread was written.
        let bytes = std::fs::read(&target_file).expect("the cache is readable");
        let file: CacheFile = postcard::from_bytes(&bytes).expect("the cache decodes");
        for (i, t) in file.trees.iter().enumerate() {
            if t.root.first().is_some_and(|(root, _)| root.starts_with("bogus")) {
                assert_eq!(t.nodes.len(), 1, "tree {i} was grafted into: {:?}", t.root);
            }
        }
        // Chain went under this package's root or nowhere.
        match target {
            Some(t) => {
                assert!(
                    !file.trees[t].root.first().is_some_and(|(root, _)| root.starts_with("bogus")),
                    "the chain went under a root this run never read"
                );
                assert_eq!(state_of(&second, "guard").as_deref(), Some("stored"), "{:?}", second.package_caches);
            }
            None => {
                assert!(parent.is_some(), "only a chain hanging from a node is refused");
                assert_eq!(state_of(&second, "guard").as_deref(), Some(REWRITTEN), "{:?}", second.package_caches);
            }
        }
        // Later run against rewritten cache reports same facts as direct read (same directory).
        let after = analyze(&dir, FLAGGED);
        for entry in std::fs::read_dir(dir.join("cache")).expect("cache").flatten() {
            if entry.file_name().to_string_lossy().starts_with("package-") {
                std::fs::remove_file(entry.path()).expect("removed");
            }
        }
        let uncached = analyze(&dir, FLAGGED);
        assert_eq!(facts(&after), facts(&uncached), "a run against the rewritten cache");
        (parent, target)
    }

    #[test]
    fn a_tree_inserted_while_a_run_stores_is_not_grafted_into() {
        if !installed() {
            return;
        }
        // Index 0 is someone else's tree now, so a store that trusted the
        // index it kept would push this package's segments under it.
        let (_, target) = grafting("insert", 1, true);
        assert_ne!(target, Some(0), "the chain went to the tree that was pushed in front");
    }

    #[test]
    fn a_tree_truncated_away_while_a_run_stores_drops_the_recording() {
        if !installed() {
            return;
        }
        grafting("truncate", MAX_TREES, false);
    }

    /// A package that leaves an `\openin` stream half read, a counter
    /// stepped, a switch of its own and a name built in part.
    const STREAMY: &str = "\\ProvidesPackage{streamy}[2020/01/01 v1]\n\\newread\\streamyin\n\
\\openin\\streamyin=streamy-data.tex\n\\read\\streamyin to\\streamyline\n\\stepcounter{section}\n\
\\newif\\ifstreamy\n\\expandafter\\def\\csname streamy@\\streamyline\\endcsname{}\n\\relax\n";

    #[test]
    fn a_replay_leaves_the_root_a_read_leaves() {
        if !installed() {
            return;
        }
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let dir = project("streamy");
        std::fs::write(dir.join("streamy.sty"), STREAMY).expect("streamy.sty");
        std::fs::write(dir.join("streamy-data.tex"), "first\nsecond\nthird\n").expect("data");
        let doc = "\\documentclass{article}\n\\usepackage{streamy}\n\\usepackage{guard}\n\
\\begin{document}\\read\\streamyin to\\x\\section{A}\\relax\\end{document}\n";
        let runs: Vec<(Analysis, Vec<String>)> = (0..3)
            .map(|_| {
                LOAD_ENDS.lock().unwrap_or_else(|e| e.into_inner()).clear();
                let run = analyze(&dir, doc);
                (run, std::mem::take(&mut *LOAD_ENDS.lock().unwrap_or_else(|e| e.into_inner())))
            })
            .collect();
        for (run, ends) in &runs[1..] {
            for package in ["streamy", "guard"] {
                assert_eq!(state_of(run, package).as_deref(), Some("cached"), "{:?}", run.package_caches);
            }
            assert_eq!(facts(run), facts(&runs[0].0));
            assert_eq!(ends, &runs[0].1, "what each load carries on");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
