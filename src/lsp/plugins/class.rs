//! Class options: every option the class declares (`\DeclareOption`, the keys of its own
//! keyval, l3keys or kvoptions family, as the run read them: [`crate::queries::query::package_options`]),
//! the ones the document passes checked, a provider template's common choices first
//! (`src/data/templates.yaml`), and the header line as the printer writes it, to copy. A lens on
//! `\documentclass`, and on the `\usepackage` of a style the project ships beside it (a
//! conference style as `acl.sty`). Writes go through the snapshot printer.

use super::super::snapshot;
use super::page::calls;
use super::{Cx, Hit, TypePlugin};
use crate::analysis::machine::Analysis;
use crate::analysis::roles::Ty;
use crate::engine::builtins::LoadKind;
use crate::project::config::TypePlugins;
use crate::text::Text;
use lsp_types::CodeLens;
use serde_json::{Value as Json, json};

pub(super) struct Class;

impl TypePlugin for Class {
    fn name(&self) -> &'static str {
        "class"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "the class's options as its code declares them, a template's common choices first, the header line to copy"
    }
    // the run's `\documentclass` triggers it, not a type of the text
    fn types(&self) -> &'static [Ty] {
        &[]
    }
    fn features(&self) -> &'static [&'static str] {
        &["lens", "view"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.class.enabled
    }
    fn lenses(&self, analysis: &Analysis, doc: &Text, path: &str) -> Vec<CodeLens> {
        loads(analysis, doc, path)
            .into_iter()
            .map(|(at, kind, _)| {
                let title = if kind == LoadKind::Class { "Class options…" } else { "Style options…" };
                super::lens(doc, path, doc.pos(at).line, title, "class")
            })
            .collect()
    }
    fn view(&self, cx: &Cx, _hit: &Hit) -> Option<Json> {
        let (at, kind, name) =
            loads(cx.analysis, cx.doc, cx.path).into_iter().find(|(at, ..)| cx.doc.pos(*at).line == cx.pos.line)?;
        let a = cx.analysis;
        let node = snapshot::read(cx.doc, at, &[0], true)?;
        let given: Vec<String> = snapshot::list(&node).iter().map(|k| k.key.clone()).collect();
        let hint = hints(&name);
        let mut offered: Vec<Json> = crate::queries::query::package_options(a, &name)
            .into_iter()
            .map(|(o, default)| json!({ "name": o, "default": default, "hint": hint.contains(&o) }))
            .collect();
        // a hint the run did not see declared (a choice key's value) is offered too
        let missing: Vec<Json> = hint
            .iter()
            .filter(|h| !offered.iter().any(|o| o["name"] == **h))
            .map(|h| json!({ "name": h, "hint": true }))
            .collect();
        offered.extend(missing);
        let load = a.facts.loads.iter().find(|l| l.kind == kind && l.name == name);
        let version =
            load.and_then(|l| crate::queries::query::identification(a, l.file).or_else(|| l.provided.clone()));
        let from = load.and_then(|l| l.path.clone()).unwrap_or_default();
        let what = if kind == LoadKind::Class { "class" } else { "style" };
        let mut about = format!("The {what} `{name}`");
        if let Some(v) = &version {
            about.push_str(&format!(", {v}"));
        }
        if !from.is_empty() {
            about.push_str(&format!(", read from `{from}`"));
        }
        about.push_str(&format!(
            ". It declares {} options. The header line below is what the document writes: copy it for a submission system.",
            offered.len()
        ));
        Some(json!({
            "title": format!("{} options: {name}", if kind == LoadKind::Class { "Class" } else { "Style" }),
            "sections": [
                { "kind": "text", "markdown": about },
                { "kind": "options", "node": node, "given": given, "offered": offered },
            ],
        }))
    }
}

/// The loads whose options the view offers: `\documentclass`, and each `\usepackage` of a style
/// the project's own folder holds. Byte, kind and name each.
fn loads(a: &Analysis, doc: &Text, path: &str) -> Vec<(usize, LoadKind, String)> {
    let text = doc.body();
    let dir = std::path::Path::new(path).parent().map(|d| d.to_string_lossy().into_owned()).unwrap_or_default();
    let named = |at: usize| {
        snapshot::read(doc, at, &[], false)?
            .args
            .iter()
            .rev()
            .find(|g| g.open == '{')
            .map(|g| g.text.trim().to_string())
    };
    let mut out = Vec::new();
    if let Some((at, ..)) = calls(text, "documentclass", 0, text.len()).into_iter().next()
        && let Some(name) = named(at)
        && a.facts.loads.iter().any(|l| l.kind == LoadKind::Class && l.name == name)
    {
        out.push((at, LoadKind::Class, name));
    }
    for (at, ..) in calls(text, "usepackage", 0, text.len()) {
        let Some(name) = named(at) else { continue };
        let local = a.facts.loads.iter().any(|l| {
            l.kind == LoadKind::Package
                && l.name == name
                && l.path.as_deref().is_some_and(|p| !dir.is_empty() && p.starts_with(&dir))
        });
        if local {
            out.push((at, LoadKind::Package, name));
        }
    }
    out
}

/// The common choices the templates of `name` name, in order: `icml{year}` stands for `icml2026`.
fn hints(name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for t in crate::lsp::templates::catalog() {
        let load = t.load().1;
        let fits = match load.split_once("{year}") {
            Some((pre, post)) => name
                .strip_prefix(pre)
                .and_then(|r| r.strip_suffix(post))
                .is_some_and(|y| y.chars().all(|c| c.is_ascii_digit())),
            None => load == name,
        };
        if fits {
            out.extend(t.hint.iter().filter(|h| !out.contains(h)).cloned().collect::<Vec<_>>());
        }
    }
    out
}
