//! What installed packages define and load, read off their text: proposals
//! for `load.expect`, which a run then confirms.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fixed::{HashMap, HashSet};

const VERSION: u32 = 4;

#[derive(Serialize, Deserialize, Default)]
pub struct Exports {
    version: u32,
    stamps: Vec<(String, u64, u64)>,
    definers: Vec<String>,
    pub files: Vec<String>,
    /// Name (no backslash) to defining files.
    defines: HashMap<String, Vec<u32>>,
    loads: Vec<Vec<u32>>,
}

/// The commands whose next name is the one they define, as `defs` were
/// written: the defined name standing right after the command
/// (`\\newcommand{\\x}`, `\\cs_new:Npn \\x`) on its line in at least half
/// of its definitions. A command that defines a name it was not given there
/// (`\\DeclareOption`, `\\@tfor`) is none. `path` gives a file's path, or
/// none for a file not to look at.
pub fn written(
    defs: &[crate::analysis::facts::Definition],
    interner: &crate::engine::tex::Interner,
    path: impl Fn(crate::engine::tex::FileId) -> Option<String>,
) -> Vec<String> {
    let mut named: HashMap<crate::engine::tex::FileId, HashSet<(u32, String, String)>> = HashMap::default();
    let mut seen: HashMap<String, (u32, u32)> = HashMap::default();
    for def in defs {
        let file = def.span.file;
        let pairs = named.entry(file).or_insert_with(|| {
            let text = path(file).and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
            let (tokens, names) = tokenize(&text);
            pairs(&tokens).map(|(line, cmd, name)| (line, names.name(cmd).into(), names.name(name).into())).collect()
        });
        if pairs.is_empty() {
            continue;
        }
        let by = interner.name(def.by).to_string();
        let next = pairs.contains(&(def.span.line, by.clone(), interner.name(def.name).to_string()));
        let count = seen.entry(by).or_default();
        count.0 += u32::from(next);
        count.1 += 1;
    }
    let mut out: Vec<String> =
        seen.into_iter().filter(|(_, (next, all))| *next > 0 && 2 * next >= *all).map(|(by, _)| by).collect();
    out.sort();
    out
}

/// `text` as a package file's categories tokenize it: LaTeX's with `@` a
/// letter, and expl3's between `\ExplSyntaxOn` (or `\ProvidesExpl...`) and
/// `\ExplSyntaxOff` (l3kernel's interface3, "Using the LaTeX3 modules").
fn tokenize(text: &str) -> (Vec<crate::engine::tex::Token>, crate::engine::tex::Interner) {
    use crate::engine::tex::{Catcode, CatcodeTable, Tok};
    let (mut latex, mut expl) = (CatcodeTable::latex(), CatcodeTable::latex());
    latex.set('@', Catcode::Letter);
    for (c, cat) in [('@', Catcode::Letter), ('_', Catcode::Letter), (':', Catcode::Letter), ('~', Catcode::Space)]
        .into_iter()
        .chain([' ', '\t', '\r'].map(|c| (c, Catcode::Ignored)))
    {
        expl.set(c, cat);
    }
    let (mut names, mut mouth) = (crate::engine::tex::Interner::default(), crate::engine::tex::Mouth::new(text, 0));
    let (mut tokens, mut on) = (Vec::new(), false);
    while let Some(t) = mouth.next(if on { &expl } else { &latex }, &mut names) {
        if let Tok::Cs(cs) = t.tok {
            let name = names.name(cs);
            on = (on || name == "ExplSyntaxOn" || name.starts_with("ProvidesExpl")) && name != "ExplSyntaxOff";
        }
        tokens.push(t);
    }
    (tokens, names)
}

/// Each control sequence with the name after it, past `{`, `*` and spaces
/// (`\\newcommand*{\\x}`), and the line it stands on.
fn pairs(
    tokens: &[crate::engine::tex::Token],
) -> impl Iterator<Item = (u32, crate::engine::tex::Sym, crate::engine::tex::Sym)> + '_ {
    use crate::engine::tex::{Catcode, Tok};
    tokens.iter().enumerate().filter_map(|(i, t)| {
        let Tok::Cs(cmd) = t.tok else { return None };
        let skip = |t: &&crate::engine::tex::Token| {
            matches!(t.tok, Tok::Chr('*', _) | Tok::Chr(_, Catcode::Begin | Catcode::Space))
        };
        let Tok::Cs(name) = tokens[i + 1..].iter().find(|t| !skip(t))?.tok else { return None };
        Some((t.span.line, cmd, name))
    })
}

/// The definers of a run: the kernel's, those its packages were written
/// with, and `extra` from `conventions.definers`.
pub fn definers(analysis: &crate::analysis::machine::Analysis, extra: &[String]) -> Vec<String> {
    let path = |file: crate::engine::tex::FileId| {
        (crate::queries::query::origin(analysis, file) != "document")
            .then(|| analysis.files.get(usize::from(file)).map(|f| f.path.clone()))
            .flatten()
    };
    let mut out = written(&analysis.facts.defs, &analysis.interner, path);
    out.extend(analysis.format.iter().flat_map(|f| f.definers.iter().cloned()));
    out.extend(extra.iter().cloned());
    out.sort();
    out.dedup();
    out
}

impl Exports {
    /// The index for `roots`, cached while the trees are unchanged.
    pub fn of(roots: &[PathBuf], cache: Option<&Path>, definers: &[String]) -> Exports {
        let stamps = crate::caches::distribution::stamps_for(roots);
        let file = cache.map(|dir| {
            let joined: String = roots.iter().map(|r| r.display().to_string()).collect();
            dir.join(format!("exports-v{VERSION}-{:08x}.postcard", crate::caches::format::digest(joined.as_bytes())))
        });
        if let Some(cached) = file
            .as_ref()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|b| postcard::from_bytes::<Exports>(&b).ok())
            .filter(|e| e.version == VERSION && e.stamps == stamps && e.definers == definers)
        {
            return cached;
        }
        let mut exports = Exports { version: VERSION, stamps, definers: definers.to_vec(), ..Exports::default() };
        let mut found: Vec<(String, PathBuf)> = crate::caches::distribution::files_with(roots, ".sty");
        found.extend(crate::caches::distribution::files_with(roots, ".cls"));
        // pgf's and TikZ's libraries, which declare arrow tips, shapes and keys
        found.extend(
            crate::caches::distribution::files_with(roots, ".code.tex")
                .into_iter()
                .filter(|(name, _)| name.starts_with("tikzlibrary") || name.starts_with("pgflibrary")),
        );
        let position: HashMap<String, u32> =
            found.iter().enumerate().map(|(i, (name, _))| (name.clone(), i as u32)).collect();
        let definers: HashSet<&str> = definers.iter().map(String::as_str).collect();
        for (i, (name, path)) in found.iter().enumerate() {
            exports.files.push(name.clone());
            let Ok(bytes) = std::fs::read(path) else {
                exports.loads.push(Vec::new());
                continue;
            };
            let text = String::from_utf8_lossy(&bytes);
            let (mut defined, loaded) = scan(&text, &definers);
            if name.ends_with(".code.tex") {
                defined.extend(declared(&text));
            }
            for name in defined {
                let files = exports.defines.entry(name).or_default();
                if files.last() != Some(&(i as u32)) {
                    files.push(i as u32);
                }
            }
            exports.loads.push(loaded.iter().filter_map(|l| position.get(l).copied()).collect());
        }
        if let Some(file) = file
            && let Ok(bytes) = postcard::to_allocvec(&exports)
        {
            let _ = std::fs::create_dir_all(file.parent().unwrap_or(Path::new(".")));
            let _ = std::fs::write(file, bytes);
        }
        exports
    }

    /// The names declared as `kind` (`arrow`), each with the files that declare it.
    pub fn declared(&self, kind: &str) -> Vec<(String, Vec<String>)> {
        let prefix = format!("{kind}:");
        self.defines
            .iter()
            .filter_map(|(n, fs)| {
                Some((
                    n.strip_prefix(&prefix)?.to_string(),
                    fs.iter().map(|&f| self.files[f as usize].clone()).collect(),
                ))
            })
            .collect()
    }

    /// Files that, with what they load, define most of `names`: most first,
    /// then classes, then fewer files loaded.
    pub fn providers(&self, names: &[String]) -> Vec<(String, usize)> {
        let wanted: Vec<HashSet<u32>> = names
            .iter()
            .map(|n| self.defines.get(n).map(|f| f.iter().copied().collect()).unwrap_or_default())
            .collect();
        let mut loaded_by: Vec<Vec<u32>> = vec![Vec::new(); self.files.len()];
        for (from, to) in self.loads.iter().enumerate() {
            for &t in to {
                loaded_by[t as usize].push(from as u32);
            }
        }
        let mut candidates: HashSet<u32> = HashSet::default();
        let mut queue: Vec<u32> = wanted.iter().flatten().copied().collect();
        while let Some(f) = queue.pop() {
            if candidates.len() < 4096 && candidates.insert(f) {
                queue.extend(loaded_by[f as usize].iter().copied());
            }
        }
        let mut out: Vec<(String, usize, bool, usize)> = candidates
            .into_iter()
            .filter_map(|c| {
                let reach = self.reach(c);
                let covered = wanted.iter().filter(|w| w.iter().any(|f| reach.contains(f))).count();
                let name = &self.files[c as usize];
                (covered > 0).then(|| (name.clone(), covered, name.ends_with(".cls"), reach.len()))
            })
            .collect();
        out.sort_by(|a, b| b.1.cmp(&a.1).then(b.2.cmp(&a.2)).then(a.3.cmp(&b.3)).then(a.0.cmp(&b.0)));
        out.into_iter().map(|(name, covered, ..)| (name, covered)).collect()
    }

    /// The `n` packages loaded most often by others.
    pub fn most_loaded(&self, n: usize) -> Vec<String> {
        let mut count = vec![0usize; self.files.len()];
        for to in &self.loads {
            for &t in to {
                count[t as usize] += 1;
            }
        }
        let mut order: Vec<usize> = (0..self.files.len()).filter(|&i| self.files[i].ends_with(".sty")).collect();
        order.sort_by(|&a, &b| count[b].cmp(&count[a]).then(self.files[a].cmp(&self.files[b])));
        order.into_iter().take(n).map(|i| self.files[i].trim_end_matches(".sty").to_string()).collect()
    }

    /// `file` and what it loads, transitively.
    fn reach(&self, file: u32) -> HashSet<u32> {
        let mut seen: HashSet<u32> = HashSet::default();
        let mut queue = vec![file];
        while let Some(f) = queue.pop() {
            if seen.insert(f) {
                queue.extend(self.loads[f as usize].iter().copied());
            }
        }
        seen
    }
}

/// Names defined after one of `definers`, and files loaded.
fn scan(text: &str, definers: &HashSet<&str>) -> (Vec<String>, Vec<String>) {
    use crate::engine::tex::Tok;
    let (tokens, names) = tokenize(text);
    let defined = pairs(&tokens).filter(|(_, cmd, _)| definers.contains(names.name(*cmd)));
    let mut loaded = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        let ext = match t.tok {
            Tok::Cs(cs) => match names.name(cs) {
                "LoadClass" => ".cls",
                "RequirePackage" | "usepackage" | "RequirePackageWithOptions" => ".sty",
                _ => continue,
            },
            _ => continue,
        };
        loaded.extend(group(&tokens[i + 1..]).split(',').map(|n| format!("{}{ext}", n.trim())));
    }
    (defined.map(|(_, _, name)| names.name(name).to_string()).collect(), loaded)
}

/// The names a pgf or TikZ library declares besides macros: arrow tips
/// (`\pgfdeclarearrow{name = Kite, …}`), shapes (`\pgfdeclareshape{x}`) and
/// keys (`kite/.style`), each as the word an error about it names.
fn declared(text: &str) -> Vec<String> {
    static FORMS: std::sync::LazyLock<[regex::Regex; 3]> = std::sync::LazyLock::new(|| {
        [
            regex::Regex::new(r"\\pgfdeclarearrow\s*\{\s*name\s*=\s*([^,}]+?)\s*[,}]").expect("arrow regex"),
            regex::Regex::new(r"\\pgfdeclareshape\s*\{([^}]+)\}").expect("shape regex"),
            regex::Regex::new(r"(?m)(?:^|[{,\s])([A-Za-z][\w -]*?)/\.(?:style|code|tip|pic|append style)\b")
                .expect("key regex"),
        ]
    });
    // each by its word, and by its kind and word (`arrow:Kite`) for completion to list a kind's
    FORMS
        .iter()
        .zip(["arrow", "shape", "key"])
        .flat_map(|(re, kind)| {
            re.captures_iter(text).flat_map(move |c| [c[1].trim().to_string(), format!("{kind}:{}", c[1].trim())])
        })
        .collect()
}

/// The text of the group after `[options]` that `tokens` start with; a
/// name in it, which a macro makes, stays a backslash that matches no file.
fn group(tokens: &[crate::engine::tex::Token]) -> String {
    use crate::engine::tex::{Catcode, Tok};
    let space = |t: &&crate::engine::tex::Token| matches!(t.tok, Tok::Chr(_, Catcode::Space));
    let mut rest = tokens.iter().skip_while(space).peekable();
    if rest.peek().is_some_and(|t| t.tok == Tok::Chr('[', Catcode::Other)) {
        rest.find(|t| t.tok == Tok::Chr(']', Catcode::Other));
    }
    let mut rest = rest.skip_while(space);
    if !rest.next().is_some_and(|t| matches!(t.tok, Tok::Chr(_, Catcode::Begin))) {
        return String::new();
    }
    rest.take_while(|t| !matches!(t.tok, Tok::Chr(_, Catcode::End)))
        .map(|t| if let Tok::Chr(c, _) = t.tok { c } else { '\\' })
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn definitions_and_loads_are_read_off_the_text() {
        let definers = ["def", "newcommand", "newrobustcmd"].into_iter().collect();
        let text = "\\RequirePackage[x]{a, b}\\def\\one{}\\newcommand*{\\two}{}\\newrobustcmd*\\three{}\n% \\def\\no{}\n\\let\\four";
        let (defined, loaded) = super::scan(text, &definers);
        assert_eq!(defined, ["one", "two", "three"]);
        assert_eq!(loaded, ["a.sty", "b.sty"]);
    }
}
