//! What installed packages define and load, read off their text: proposals
//! for `load.expect`, which a run then confirms.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fixed::{HashMap, HashSet};

const VERSION: u32 = 8;

#[derive(Serialize, Deserialize, Default)]
pub struct Exports {
    version: u32,
    stamps: Vec<(String, u64, u64)>,
    definers: Vec<String>,
    pub files: Vec<String>,
    /// Name (no backslash) to defining files.
    defines: HashMap<String, Vec<u32>>,
    /// Name a document can write (no `@`, `_`, `:`) to files where it stands right after a command
    /// that starts its line and that the index knows as no definer (url's `\\DeclareUrlCommand\\url`): candidates a run confirms.
    mentions: HashMap<String, Vec<u32>>,
    loads: Vec<Vec<u32>>,
    /// Per name, the shipped index's other packages that may provide it (`+name` in `exports.tsv`).
    #[serde(skip)]
    alternatives: HashMap<String, Vec<u32>>,
    /// Per file, the files that load it: `loads` reversed, made once.
    #[serde(skip)]
    loaded_by: std::sync::OnceLock<Vec<Vec<u32>>>,
    /// Each file's place in `files`, made once.
    #[serde(skip)]
    position: std::sync::OnceLock<HashMap<String, u32>>,
}

/// The commands whose next name is the one they define, as `defs` were
/// written: the defined name standing right after the command
/// (`\\newcommand{\\x}`, `\\cs_new:Npn \\x`) on its line in at least half
/// of its definitions. A command that defines a name it was not given there
/// (`\\DeclareOption`, `\\@tfor`) is none. `path` gives a file's path, or
/// none for a file not to look at. A definer that also defines names made
/// from the given one by a prefix (`\\newenvironment{x}` defines `\\x` and
/// `\\endx`, ltdefns.dtx) is listed once more with it, after a space:
/// `newenvironment end`.
pub fn written(
    defs: &crate::fixed::Chunked<crate::analysis::facts::Definition>,
    interner: &crate::engine::tex::Interner,
    path: impl Fn(crate::engine::tex::FileId) -> Option<String>,
) -> Vec<String> {
    let mut named: HashMap<crate::engine::tex::FileId, HashSet<(u32, String, String)>> = HashMap::default();
    let mut seen: HashMap<String, (u32, u32)> = HashMap::default();
    let mut prefixes: HashMap<(String, String), u32> = HashMap::default();
    for def in defs {
        let file = def.span.file;
        let pairs = named.entry(file).or_insert_with(|| {
            let text = path(file).and_then(|p| std::fs::read_to_string(p).ok()).unwrap_or_default();
            let (tokens, names) = tokenize(&text);
            pairs(&tokens, &names).map(|(line, cmd, name)| (line, names.name(cmd).into(), name)).collect()
        });
        if pairs.is_empty() {
            continue;
        }
        let (by, name) = (interner.name(def.by).to_string(), interner.name(def.name));
        // a name no file can write as one control word (ltcmd's `\x code`) is the definer's own
        // helper, made from the given name: it says nothing of where the definer puts that name
        if name.contains(' ') {
            continue;
        }
        let next = pairs.contains(&(def.span.line, by.clone(), name.to_string()));
        // a name the call made from its given one: the prefix it put before it
        let given = pairs.iter().filter(|(line, cmd, _)| *line == def.span.line && *cmd == by);
        for (_, _, word) in given.filter(|(_, _, w)| !next && name.len() > w.len() && name.ends_with(w.as_str())) {
            *prefixes.entry((by.clone(), name[..name.len() - word.len()].to_string())).or_default() += 1;
        }
        let count = seen.entry(by).or_default();
        count.0 += u32::from(next);
        count.1 += 1;
    }
    let mut out: Vec<String> =
        seen.iter().filter(|(_, (next, all))| *next > 0 && 2 * next >= *all).map(|(by, _)| by.clone()).collect();
    // a prefix the definer puts in at least half of its calls that name one
    let prefixed = prefixes.into_iter().filter(|((by, _), n)| out.contains(by) && 2 * n >= seen[by].0);
    out.extend(prefixed.map(|((by, prefix), _)| format!("{by} {prefix}")).collect::<Vec<_>>());
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
/// (`\\newcommand*{\\x}`), or the word braced right after it
/// (`\\newenvironment{align*}`, the name the call makes a control sequence
/// of), and the line it stands on.
fn pairs<'a>(
    tokens: &'a [crate::engine::tex::Token],
    names: &'a crate::engine::tex::Interner,
) -> impl Iterator<Item = (u32, crate::engine::tex::Sym, String)> + 'a {
    use crate::engine::tex::{Catcode, Tok};
    tokens.iter().enumerate().filter_map(|(i, t)| {
        let Tok::Cs(cmd) = t.tok else { return None };
        let blank = |t: &crate::engine::tex::Token| matches!(t.tok, Tok::Chr('*', _) | Tok::Chr(_, Catcode::Space));
        let mut k = i + 1;
        let skip = |k: &mut usize| {
            while tokens.get(*k).is_some_and(blank) {
                *k += 1
            }
        };
        skip(&mut k);
        let braced = tokens.get(k).is_some_and(|t| matches!(t.tok, Tok::Chr(_, Catcode::Begin)));
        if braced {
            k += 1;
            skip(&mut k);
        }
        match tokens.get(k)?.tok {
            Tok::Cs(name) => Some((t.span.line, cmd, names.name(name).to_string())),
            // a word of letters and others closed right after it: no argument made of code
            Tok::Chr(_, Catcode::Letter | Catcode::Other) if braced => {
                let word: String = tokens[k..]
                    .iter()
                    .map_while(|t| match t.tok {
                        Tok::Chr(c, Catcode::Letter | Catcode::Other) => Some(c),
                        _ => None,
                    })
                    .collect();
                let after = tokens.get(k + word.chars().count())?;
                matches!(after.tok, Tok::Chr(_, Catcode::End)).then(|| (t.span.line, cmd, word))
            }
            _ => None,
        }
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
    // written as in a document (`\\defmacro`) or as the index keeps them
    out.extend(extra.iter().map(|d| d.trim_start_matches('\\').to_string()));
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
        let code: HashMap<String, PathBuf> =
            crate::caches::distribution::files_with(roots, ".code.tex").into_iter().collect();
        let definers: HashSet<&str> = definers.iter().map(String::as_str).collect();
        for (i, (name, path)) in found.iter().enumerate() {
            exports.files.push(name.clone());
            let Ok(bytes) = std::fs::read(path) else {
                exports.loads.push(Vec::new());
                continue;
            };
            let text = String::from_utf8_lossy(&bytes);
            let (mut defined, loaded, mentioned) = scan(&text, &definers);
            if name.ends_with(".code.tex") {
                defined.extend(declared(&text));
            }
            if let Some(stem) = name.strip_suffix(".sty").filter(|_| math_font(&text)) {
                defined.push(format!("mathfont:{stem}"));
            }
            // what the code files it inputs define is the package's own, as far as they input more
            let mut inputs: Vec<String> = loaded.iter().filter(|l| l.ends_with(".code.tex")).cloned().collect();
            let mut read: HashSet<String> = HashSet::default();
            while let Some(input) = inputs.pop() {
                let Some(path) = code.get(&input).filter(|_| read.insert(input.clone())) else { continue };
                let Ok(bytes) = std::fs::read(path) else { continue };
                let (more, also, _) = scan(&String::from_utf8_lossy(&bytes), &definers);
                defined.extend(more);
                inputs.extend(also.into_iter().filter(|l| l.ends_with(".code.tex")));
            }
            for (into, names) in [(&mut exports.defines, defined), (&mut exports.mentions, mentioned)] {
                for name in names {
                    let files = into.entry(name).or_default();
                    if files.last() != Some(&(i as u32)) {
                        files.push(i as u32);
                    }
                }
            }
            exports.loads.push(loaded.iter().filter_map(|l| position.get(l).copied()).collect());
        }
        // the OpenType math fonts (`\setmathfont`): one file a family, its regular face where it has one
        let mut families: HashMap<String, (u32, bool)> = HashMap::default();
        for (name, path) in crate::engine::tfm::font_files(roots, ".otf") {
            if !name.to_ascii_lowercase().contains("math") {
                continue;
            }
            let Some((family, regular)) = std::fs::read(&path).ok().as_deref().and_then(otf_math_family) else {
                continue;
            };
            if families.get(&family).is_some_and(|&(_, r)| r || !regular) {
                continue;
            }
            families.insert(family, (exports.files.len() as u32, regular));
            exports.files.push(name);
            exports.loads.push(Vec::new());
        }
        for (family, (i, _)) in families {
            exports.defines.insert(format!("mathfont:{family}"), vec![i]);
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
        let loaded_by = self.loaded_by();
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
        let count = self.load_counts();
        let mut order: Vec<usize> = (0..self.files.len()).filter(|&i| self.files[i].ends_with(".sty")).collect();
        order.sort_by(|&a, &b| count[b].cmp(&count[a]).then(self.files[a].cmp(&self.files[b])));
        order.into_iter().take(n).map(|i| self.files[i].trim_end_matches(".sty").to_string()).collect()
    }

    /// How many files load each file.
    fn load_counts(&self) -> Vec<usize> {
        let mut count = vec![0usize; self.files.len()];
        for to in &self.loads {
            for &t in to {
                count[t as usize] += 1;
            }
        }
        count
    }

    /// The package that defines `name` itself, the one most files load, directly or through
    /// others, when several do: the provider a document most likely means (`graphics` for
    /// `\includegraphics`, which graphicx and tikz load, not adjustbox, which redefines it).
    pub fn provider(&self, name: &str) -> Option<String> {
        let files = self.defines.get(name)?;
        let best = (files.iter().copied())
            .filter(|&f| self.files[f as usize].ends_with(".sty"))
            .map(|f| (self.users(f), std::cmp::Reverse(&self.files[f as usize]), f))
            .max()?;
        Some(self.files[best.2 as usize].clone())
    }

    /// The packages that may provide `name`, most loaded first: those that define it and those
    /// that load one of them directly, which may change how its call reads its arguments
    /// (graphicx loads graphics and gives its `\includegraphics` key=value options).
    pub fn candidates(&self, name: &str) -> Vec<String> {
        let Some(files) = self.defines.get(name) else { return Vec::new() };
        let sty = |f: &u32| self.files[*f as usize].ends_with(".sty");
        let definers: Vec<u32> = files.iter().copied().filter(sty).collect();
        let mut all = definers.clone();
        all.extend(definers.iter().flat_map(|&f| self.loaded_by()[f as usize].iter().copied()).filter(sty));
        all.extend(self.alternatives.get(name).into_iter().flatten().copied());
        all.sort_unstable();
        all.dedup();
        let mut ranked: Vec<_> =
            all.into_iter().map(|f| (std::cmp::Reverse(self.users(f)), &self.files[f as usize])).collect();
        ranked.sort();
        ranked.into_iter().map(|(_, f)| f.clone()).collect()
    }

    /// The packages that name `name` after a command the index knows as no definer and that more
    /// files load than `pick` ([`Exports::package_for`]'s), most loaded first: a run of each says
    /// whether it defines it.
    pub fn proposed(&self, name: &str, pick: Option<&str>) -> Vec<String> {
        let Some(files) = self.mentions.get(name) else { return Vec::new() };
        let floor = pick.and_then(|p| self.position().get(&format!("{p}.sty"))).map_or(0, |&f| self.users(f));
        let mut ranked: Vec<_> = (files.iter().copied())
            .filter(|&f| self.files[f as usize].ends_with(".sty"))
            .map(|f| (std::cmp::Reverse(self.users(f)), &self.files[f as usize]))
            .filter(|(users, _)| users.0 > floor)
            .collect();
        ranked.sort();
        ranked.into_iter().map(|(_, f)| f.trim_end_matches(".sty").to_string()).collect()
    }

    /// How many files load `file`, directly or through others.
    fn users(&self, file: u32) -> usize {
        let (loaded_by, mut seen, mut queue) = (self.loaded_by(), HashSet::default(), vec![file]);
        while let Some(f) = queue.pop() {
            if seen.insert(f) {
                queue.extend(loaded_by[f as usize].iter().copied());
            }
        }
        seen.len()
    }

    fn position(&self) -> &HashMap<String, u32> {
        self.position.get_or_init(|| self.files.iter().enumerate().map(|(i, f)| (f.clone(), i as u32)).collect())
    }

    fn loaded_by(&self) -> &[Vec<u32>] {
        self.loaded_by.get_or_init(|| {
            let mut by = vec![Vec::new(); self.files.len()];
            for (from, to) in self.loads.iter().enumerate() {
                to.iter().for_each(|&t| by[t as usize].push(from as u32));
            }
            by
        })
    }

    /// The names a document can write (no `@` or `_`: no internals), each with the file to load
    /// for it: [`Exports::package_for`]'s package, else the library that declares it. This is
    /// what the playground ships ([`Exports::of_names`]): the browser has no installation to read.
    /// With them, up to `alternatives` other packages that may provide a name ([`Exports::candidates`]):
    /// a call the usual one rejects (graphics' `\includegraphics[width=…]`) is tried with those.
    pub fn names(&self, alternatives: usize) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = (self.defines.iter())
            .filter(|(name, _)| !name.contains(['@', '_']))
            .flat_map(|(name, files)| {
                // a library, or a font that declares itself (an OpenType math font)
                let library = || {
                    let own = |f: &&String| f.ends_with(".code.tex") || f.ends_with(".otf");
                    files.iter().map(|&f| &self.files[f as usize]).find(own).cloned()
                };
                let first = self.package_for(name).map(|p| format!("{p}.sty")).or_else(library);
                let others: Vec<String> = self
                    .candidates(name)
                    .into_iter()
                    .filter(|c| Some(c) != first.as_ref())
                    .take(alternatives)
                    .collect();
                // an alternative is marked `+`: tried for a call the usual one rejects, never offered first
                let first = first.into_iter().map(|f| (name.clone(), f));
                first.chain(others.into_iter().map(|f| (format!("+{name}"), f))).collect::<Vec<_>>()
            })
            .collect();
        out.sort();
        out
    }

    /// The index of [`Exports::names`] written one file a line, its names after it:
    /// `file<TAB>name<TAB>name…`. Each name is defined by its one file; `+name` names another
    /// package that may provide it.
    pub fn of_names(text: &str) -> Exports {
        let mut exports = Exports { version: VERSION, ..Exports::default() };
        for line in text.lines() {
            let mut fields = line.split('\t');
            let Some(file) = fields.next() else { continue };
            let i = exports.files.len() as u32;
            exports.files.push(file.to_string());
            exports.loads.push(Vec::new());
            for name in fields {
                match name.strip_prefix('+') {
                    // another package that may provide it ([`Exports::names`]' alternatives)
                    Some(name) => exports.alternatives.entry(name.to_string()).or_default().push(i),
                    None => _ = exports.defines.insert(name.to_string(), vec![i]),
                }
            }
        }
        exports
    }

    /// The names `keep` takes; `pair` asks for `end⟨name⟩` too, so the name is an environment's.
    pub fn named(&self, keep: impl Fn(&str) -> bool, pair: bool) -> Vec<String> {
        (self.defines.keys())
            .filter(|n| keep(n) && (!pair || self.defines.contains_key(&format!("end{n}"))))
            .cloned()
            .collect()
    }

    /// The package (`.sty`) to load for `name`: of those that define it with what they load
    /// ([`Exports::providers`]), the one other files load most, the one loading least on a tie. Before
    /// them a code file's own package, then the most loaded package that defines it itself
    /// ([`Exports::provider`]): `tikzpicture` is tikz's (`tikz.code.tex`), not that of forest (which
    /// patches it) or of a pgfplots shim.
    pub fn package_for(&self, name: &str) -> Option<String> {
        // a code file a package of its name inputs (pgf's `tikz.code.tex`, read by `tikz.sty`) is that package's
        let code = self.defines.get(name)?.iter().map(|&f| self.files[f as usize].as_str());
        let own = code.filter(|f| !f.ends_with(".sty")).filter_map(|f| Some(format!("{}.sty", f.split('.').next()?)));
        if let Some(own) = own.into_iter().find(|s| self.position().contains_key(s)) {
            return Some(own.trim_end_matches(".sty").to_string());
        }
        // a package that defines it itself before one that only loads it: `\includegraphics` is
        // graphics', not tikz's, which loads it through graphicx and is loaded more
        if let Some(own) = self.provider(name) {
            return Some(own.trim_end_matches(".sty").to_string());
        }
        let count = self.load_counts();
        let at = |f: &str| self.position().get(f).map_or(0, |&i| count[i as usize]);
        let stys = self.providers(&[name.to_string()]).into_iter().filter(|(f, _)| f.ends_with(".sty"));
        // `providers` puts files that load fewer first: the first of the most loaded wins a tie
        let best = stys.enumerate().max_by_key(|(i, (f, _))| (at(f), std::cmp::Reverse(*i)))?;
        Some(best.1.0.trim_end_matches(".sty").to_string())
    }

    /// Whether `file` loads `other`, directly or through what it loads.
    pub fn reaches(&self, file: &str, other: &str) -> bool {
        let at = |n: &str| self.files.iter().position(|f| f == n).map(|i| i as u32);
        at(file).zip(at(other)).is_some_and(|(a, b)| a != b && self.reach(a).contains(&b))
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

/// Whether the text of `file` defines `name` with one of `definers`: its names read once per
/// process for each set of definers, so what one run's definers found never answers another's.
pub fn defined_in(file: &Path, definers: &HashSet<&str>, name: &str) -> bool {
    type Names = std::sync::Arc<HashSet<String>>;
    type Memo = HashMap<(PathBuf, Vec<String>), Names>;
    static READ: std::sync::Mutex<Option<Memo>> = std::sync::Mutex::new(None);
    let mut by: Vec<String> = definers.iter().map(|d| d.to_string()).collect();
    by.sort_unstable();
    let Ok(mut read) = READ.lock() else { return false };
    let names = read.get_or_insert_with(HashMap::default).entry((file.to_path_buf(), by)).or_insert_with(|| {
        let text = std::fs::read(file).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        std::sync::Arc::new(scan(&text, definers).0.into_iter().collect())
    });
    names.contains(name)
}

/// The names `text` defines with one of `definers` (the run's, [`definers`]), as the index reads a package.
pub fn defines(text: &str, definers: &HashSet<&str>) -> Vec<String> {
    scan(text, definers).0
}

/// Names defined after one of `definers`, files loaded, and the names a document can write that
/// stand after another command that starts its line.
fn scan(text: &str, definers: &HashSet<&str>) -> (Vec<String>, Vec<String>, Vec<String>) {
    use crate::engine::tex::Tok;
    let (tokens, names) = tokenize(text);
    // `newenvironment end`: the definer also makes `end` + the name ([`written`])
    let mut made: HashMap<&str, Vec<&str>> = HashMap::default();
    for d in definers {
        let (by, prefix) = d.split_once(' ').unwrap_or((d, ""));
        made.entry(by).or_default().push(prefix);
    }
    made.values_mut().for_each(|p| p.sort_unstable());
    // the command each line starts with: where a package's definitions stand
    let mut first: HashMap<u32, Option<crate::engine::tex::Sym>> = HashMap::default();
    for t in &tokens {
        first.entry(t.span.line).or_insert(if let Tok::Cs(cs) = t.tok { Some(cs) } else { None });
    }
    let (mut defined, mut mentioned) = (Vec::new(), Vec::new());
    for (line, cmd, name) in pairs(&tokens, &names) {
        match made.get(names.name(cmd)) {
            Some(prefixes) => defined.extend(prefixes.iter().map(|p| format!("{p}{name}"))),
            None if first.get(&line) == Some(&Some(cmd)) && !name.is_empty() && !name.contains(['@', '_', ':']) => {
                mentioned.push(name)
            }
            None => {}
        }
    }
    let mut loaded = Vec::new();
    for (i, t) in tokens.iter().enumerate() {
        let ext = match t.tok {
            Tok::Cs(cs) => match names.name(cs) {
                "LoadClass" => ".cls",
                "RequirePackage" | "usepackage" | "RequirePackageWithOptions" => ".sty",
                // a code file the package reads as its own text (`tikz.sty`'s `\input{tikz.code.tex}`)
                "input" => {
                    let braced = group(&tokens[i + 1..]);
                    let bare = || -> String {
                        (tokens[i + 1..]
                            .iter()
                            .skip_while(|t| matches!(t.tok, Tok::Chr(_, crate::engine::tex::Catcode::Space))))
                        .map_while(|t| match t.tok {
                            Tok::Chr(c, crate::engine::tex::Catcode::Letter | crate::engine::tex::Catcode::Other) => {
                                Some(c)
                            }
                            _ => None,
                        })
                        .collect()
                    };
                    let name = if braced.is_empty() { bare() } else { braced };
                    if name.ends_with(".code.tex") {
                        loaded.push(name.trim().to_string());
                    }
                    continue;
                }
                _ => continue,
            },
            _ => continue,
        };
        loaded.extend(group(&tokens[i + 1..]).split(',').map(|n| format!("{}{ext}", n.trim())));
    }
    (defined, loaded, mentioned)
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

/// Whether a package's text sets math fonts: it declares the symbol fonts letters, operators or
/// large symbols come from (fntguide § 4.4), as newtxmath, eulervm and mathpazo do.
fn math_font(text: &str) -> bool {
    static FORM: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"\\DeclareSymbolFont\s*\{\s*(?:letters|operators|largesymbols)\s*\}")
            .expect("math font regex")
    });
    FORM.is_match(text)
}

/// An OpenType font's family and whether the file is its regular face, when it has a `MATH` table.
fn otf_math_family(data: &[u8]) -> Option<(String, bool)> {
    use ttf_parser::name_id;
    let face = ttf_parser::Face::parse(data, 0).ok()?;
    face.tables().math?;
    let name = |ids: [u16; 2]| {
        ids.iter().find_map(|&id| face.names().into_iter().find(|n| n.name_id == id && n.is_unicode())?.to_string())
    };
    let family = name([name_id::TYPOGRAPHIC_FAMILY, name_id::FAMILY])?;
    let regular = name([name_id::TYPOGRAPHIC_SUBFAMILY, name_id::SUBFAMILY]).is_some_and(|s| s == "Regular");
    Some((family, regular))
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
    /// The names a file defines depend on the definers asked with: a second
    /// run that knows `\newcommand` finds `\a`, though a first run that knew
    /// only `\def` read the file. Pins a process-wide memo keyed by the file
    /// alone, whose answer was the first caller's in every later run.
    #[test]
    fn a_file_is_read_again_for_other_definers() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/scratch/defined-in-test");
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join(format!("p{}.sty", std::process::id()));
        std::fs::write(&file, "\\newcommand\\a{}\\def\\b{}").unwrap();
        let def = ["def"].into_iter().collect();
        let both = ["def", "newcommand"].into_iter().collect();
        assert!(super::defined_in(&file, &def, "b") && !super::defined_in(&file, &def, "a"));
        assert!(super::defined_in(&file, &both, "a"), "the second set's answer, not the first's");
        let _ = std::fs::remove_file(&file);
    }

    #[test]
    fn definitions_and_loads_are_read_off_the_text() {
        let definers = ["def", "newcommand", "newrobustcmd"].into_iter().collect();
        let text = "\\RequirePackage[x]{a, b}\\def\\one{}\\newcommand*{\\two}{}\\newrobustcmd*\\three{}\n% \\def\\no{}\n\\let\\four";
        let (defined, loaded, _) = super::scan(text, &definers);
        assert_eq!(defined, ["one", "two", "three"]);
        assert_eq!(loaded, ["a.sty", "b.sty"]);
    }

    /// Intuition: a name a document can write right after a command that starts its line and that the
    /// index knows as no definer is a candidate, not a definition: url's `\\DeclareUrlCommand\\url`. Internals (`\\x@y`) are none. Pins
    /// the index losing url.sty for `\\url` when the run never used url's definer.
    #[test]
    fn names_after_unknown_definers_are_candidates() {
        let definers = ["def"].into_iter().collect();
        let (defined, _, mentioned) =
            super::scan("\\DeclareUrlCommand\\url{}\n\\let\\a@b\\relax\n\\def\\path{}\\x\\y", &definers);
        assert_eq!(defined, ["path"]);
        assert_eq!(mentioned, ["url"]);
    }

    /// Intuition: the playground's index ships one file per name, plus `+name` for another
    /// package that may provide it. Pins graphicx being no candidate there for
    /// `\\includegraphics[width=…]` (the snapshot printer loaded graphics, the build failed), and
    /// an alternative never becoming the usual provider (`tikzpicture` stays tikz's).
    #[test]
    fn the_shipped_index_keeps_alternatives_apart() {
        let e = super::Exports::of_names(
            "graphics.sty\tincludegraphics\ngraphicx.sty\t+includegraphics\ntikz.sty\ttikzpicture\nforest.sty\t+tikzpicture\n",
        );
        assert_eq!(e.provider("includegraphics").as_deref(), Some("graphics.sty"));
        assert_eq!(e.candidates("includegraphics"), ["graphics.sty", "graphicx.sty"]);
        assert_eq!(e.provider("tikzpicture").as_deref(), Some("tikz.sty"));
        assert!(e.names(0).iter().all(|(n, _)| !n.starts_with('+')));
    }

    /// Intuition: `\\newenvironment{align}` defines `\\align` and `\\endalign` (ltdefns.dtx), learned from runs as
    /// `newenvironment` and `newenvironment end`; a braced word that goes on in code (`{a\\x}`) is none. Pins the index knowing
    /// amstex's `\\def\\align` only, so `align` was offered with `\\usepackage{amstex}`.
    #[test]
    fn braced_names_and_their_prefixed_partners_are_read_off_the_text() {
        let definers = ["newenvironment", "newenvironment end"].into_iter().collect();
        let text = "\\newenvironment{align*}{}{}\\newenvironment{a\\x}{}{}\\newenvironment {gather}";
        let (defined, ..) = super::scan(text, &definers);
        assert_eq!(defined, ["align*", "endalign*", "gather", "endgather"]);
    }
}
