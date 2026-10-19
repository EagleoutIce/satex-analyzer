//! What installed packages define and load, read off their text: proposals
//! for `load.expect`, which a run then confirms.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fixed::{HashMap, HashSet};

const VERSION: u32 = 1;

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
/// (`\\newcommand{\\x}`, `\\cs_new:Npn \\x`) in at least half of its
/// definitions. A command that defines a name it was not given there
/// (`\\DeclareOption`, `\\@tfor`) is none. `path` gives a file's path, or
/// none for a file not to look at.
pub fn written(
    defs: &[crate::facts::Definition],
    interner: &crate::tex::Interner,
    path: impl Fn(crate::tex::FileId) -> Option<String>,
) -> Vec<String> {
    let mut texts: HashMap<crate::tex::FileId, Option<String>> = HashMap::default();
    let mut seen: HashMap<String, (u32, u32)> = HashMap::default();
    for def in defs {
        let file = def.span.file;
        let text = texts.entry(file).or_insert_with(|| path(file).and_then(|p| std::fs::read_to_string(p).ok()));
        let Some(line) = text.as_deref().and_then(|t| t.lines().nth(def.span.line.saturating_sub(1) as usize)) else {
            continue;
        };
        let by = interner.name(def.by).to_string();
        let before: String = line.chars().take(def.span.col.saturating_sub(1) as usize).collect();
        let next = before.trim_end().trim_end_matches('{').trim_end().ends_with(&format!("\\{by}"));
        let count = seen.entry(by).or_default();
        count.0 += u32::from(next);
        count.1 += 1;
    }
    let mut out: Vec<String> =
        seen.into_iter().filter(|(_, (next, all))| *next > 0 && 2 * next >= *all).map(|(by, _)| by).collect();
    out.sort();
    out
}

/// The definers of a run: the kernel's, those its packages were written
/// with, and `extra` from `conventions.definers`.
pub fn definers(analysis: &crate::machine::Analysis, extra: &[String]) -> Vec<String> {
    let path = |file: crate::tex::FileId| {
        (crate::query::origin(analysis, file) != "document")
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
        let stamps = crate::distribution::stamps_for(roots);
        let file = cache.map(|dir| {
            let joined: String = roots.iter().map(|r| r.display().to_string()).collect();
            dir.join(format!("exports-v{VERSION}-{:08x}.postcard", crate::format::digest(joined.as_bytes())))
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
        let mut found: Vec<(String, PathBuf)> = crate::distribution::files_with(roots, ".sty");
        found.extend(crate::distribution::files_with(roots, ".cls"));
        let position: HashMap<String, u32> =
            found.iter().enumerate().map(|(i, (name, _))| (name.clone(), i as u32)).collect();
        let definers: HashSet<&str> = definers.iter().map(String::as_str).collect();
        for (i, (name, path)) in found.iter().enumerate() {
            exports.files.push(name.clone());
            let Ok(bytes) = std::fs::read(path) else {
                exports.loads.push(Vec::new());
                continue;
            };
            let (defined, loaded) = scan(&String::from_utf8_lossy(&bytes), &definers);
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
    let bytes = text.as_bytes();
    let (mut defined, mut loaded) = (Vec::new(), Vec::new());
    let word = |at: usize| -> (usize, &str) {
        let end =
            (at..bytes.len()).find(|&k| !(bytes[k].is_ascii_alphabetic() || bytes[k] == b'@')).unwrap_or(bytes.len());
        (end, &text[at..end])
    };
    let mut i = 0;
    let mut after: Option<&str> = None;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                i = (i..bytes.len()).find(|&k| bytes[k] == b'\n').unwrap_or(bytes.len());
                continue;
            }
            b'\\' if i + 1 < bytes.len() => {
                let (end, name) = word(i + 1);
                if name.is_empty() {
                    after = None;
                    i += 2;
                    continue;
                }
                if after.is_some() {
                    defined.push(name.to_string());
                    after = None;
                } else if definers.contains(name) {
                    after = Some(name);
                } else if matches!(name, "RequirePackage" | "usepackage" | "LoadClass" | "RequirePackageWithOptions") {
                    let ext = if name == "LoadClass" { ".cls" } else { ".sty" };
                    let mut k = end;
                    if bytes.get(k) == Some(&b'[') {
                        k = (k..bytes.len()).find(|&j| bytes[j] == b']').map_or(bytes.len(), |j| j + 1);
                    }
                    while bytes.get(k).is_some_and(u8::is_ascii_whitespace) {
                        k += 1;
                    }
                    if bytes.get(k) == Some(&b'{')
                        && let Some(close) = (k..bytes.len()).find(|&j| bytes[j] == b'}')
                    {
                        loaded.extend(text[k + 1..close].split(',').map(|n| format!("{}{ext}", n.trim())));
                    }
                }
                i = end;
                continue;
            }
            // `\newcommand*{\x}`.
            b'*' | b'{' | b' ' | b'\t' | b'\n' | b'\r' => {}
            _ => after = None,
        }
        i += 1;
    }
    (defined, loaded)
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
