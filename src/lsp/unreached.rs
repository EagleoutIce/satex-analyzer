//! Definition sites the analysis never reached. When a run stops early, a
//! later file's macros have no record. This scans the source instead: it
//! follows the loads the document's text names and lexes each file for the
//! definition primitives. It knows no package macros.

use crate::fixed::{HashMap, HashSet};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::builtins::LoadKind;
use crate::lint::fix::{Group, Lexeme, Pos, Scan, Text};
use crate::loader::Resolver;
use crate::machine::Analysis;
use crate::tex::{Catcode, CatcodeTable, Tok};

/// Bounds on the scan: files read and load depth.
const MAX_FILES: usize = 200;
const MAX_DEPTH: usize = 16;

thread_local! {
    /// One resolver per document directory: the distribution index is costly.
    static RESOLVERS: RefCell<HashMap<PathBuf, Resolver>> = RefCell::default();
}

/// A definition site found by the scan: file, its text, where the name stands.
pub struct Site {
    pub path: String,
    pub text: Rc<Text>,
    pub pos: Pos,
}

/// How a command's text reads: the loads it names, the files they resolve as.
fn load_kind(cs: &str) -> Option<LoadKind> {
    Some(match cs {
        "input" | "include" | "subfile" | "InputIfFileExists" => LoadKind::Input,
        "usepackage" | "RequirePackage" | "RequirePackageWithOptions" => LoadKind::Package,
        "documentclass" | "LoadClass" | "LoadClassWithOptions" => LoadKind::Class,
        _ => return None,
    })
}

/// Whether `cs` is a definition primitive or a kernel defining command.
fn defines(cs: &str) -> bool {
    matches!(
        cs,
        "def"
            | "gdef"
            | "edef"
            | "xdef"
            | "let"
            | "chardef"
            | "mathchardef"
            | "countdef"
            | "dimendef"
            | "skipdef"
            | "toksdef"
            | "font"
            | "newcommand"
            | "renewcommand"
            | "providecommand"
            | "DeclareRobustCommand"
            | "NewDocumentCommand"
            | "RenewDocumentCommand"
            | "ProvideDocumentCommand"
            | "DeclareDocumentCommand"
            | "NewExpandableDocumentCommand"
            | "RenewExpandableDocumentCommand"
            | "ProvideExpandableDocumentCommand"
            | "DeclareExpandableDocumentCommand"
            | "NewCommandCopy"
            | "RenewCommandCopy"
            | "DeclareCommandCopy"
            | "csdef"
            | "csgdef"
            | "csedef"
            | "csxdef"
            | "cslet"
            | "csletcs"
    ) || ["cs_new", "cs_set", "cs_gset"].iter().any(|p| cs.starts_with(p))
}

/// Plain text of a group's characters, `None` if it holds a control sequence.
fn chars(group: &Group) -> Option<String> {
    group
        .inner
        .iter()
        .map(|l| match l.tok {
            Tok::Chr(c, _) => Some(c),
            _ => None,
        })
        .collect()
}

/// A load's names: `{a,b}`, or `\input file` read up to a space.
fn load_names(scan: &mut Scan) -> Vec<String> {
    scan.optional();
    let Some(arg) = scan.argument() else { return Vec::new() };
    let mut text = chars(&arg).unwrap_or_default();
    if arg.start == arg.inner_start {
        while let Some(Lexeme { tok: Tok::Chr(c, Catcode::Letter | Catcode::Other), .. }) = scan.peek() {
            text.push(*c);
            scan.advance();
        }
    }
    text.split(',').map(|n| n.trim().to_string()).filter(|n| !n.is_empty()).collect()
}

/// The name a definition command defines, and where it stands.
fn defined(scan: &mut Scan) -> Option<(String, Pos)> {
    scan.star();
    let arg = scan.argument()?;
    match arg.inner.as_slice() {
        [one] if matches!(one.tok, Tok::Cs(_)) => Some((scan.name(one)?.to_string(), one.start)),
        _ => Some((chars(&arg)?.trim().to_string(), arg.inner_start)),
    }
}

fn catcodes(text: &str) -> CatcodeTable {
    let mut cats = CatcodeTable::latex();
    cats.set('@', Catcode::Letter);
    if text.contains("\\ExplSyntaxOn") || text.contains("\\ProvidesExpl") {
        cats.set('_', Catcode::Letter);
        cats.set(':', Catcode::Letter);
    }
    cats
}

/// Every site defining `\name` in the files the document's source loads,
/// breadth first from the main file and `current`.
pub fn sites(analysis: &Analysis, current: &str, name: &str) -> Vec<Site> {
    let main = PathBuf::from(analysis.file_name(analysis.main_file));
    let base = main.parent().map(Path::to_path_buf).unwrap_or_default();
    RESOLVERS.with_borrow_mut(|resolvers| {
        let resolver = resolvers
            .entry(base.clone())
            .or_insert_with(|| Resolver::for_project(&analysis.settings, &analysis.project));
        let mut queue: VecDeque<(PathBuf, usize)> = [(main, 0), (PathBuf::from(current), 0)].into();
        let mut seen = HashSet::default();
        let mut found = Vec::new();
        while let Some((path, depth)) = queue.pop_front() {
            if seen.len() >= MAX_FILES || !seen.insert(path.clone()) {
                continue;
            }
            let Ok(body) = crate::overlay::read_to_string(&path) else { continue };
            let cats = catcodes(&body);
            let text = Rc::new(Text::new(body));
            let Some(mut scan) = Scan::at(&text, Pos::new(1, 1), cats) else { continue };
            let mut hits = Vec::new();
            while let Some(lexeme) = scan.advance() {
                let Some(cs) = scan.name(&lexeme).map(str::to_string) else { continue };
                if let Some(kind) = load_kind(&cs) {
                    for load in load_names(&mut scan) {
                        let found = resolver.resolve(&load, kind, &base);
                        if let Some(file) = found.filter(|_| depth < MAX_DEPTH) {
                            queue.push_back((file, depth + 1));
                        }
                    }
                } else if defines(&cs)
                    && let Some((defined, pos)) = defined(&mut scan)
                    && defined == name
                {
                    hits.push(pos);
                }
            }
            let path = path.display().to_string();
            found.extend(hits.into_iter().map(|pos| Site { path: path.clone(), text: text.clone(), pos }));
        }
        found
    })
}
