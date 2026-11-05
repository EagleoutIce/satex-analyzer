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
pub mod linter;
pub mod lsp;
pub mod oracle;
pub mod plugins;
pub mod project;
pub mod queries;

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
    pub type Hasher = std::hash::BuildHasherDefault<std::collections::hash_map::DefaultHasher>;
    pub type HashMap<K, V> = std::collections::HashMap<K, V, Hasher>;
    pub type HashSet<K> = std::collections::HashSet<K, Hasher>;
    /// For tables that are only looked up, or whose iteration is sorted
    /// before use: no order of theirs is ever seen, so a fast hash will do.
    pub type LookupMap<K, V> = std::collections::HashMap<K, V, crate::engine::tex::NameHash>;
    pub type LookupSet<K> = std::collections::HashSet<K, crate::engine::tex::NameHash>;

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

        pub fn is_empty(&self) -> bool {
            self.1 == 0
        }

        pub fn get(&self, i: usize) -> Option<&T> {
            (i < self.1).then(|| &self.0[i / CHUNK][i % CHUNK])
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

        pub fn iter(&self) -> impl Iterator<Item = &T> {
            self.0.iter().flat_map(|c| c.iter())
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

    impl<T> std::ops::Deref for Mutex<T> {
        type Target = std::sync::Mutex<T>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
}
