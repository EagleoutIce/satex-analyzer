//! The dependency graph: vertices, edges, data and control dependencies.

use crate::fixed::{HashMap, LookupMap};

use serde::Serialize;

use crate::engine::env::NodeId;
use crate::engine::tex::{Interner, Span, Sym};

named_enum! {
    /// What kind of program point a [`Vertex`] stands for: a value, a use, a
    /// macro call, or the definition of a variable or macro.
    #[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, serde::Deserialize)]
    #[serde(rename_all = "kebab-case")]
    pub enum VertexTag {
        Value => "value",
        Use => "use",
        MacroCall => "macro-call",
        VariableDefinition => "variable-definition",
        MacroDefinition => "macro-definition",
    }
}

/// How one [`Vertex`] depends on another, as a bit set so that parallel edges
/// between the same two vertices collapse into one. [`DependencyGraph::slice_until`]
/// filters a walk by which kinds it follows.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Serialize, serde::Deserialize)]
pub struct EdgeKind(pub u16);

impl EdgeKind {
    pub const READS: EdgeKind = EdgeKind(1 << 0);
    pub const DEFINED_BY: EdgeKind = EdgeKind(1 << 1);
    pub const EXPANDS: EdgeKind = EdgeKind(1 << 2);
    pub const ARGUMENT: EdgeKind = EdgeKind(1 << 3);
    pub const SIDE_EFFECT_ON_CALL: EdgeKind = EdgeKind(1 << 4);
    pub const CONTROL: EdgeKind = EdgeKind(1 << 5);
    /// Given to [`DependencyGraph::edge`] with `READS`: a name a macro body
    /// mentions, which it depends on without reading it when it is made,
    /// so no call that made the name is charged.  Never stored.
    pub const MENTION: EdgeKind = EdgeKind(1 << 15);

    const NAMES: [(EdgeKind, &'static str); 6] = [
        (EdgeKind::READS, "reads"),
        (EdgeKind::DEFINED_BY, "defined-by"),
        (EdgeKind::EXPANDS, "expands"),
        (EdgeKind::ARGUMENT, "argument"),
        (EdgeKind::SIDE_EFFECT_ON_CALL, "side-effect-on-call"),
        (EdgeKind::CONTROL, "control-dependency"),
    ];

    #[inline]
    pub fn intersects(self, other: EdgeKind) -> bool {
        self.0 & other.0 != 0
    }

    pub fn names(self) -> Vec<&'static str> {
        Self::NAMES.iter().filter(|(k, _)| self.intersects(*k)).map(|(_, n)| *n).collect()
    }

    pub fn parse(name: &str) -> Option<EdgeKind> {
        Self::NAMES.iter().find(|(_, n)| *n == name).map(|(k, _)| *k)
    }
}

impl std::ops::BitOr for EdgeKind {
    type Output = EdgeKind;
    fn bitor(self, rhs: EdgeKind) -> EdgeKind {
        EdgeKind(self.0 | rhs.0)
    }
}

impl std::ops::BitOrAssign for EdgeKind {
    fn bitor_assign(&mut self, rhs: EdgeKind) {
        self.0 |= rhs.0;
    }
}

/// One branch a [`Vertex`] depends on: the vertex `on` that decided it, and
/// whether this path was the one `taken`. Recorded when a conditional is
/// undecided and every arm is analyzed in turn.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, serde::Deserialize)]
pub struct ControlDep {
    pub on: NodeId,
    pub taken: bool,
}

/// One node of the [`DependencyGraph`]: a control sequence or key, tagged with
/// a [`VertexTag`], at the [`Span`] it was seen.
#[derive(PartialEq, Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Vertex {
    pub tag: VertexTag,
    pub name: Sym,
    pub key: bool,
    pub span: Span,
    pub cds: Vec<ControlDep>,
    /// Enclosing macro definition, if the vertex was created while expanding
    /// one; the analogue of flowR's `subflow`.
    pub within: Option<NodeId>,
}

/// The program's dependency graph: [`Vertex`]es for definitions, calls and
/// uses, connected by [`EdgeKind`] edges. [`DependencyGraph::slice_until`] is what
/// `satex slice` walks.
#[derive(PartialEq, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct DependencyGraph {
    /// Shared in chunks by checkpoints and forks, stored as a plain list.
    #[serde(with = "crate::fixed::flat")]
    pub vertices: crate::fixed::Chunked<Vertex>,
    /// `limits.vertices` (0: none): past it a new site shares one overflow vertex, so
    /// the graph stops growing while the analysis goes on.
    pub cap: usize,
    overflow: Option<NodeId>,
    /// Outgoing edges, indexed by source vertex.
    #[serde(with = "crate::fixed::flat")]
    out: crate::fixed::Chunked<Vec<(NodeId, EdgeKind)>>,
    /// Where each target sits in `out`, for the vertices with many outgoing
    /// edges: a call that runs a long loop reaches every vertex the loop
    /// makes, and finding an edge by scanning would make it quadratic.
    /// Shared with copies, each index copied once written.
    wide: crate::fixed::SharedMap<NodeId, std::rc::Rc<LookupMap<NodeId, u32>>>,
    /// One vertex per source site.  A macro body that is read several times —
    /// because a package is analyzed under both arms of an undecided
    /// conditional, or because a loop is unrolled — describes the same
    /// program point every time.
    sites: crate::fixed::HotMap<(Span, Sym, VertexTag), NodeId>,
    /// The last line a construct that began at a span reached, for the ones
    /// whose arguments ran past their own line.  Only spans that reach
    /// further than their own line are in here.
    extents: HashMap<Span, u32>,
    /// The call read from a file whose expansion last ran a definition site.
    /// A definition vertex stands for its site in every expansion that runs
    /// it, so what reads the definition reads the call that made it now.
    made_by: LookupMap<NodeId, NodeId>,
    /// How many times each name has been defined so far, counting every run of a
    /// definition site.
    defs_of: LookupMap<Sym, u32>,
    /// The call read from a file whose expansion is running, if any: what
    /// it reads is read on behalf of that call.  Set through
    /// [`DependencyGraph::set_reader`].
    pub reader: Option<NodeId>,
    /// For each call that was [`DependencyGraph::reader`]: how many vertices
    /// there were when it last stopped being it.  A vertex made later is
    /// nothing its expansion can have read.
    ends: HashMap<NodeId, NodeId>,
    /// While a package cache is being captured: the vertices it reached, new
    /// or not, in the order it first reached them.
    touched: Option<(crate::fixed::HashSet<NodeId>, Vec<NodeId>)>,
    /// The call read from a file when each touched vertex was first
    /// reached, in the same order.
    touched_readers: Vec<Option<NodeId>>,
    /// While a package cache is being captured: every edge added, in order,
    /// with the call a read was charged to, and each `made_by` among them
    /// as an edge of no kind; every extent noted.
    log: Option<GraphLog>,
    /// Vertices only, no edges, extents or makers: set while building a
    /// kernel whose graph is dropped once it is cached, which reads none of them.
    #[serde(skip)]
    pub vertices_only: bool,
}

#[derive(PartialEq, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GraphLog {
    pub edges: Vec<(NodeId, NodeId, EdgeKind, Option<NodeId>)>,
    /// Reads linked to whatever defines a name, not to particular vertices:
    /// where among the edges, from what, which name, charged to which call.
    pub links: Vec<(usize, NodeId, Sym, Option<NodeId>, EdgeKind)>,
    /// Set while such a read's own edges are made, which are not logged.
    pub linking: bool,
    pub extents: Vec<(Span, u32)>,
}

impl DependencyGraph {
    pub fn probe(&self) -> Vec<(&'static str, usize)> {
        crate::probe_sz!("g.vertices" => self.vertices, "g.out" => self.out, "g.wide" => self.wide, "g.sites" => self.sites, "g.extents" => self.extents, "g.made_by" => self.made_by, "g.defs_of" => self.defs_of, "g.ends" => self.ends, "g.touched" => self.touched, "g.touched_readers" => self.touched_readers, "g.log" => self.log)
    }
}

impl Vertex {
    /// How the name should be written: `\foo` for a control sequence, the
    /// bare text for a key.
    pub fn render(&self, it: &Interner) -> String {
        if self.key { it.name(self.name).to_string() } else { it.cs(self.name) }
    }
}

/// What keeps a slice from following every edge: a last vertex, and the file
/// whose calls a body vertex answers to.
pub struct SliceBound {
    pub limit: NodeId,
    pub home: Option<crate::engine::tex::FileId>,
    /// Calls reached only through what they define, never as a whole: a
    /// load is used by way of its file's definitions.
    pub opaque: crate::fixed::HashSet<NodeId>,
}

/// Definitions after which a name counts as scratch state.
const SCRATCH_DEFS: u32 = 16;

impl DependencyGraph {
    /// After an edit: `self`, the last run's graph at its end, takes this run's vertices up
    /// to where both runs met (`new`) for the ones the last run had there (`at`). The runs made
    /// the same vertices and edges up to there, apart from where on the `edited` lines a vertex
    /// sits, and the rest changed none of those. `false`, with `self` unchanged, otherwise.
    ///
    /// `sites` and `wide` are not compared: both follow from what is. A site is its vertex's
    /// span, name and tag ([`DependencyGraph::push_site`], vertices are never removed or renamed),
    /// so the sites differ exactly where the vertices do; `wide` indexes `out`. Only the
    /// vertices that differ are read and written, so the cost is the edit's, not the graph's.
    pub(crate) fn splice(&mut self, new: &Self, at: &Self, edited: impl Fn(&Span) -> bool) -> bool {
        let moved =
            |a: &Vertex, b: &Vertex| edited(&a.span) && edited(&b.span) && Vertex { span: b.span, ..a.clone() } == *b;
        let n = at.vertices.len();
        if new.vertices.len() != n || self.vertices.len() < n {
            return false;
        }
        let moves = new.vertices.differing(&at.vertices, |a, b| a == b);
        let extents =
            |a: &HashMap<Span, u32>, b: &HashMap<Span, u32>| a.iter().all(|(k, v)| edited(k) || b.get(k) == Some(v));
        let alike =
            moves.iter().all(|&i| moved(&new.vertices[i], &at.vertices[i]) && self.vertices[i] == at.vertices[i])
                && (new.cap, new.overflow, new.reader) == (at.cap, at.overflow, at.reader)
                && new.out == at.out
                && new.made_by == at.made_by
                && new.defs_of == at.defs_of
                && new.ends == at.ends
                && new.touched == at.touched
                && new.touched_readers == at.touched_readers
                && new.log == at.log
                && new.extents.len() == at.extents.len()
                && extents(&new.extents, &at.extents)
                && extents(&at.extents, &new.extents);
        if !alike {
            return false;
        }
        let site = |v: &Vertex| (v.span, v.name, v.tag);
        for &i in &moves {
            self.sites.remove(&site(&at.vertices[i]));
        }
        for i in moves {
            self.vertices[i].span = new.vertices[i].span;
            self.sites.insert_mut(site(&new.vertices[i]), i as NodeId);
        }
        self.extents.retain(|k, _| !edited(k));
        self.extents.extend(new.extents.iter().filter(|(k, _)| edited(k)).map(|(k, v)| (*k, *v)));
        true
    }

    /// Whether the graph is `old` apart from where a vertex stands: the same vertices and edges.
    pub fn alike_but_places(&self, old: &Self) -> bool {
        let moved = |i: usize| Vertex { span: old.vertices[i].span, ..self.vertices[i].clone() } == old.vertices[i];
        self.vertices.len() == old.vertices.len()
            && self.vertices.differing(&old.vertices, |a, b| a == b).into_iter().all(moved)
            && self.out == old.out
    }

    pub fn push(
        &mut self,
        tag: VertexTag,
        name: Sym,
        span: Span,
        within: Option<NodeId>,
        cds: &[ControlDep],
    ) -> NodeId {
        self.push_named(tag, name, false, span, within, cds)
    }

    /// A vertex whose name is a key rather than a control sequence.
    pub fn push_key(
        &mut self,
        tag: VertexTag,
        name: Sym,
        span: Span,
        within: Option<NodeId>,
        cds: &[ControlDep],
    ) -> NodeId {
        self.push_named(tag, name, true, span, within, cds)
    }

    fn push_named(
        &mut self,
        tag: VertexTag,
        name: Sym,
        key: bool,
        span: Span,
        within: Option<NodeId>,
        cds: &[ControlDep],
    ) -> NodeId {
        let id = self.push_site(tag, name, key, span, within, cds);
        self.touch(id);
        id
    }

    fn touch(&mut self, id: NodeId) {
        if let Some((seen, order)) = &mut self.touched
            && seen.insert(id)
        {
            order.push(id);
            self.touched_readers.push(self.reader);
        }
    }

    /// Start or stop the log of edges, `made_by` and extents.
    pub fn record_log(&mut self, on: bool) {
        self.log = on.then(GraphLog::default);
    }

    pub fn log(&self) -> Option<&GraphLog> {
        self.log.as_ref()
    }

    /// The vertices reached so far while recording, in order.
    pub fn touches(&self) -> &[NodeId] {
        self.touched.as_ref().map_or(&[][..], |(_, order)| &order[..])
    }

    /// The call read from a file when each of [`DependencyGraph::touches`]
    /// was first reached.
    pub fn touch_readers(&self) -> &[Option<NodeId>] {
        &self.touched_readers
    }

    /// Start recording the vertices reached, or stop and hand them back.
    pub fn record_touches(&mut self, on: bool) -> Vec<NodeId> {
        self.touched_readers.clear();
        let previous = std::mem::replace(&mut self.touched, on.then(Default::default));
        previous.map(|(_, order)| order).unwrap_or_default()
    }

    /// The vertex for a site, made as it is given when there is none yet,
    /// without the edges [`DependencyGraph::push`] adds: a package cache
    /// replays those itself.
    pub fn push_vertex(&mut self, vertex: Vertex) -> NodeId {
        if let Some(&id) = self.sites.get(&(vertex.span, vertex.name, vertex.tag)) {
            return id;
        }
        let id = self.vertices.len() as NodeId;
        self.sites.insert_mut((vertex.span, vertex.name, vertex.tag), id);
        self.out.push(Vec::new());
        self.vertices.push(vertex);
        id
    }

    fn push_site(
        &mut self,
        tag: VertexTag,
        name: Sym,
        key: bool,
        span: Span,
        within: Option<NodeId>,
        cds: &[ControlDep],
    ) -> NodeId {
        if let Some(id) = self.sites.get_mut(&(span, name, tag)).map(|id| *id) {
            // Only a change unshares the vertex's chunk.
            if self.vertices[id as usize].cds != cds {
                let vertex = &mut self.vertices[id as usize];
                if cds.len() > 16 {
                    let keep: crate::fixed::HashSet<(NodeId, bool)> = cds.iter().map(|c| (c.on, c.taken)).collect();
                    vertex.cds.retain(|c| keep.contains(&(c.on, c.taken)));
                } else {
                    vertex.cds.retain(|c| cds.contains(c));
                }
            }
            if let Some(reader) = self.reader.filter(|reader| *reader != id) {
                self.add_edge(reader, id, EdgeKind::EXPANDS);
            }
            return id;
        }
        if let Some(id) = self.overflow {
            return id;
        }
        let id = self.vertices.len() as NodeId;
        if self.cap > 0 && self.vertices.len() + 1 >= self.cap {
            self.overflow = Some(id);
        }
        self.sites.insert_mut((span, name, tag), id);
        self.out.push(Vec::new());
        self.vertices.push(Vertex { tag, name, key, span, cds: cds.to_vec(), within });
        // A control dependency is an edge like any other, so that slicing is
        // one walk in each direction rather than a walk plus a special case.
        for cd in cds {
            self.edge(id, cd.on, EdgeKind::CONTROL);
        }
        // What an expansion produced is part of what the call read from the
        // file depends on.  Not of the macro it was produced in: that vertex
        // stands for every expansion of its site, before and after this one.
        if let Some(parent) = within {
            self.add_edge(self.reader.unwrap_or(parent), id, EdgeKind::EXPANDS);
        }
        id
    }

    /// Charge what follows to `reader`, noting when the one before stopped.
    pub fn set_reader(&mut self, reader: Option<NodeId>) {
        if let Some(old) = self.reader.filter(|old| reader != Some(*old)) {
            let now = self.vertices.len() as NodeId;
            let end = self.ends.entry(old).or_insert(now);
            *end = (*end).max(now);
        }
        self.reader = reader;
    }

    /// Record that the construct beginning at `span` reads as far as `line`.
    pub fn note_extent(&mut self, span: Span, line: u32) {
        if line <= span.line || self.vertices_only {
            return;
        }
        if let Some(log) = &mut self.log {
            log.extents.push((span, line));
        }
        let slot = self.extents.entry(span).or_insert(line);
        *slot = (*slot).max(line);
    }

    /// The last line of the construct that began at `span`, when it ran past
    /// the line it started on.
    pub fn extent(&self, span: Span) -> Option<u32> {
        self.extents.get(&span).copied()
    }

    pub fn extents(&self) -> &HashMap<Span, u32> {
        &self.extents
    }

    /// Moves the recent sites into the shared table, before a checkpoint copies the graph.
    pub fn settle(&mut self) {
        self.sites.settle();
    }

    /// The vertex already standing for this source site, if any.
    pub fn find(&self, span: Span, name: Sym, tag: VertexTag) -> Option<NodeId> {
        self.sites.get(&(span, name, tag)).copied()
    }

    /// The call read from a file whose expansion last ran `def`.
    pub fn maker(&self, def: NodeId) -> Option<NodeId> {
        self.made_by.get(&def).copied()
    }

    /// Record that `call`'s expansion has just run the definition `def`.
    pub fn made_by(&mut self, def: NodeId, call: NodeId) {
        if self.vertices_only {
            return;
        }
        if let Some(log) = &mut self.log {
            // In the order of the edges too: a read after it is charged to
            // this call, one before it to the one before.
            log.edges.push((def, call, EdgeKind::default(), None));
        }
        if let Some(v) = self.vertices.get(def as usize) {
            *self.defs_of.entry(v.name).or_default() += 1;
        }
        self.made_by.insert(def, call);
    }

    pub fn edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) {
        if self.vertices_only {
            return;
        }
        let mention = kind.intersects(EdgeKind::MENTION);
        let kind = EdgeKind(kind.0 & !EdgeKind::MENTION.0);
        // A vertex inside a macro body stands for every expansion of it, so
        // the dependency on the call that made the definition belongs to the
        // call from the file this read is part of.
        let reader = self.reader.unwrap_or(from);
        if kind.intersects(EdgeKind::READS)
            && !mention
            && let Some(&call) = self.made_by.get(&to)
            && call != reader
            && self.defs_of.get(&self.vertices[to as usize].name).copied().unwrap_or(0) < SCRATCH_DEFS
        {
            self.add_edge(reader, call, EdgeKind::SIDE_EFFECT_ON_CALL);
        }
        self.add_edge(from, to, kind);
    }

    /// An edge as it is given, without what [`DependencyGraph::edge`] infers.
    pub fn add_raw_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) {
        self.add_edge(from, to, kind);
    }

    /// Record a read linked to whatever defines `name`; the edges made for
    /// it until [`DependencyGraph::end_link`] are not logged.
    pub fn begin_link(&mut self, from: NodeId, name: Sym, kind: EdgeKind) {
        let reader = self.reader;
        if let Some(log) = &mut self.log {
            log.links.push((log.edges.len(), from, name, reader, kind));
            log.linking = true;
        }
    }

    pub fn end_link(&mut self) {
        if let Some(log) = &mut self.log {
            log.linking = false;
        }
    }

    fn add_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) {
        if self.vertices_only {
            return;
        }
        if let Some(log) = &mut self.log
            && !log.linking
        {
            log.edges.push((from, to, kind, self.reader));
        }
        const WIDE: usize = 32;
        // Only a new edge or kind unshares the vertex's chunk.
        let Some(slot) = self.out.get(from as usize) else { return };
        let at = match slot.len() < WIDE {
            true => slot.iter().position(|(t, _)| *t == to),
            false => self.wide.get(&from).and_then(|w| w.get(&to)).map(|&i| i as usize),
        };
        if at.is_some_and(|i| slot[i].1 | kind == slot[i].1) {
            return;
        }
        let Some(slot) = self.out.get_mut(from as usize) else { return };
        if slot.len() < WIDE {
            if let Some(e) = slot.iter_mut().find(|(t, _)| *t == to) {
                e.1 |= kind;
            } else {
                slot.push((to, kind));
            }
            return;
        }
        if self.wide.get(&from).is_none() {
            let index = slot.iter().enumerate().map(|(i, (t, _))| (*t, i as u32)).rev().collect();
            self.wide.insert_mut(from, std::rc::Rc::new(index));
        }
        let index = std::rc::Rc::make_mut(self.wide.get_mut(&from).expect("inserted"));
        match index.get(&to) {
            Some(&i) => slot[i as usize].1 |= kind,
            None => {
                index.insert(to, slot.len() as u32);
                slot.push((to, kind));
            }
        }
    }

    pub fn len(&self) -> usize {
        self.vertices.len()
    }

    /// An empty graph that stops growing at `cap` vertices (0: never).
    pub fn with_cap(cap: usize) -> Self {
        Self { cap, ..Default::default() }
    }

    /// Whether the graph reached [`DependencyGraph::cap`].
    pub fn full(&self) -> bool {
        self.overflow.is_some()
    }

    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

    pub fn vertex(&self, id: NodeId) -> Option<&Vertex> {
        self.vertices.get(id as usize)
    }

    pub fn outgoing(&self, id: NodeId) -> &[(NodeId, EdgeKind)] {
        self.out.get(id as usize).map(|v| &v[..]).unwrap_or(&[])
    }

    pub fn edge_count(&self) -> usize {
        self.out.iter().map(|v| v.len()).sum()
    }

    /// The graph with every edge reversed, built on demand.
    fn transposed(&self) -> Vec<Vec<(NodeId, EdgeKind)>> {
        let mut out = vec![Vec::new(); self.vertices.len()];
        for (from, targets) in self.out.iter().enumerate() {
            for (to, kind) in targets {
                out[*to as usize].push((from as NodeId, *kind));
            }
        }
        out
    }

    /// The names the run defines over and over, like `\toks@` or `\reserved@a`:
    /// a definition of one is only ever read soon after it is made.
    fn scratch_names(&self) -> crate::fixed::HashSet<Sym> {
        self.defs_of.iter().filter(|&(_, &n)| n >= SCRATCH_DEFS).map(|(&name, _)| name).collect()
    }

    /// Backward: what the criteria depend on.  Forward: what depends on them.
    /// The two are exact transposes because they walk the same relation.
    /// Vertices past `bound.limit` are not followed.  A body vertex stands for
    /// every expansion of its macro, so its edges reach calls that ran after
    /// the criterion and cannot have influenced it.
    pub fn slice_until(&self, criteria: &[NodeId], kinds: EdgeKind, forward: bool, bound: &SliceBound) -> Vec<NodeId> {
        let transposed = forward.then(|| self.transposed());
        let mut kept = vec![false; self.vertices.len()];
        // Each vertex travels with the call of the home file it was reached
        // from: a body vertex stands for every expansion, and only the call
        // this one came through made what it defines.
        let scratch = self.scratch_names();
        let is_scratch = |id: NodeId| scratch.contains(&self.vertices[id as usize].name);
        let file_of = |id: NodeId| self.vertices[id as usize].span.file;
        // Each vertex also travels with the last vertex it may be: what an
        // expansion read was made before it ended (`ends`), and a body
        // vertex's edges from later expansions point past that.
        let mut stack: Vec<(NodeId, Option<NodeId>, NodeId)> =
            criteria.iter().map(|&c| (c, None, bound.limit)).collect();
        let mut seen: crate::fixed::HashMap<(NodeId, Option<NodeId>), NodeId> = Default::default();
        while let Some((node, entry, last)) = stack.pop() {
            let index = node as usize;
            if index >= kept.len() || node > last || seen.get(&(node, entry)).is_some_and(|&l| l >= last) {
                continue;
            }
            seen.insert((node, entry), last);
            kept[index] = true;
            let at_home = |id: NodeId| bound.home.is_some_and(|h| self.vertices[id as usize].span.file == h);
            let next = match &transposed {
                Some(incoming) => &incoming[index],
                None => self.outgoing(node),
            };
            for (other, kind) in next.iter() {
                let kind = &match !forward && bound.opaque.contains(other) {
                    true => EdgeKind(kind.0 & !EdgeKind::SIDE_EFFECT_ON_CALL.0),
                    false => *kind,
                };
                if !kind.intersects(kinds) {
                    continue;
                }
                let made_by = kind.intersects(EdgeKind(EdgeKind::DEFINED_BY.0 | EdgeKind::SIDE_EFFECT_ON_CALL.0));
                if !forward
                    && made_by
                    && !at_home(node)
                    && at_home(*other)
                    && self.vertices[*other as usize].tag == VertexTag::MacroCall
                    && entry.is_some_and(|e| e != *other)
                {
                    continue;
                }
                // Scratch state is rewritten all the time, so what one file
                // reads of it says nothing about the file that wrote it last.
                // An edge that touches the file being sliced is its own, and
                // may be the only way to what wrote it (`\DeclareOption`).
                if !forward
                    && kind.intersects(EdgeKind(EdgeKind::READS.0 | EdgeKind::DEFINED_BY.0))
                    && (is_scratch(node) || is_scratch(*other))
                    && file_of(node) != file_of(*other)
                    && !at_home(node)
                    && !at_home(*other)
                {
                    continue;
                }
                // Only an expansion runs a call's body: what a home vertex reads
                // was made by whichever call made it.
                let expands = at_home(node) && kind.intersects(EdgeKind::EXPANDS);
                let entry = match (at_home(node), expands) {
                    (true, true) => Some(node),
                    (true, false) => None,
                    (false, _) => entry,
                };
                let last = match self.ends.get(&node) {
                    Some(&end) if expands && !forward => last.min(end.saturating_sub(1)),
                    _ => last,
                };
                stack.push((*other, entry, last));
            }
        }
        (0..kept.len() as NodeId).filter(|&id| kept[id as usize]).collect()
    }

    pub fn to_dot(&self, it: &Interner, files: &[String]) -> String {
        let mut s = String::from("digraph satex {\n  rankdir=LR;\n  node [shape=box, fontname=\"monospace\"];\n"); // not prose
        for (i, v) in self.vertices.iter().enumerate() {
            let (shape, color) = match v.tag {
                VertexTag::VariableDefinition => ("box", "#1f77b4"),
                VertexTag::MacroDefinition => ("box3d", "#2ca02c"),
                VertexTag::MacroCall => ("ellipse", "#d62728"),
                VertexTag::Use => ("ellipse", "#7f7f7f"),
                VertexTag::Value => ("note", "#9467bd"),
            };
            let file = files.get(v.span.file as usize).map(String::as_str).unwrap_or("?");
            s.push_str(&format!(
                "  n{i} [label=\"{}\\n{}\\n{file}:{}\", shape={shape}, color=\"{color}\"];\n", // not prose
                v.tag.as_str(),
                escape(&v.render(it)),
                v.span,
            ));
            for cd in &v.cds {
                s.push_str(&format!(
                    "  n{i} -> n{} [style=dashed, color=\"#ff7f0e\", label=\"cd:{}\"];\n", // not prose
                    cd.on, cd.taken
                ));
            }
        }
        for (from, targets) in self.out.iter().enumerate() {
            for (to, kind) in targets {
                s.push_str(&format!("  n{from} -> n{to} [label=\"{}\"];\n", kind.names().join(","))); // not prose
            }
        }
        s.push_str("}\n");
        s
    }

    pub fn to_json(&self, it: &Interner, files: &[String]) -> serde_json::Value {
        let vertices: Vec<_> = self
            .vertices
            .iter()
            .enumerate()
            .map(|(i, v)| {
                serde_json::json!({
                    "id": i,
                    "tag": v.tag.as_str(),
                    "name": v.render(it),
                    "file": files.get(v.span.file as usize),
                    "line": v.span.line,
                    "col": v.span.col,
                    "cds": v.cds,
                    "within": v.within,
                })
            })
            .collect();
        let edges: Vec<_> = self
            .out
            .iter()
            .enumerate()
            .flat_map(|(from, ts)| {
                ts.iter().map(move |(to, k)| serde_json::json!({ "from": from, "to": to, "types": k.names() }))
            })
            .collect();
        serde_json::json!({ "vertices": vertices, "edges": edges })
    }
}

/// A call graph over control-sequence names, derived from the bodies of macro
/// definitions rather than from the [`DependencyGraph`], so that macros that
/// are never invoked are still covered. [`crate::analysis::machine::Analysis::calls`]
/// holds the one for a run.
#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct CallGraph {
    #[serde(with = "crate::fixed::flat")]
    pub nodes: crate::fixed::Chunked<Sym>,
    index: crate::fixed::SharedMap<Sym, usize>,
    /// The nodes looked up lately, one table probe each (the trie takes about five
    /// pointers, twice per macro call). `index` never drops a name, so an entry holds.
    #[serde(skip)]
    front: Front,
    #[serde(with = "crate::fixed::flat")]
    pub edges: crate::fixed::Chunked<Vec<usize>>,
    /// While a package cache is being captured: every distinct `node` and
    /// `add` since the last `clear` of its name, and every `clear`, in order.
    log: Option<(crate::fixed::LookupSet<CallEvent>, Vec<CallEvent>)>,
}

/// [`CallGraph::index`] entries in front of it. No part of the graph: a copy starts
/// without, and any two are equal.
#[derive(Default)]
struct Front(crate::fixed::LookupMap<Sym, usize>);

/// Entries [`Front`] holds before it starts over.
const FRONT: usize = 1 << 12;

impl Clone for Front {
    fn clone(&self) -> Self {
        Front::default()
    }
}

impl PartialEq for Front {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

/// `index` says where each of `nodes` stands ([`CallGraph::node`], the only writer of both):
/// equal nodes have equal indices, and the lists skip the chunks two copies share.
impl PartialEq for CallGraph {
    fn eq(&self, other: &Self) -> bool {
        self.nodes == other.nodes && self.edges == other.edges && self.log == other.log
    }
}

/// One change to a [`CallGraph`], as a package cache replays it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, serde::Deserialize)]
pub enum CallEvent {
    Node(Sym),
    Edge(Sym, Sym),
    Clear(Sym),
}

impl CallGraph {
    /// What has been recorded so far, in order.
    pub fn recorded(&self) -> &[CallEvent] {
        self.log.as_ref().map_or(&[][..], |(_, order)| &order[..])
    }

    /// Start recording, or stop and hand back what was recorded.
    pub fn record(&mut self, on: bool) -> Vec<CallEvent> {
        let previous = std::mem::replace(&mut self.log, on.then(Default::default));
        previous.map(|(_, order)| order).unwrap_or_default()
    }

    fn note(&mut self, event: CallEvent) {
        if let Some((seen, order)) = &mut self.log
            && (matches!(event, CallEvent::Clear(_)) || seen.insert(event))
        {
            order.push(event);
        }
    }

    /// Forget what `s` calls: a redefinition replaces it.
    /// Where `s` is, if it is a node.
    fn at(&mut self, s: Sym) -> Option<usize> {
        if let Some(&i) = self.front.0.get(&s) {
            return Some(i);
        }
        let i = *self.index.get(&s)?;
        if self.front.0.len() >= FRONT {
            self.front.0.clear();
        }
        self.front.0.insert(s, i);
        Some(i)
    }

    pub fn clear(&mut self, s: Sym) {
        if let Some(i) = self.at(s) {
            let targets = std::mem::take(&mut self.edges[i]);
            if let Some((seen, _)) = &mut self.log {
                for t in targets {
                    seen.remove(&CallEvent::Edge(s, self.nodes[t]));
                }
            }
        }
        self.note(CallEvent::Clear(s));
    }

    /// Apply a recorded change.
    pub fn replay(&mut self, event: CallEvent) {
        match event {
            CallEvent::Node(s) => {
                self.node(s);
            }
            CallEvent::Edge(from, to) => self.add(from, to),
            CallEvent::Clear(s) => self.clear(s),
        }
    }

    pub fn node(&mut self, s: Sym) -> usize {
        self.note(CallEvent::Node(s));
        if let Some(i) = self.at(s) {
            return i;
        }
        let i = self.nodes.len();
        self.nodes.push(s);
        self.index.insert_mut(s, i);
        self.edges.push(Vec::new());
        i
    }

    /// Whether `from` calls `to` already and no log waits for the edge.
    pub fn has(&mut self, from: Sym, to: Sym) -> bool {
        self.log.is_none() && self.at(from).zip(self.at(to)).is_some_and(|(a, b)| self.edges[a].contains(&b))
    }

    pub fn add(&mut self, from: Sym, to: Sym) {
        self.note(CallEvent::Edge(from, to));
        let (a, b) = (self.node(from), self.node(to));
        // Called once per expansion: a repeated edge would grow without bound.
        if !self.edges[a].contains(&b) {
            self.edges[a].push(b);
        }
    }

    /// Collapse parallel edges once, after the run.
    pub fn finish(&mut self) {
        for targets in self.edges.iter_mut() {
            targets.sort_unstable();
            targets.dedup();
        }
    }

    pub fn get(&self, s: Sym) -> Option<usize> {
        self.index.get(&s).copied()
    }

    /// Names that are directly or mutually recursive: the strongly
    /// connected components of the call graph, one of more than one name
    /// being a cycle of macros calling one another.
    pub fn recursive(&self) -> Vec<(Sym, Vec<Sym>)> {
        let mut out = Vec::new();
        strongly_connected(&self.edges, |component| match *component {
            [only] if self.edges[only].contains(&only) => out.push((self.nodes[only], vec![self.nodes[only]])),
            [_] => {}
            _ => {
                let cycle: Vec<Sym> = component.iter().map(|&node| self.nodes[node]).collect();
                out.extend(component.iter().map(|&node| (self.nodes[node], cycle.clone())));
            }
        });
        out
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The strongly connected components of `edges` (Tarjan), each handed to `each` (in the order
/// its nodes leave the stack) before the ones that reach it, iteratively so a long chain of
/// calls cannot overflow the stack. One buffer for all of them: most components are one node,
/// and a vector each cost the lint rule reading them most of its time in the browser.
fn strongly_connected(edges: &crate::fixed::Chunked<Vec<usize>>, mut each: impl FnMut(&[usize])) {
    const NONE: usize = usize::MAX;
    let n = edges.len();
    let (mut index, mut low, mut on) = (vec![NONE; n], vec![0; n], vec![false; n]);
    let (mut next, mut stack, mut frames, mut component) = (0, Vec::new(), Vec::new(), Vec::new());
    for root in 0..n {
        if index[root] != NONE {
            continue;
        }
        // A frame: the node and how many of its edges are done.
        frames.push((root, 0));
        (index[root], low[root], next) = (next, next, next + 1);
        stack.push(root);
        on[root] = true;
        while let Some(&mut (v, ref mut done)) = frames.last_mut() {
            if let Some(&w) = edges[v].get(*done) {
                *done += 1;
                if index[w] == NONE {
                    (index[w], low[w], next) = (next, next, next + 1);
                    stack.push(w);
                    on[w] = true;
                    frames.push((w, 0));
                } else if on[w] {
                    low[v] = low[v].min(index[w]);
                }
                continue;
            }
            frames.pop();
            if let Some(&(parent, _)) = frames.last() {
                low[parent] = low[parent].min(low[v]);
            }
            if low[v] == index[v] {
                component.clear();
                while let Some(w) = stack.pop() {
                    on[w] = false;
                    component.push(w);
                    if w == v {
                        break;
                    }
                }
                each(&component);
            }
        }
    }
}

#[cfg(test)]
mod scc_tests {
    #[test]
    fn components_close_before_what_reaches_them() {
        // 0 -> 1 <-> 2, 3 -> 3, 4 alone.
        let edges = vec![vec![1], vec![2], vec![1], vec![3], vec![]];
        let mut found: Vec<Vec<usize>> = Vec::new();
        super::strongly_connected(&edges.into(), |c| found.push(c.to_vec()));
        found.iter_mut().for_each(|c| c.sort());
        assert_eq!(found, [vec![1, 2], vec![0], vec![3], vec![4]]);
    }
}

#[cfg(test)]
mod vertices_only_tests {
    use super::{DependencyGraph, EdgeKind, VertexTag};
    use crate::engine::tex::{Span, Sym};

    /// A kernel build keeps only vertices: its graph is dropped once the kernel is
    /// cached. The ids must be the ones a full graph hands out, since the run keeps
    /// them (open calls, control dependencies), while no edge is stored.
    #[test]
    fn a_vertices_only_graph_numbers_vertices_as_a_full_one_and_keeps_no_edges() {
        let build = |vertices_only| {
            let mut g = DependencyGraph { vertices_only, ..Default::default() };
            let at = |line| Span { file: 1, line, col: 0 };
            let a = g.push(VertexTag::MacroCall, Sym(1), at(1), None, &[]);
            g.set_reader(Some(a));
            let b = g.push(VertexTag::MacroCall, Sym(2), at(2), Some(a), &[]);
            let again = g.push(VertexTag::MacroCall, Sym(2), at(2), Some(a), &[]);
            g.edge(a, b, EdgeKind::READS);
            g.made_by(b, a);
            g.note_extent(at(1), 5);
            ((a, b, again, g.len()), g.edge_count(), g.maker(b))
        };
        let (full, lean) = (build(false), build(true));
        assert_eq!(full.0, lean.0);
        assert!(full.1 > 0 && full.2.is_some());
        assert_eq!((lean.1, lean.2), (0, None));
    }
}
