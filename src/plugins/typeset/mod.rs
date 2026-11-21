//! Typesetting plugins: TeX's own algorithms run again on text the last run
//! did not set, for the playground's echo of typed text. Each reads the run
//! only through [`Facts`], the state it ended in: parameters, codes, fonts and
//! hyphenation tables. `plugins.typeset.<name>` in `satex.yaml` switches each.

pub mod hyphenate;
pub mod linebreak;
pub mod lines;
pub mod mathwidths;

use crate::analysis::machine::Analysis;
use crate::engine::builtins::CodeTable;
use crate::engine::hyph::HyphTables;
use crate::engine::linebreak::Tables;
use crate::engine::tfm::Metrics;
use crate::engine::value::{Glue, Scaled, Value};
use crate::project::config::TypesetSwitches;
use serde_json::Value as Json;
use std::rc::Rc;

/// One plugin of the collection.
pub struct Plugin {
    /// Its key under `plugins.typeset`.
    pub name: &'static str,
    /// The language server request it answers.
    pub request: &'static str,
    pub about: &'static str,
    pub enabled: fn(&TypesetSwitches) -> bool,
    /// Its answer to the request's params, for a document at `path`.
    pub answer: fn(&mut Facts, &str, &Json) -> Json,
}

pub const PLUGINS: [Plugin; 3] = [
    Plugin {
        name: "hyphenate",
        request: "satex/hyphenate",
        about: "where a word may take a hyphen: the run's patterns, exceptions, hyphen mins and `\\lccode`s",
        enabled: |s| s.hyphenate,
        answer: |facts, _, params| {
            let words: Vec<String> = serde_json::from_value(params["words"].clone()).unwrap_or_default();
            serde_json::to_value(hyphenate::points(facts, &words)).unwrap_or_default()
        },
    },
    Plugin {
        name: "linebreak",
        request: "satex/break",
        about: "TeX's line breaking of a paragraph's words: the run's font metrics and parameters",
        enabled: |s| s.linebreak,
        answer: linebreak::answer,
    },
    Plugin {
        name: "mathwidths",
        request: "satex/mathwidths",
        about: "math symbols as the run's math fonts set them: their math codes, family fonts and metrics",
        enabled: |s| s.mathwidths,
        answer: |facts, _, params| mathwidths::answer(facts, params),
    },
];

/// `method`'s answer from a finished run of the document at `path`: `null`
/// when its plugin is off.
pub fn answer(analysis: &Analysis, method: &str, path: &str, params: &Json) -> Json {
    let on = |p: &&Plugin| p.request == method && (p.enabled)(&analysis.settings.plugins.typeset);
    PLUGINS.iter().find(on).map_or(Json::Null, |p| (p.answer)(&mut Facts::of(analysis), path, params))
}

/// A finished run's state, read only: what a typesetting plugin may see.
pub struct Facts<'a> {
    pub analysis: &'a Analysis,
    fonts: Fonts<'a>,
    tables: Option<Option<HyphTables>>,
}

impl<'a> Facts<'a> {
    pub fn of(analysis: &'a Analysis) -> Facts<'a> {
        Facts { analysis, fonts: Fonts::of(analysis), tables: None }
    }

    /// A value as the run left it: `None` unbound.
    fn value(&self, name: &str) -> Option<Value> {
        let a = self.analysis;
        a.interner.lookup(name).filter(|&s| a.env.get(s).is_some()).map(|s| a.env.value(s))
    }

    /// An engine slot ([`crate::engine::commands::ENGINE`]).
    pub(crate) fn engine_value(&self, key: &str) -> Option<Value> {
        self.engine(key)
    }

    fn engine(&self, key: &str) -> Option<Value> {
        self.value(&format!("{}{key}", crate::engine::commands::ENGINE))
    }

    /// An integer parameter, 0 never set (tex.web § 240), `None` unknown.
    pub fn int(&self, name: &str) -> Option<i64> {
        self.value(name).map_or(Some(0), |v| v.as_int())
    }

    /// A glue parameter, zero never set.
    pub fn glue(&self, name: &str) -> Option<Glue> {
        match self.value(name) {
            None => Some(Glue::default()),
            Some(Value::Glue(g)) => Some(g),
            Some(_) => None,
        }
    }

    pub fn dimen(&self, name: &str) -> Option<Scaled> {
        self.value(name).map_or(Some(0), |v| v.as_dimen())
    }

    /// A character's code in `table`; `None` once an unpinned write may have reached it.
    pub fn code(&self, table: CodeTable, c: char) -> Option<i64> {
        let code = crate::engine::exec::CODE;
        if self.value(&format!("{code}{}{code}any", table.as_str())).is_some() {
            return None;
        }
        match self.value(&format!("{code}{}{c}", table.as_str())) {
            Some(v) => v.as_int(),
            None => Some(table.default_for(c)),
        }
    }

    /// A font's `\fontdimen` `k` (tex.web § 547), from its metrics.
    pub fn font_dimen(&mut self, font: u32, k: usize) -> Option<Scaled> {
        if let Some(v) = self.engine(&format!("font{font}.p{k}")) {
            return v.as_dimen();
        }
        let (m, z) = self.metrics(font)?;
        m.param(k, z)
    }
}

impl Tables for Facts<'_> {
    fn lower(&mut self, c: u32) -> Option<i64> {
        self.code(CodeTable::Lower, char::from_u32(c)?)
    }

    fn hyphen_char(&mut self, font: u32) -> Option<i64> {
        self.engine(&format!("font{font}.hyphenchar"))?.as_int()
    }

    fn metrics(&mut self, font: u32) -> Option<(Rc<Metrics>, Scaled)> {
        self.fonts.metrics(u16::try_from(font).ok()?)
    }

    /// The language in force at the run's end: its `\patterns` and `\hyphenation`.
    fn patterns(&mut self) -> Option<HyphTables> {
        if self.tables.is_none() {
            let language = match self.value("language") {
                Some(Value::Unknown) => None,
                v => Some(v.and_then(|v| v.as_int()).filter(|l| (1..=255).contains(l)).unwrap_or(0)),
            }
            .filter(|_| self.engine("hyph.lost").is_none());
            let text = |key: &str| match self.engine(key) {
                Some(Value::Toks(t)) => Some(t),
                Some(_) => None,
                None => Some(Rc::from(Vec::new())),
            };
            let tables = language.and_then(|l| {
                // One request hyphenates a paragraph once: no words to keep.
                let p = crate::engine::hyph::parse_patterns(&text(&format!("patterns.{l}"))?, 0);
                Some((Rc::new(p), crate::engine::hyph::parse_exceptions(&text(&format!("hyphenation.{l}"))?)))
            });
            self.tables = Some(tables);
        }
        self.tables.clone().flatten()
    }

    fn mins(&mut self) -> (usize, usize) {
        let min = |name: &str| self.int(name).map_or(1, |v| v.clamp(1, 63)) as usize;
        (min("lefthyphenmin"), min("righthyphenmin"))
    }
}

/// The metrics of the fonts the run set characters in, each TFM read once.
pub(crate) struct Fonts<'a> {
    analysis: &'a Analysis,
    base: std::path::PathBuf,
    read: crate::fixed::HashMap<u16, Option<(Rc<Metrics>, Scaled)>>,
}

impl<'a> Fonts<'a> {
    pub(crate) fn of(analysis: &'a Analysis) -> Fonts<'a> {
        let base = crate::project::paths::dir_of(std::path::Path::new(analysis.file_name(analysis.main_file)));
        Fonts { analysis, base, read: Default::default() }
    }

    /// The TFM the engine had loaded for a font state, and the size it was
    /// loaded at.
    pub(crate) fn metrics(&mut self, font: u16) -> Option<(Rc<Metrics>, Scaled)> {
        let (analysis, base) = (self.analysis, &self.base);
        self.read
            .entry(font)
            .or_insert_with(|| {
                let used = analysis.fonts.get(usize::from(font))?.as_ref()?;
                let path = crate::engine::tfm::locate(base, &analysis.distribution.roots, &used.name, "tfm")?;
                Some((crate::engine::tfm::load(&path)?, used.size))
            })
            .clone()
    }

    /// The italic correction the font gives a character, which is the kern
    /// `\/` appends after it (tex.web § 1113).
    pub(crate) fn italic(&mut self, state: &crate::analysis::facts::FontState, c: char) -> Option<Scaled> {
        let (metrics, size) = self.metrics(state.font)?;
        Some(metrics.char_box(c as u32, size)?.italic)
    }

    pub(crate) fn name(&self, state: &crate::analysis::facts::FontState) -> String {
        match self.analysis.fonts.get(usize::from(state.font)).and_then(Option::as_ref) {
            Some(used) => format!("{} ({})", state.nfss(), used.name),
            None => state.nfss(),
        }
    }
}
