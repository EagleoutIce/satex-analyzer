//! `satex new`: a new project from a provider template or from another project's files
//! (`src/lsp/templates.rs`). It writes the files, then the run that follows reads the new
//! main file and reports the class or style it loaded, with the version the file says it is.

use crate::cli::{Context, Output};
use crate::lsp::templates::{self, Project};
use crate::project::config::TemplatesConfig;
use crate::queries::query::Record;
use serde_json::json;
use std::path::{Path, PathBuf};

/// What `satex new` was asked for.
pub struct Options<'a> {
    pub dir: Option<&'a Path>,
    pub template: Option<&'a str>,
    pub from: Option<&'a str>,
    pub options: &'a [String],
    pub force: bool,
}

/// The templates, one record each: what the installation or the cache has of it.
pub fn list() -> Vec<Record> {
    templates::catalog()
        .iter()
        .map(|t| {
            let (call, name) = t.load();
            let mut r = Record::new();
            r.insert("id".into(), json!(t.id));
            r.insert("name".into(), json!(t.name));
            r.insert("tag".into(), json!(t.provider));
            let from = t.fetch.as_ref().map_or("from the TeX installation", |_| "fetched from the provider");
            r.insert("detail".into(), json!(format!("\\{call}{{{name}}}, {from}")));
            r
        })
        .collect()
}

/// Writes the project `o` asks for and says where its files came from. The main file's path.
pub fn write(o: &Options, cfg: &TemplatesConfig) -> Result<(PathBuf, Project), String> {
    let project = match (o.template, o.from) {
        (Some(_), Some(_)) => return Err("`--template` and `--from` contradict each other".into()),
        (Some(id), None) => templates::find(id)?.project(cfg, (!o.options.is_empty()).then_some(o.options))?,
        (None, Some(from)) => templates::import(from, cfg)?,
        (None, None) => return Err("name a template (`--template`, `--list` shows them) or `--from` a project".into()),
    };
    let dir = o.dir.ok_or("name the folder of the new project")?;
    let main = templates::write(&project, dir, o.force)?;
    for s in &project.sources {
        let mut line = format!("satex: {} from {} ({})", s.file, s.origin, s.location);
        if let Some(v) = &s.version {
            line.push_str(&format!(", {v}"));
        }
        if let Some(n) = &s.note {
            line.push_str(&format!(": {n}"));
        }
        anstream::eprintln!("{line}");
    }
    anstream::eprintln!(
        "satex: wrote {} files into {}, main file {}",
        project.files.len(),
        dir.display(),
        project.main
    );
    Ok((main, project))
}

/// After the run read the new main file: its header line, and each class or package it loaded
/// from outside the project with the version the file says it is (`\ProvidesClass`).
pub fn report(context: &Context) -> Result<Output, String> {
    let a = context.analysis;
    let mut out = Vec::new();
    let text = crate::text::Text::new(context.source.to_string());
    let header: Vec<&str> =
        text.body().lines().filter(|l| l.trim_start().starts_with("\\documentclass")).take(1).collect();
    let mut r = Record::new();
    r.insert("kind".into(), json!("header"));
    r.insert("name".into(), json!(header.join("")));
    out.push(r);
    for load in a.facts.loads.iter().filter(|l| {
        l.depth == 0
            && matches!(l.kind, crate::engine::builtins::LoadKind::Class | crate::engine::builtins::LoadKind::Package)
    }) {
        let provided = crate::queries::query::identification(a, load.file).or_else(|| load.provided.clone());
        let mut r = Record::new();
        r.insert("kind".into(), json!(load.kind.as_str()));
        r.insert("name".into(), json!(load.name));
        r.insert("detail".into(), json!(provided));
        r.insert("about".into(), json!(load.path));
        out.push(r);
    }
    Ok(Output::Records(out))
}
