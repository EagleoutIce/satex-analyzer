//! The slots a path changed since a fork: a persistent trie on the name's
//! bits. A snapshot is a pointer copy. A path resumed from another, or a
//! join started from one side, shares every node it did not write, so a
//! walk over two such maps skips their shared subtrees ([`Changed::pairs`]).

use std::cell::Cell;
use std::rc::Rc;

use super::Binding;

pub type Entry = (Option<Binding>, u32);

thread_local! {
    /// Entries the maps of this thread walked since [`take_walked`]: the work
    /// of joins, comparisons and resumes, which reads no token.
    static WALKED: Cell<u64> = const { Cell::new(0) };
}

fn walk(n: usize) {
    WALKED.with(|w| w.set(w.get() + n as u64));
}

/// The entries walked since the last call ([`Machine::charge`] pays them).
pub fn take_walked() -> u64 {
    WALKED.with(|w| w.replace(0))
}

const BITS: u32 = 5;
/// A node's [`Node::summary`] not computed since it last changed.
const DIRTY: u32 = u32::MAX;
/// Summary bit: some entry below is not [`self_joins`].
const MOVES: u32 = 1 << 31;

struct Leaf {
    key: u32,
    entry: Entry,
    /// [`self_joins`] of `entry`, once known: 0 not yet, 1 no, 2 yes.
    joins: Cell<u8>,
}

#[derive(Clone)]
enum Kid {
    Leaf(Rc<Leaf>),
    Node(Rc<Node>),
}

#[derive(Clone, Default)]
struct Node {
    /// Which of the 32 child positions are taken.
    bits: u32,
    kids: Vec<Kid>,
    /// The highest group level below, with [`MOVES`]; [`DIRTY`] when unknown.
    /// Kept while the node is shared, which is when nothing can change it.
    summary: Cell<u32>,
}

fn at(key: u32, depth: u32) -> u32 {
    key.checked_shr(BITS * depth).unwrap_or(0) & 31
}

/// Whether the join of `slot` with itself is `slot` again in every part a
/// join looks at (`super::join`): one meaning (or a set of one), its
/// definitions sorted and each once, and a value equal to itself. Then a
/// join of two paths that share the entry keeps it as it is.
fn self_joins(slot: &Option<Binding>) -> bool {
    let Some(b) = slot else { return true };
    b.may.as_ref().is_none_or(|m| m.len() == 1)
        && b.defs.windows(2).all(|w| w[0] < w[1])
        && super::identical(slot, slot)
}

impl Leaf {
    fn joins(&self) -> bool {
        if self.joins.get() == 0 {
            self.joins.set(1 + u8::from(self_joins(&self.entry.0)));
        }
        self.joins.get() == 2
    }
}

impl Node {
    fn summary(&self) -> u32 {
        if self.summary.get() == DIRTY {
            let s = self.kids.iter().fold(0, |s, kid| match kid {
                Kid::Leaf(l) => both(s, leaf_summary(l)),
                Kid::Node(n) => both(s, n.summary()),
            });
            self.summary.set(s);
        }
        self.summary.get()
    }

    fn pos(&self, i: u32) -> usize {
        (self.bits & ((1 << i) - 1)).count_ones() as usize
    }

    fn kid(&self, i: u32) -> Option<&Kid> {
        (self.bits & (1 << i) != 0).then(|| &self.kids[self.pos(i)])
    }
}

/// Mutable access to a node, unshared first; its summary is then unknown.
fn own(node: &mut Rc<Node>) -> &mut Node {
    let n = Rc::make_mut(node);
    n.summary.set(DIRTY);
    n
}

fn insert(node: &mut Rc<Node>, depth: u32, key: u32, entry: Entry) -> bool {
    let n = own(node);
    let i = at(key, depth);
    let pos = n.pos(i);
    let leaf = |key, entry| Kid::Leaf(Rc::new(Leaf { key, entry, joins: Cell::new(0) }));
    if n.bits & (1 << i) == 0 {
        n.bits |= 1 << i;
        n.kids.insert(pos, leaf(key, entry));
        return true;
    }
    match &mut n.kids[pos] {
        Kid::Node(c) => insert(c, depth + 1, key, entry),
        Kid::Leaf(l) if l.key == key => {
            *l = Rc::new(Leaf { key, entry, joins: Cell::new(0) });
            false
        }
        Kid::Leaf(l) => {
            let old = l.clone();
            let mut c = Rc::new(Node { summary: Cell::new(DIRTY), ..Node::default() });
            let j = at(old.key, depth + 1);
            own(&mut c).bits = 1 << j;
            own(&mut c).kids.push(Kid::Leaf(old));
            insert(&mut c, depth + 1, key, entry);
            n.kids[pos] = Kid::Node(c);
            true
        }
    }
}

fn remove(node: &mut Rc<Node>, depth: u32, key: u32) -> bool {
    let i = at(key, depth);
    let Some(kid) = node.kid(i) else { return false };
    let found = match kid {
        Kid::Leaf(l) => l.key == key,
        Kid::Node(c) => c.get(depth + 1, key).is_some(),
    };
    if !found {
        return false;
    }
    let n = own(node);
    let pos = n.pos(i);
    if let Kid::Node(c) = &mut n.kids[pos] {
        remove(c, depth + 1, key);
        // A node left with one leaf becomes that leaf.
        let single = match &c.kids[..] {
            [Kid::Leaf(l)] => Some(l.clone()),
            _ => None,
        };
        if let Some(l) = single {
            n.kids[pos] = Kid::Leaf(l);
        }
    } else {
        n.kids.remove(pos);
        n.bits &= !(1 << i);
    }
    true
}

impl Node {
    fn get(&self, depth: u32, key: u32) -> Option<&Entry> {
        match self.kid(at(key, depth))? {
            Kid::Leaf(l) => (l.key == key).then_some(&l.entry),
            Kid::Node(c) => c.get(depth + 1, key),
        }
    }
}

/// One name of [`Changed::pairs`]: its entry in each map, if any.
pub type Pair<'a> = (u32, Option<&'a Entry>, Option<&'a Entry>);

fn pairs<'a>(x: Option<&'a Kid>, y: Option<&'a Kid>, skip: &dyn Fn(u32) -> bool, out: &mut Vec<Pair<'a>>) {
    match (x, y) {
        (Some(Kid::Node(p)), Some(Kid::Node(q))) if Rc::ptr_eq(p, q) && skip(p.summary()) => {}
        (Some(Kid::Node(p)), Some(Kid::Node(q))) => {
            for i in 0..32 {
                if (p.bits | q.bits) & (1 << i) != 0 {
                    pairs(p.kid(i), q.kid(i), skip, out);
                }
            }
        }
        (Some(Kid::Leaf(l)), Some(Kid::Leaf(m))) if Rc::ptr_eq(l, m) && skip(leaf_summary(l)) => {}
        (Some(Kid::Leaf(l)), y) => {
            let other = y.map(|y| with(y, l.key, false, out));
            out.push((l.key, Some(&l.entry), other.flatten()));
        }
        (x, Some(Kid::Leaf(m))) => {
            let mine = x.map(|x| with(x, m.key, true, out));
            out.push((m.key, mine.flatten(), Some(&m.entry)));
        }
        (Some(Kid::Node(p)), None) => p.kids.iter().for_each(|k| pairs(Some(k), None, skip, out)),
        (None, Some(Kid::Node(q))) => q.kids.iter().for_each(|k| pairs(None, Some(k), skip, out)),
        (None, None) => {}
    }
}

/// Pushes every entry under `kid` but `key`'s, on the side `mine` says,
/// and returns `key`'s entry there.
fn with<'a>(kid: &'a Kid, key: u32, mine: bool, out: &mut Vec<Pair<'a>>) -> Option<&'a Entry> {
    match kid {
        Kid::Leaf(l) if l.key == key => Some(&l.entry),
        Kid::Leaf(l) => {
            out.push(if mine { (l.key, Some(&l.entry), None) } else { (l.key, None, Some(&l.entry)) });
            None
        }
        Kid::Node(n) => n.kids.iter().fold(None, |found, k| with(k, key, mine, out).or(found)),
    }
}

/// The summary of two subtrees together.
fn both(s: u32, t: u32) -> u32 {
    ((s | t) & MOVES) | (s & !MOVES).max(t & !MOVES)
}

fn leaf_summary(l: &Leaf) -> u32 {
    (if l.joins() { 0 } else { MOVES }) | l.entry.1.min(MOVES - 1)
}

#[derive(Clone)]
pub struct Changed {
    root: Rc<Node>,
    size: usize,
}

impl Default for Changed {
    fn default() -> Self {
        Changed { root: Rc::new(Node { summary: Cell::new(DIRTY), ..Node::default() }), size: 0 }
    }
}

impl Changed {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get(&self, key: &u32) -> Option<&Entry> {
        self.root.get(0, *key)
    }

    pub fn contains_key(&self, key: &u32) -> bool {
        self.get(key).is_some()
    }

    pub fn insert_mut(&mut self, key: u32, entry: Entry) {
        self.size += usize::from(insert(&mut self.root, 0, key, entry));
    }

    pub fn remove_mut(&mut self, key: &u32) -> bool {
        let gone = remove(&mut self.root, 0, *key);
        self.size -= usize::from(gone);
        gone
    }

    pub fn size(&self) -> usize {
        self.size
    }

    pub fn is_empty(&self) -> bool {
        self.size == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = (&u32, &Entry)> {
        walk(self.size);
        Iter { stack: vec![self.root.kids.iter()] }
    }

    pub fn keys(&self) -> impl Iterator<Item = &u32> {
        self.iter().map(|(k, _)| k)
    }

    /// Every name either map holds, with both entries, except names in a
    /// subtree both maps share whose summary `skip` accepts: there the two
    /// entries are one. `skip` gets the subtree's highest group level, with
    /// bit 31 set when an entry there does not join with itself as itself.
    pub fn pairs<'a>(&'a self, other: &'a Changed, skip: impl Fn(u32) -> bool) -> Vec<Pair<'a>> {
        let mut out = Vec::new();
        if Rc::ptr_eq(&self.root, &other.root) && skip(self.root.summary()) {
            return out;
        }
        let (p, q) = (&self.root, &other.root);
        for i in 0..32 {
            if (p.bits | q.bits) & (1 << i) != 0 {
                pairs(p.kid(i), q.kid(i), &skip, &mut out);
            }
        }
        walk(out.len());
        out
    }

    /// Whether every entry of a subtree with `summary` joins with itself as
    /// itself, at a group level of at most `depth`.
    pub fn settled(summary: u32, depth: u32) -> bool {
        summary & MOVES == 0 && summary <= depth
    }
}

struct Iter<'a> {
    stack: Vec<std::slice::Iter<'a, Kid>>,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a u32, &'a Entry);

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            match self.stack.last_mut()?.next() {
                None => {
                    self.stack.pop();
                }
                Some(Kid::Leaf(l)) => return Some((&l.key, &l.entry)),
                Some(Kid::Node(n)) => self.stack.push(n.kids.iter()),
            }
        }
    }
}

impl FromIterator<(u32, Entry)> for Changed {
    fn from_iter<I: IntoIterator<Item = (u32, Entry)>>(iter: I) -> Self {
        let mut map = Changed::new();
        for (k, e) in iter {
            map.insert_mut(k, e);
        }
        walk(map.size);
        map
    }
}

impl serde::Serialize for Changed {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.iter())
    }
}

impl<'de> serde::Deserialize<'de> for Changed {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Vec::<(u32, Entry)>::deserialize(d)?.into_iter().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    /// Inserts and removes on copies that share nodes agree with a plain map,
    /// and `pairs` names every key either side holds, minus shared subtrees
    /// when asked to skip them, never a key whose entries differ. Keys spread
    /// over all 32 bits make splits go to the deepest level.
    #[test]
    fn shared_copies_agree_with_a_plain_map() {
        let mut seed = 7u64;
        let mut next = || {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) as u32
        };
        let (mut maps, mut models) = (vec![Changed::new()], vec![BTreeMap::new()]);
        for step in 0..20_000 {
            let n = next() as usize % maps.len();
            if step % 50 == 0 {
                maps.push(maps[n].clone());
                models.push(models[n].clone());
                continue;
            }
            let r = next();
            let key = if r % 3 == 0 { r } else { r % 300 };
            if next() % 4 == 0 {
                assert_eq!(maps[n].remove_mut(&key), models[n].remove(&key).is_some());
            } else {
                let level = next() % 9;
                maps[n].insert_mut(key, (None, level));
                models[n].insert(key, level);
            }
        }
        for (map, model) in maps.iter().zip(&models) {
            assert_eq!(map.size(), model.len());
            let got: BTreeMap<u32, u32> = map.iter().map(|(k, e)| (*k, e.1)).collect();
            assert_eq!(&got, model);
        }
        for (a, b) in [(0, 1), (3, 4), (10, 11), (5, 30)] {
            let all = maps[a].pairs(&maps[b], |_| false);
            let mut keys: Vec<u32> = all.iter().map(|p| p.0).collect();
            keys.sort_unstable();
            let union: Vec<u32> = models[a]
                .keys()
                .chain(models[b].keys())
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            assert_eq!(keys, union);
            for (k, x, y) in &all {
                assert_eq!(x.map(|e| e.1), models[a].get(k).copied());
                assert_eq!(y.map(|e| e.1), models[b].get(k).copied());
            }
            let some: Vec<u32> = maps[a].pairs(&maps[b], |_| true).iter().map(|p| p.0).collect();
            for k in union {
                if models[a].get(&k) != models[b].get(&k) {
                    assert!(some.contains(&k), "{k} differs but was skipped");
                }
            }
        }
    }
}
