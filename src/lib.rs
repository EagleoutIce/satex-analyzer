//! SaTeX: static analysis for TeX and LaTeX.
//!
//! An abstract interpreter that runs a document the way the engine would —
//! the real `latex.ltx`, expl3 and package code, with TeX, e-TeX, pdfTeX and
//! XeTeX primitives implemented in Rust — and records what it saw: definitions,
//! uses, a dependency graph (data and control dependencies), occurrences such
//! as labels, citations and environments, and the places it could not decide.
//! Values it cannot know stay unknown, undecided conditionals are analyzed on
//! every path, and budgets bound every run.  The semantics are specified in
//! [`SEMANTICS.md`](https://eagleoutice.github.io/satex-analyzer/SEMANTICS).
//!
//! The `satex` command-line tool is the main interface (`lint`, `explain`,
//! `slice`, `query`, `summary`, `lsp`, …); see the
//! [documentation](https://eagleoutice.github.io/satex-analyzer/).  As a library:
//!
//! ```no_run
//! use satex::project::config::Config;
//! use satex::analysis::machine::Machine;
//!
//! let source = r"\documentclass{article}\newcommand\hi[1]{Hi #1}\begin{document}\hi{you}\end{document}";
//! let analysis = Machine::analyze(source, None, &Config::default());
//! for def in &analysis.facts.defs {
//!     println!("{} at {}", analysis.interner.cs(def.name), def.span);
//! }
//! for finding in satex::linter::lint(&analysis) {
//!     println!("{}", finding["message"]);
//! }
//! ```
//!
//! Entry points: [`analysis::machine::Machine::analyze`] runs a document,
//! [`queries::query`] answers questions about the result, [`linter`] reports findings,
//! [`project::config::Config`] holds every setting of `satex.yaml`.

#[macro_use]
pub mod util;
pub mod analysis;
pub mod caches;
pub mod cli;
pub mod engine;
pub mod knowledge;
pub mod linter;
pub mod lsp;
pub mod oracle;
pub mod plugins;
pub mod project;
pub mod queries;
pub mod text;

pub use analysis::machine::{Analysis, Machine};
pub use project::config::Config;

use std::path::Path;

pub fn analyze(source: &str, path: Option<&Path>, cfg: &Config) -> Analysis {
    Machine::analyze(source, path, cfg)
}

/// Push onto a stack the path snapshots share. A shared stack is copied
/// with room to grow, so that the push does not copy it a second time.
pub(crate) fn push_shared<T: Clone>(stack: &mut std::rc::Rc<Vec<T>>, item: T) {
    if let Some(own) = std::rc::Rc::get_mut(stack) {
        own.push(item);
        return;
    }
    let mut copy = Vec::with_capacity(stack.len() * 2 + 1);
    copy.extend_from_slice(stack);
    copy.push(item);
    *stack = std::rc::Rc::new(copy);
}

/// Hash collections with fixed keys, so iteration order is the same every
/// run: with the std's random keys one run in about eight took other paths.
pub mod fixed {
    #[macro_export]
    macro_rules! probe_sz { ($($n:literal => $e:expr),* $(,)?) => {{ let mut v: Vec<(&'static str, usize)> = Vec::new(); $( let b = $crate::analysis::budget::in_use(); let x = $e.clone(); let d = $crate::analysis::budget::in_use().saturating_sub(b); drop(x); v.push(($n, d >> 10)); )* v }} }
    pub type Hasher = std::hash::BuildHasherDefault<std::collections::hash_map::DefaultHasher>;
    pub type HashMap<K, V> = std::collections::HashMap<K, V, Hasher>;
    pub type HashSet<K> = std::collections::HashSet<K, Hasher>;
    /// For tables that are only looked up, or whose iteration is sorted
    /// before use: no order of theirs is ever seen, so a fast hash will do.
    pub type LookupMap<K, V> = std::collections::HashMap<K, V, crate::engine::tex::NameHash>;
    pub type LookupSet<K> = std::collections::HashSet<K, crate::engine::tex::NameHash>;
    /// A map a clone shares: one place to pick the pointer kind.
    pub type SharedMap<K, V> = rpds::HashTrieMap<K, V, archery::RcK, crate::engine::tex::NameHash>;

    /// A [`SharedMap`] with a plain hash table in front for what was written or
    /// updated lately. A trie lookup chases about five pointers, a table probe
    /// one: the per-call site tables are looked up several times per macro
    /// call. `hot` overrides `base`. It moves into `base` when it is full and
    /// before a checkpoint copies the map ([`HotMap::settle`]), so a copy still
    /// shares nearly everything.
    pub struct HotMap<K, V> {
        base: SharedMap<K, V>,
        hot: LookupMap<K, V>,
    }

    /// Entries [`HotMap::hot`] holds before they move to the trie.
    const HOT: usize = 1 << 16;

    impl<K: Eq + std::hash::Hash + Clone, V: Clone> HotMap<K, V> {
        pub fn get(&self, key: &K) -> Option<&V> {
            self.hot.get(key).or_else(|| self.base.get(key))
        }

        /// The entry, moved to the front table: the next lookup is one probe.
        pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
            if self.hot.len() >= HOT {
                self.settle();
            }
            match self.hot.entry(key.clone()) {
                std::collections::hash_map::Entry::Occupied(e) => Some(e.into_mut()),
                std::collections::hash_map::Entry::Vacant(e) => Some(e.insert(self.base.get(key)?.clone())),
            }
        }

        pub fn insert_mut(&mut self, key: K, value: V) {
            if self.hot.len() >= HOT {
                self.settle();
            }
            self.hot.insert(key, value);
        }

        pub fn remove(&mut self, key: &K) {
            self.hot.remove(key);
            self.base.remove_mut(key);
        }

        /// Moves the front table into the trie.
        pub fn settle(&mut self) {
            for (k, v) in self.hot.drain() {
                self.base.insert_mut(k, v);
            }
        }

        pub fn size(&self) -> usize {
            self.base.size() + self.hot.keys().filter(|k| !self.base.contains_key(k)).count()
        }

        /// Every entry once, in no fixed order.
        pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
            self.hot.iter().chain(self.base.iter().filter(|(k, _)| !self.hot.contains_key(k)))
        }

        /// The trie with the front table in it: what the map was before it had one.
        fn whole(&self) -> std::borrow::Cow<'_, SharedMap<K, V>> {
            if self.hot.is_empty() {
                return std::borrow::Cow::Borrowed(&self.base);
            }
            let mut all = self.base.clone();
            for (k, v) in &self.hot {
                all.insert_mut(k.clone(), v.clone());
            }
            std::borrow::Cow::Owned(all)
        }
    }

    impl<K: Eq + std::hash::Hash + Clone, V: Clone> Clone for HotMap<K, V> {
        fn clone(&self) -> Self {
            HotMap { base: self.base.clone(), hot: self.hot.clone() }
        }
    }

    impl<K: Eq + std::hash::Hash, V> Default for HotMap<K, V> {
        fn default() -> Self {
            HotMap { base: SharedMap::default(), hot: Default::default() }
        }
    }

    impl<K: Eq + std::hash::Hash + Clone, V: Clone> FromIterator<(K, V)> for HotMap<K, V> {
        fn from_iter<I: IntoIterator<Item = (K, V)>>(entries: I) -> Self {
            HotMap { base: entries.into_iter().collect(), hot: Default::default() }
        }
    }

    impl<K: Eq + std::hash::Hash + Clone, V: Clone + PartialEq> PartialEq for HotMap<K, V> {
        fn eq(&self, other: &Self) -> bool {
            if self.hot.is_empty() && other.hot.is_empty() {
                return self.base == other.base;
            }
            self.size() == other.size() && self.iter().all(|(k, v)| other.get(k) == Some(v))
        }
    }

    impl<K: Eq + std::hash::Hash + Clone + serde::Serialize, V: Clone + serde::Serialize> serde::Serialize
        for HotMap<K, V>
    {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            self.whole().serialize(s)
        }
    }

    impl<'de, K: Eq + std::hash::Hash + Clone + serde::Deserialize<'de>, V: Clone + serde::Deserialize<'de>>
        serde::Deserialize<'de> for HotMap<K, V>
    {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            Ok(HotMap { base: serde::Deserialize::deserialize(d)?, hot: Default::default() })
        }
    }

    /// A mutex whose clone holds a copy of its value: an [`crate::analysis::machine::Analysis`] is cloned whole.
    #[derive(Default, Debug)]
    pub struct Mutex<T>(pub std::sync::Mutex<T>);

    impl<T: Clone> Clone for Mutex<T> {
        fn clone(&self) -> Self {
            Mutex(std::sync::Mutex::new(self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()))
        }
    }

    /// A vector whose copies share its chunks until one is written: a
    /// checkpoint of a run copies the bindings of every name.
    #[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
    #[serde(bound(serialize = "T: serde::Serialize + 'static", deserialize = "T: serde::Deserialize<'de> + 'static"))]
    pub struct Chunked<T>(#[serde(with = "shared::all")] Vec<std::rc::Rc<Vec<T>>>, usize);

    const CHUNK: usize = 1024;

    /// A `&'static str` field serde does not take for borrowed.
    pub type Str = &'static str;

    /// Serde for a `&'static str`: read back interned ([`crate::caches::format::leak`]).
    pub mod leaked {
        pub fn serialize<S: serde::Serializer>(t: &&'static str, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_str(t)
        }
        pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<&'static str, D::Error> {
            let t: std::borrow::Cow<'de, str> = serde::Deserialize::deserialize(d)?;
            Ok(crate::caches::format::leak(&t))
        }
    }

    /// [`leaked`] for pairs that end in one.
    pub mod leaked_pairs {
        pub fn serialize<S: serde::Serializer, K: serde::Serialize>(
            v: &[(K, super::Str)],
            s: S,
        ) -> Result<S::Ok, S::Error> {
            serde::Serialize::serialize(v, s)
        }
        pub fn deserialize<'de, D: serde::Deserializer<'de>, K: serde::Deserialize<'de>>(
            d: D,
        ) -> Result<Vec<(K, super::Str)>, D::Error> {
            let v: Vec<(K, String)> = serde::Deserialize::deserialize(d)?;
            Ok(v.into_iter().map(|(k, t)| (k, crate::caches::format::leak(&t))).collect())
        }
    }

    /// Serde through JSON text, for a type that needs a self-describing format.
    pub mod json {
        pub fn serialize<S: serde::Serializer, T: serde::Serialize>(t: &T, s: S) -> Result<S::Ok, S::Error> {
            s.serialize_str(&serde_json::to_string(t).map_err(serde::ser::Error::custom)?)
        }
        pub fn deserialize<'de, D: serde::Deserializer<'de>, T: serde::de::DeserializeOwned>(
            d: D,
        ) -> Result<T, D::Error> {
            let text: std::borrow::Cow<'de, str> = serde::Deserialize::deserialize(d)?;
            serde_json::from_str(&text).map_err(serde::de::Error::custom)
        }
    }

    /// Serde for an `Rc` written once inside [`shared::with`]: a later copy
    /// of the same pointer is its index. Outside, the value as serde writes it.
    pub mod shared {
        use serde::{Deserialize, Serialize};
        use std::any::Any;
        use std::cell::RefCell;
        use std::rc::Rc;

        type Table = (super::HashMap<usize, u32>, Vec<Rc<dyn Any>>);

        thread_local! {
            static TABLE: RefCell<Option<Table>> = const { RefCell::new(None) };
        }

        /// `f` with a table of its own.
        pub fn with<R>(f: impl FnOnce() -> R) -> R {
            let outer = TABLE.with_borrow_mut(|t| t.replace(Table::default()));
            let result = f();
            TABLE.with_borrow_mut(|t| *t = outer);
            result
        }

        #[derive(Serialize)]
        enum Out<'a, T> {
            New(&'a T),
            Seen(u32),
        }

        #[derive(Deserialize)]
        enum In<T> {
            New(T),
            Seen(u32),
        }

        pub fn serialize<S: serde::Serializer, T: Serialize + 'static>(rc: &Rc<T>, s: S) -> Result<S::Ok, S::Error> {
            let at = Rc::as_ptr(rc) as usize;
            match TABLE.with_borrow(|t| t.as_ref().map(|(ids, _)| ids.get(&at).copied())) {
                None => T::serialize(&**rc, s),
                Some(Some(id)) => Out::<T>::Seen(id).serialize(s),
                Some(None) => {
                    let out = Out::New(&**rc).serialize(s)?;
                    // Numbered once written: what it holds is numbered first, as on reading.
                    TABLE.with_borrow_mut(|t| t.as_mut().map(|(ids, _)| ids.insert(at, ids.len() as u32)));
                    Ok(out)
                }
            }
        }

        pub fn deserialize<'de, D: serde::Deserializer<'de>, T: Deserialize<'de> + 'static>(
            d: D,
        ) -> Result<Rc<T>, D::Error> {
            if TABLE.with_borrow(Option::is_none) {
                return T::deserialize(d).map(Rc::new);
            }
            match In::<T>::deserialize(d)? {
                In::New(t) => {
                    let rc = Rc::new(t);
                    TABLE.with_borrow_mut(|t| t.as_mut().map(|(_, all)| all.push(rc.clone())));
                    Ok(rc)
                }
                In::Seen(id) => TABLE
                    .with_borrow(|t| t.as_ref().and_then(|(_, all)| all.get(id as usize).cloned()))
                    .and_then(|rc| rc.downcast::<T>().ok())
                    .ok_or_else(|| serde::de::Error::custom("a shared value out of order")),
            }
        }

        /// [`shared`] for each of a list.
        pub mod all {
            use serde::{Deserialize, Serialize};
            use std::rc::Rc;

            struct Item<T: 'static>(Rc<T>);

            impl<T: Serialize + 'static> Serialize for Item<T> {
                fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                    super::serialize(&self.0, s)
                }
            }

            impl<'de, T: Deserialize<'de> + 'static> Deserialize<'de> for Item<T> {
                fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                    super::deserialize(d).map(Item)
                }
            }

            pub fn serialize<S: serde::Serializer, T: Serialize + 'static>(
                v: &[Rc<T>],
                s: S,
            ) -> Result<S::Ok, S::Error> {
                s.collect_seq(v.iter().map(|rc| Item(rc.clone())))
            }

            pub fn deserialize<'de, D: serde::Deserializer<'de>, T: Deserialize<'de> + 'static>(
                d: D,
            ) -> Result<Vec<Rc<T>>, D::Error> {
                Ok(Vec::<Item<T>>::deserialize(d)?.into_iter().map(|i| i.0).collect())
            }
        }
    }

    /// Serde for arrays longer than serde's 32.
    pub mod array {
        pub fn serialize<S: serde::Serializer, T: serde::Serialize, const N: usize>(
            a: &[T; N],
            s: S,
        ) -> Result<S::Ok, S::Error> {
            serde::Serialize::serialize(&a[..], s)
        }
        pub fn deserialize<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>, const N: usize>(
            d: D,
        ) -> Result<[T; N], D::Error> {
            let v: Vec<T> = serde::Deserialize::deserialize(d)?;
            v.try_into().map_err(|_| serde::de::Error::custom("array length"))
        }
    }

    impl<T> Default for Chunked<T> {
        fn default() -> Self {
            Chunked(Vec::new(), 0)
        }
    }

    impl<T: Clone> Chunked<T> {
        /// `n` times `value`.
        pub fn filled(n: usize, value: T) -> Self {
            let mut out = Chunked::default();
            out.resize(n, value);
            out
        }

        pub fn len(&self) -> usize {
            self.1
        }

        /// Whether every element is `eq` to `other`'s. Chunks both still share are skipped.
        pub fn alike(&self, other: &Self, eq: impl Fn(&T, &T) -> bool) -> bool {
            self.1 == other.1
                && self
                    .0
                    .iter()
                    .zip(&other.0)
                    .all(|(a, b)| std::rc::Rc::ptr_eq(a, b) || a.iter().zip(b.iter()).all(|(x, y)| eq(x, y)))
        }

        /// Where the elements are not `eq` to `other`'s, in order; a longer one's rest too.
        /// Chunks both still share are skipped.
        pub fn differing(&self, other: &Self, eq: impl Fn(Option<&T>, Option<&T>) -> bool) -> Vec<usize> {
            let mut out = Vec::new();
            for c in 0..self.0.len().max(other.0.len()) {
                let (a, b) = (self.0.get(c), other.0.get(c));
                if matches!((a, b), (Some(a), Some(b)) if std::rc::Rc::ptr_eq(a, b)) {
                    continue;
                }
                let chunk = (0..CHUNK).filter(|&i| !eq(a.and_then(|x| x.get(i)), b.and_then(|x| x.get(i))));
                out.extend(chunk.map(|i| c * CHUNK + i));
            }
            out
        }

        pub fn is_empty(&self) -> bool {
            self.1 == 0
        }

        pub fn get(&self, i: usize) -> Option<&T> {
            (i < self.1).then(|| &self.0[i / CHUNK][i % CHUNK])
        }

        /// As `slice::partition_point`: the first index where `pred` stops holding (the list is partitioned by it).
        pub fn partition_point(&self, mut pred: impl FnMut(&T) -> bool) -> usize {
            let (mut lo, mut hi) = (0, self.1);
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                if pred(&self[mid]) { lo = mid + 1 } else { hi = mid }
            }
            lo
        }

        /// Grows to `n` with `value`; never shrinks.
        pub fn resize(&mut self, n: usize, value: T) {
            while self.1 < n {
                if self.1.is_multiple_of(CHUNK) {
                    self.0.push(std::rc::Rc::new(Vec::with_capacity(CHUNK)));
                }
                let last = std::rc::Rc::make_mut(self.0.last_mut().expect("pushed"));
                let more = (CHUNK - last.len()).min(n - self.1);
                last.resize(last.len() + more, value.clone());
                self.1 += more;
            }
        }

        pub fn iter(&self) -> Iter<'_, T> {
            self.iter_from(0)
        }

        /// The last element, if any.
        pub fn last(&self) -> Option<&T> {
            self.1.checked_sub(1).map(|i| &self[i])
        }

        pub fn get_mut(&mut self, i: usize) -> Option<&mut T> {
            (i < self.1).then(|| &mut self[i])
        }

        /// Appends; a chunk a copy still shares is copied first (one chunk at most).
        pub fn push(&mut self, value: T) {
            if self.1.is_multiple_of(CHUNK) {
                self.0.push(std::rc::Rc::new(Vec::new()));
            }
            std::rc::Rc::make_mut(self.0.last_mut().expect("pushed")).push(value);
            self.1 += 1;
        }

        pub fn pop(&mut self) -> Option<T> {
            let last = std::rc::Rc::make_mut(self.0.last_mut()?);
            let value = last.pop();
            if last.is_empty() {
                self.0.pop();
            }
            self.1 -= 1;
            value
        }

        /// Keeps the first `n`; chunks past it are dropped, not copied.
        pub fn truncate(&mut self, n: usize) {
            if n >= self.1 {
                return;
            }
            self.0.truncate(n.div_ceil(CHUNK));
            let keep = n - (n - 1) / CHUNK * CHUNK;
            if let Some(last) = self.0.last_mut().filter(|c| c.len() > keep) {
                std::rc::Rc::make_mut(last).truncate(keep);
            }
            self.1 = n;
        }

        pub fn swap(&mut self, a: usize, b: usize) {
            if a != b {
                let y = self[b].clone();
                let x = std::mem::replace(&mut self[a], y);
                self[b] = x;
            }
        }

        pub fn clear(&mut self) {
            *self = Chunked::default();
        }

        /// Whether `prefix`'s elements are this one's first ones. Chunks both share are equal unread.
        pub(crate) fn starts_with(&self, prefix: &Self) -> bool
        where
            T: PartialEq,
        {
            let whole = prefix.1 / CHUNK;
            prefix.1 <= self.1
                && self.0.iter().zip(&prefix.0[..whole]).all(|(a, b)| std::rc::Rc::ptr_eq(a, b) || a == b)
                && self.iter_from(whole * CHUNK).zip(prefix.iter_from(whole * CHUNK)).all(|(a, b)| a == b)
        }

        /// The elements from `from` on, in order.
        pub fn iter_from(&self, from: usize) -> Iter<'_, T> {
            Iter(self, from.min(self.1)..self.1)
        }

        /// Every element, writable: every chunk is unshared first.
        pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut T> {
            self.iter_mut_from(0)
        }

        /// The elements from `from` on, writable: only their chunks are unshared.
        pub fn iter_mut_from(&mut self, from: usize) -> impl Iterator<Item = &mut T> {
            let chunks = self.0.get_mut(from / CHUNK..).unwrap_or_default();
            chunks.iter_mut().flat_map(|c| std::rc::Rc::make_mut(c).iter_mut()).skip(from % CHUNK)
        }
    }

    impl<T: PartialEq> PartialEq for Chunked<T> {
        fn eq(&self, other: &Self) -> bool {
            self.1 == other.1 && self.0.iter().zip(&other.0).all(|(a, b)| std::rc::Rc::ptr_eq(a, b) || a == b)
        }
    }

    impl<T: Clone> FromIterator<T> for Chunked<T> {
        fn from_iter<I: IntoIterator<Item = T>>(items: I) -> Self {
            let mut out = Chunked::default();
            out.extend(items);
            out
        }
    }

    impl<T: Clone> Extend<T> for Chunked<T> {
        fn extend<I: IntoIterator<Item = T>>(&mut self, items: I) {
            for x in items {
                self.push(x);
            }
        }
    }

    impl<T: Clone> From<Vec<T>> for Chunked<T> {
        fn from(v: Vec<T>) -> Self {
            v.into_iter().collect()
        }
    }

    /// The elements of a [`Chunked`] in a range of positions.
    #[derive(Clone)]
    pub struct Iter<'a, T>(&'a Chunked<T>, std::ops::Range<usize>);

    impl<'a, T> Iterator for Iter<'a, T> {
        type Item = &'a T;
        fn next(&mut self) -> Option<&'a T> {
            self.1.next().map(|i| &self.0.0[i / CHUNK][i % CHUNK])
        }
        fn size_hint(&self) -> (usize, Option<usize>) {
            self.1.size_hint()
        }
        fn nth(&mut self, n: usize) -> Option<&'a T> {
            self.1.nth(n).map(|i| &self.0.0[i / CHUNK][i % CHUNK])
        }
    }

    impl<T> DoubleEndedIterator for Iter<'_, T> {
        fn next_back(&mut self) -> Option<Self::Item> {
            self.1.next_back().map(|i| &self.0.0[i / CHUNK][i % CHUNK])
        }
    }

    impl<T> ExactSizeIterator for Iter<'_, T> {}

    impl<'a, T: Clone> IntoIterator for &'a Chunked<T> {
        type Item = &'a T;
        type IntoIter = Iter<'a, T>;
        fn into_iter(self) -> Iter<'a, T> {
            self.iter()
        }
    }

    /// A [`HotMap`] is the trie it fronts: every lookup, size, iteration, comparison and
    /// encoding agrees with a plain [`SharedMap`] given the same writes, across settles and
    /// copies. Pins the per-call site tables: a stale trie entry shadowing a newer front one
    /// would bring back a site's old repeat count, and the format cache's bytes would change.
    #[cfg(test)]
    #[test]
    fn hot_map_is_the_trie_it_fronts() {
        let (mut hot, mut plain): (HotMap<u32, u64>, SharedMap<u32, u64>) = Default::default();
        let mut copies = Vec::new();
        for i in 0..3 * HOT as u64 + 7 {
            let key = (i.wrapping_mul(2_654_435_761) % (HOT as u64 + 300)) as u32;
            match i % 5 {
                0 => {
                    hot.insert_mut(key, i);
                    plain.insert_mut(key, i);
                }
                1 => {
                    if let Some(v) = hot.get_mut(&key) {
                        *v += 1;
                    }
                    if let Some(v) = plain.get_mut(&key) {
                        *v += 1;
                    }
                }
                2 if i % 7919 == 2 => {
                    copies.push((hot.clone(), plain.clone()));
                    hot.settle();
                }
                _ => assert_eq!(hot.get(&key), plain.get(&key), "lookup {key}"),
            }
        }
        copies.push((hot, plain));
        for (hot, plain) in &copies {
            assert_eq!(hot.size(), plain.size());
            assert_eq!(hot.iter().count(), plain.size(), "each entry once");
            assert!(hot.iter().all(|(k, v)| plain.get(k) == Some(v)));
            assert!(*hot == hot.iter().map(|(k, v)| (*k, *v)).collect::<HotMap<_, _>>());
            assert_eq!(postcard::to_allocvec(hot).expect("encodes"), postcard::to_allocvec(plain).expect("encodes"));
        }
    }

    /// A [`Chunked`] behaves as a `Vec` would, and a copy shares the chunks it does not write.
    /// Pins the record lists checkpoints share: a wrong `truncate` or `pop` near a chunk border
    /// would drop or keep one record, and `flat` must write a `Vec`'s bytes, else cache files change.
    #[cfg(test)]
    #[test]
    fn chunked_is_a_vec_that_copies_share() {
        let n = 3 * CHUNK + 5;
        let (mut c, mut v): (Chunked<usize>, Vec<usize>) = (Chunked::default(), Vec::new());
        for i in 0..n {
            c.push(i);
            v.push(i);
        }
        let copy = c.clone();
        for cut in [n - 1, 2 * CHUNK + 1, 2 * CHUNK, CHUNK - 1, 0] {
            c.truncate(cut);
            v.truncate(cut);
            assert!(c.iter().eq(v.iter()) && c.len() == v.len(), "truncate {cut}");
        }
        assert!(copy.iter().copied().eq(0..n), "the copy keeps every element");
        let mut c = copy.clone();
        c[CHUNK + 1] = 0;
        assert!(std::rc::Rc::ptr_eq(&c.0[0], &copy.0[0]) && !std::rc::Rc::ptr_eq(&c.0[1], &copy.0[1]));
        assert_eq!(c.pop(), Some(n - 1));
        assert_eq!(c.iter_from(n - 3).copied().collect::<Vec<_>>(), vec![n - 3, n - 2]);
        assert_eq!(c.iter().rev().len(), n - 1);
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Flat(#[serde(with = "flat")] Chunked<usize>);
        let plain: Vec<usize> = copy.iter().copied().collect();
        let bytes = postcard::to_allocvec(&Flat(copy)).expect("encodes");
        assert_eq!(bytes, postcard::to_allocvec(&plain).expect("encodes"));
        let back: Flat = postcard::from_bytes(&bytes).expect("decodes");
        assert!(back.0.iter().eq(plain.iter()));
    }

    /// Cutting a list back to a chunk border, or past the end, leaves the chunks a copy
    /// shares shared: a checkpoint's rewind would otherwise copy a full chunk for nothing.
    /// And a log kept in a `RefCell` (`Env`'s read log) writes a `Vec`'s bytes too.
    #[cfg(test)]
    #[test]
    fn chunked_truncate_keeps_untouched_chunks_shared() {
        let mut c: Chunked<u32> = (0..2 * CHUNK as u32).collect();
        let copy = c.clone();
        c.truncate(CHUNK);
        c.truncate(5 * CHUNK);
        assert!(std::rc::Rc::ptr_eq(&c.0[0], &copy.0[0]), "a full chunk kept whole stays shared");
        assert_eq!(c.len(), CHUNK);
        assert!(c.iter_from(CHUNK - 2).copied().eq([CHUNK as u32 - 2, CHUNK as u32 - 1]));
        #[derive(serde::Serialize, serde::Deserialize)]
        struct Log(#[serde(with = "flat::cell")] std::cell::RefCell<Chunked<(u32, u64)>>);
        let log = Log(std::cell::RefCell::new((0..CHUNK as u32 + 3).map(|i| (i, 2 * i as u64)).collect()));
        let plain: Vec<(u32, u64)> = log.0.borrow().iter().copied().collect();
        let bytes = postcard::to_allocvec(&log).expect("encodes");
        assert_eq!(bytes, postcard::to_allocvec(&plain).expect("encodes"));
        let back: Log = postcard::from_bytes(&bytes).expect("decodes");
        assert!(back.0.borrow().iter().eq(plain.iter()));
    }

    /// Serde for a [`Chunked`] as the plain sequence a `Vec` writes: a record
    /// list that checkpoints share is stored as it was before.
    pub mod flat {
        pub fn serialize<S: serde::Serializer, T: serde::Serialize + Clone>(
            v: &super::Chunked<T>,
            s: S,
        ) -> Result<S::Ok, S::Error> {
            s.collect_seq(v.iter())
        }
        pub fn deserialize<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de> + Clone>(
            d: D,
        ) -> Result<super::Chunked<T>, D::Error> {
            struct Seq<T>(std::marker::PhantomData<T>);
            impl<'de, T: serde::Deserialize<'de> + Clone> serde::de::Visitor<'de> for Seq<T> {
                type Value = super::Chunked<T>;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("a sequence")
                }
                fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                    let mut out = super::Chunked::default();
                    while let Some(x) = seq.next_element()? {
                        out.push(x);
                    }
                    Ok(out)
                }
            }
            d.deserialize_seq(Seq(std::marker::PhantomData))
        }

        /// `flat` inside a `RefCell`.
        pub mod cell {
            use std::cell::RefCell;
            pub fn serialize<S: serde::Serializer, T: serde::Serialize + Clone>(
                v: &RefCell<super::super::Chunked<T>>,
                s: S,
            ) -> Result<S::Ok, S::Error> {
                super::serialize(&v.borrow(), s)
            }
            pub fn deserialize<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de> + Clone>(
                d: D,
            ) -> Result<RefCell<super::super::Chunked<T>>, D::Error> {
                super::deserialize(d).map(RefCell::new)
            }
        }
    }

    impl<T> std::ops::Index<usize> for Chunked<T> {
        type Output = T;
        fn index(&self, i: usize) -> &T {
            assert!(i < self.1, "index {i} past {}", self.1);
            &self.0[i / CHUNK][i % CHUNK]
        }
    }

    impl<T: Clone> std::ops::IndexMut<usize> for Chunked<T> {
        fn index_mut(&mut self, i: usize) -> &mut T {
            assert!(i < self.1, "index {i} past {}", self.1);
            &mut std::rc::Rc::make_mut(&mut self.0[i / CHUNK])[i % CHUNK]
        }
    }

    /// Per slot, the assignment its binding came from ([`crate::engine::env::Env`]),
    /// with an upper bound of each chunk's newest stamp: what was written after
    /// a mark is found without visiting every slot. Stored as the plain stamps.
    #[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
    #[serde(from = "Chunked<u64>", into = "Chunked<u64>")]
    pub struct Stamps {
        all: Chunked<u64>,
        /// Per chunk, at least its largest stamp (a restore may lower a stamp, never this).
        newest: Vec<u64>,
    }

    impl From<Chunked<u64>> for Stamps {
        fn from(all: Chunked<u64>) -> Self {
            let newest = all.0.iter().map(|c| c.iter().copied().max().unwrap_or(0)).collect();
            Stamps { all, newest }
        }
    }

    impl From<Stamps> for Chunked<u64> {
        fn from(s: Stamps) -> Self {
            s.all
        }
    }

    impl Stamps {
        pub fn filled(n: usize) -> Self {
            Chunked::filled(n, 0).into()
        }

        pub fn len(&self) -> usize {
            self.all.len()
        }

        pub fn is_empty(&self) -> bool {
            self.all.is_empty()
        }

        pub fn get(&self, i: usize) -> Option<&u64> {
            self.all.get(i)
        }

        pub fn resize(&mut self, n: usize) {
            self.all.resize(n, 0);
            self.newest.resize(self.all.0.len(), 0);
        }

        /// Sets slot `i` to `stamp` and returns the old one.
        pub fn set(&mut self, i: usize, stamp: u64) -> u64 {
            let newest = &mut self.newest[i / CHUNK];
            *newest = (*newest).max(stamp);
            std::mem::replace(&mut self.all[i], stamp)
        }

        /// The slots stamped after `mark`, ascending.
        pub fn after(&self, mark: u64) -> impl Iterator<Item = usize> + '_ {
            self.all.0.iter().zip(&self.newest).enumerate().filter(move |&(_, (_, &n))| n > mark).flat_map(
                move |(c, (chunk, _))| {
                    chunk.iter().enumerate().filter(move |&(_, &at)| at > mark).map(move |(i, _)| c * CHUNK + i)
                },
            )
        }
    }

    impl std::ops::Index<usize> for Stamps {
        type Output = u64;
        fn index(&self, i: usize) -> &u64 {
            &self.all[i]
        }
    }

    impl<T> std::ops::Deref for Mutex<T> {
        type Target = std::sync::Mutex<T>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
}

#[cfg(test)]
mod stamps_tests {
    use crate::fixed::{Chunked, Stamps};

    /// `after` skips chunks by their newest stamp: it must find exactly what a
    /// scan of every slot finds, through restores to older stamps, growth and a
    /// cache round trip (which stores only the stamps).
    #[test]
    fn after_finds_what_a_full_scan_finds() {
        let mut stamps = Stamps::filled(3000);
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        let mut clock = 0;
        for round in 0..4000 {
            if round == 2000 {
                stamps.resize(5000);
            }
            let i = (next() % stamps.len() as u64) as usize;
            if next() % 4 == 0 {
                // a group end puts back an older stamp
                let old = stamps[i];
                stamps.set(i, old / 2);
            } else {
                clock += 1;
                stamps.set(i, clock);
            }
            if round % 97 == 0 {
                let back: Stamps = postcard::from_bytes(&postcard::to_stdvec(&stamps).unwrap()).unwrap();
                for s in [&stamps, &back] {
                    for mark in [0, clock / 3, clock.saturating_sub(5), clock] {
                        let scan: Vec<usize> = (0..s.len()).filter(|&i| s[i] > mark).collect();
                        assert_eq!(s.after(mark).collect::<Vec<_>>(), scan, "mark {mark}");
                    }
                }
                // stored as the plain stamps: caches written before read the same
                let plain: Chunked<u64> = stamps.clone().into();
                assert_eq!(postcard::to_stdvec(&plain).unwrap(), postcard::to_stdvec(&stamps).unwrap());
            }
        }
    }
}
