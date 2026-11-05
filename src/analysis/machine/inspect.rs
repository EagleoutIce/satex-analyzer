//! Dev mode (`trace_state`): each trace event carries the abstract state
//! the step ran in, so a wrong result can be traced to the domain that
//! lost precision. Built only when asked: a run without it pays one test.

use super::{Frame, Machine};
use crate::analysis::roles::Ty;
use crate::engine::tex::{Meaning, RegKind, Sym, UNNUMBERED};
use crate::engine::value::{Quantity, Value};
use serde_json::{Value as Json, json};

/// The most characters one entry of a snapshot shows.
pub(super) const TEXT: usize = 240;

fn clip(s: impl AsRef<str>) -> Json {
    json!(s.as_ref().chars().take(TEXT).collect::<String>())
}

/// A value's place in the `Ty` lattice and what the domain knows of it:
/// exact, an interval, or a set.
fn value(v: &Value) -> Json {
    let ty = match v {
        Value::Int(_) | Value::Num { quantity: Quantity::Int, .. } => Ty::Number,
        Value::Dimen(_) | Value::Num { quantity: Quantity::Dimen, .. } => Ty::Dimen,
        Value::Glue(_) | Value::Num { quantity: Quantity::Glue, .. } => Ty::Glue,
        Value::MuGlue(_) | Value::Num { quantity: Quantity::MuGlue, .. } => Ty::MuGlue,
        Value::GlueNum { .. } => Ty::Quantity,
        Value::Toks(_) => Ty::Text,
        _ => Ty::Top,
    };
    let known = match v {
        Value::Num { num, .. } => json!({ "interval": [num.lo, num.hi] }),
        Value::OneOf(set) => json!({ "set": set.iter().map(Value::render).collect::<Vec<_>>() }),
        Value::Unknown => json!({ "unknown": true }),
        Value::GlueNum { .. } | Value::List(..) => json!({ "abstract": clip(format!("{v:?}")) }),
        v => json!({ "exact": v.render() }),
    };
    let mut out = json!({ "type": ty.to_string() });
    out.as_object_mut().into_iter().zip(known.as_object()).for_each(|(o, k)| o.extend(k.clone()));
    out
}

impl Machine<'_> {
    /// `\meaning` of one member of a meaning set, as [`Machine::meaning_text`] shows a name's.
    fn meaning_member(&self, m: &Meaning) -> String {
        match m {
            Meaning::Macro(def) => {
                self.macro_head(def, Some('\\'))
                    + &crate::engine::tex::detokenize(&def.replacement_text, &self.out.interner)
            }
            Meaning::Register(kind, i) if *i != UNNUMBERED => format!("\\{}{i}", kind.as_str()),
            other => other.kind().into(),
        }
    }

    /// One name's binding in every domain that holds it.
    fn binding(&mut self, sym: Sym) -> Json {
        let name = self.out.interner.cs(sym);
        let Some(b) = self.env.slot(sym).cloned() else { return json!({ "name": name, "meaning": "undefined" }) };
        let mut out = json!({
            "name": name,
            "meaning": clip(self.meaning_member(&b.meaning)),
            "certain": b.certain,
            "global": b.global,
        });
        if let Some(may) = &b.may {
            out["may"] = may.iter().map(|m| clip(self.meaning_member(m))).collect();
            out["maybe_undefined"] = json!(may.iter().any(|m| matches!(m, Meaning::Undefined)));
        }
        let store = match b.meaning {
            Meaning::Register(
                kind @ (RegKind::Count | RegKind::Dimen | RegKind::Skip | RegKind::MuSkip | RegKind::Toks),
                i,
            ) if i != UNNUMBERED => self.register_sym(kind, i64::from(i)),
            _ => sym,
        };
        let v = self.env.slot(store).map(|b| b.value.clone()).unwrap_or_default();
        if !matches!(v, Value::Unknown) || store != sym {
            out["value"] = value(&v);
        }
        out
    }

    /// The abstract state at this step: paths, stacks, mode, and the
    /// bindings of `names` (the step's own and those ahead), capped at
    /// `limits.trace_state` entries per list. `text` is the file text ahead.
    pub(super) fn snapshot(&mut self, names: &[Sym], text: &str) -> Json {
        let cap = self.cfg.limits.trace_state;
        let at = |m: &Self, s: crate::engine::tex::Span| format!("{}:{}:{}", m.out.short_name(s.file), s.line, s.col);
        let (paired, rows) = self.env.paired();
        let mut wanted: Vec<Sym> = Vec::new();
        for &s in names.iter().chain(&paired) {
            if wanted.len() < cap && !wanted.contains(&s) {
                wanted.push(s);
            }
        }
        let bindings: Vec<Json> = wanted.into_iter().map(|s| self.binding(s)).collect();
        let name = |m: &Self, s: Sym| m.out.interner.cs(s);
        let choices: Vec<Json> = self
            .choices
            .iter()
            .take(cap)
            .map(|c| json!({ "by": name(self, c.by), "at": at(self, c.span), "arm": c.arm, "arms": c.arms, "joined": c.joined }))
            .collect();
        let conds: Vec<Json> = self
            .conds
            .iter()
            .rev()
            .take(cap)
            .map(|c| json!({ "at": at(self, c.at), "limit": format!("{:?}", c.limit), "many": c.many }))
            .collect();
        let groups: Vec<Json> = self
            .env
            .group_stack()
            .rev()
            .take(cap)
            .map(|(kind, opened, many)| json!({ "kind": format!("{kind:?}"), "at": at(self, opened), "many": many }))
            .collect();
        let input: Vec<Json> = self
            .input
            .iter()
            .rev()
            .filter_map(|f| match f {
                Frame::File { mouth, .. } => Some(json!({ "file": at(self, mouth.here()) })),
                Frame::Tokens { toks, pos, expanding, .. } => Some(json!({
                    "tokens": toks.len().saturating_sub(*pos),
                    "expanding": expanding.map(|s| name(self, s)),
                })),
                Frame::Pseudo { at: s, .. } => Some(json!({ "scantokens": at(self, *s) })),
                Frame::Returned(t) => Some(json!({ "returned": t.len() })),
                Frame::Boundary => None,
            })
            .take(cap)
            .collect();
        // The catcodes of the characters ahead: what the next reads see.
        let mut cats = serde_json::Map::new();
        for c in text.chars().filter(|c| !c.is_alphanumeric()) {
            if cats.len() < cap {
                cats.entry(c.to_string()).or_insert(json!(self.catcodes.get(c) as u8));
            }
        }
        json!({
            "paths": {
                "live": self.waiting + 1,
                "forks": self.out.forks,
                "peak": self.out.peak_paths,
                "apart": self.apart.as_ref().map(|a| a.1.as_ref()).filter(|a| !a.is_empty()),
            },
            "choices": choices,
            "mode": self.mode.names(),
            "conditionals": conds,
            "groups": groups,
            "input": input,
            "bindings": bindings,
            "paired": { "names": paired.iter().take(cap).map(|&s| name(self, s)).collect::<Vec<_>>(), "arms": rows },
            "catcodes": cats,
        })
    }
}
