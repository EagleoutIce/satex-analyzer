//! Links an entry to the PDF manual in the package's `doc/` directory that
//! typesets its file: the file's own PDF, or a manual that reads it with
//! `\DocInput`, `\DocInclude`, `\input` or `\include` (source2e reads
//! lterror.dtx, interface3 reads l3tl.dtx through source3body.tex).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::markup::{group, lines};
use super::{Audience, Dirs, Entry};

pub struct Texdoc;

/// How deep a manual's reads are followed.
const DEPTH: usize = 3;

const READS: [&str; 4] = ["\\DocInput", "\\DocInclude", "\\input", "\\include"];

/// A manual: its name and whether it shows the code, which l3doc turns on
/// with `\EnableImplementation`.
struct Manual {
    name: String,
    code: bool,
    reads: HashSet<String>,
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

impl super::DocPlugin for Texdoc {
    fn link(&self, dirs: &Dirs, entries: &mut [Entry]) {
        let Some(doc) = &dirs.doc else { return };
        let pdfs: HashSet<String> = std::fs::read_dir(doc)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "pdf"))
            .map(|p| stem(&p))
            .collect();
        let sources: HashMap<String, PathBuf> = dirs.files().into_iter().map(|p| (stem(&p), p)).collect();
        let mut manuals: Vec<Manual> = sources
            .iter()
            .filter(|(s, _)| pdfs.contains(*s))
            .map(|(s, p)| {
                let text = std::fs::read(p).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
                let code = lines(p, &text).iter().any(|l| l.contains("\\EnableImplementation"));
                Manual { name: s.clone(), code, reads: reads(s, &sources) }
            })
            .collect();
        manuals.sort_by(|a, b| a.name.cmp(&b.name));
        for e in entries {
            let rank = |m: &Manual| (m.name != e.from, m.code != (e.audience == Audience::Code));
            if let Some(m) = manuals.iter().filter(|m| m.reads.contains(&e.from)).min_by_key(|m| rank(m)) {
                e.link = Some((format!("texdoc {}", m.name), crate::query::texdoc(&m.name)));
            }
        }
    }
}

/// The stems `name` reads, itself among them, up to [`DEPTH`] deep.
fn reads(name: &str, sources: &HashMap<String, PathBuf>) -> HashSet<String> {
    let mut seen = HashSet::from([name.to_string()]);
    let mut todo = vec![name.to_string()];
    for _ in 0..DEPTH {
        let mut next = Vec::new();
        for s in todo.drain(..) {
            let Some(path) = sources.get(&s) else { continue };
            let Ok(bytes) = std::fs::read(path) else { continue };
            let text = lines(path, &String::from_utf8_lossy(&bytes)).join("\n");
            for cmd in READS {
                for (at, _) in text.match_indices(cmd) {
                    let Some((arg, _)) = group(&text[at + cmd.len()..], '{', '}') else { continue };
                    let read = stem(Path::new(arg.trim()));
                    if seen.insert(read.clone()) {
                        next.push(read);
                    }
                }
            }
        }
        todo = next;
    }
    seen
}
