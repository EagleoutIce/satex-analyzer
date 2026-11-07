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
    /// The constants the style predefines (`MACRO {jan} {"January"}`) and their text.
    pub macros: Vec<(String, String)>,
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
        Some(text) => bst(&text),
        None => {
            let biblatex = analysis.facts.loads.iter().find(|l| l.name == "biblatex");
            let dm = biblatex.and_then(|l| l.path.as_deref()).and_then(|p| Path::new(p).parent());
            let mut m = read(dm.map(|d| d.join("blx-dm.def"))).map(|t| datamodel(&t)).unwrap_or_default();
            // biber predefines BibTeX's month constants: those of the standard styles
            m.macros = installed("plain.bst").map(|t| bst(&t).macros).unwrap_or_default();
            m
        }
    };
    // datatypes and aliases: the installation's biblatex, whatever the style
    if model.datatypes.is_empty()
        && let Some(text) = installed("blx-dm.def")
    {
        model.datatypes = datamodel(&text).datatypes;
    }
    if let Some(text) = installed("biblatex.def") {
        (model.aliases, model.type_aliases) = sourcemap(&text);
    }
    model
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
        // `"x" output.check` in the type's own body: x is required, `"x and y"` either
        let body = bodies.iter().find(|b| b.0 == *t).map_or("", |b| b.1);
        let mut needs: Vec<Vec<String>> = Vec::new();
        for (at, _) in body.match_indices("output.check") {
            let Some(q) = body[..at].trim_end().strip_suffix('"') else { continue };
            let Some(open) = q.rfind('"') else { continue };
            let names: Vec<String> = q[open + 1..].split(" and ").map(|n| n.trim().to_string()).collect();
            if names.iter().all(|n| fields.contains(n)) && !needs.contains(&names) {
                needs.push(names);
            }
        }
        required.push((t.clone(), needs));
    }
    Model { types, fields, of_type, required, macros, ..Model::default() }
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
    for part in text.split("\\DeclareDatamodelEntryfields[").skip(1) {
        let Some((kinds, rest)) = part.split_once(']') else { continue };
        let Some((list, _)) = group(rest) else { continue };
        let fields = names(list);
        for kind in kinds.split(',').map(str::trim) {
            match model.of_type.iter_mut().find(|t| t.0 == kind) {
                Some(t) => t.1.extend(fields.iter().cloned()),
                None => model.of_type.push((kind.to_string(), fields.clone())),
            }
        }
    }
    // `\DeclareDatamodelConstraints[types]{\constraint[type=mandatory]{…}}`: fields, or alternatives
    for part in text.split("\\DeclareDatamodelConstraints").skip(1) {
        let part = part.trim_start();
        let (kinds, rest) = match part.strip_prefix('[') {
            Some(p) => p.split_once(']').map_or((vec![], p), |(k, r)| (names(k), r)),
            None => (model.types.clone(), part),
        };
        let Some((body, _)) = group(rest) else { continue };
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
        for kind in kinds {
            match model.required.iter_mut().find(|t| t.0 == kind) {
                Some(t) => t.1.extend(needs.iter().cloned()),
                None => model.required.push((kind, needs.clone())),
            }
        }
    }
    model
}

/// Renamings: `(from, to)`.
type Pairs = Vec<(String, String)>;

/// biblatex's driver source map for BibTeX databases: the fields and types it renames.
fn sourcemap(text: &str) -> (Pairs, Pairs) {
    let Some(map) = text.split("\\DeclareDriverSourcemap[datatype=bibtex]").nth(1).and_then(group).map(|g| g.0) else {
        return Default::default();
    };
    let (mut fields, mut types) = (Vec::new(), Vec::new());
    for step in map.split("\\step[").skip(1) {
        let step = step.split(']').next().unwrap_or_default();
        let opt = |k: &str| {
            step.split(',')
                .find_map(|o| o.trim().strip_prefix(k).map(|v| v.trim_start_matches([' ', '=']).trim().to_string()))
        };
        match (opt("fieldsource"), opt("fieldtarget"), opt("typesource"), opt("typetarget")) {
            (Some(s), Some(t), _, _) => fields.push((s, t)),
            (_, _, Some(s), Some(t)) => types.push((s, t)),
            _ => {}
        }
    }
    (fields, types)
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

    #[test]
    fn biblatex_declares_types_constraints_and_aliases() {
        let (Some(dm), Some(def)) = (installed("blx-dm.def"), installed("biblatex.def")) else { return };
        let mut m = datamodel(&dm);
        (m.aliases, m.type_aliases) = sourcemap(&def);
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
    }
}
