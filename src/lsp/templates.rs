//! New projects: from a provider template (`src/data/templates.yaml`) or from an archive (a
//! GitHub repository, a zip). `satex new`, the LSP's `satex/newDocument` and the playground's
//! New document dialog all make them here.
//!
//! A template's own class or style comes from the TeX installation when it ships one. Otherwise
//! it is fetched from the provider over HTTPS into the user cache, at most every
//! `templates.refresh_days`, never waited on longer than `templates.timeout`: offline, the cached
//! copy serves. An archive is only read: paths stay inside the project, sizes are capped, links
//! are skipped, and nothing in it runs.

use crate::project::config::TemplatesConfig;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// A provider template, as the catalog writes it.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub provider: String,
    /// The class whose options the dialog offers, or the package (`package`).
    #[serde(default)]
    pub class: Option<String>,
    #[serde(default)]
    pub package: Option<String>,
    /// A few common choices, shown first: the run reads every option from the code.
    #[serde(default)]
    pub hint: Vec<String>,
    #[serde(default)]
    pub fetch: Option<Fetch>,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
    pub main: String,
}

/// Where a file the installation lacks comes from.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub struct Fetch {
    /// An archive or a file; `{year}` is tried from the coming year back to `since`.
    pub url: String,
    /// The files taken from the archive, by name.
    pub files: Vec<String>,
    #[serde(default)]
    pub since: Option<i32>,
    /// SHA-256 of the archive per year (or `""`) as checked when the catalog was written.
    #[serde(default)]
    pub known: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct Catalog {
    templates: Vec<Template>,
}

/// The catalog the build ships.
pub fn catalog() -> &'static [Template] {
    static CATALOG: std::sync::OnceLock<Vec<Template>> = std::sync::OnceLock::new();
    CATALOG.get_or_init(|| {
        let text = include_str!("../data/templates.yaml");
        serde_yaml_ng::from_str::<Catalog>(text).expect("src/data/templates.yaml is a valid catalog").templates
    })
}

/// The template named `id`.
pub fn find(id: &str) -> Result<&'static Template, String> {
    catalog().iter().find(|t| t.id == id).ok_or_else(|| {
        let ids: Vec<&str> = catalog().iter().map(|t| t.id.as_str()).collect();
        format!("no template `{id}`. Templates: {}", ids.join(", "))
    })
}

/// A new project: its files, the main one, its header line, and where its files came from.
#[derive(Serialize, Clone, Debug, Default)]
pub struct Project {
    #[serde(serialize_with = "texts")]
    pub files: BTreeMap<String, Vec<u8>>,
    pub main: String,
    /// The lines a conference asks for: `\documentclass[…]{…}`, and the style's `\usepackage`.
    pub header: String,
    pub sources: Vec<Source>,
}

/// Where a file of a project came from.
#[derive(Serialize, Clone, Debug, Default, PartialEq)]
pub struct Source {
    pub file: String,
    /// `TeX installation`, `fetched`, `cache` or `archive`.
    pub origin: String,
    /// A path or URL.
    pub location: String,
    /// What the file says it is (`\ProvidesClass{…}[…]`), when it says so plainly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

/// Text files as text, others as a list of bytes.
fn texts<S: serde::Serializer>(files: &BTreeMap<String, Vec<u8>>, s: S) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeMap;
    let mut map = s.serialize_map(Some(files.len()))?;
    for (k, v) in files {
        match std::str::from_utf8(v) {
            Ok(t) => map.serialize_entry(k, t)?,
            Err(_) => map.serialize_entry(k, v)?,
        }
    }
    map.end()
}

impl Template {
    /// The call its options go in: `documentclass` or `usepackage`, and the name.
    pub fn load(&self) -> (&'static str, String) {
        match (&self.class, &self.package) {
            (Some(c), _) => ("documentclass", c.clone()),
            (None, Some(p)) => ("usepackage", p.clone()),
            (None, None) => ("documentclass", "article".into()),
        }
    }

    /// The main file for `year`, its loaded call's options replaced by `options` when given
    /// (written by the snapshot printer), and its header line.
    pub fn main_file(&self, year: Option<i32>, options: Option<&[String]>) -> Result<(String, String), String> {
        let year = year.map(|y| y.to_string()).unwrap_or_else(|| self.newest_known().unwrap_or_default());
        let fill = |s: &str| s.replace("{year}", &year);
        let (call, name) = self.load();
        let doc = crate::text::Text::new(fill(&self.main));
        let at = call_of(&doc, call, &fill(&name)).ok_or_else(|| format!("the template has no \\{call}{{{name}}}"))?;
        let text = match options {
            Some(options) => {
                let mut node = crate::lsp::snapshot::read(&doc, at, &[0], true).ok_or("an unreadable load")?;
                let keys = options.iter().filter(|o| !o.trim().is_empty()).map(|o| {
                    let (k, v) = o.split_once('=').map_or((o.trim(), None), |(k, v)| (k.trim(), Some(v.trim())));
                    crate::lsp::snapshot::Key { key: k.to_string(), value: v.map(str::to_string) }
                });
                node.args[0].keys = Some(keys.collect());
                crate::lsp::snapshot::rewritten(&doc, &[node])?
            }
            None => doc.body().to_string(),
        };
        let doc = crate::text::Text::new(text);
        Ok((doc.body().to_string(), header(&doc, call, &fill(&name))))
    }

    /// The year of the newest archive the catalog checked.
    fn newest_known(&self) -> Option<String> {
        self.fetch.as_ref()?.known.keys().filter(|k| !k.is_empty()).max().cloned()
    }

    /// The project of this template: the main file, the template's files, and (native) the
    /// provider's files the installation lacks. `options` replace the load's when given.
    pub fn project(&self, cfg: &TemplatesConfig, options: Option<&[String]>) -> Result<Project, String> {
        let (fetched, year, mut sources) = self.provider_files(cfg)?;
        let (main, header) = self.main_file(year, options)?;
        let mut files: BTreeMap<String, Vec<u8>> =
            self.files.iter().map(|(k, v)| (k.clone(), v.clone().into_bytes())).collect();
        files.extend(fetched);
        files.insert("main.tex".into(), main.into_bytes());
        sources.sort_by(|a, b| a.file.cmp(&b.file));
        Ok(Project { files, main: "main.tex".into(), header, sources })
    }

    /// The provider's files the project needs: none when the installation ships the load (its
    /// source says where from), else the fetched or cached ones and the year they are of.
    fn provider_files(&self, cfg: &TemplatesConfig) -> Result<Provided, String> {
        let (call, name) = self.load();
        let ext = if call == "documentclass" { "cls" } else { "sty" };
        let Some(fetch) = &self.fetch else {
            let file = format!("{name}.{ext}");
            let source = installed(&file).ok_or_else(|| {
                format!("{file} is not in this TeX installation: install it (`satex install` names the package)")
            })?;
            return Ok((BTreeMap::new(), None, vec![source]));
        };
        fetched(&self.id, fetch, cfg)
    }
}

/// Byte of the first `\call` whose last braced argument is `name`.
fn call_of(doc: &crate::text::Text, call: &str, name: &str) -> Option<usize> {
    let text = doc.body();
    text.match_indices(&format!("\\{call}")).map(|(at, _)| at).find(|&at| {
        crate::lsp::snapshot::read(doc, at, &[], false)
            .and_then(|n| n.args.iter().rev().find(|a| a.open == '{').map(|a| a.text.trim() == name))
            .unwrap_or(false)
    })
}

/// The header: the `\documentclass` call, and the style's `\usepackage` when the load is one.
fn header(doc: &crate::text::Text, call: &str, name: &str) -> String {
    let text = doc.body();
    let of = |at: usize| crate::lsp::snapshot::read(doc, at, &[], false).map(|n| n.text).unwrap_or_default();
    let class = text.find("\\documentclass").map(of).unwrap_or_default();
    match call {
        "usepackage" => format!("{class}\n{}", call_of(doc, call, name).map(of).unwrap_or_default()),
        _ => class,
    }
}

/// A template's provider files, the year they are of, and where each came from.
type Provided = (BTreeMap<String, Vec<u8>>, Option<i32>, Vec<Source>);

/// What `\Provides…{name}[…]` writes in `text`, when written out (no macro in it).
pub fn provided(text: &str, name: &str) -> Option<String> {
    let re = regex::Regex::new(&format!(
        r"\\Provides(?:Class|Package|File|ExplClass|ExplPackage)\s*\{{{}(?:\.\w+)?\}}\s*(?:\{{[^}}]*\}}\s*\{{[^}}]*\}}\s*\{{([^}}]*)\}}|\[([^\]]*)\])",
        regex::escape(name)
    ))
    .ok()?;
    let c = re.captures(text)?;
    let v = c.get(1).or_else(|| c.get(2))?.as_str();
    let v = v.replace("^^J", " ").split_whitespace().collect::<Vec<_>>().join(" ");
    (!v.is_empty() && !v.contains('\\')).then_some(v)
}

/// The installation's `file`, and what it says it is.
fn installed(file: &str) -> Option<Source> {
    if cfg!(target_os = "emscripten") {
        return None;
    }
    let out = std::process::Command::new("kpsewhich").arg(file).output().ok()?;
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if path.is_empty() {
        return None;
    }
    let text = std::fs::read(&path).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    let stem = file.rsplit_once('.').map_or(file, |(s, _)| s);
    Some(Source {
        file: file.into(),
        origin: "TeX installation".into(),
        location: path,
        version: provided(&text, stem),
        ..Source::default()
    })
}

/// What the cache keeps of a fetched template.
#[derive(Serialize, Deserialize, Default)]
struct Manifest {
    url: String,
    year: Option<i32>,
    sha256: String,
    /// Seconds since the epoch.
    fetched: i64,
    files: Vec<String>,
}

/// The provider's files of template `id`: fresh from the cache, else downloaded (newest year
/// first), else the stale cache; an error only when there is no copy at all.
fn fetched(id: &str, fetch: &Fetch, cfg: &TemplatesConfig) -> Result<Provided, String> {
    let dir = crate::project::paths::cache_dir().map(|d| d.join("satex").join("templates").join(id));
    let now = time::OffsetDateTime::now_utc();
    let cached = dir.as_ref().and_then(|d| {
        let m: Manifest = serde_json::from_slice(&std::fs::read(d.join("manifest.json")).ok()?).ok()?;
        let files: Option<BTreeMap<String, Vec<u8>>> =
            m.files.iter().map(|f| Some((f.clone(), std::fs::read(d.join(f)).ok()?))).collect();
        Some((m, files?))
    });
    let fresh =
        cached.as_ref().is_some_and(|(m, _)| now.unix_timestamp() - m.fetched < i64::from(cfg.refresh_days) * 86_400);
    let sources = |m: &Manifest, files: &BTreeMap<String, Vec<u8>>, origin: &str, note: Option<String>| {
        let check = fetch.known.get(&m.year.map(|y| y.to_string()).unwrap_or_default());
        let note = note.or_else(|| match check {
            Some(k) if *k == m.sha256 => None,
            None if fetch.known.is_empty() => Some("the provider's current release, not pinned".into()),
            Some(_) => Some("differs from the archive the catalog checked: a new release".into()),
            None => Some("a release newer than the catalog: not checked".into()),
        });
        files
            .iter()
            .map(|(f, bytes)| Source {
                file: f.clone(),
                origin: origin.into(),
                location: m.url.clone(),
                version: provided(&String::from_utf8_lossy(bytes), f.rsplit_once('.').map_or(f, |(s, _)| s)),
                sha256: Some(m.sha256.clone()),
                note: note.clone(),
            })
            .collect::<Vec<_>>()
    };
    if let Some((m, files)) = cached.as_ref().filter(|_| fresh || !cfg.fetch) {
        return Ok((files.clone(), m.year, sources(m, files, "cache", None)));
    }
    let mut failed = Vec::new();
    if cfg.fetch {
        for (url, year) in candidates(fetch, now.year()) {
            let bytes = match download(&url, cfg) {
                Ok(b) => b,
                Err(e) => {
                    failed.push(format!("{url}: {e}"));
                    continue;
                }
            };
            let fill = |s: &str| year.map_or(s.to_string(), |y| s.replace("{year}", &y.to_string()));
            let wanted: Vec<String> = fetch.files.iter().map(|f| fill(f)).collect();
            let files = take(&bytes, &wanted, cfg)?;
            let m = Manifest { url, year, sha256: sha256(&bytes), fetched: now.unix_timestamp(), files: wanted };
            if let Some(d) = &dir {
                // a cache that cannot be written is only slower next time
                let _ = std::fs::create_dir_all(d).and_then(|()| {
                    files.iter().try_for_each(|(f, b)| std::fs::write(d.join(f), b))?;
                    std::fs::write(d.join("manifest.json"), serde_json::to_vec_pretty(&m).unwrap_or_default())
                });
            }
            let out = sources(&m, &files, "fetched", None);
            return Ok((files, year, out));
        }
    }
    match cached {
        Some((m, files)) => {
            let note = format!("offline or not reachable: the cached copy of {}", day(m.fetched));
            let out = sources(&m, &files, "cache", Some(note));
            Ok((files, m.year, out))
        }
        None if !cfg.fetch => {
            Err(format!("template `{id}` needs files from {}: set `templates.fetch: true`", fetch.url))
        }
        None => Err(format!("template `{id}`: no copy could be fetched ({})", failed.join(", "))),
    }
}

/// The date of `secs` since the epoch.
fn day(secs: i64) -> String {
    time::OffsetDateTime::from_unix_timestamp(secs).map(|t| t.date().to_string()).unwrap_or_default()
}

/// The URLs to try, newest first: the coming year back to `since` for a `{year}` pattern.
fn candidates(fetch: &Fetch, this_year: i32) -> Vec<(String, Option<i32>)> {
    if !fetch.url.contains("{year}") {
        return vec![(fetch.url.clone(), None)];
    }
    let since = fetch.since.unwrap_or(this_year - 1);
    (since..=this_year + 1).rev().map(|y| (fetch.url.replace("{year}", &y.to_string()), Some(y))).collect()
}

/// The SHA-256 of `bytes`, in hex.
pub fn sha256(bytes: &[u8]) -> String {
    use sha2::Digest;
    sha2::Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

/// `url`'s body over HTTPS, by curl: redirects followed, a time limit, a size limit.
fn download(url: &str, cfg: &TemplatesConfig) -> Result<Vec<u8>, String> {
    if !url.starts_with("https://") {
        return Err("only https:// URLs are fetched".into());
    }
    let out = std::process::Command::new("curl")
        .args(["-fsSL", "--proto", "=https", "--proto-redir", "=https", "--max-time"])
        .arg(cfg.timeout.to_string())
        .arg("--max-filesize")
        .arg(cfg.max_bytes.as_u64().to_string())
        .arg("--")
        .arg(url)
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(out.stdout)
}

/// The `wanted` files of a fetched body: from a zip by name, else the body itself as the one wanted.
fn take(bytes: &[u8], wanted: &[String], cfg: &TemplatesConfig) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if !bytes.starts_with(b"PK") {
        return match wanted {
            [one] => Ok(BTreeMap::from([(one.clone(), bytes.to_vec())])),
            _ => Err("not a zip archive".into()),
        };
    }
    let entries = unzip(bytes, cfg)?;
    wanted
        .iter()
        .map(|w| {
            let hit =
                entries.iter().filter(|(p, _)| p.rsplit('/').next() == Some(w.as_str())).min_by_key(|(p, _)| p.len());
            hit.map(|(_, b)| (w.clone(), b.clone())).ok_or_else(|| format!("the archive has no {w}"))
        })
        .collect()
}

/// A path of an archive or a request kept inside the project: relative, no `..`, `/` separated.
pub fn inside(path: &str) -> Option<String> {
    let path = path.replace('\\', "/");
    let p = Path::new(&path);
    let parts: Option<Vec<&str>> = p
        .components()
        .map(|c| match c {
            Component::Normal(s) => s.to_str(),
            Component::CurDir => Some(""),
            _ => None,
        })
        .collect();
    let parts: Vec<&str> = parts?.into_iter().filter(|s| !s.is_empty()).collect();
    (!parts.is_empty() && !path.contains(':')).then(|| parts.join("/"))
}

/// A zip's files (stored or deflated), read from its central directory: directories, links and
/// `.git`/`__MACOSX` left out, paths kept inside, one top folder all share stripped. Past
/// `templates.max_files` files or `templates.max_bytes` unpacked it is refused.
pub fn unzip(buf: &[u8], cfg: &TemplatesConfig) -> Result<Vec<(String, Vec<u8>)>, String> {
    let u16_at = |p: usize| buf.get(p..p + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let u32_at = |p: usize| buf.get(p..p + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let bad = || "a damaged zip archive".to_string();
    let end =
        (0..buf.len().saturating_sub(21)).rev().find(|&p| u32_at(p) == Some(0x0605_4b50)).ok_or("not a zip archive")?;
    let count = u16_at(end + 10).ok_or_else(bad)?;
    if count > cfg.max_files {
        return Err(format!("more than {} files", cfg.max_files));
    }
    let (mut p, mut total, mut out) = (u32_at(end + 16).ok_or_else(bad)?, 0u64, Vec::new());
    let limit = cfg.max_bytes.as_u64();
    for _ in 0..count {
        (u32_at(p) == Some(0x0201_4b50)).then_some(()).ok_or_else(bad)?;
        let method = u16_at(p + 10).ok_or_else(bad)?;
        let packed = u32_at(p + 20).ok_or_else(bad)?;
        let size = u32_at(p + 24).ok_or_else(bad)? as u64;
        let (n, extra, comment) =
            (u16_at(p + 28).ok_or_else(bad)?, u16_at(p + 30).ok_or_else(bad)?, u16_at(p + 32).ok_or_else(bad)?);
        let mode = u32_at(p + 38).ok_or_else(bad)? >> 16;
        let local = u32_at(p + 42).ok_or_else(bad)?;
        let name = String::from_utf8_lossy(buf.get(p + 46..p + 46 + n).ok_or_else(bad)?).into_owned();
        p += 46 + n + extra + comment;
        let link = mode & 0o170000 == 0o120000;
        if name.ends_with('/') || link || name.split('/').any(|s| s == ".git" || s == "__MACOSX") {
            continue;
        }
        let Some(path) = inside(&name) else { return Err(format!("{name}: a path outside the project")) };
        total += size;
        if total > limit {
            return Err(format!("unpacks to more than {}", cfg.max_bytes));
        }
        let from = local + 30 + u16_at(local + 26).ok_or_else(bad)? + u16_at(local + 28).ok_or_else(bad)?;
        let data = buf.get(from..from + packed).ok_or_else(bad)?;
        let bytes = match method {
            0 => data.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec_with_limit(data, size as usize)
                .map_err(|_| format!("{name}: damaged"))?,
            _ => return Err(format!("{name}: an unsupported compression")),
        };
        out.push((path, bytes));
    }
    let top = out.first().and_then(|(p, _)| p.split_once('/')).map(|(t, _)| format!("{t}/"));
    if let Some(top) = top.filter(|t| out.iter().all(|(p, _)| p.starts_with(t.as_str()))) {
        for (p, _) in &mut out {
            *p = p[top.len()..].to_string();
        }
    }
    Ok(out)
}

/// The archive URL of a GitHub repository URL (`https://github.com/o/r`, `…/tree/ref/sub`) and
/// the folder in it, or `None` for any other.
pub fn github(url: &str) -> Option<(String, Option<String>)> {
    let rest = url.strip_prefix("https://github.com/").or_else(|| url.strip_prefix("github:"))?;
    let mut parts = rest.trim_end_matches('/').split('/');
    let (owner, repo) = (parts.next()?, parts.next()?.trim_end_matches(".git"));
    let ok = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    (ok(owner) && ok(repo)).then_some(())?;
    let (reference, sub) = match parts.next() {
        Some("tree") => (parts.next()?.to_string(), parts.collect::<Vec<_>>().join("/")),
        None => ("HEAD".into(), String::new()),
        Some(_) => return None,
    };
    let sub = (!sub.is_empty()).then_some(sub);
    Some((format!("https://codeload.github.com/{owner}/{repo}/zip/{reference}"), sub))
}

/// A project from `from`: a GitHub repository URL, an HTTPS zip URL, a local zip or folder.
pub fn import(from: &str, cfg: &TemplatesConfig) -> Result<Project, String> {
    let local = Path::new(from);
    let (files, location, sha) = if local.is_dir() {
        (folder(local, cfg)?, local.display().to_string(), None)
    } else if local.is_file() {
        let bytes = std::fs::read(local).map_err(|e| format!("{from}: {e}"))?;
        (unzip(&bytes, cfg)?, local.display().to_string(), Some(sha256(&bytes)))
    } else {
        let (url, sub) = github(from).unwrap_or_else(|| (from.to_string(), None));
        if !cfg.fetch {
            return Err(format!("{from}: fetching is off (`templates.fetch`)"));
        }
        let bytes = download(&url, cfg)?;
        let mut files = unzip(&bytes, cfg)?;
        if let Some(sub) = sub {
            let prefix = format!("{}/", sub.trim_matches('/'));
            files = files.into_iter().filter_map(|(p, b)| Some((p.strip_prefix(&prefix)?.to_string(), b))).collect();
        }
        (files, url, Some(sha256(&bytes)))
    };
    from_files(files, location, sha)
}

/// A project of the files of an archive: its main file found, its header read.
pub fn from_files(files: Vec<(String, Vec<u8>)>, location: String, sha256: Option<String>) -> Result<Project, String> {
    let files: BTreeMap<String, Vec<u8>> = files.into_iter().collect();
    let main = main_of(&files).ok_or("no main file: no .tex file has \\documentclass and \\begin{document}")?;
    let doc = crate::text::Text::new(String::from_utf8_lossy(&files[&main]).into_owned());
    let header = header(&doc, "documentclass", "");
    let source = Source { file: main.clone(), origin: "archive".into(), location, sha256, ..Source::default() };
    Ok(Project { files, main, header, sources: vec![source] })
}

/// The main file: one a `.latexmain` names, else the shallowest `.tex` file whose code (not a
/// comment) has `\documentclass` and `\begin{document}`, `main.tex` first among equals.
pub fn main_of(files: &BTreeMap<String, Vec<u8>>) -> Option<String> {
    let named = files.keys().filter_map(|n| n.strip_suffix(".latexmain")).find(|n| files.contains_key(*n));
    if let Some(n) = named {
        return Some(n.to_string());
    }
    let code = |b: &[u8]| -> String {
        String::from_utf8_lossy(b).lines().map(|l| l.split('%').next().unwrap_or("")).collect::<Vec<_>>().join("\n")
    };
    let root = |b: &[u8]| {
        let c = code(b);
        c.contains("\\documentclass") && c.contains("\\begin{document}")
    };
    files
        .iter()
        .filter(|(n, b)| n.ends_with(".tex") && root(b))
        .map(|(n, _)| n)
        .min_by_key(|n| (n.matches('/').count(), !n.ends_with("main.tex"), n.len()))
        .cloned()
}

/// A local folder's files, as an archive's are taken: links not followed, sizes capped.
fn folder(dir: &Path, cfg: &TemplatesConfig) -> Result<Vec<(String, Vec<u8>)>, String> {
    let (mut out, mut total) = (Vec::new(), 0u64);
    for e in walkdir::WalkDir::new(dir).follow_links(false).into_iter().filter_map(Result::ok) {
        if !e.file_type().is_file() {
            continue;
        }
        let rel = e.path().strip_prefix(dir).ok().and_then(|p| inside(&p.to_string_lossy()));
        let Some(rel) = rel.filter(|r| !r.split('/').any(|s| s == ".git")) else { continue };
        total += e.metadata().map(|m| m.len()).unwrap_or(0);
        if total > cfg.max_bytes.as_u64() || out.len() >= cfg.max_files {
            return Err(format!("{}: more than {} or {} files", dir.display(), cfg.max_bytes, cfg.max_files));
        }
        out.push((rel, std::fs::read(e.path()).map_err(|err| format!("{}: {err}", e.path().display()))?));
    }
    Ok(out)
}

/// Writes `project` into `dir`, every path inside it; an existing file is kept unless `force`.
/// The main file's path.
pub fn write(project: &Project, dir: &Path, force: bool) -> Result<PathBuf, String> {
    let paths: Vec<(PathBuf, &Vec<u8>)> = project
        .files
        .iter()
        .map(|(p, b)| inside(p).map(|p| (dir.join(p), b)).ok_or_else(|| format!("{p}: a path outside the project")))
        .collect::<Result<_, _>>()?;
    if !force && let Some((p, _)) = paths.iter().find(|(p, _)| p.exists()) {
        return Err(format!("{} exists: pass --force to write over it", p.display()));
    }
    for (p, b) in paths {
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        std::fs::write(&p, b).map_err(|e| format!("{}: {e}", p.display()))?;
    }
    Ok(dir.join(&project.main))
}

/// `satex/templates`: each template with what the dialog shows of it.
pub fn listing() -> serde_json::Value {
    let items: Vec<serde_json::Value> = catalog()
        .iter()
        .map(|t| {
            let (call, name) = t.load();
            let (_, header) = t.main_file(None, None).unwrap_or_default();
            serde_json::json!({ "id": t.id, "name": t.name, "provider": t.provider, "load": call, "loads": name,
                "hint": t.hint, "fetch": t.fetch.as_ref().map(|f| &f.url), "header": header,
                "given": given(&header, call) })
        })
        .collect();
    serde_json::Value::Array(items)
}

/// The options the header's load passes.
fn given(header: &str, call: &str) -> Vec<String> {
    let doc = crate::text::Text::new(header.to_string());
    let at = header.find(&format!("\\{call}")).unwrap_or(0);
    let node = crate::lsp::snapshot::read(&doc, at, &[0], true);
    node.map(|n| {
        crate::lsp::snapshot::list(&n)
            .iter()
            .map(|k| match &k.value {
                Some(v) => format!("{}={v}", k.key),
                None => k.key.clone(),
            })
            .collect()
    })
    .unwrap_or_default()
}

/// `satex/newDocument`: `{template, options?}`, `{from}` (a URL or path) or `{files, name}` (an
/// archive's files the page fetched, path to text), written into `dir` when given (`force`
/// writes over files). The project, and `written`: the main file's path.
pub fn request(params: &serde_json::Value, cfg: &TemplatesConfig) -> Result<serde_json::Value, String> {
    let options: Option<Vec<String>> = params.get("options").and_then(|o| serde_json::from_value(o.clone()).ok());
    let project = if let Some(id) = params["template"].as_str() {
        find(id)?.project(cfg, options.as_deref())?
    } else if let Some(from) = params["from"].as_str() {
        import(from, cfg)?
    } else if let Some(files) = params["files"].as_object() {
        let files = files.iter().filter_map(|(k, v)| Some((inside(k)?, v.as_str()?.as_bytes().to_vec()))).collect();
        from_files(files, params["name"].as_str().unwrap_or("upload").to_string(), None)?
    } else {
        return Err("expected `template`, `from` or `files`".into());
    };
    let mut out = serde_json::to_value(&project).map_err(|e| e.to_string())?;
    if let Some(dir) = params["dir"].as_str() {
        let main = write(&project, Path::new(dir), params["force"].as_bool() == Some(true))?;
        out["written"] = serde_json::json!(main.display().to_string());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The catalog parses, ids are unique, and every main file names its load.
    #[test]
    fn catalog_is_sound() {
        let mut ids: Vec<&str> = catalog().iter().map(|t| t.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), catalog().len());
        for t in catalog() {
            let (main, header) = t.main_file(None, None).unwrap();
            assert!(main.contains("\\begin{document}") && header.starts_with("\\documentclass"), "{}", t.id);
            assert!(!main.contains("{year}"), "{}", t.id);
        }
    }

    /// Options in replace the load's, written by the snapshot printer; the header shows them.
    #[test]
    fn options_round_trip() {
        let t = find("acm-sigconf").unwrap();
        let opts = ["sigconf".to_string(), "review".into(), "anonymous".into()];
        let (main, header) = t.main_file(None, Some(&opts)).unwrap();
        assert_eq!(header, "\\documentclass[sigconf, review, anonymous]{acmart}");
        assert!(main.starts_with(&header));
        let (_, header) = find("acl").unwrap().main_file(None, Some(&["final".into()])).unwrap();
        assert_eq!(header, "\\documentclass[11pt]{article}\n\\usepackage[final]{acl}");
    }

    /// Paths of an archive stay inside: `..`, absolute and drive paths are refused.
    #[test]
    fn paths_stay_inside() {
        assert_eq!(inside("a/./b.tex").as_deref(), Some("a/b.tex"));
        for bad in ["../x", "/etc/passwd", "a/../../x", "C:/x", "a\\..\\..\\x", ""] {
            assert_eq!(inside(bad), None, "{bad}");
        }
    }

    /// GitHub URLs map to their archive, a folder of a branch to its prefix.
    #[test]
    fn github_urls() {
        assert_eq!(github("https://github.com/o/r").unwrap().0, "https://codeload.github.com/o/r/zip/HEAD");
        assert_eq!(
            github("https://github.com/o/r/tree/main/paper").unwrap(),
            ("https://codeload.github.com/o/r/zip/main".into(), Some("paper".into()))
        );
        assert_eq!(github("https://example.org/o/r"), None);
    }

    /// `{year}` patterns try the coming year first, back to `since`.
    #[test]
    fn newest_year_first() {
        let f = Fetch { url: "https://x/{year}.zip".into(), files: vec![], since: Some(2024), known: BTreeMap::new() };
        let years: Vec<_> = candidates(&f, 2026).into_iter().map(|(_, y)| y.unwrap()).collect();
        assert_eq!(years, [2027, 2026, 2025, 2024]);
    }

    /// The literal version of a `\Provides…` line; one built from macros is not guessed.
    #[test]
    fn versions() {
        let t = "\\ProvidesClass{acmart}\n [2025/08/27 v2.16 Typesetting articles]";
        assert_eq!(provided(t, "acmart").as_deref(), Some("2025/08/27 v2.16 Typesetting articles"));
        assert_eq!(provided("\\ProvidesClass{\\RCSfile}[\\RCSdate]", "x"), None);
    }
}
