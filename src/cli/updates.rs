//! `satex updates`: whether each package the document loads is its newest release, and with
//! `updates.changes` what that release defines differently. The installed version is what the
//! run's `\ver@⟨file⟩` holds. The newest is the `updates.index` record (CTAN's JSON API). The
//! changes come from running both releases, never from their text.
//!
//! Sound offline: without a fresh record a package is `unknown`, never `current`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::Output;
use crate::analysis::machine::{Analysis, Machine};
use crate::engine::builtins::LoadKind;
use crate::project::config::{Config, TemplatesConfig, UpdatesConfig};
use crate::queries::query::{Record, identification};

pub fn run(analysis: &Analysis, cfg: &Config) -> Result<Output, String> {
    let dir = cfg.cache.dir.clone().or_else(crate::caches::format::default_cache_dir).map(|d| d.join("updates"));
    let mut seen = BTreeSet::new();
    let mut rows = Vec::new();
    for load in &analysis.facts.loads {
        let wanted = matches!(load.kind, LoadKind::Package | LoadKind::Class);
        let (Some(path), true) = (load.path.as_deref(), wanted) else { continue };
        if !seen.insert(load.name.clone()) {
            continue;
        }
        let installed = identification(analysis, load.file).or_else(|| load.provided.clone()).unwrap_or_default();
        rows.push(check(&load.name, load.kind, Path::new(path), &installed, cfg, dir.as_deref()));
    }
    Ok(Output::Records(rows))
}

/// One package's row: installed against newest, and the changes when asked for and outdated.
fn check(name: &str, kind: LoadKind, path: &Path, installed: &str, cfg: &Config, dir: Option<&Path>) -> Record {
    let (date, version) = stamp(installed);
    let mut row = Record::new();
    row.insert("name".into(), json!(name));
    row.insert("installed".into(), json!(format!("{date} v{version}").trim_end_matches(" v").trim()));
    let (record, fresh) = match release(name, &cfg.updates, dir) {
        Ok(found) => found,
        Err(why) => {
            row.insert("status".into(), json!("unknown"));
            row.insert("note".into(), json!(why));
            return row;
        }
    };
    let text = |k: &str| record["version"][k].as_str().unwrap_or_default().to_string();
    let (new_date, new_version) = (text("date"), text("number"));
    row.insert("newest".into(), json!(format!("{new_date} v{new_version}").trim_end_matches(" v").trim()));
    let dated = |d: &str| d.len() == 10 && d.bytes().enumerate().all(|(i, b)| b.is_ascii_digit() || i == 4 || i == 7);
    let same = !version.is_empty() && version == new_version.trim_start_matches('v');
    let status = match (dated(&date) && dated(&new_date), same) {
        (_, true) => "current",
        (true, _) if date >= new_date => "current",
        (true, _) => "outdated",
        _ => "unknown",
    };
    // a stale record may miss a newer release: it shows a package outdated, never current
    let status = if status == "current" && !fresh { "unknown" } else { status };
    row.insert("status".into(), json!(status));
    if !fresh {
        row.insert("note".into(), json!("the cached record is past `updates.refresh_days`"));
    }
    if status == "outdated" && cfg.updates.changes {
        match changes(name, kind, path, &record, cfg, dir) {
            Ok(found) => row.extend(found),
            Err(why) => drop(row.insert("note".into(), json!(why))),
        }
    }
    row
}

/// The date (`YYYY-MM-DD`) and version (no `v`) of an identification text: `2025/02/06 v3.11 …`.
pub fn stamp(text: &str) -> (String, String) {
    let mut words = text.split_whitespace();
    let date = words.next().unwrap_or_default().replace('/', "-");
    let version = words.next().filter(|w| w.starts_with(|c: char| c == 'v' || c.is_ascii_digit()));
    (date, version.unwrap_or_default().trim_start_matches('v').to_string())
}

/// The release record of `name` and whether it is fresh: from the cache within
/// `refresh_days`, else from `index`, else the stale cache.
fn release(name: &str, u: &UpdatesConfig, dir: Option<&Path>) -> Result<(Value, bool), String> {
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) {
        return Err(format!("`{name}` is no package name an index knows"));
    }
    let url = u.index.replace("{name}", name);
    let file = dir.map(|d| d.join(format!("{name}.json")));
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let cached = file.as_ref().and_then(|f| serde_json::from_slice::<Value>(&std::fs::read(f).ok()?).ok());
    let cached = cached.filter(|c| c["url"] == url);
    let at = cached.as_ref().and_then(|c| c["fetched"].as_i64()).unwrap_or_default();
    match cached {
        Some(c) if now - at < i64::from(u.refresh_days) * 86_400 => return Ok((c["record"].clone(), true)),
        _ if url.starts_with("file://") => {}
        Some(c) if !u.fetch => return Ok((c["record"].clone(), false)),
        _ => {}
    }
    let fetched = bytes(&url, u).and_then(|b| serde_json::from_slice::<Value>(&b).map_err(|e| format!("{url}: {e}")));
    let record = match fetched {
        Ok(r) if r["version"].is_object() => r,
        Ok(_) => return Err(format!("{url}: no release version")),
        Err(e) => return cached.map(|c| (c["record"].clone(), false)).ok_or(e),
    };
    if let Some(f) = file.filter(|_| !url.starts_with("file://")) {
        // a cache that cannot be written only asks again next time
        let entry = json!({ "url": url, "fetched": now, "record": record });
        let _ = std::fs::create_dir_all(f.parent().unwrap_or(Path::new(".")))
            .and_then(|()| std::fs::write(&f, entry.to_string()));
    }
    Ok((record, true))
}

fn limits(u: &UpdatesConfig) -> TemplatesConfig {
    TemplatesConfig {
        fetch: u.fetch,
        timeout: u.timeout,
        max_bytes: u.max_bytes,
        max_files: u.max_files,
        ..Default::default()
    }
}

/// The body at `url`: a `file://` path is read, anything else needs `updates.fetch`.
fn bytes(url: &str, u: &UpdatesConfig) -> Result<Vec<u8>, String> {
    if let Some(p) = url.strip_prefix("file://") {
        return std::fs::read(p).map_err(|e| format!("{url}: {e}"));
    }
    if !u.fetch {
        return Err(format!("{url}: fetching is off (`updates.fetch`)"));
    }
    crate::lsp::templates::download(url, &limits(u)).map_err(|e| format!("{url}: {e}"))
}

/// The files of a release: a `file://` folder, else a zip.
fn files(url: &str, u: &UpdatesConfig) -> Result<Vec<(String, Vec<u8>)>, String> {
    match url.strip_prefix("file://").map(Path::new).filter(|p| p.is_dir()) {
        Some(dir) => crate::lsp::templates::folder(dir, &limits(u)),
        None => crate::lsp::templates::unzip(&bytes(url, u)?, &limits(u)),
    }
}

/// The newest release unpacked under the cache, once per version: the folder holding its `file`.
fn unpacked(name: &str, file: &str, record: &Value, u: &UpdatesConfig, dir: &Path) -> Result<PathBuf, String> {
    let version: String = record["version"]["number"]
        .as_str()
        .unwrap_or("unversioned")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || ".-_".contains(*c))
        .collect();
    let at = dir.join(name).join(version);
    if !at.is_dir() {
        let fill = |t: &String| {
            let (install, path) = (record["install"].as_str(), record["ctan"]["path"].as_str());
            let t = t.replace("{name}", name);
            let t = if t.contains("{install}") { t.replace("{install}", install?) } else { t };
            Some(if t.contains("{path}") { t.replace("{path}", path?) } else { t })
        };
        let mut failed = Vec::new();
        let got = u.archives.iter().filter_map(fill).find_map(|url| files(&url, u).map_err(|e| failed.push(e)).ok());
        let got = got.ok_or_else(|| format!("no archive of the release: {}", failed.join(", ")))?;
        let part = at.with_extension("part");
        let _ = std::fs::remove_dir_all(&part);
        for (p, b) in &got {
            let to = part.join(p);
            std::fs::create_dir_all(to.parent().unwrap_or(&part))
                .and_then(|()| std::fs::write(&to, b))
                .map_err(|e| format!("{}: {e}", to.display()))?;
        }
        std::fs::rename(&part, &at).map_err(|e| format!("{}: {e}", at.display()))?;
    }
    walkdir::WalkDir::new(&at)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_file() && e.file_name() == file)
        .min_by_key(walkdir::DirEntry::depth)
        .and_then(|e| e.path().parent().map(Path::to_path_buf))
        .ok_or_else(|| format!("the release ships no {file}, only its sources"))
}

/// What the newest release defines differently from the installed one: each run alone with
/// the package, the public names compared by meaning, the internal ones counted.
fn changes(
    name: &str,
    kind: LoadKind,
    path: &Path,
    record: &Value,
    cfg: &Config,
    dir: Option<&Path>,
) -> Result<Record, String> {
    let ext = if kind == LoadKind::Class { "cls" } else { "sty" };
    let dir = dir.ok_or("no cache directory to unpack the release into")?;
    let newer = unpacked(name, &format!("{name}.{ext}"), record, &cfg.updates, dir)?;
    let old = surface(&probe(name, kind, path.parent().unwrap_or(Path::new(".")), cfg), name);
    let new = surface(&probe(name, kind, &newer, cfg), name);
    let mut out = Record::new();
    let (mut internal, mut lists) = ([0; 3], [Vec::new(), Vec::new(), Vec::new()]);
    for key in old.keys().chain(new.keys()).collect::<BTreeSet<_>>() {
        let which = match (old.get(key), new.get(key)) {
            (None, Some(_)) => 0,
            (Some(_), None) => 1,
            (Some(a), Some(b)) if a != b => 2,
            _ => continue,
        };
        match key.1 {
            true => internal[which] += 1,
            false => lists[which].push(key.0.clone()),
        }
    }
    for ((field, list), n) in ["added", "removed", "changed"].into_iter().zip(lists).zip(internal) {
        out.insert(field.into(), json!(list.join(" ")));
        out.insert(format!("{field}_internal"), json!(n));
    }
    Ok(out)
}

/// The document that loads `name` alone, run in `dir` so a release there is the one read.
fn probe(name: &str, kind: LoadKind, dir: &Path, cfg: &Config) -> Analysis {
    let source = match kind {
        LoadKind::Class => format!("\\documentclass{{{name}}}\n\\begin{{document}}\n\\end{{document}}\n"),
        _ => format!("\\documentclass{{article}}\n\\usepackage{{{name}}}\n\\begin{{document}}\n\\end{{document}}\n"),
    };
    let mut cfg = cfg.clone();
    cfg.cache.document = false;
    Machine::analyze(&source, Some(&dir.join("satex-probe.tex")), &cfg)
}

/// What `package` defines, by shown name and whether it is internal: the meanings each run gave
/// it. A pair `\x`/`\endx` is the environment `{x}`, an option is `[x]`.
fn surface(a: &Analysis, package: &str) -> BTreeMap<(String, bool), BTreeSet<String>> {
    let mut names: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let ours = a.facts.defs.iter().filter(|d| d.package.is_some_and(|p| a.interner.name(p) == package));
    for d in ours.filter(|d| d.tag != "option") {
        let meaning = match &d.mac {
            Some(m) => format!(
                "{}{}{} {}:{}",
                if m.long { "long " } else { "" },
                if m.protected { "protected " } else { "" },
                m.arity(),
                a.interner.name(d.by),
                crate::engine::tex::detokenize(&m.replacement_text, &a.interner)
            ),
            None => format!("{} {}", a.interner.name(d.by), d.tag),
        };
        names.entry(a.interner.name(d.name).to_string()).or_default().insert(meaning);
    }
    let mut out = BTreeMap::new();
    for (name, meanings) in &names {
        if name.starts_with("end") && names.contains_key(&name[3..]) {
            continue;
        }
        // public: a control word of letters the document can write (`*`: `{tabular*}`)
        let word = name.chars().all(|c| c.is_alphabetic() || c == '*');
        let internal = !word || a.settings.conventions.is_internal(name);
        let end = names.get(&format!("end{name}"));
        let shown = if end.is_some() { format!("{{{name}}}") } else { format!("\\{name}") };
        let both = meanings.iter().chain(end.into_iter().flatten()).cloned().collect();
        out.insert((shown, internal), both);
    }
    for (option, default) in crate::queries::query::package_options(a, package) {
        out.insert((format!("[{option}]"), false), default.into_iter().collect());
    }
    out
}
