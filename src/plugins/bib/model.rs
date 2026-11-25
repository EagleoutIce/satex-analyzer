//! The data model of a document's bibliography style: entry types, fields, what each type
//! reads and requires, each field's datatype, aliases and predefined constants. Read from
//! the `.bst` the document names (its `ENTRY`, entry functions, `output.check`s and
//! `MACRO`s), else from biblatex's `blx-dm.def` and `biblatex.def`. Datatypes come from the
//! installation's `blx-dm.def` either way: a `.bst` declares none.

use std::path::{Path, PathBuf};

use crate::analysis::machine::Analysis;
use crate::engine::builtins::OccKind;

/// The entry types and fields of a style.
#[derive(Default, Clone, Debug)]
pub struct Model {
    pub types: Vec<String>,
    pub fields: Vec<String>,
    /// Per entry type, the fields the style reads of it, in its order.
    pub of_type: Vec<(String, Vec<String>)>,
    /// Per entry type, what it requires: each item a field or its alternatives (`author`, `editor`).
    pub required: Vec<(String, Vec<Vec<String>>)>,
    /// Per field, its datatype: `name`, `date`, `datepart`, `range`, `integer`, `uri`,
    /// `verbatim`, `entrykey`, `key`, `list` (a literal list), `literal`.
    pub datatypes: Vec<(String, String)>,
    /// `(BibTeX field, biblatex field)` and `(type, type)` pairs biber renames on reading.
    pub aliases: Vec<(String, String)>,
    pub type_aliases: Vec<(String, String)>,
    /// `(type, field)`: the fields biber sets itself on reading a type (`type` of a `@phdthesis`).
    pub type_sets: Vec<(String, String)>,
    /// Per entry type, the requirements that hold only without `crossref`: a `.bst` checks
    /// them in its `crossref missing$` branch.
    pub uncrossed: Vec<(String, Vec<Vec<String>>)>,
    /// The constants the style predefines (`MACRO {jan} {"January"}`) and their text.
    pub macros: Vec<(String, String)>,
    /// The fields the style reads of every entry type: a `.bst`'s sorting and labels,
    /// biblatex's untyped `\DeclareDatamodelEntryfields`.
    pub common: Vec<String>,
    /// Every function a `.bst` defines: BibTeX calls the one an entry's type names.
    pub defined: Vec<String>,
    /// Per entry type, the field sets the style takes one of only (`either.or.check`).
    pub exclusive: Vec<(String, Vec<Vec<String>>)>,
    /// Per field, what its value must be beyond its datatype: `isbn`, `issn`, `ismn`, or
    /// `pattern` with its regular expression (biblatex's data constraints).
    pub checks: Vec<(String, String, String)>,
    /// biblatex's `\DeclareDataInheritance`s, what a `crossref` child takes from its parent.
    pub inherit: Vec<Inherit>,
    /// Who reads the databases: biber against biblatex's data model, or BibTeX with a `.bst`.
    /// Neither while the document names no style.
    pub biblatex: bool,
    pub bibtex: bool,
    /// Whether the document or its style rewrites entries on the way in (a source map,
    /// inheritance rules of its own): what biber then sees is not what the database says.
    pub custom: bool,
}

/// One `\DeclareDataInheritance`: between which types, which fields arrive renamed
/// (`title` as `booktitle`), which not at all. `*` is every type.
#[derive(Default, Clone, Debug)]
pub struct Inherit {
    pub parents: Vec<String>,
    pub children: Vec<String>,
    pub renames: Vec<(String, String)>,
    pub never: Vec<String>,
}

impl Model {
    /// The fields `kind` reads, none for an unknown type.
    pub fn fields_of(&self, kind: &str) -> &[String] {
        self.of_type.iter().find(|t| t.0 == kind).map(|t| t.1.as_slice()).unwrap_or_default()
    }
    pub fn required_of(&self, kind: &str) -> &[Vec<String>] {
        self.required.iter().find(|t| t.0 == kind).map(|t| t.1.as_slice()).unwrap_or_default()
    }
    /// A field's datatype, through its alias; `literal` when the model names none.
    pub fn datatype(&self, field: &str) -> &str {
        let target = self.aliases.iter().find(|a| a.0 == field).map_or(field, |a| a.1.as_str());
        let of = |f: &str| self.datatypes.iter().find(|d| d.0 == f).map(|d| d.1.as_str());
        of(field).or_else(|| of(target)).unwrap_or("literal")
    }
    /// The type the style reads `kind` as: itself, or what biber renames it to (`phdthesis` → `thesis`).
    pub fn kind<'a>(&'a self, kind: &'a str) -> &'a str {
        let known = self.types.iter().any(|t| t == kind);
        self.type_aliases.iter().find(|t| t.0 == kind && !known).map_or(kind, |t| t.1.as_str())
    }
    /// The name biber reads `field` under (`journal` as `journaltitle`); BibTeX renames nothing.
    pub fn read_as<'a>(&'a self, field: &'a str) -> &'a str {
        self.aliases.iter().find(|a| self.biblatex && a.0 == field).map_or(field, |a| a.1.as_str())
    }
    /// Whether the style reads `field` of `kind`. biber reads a date as its parts: `urldate`
    /// where the type has `urlyear`.
    pub fn reads(&self, kind: &str, field: &str) -> bool {
        let target = self.read_as(field);
        let year = target.strip_suffix("date").filter(|_| self.datatype(target) == "date").map(|p| format!("{p}year"));
        let target = year.as_deref().unwrap_or(target);
        let mut read = self.fields_of(self.kind(kind)).iter().chain(&self.common);
        read.any(|f| f == field || f == target)
    }
    /// The names under which a `kind` child receives its `parent` type's `field` through
    /// `crossref`: its own name unless a declaration renames or withholds it.
    pub fn inherited(&self, parent: &str, kind: &str, field: &str) -> Vec<String> {
        let holds = |types: &[String], t: &str| types.iter().any(|x| x == t || x == "*");
        let (parent, kind) = (self.kind(parent), self.kind(kind));
        let rules = || self.inherit.iter().filter(|i| holds(&i.parents, parent) && holds(&i.children, kind));
        let mut to: Vec<String> =
            rules().flat_map(|i| &i.renames).filter(|r| r.0 == field).map(|r| r.1.clone()).collect();
        if to.is_empty() && !rules().any(|i| i.never.iter().any(|f| f == field)) {
            to.push(field.to_string());
        }
        to
    }
    /// The name `field` has in the other dialect (`journal` ↔ `journaltitle`).
    pub fn alias(&self, field: &str) -> Option<&str> {
        let a = self.aliases.iter();
        a.clone()
            .find(|a| a.0 == field)
            .map(|a| a.1.as_str())
            .or_else(|| a.clone().find(|a| a.1 == field).map(|a| a.0.as_str()))
    }
}

/// The model of the document's style: the `\bibliographystyle`'s `.bst`, else biblatex's.
pub fn of(analysis: &Analysis, path: &str) -> Model {
    let read = |file: Option<PathBuf>| file.and_then(|f| std::fs::read_to_string(f).ok());
    let roots = &analysis.distribution.roots;
    let installed = |name: &str| read(crate::caches::distribution::tex_file(roots, name));
    let style = analysis.facts.occurrences.iter().find(|o| o.kind == OccKind::BibStyle);
    let mut model = match style.and_then(|s| read(style_file(analysis, path, &format!("{}.bst", s.key), None))) {
        Some(text) => Model { bibtex: true, ..bst(&text) },
        None => {
            let biblatex = analysis.facts.loads.iter().find(|l| l.name == "biblatex");
            let dm = biblatex.and_then(|l| l.path.as_deref()).and_then(|p| Path::new(p).parent());
            let mut m = read(dm.map(|d| d.join("blx-dm.def"))).map(|t| datamodel(&t)).unwrap_or_default();
            // biber predefines BibTeX's month constants: those of the standard styles
            m.macros = read(style_file(analysis, path, "plain.bst", None)).map(|t| bst(&t).macros).unwrap_or_default();
            // biber reads the databases unless the `.aux` names a `.bst` (`backend=bibtex`)
            m.biblatex = biblatex.is_some() && style.is_none();
            m
        }
    };
    // biber predefines BibTeX's month constants: with biblatex the `.aux`
    // names `biblatex.bst`, which defines none of them
    if analysis.facts.loads.iter().any(|l| l.name == "biblatex")
        && let Some(months) = read(style_file(analysis, path, "plain.bst", None)).map(|t| bst(&t).macros)
    {
        for (name, value) in months {
            if !model.macros.iter().any(|m| m.0.eq_ignore_ascii_case(&name)) {
                model.macros.push((name, value));
            }
        }
    }
    // datatypes and aliases: the installation's biblatex, whatever the style
    if model.datatypes.is_empty()
        && let Some(text) = installed("blx-dm.def")
    {
        model.datatypes = datamodel(&text).datatypes;
    }
    if let Some(text) = installed("biblatex.def") {
        (model.aliases, model.type_aliases, model.type_sets) = sourcemap(&text);
        model.inherit = inheritance(&text);
    }
    let rewrites = |x: &crate::analysis::facts::Expansion| match analysis.interner.name(x.name) {
        "DeclareSourcemap" | "DeclareStyleSourcemap" => true,
        "DeclareDataInheritance" | "DefaultInheritance" | "ResetDataInheritance" => {
            !analysis.file_name(x.span.file).ends_with("biblatex.def")
        }
        _ => false,
    };
    model.custom = model.biblatex && analysis.facts.expansions.iter().any(rewrites);
    // a style's own additions to the data model (`apa.dbx`)
    for file in analysis.files.iter().filter(|f| model.biblatex && f.path.ends_with(".dbx")) {
        if let Some(more) = read(Some(PathBuf::from(&file.path))).map(|t| datamodel(&t)) {
            model.types.extend(more.types);
            model.fields.extend(more.fields);
            model.datatypes.extend(more.datatypes);
            model.common.extend(more.common);
            model.checks.extend(more.checks);
            more.of_type.into_iter().for_each(|(kind, list)| extend(&mut model.of_type, &kind, list));
            more.required.into_iter().for_each(|(kind, list)| extend(&mut model.required, &kind, list));
        }
    }
    model
}

/// `list` added to what `kind` has in `to`.
fn extend<T>(to: &mut Vec<(String, Vec<T>)>, kind: &str, list: Vec<T>) {
    match to.iter_mut().find(|t| t.0 == kind) {
        Some(t) => t.1.extend(list),
        None => to.push((kind.to_string(), list)),
    }
}

/// The style file `file`: beside the document `path`, in `shipped`, on BSTINPUTS, else in the installation's styles.
pub fn style_file(analysis: &Analysis, path: &str, file: &str, shipped: Option<PathBuf>) -> Option<PathBuf> {
    let dirs = crate::project::paths::effective(
        "BSTINPUTS",
        &analysis.project,
        &analysis.settings.paths.bstinputs,
        &analysis.project.root,
    );
    let beside = Path::new(path).parent().map(|d| d.join(file));
    let found = beside.into_iter().chain(shipped).chain(dirs.dirs.iter().map(|d| d.join(file))).find(|p| p.is_file());
    found.or_else(|| crate::caches::distribution::file_under(&analysis.distribution.roots, "bibtex/bst", file))
}

/// `text` without its `%` comments.
fn uncommented(text: &str) -> String {
    text.lines().map(|l| l.split('%').next().unwrap_or_default()).collect::<Vec<_>>().join("\n")
}

/// The braced group opening at the first `{` of `text`, and the rest after it.
pub(crate) fn group(text: &str) -> Option<(&str, &str)> {
    let open = text.find('{')?;
    let mut depth = 0;
    for (i, c) in text[open..].char_indices() {
        depth += match c {
            '{' => 1,
            '}' => -1,
            _ => 0,
        };
        if depth == 0 {
            return Some((&text[open + 1..open + i], &text[open + i + 1..]));
        }
    }
    None
}

/// A `.bst`: the fields its `ENTRY` lists, the entry types (the functions that start a
/// bibitem), the fields each reads (itself or through the functions it calls), those it
/// checks (`"author" output.check`, `"author and editor"` for either), and its `MACRO`s.
pub fn bst(text: &str) -> Model {
    let text = uncommented(text);
    let fields: Vec<String> = text
        .split("ENTRY")
        .nth(1)
        .and_then(group)
        .map(|(g, _)| g.split_whitespace().map(str::to_lowercase).collect())
        .unwrap_or_default();
    let mut types = Vec::new();
    let mut bodies: Vec<(&str, &str)> = Vec::new();
    let mut macros = Vec::new();
    for (at, _) in text.match_indices("MACRO") {
        let Some((name, rest)) = group(&text[at..]) else { continue };
        let Some((value, _)) = group(rest) else { continue };
        macros.push((name.trim().to_string(), value.trim().trim_matches('"').to_string()));
    }
    let mut rest = text.as_str();
    while let Some(at) = rest.find("FUNCTION") {
        rest = &rest[at + "FUNCTION".len()..];
        let Some((name, after)) = group(rest) else { break };
        let Some((body, _)) = group(after) else { break };
        let name = name.trim();
        bodies.push((name, body));
        if body.contains("output.bibitem") && !name.contains(['.', '$']) {
            types.push(name.to_string());
        }
    }
    let words = |body: &str| {
        body.split(|c: char| c.is_whitespace() || "{}\"#'".contains(c))
            .filter(|w| !w.is_empty())
            .map(String::from)
            .collect::<Vec<_>>()
    };
    let mut of_type = Vec::new();
    let mut required = Vec::new();
    let (mut exclusive, mut uncrossed, mut reached) = (Vec::new(), Vec::new(), Vec::<String>::new());
    for t in &types {
        let (mut seen, mut out, mut todo) = (vec![t.clone()], Vec::<String>::new(), vec![t.clone()]);
        while let Some(f) = todo.pop() {
            let Some((_, body)) = bodies.iter().find(|b| b.0 == f) else { continue };
            for w in words(body) {
                if fields.contains(&w) && !out.contains(&w) {
                    out.push(w);
                } else if bodies.iter().any(|b| b.0 == w) && !seen.contains(&w) {
                    seen.push(w.clone());
                    todo.push(w);
                }
            }
        }
        of_type.push((t.clone(), out));
        // `"volume and number" number either.or.check` in what the type calls: one of them only
        let mut either: Vec<Vec<String>> = Vec::new();
        for (_, body) in bodies.iter().filter(|b| seen.iter().any(|s| s == b.0)) {
            for (at, _) in body.match_indices("either.or.check") {
                let named = body[..at].rfind('"').and_then(|close| Some((body[..close].rfind('"')?, close)));
                let Some((open, close)) = named else { continue };
                let names: Vec<String> = body[open + 1..close].split(" and ").map(|n| n.trim().to_string()).collect();
                if names.len() > 1 && names.iter().all(|n| fields.contains(n)) && !either.contains(&names) {
                    either.push(names);
                }
            }
        }
        exclusive.push((t.clone(), either));
        reached.extend(seen);
        // `"x" output.check` in the type's own body: x is required, `"x and y"` either
        let body = bodies.iter().find(|b| b.0 == *t).map_or("", |b| b.1);
        let (mut needs, mut bare): (Vec<Vec<String>>, Vec<Vec<String>>) = (Vec::new(), Vec::new());
        for (at, _) in body.match_indices("output.check") {
            // one field checked in a branch (`{ … } { … } if$`, ACM's editor where the author is
            // empty) is no requirement
            // The branch for an entry without `crossref` stays a requirement: one with it inherits
            // its fields and is not reported.
            let (mut open, mut quoted): (Vec<bool>, bool) = (Vec::new(), false);
            for (i, c) in body[..at].char_indices() {
                match c {
                    '"' => quoted = !quoted,
                    '{' if !quoted => open.push(!body[..i].trim_end().ends_with("crossref missing$")),
                    '}' if !quoted => _ = open.pop(),
                    _ => {}
                }
            }
            let (branch, crossed) = (open.iter().any(|&branch| branch), open.iter().any(|&branch| !branch));
            let Some(q) = body[..at].trim_end().strip_suffix('"') else { continue };
            let Some(open) = q.rfind('"') else { continue };
            let names: Vec<String> = q[open + 1..].split(" and ").map(|n| n.trim().to_string()).collect();
            // `"author and editor"` states the alternative itself, in any branch; one field checked
            // in both arms of an `if$` (plain's `year` either side of `address empty$`) is required
            let again = body.matches(&format!("\"{}\" output.check", names[0])).count() > 1;
            if branch && names.len() == 1 && !again {
                continue;
            }
            if names.iter().all(|n| fields.contains(n)) && !needs.contains(&names) {
                bare.extend(crossed.then(|| names.clone()));
                needs.push(names);
            }
        }
        required.push((t.clone(), needs));
        uncrossed.push((t.clone(), bare));
    }
    // what no entry function reaches reads its fields of every type: sorting, labels
    let mut common = Vec::new();
    for (_, body) in bodies.iter().filter(|b| !reached.iter().any(|r| r == b.0)) {
        common.extend(words(body).into_iter().filter(|w| fields.contains(w)));
    }
    let defined = bodies.iter().map(|b| b.0.to_string()).collect();
    Model { types, fields, of_type, required, uncrossed, macros, common, defined, exclusive, ..Model::default() }
}

/// biblatex's `blx-dm.def`: the entry types and fields it declares, the fields per type, the
/// mandatory constraints and the datatypes.
pub fn datamodel(text: &str) -> Model {
    let text = uncommented(text);
    let mut model = Model::default();
    let names = |list: &str| -> Vec<String> {
        list.split(',')
            .map(str::trim)
            .filter(|n| !n.is_empty() && !n.contains(['\\', '#', ' ']))
            .map(String::from)
            .collect()
    };
    for (command, out) in [("Entrytypes", &mut model.types), ("Fields", &mut model.fields)] {
        for part in text.split(&format!("\\DeclareDatamodel{command}")).skip(1) {
            let part = part.trim_start();
            let (opts, part) = match part.strip_prefix('[') {
                Some(p) => p.split_once(']').unwrap_or(("", p)),
                None => ("", part),
            };
            let Some((list, _)) = group(part) else { continue };
            for n in names(list) {
                if command == "Fields" {
                    let opt = |k: &str| {
                        opts.split(',')
                            .find_map(|o| o.trim().strip_prefix(k).map(|v| v.trim_start_matches([' ', '=']).trim()))
                    };
                    let ty = match (opt("type"), opt("datatype")) {
                        (Some("list"), Some("literal")) => "list",
                        (_, Some(d)) => d,
                        _ => "literal",
                    };
                    model.datatypes.push((n.clone(), ty.to_string()));
                }
                if !out.contains(&n) {
                    out.push(n);
                }
            }
        }
    }
    // the date parts (`year`, `month`) biblatex declares through lists of its own
    for list in ["\\def\\blx@notnulldateparts", "\\def\\blx@nullokdateparts"] {
        if let Some((parts, _)) = text.split(list).nth(1).and_then(group) {
            model.datatypes.extend(names(parts).into_iter().map(|n| (n, "datepart".to_string())));
        }
    }
    for part in text.split("\\DeclareDatamodelEntryfields").skip(1).filter(|p| p.trim_start().starts_with('{')) {
        model.common.extend(group(part).map(|g| names(g.0)).unwrap_or_default());
    }
    for part in text.split("\\DeclareDatamodelEntryfields[").skip(1) {
        let Some((kinds, rest)) = part.split_once(']') else { continue };
        let Some((list, _)) = group(rest) else { continue };
        let fields = names(list);
        kinds.split(',').for_each(|kind| extend(&mut model.of_type, kind.trim(), fields.clone()));
    }
    // `\DeclareDatamodelConstraints[types]{\constraint[type=mandatory]{…}}`: fields, or alternatives
    for part in text.split("\\DeclareDatamodelConstraints").skip(1) {
        let part = part.trim_start();
        let (kinds, rest) = match part.strip_prefix('[') {
            Some(p) => p.split_once(']').map_or((vec![], p), |(k, r)| (names(k), r)),
            None => (model.types.clone(), part),
        };
        let Some((body, _)) = group(rest) else { continue };
        // `\constraint[type=data, datatype=isbn]{\constraintfield{isbn}}`, of every type
        for c in body.split("\\constraint[type=data").skip(1).filter(|_| !part.starts_with('[')) {
            let Some((opts, _)) = c.split_once("]{") else { continue };
            let Some(kind) = opts.split("datatype=").nth(1).map(|k| k.split(',').next().unwrap_or_default().trim())
            else {
                continue;
            };
            let pattern = opts.split("\\regexp").nth(1).and_then(group).map_or("", |g| g.0);
            let Some((mut rest, _)) = group(&c[opts.len()..]) else { continue };
            while let Some((field, after)) = rest.find("\\constraintfield").and_then(|at| group(&rest[at..])) {
                model.checks.push((field.trim().to_string(), kind.to_string(), pattern.to_string()));
                rest = after;
            }
        }
        let mut needs: Vec<Vec<String>> = Vec::new();
        for c in body.split("\\constraint[type=mandatory]").skip(1) {
            let Some((inner, _)) = group(c) else { continue };
            let mut rest = inner;
            while let Some(at) = rest.find("\\constraintfield") {
                rest = &rest[at..];
                let plural = rest.starts_with("\\constraintfields");
                let Some((g, after)) = group(rest) else { break };
                let fields: Vec<String> =
                    g.split("\\constraintfield").filter_map(|f| group(f).map(|(n, _)| n.trim().to_string())).collect();
                needs.push(if plural { fields } else { vec![g.trim().to_string()] });
                rest = after;
            }
        }
        kinds.iter().for_each(|kind| extend(&mut model.required, kind, needs.clone()));
    }
    model
}

/// biblatex's `\DeclareDataInheritance{parents}{children}{\inherit{from}{to} \noinherit{field}}`s.
fn inheritance(text: &str) -> Vec<Inherit> {
    let text = uncommented(text);
    let list = |l: &str| l.split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect::<Vec<_>>();
    let mut out = Vec::new();
    for part in text.split("\\DeclareDataInheritance").skip(1) {
        let groups = group(part).and_then(|(p, r)| group(r).and_then(|(c, r)| group(r).map(|(b, _)| (p, c, b))));
        let Some((parents, children, body)) = groups.filter(|g| !g.0.contains('#')) else { continue };
        let mut i = Inherit { parents: list(parents), children: list(children), ..Inherit::default() };
        for rule in body.split("\\noinherit").skip(1) {
            i.never.extend(group(rule).map(|g| g.0.trim().to_string()));
        }
        for rule in body.split("\\inherit").skip(1) {
            let pair = group(rule).and_then(|(from, r)| group(r).map(|(to, _)| (from.trim().into(), to.trim().into())));
            i.renames.extend(pair);
        }
        out.push(i);
    }
    out
}

/// Renamings: `(from, to)`.
type Pairs = Vec<(String, String)>;

/// biblatex's driver source map for BibTeX databases: the fields and types it renames, and
/// the fields it sets where it renames a type.
fn sourcemap(text: &str) -> (Pairs, Pairs, Pairs) {
    let Some(map) = text.split("\\DeclareDriverSourcemap[datatype=bibtex]").nth(1).and_then(group).map(|g| g.0) else {
        return Default::default();
    };
    let (mut fields, mut types, mut sets) = (Vec::new(), Vec::new(), Vec::new());
    for map in map.split("\\map").skip(1) {
        let mut renamed = None;
        for step in map.split("\\step[").skip(1) {
            let step = step.split(']').next().unwrap_or_default();
            let opt = |k: &str| {
                step.split(',')
                    .find_map(|o| o.trim().strip_prefix(k).map(|v| v.trim_start_matches([' ', '=']).trim().to_string()))
            };
            match (opt("fieldsource"), opt("fieldtarget"), opt("typesource"), opt("typetarget"), opt("fieldset")) {
                (Some(s), Some(t), ..) => fields.push((s, t)),
                (_, _, Some(s), Some(t), _) => {
                    renamed = Some(s.clone());
                    types.push((s, t));
                }
                (.., Some(field)) => sets.extend(renamed.clone().map(|kind| (kind, field))),
                _ => {}
            }
        }
    }
    (fields, types, sets)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn installed(name: &str) -> Option<String> {
        let out = std::process::Command::new("kpsewhich").arg(name).output().ok()?;
        std::fs::read_to_string(String::from_utf8(out.stdout).ok()?.trim()).ok()
    }

    #[test]
    fn plain_bst_requires_what_it_checks() {
        let Some(text) = installed("plain.bst") else { return };
        let m = bst(&text);
        let article = m.required_of("article");
        for f in ["author", "title", "journal", "year"] {
            assert!(article.contains(&vec![f.to_string()]), "{f}: {article:?}");
        }
        assert!(m.required_of("book").contains(&vec!["author".into(), "editor".into()]), "{:?}", m.required_of("book"));
        assert!(m.macros.iter().any(|(n, v)| n == "jan" && v == "January"), "{:?}", m.macros);
    }

    /// Intuition: a check in one branch of an `if$` runs only on that branch: ACM's `article`
    /// checks `editor` only where the author is empty. Pins C052 saying "lacks `editor`,
    /// required for `@article`" on every ACM article with an author.
    #[test]
    fn a_check_in_a_branch_is_no_requirement() {
        let text = "ENTRY { author editor title } {} {}\nFUNCTION {article}\n{ output.bibitem\n  author empty.or.unknown\n    { editor empty.or.unknown { \"neither {x}\" warning$ } { format.editors \"editor\" output.check } if$ }\n    { format.authors \"author\" output.check }\n  if$\n  format.title \"title\" output.check\n}\n";
        let m = bst(text);
        assert_eq!(m.required_of("article"), [vec!["title".to_string()]]);
    }

    #[test]
    fn biblatex_declares_types_constraints_and_aliases() {
        let (Some(dm), Some(def)) = (installed("blx-dm.def"), installed("biblatex.def")) else { return };
        let mut m = datamodel(&dm);
        (m.aliases, m.type_aliases, m.type_sets) = sourcemap(&def);
        assert_eq!(m.datatype("author"), "name");
        assert_eq!(m.datatype("pages"), "range");
        assert_eq!(m.datatype("date"), "date");
        assert_eq!(m.datatype("year"), "datepart");
        assert_eq!(m.datatype("url"), "uri");
        assert_eq!(m.datatype("crossref"), "entrykey");
        assert_eq!(m.alias("journal"), Some("journaltitle"));
        assert_eq!(m.datatype("journal"), "literal");
        let article = m.required_of("article");
        assert!(article.contains(&vec!["journaltitle".to_string()]), "{article:?}");
        assert!(article.contains(&vec!["date".to_string(), "year".to_string()]), "{article:?}");
        assert!(m.type_aliases.contains(&("phdthesis".into(), "thesis".into())));
        assert!(m.type_sets.contains(&("phdthesis".into(), "type".into())), "{:?}", m.type_sets);
    }

    /// Intuition: what a `.bst` reads of a type is what its function reaches. What only the
    /// sorting and label code reads is read of every type, and a pair it hands to
    /// `either.or.check` excludes each other. Pins `booktitle` as unread of an `@article`
    /// and `key` as read of all.
    #[test]
    fn plain_bst_reads_per_type_and_excludes_pairs() {
        let Some(text) = installed("plain.bst") else { return };
        let m = bst(&text);
        assert!(m.reads("inproceedings", "booktitle") && !m.reads("article", "booktitle"));
        assert!(m.reads("article", "key"), "{:?}", m.common);
        let pair = |a: &str, b: &str| vec![a.to_string(), b.to_string()];
        let of = |kind: &str| m.exclusive.iter().find(|x| x.0 == kind).map(|x| x.1.clone()).unwrap_or_default();
        assert_eq!(of("book"), [pair("volume", "number"), pair("author", "editor")]);
        assert_eq!(of("article"), Vec::<Vec<String>>::new());
    }

    /// Intuition: biber hands a parent's field to a `crossref` child under its own name
    /// unless a declaration for the two types renames or withholds it. Pins a
    /// `@proceedings`' `title` arriving as `booktitle` only, and the data constraints
    /// (`isbn`, a pattern) and untyped fields the data model declares.
    #[test]
    fn biblatex_declares_inheritance_checks_and_common_fields() {
        let (Some(dm), Some(def)) = (installed("blx-dm.def"), installed("biblatex.def")) else { return };
        let mut m = Model { inherit: inheritance(&def), biblatex: true, ..datamodel(&dm) };
        (m.aliases, m.type_aliases, m.type_sets) = sourcemap(&def);
        assert_eq!(m.inherited("proceedings", "inproceedings", "title"), ["booktitle"]);
        assert_eq!(m.inherited("proceedings", "inproceedings", "year"), ["year"]);
        assert_eq!(m.inherited("book", "inbook", "author"), ["author", "bookauthor"]);
        assert!(m.inherited("book", "inbook", "shorttitle").is_empty());
        assert!(m.checks.iter().any(|c| c.0 == "isbn" && c.1 == "isbn"), "{:?}", m.checks);
        assert!(m.checks.iter().any(|c| c.0 == "gender" && c.1 == "pattern" && c.2.contains("sf|")), "{:?}", m.checks);
        assert!(m.reads("article", "abstract") && m.reads("article", "journal") && !m.reads("article", "booktitle"));
        assert!(m.reads("article", "date") && m.reads("article", "urldate") && !m.reads("article", "eventdate"));
        assert!(m.reads("phdthesis", "institution") && m.reads("phdthesis", "school"));
    }
}
