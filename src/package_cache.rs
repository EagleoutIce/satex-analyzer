//! Package caches: what reading a `\usepackage` or `\documentclass` of the
//! document did, kept as a chain of segments and replayed by any later run
//! whose state agrees with what each segment looked at.
//!
//! While a package is read, its file is cut into segments at *checkpoints*:
//! the start of a line of the package's own file, with nothing of it pending
//! on the input stack and the interpreter otherwise at rest.  Each segment
//! records its *reads* — every part of a binding it looked at while that part
//! still held what it held before the load, the hooks it looked at, the
//! packages it asked about, the options it was given — and its *effect*: the
//! bindings, hooks, facts, vertices and edges it made, in order, and the
//! state it left.
//!
//! A later load of the same file walks the segments in order and checks each
//! one's reads against the state it is in.  Every segment that agrees is
//! installed from the cache; at the first that does not, the file is read on
//! from the checkpoint before it, and what is read from there is stored as
//! another branch of the package's tree of segments.  What a package does
//! before it looks at its options or its context — typically everything up
//! to `\ProcessOptions` — is therefore shared by every document.
//!
//! Register allocation is an effect, not a read: a replay allocates again
//! from the counters of the run it is replayed in, and every register the
//! package allocated is renumbered to match.  Names and files are written
//! against each segment's own tables ([`crate::tex::codec`]), so a segment
//! installs into a run that numbers them differently.  Every file a segment
//! opened is stamped, and every file name it looked up must still resolve
//! the same.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use super::{Frame, Machine, Metadata, Site};
use crate::builtins::{DefMode, LoadKind, OccKind};
use crate::env::{Binding, NodeId, ReadKind};
use crate::facts::{
    CsnameRole, Definition, Diagnostic, Expansion, Load, LoadStatus, MeaningKind, Occurrence, Severity,
};
use crate::graph::{ControlDep, EdgeKind, Vertex, VertexTag};
use crate::tex::codec::{self, Mode};
use crate::tex::{CatcodeTable, EndLineChar, FileId, LineMark, MacroDef, Meaning, Mouth, RegKind, Span, Sym, Token};
use crate::value::Value;

const ENCODING_VERSION: u32 = 1;
const INTERPRETER: &str = env!("SATEX_SOURCE_DIGEST");
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

const TAGS: [VertexTag; 5] = [
    VertexTag::Value,
    VertexTag::Use,
    VertexTag::FunctionCall,
    VertexTag::VariableDefinition,
    VertexTag::FunctionDefinition,
];
const STATUSES: [LoadStatus; 7] = [
    LoadStatus::Read,
    LoadStatus::NotFollowed,
    LoadStatus::Skipped,
    LoadStatus::AlreadyLoaded,
    LoadStatus::TooDeep,
    LoadStatus::NotFound,
    LoadStatus::Unreadable,
];
const SEVERITIES: [Severity; 4] =
    [Severity::Info, Severity::Warning, Severity::Imprecision, Severity::Unsupported];
const REGISTERS: [RegKind; 10] = [
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
];

fn index_of<T: PartialEq>(all: &[T], item: &T) -> u8 {
    all.iter().position(|x| x == item).unwrap_or(0) as u8
}

/// The run's diagnostic codes and fact tags are `&'static str`; a cached one
/// is interned once per process.
fn leak(text: &str) -> &'static str {
    static SEEN: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());
    let mut seen = SEEN.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(found) = seen.iter().find(|s| **s == text) {
        return found;
    }
    let leaked: &'static str = Box::leak(text.to_string().into_boxed_str());
    seen.push(leaked);
    leaked
}

struct Fnv(u64);

impl Fnv {
    fn new() -> Fnv {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 ^= u64::from(*byte);
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
    fn put<T: Serialize + ?Sized>(&mut self, value: &T) {
        let bytes = postcard::to_allocvec(value).unwrap_or_default();
        self.write(&(bytes.len() as u64).to_le_bytes());
        self.write(&bytes);
    }
}


/// A vertex a segment refers to: one of the segment's table, or one of the
/// definitions a name had before the load, which a replay finds by the name.
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
    /// Made by this segment, as opposed to reached by it.
    new: bool,
    /// The call the load is read on behalf of: whichever call that is
    /// where the segment is replayed.
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
    /// Where the text the name held when the segment began goes in the
    /// replacement text, which is stored without it.
    splice: Option<u32>,
}

#[derive(Serialize, Deserialize)]
struct CachedDefinition {
    /// Whether `redefines` is decided by the state the load begins in.
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

/// A call site the segment reached: the fact it makes where there is none
/// yet, and the calls the segment made there.
#[derive(Serialize, Deserialize)]
struct CachedExpansion {
    name: Sym,
    span: Span,
    fact: Option<CachedFact>,
    count: u32,
    /// The modes of the segment's calls there.
    mode: crate::mode::Modes,
    repeats: u32,
    /// Whether the site's last call read nothing new since, which is what
    /// the next call there compares.
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

/// What one segment looked at from before the load, with what it found.
#[derive(Serialize, Deserialize, Default)]
struct Reads {
    /// A name, the parts of its binding read, and their digest.
    bindings: Vec<(Sym, u8, u64)>,
    /// Gaps it found already reported.
    gaps: Vec<(String, Sym)>,
    /// Texts it copied through (a [`ReadKind::Copy`] read): the name, how
    /// long its text was, and by how much it may grow within the limits the
    /// copies met.
    copies: Vec<(Sym, u32, i64)>,
}


/// The small state a checkpoint leaves that changes on almost every line.
#[derive(Serialize, Deserialize, Default)]
struct Scalars {
    /// Tokens read, input read and assignments made since the load began.
    steps: u64,
    progress: u64,
    assignments: u64,
    /// Tokens since the last one read from a file, and the names assigned
    /// meanwhile: what stall detection sees.
    stalled: u64,
    recent: Vec<Sym>,
    after_assignment: Option<Token>,
    last_file: Option<Span>,
    file_call: Option<(Sym, Span)>,
    identifying: Option<i64>,
    call_shape: Option<(Sym, Span, String, usize)>,
    /// Whether the segment completed the call pending at its start.
    finished_call: bool,
    last_named_cs: Option<Sym>,
    /// The tokens read from a file and given back since the load began,
    /// which tell a token given back by a lookahead that came from one.
    /// Those given back before it are the replaying run's own.
    recent_file_spans: Vec<Span>,
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
    aliases: Vec<(Sym, Sym)>,
    wild: crate::machine::Wild,
}

/// One cached diagnostic: severity, code, message, span, and the gap it names, if any.
type CachedDiagnostic = (u8, String, String, Span, Option<(String, Sym)>, u32);

/// What one segment did.
#[derive(Serialize, Deserialize, Default)]
struct Effect {
    /// Names in the order the segment first asked for them.
    names: Vec<Sym>,
    files: Vec<(String, LoadKind, Option<Span>)>,
    vertices: Vec<CachedVertex>,
    /// The table entries the segment reached, in the order it did, with the
    /// call read from a file then: a run that does not have them makes them
    /// in that order, below that call.
    reached: Vec<(u32, Option<VRef>)>,
    /// Every edge added, in order, with the call a read was charged to.
    edges: Vec<(VRef, VRef, u16, Option<VRef>)>,
    extents: Vec<(Span, u32)>,
    calls: Vec<crate::graph::CallEvent>,
    observed: Vec<crate::graph::CallEvent>,
    /// Every name assigned, what it holds at the checkpoint, and which
    /// parts of it the load has written.
    bindings: Vec<(Sym, Option<CachedBinding>, u8)>,
    /// The allocation counters advanced since the load began: which, from
    /// what, to what, and the assignments that set them.
    moved: Vec<(u8, i64, i64, Vec<VRef>)>,
    /// Which counter handed out each register the segment named.
    origins: Vec<((RegKind, u16), u8)>,
    definitions: Vec<CachedDefinition>,
    expansions: Vec<CachedExpansion>,
    loads: Vec<CachedLoad>,
    /// `\Provides…` banners, by load relative to the load's own, which is
    /// -1.
    identifications: Vec<(i64, Option<String>)>,
    occurrences: Vec<CachedOccurrence>,
    diagnostics: Vec<CachedDiagnostic>,
    metadata: Vec<(String, String, String)>,
    csnames: Vec<(Span, u8, bool, bool)>,
    shapes: Vec<(Sym, String)>,
    gaps: Vec<(String, Sym)>,
    scalars: Scalars,
    lists: Option<Lists>,
}

/// One segment of a package's tree.
#[derive(Serialize, Deserialize)]
struct Node {
    parent: Option<u32>,
    /// Where the segment ends: the checkpoint after it.
    end: LineMark,
    /// Whether that is the end of the file.
    last: bool,
    /// Tokens the segment read.
    steps: u64,
    stamps: Vec<(String, u64, u64)>,
    lookups: Vec<(String, LoadKind, Option<PathBuf>)>,
    names: Vec<String>,
    paths: Vec<String>,
    reads: Vec<u8>,
    effect: Vec<u8>,
}

/// The segments of one package read under one engine, configuration and
/// category-code regime.
#[derive(Serialize, Deserialize)]
struct Tree {
    root: Vec<(String, u64)>,
    nodes: Vec<Node>,
}

#[derive(Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    trees: Vec<Tree>,
}

/// What the interpreter logs while a package is recorded, beyond what the
/// state at a checkpoint shows.
#[derive(Default)]
pub(super) struct Log {
    /// Every definition fact the package would make, in order, including
    /// those at a site this run already had, and whether whether it
    /// redefines something was decided by the state before the load.
    pub(super) definitions: Vec<(Definition, bool)>,
    /// Call sites this run already had when the package reached them: the
    /// fact the package would have made, and the count it found.
    pub(super) touched: HashMap<(Span, Sym), (Expansion, u32)>,
    /// Call sites reached since the last checkpoint, in order, with the
    /// calls counted there before.
    sites: Vec<((Span, Sym), u32)>,
    sites_seen: HashSet<(Span, Sym)>,
    /// The modes of the calls at each of those sites.
    site_modes: HashMap<(Span, Sym), crate::mode::Modes>,
    /// Every site first reached while recording, for telling a site's first
    /// segment.
    first_seen: HashSet<(Span, Sym)>,
    pub(super) shapes: Vec<(Sym, String)>,
    pub(super) gaps_seen: Vec<(&'static str, Sym)>,
    pub(super) gaps_new: Vec<(&'static str, Sym, usize)>,
    csnames: Vec<(Span, CsnameRole)>,
    csname_spans: HashSet<Span>,
    origins: Vec<((RegKind, u16), u8)>,
    identifications: Vec<(usize, Option<String>)>,
    opened: Vec<String>,
    /// Set when the package changed something recorded before the segment
    /// began, which no segment can express: recording stops there.
    spoiled: Option<&'static str>,
    /// Definitions whose text holds the text their name held when their
    /// segment began: where and how long.
    def_splices: HashMap<usize, (usize, usize)>,
}

/// The state a load begins in, which reads are compared with.
struct Before {
    slots: Vec<Option<Binding>>,
    aliases: HashMap<Sym, Sym>,
    /// The name each definition vertex before the load belongs to.
    def_owner: HashMap<NodeId, Sym>,
    vertices: usize,
    loads: usize,
    counters: Vec<(u8, Option<i64>)>,
    steps: u64,
    progress: u64,
    assignments: u64,
    recent_file_spans: Vec<Span>,
    /// The call the load is read on behalf of.
    reader: Option<NodeId>,
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
    diagnostics: usize,
    metadata: usize,
    files: usize,
    steps: u64,
}

/// What the interpreter must be back at for a checkpoint: as it was when
/// the package's file was pushed.
#[derive(PartialEq, Debug)]
struct Rest {
    input: usize,
    depth: usize,
    conds: usize,
    cds: usize,
    testing: usize,
    prefixes: (bool, bool, bool, bool),
    long_argument: bool,
    runaway: bool,
    within: usize,
    held: usize,
    edef_depth: usize,
    scanning: usize,
    scan_undecided: bool,
    pdf_string: bool,
    def_body_depth: Option<usize>,
    noexpanded: bool,
    branch_depth: u16,
    split_at: usize,
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
pub(super) struct Recorder {
    pub(super) file: FileId,
    pub(super) expansions_start: usize,
    pub(super) log: Log,
    frame: usize,
    name: String,
    cache: PathBuf,
    site: Span,
    root: Vec<(String, u64)>,
    tree: Option<usize>,
    parent: Option<u32>,
    before: Before,
    rest: Rest,
    marks: Marks,
    last_line: u32,
    chain: Vec<Node>,
    lists: Option<Lists>,
    lookups: HashSet<(String, LoadKind)>,
    /// The state the current segment began in, which its reads of what the
    /// load itself changes are compared with.
    seg_gaps: HashSet<(&'static str, Sym)>,
    seg_vertices: NodeId,
    /// The call whose shape was being read when the segment began: a segment
    /// that completes it completes whatever this run has read of it.
    pub(super) seg_call: Option<(Sym, Span)>,
    seg_call_finished: bool,
    /// What the names holding a copied text held when the segment began,
    /// which their reads are compared with.
    seg_start: HashMap<Sym, Option<Binding>>,
    /// The texts the segment copied through, by name: how long each was when
    /// the segment began, and how much it may grow.
    seg_copies: HashMap<Sym, (usize, i64)>,
    /// Tokens served from the cache before recording began.
    served: u64,
}

fn family(name: &str) -> String {
    let safe: String =
        name.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    format!("package-{safe}-v{ENCODING_VERSION}-")
}

/// Delete the caches no build can use and bound the rest: see
/// [`crate::format::prune_caches`].
pub fn prune(dir: &Path, bound: u64) -> crate::format::Pruned {
    crate::format::prune_caches(dir, ENCODING_VERSION, bound)
}

fn cache_file(dir: &Path, name: &str, key: u64) -> PathBuf {
    dir.join(format!("{}{INTERPRETER}-{key:016x}.postcard", family(name)))
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
        use crate::builtins::Primitive;
        let role = match binding.and_then(|b| b.meaning.prim()) {
            Some(Primitive::If(_)) => 1u8,
            Some(Primitive::Fi) => 2,
            Some(Primitive::Else) => 3,
            Some(Primitive::Or) => 4,
            _ => 0,
        };
        hash.put(&role);
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


/// The codec mode that writes names and paths out in full.
fn hash_mode(m: &Machine) -> Mode {
    let files = (0..m.out.files.len())
        .map(|i| if i as FileId == m.out.main_file { Rc::from(MAIN) } else { Rc::from(m.out.files[i].path.as_str()) })
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
        let Some(kind) = REGISTERS[..6].iter().copied().filter(|k| rest.starts_with(k.as_str())).max_by_key(|k| k.as_str().len())
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
    /// Whether the file being pushed can be served from, or recorded into,
    /// a package cache: a package or class the document itself loads, at the
    /// outer level, outside every conditional, with the kernel in place.
    fn cacheable_at_push(&self, kind: LoadKind) -> bool {
        let reason = if !matches!(kind, LoadKind::Package | LoadKind::Class)
            || !self.cfg.cache
            || self.cfg.trace
            || self.budget.is_some()
            || self.package_capture.is_some()
            || self.file_depth != self.document_file_depth + 1
        {
            return false;
        } else if self.branch_depth != 0
            || self.env.forking()
            || !self.cds.is_empty()
            || self.conds.iter().any(|c| c.undecided || c.dep)
        {
            "it is loaded under a conditional"
        } else if self.env.depth() != 0 {
            "it is loaded inside a group"
        } else if !self.held.is_empty() || self.edef_depth != 0 || self.scanning != 0 {
            "it is loaded from inside an expansion"
        } else if !self.testing.is_empty() || self.pdf_string {
            "it is loaded while a test is read"
        } else if self.halted || self.out.exhausted {
            "the analysis ran out of budget"
        } else {
            return true;
        };
        if self.cfg.verbose >= 1 {
            eprintln!(
                "satex: a load is not cached: {reason} ({} conditionals open, {} undecided, branch depth {})",
                self.conds.len(),
                self.cds.len(),
                self.branch_depth
            );
        }
        false
    }

    /// The part of the state every package can see and no read set tracks:
    /// the configuration, the engine, the prefixes and flags a load at the
    /// outer level begins with, and the call sites a first expansion could
    /// count as a repeat.
    fn fixed_state(&self, site: Span) -> Vec<(String, u64)> {
        let (digest, _) = codec::with(hash_mode(self), || {
            let mut parts: Vec<(String, u64)> = Vec::new();
            let mut hash = Fnv::new();
            hash.put(&(INTERPRETER, self.out.plugins.engine.as_str()));
            parts.push(("engine".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            let mut settings = self.cfg.clone();
            settings.source = None;
            settings.cache = true;
            settings.rebuild_format = false;
            settings.cache_dir = None;
            settings.verbose = 0;
            settings.timings = false;
            settings.log_gaps = false;
            settings.gaps_log = None;
            settings.cache_index = Default::default();
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
            hash.put(&self.docstrip_dir);
            parts.push(("section and streams".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            let mut pinned: Vec<Sym> = self.pinned.iter().copied().collect();
            pinned.sort();
            hash.put(&pinned);
            parts.push(("pinned names".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            let open: Vec<(Sym, u16)> = self
                .expanding
                .iter()
                .enumerate()
                .filter(|(_, n)| **n > 0)
                .map(|(i, n)| (Sym(i as u32), *n))
                .collect();
            hash.put(&open);
            parts.push(("open expansions".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            // The expansions the load happens inside, which the package's
            // vertices hang from.
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
            // The conditionals the load stands in, decided ones, as latex.ltx
            // reads a package from the arm of `\IfFileExists`: what the
            // package's own `\else` or `\fi` would meet.
            let conds: Vec<(u8, Option<i8>, Span)> = self
                .conds
                .iter()
                .map(|c| (c.limit as u8, c.kind, span_out(c.at, site)))
                .collect();
            hash.put(&conds);
            parts.push(("the conditionals open".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            // Whether the counter namespace is known yet decides whether the
            // package's first counter probes for it.
            hash.put(&self.counter_namespace);
            parts.push(("the counter namespace".into(), std::mem::replace(&mut hash, Fnv::new()).0));
            parts
        });
        digest
    }


    /// What selects a package's tree: the state every segment of it takes
    /// for granted, apart from what the segments' reads check.
    fn root_state(&self, site: Span, path: &str) -> Vec<(String, u64)> {
        let mut parts = self.fixed_state(site);
        let mut hash = Fnv::new();
        hash.put(&(CatcodeTable::latex().diff(&self.catcodes), self.catcodes.diff(&CatcodeTable::latex()), self.end_line.0));
        parts.push(("category codes".into(), hash.0));
        let mut hash = Fnv::new();
        hash.put(&(path, crate::format::stamp(Path::new(path))));
        parts.push(("the package file".into(), hash.0));
        let mut hash = Fnv::new();
        hash.put(&self.lua.files());
        parts.push(("the Lua files read".into(), hash.0));
        parts
    }

    /// The state a load begins in, which its reads are compared with.
    fn before_state(&self) -> Before {
        let slots: Vec<Option<Binding>> =
            (0..self.out.interner.len() as u32).map(|i| self.env.get(Sym(i)).cloned()).collect();
        let mut def_owner = HashMap::new();
        for (i, slot) in slots.iter().enumerate() {
            for def in slot.iter().flat_map(|b| b.defs.iter()) {
                def_owner.entry(*def).or_insert(Sym(i as u32));
            }
        }
        let graph = &self.out.graph;
        let counters = ALLOCATION_COUNTERS
            .map(|n| (n as u8, self.register_sym_peek(n).and_then(|sym| self.env.get(sym)).and_then(|b| b.value.as_int())))
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
        }
    }


    /// Where the interpreter has to be back at for a checkpoint.  Every
    /// field of the machine is named here, with what a package cache does
    /// with it: compared at every checkpoint, recorded in the segments, or
    /// the same throughout a load.  A field added to the machine does not
    /// build until it is placed.
    fn rest_state(&self, input: usize) -> Rest {
        let Machine {
            // The same throughout a load.
            cfg: _,
            base: _,
            resolver: _,
            dont_expand: _,
            unknown: _,
            unknown_rest: _,
            unknown_digits: _,
            unknown_more: _,
            // Recorded: facts, the graph, names, files and bindings.
            out: _,
            env,
            definition_sites: _,
            own_switches: _,
            expansion_sites: _,
            cond_sites: _,
            // Recorded in a segment's lists and scalars.
            catcodes: _,
            lua: _,
            end_line: _,
            after_assignment: _,
            verbatim_environments: _,
            wild: _,
            section: _,
            streams: _,
            loaded: _,
            class_options: _,
            identifying: _,
            last_named_cs: _,
            source_progress: _,
            call_shape: _,
            gaps: _,
            last_file: _,
            recent_file_spans: _,
            file_call: _,
            file_calls: _,
            counter_namespace: _,
            progress_step: _,
            // Part of the tree's root.
            docstrip_dir: _,
            // Follows the input stack: an expansion is counted while its
            // tokens are on it.
            expanding: _,
            // Compared.
            input: _,
            prefixes,
            long_argument,
            runaway,
            cds,
            conds,
            testing,
            testing_kind: _,
            mode: _,
            list: _,
            material: _,
            natural: _,
            docstrip,
            pinned,
            within,
            source: _,
            packages,
            file_depth,
            branch_depth,
            split_at,
            // Only while paths run, which a load is never captured in.
            heads: _,
            abs: _,
            abs_from: _,
            abs_whole: _,
            narrowing: _,
            halted,
            budget,
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
            key_names,
            held,
            command_level: _,
            // The cache's own and the run's bookkeeping.
            package_capture: _,
            // Only while an `\edef` body is scanned.
            copy_target: _,
            copies: _,
            document_file_depth: _,
            popping_in_read: _,
            reading_command: _,
            trace_full: _,
            started: _,
            step_depth: _,
            stack_base: _,
            work: _,
            scanner: _,
            pseudo_file: _,
            probe: _,
            // Bookkeeping of the output, not state the run depends on.
            diag_seen: _,
            diag_floor: _,
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
            branch_depth: *branch_depth,
            split_at: split_at.len(),
            packages: packages.len(),
            file_depth: *file_depth,
            aux: (line_defined.is_some(), written_lines.len() + name_reads.len() + stream_files.len() + key_names.len()),
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

    /// The mouth for a package file about to be pushed: one standing where
    /// the cache leaves off, after installing every segment that fits.
    pub(super) fn open_package(&mut self, source: &str, id: FileId, name: &str, kind: LoadKind, span: Span) -> Mouth {
        if !self.cacheable_at_push(kind) {
            return Mouth::file(source, id);
        }
        let Some(dir) = self.cache_dir() else { return Mouth::file(source, id) };
        let path = self.out.files[id as usize].path.clone();
        let mut key = Fnv::new();
        key.put(&(INTERPRETER, name, kind, &path));
        let distribution = &self.out.distribution;
        key.put(&(&distribution.roots, distribution.indexed, distribution.year));
        let cache = cache_file(&dir, name, key.0);
        let started = std::time::Instant::now();
        let root = self.root_state(span, &path);
        let before = self.before_state();
        let rest = self.rest_state(self.input.len() + 1);
        let base = self.base.clone();
        let mut tree_index = None;
        let mut at: Option<u32> = None;
        let mut end: Option<(LineMark, bool)> = None;
        let mut served = 0;
        let mut reason: Option<String> = None;
        let mut written: Vec<(Sym, u8)> = Vec::new();
        let mut origins: Vec<((RegKind, u16), u8)> = Vec::new();
        let mut carried: Vec<Sym> = Vec::new();
        let replaying = !self.cfg.rebuild_format && !self.cfg.cache_index.refresh;
        match replaying.then(|| read(&cache)) {
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
                    reason = Some(format!("{differs} differs"));
                }
                Some(t) => {
                    tree_index = Some(t);
                    let tree = &file.trees[t];
                    let mut children: HashMap<Option<u32>, Vec<u32>> = HashMap::new();
                    for (i, node) in tree.nodes.iter().enumerate() {
                        children.entry(node.parent).or_default().push(i as u32);
                    }
                    while let Some(kids) = children.get(&at) {
                        let mut chosen = None;
                        for &k in kids {
                            match self.segment_fits(&tree.nodes[k as usize], &base) {
                                Ok(()) => {
                                    chosen = Some(k);
                                    break;
                                }
                                Err(why) => {
                                    reason.get_or_insert(why);
                                }
                            }
                        }
                        let Some(k) = chosen else { break };
                        reason = None;
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
                }
            },
        }
        if served > 0 {
            self.out.timings.credit_tokens(id, served);
            self.out.cached_files.insert(id);
        }
        let cache_text = cache.display().to_string();
        let mouth = match end {
            Some((mark, _)) => Mouth::resumed(source, id, mark),
            None => Mouth::file(source, id),
        };
        if end.is_some_and(|(_, last)| last) {
            if self.cfg.verbose >= 1 {
                eprintln!("satex: {name} from its cache in {:?}", started.elapsed());
            }
            self.out.package_caches.push((name.to_string(), cache_text, "cached".into()));
            return mouth;
        }
        if let Some(why) = &reason {
            if self.cfg.verbose >= 1 {
                let line = end.map_or(1, |(mark, _)| mark.line);
                eprintln!("satex: {name}: {served} tokens from its cache; {why}, so it is read on from line {line}");
            }
            self.out.package_caches.push((name.to_string(), cache_text.clone(), format!("stale: {why}")));
        }
        if !self.cfg.cache_index.auto && !self.cfg.cache_index.building {
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
            tree: tree_index,
            parent: at,
            before,
            rest,
            marks,
            last_line: end.map_or(0, |(mark, _)| mark.line),
            chain: Vec::new(),
            lists: None,
            lookups,
            seg_gaps: self.gaps.clone(),
            seg_vertices: 0,
            seg_call: None,
            seg_call_finished: false,
            seg_start: HashMap::new(),
            seg_copies: HashMap::new(),
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

    /// Whether a segment's files are unchanged and this run agrees with
    /// what it read.
    fn segment_fits(&self, node: &Node, base: &Path) -> Result<(), String> {
        for (path, size, modified) in &node.stamps {
            if crate::format::stamp(Path::new(path)) != Some((*size, *modified)) {
                return Err(format!("{path} changed"));
            }
        }
        for (looked, kind, found) in &node.lookups {
            if self.resolver.search_again(looked, *kind, base) != *found {
                return Err(format!("{looked} now resolves elsewhere"));
            }
        }
        // Decoded with the segment's own name numbers: the names it read may
        // not exist here, and are looked up, never interned.
        let local: Vec<Sym> = (0..node.names.len() as u32).map(Sym).collect();
        let files = self.lookup_files(&node.paths);
        let (decoded, _) = codec::with(Mode::Decode { syms: local, files }, || postcard::from_bytes::<Reads>(&node.reads));
        let reads = decoded.map_err(|e| format!("cannot decode it: {e}"))?;
        let here = |sym: Sym| node.names.get(sym.0 as usize).and_then(|n| self.out.interner.lookup(n));
        for (code, sym) in &reads.gaps {
            let sym = here(*sym);
            if !self.gaps.iter().any(|(c, s)| c == code && Some(*s) == sym) {
                return Err("a gap it relied on being reported is not".into());
            }
        }
        let (mismatch, _) = codec::with(hash_mode(self), || {
            for (sym, kinds, digest) in &reads.bindings {
                let sym = here(*sym);
                let binding = sym.and_then(|sym| self.env.get(sym));
                let alias = sym.and_then(|sym| self.env.aliases.get(&sym)).map(|a| self.out.interner.name(*a).to_string());
                if binding_digest(binding, alias.as_deref(), *kinds) != *digest {
                    let name = sym.map_or_else(|| "a name".to_string(), |sym| self.out.interner.cs(sym));
                    return Some(format!("{name} differs"));
                }
            }
            None
        });
        if let Some(reason) = mismatch {
            return Err(reason);
        }
        // A copied text may be longer here, as far as the limits its copies
        // met allow.
        for (sym, len, room) in &reads.copies {
            let Some(sym) = here(*sym) else { return Err("a text it copied is gone".into()) };
            let Some(text) = self.env.slot(sym).and_then(|b| crate::env::copied_text(&b.meaning)) else {
                return Err("a text it copied is gone".into());
            };
            if text.len() as i64 - i64::from(*len) > *room {
                return Err(format!("{} is too long to copy", self.out.interner.cs(sym)));
            }
        }
        Ok(())
    }

    /// This run's numbers for a segment's files, without registering any.
    fn lookup_files(&self, paths: &[String]) -> Vec<FileId> {
        paths
            .iter()
            .map(|p| match p.as_str() {
                SITE => LOAD_SITE,
                MAIN => self.out.main_file,
                path => self.out.files.iter().position(|f| f.path == path).map_or(UNREAD, |i| i as FileId),
            })
            .collect()
    }

    /// The storage symbol of `\\count⟨n⟩`, without interning it.
    fn register_sym_peek(&self, n: i64) -> Option<Sym> {
        self.out.interner.lookup(&format!("{}{}{n}", super::REGISTER, RegKind::Count.as_str()))
    }
}

impl Machine<'_> {
    /// Install one segment: the names, files, vertices, edges, bindings,
    /// facts and state it recorded.  Returns the names it wrote and which
    /// parts of each, or `None`, having changed nothing, when the segment
    /// cannot be decoded.
    fn apply_segment(
        &mut self,
        node: &Node,
        site: Span,
        before: &Before,
        origins: &mut Vec<((RegKind, u16), u8)>,
        carried: &mut Vec<Sym>,
    ) -> Option<Vec<(Sym, u8)>> {
        // Read once with the segment's own numbers, for its allocations and
        // the order it asked for names in.
        let local: Vec<Sym> = (0..node.names.len() as u32).map(Sym).collect();
        let (provisional, _) = codec::with(Mode::Decode { syms: local, files: self.lookup_files(&node.paths) }, || {
            postcard::from_bytes::<Effect>(&node.effect)
        });
        let provisional = provisional.ok()?;
        let mut shift: HashMap<u8, i64> = HashMap::new();
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
        for (path, kind, _) in &provisional.files {
            self.register_file(path.clone(), *kind);
        }
        // The segment's own files were registered above, in the order it
        // read them; a file it only refers to that this run has not read
        // stays unread.
        let files = self.lookup_files(&node.paths);
        let (decoded, _) = codec::with(Mode::Decode { syms, files }, || postcard::from_bytes::<Effect>(&node.effect));
        let st = decoded.ok()?;
        // The texts the segment copied through, as this run has them where
        // the segment begins.
        let mut origs: HashMap<Sym, Rc<[Token]>> = HashMap::new();
        let spliced = st.bindings.iter().filter(|(_, b, _)| b.as_ref().is_some_and(|b| b.splice.is_some())).map(|(sym, ..)| *sym);
        for sym in spliced.chain(st.definitions.iter().filter(|d| d.splice.is_some()).map(|d| d.name)) {
            let text = self.env.slot(sym).and_then(|b| crate::env::copied_text(&b.meaning)).cloned()?;
            origs.insert(sym, text);
        }
        origins.extend(st.origins.iter().copied());
        for (path, _, entry) in &st.files {
            if let Some(id) = self.out.files.iter().position(|f| f.path == *path)
                && let Some(slot) = self.out.entry.get_mut(id)
            {
                *slot = entry.map(|s| span_in(s, site));
            }
        }

        // The call the segment completed, as far as this run has read it.
        let pending = self.call_shape.as_ref().map(|(sym, span, ..)| (*sym, *span));
        if st.scalars.finished_call {
            self.finish_shape();
        }
        // Vertices, in the order the segment first reached them.
        let mut ids: Vec<Option<NodeId>> = vec![None; st.vertices.len()];
        let reached: HashSet<u32> = st.reached.iter().map(|(i, _)| *i).collect();
        for (i, reader) in &st.reached {
            let reader = reader.and_then(|r| self.resolve_vertex(&st, &mut ids, &reached, before, site, r, 0).first().copied());
            let charged = std::mem::replace(&mut self.out.graph.reader, reader);
            self.resolve_vertex(&st, &mut ids, &reached, before, site, VRef::Table(*i), 0);
            self.out.graph.reader = charged;
        }
        for (from, to, kind, reader) in &st.edges {
            let froms = self.resolve_vertex(&st, &mut ids, &reached, before, site, *from, 0);
            let tos = self.resolve_vertex(&st, &mut ids, &reached, before, site, *to, 0);
            let kind = EdgeKind(*kind);
            // A definition's maker, in the order the edges were made.
            if kind == EdgeKind::default() {
                for d in &froms {
                    if let Some(c) = tos.first() {
                        self.out.graph.made_by(*d, *c);
                    }
                }
                continue;
            }
            if kind.intersects(EdgeKind::READS) {
                let reader = reader.and_then(|r| self.resolve_vertex(&st, &mut ids, &reached, before, site, r, 0).first().copied());
                let charged = std::mem::replace(&mut self.out.graph.reader, reader);
                for f in &froms {
                    for t in &tos {
                        self.out.graph.edge(*f, *t, kind);
                    }
                }
                self.out.graph.reader = charged;
            } else {
                for f in &froms {
                    for t in &tos {
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

        // Bindings, with the registers the package allocated renumbered.
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

        // Facts, in the order the segment made them.
        for d in &st.definitions {
            let mac = match (d.splice, &d.mac, origs.get(&d.name)) {
                (Some(at), Some(mac), Some(orig)) => {
                    let mut mac = (**mac).clone();
                    mac.replacement_text = splice_in(&mac.replacement_text, at as usize, orig)?;
                    Some(Rc::new(mac))
                }
                _ => d.mac.clone(),
            };
            let Some(node_id) = self.resolve_vertex(&st, &mut ids, &reached, before, site, d.node, 0).first().copied() else {
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
                    before.slots.get(d.name.0 as usize).and_then(|b| b.as_ref()).is_some_and(|b| b.meaning != Meaning::Undefined)
                } else {
                    d.redefines
                },
                certain: d.certain,
                cds,
                // A package load is outside any environment; not persisted
                // in the cache, so a replay reports it that way too.
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
            self.expansion_sites.insert(key, Site { fact, repeats: e.repeats, progress: 0 });
            sites.push((key, fact, e.repeats, e.current));
        }
        let base_loads = before.loads as i64;
        for l in &st.loads {
            let file = l.path.as_ref().and_then(|p| self.out.files.iter().position(|f| f.path == *p)).map(|i| i as FileId);
            self.out.facts.loads.push(Load {
                name: l.name.clone(),
                kind: l.kind,
                options: l.options.clone(),
                span: span_in(l.span, site),
                by: l.by,
                file,
                path: l.path.clone(),
                status: STATUSES[l.status as usize],
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
        for o in &st.occurrences {
            let Some(node_id) = self.resolve_vertex(&st, &mut ids, &reached, before, site, o.node, 0).first().copied() else {
                continue;
            };
            self.out.facts.occurrences.push(Occurrence {
                kind: o.kind,
                key: relocate_text(&o.key, &relocate),
                detail: o.detail.as_deref().map(|d| relocate_text(d, &relocate)),
                span: span_in(o.span, site),
                package: o.package,
                node: node_id,
                section: o.section.as_deref().map(Rc::from),
                certain: o.certain && self.exact(),
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
            self.out.facts.diagnostics.push(Diagnostic {
                severity: SEVERITIES[*severity as usize],
                code: leak(code),
                message: relocate_text(message, &relocate),
                span: span_in(*span, site),
                count: *count,
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

        // The state the checkpoint left.
        if let Some(lists) = &st.lists {
            let mut catcodes = CatcodeTable::latex();
            catcodes.apply(&lists.catcodes);
            self.catcodes = catcodes;
            self.end_line = EndLineChar(lists.end_line);
            self.loaded = lists.loaded.clone();
            self.class_options = lists.class_options.clone();
            self.wild = lists.wild.clone();
            self.verbatim_environments = lists.verbatim.iter().cloned().collect();
            self.section = lists.section.as_deref().map(Rc::from);
            self.lua.set_files(lists.lua.clone());
            self.counter_namespace = lists.counter_namespace.clone();
            self.streams = Rc::new(
                lists.streams.iter().map(|(n, lines, at)| (*n, (Rc::new(lines.clone()), *at))).collect(),
            );
            self.env.aliases = lists.aliases.iter().copied().collect();
        }
        let s = &st.scalars;
        self.after_assignment = s.after_assignment;
        self.last_file = s.last_file.map(|span| span.file);
        self.file_call = s.file_call.map(|(sym, span)| (sym, span_in(span, site)));
        self.identifying = s.identifying.and_then(|i| usize::try_from(base_loads + i).ok());
        // A call pending at the segment's start that it did not complete is
        // still this run's own.
        if s.call_shape.is_some() || st.scalars.finished_call || pending.is_none() {
            self.call_shape = s.call_shape.clone().map(|(sym, span, shape, depth)| (sym, span_in(span, site), shape, depth));
        }
        self.last_named_cs = s.last_named_cs;
        let mut recent = before.recent_file_spans.clone();
        recent.extend(s.recent_file_spans.iter().map(|span| span_in(*span, site)));
        let excess = recent.len().saturating_sub(RECENT_FILE_SPANS);
        recent.drain(..excess);
        self.recent_file_spans = recent;
        self.out.steps = before.steps + s.steps;
        self.source_progress = before.progress + s.progress;
        self.env.assignments = before.assignments + s.assignments;
        self.progress_step = self.out.steps.saturating_sub(s.stalled);
        self.env.set_recent(s.recent.clone());
        let now = self.site_progress();
        for (key, fact, repeats, current) in sites {
            let progress = if current { now } else { !now };
            self.expansion_sites.insert(key, Site { fact, repeats, progress });
        }
        Some(written)
    }

    /// The vertices a reference stands for here: the table entry, found by
    /// its site or made when the segment reached it; or the definitions the
    /// name had before the load.
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
                return before.slots.get(sym.0 as usize).and_then(|b| b.as_ref()).map(|b| b.defs.clone()).unwrap_or_default();
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
        // A vertex the segment only pointed at, which this run never made,
        // is not one reading the package would make either.
        if !reached.contains(&(i as u32)) || depth > st.vertices.len() {
            return Vec::new();
        }
        let within = v.within.and_then(|w| self.resolve_vertex(st, ids, reached, before, site, w, depth + 1).first().copied());
        let mut cds = Vec::new();
        for (on, taken) in &v.cds {
            for on in self.resolve_vertex(st, ids, reached, before, site, *on, depth + 1) {
                cds.push(ControlDep { on, taken: *taken });
            }
        }
        let fresh = self.out.graph.len() as NodeId;
        let id = self.out.graph.push_vertex(Vertex { tag, name: v.name, key: v.key, span, cds: cds.clone(), within });
        // A vertex the recording run already had came without the edges a
        // new one gets; a run making it here adds them, as reading would.
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

/// How many given-back file tokens the machine remembers.
const RECENT_FILE_SPANS: usize = 64;

/// How many tokens a segment holds at least before a checkpoint ends it,
/// unless the file ends first.  A load that diverges from its cache is read
/// on from the checkpoint before the divergence, so this bounds what a
/// divergence costs; fewer checkpoints keep the cache small.  Measured on
/// TikZ (8·10⁶ tokens) and amsmath (8·10⁴): a few hundred segments for the
/// one, a handful for the other.
const SEGMENT_TOKENS: u64 = 20_000;

impl Machine<'_> {
    /// Begin a segment: every log counts from here, and what the segment
    /// reads of the load's own state is compared with this.
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
        let seg_call = self.call_shape.as_ref().map(|(sym, span, ..)| (*sym, *span));
        let _ = self.env.take_read_marks();
        let _ = self.env.take_dirty();
        self.env.begin_segment();
        let seg_start = self.env.carrying().into_iter().map(|sym| (sym, self.env.slot(sym).cloned())).collect();
        let Some(rec) = self.package_capture.as_mut() else { return };
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

    /// Called where the main loop is about to read a token: a checkpoint
    /// when the package's own file is being read at the start of a line and
    /// the interpreter is otherwise as it was when the file was pushed.
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

    /// Whether defining `name` redefines it, and whether that was decided by
    /// the state before the load: then it is no read, and a replay decides
    /// it again from the state it is replayed in.
    pub(super) fn redefines(&self, name: Sym) -> (bool, bool) {
        if self.package_capture.is_some() && !self.env.meaning_written(name) {
            let defined = self.env.slot(name).is_some_and(|b| b.meaning != Meaning::Undefined);
            return (defined, true);
        }
        (self.env.is_defined(name), false)
    }

    /// The texts copied through into `meaning`, the new text of `name`
    /// ([`Machine::copy_through`]): where the text `name` held when the
    /// segment began stands in it, when a replay can make the definition
    /// again from what the name holds there.  Anything else reads the texts
    /// as a whole.
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
        let splice = (kept && global && copy.fresh && !self.env.forking() && self.package_capture.is_some())
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

    /// `name` was defined with the text it held when the segment began at
    /// `at`, `len` long.
    pub(crate) fn note_copied(&mut self, name: Sym, at: usize, len: usize) {
        self.env.set_splice(name, at, len);
        let Some(rec) = self.package_capture.as_mut() else { return };
        let last = rec.log.definitions.len().wrapping_sub(1);
        if rec.log.definitions.last().is_some_and(|(d, _)| d.name == name) {
            rec.log.def_splices.insert(last, (at, len));
        }
    }

    /// A call's shape was completed: when it is the one pending at the
    /// segment's start, the segment's replay completes it too.
    pub(crate) fn note_call_finished(&mut self, sym: Sym, span: Span) {
        if let Some(rec) = self.package_capture.as_mut()
            && rec.seg_call == Some((sym, span))
        {
            rec.seg_call_finished = true;
        }
    }

    /// `from` reads whatever defines `name`: edges to its definitions.  While
    /// a package is recorded and `name` still has the definitions it had
    /// before the load, the read is recorded as such, and a replay links it
    /// to what defines `name` there.
    pub(crate) fn link_reads(&mut self, from: NodeId, name: Sym, defs: &[NodeId]) {
        self.link_edges(from, name, defs, EdgeKind::READS);
    }

    /// `from` is a macro body that mentions `name`: it depends on what
    /// defines it, but making the body reads nothing, so no call is charged.
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

    /// The main loop ran off the end of the package's file while reading
    /// the next command: the last checkpoint, if the interpreter is at rest.
    /// The read has already counted the token it goes on to return from the
    /// frame below, which a replay's own read counts again.
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

    /// A call site is about to be expanded: note it, with the calls counted
    /// there so far, the first time the segment reaches it.
    pub(super) fn note_site(&mut self, span: Span, name: Sym) {
        let mode = self.mode;
        let Some(rec) = self.package_capture.as_mut() else { return };
        let modes = rec.log.site_modes.entry((span, name)).or_default();
        *modes = modes.union(mode);
        if !rec.log.sites_seen.insert((span, name)) {
            return;
        }
        let count = self.expansion_sites.get(&(span, name)).map_or(0, |site| self.out.facts.expansions[site.fact].count);
        rec.log.sites.push(((span, name), count));
    }

    /// A file is read while a package is recorded: its contents are part of
    /// what the segment depends on.
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

    /// A register is being given a name: remember which allocation counter
    /// handed it out, when one the load advanced holds its number.
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
        // Two counters holding the number at once: the one that hands out
        // this kind of register first is the one that did.
        if holders.len() > 1
            && let Some(primary) = holders.iter().copied().find(|c| allocates(*c).first() == Some(&kind))
        {
            holders = vec![primary];
        }
        if let &[counter] = &holders[..] {
            if let Some(rec) = self.package_capture.as_mut() {
                rec.log.origins.push(((kind, index), counter));
            }
            // A register the load allocated holds nothing from before it,
            // whatever a run that numbers it differently has there.
            let storage = self.register_sym(kind, i64::from(index));
            self.env.mark_written(storage, u8::MAX);
        }
    }

    /// `\csname` formed a name at `span`: the first role recorded for a
    /// site stands.
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

    /// The package's file has been read to its end, or recording must stop:
    /// store what was recorded and stop recording.
    pub(super) fn finish_recording(&mut self) {
        let Some(rec) = self.package_capture.take() else { return };
        self.env.end_tracking();
        self.out.interner.record_touches(false);
        self.out.graph.record_touches(false);
        self.out.graph.record_log(false);
        self.out.calls.record(false);
        self.out.observed.record(false);
        if let Some(why) = rec.log.spoiled
            && self.cfg.verbose >= 1
        {
            eprintln!("satex: {} is recorded only up to line {}: {why}", rec.name, rec.last_line);
        }
        let total = self.out.steps - rec.before.steps;
        if rec.chain.is_empty() {
            if rec.served == 0 {
                let why = rec.log.spoiled.unwrap_or("no line of it was at rest");
                self.out.package_caches.push((rec.name.clone(), rec.cache.display().to_string(), format!("not stored: {why}")));
            }
            return;
        }
        let Some(dir) = rec.cache.parent() else { return };
        if std::fs::create_dir_all(dir).is_err() {
            return;
        }
        let mut file = read(&rec.cache).unwrap_or(CacheFile { version: ENCODING_VERSION, trees: Vec::new() });
        let tree = match rec.tree.filter(|t| *t < file.trees.len()) {
            Some(t) => t,
            None => {
                file.trees.insert(0, Tree { root: rec.root.clone(), nodes: Vec::new() });
                file.trees.truncate(MAX_TREES);
                0
            }
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
        let Ok(bytes) = postcard::to_allocvec(&file) else { return };
        let temporary = crate::format::temporary_for(&rec.cache);
        if std::fs::write(&temporary, bytes).is_ok() && std::fs::rename(&temporary, &rec.cache).is_ok() {
            crate::format::prune_family(dir, &family(&rec.name), INTERPRETER);
            prune(dir, self.cfg.limits.cache_size.as_u64());
            let recorded = total.saturating_sub(rec.served);
            let _ = recorded;
            let cache_text = rec.cache.display().to_string();
            let state = match rec.served {
                0 => "stored".to_string(),
                served => {
                    // The replay's entry says why it stopped: one entry says both.
                    let why = match self.out.package_caches.last() {
                        Some((name, cache, why)) if *name == rec.name && *cache == cache_text && why.starts_with("stale: ") => {
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

    /// The segment from the last checkpoint to `mark`, written against its
    /// own tables.
    fn encode_segment(&mut self, mark: LineMark) -> Option<Node> {
        let mut reads = self.env.take_read_marks();
        let dirty = self.env.take_dirty();
        let rec = self.package_capture.as_ref()?;
        // What a name holding a copied text held when the segment began, and
        // the names whose text is stored whole although it holds one, which
        // the segment then reads whole.
        let seg_binding = |sym: Sym| -> Option<&Binding> {
            match rec.seg_start.get(&sym) {
                Some(b) => b.as_ref(),
                None => rec.before.slots.get(sym.0 as usize).and_then(|b| b.as_ref()),
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

        // The vertex table.
        let first_new = rec.seg_vertices;
        let table: std::cell::RefCell<(HashMap<NodeId, u32>, Vec<NodeId>)> = Default::default();
        // A vertex the load reached stands for its site; one it only points
        // at that a name defined before the load stands for that name's
        // definitions.
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
        // A read of a definition made before the load is replayed as a read,
        // which charges the call that made the definition where it is
        // replayed; the charge the recording made is left out.
        let logged = &glog.edges[m.edges..];
        let mut edges: Vec<(VRef, VRef, u16, Option<VRef>)> = Vec::with_capacity(logged.len());
        let mut links = glog.links[m.links..].iter().peekable();
        for (i, (from, to, kind, reader)) in logged.iter().enumerate() {
            while let Some((_, lfrom, name, lreader, lkind)) = links.next_if(|(at, ..)| *at <= m.edges + i) {
                edges.push((vref(*lfrom), VRef::Before(*name), lkind.0, lreader.map(vref)));
            }
            let to_ref = vref(*to);
            // The charge a read makes to the call that made what it reads is
            // made again when the read is replayed, against the maker there.
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
        let extents: Vec<(Span, u32)> = glog.extents[m.extents..].iter().map(|(s, l)| (span_out(*s, site), *l)).collect();
        let calls = self.out.calls.recorded()[m.calls..].to_vec();
        let observed = self.out.observed.recorded()[m.observed..].to_vec();

        // Bindings: every part the load has written, as it stands now.
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
                    // What the name was defined by before the load stands for
                    // what defines it where the load is replayed.
                    defs: {
                        let mut defs: Vec<VRef> = b
                            .defs
                            .iter()
                            .map(|d| match own {
                                Some(own) if (*d as usize) < before.vertices && own.contains(d) && !reached_all.contains(d) => {
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

        // Facts.
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
        let occurrences: Vec<CachedOccurrence> = self.out.facts.occurrences[m.occurrences.min(self.out.facts.occurrences.len())..]
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
            .map(|l| CachedLoad {
                name: l.name.clone(),
                kind: l.kind,
                options: l.options.clone(),
                span: span_out(l.span, site),
                by: l.by,
                path: l.path.clone(),
                status: index_of(&STATUSES, &l.status),
                required: l.required.clone(),
                provided: l.provided.clone(),
                depth: l.depth,
            })
            .collect();
        let identifications = rec.log.identifications[m.identifications..]
            .iter()
            .map(|(load, provided)| (*load as i64 - before.loads as i64, provided.clone()))
            .collect();
        let lists = Lists {
            catcodes: self.catcodes.diff(&CatcodeTable::latex()),
            end_line: self.end_line.0,
            loaded: self.loaded.clone(),
            class_options: self.class_options.clone(),
            wild: self.wild.clone(),
            verbatim: {
                let mut v: Vec<_> = self.verbatim_environments.iter().cloned().collect();
                v.sort();
                v
            },
            section: self.section.as_deref().map(str::to_string),
            lua: self.lua.files(),
            counter_namespace: self.counter_namespace.clone(),
            streams: {
                let mut s: Vec<_> =
                    self.streams.iter().map(|(n, (lines, at))| (*n, lines.as_ref().clone(), *at)).collect();
                s.sort_by_key(|(n, ..)| *n);
                s
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
            stalled: self.out.steps.saturating_sub(self.progress_step),
            recent: self.env.recent(),
            after_assignment: self.after_assignment,
            last_file: self.last_file.map(|f| Span::new(f, 0, 0)),
            file_call: self.file_call.map(|(sym, span)| (sym, span_out(span, site))),
            identifying,
            call_shape: self
                .call_shape
                .clone()
                .filter(|(sym, span, ..)| rec.seg_call != Some((*sym, *span)))
                .map(|(sym, span, shape, depth)| (sym, span_out(span, site), shape, depth)),
            finished_call: rec.seg_call_finished,
            last_named_cs: self.last_named_cs,
            recent_file_spans: {
                let before = &rec.before.recent_file_spans;
                let kept = (0..=before.len())
                    .find(|e| self.recent_file_spans.starts_with(&before[*e..]))
                    .map_or(0, |e| before.len() - e);
                self.recent_file_spans[kept.min(self.recent_file_spans.len())..].iter().map(|s| span_out(*s, site)).collect()
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
                .map(|(i, f)| (f.path.clone(), f.kind, self.out.entry.get(m.files + i).copied().flatten().map(|s| span_out(s, site))))
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
            diagnostics,
            metadata: self.out.metadata[m.metadata..].iter().map(|x| (x.field.to_string(), x.text.clone(), x.source.clone())).collect(),
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
        };

        // The reads, against the state the segment began in.
        // An allocation counter the load advances is no dependency: a replay
        // allocates from this run's counters instead, and links what reads
        // them to what set them there.
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
                    (*sym, *bits, binding_digest(before_binding, alias.as_deref(), *bits))
                })
                .collect::<Vec<(Sym, u8, u64)>>()
        });
        let gaps = rec.log.gaps_seen[m.gaps_seen..]
            .iter()
            .filter(|g| rec.seg_gaps.contains(g))
            .map(|(code, sym)| (code.to_string(), *sym))
            .collect();
        let reads = Reads {
            copies,
            bindings: read_bindings,
            gaps,
        };

        // Files: every file opened in the segment and the new ones, and the
        // names looked up for the first time.
        let mut stamps = Vec::new();
        let mut seen = HashSet::new();
        let opened = rec.log.opened[m.opened..].iter().cloned();
        let new_files = self.out.files[m.files..].iter().map(|f| f.path.clone());
        for path in opened.chain(new_files) {
            if path.starts_with('<') || !seen.insert(path.clone()) {
                continue;
            }
            let (size, modified) = crate::format::stamp(Path::new(&path))?;
            stamps.push((path, size, modified));
        }
        let lookups: Vec<(String, LoadKind, Option<PathBuf>)> = self
            .resolver
            .lookups()
            .filter(|(key, _)| !rec.lookups.contains(*key))
            .map(|((name, kind), found)| (name.clone(), *kind, found.clone()))
            .collect();
        let steps = self.out.steps - m.steps;

        let encode = || Mode::Encode { syms: HashMap::new(), sym_order: Vec::new(), files: HashMap::new(), file_order: Vec::new() };
        let ((reads, effect), mode) = codec::with(encode(), || (postcard::to_allocvec(&reads), postcard::to_allocvec(&effect)));
        let Mode::Encode { sym_order, file_order, .. } = mode else { return None };
        let names = sym_order.iter().map(|i| self.out.interner.name(Sym(*i)).to_string()).collect();
        let paths = file_order
            .iter()
            .map(|f| match *f {
                LOAD_SITE => SITE.to_string(),
                f if f == main => MAIN.to_string(),
                f => self.out.files.get(f as usize).map_or_else(String::new, |i| i.path.clone()),
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
            reads: reads.ok()?,
            effect: effect.ok()?,
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


/// The counter that handed out a register: the one recorded for it, or, for
/// the slots of an insert, the one its name came from.
fn origin_of(origins: &[((RegKind, u16), u8)], kind: RegKind, index: i64) -> Option<u8> {
    origins
        .iter()
        .find(|((k, i), _)| *k == kind && i64::from(*i) == index)
        .or_else(|| origins.iter().find(|((_, i), c)| i64::from(*i) == index && allocates(*c).contains(&kind)))
        .map(|(_, c)| *c)
}

/// The trees cached for a package, or why they cannot be used: `None` when
/// there are none.
fn read(file: &Path) -> Result<CacheFile, Option<String>> {
    let bytes = std::fs::read(file).map_err(|_| None)?;
    let cached: CacheFile = postcard::from_bytes(&bytes).map_err(|e| Some(format!("cannot decode it: {e}")))?;
    if cached.version != ENCODING_VERSION {
        return Err(Some("written in another encoding".into()));
    }
    crate::format::touch(file);
    Ok(cached)
}

fn same_tokens(a: &[Token], b: &[Token]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.tok == y.tok && x.span == y.span)
}

/// `text` with `len` tokens at `at` taken out, when they are `orig`.
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
