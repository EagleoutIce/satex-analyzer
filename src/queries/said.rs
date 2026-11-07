//! What documents a command: the package that owns it and one chain of
//! sources, first that speaks wins. satex's own description of a primitive
//! or kernel command; the manual of a package that defines it, first definer
//! first; the manuals of what an alias copies; any loaded package's manual;
//! the kernel's manual; the comment above its definition. Hover and `scope`
//! (the site's cards) both read it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::analysis::facts::Definition;
use crate::analysis::machine::Analysis;
use crate::plugins::docs::Entry;

/// Where [`said`]'s text comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source {
    /// satex's descriptions of primitives and kernel commands.
    Satex,
    /// The manual [`Said::manual`].
    Manual,
    Comment,
}

#[derive(Clone, Debug, Default)]
pub struct Said {
    /// The package that owns the name: the format, else the first package
    /// that defines it. A name only `\let` in the run belongs to the loaded
    /// package whose manual documents it and whose source defines it: the
    /// alias redefines it.
    pub owner: Option<String>,
    pub text: Option<(String, Source)>,
    /// The usage line of the first manual that gives one.
    pub usage: Option<String>,
    /// The manual that documents it, for its link.
    pub manual: Option<Entry>,
    /// The name an alias copies, whose documentation spoke.
    pub via: Option<String>,
    /// The sources asked, for a name none documents.
    pub tried: Vec<String>,
}

impl Said {
    /// "No documentation found" with the sources asked.
    pub fn none(&self) -> String {
        format!("No documentation found (tried {}).", self.tried.join(", "))
    }
}

/// [`Said`] of `name` (no backslash), the comment read above `at` (the
/// definition in force: path, line). `wait` mines a manual not mined yet.
pub fn said(analysis: &Analysis, name: &str, at: Option<(&str, usize)>, wait: bool) -> Said {
    said_in(analysis, name, at, wait, &mut HashSet::new())
}

fn said_in(analysis: &Analysis, name: &str, at: Option<(&str, usize)>, wait: bool, seen: &mut HashSet<String>) -> Said {
    let mut out = Said::default();
    seen.insert(name.to_string());
    let Some(sym) = analysis.interner.lookup(name) else { return out };
    let meaning = analysis.env.meaning(sym);
    // what the run made it: `\theX` prints counter `X` where `\c@X` is one
    let counter = name.strip_prefix("the").and_then(|c| {
        let reg = analysis.interner.lookup(&format!("c@{c}"))?;
        matches!(
            analysis.env.meaning(reg),
            crate::engine::tex::Meaning::Register(crate::engine::tex::RegKind::Count, _)
        )
        .then(|| format!("The printed value of counter {c}."))
    });
    let satex = super::query::describe_meaning(analysis, name, &meaning).map(str::to_string);
    out.text = satex.map(|t| (t, Source::Satex));
    // A primitive's manual (or the macro the format puts in its place) explains code, not the command.
    let docs = analysis.settings.plugins.docs
        && !crate::engine::builtins::primitive_names(analysis.plugins.engine).contains_key(name);
    let index = crate::plugins::docs::index(&analysis.settings.docs);
    let look = |file: &Path| if wait { index.lookup_now(file, name) } else { index.lookup(file, name) };
    let run = crate::util::progress::built(&analysis.said, || Run::of(analysis));
    let mut defs: Vec<&Definition> =
        run.defs.get(&sym).into_iter().flatten().map(|&i| &analysis.facts.defs[i]).collect();
    defs.sort_by_key(|d| (d.tag == "alias", d.node));
    let file = |f: crate::engine::tex::FileId| analysis.files.get(usize::from(f)).map(|f| PathBuf::from(&f.path));
    // The definers, first first: the format, each package that defines it, then those that only alias it.
    let mut sites: Vec<(PathBuf, String)> = Vec::new();
    let kernel = analysis.kernel_sites.get(&sym).and_then(|s| file(s.file));
    sites.extend(kernel.clone().map(|k| (k, "kernel".to_string())));
    for d in &defs {
        let package = super::query::origin_of(analysis, d.package, d.span.file).1;
        let package = package.as_str().unwrap_or("document").to_string();
        if let Some(path) = file(d.span.file).filter(|p| !sites.iter().any(|(q, _)| q == p)) {
            sites.push((path, package));
        }
    }
    out.owner = sites.first().map(|(_, p)| p.clone());
    // the first manual that describes it, else the first that names it (its usage, its link)
    let found = |e: Entry, out: &mut Said| {
        out.usage = out.usage.take().or_else(|| e.usage.clone());
        if out.manual.as_ref().is_none_or(|m| m.text.is_none()) {
            out.manual = Some(e);
        }
        out.manual.as_ref().is_some_and(|m| m.text.is_some())
    };
    let mut done = !docs;
    for (path, package) in &sites {
        if done {
            break;
        }
        // a project's own file has no manual
        if package == "document" {
            continue;
        }
        out.tried.push(format!("the {package} manual"));
        if let Some(e) = look(path) {
            done = found(e, &mut out);
        }
    }
    // an authored description stays first: the owner's manual adds its link
    done |= out.text.is_some();
    // An alias speaks with the documentation of the name it copies.
    if !done && let Some(target) = copied(analysis, &defs, seen) {
        let more = said_in(analysis, &target, None, wait, seen);
        if more.text.is_some() {
            out.text = more.text;
            out.via = Some(format!("\\{target}"));
            out.usage = out.usage.or(more.usage);
            out.manual = more.manual.or(out.manual.take());
            done = true;
        } else {
            out.tried.push(format!("the documentation of `\\{target}`"));
        }
    }
    // a name the format defines is the format's: another manual's namesake (pgf's `\fill`) is not it
    let format = kernel.is_some()
        || analysis.kernel.as_ref().is_some_and(|k| !matches!(k.meaning(sym), crate::engine::tex::Meaning::Undefined));
    if !done && !format {
        out.tried.push("every loaded package's manual".into());
        // a lookup never waits where a listener asks again once a manual is mined (the language server)
        let wait = wait && !index.listened();
        if let Some((hit, e)) = run.loaded.iter().find_map(|f| {
            let e = if wait { index.lookup_now(f, name) } else { index.lookup(f, name) };
            e.filter(|e| e.text.is_some()).map(|e| (f, e))
        }) {
            // the package whose source of that manual defines it owns it, unless a definer does
            let owns = defs.iter().all(|d| d.tag == "alias");
            if owns && let Some(o) = defining(analysis, hit, name) {
                out.owner = Some(o);
            }
            done = found(e, &mut out);
        }
    }
    if !done && format && kernel.is_none() {
        out.tried.push("the kernel manual".into());
        if let Some(e) =
            crate::caches::distribution::tex_file(&analysis.distribution.roots, "latex.ltx").and_then(|f| look(&f))
        {
            found(e, &mut out);
        }
    }
    if out.text.is_none()
        && let Some(e) = out.manual.as_ref().filter(|m| m.text.is_some())
    {
        out.text = Some((e.text.clone().unwrap_or_default(), Source::Manual));
    }
    if out.text.is_none() {
        out.tried.push("satex's descriptions".into());
        out.text = counter.map(|t| (t, Source::Satex));
    }
    if out.text.is_none() {
        // the definition in force, else the first
        let at = at.map(|(p, l)| (PathBuf::from(p), l)).or_else(|| {
            let d = defs.iter().filter(|d| d.tag != "alias").chain(&defs).next()?;
            Some((file(d.span.file)?, d.span.line as usize))
        });
        out.tried.push("the comment above its definition".into());
        let comment = at.and_then(|(p, l)| crate::knowledge::doccomment::above(&run.text(&p)?, l));
        if let Some(c) = comment.filter(|c| !c.text.is_empty()) {
            out.text = Some((c.text, Source::Comment));
        }
    }
    out
}

/// The name an alias of `defs` copies: a macro another name held before
/// the `\let`, a definer's or the format's. An empty one copies no name in particular.
fn copied(analysis: &Analysis, defs: &[&Definition], seen: &HashSet<String>) -> Option<String> {
    let alias = defs.iter().rev().find(|d| d.tag == "alias")?;
    let mac = alias.mac.as_ref()?;
    // the same meaning (`\ifx`) from the same source: its text's first token stands at the same place
    let at = mac.replacement_text.first()?.span;
    let same = |m: Option<&Rc<crate::engine::tex::MacroDef>>| {
        m.is_some_and(|m| m.replacement_text.first().is_some_and(|t| t.span == at) && **m == **mac)
    };
    let unseen = |s: crate::engine::tex::Sym| Some(analysis.interner.name(s)).filter(|n| !seen.contains(*n));
    let defined = analysis.facts.defs.iter().filter(|d| d.node < alias.node && d.tag != "alias");
    defined
        .filter(|d| same(d.mac.as_ref()))
        .max_by_key(|d| d.node)
        .and_then(|d| unseen(d.name))
        .or_else(|| {
            let kernel = analysis.kernel.as_ref()?;
            let sites = analysis.kernel_sites.iter().filter(|(s, _)| same(kernel.meaning(**s).as_macro()));
            // the first the format defined is the original, the others its copies
            sites.min_by_key(|(_, at)| **at).and_then(|(&s, _)| unseen(s))
        })
        .map(str::to_string)
}

/// What [`said`] reads of a run, built once.
#[derive(Clone, Default)]
pub struct Run {
    /// Each name's definitions, by index into the run's.
    defs: crate::fixed::HashMap<crate::engine::tex::Sym, Vec<usize>>,
    /// One file per directory the run read from: a manual lookup reads the package of the directory.
    loaded: Vec<PathBuf>,
    /// The files read for their comments, each once.
    texts: std::cell::RefCell<crate::fixed::HashMap<PathBuf, Option<Rc<str>>>>,
}

impl Run {
    fn of(analysis: &Analysis) -> Run {
        let mut run = Run::default();
        for (i, d) in analysis.facts.defs.iter().enumerate() {
            run.defs.entry(d.name).or_default().push(i);
        }
        let mut dirs = HashSet::new();
        run.loaded = (analysis.files.iter().map(|f| PathBuf::from(&f.path)))
            .filter(|p| p.parent().is_some_and(|d| dirs.insert(d.to_path_buf())))
            .collect();
        run
    }

    fn text(&self, path: &Path) -> Option<Rc<str>> {
        let mut texts = self.texts.borrow_mut();
        let text = texts
            .entry(path.to_path_buf())
            .or_insert_with(|| crate::project::overlay::read_to_string(path).ok().map(Rc::from));
        text.clone()
    }
}

/// The package of the files the run read beside `hit` (one manual's
/// package) whose source defines `name` (a format definer before it).
fn defining(analysis: &Analysis, hit: &Path, name: &str) -> Option<String> {
    let index = crate::plugins::docs::index(&analysis.settings.docs);
    let dirs = index.dirs(hit)?;
    let definers: crate::fixed::HashSet<&str> =
        analysis.format.iter().flat_map(|f| f.definers.iter().map(String::as_str)).collect();
    analysis.files.iter().enumerate().find_map(|(id, f)| {
        let path = Path::new(&f.path);
        (index.dirs(path).as_ref() == Some(&dirs)).then_some(())?;
        crate::caches::exports::defined_in(path, &definers, name).then_some(())?;
        let id = id as crate::engine::tex::FileId;
        let def = analysis.facts.defs.iter().find(|d| d.span.file == id);
        let package = super::query::origin_of(analysis, def.and_then(|d| d.package), id).1;
        package.as_str().map(str::to_string)
    })
}

/// `said`'s fields on a `scope` or `explain` record: `does` and where it
/// comes from, the owner, the manual's link, the sources tried for none.
pub fn insert(record: &mut super::query::Record, said: &Said) {
    use serde_json::json;
    if let Some(owner) = &said.owner {
        record.insert("provider".into(), json!(owner));
    }
    match &said.text {
        Some((text, source)) => {
            record.insert("does".into(), json!(text));
            let from = match source {
                Source::Satex => "satex".to_string(),
                Source::Comment => "the source comment".to_string(),
                Source::Manual => said.manual.as_ref().map(manual).unwrap_or_default(),
            };
            record.insert("does_from".into(), json!(from));
        }
        None => {
            record.insert("tried".into(), json!(said.tried));
        }
    }
    if let Some(via) = &said.via {
        record.insert("via".into(), json!(via));
    }
    if let Some((label, url)) = said.manual.as_ref().and_then(|m| m.link.as_ref()) {
        record.insert("manual".into(), json!([label, url]));
    }
}

/// The manual an entry stands in: `texdoc pgfmanual`'s name, else the file it was mined from.
pub fn manual(e: &Entry) -> String {
    let name = e.link.as_ref().and_then(|(l, _)| l.strip_prefix("texdoc ")).unwrap_or(&e.from);
    format!("the {name} manual")
}
