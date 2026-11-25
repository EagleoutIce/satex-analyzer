//! memoize (<https://ctan.org/pkg/memoize>): a picture is typeset once and shipped as an
//! extra page of the document, its extern; a later run includes the page extracted from the
//! document's PDF instead of running the picture's code. The analysis needs nothing of its own:
//! satex runs memoize's code, which finds a memo only by the md5 sum of the code it sees
//! (memoize.sty `\mmz@cmemo@path`), so an edited picture is compiled again.
//!
//! The `satex` build extracts the externs itself (`compile.memoize`): memoize's own TeX method
//! (memoize.sty `\mmz@tex@extraction@systemcall`) run as a build tool between passes, so no
//! shell escape and no Perl or Python module is needed, and memoize is told not to extract.

use std::path::Path;

/// TeX code read before the document: memoize leaves extraction to the build
/// (memoize.sty `extract/no`). A document's own `extract=` option comes later and wins.
pub const PRETEX: &str = r"\PassOptionsToPackage{extract=no}{memoize}";

/// Whether the document may load memoize: its source names it, or the last pass read it (`fls`,
/// pdfTeX's `-recorder`). [`PRETEX`] is harmless where memoize is never loaded, so a guess suffices.
pub fn may_load(source: &str, fls: &Path) -> bool {
    source.contains("memoize") || std::fs::read_to_string(fls).is_ok_and(|f| f.contains("/memoize.sty"))
}

/// The externs `.mmz` lists as new (`\mmzNewExtern{path}{page}{width}{height}`, memoize.sty
/// `record/mmz/new extern`) whose file is not there yet: path, page, width, height.
fn wanted(mmz: &str, at: &Path) -> Vec<[String; 4]> {
    let args = |line: &str| -> Option<[String; 4]> {
        let mut rest = line.trim().strip_prefix(r"\mmzNewExtern")?;
        let mut arg = || {
            let (value, after) = rest.trim_start().strip_prefix('{')?.split_once('}')?;
            rest = after;
            Some(value.trim().to_string())
        };
        Some([arg()?, arg()?, arg()?, arg()?])
    };
    mmz.lines().filter_map(args).filter(|[path, ..]| !at.join(path).is_file()).collect()
}

/// The pdfTeX runs that extract each extern `<job>.mmz` in `at` asks for from `<job>.pdf`, as
/// memoize's TeX method does (memoize-extract-one.tex: the page at its recorded size, checked
/// within 0.01pt): each run's arguments, its first line and the extern it makes.
pub fn extractions(job: &str, at: &Path) -> Vec<([String; 3], String, String)> {
    let mmz = std::fs::read_to_string(at.join(format!("{job}.mmz"))).unwrap_or_default();
    let run = |[path, page, width, height]: [String; 4]| {
        let base = path.strip_suffix(".pdf")?.to_string();
        let script = format!(
            r"\def\fromdocument{{{job}.pdf}}\def\pagenumber{{{page}}}\def\expectedwidth{{{width}}}\def\expectedheight{{{height}}}\input memoize-extract-one"
        );
        Some((["-halt-on-error".into(), "-interaction=batchmode".into(), format!("-jobname={base}")], script, path))
    };
    wanted(&mmz, at).into_iter().filter_map(run).collect()
}
