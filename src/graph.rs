//! The dependency graph: vertices, edges, data and control dependencies.

use std::collections::HashMap;

use serde::Serialize;

use crate::env::NodeId;
use crate::tex::{Interner, Span, Sym};

/// One charged read edge: (from, to, call, made-by-reader).
type ReadEdge = (NodeId, NodeId, NodeId, Option<NodeId>);

/// What kind of program point a [`Vertex`] stands for: a value, a use, a
/// function call, or the definition of a variable or function.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum VertexTag {
    Value,
    Use,
    FunctionCall,
    VariableDefinition,
    FunctionDefinition,
}

impl VertexTag {
    pub fn as_str(self) -> &'static str {
        match self {
            VertexTag::Value => "value",
            VertexTag::Use => "use",
            VertexTag::FunctionCall => "function-call",
            VertexTag::VariableDefinition => "variable-definition",
            VertexTag::FunctionDefinition => "function-definition",
        }
    }
}

/// How one [`Vertex`] depends on another, as a bit set so that parallel edges
/// between the same two vertices collapse into one. [`DependencyGraph::slice`]
/// filters a walk by which kinds it follows.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Serialize)]
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
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize)]
pub struct ControlDep {
    pub on: NodeId,
    pub taken: bool,
}

/// One node of the [`DependencyGraph`]: a control sequence or key, tagged with
/// a [`VertexTag`], at the [`Span`] it was seen.
#[derive(Clone, Debug)]
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
/// uses, connected by [`EdgeKind`] edges. [`DependencyGraph::slice`] is what
/// `satex slice` walks.
#[derive(Default)]
pub struct DependencyGraph {
    pub vertices: Vec<Vertex>,
    /// Outgoing edges, indexed by source vertex.
    out: Vec<Vec<(NodeId, EdgeKind)>>,
    /// Where each target sits in `out`, for the vertices with many outgoing
    /// edges: a call that runs a long loop reaches every vertex the loop
    /// makes, and finding an edge by scanning would make it quadratic.
    wide: HashMap<NodeId, HashMap<NodeId, u32>>,
    /// One vertex per source site.  A macro body that is read several times —
    /// because a package is analyzed under both arms of an undecided
    /// conditional, or because a loop is unrolled — describes the same
    /// program point every time.
    sites: HashMap<(Span, Sym, VertexTag), NodeId>,
    /// The last line a construct that began at a span reached, for the ones
    /// whose arguments ran past their own line.  Only spans that reach
    /// further than their own line are in here.
    extents: HashMap<Span, u32>,
    /// The call read from a file whose expansion last ran a definition site.
    /// A definition vertex stands for its site in every expansion that runs
    /// it, so what reads the definition reads the call that made it now.
    made_by: HashMap<NodeId, NodeId>,
    /// The call read from a file whose expansion is running, if any: what
    /// it reads is read on behalf of that call.
    pub reader: Option<NodeId>,
    /// While a package cache is being captured: the vertices it reached, new
    /// or not, in the order it first reached them.
    touched: Option<(std::collections::HashSet<NodeId>, Vec<NodeId>)>,
    /// The call read from a file when each touched vertex was first
    /// reached, in the same order.
    touched_readers: Vec<Option<NodeId>>,
    /// While a package cache is being captured: every read edge made, with
    /// the call charged with it.
    reads: Option<Vec<ReadEdge>>,
    /// While a package cache is being captured: every edge added, in order,
    /// with the call a read was charged to, and each `made_by` among them
    /// as an edge of no kind; every extent noted.
    log: Option<GraphLog>,
}

#[derive(Default)]
pub struct GraphLog {
    pub edges: Vec<(NodeId, NodeId, EdgeKind, Option<NodeId>)>,
    /// Reads linked to whatever defines a name, not to particular vertices:
    /// where among the edges, from what, which name, charged to which call.
    pub links: Vec<(usize, NodeId, Sym, Option<NodeId>, EdgeKind)>,
    /// Set while such a read's own edges are made, which are not logged.
    pub linking: bool,
    pub extents: Vec<(Span, u32)>,
}

impl Vertex {
    /// How the name should be written: `\foo` for a control sequence, the
    /// bare text for a key.
    pub fn render(&self, it: &Interner) -> String {
        if self.key {
            it.name(self.name).to_string()
        } else {
            it.cs(self.name)
        }
    }
}

impl DependencyGraph {
    pub fn push(&mut self, tag: VertexTag, name: Sym, span: Span, within: Option<NodeId>, cds: Vec<ControlDep>) -> NodeId {
        self.push_named(tag, name, false, span, within, cds)
    }

    /// A vertex whose name is a key rather than a control sequence.
    pub fn push_key(&mut self, tag: VertexTag, name: Sym, span: Span, within: Option<NodeId>, cds: Vec<ControlDep>) -> NodeId {
        self.push_named(tag, name, true, span, within, cds)
    }

    fn push_named(&mut self, tag: VertexTag, name: Sym, key: bool, span: Span, within: Option<NodeId>, cds: Vec<ControlDep>) -> NodeId {
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
        self.reads = on.then(Vec::new);
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
        self.sites.insert((vertex.span, vertex.name, vertex.tag), id);
        self.out.push(Vec::new());
        self.vertices.push(vertex);
        id
    }

    /// The read edges made while recording: from, to, the call charged, and
    /// the call that had made the definition read.
    pub fn take_reads(&mut self) -> Vec<ReadEdge> {
        self.reads.take().unwrap_or_default()
    }

    /// Every definition vertex and the call that last ran it.
    pub fn made_by_all(&self) -> &HashMap<NodeId, NodeId> {
        &self.made_by
    }

    fn push_site(&mut self, tag: VertexTag, name: Sym, key: bool, span: Span, within: Option<NodeId>, cds: Vec<ControlDep>) -> NodeId {
        if let Some(&id) = self.sites.get(&(span, name, tag)) {
            let vertex = &mut self.vertices[id as usize];
            if vertex.cds != cds {
                if cds.len() > 16 {
                    let keep: std::collections::HashSet<(NodeId, bool)> =
                        cds.iter().map(|c| (c.on, c.taken)).collect();
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
        let id = self.vertices.len() as NodeId;
        self.sites.insert((span, name, tag), id);
        self.out.push(Vec::new());
        self.vertices.push(Vertex { tag, name, key, span, cds: cds.clone(), within });
        // A control dependency is an edge like any other, so that slicing is
        // one walk in each direction rather than a walk plus a special case.
        for cd in &cds {
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

    /// Record that the construct beginning at `span` reads as far as `line`.
    pub fn note_extent(&mut self, span: Span, line: u32) {
        if line <= span.line {
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

    /// Replace a vertex's outgoing edges, keeping the order given.
    pub fn set_edges(&mut self, id: NodeId, edges: Vec<(NodeId, EdgeKind)>) {
        if let Some(slot) = self.out.get_mut(id as usize) {
            *slot = edges;
            self.wide.remove(&id);
        }
    }

    pub fn extents(&self) -> &HashMap<Span, u32> {
        &self.extents
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
        if let Some(log) = &mut self.log {
            // In the order of the edges too: a read after it is charged to
            // this call, one before it to the one before.
            log.edges.push((def, call, EdgeKind::default(), None));
        }
        self.made_by.insert(def, call);
    }

    pub fn edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) {
        let mention = kind.intersects(EdgeKind::MENTION);
        let kind = EdgeKind(kind.0 & !EdgeKind::MENTION.0);
        // A vertex inside a macro body stands for every expansion of it, so
        // the dependency on the call that made the definition belongs to the
        // call from the file this read is part of.
        let reader = self.reader.unwrap_or(from);
        if kind.intersects(EdgeKind::READS)
            && !mention
            && let Some(log) = &mut self.reads
        {
            log.push((from, to, reader, self.made_by.get(&to).copied()));
        }
        if kind.intersects(EdgeKind::READS)
            && !mention
            && let Some(&call) = self.made_by.get(&to)
            && call != reader
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
        if let Some(log) = &mut self.log
            && !log.linking
        {
            log.edges.push((from, to, kind, self.reader));
        }
        const WIDE: usize = 32;
        let Some(slot) = self.out.get_mut(from as usize) else { return };
        if slot.len() < WIDE {
            if let Some(e) = slot.iter_mut().find(|(t, _)| *t == to) {
                e.1 |= kind;
            } else {
                slot.push((to, kind));
            }
            return;
        }
        let index = self.wide.entry(from).or_insert_with(|| {
            slot.iter().enumerate().map(|(i, (t, _))| (*t, i as u32)).rev().collect()
        });
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

    /// Backward: what the criteria depend on.  Forward: what depends on them.
    /// The two are exact transposes because they walk the same relation.
    pub fn slice(&self, criteria: &[NodeId], kinds: EdgeKind, forward: bool) -> Vec<NodeId> {
        let transposed = forward.then(|| self.transposed());
        let mut seen = vec![false; self.vertices.len()];
        let mut stack: Vec<NodeId> = criteria.to_vec();
        let mut out = Vec::new();
        while let Some(node) = stack.pop() {
            let index = node as usize;
            if index >= seen.len() || seen[index] {
                continue;
            }
            seen[index] = true;
            out.push(node);
            let next = match &transposed {
                Some(incoming) => &incoming[index],
                None => self.outgoing(node),
            };
            stack.extend(next.iter().filter(|(_, k)| k.intersects(kinds)).map(|(other, _)| *other));
        }
        out.sort_unstable();
        out
    }

    pub fn to_dot(&self, it: &Interner, files: &[String]) -> String {
        let mut s = String::from("digraph satex {\n  rankdir=LR;\n  node [shape=box, fontname=\"monospace\"];\n");
        for (i, v) in self.vertices.iter().enumerate() {
            let (shape, color) = match v.tag {
                VertexTag::VariableDefinition => ("box", "#1f77b4"),
                VertexTag::FunctionDefinition => ("box3d", "#2ca02c"),
                VertexTag::FunctionCall => ("ellipse", "#d62728"),
                VertexTag::Use => ("ellipse", "#7f7f7f"),
                VertexTag::Value => ("note", "#9467bd"),
            };
            let file = files.get(v.span.file as usize).map(String::as_str).unwrap_or("?");
            s.push_str(&format!(
                "  n{i} [label=\"{}\\n{}\\n{file}:{}\", shape={shape}, color=\"{color}\"];\n",
                v.tag.as_str(),
                escape(&v.render(it)),
                v.span,
            ));
            for cd in &v.cds {
                s.push_str(&format!(
                    "  n{i} -> n{} [style=dashed, color=\"#ff7f0e\", label=\"cd:{}\"];\n",
                    cd.on, cd.taken
                ));
            }
        }
        for (from, targets) in self.out.iter().enumerate() {
            for (to, kind) in targets {
                s.push_str(&format!("  n{from} -> n{to} [label=\"{}\"];\n", kind.names().join(",")));
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
                ts.iter().map(move |(to, k)| {
                    serde_json::json!({ "from": from, "to": to, "types": k.names() })
                })
            })
            .collect();
        serde_json::json!({ "vertices": vertices, "edges": edges })
    }
}

/// A call graph over control-sequence names, derived from the bodies of macro
/// definitions rather than from the [`DependencyGraph`], so that macros that
/// are never invoked are still covered. [`crate::machine::Analysis::calls`]
/// holds the one for a run.
#[derive(Default)]
pub struct CallGraph {
    pub nodes: Vec<Sym>,
    index: HashMap<Sym, usize>,
    pub edges: Vec<Vec<usize>>,
    /// While a package cache is being captured: every distinct `node` and
    /// `add` since the last `clear` of its name, and every `clear`, in order.
    log: Option<(std::collections::HashSet<CallEvent>, Vec<CallEvent>)>,
}

/// One change to a [`CallGraph`], as a package cache replays it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, serde::Deserialize)]
pub enum CallEvent {
    Node(Sym),
    Edge(Sym, Sym),
    Clear(Sym),
}

impl CallGraph {
    /// Start recording, or stop and hand back what was recorded.
    /// What has been recorded so far, in order.
    pub fn recorded(&self) -> &[CallEvent] {
        self.log.as_ref().map_or(&[][..], |(_, order)| &order[..])
    }

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
    pub fn clear(&mut self, s: Sym) {
        if let Some(&i) = self.index.get(&s) {
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
        if let Some(&i) = self.index.get(&s) {
            return i;
        }
        let i = self.nodes.len();
        self.nodes.push(s);
        self.index.insert(s, i);
        self.edges.push(Vec::new());
        i
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
        for targets in &mut self.edges {
            targets.sort_unstable();
            targets.dedup();
        }
    }

    pub fn get(&self, s: Sym) -> Option<usize> {
        self.index.get(&s).copied()
    }

    /// Strongly connected components; a component of more than one name is a
    /// cycle of macros calling one another.
    pub fn sccs(&self) -> Vec<Vec<Sym>> {
        let mut graph: petgraph::graph::DiGraph<(), ()> = petgraph::graph::DiGraph::new();
        let nodes: Vec<_> = self.nodes.iter().map(|_| graph.add_node(())).collect();
        for (from, targets) in self.edges.iter().enumerate() {
            for to in targets {
                graph.add_edge(nodes[from], nodes[*to], ());
            }
        }
        petgraph::algo::tarjan_scc(&graph)
            .into_iter()
            .map(|component| {
                component.into_iter().map(|node| self.nodes[node.index()]).collect()
            })
            .collect()
    }

    /// Names that are directly or mutually recursive.
    pub fn recursive(&self) -> Vec<(Sym, Vec<Sym>)> {
        let mut out = Vec::new();
        for comp in self.sccs() {
            if comp.len() > 1 {
                for &s in &comp {
                    out.push((s, comp.clone()));
                }
            } else if let Some(i) = self.get(comp[0])
                && self.edges[i].contains(&i) {
                    out.push((comp[0], comp));
                }
        }
        out
    }
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
