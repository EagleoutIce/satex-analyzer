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
//! use satex::config::Config;
//! use satex::machine::Machine;
//!
//! let source = r"\documentclass{article}\newcommand\hi[1]{Hi #1}\begin{document}\hi{you}\end{document}";
//! let analysis = Machine::analyze(source, None, &Config::default());
//! for def in &analysis.facts.defs {
//!     println!("{} at {}", analysis.interner.cs(def.name), def.span);
//! }
//! for finding in satex::lint::lint(&analysis) {
//!     println!("{}", finding["message"]);
//! }
//! ```
//!
//! Entry points: [`machine::Machine::analyze`] runs a document,
//! [`query`] answers questions about the result, [`lint`] reports findings,
//! [`config::Config`] holds every setting of `satex.yaml`.

#[macro_use]
mod names;
pub mod budget;
pub mod builtins;
pub mod cmd;
pub mod commands;
pub mod config;
pub mod distribution;
pub mod env;
pub mod exec;
pub mod exports;
pub mod facts;
pub mod format;
pub mod graph;
pub mod hyph;
pub mod linebreak;
pub mod lint;
pub mod literate;
pub mod loader;
pub mod lsp;
pub mod machine;
pub mod mode;
pub mod observe;
pub mod oracle;
pub mod otf;
pub mod overlay;
pub mod paths;
pub mod pdfbox;
pub mod plugin;
pub mod probe;
pub mod progress;
pub mod project;
pub mod query;
pub mod rename;
pub mod render;
pub mod scan;
pub mod source;
pub mod tex;
pub mod tfm;
pub mod timing;
pub mod value;

pub use config::Config;
pub use machine::{Analysis, Machine};

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
    pub type LookupMap<K, V> = std::collections::HashMap<K, V, crate::tex::NameHash>;
    pub type LookupSet<K> = std::collections::HashSet<K, crate::tex::NameHash>;
}
