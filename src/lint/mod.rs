//! Lint rules: each carries its documentation and implementation.

pub mod apply;
pub mod bibliography;
pub mod build;
pub mod chktex;
pub mod cite;
pub mod colors;
pub mod correctness;
pub mod dashes;
pub mod fix;
pub mod italics;
pub mod mathtext;
pub mod numbering;
pub mod packages;
pub mod pdf;
pub mod performance;
pub mod precision;
pub mod quotes;
mod shared;
pub mod sourcetext;
pub mod spacing;
pub mod style;
pub mod suppress;
pub mod tables;
pub mod text;
pub mod typography;
pub mod units;
pub mod verbatim;

use serde_json::json;

use crate::lint::shared::Index;
use crate::machine::Analysis;
use crate::query::Record;
use crate::tex::{Span, Sym};

pub fn all() -> impl Iterator<Item = &'static Rule> {
    correctness::RULES
        .iter()
        .chain(bibliography::RULES)
        .chain(pdf::RULES)
        .chain(packages::RULES)
        .chain(style::RULES)
        .chain(dashes::RULES)
        .chain(quotes::RULES)
        .chain(spacing::RULES)
        .chain(mathtext::RULES)
        .chain(numbering::RULES)
        .chain(italics::RULES)
        .chain(sourcetext::RULES)
        .chain(typography::RULES)
        .chain(units::RULES)
        .chain(performance::RULES)
        .chain(precision::RULES)
        .chain(build::RULES)
        .chain(suppress::RULES)
        .chain(colors::RULES)
        .chain(tables::RULES)
}

/// Which lint family a [`Rule`] belongs to. `satex lint --filter category=…`
/// selects by it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Category {
    Correctness,
    /// What the document lets the build do to the machine around it: running
    /// programs, writing files, reading paths outside the project.
    Security,
    Style,
    Performance,
    Precision,
}

impl Category {
    pub fn as_str(self) -> &'static str {
        match self {
            Category::Correctness => "correctness",
            Category::Security => "security",
            Category::Style => "style",
            Category::Performance => "performance",
            Category::Precision => "precision",
        }
    }
}

/// How serious a lint [`Rule`] considers its own finding: error, warning or
/// info. Distinct from [`crate::facts::Severity`], which is about the
/// analysis's own certainty.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        }
    }
}

/// A published source a rule follows, so a reader can check the rule
/// against it instead of against satex.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ref {
    pub source: Source,
    /// Which part of it: a chktex warning number, a section, an appendix.
    pub at: &'static str,
}

impl Ref {
    /// How a finding cites it: `chktex 8`, `tex.web § 1034`.
    pub fn cite(&self) -> String {
        match self.source.numbered() {
            _ if self.at.starts_with("chapter ") || self.at.starts_with("appendix ") => {
                format!("{}, {}", self.source.as_str(), self.at)
            }
            true => format!("{} {}", self.source.as_str(), self.at),
            false => format!("{} § {}", self.source.as_str(), self.at),
        }
    }

    /// URL pinned to the exact line for [`Source::TeXWeb`] (`at` is a WEB
    /// module number), so the citation lands on the paragraph, not the
    /// file's top. Falls back to the file's own URL otherwise.
    pub fn url(&self) -> String {
        if self.source == Source::TeXWeb
            && let Ok(section) = self.at.parse::<u32>()
            && let Some(line) = tex_web_section_line(section)
        {
            return format!("{}#L{line}", self.source.url());
        }
        if self.source == Source::TeXbook
            && let Some((kind, n)) = self.at.split_once(' ')
            && matches!(kind, "chapter" | "appendix")
        {
            return format!("{}#{kind}-{}", self.source.url(), n.to_lowercase());
        }
        self.source.url().to_string()
    }
}

/// tex.web's URL at the commit `doc/src/tex-web-sha.txt` names, written
/// together with `doc/src/tex-web-sections.tsv` by
/// `satex-doc --refresh-citations`; CI re-resolves both against upstream
/// `trunk` (`--check-citations`) and fails on drift.
fn tex_web_base_url() -> &'static str {
    static URL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    URL.get_or_init(|| {
        let sha = include_str!("../../doc/src/tex-web-sha.txt").trim();
        format!("https://github.com/TeX-Live/texlive-source/blob/{sha}/texk/web2c/tex.web")
    })
    .as_str()
}

/// The line `section` starts on in the pinned tex.web
/// (`doc/src/tex-web-sections.tsv`). Exposed so `satex-doc` can also pin a
/// `tex.web § N` citation found in prose, not just a [`Ref`].
pub fn tex_web_section_line(section: u32) -> Option<u32> {
    static SECTIONS: std::sync::OnceLock<crate::fixed::HashMap<u32, u32>> = std::sync::OnceLock::new();
    SECTIONS
        .get_or_init(|| {
            const TSV: &str = include_str!("../../doc/src/tex-web-sections.tsv");
            TSV.lines()
                .filter_map(|line| {
                    let (n, l) = line.split_once('\t')?;
                    Some((n.parse().ok()?, l.parse().ok()?))
                })
                .collect()
        })
        .get(&section)
        .copied()
}

/// The works satex's rules follow.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Source {
    Chktex,
    Csquotes,
    TeXbook,
    TeXWeb,
    Source2e,
    Fntguide,
    Usrguide,
    Clsguide,
    Interface3,
    L2tabu,
    Companion,
    Siunitx,
    Microtype,
    Babel,
    Biblatex,
    Amsmath,
    Hyperref,
    Latexmk,
    Arara,
    Glossaries,
    Graphics,
    Bibtex,
    Lmodern,
    Pdftex,
    Luatex,
    /// ISO 32000-1, the PDF specification.
    Pdf,
    /// The BIPM's SI brochure, which siunitx follows.
    SiBrochure,
    TexLive,
    MikTeX,
    Xcolor,
    Cleveref,
    Booktabs,
}

impl Source {
    /// The name a citation gives the work, and where to read it.
    fn entry(self) -> (&'static str, &'static str) {
        match self {
            Source::Chktex => ("chktex", "https://www.nongnu.org/chktex/"),
            Source::Csquotes => ("csquotes", "https://ctan.org/pkg/csquotes"),
            Source::TeXbook => ("The TeXbook", "https://ctan.org/pkg/texbook"),
            Source::TeXWeb => ("tex.web", tex_web_base_url()),
            Source::Source2e => ("source2e", "https://ctan.org/pkg/source2e"),
            Source::Fntguide => ("fntguide", "https://ctan.org/pkg/fntguide"),
            Source::Usrguide => ("usrguide", "https://ctan.org/pkg/usrguide"),
            Source::Clsguide => ("clsguide", "https://ctan.org/pkg/clsguide"),
            Source::Interface3 => ("interface3", "https://ctan.org/pkg/l3kernel"),
            Source::L2tabu => ("l2tabu", "https://ctan.org/pkg/l2tabu"),
            Source::Companion => ("The LaTeX Companion", "https://ctan.org/pkg/tlc3-examples"),
            Source::Siunitx => ("siunitx", "https://ctan.org/pkg/siunitx"),
            Source::Microtype => ("microtype", "https://ctan.org/pkg/microtype"),
            Source::Babel => ("babel", "https://ctan.org/pkg/babel"),
            Source::Biblatex => ("biblatex", "https://ctan.org/pkg/biblatex"),
            Source::Amsmath => ("amsmath", "https://ctan.org/pkg/amsmath"),
            Source::Hyperref => ("hyperref", "https://ctan.org/pkg/hyperref"),
            Source::Latexmk => ("latexmk", "https://ctan.org/pkg/latexmk"),
            Source::Arara => ("arara", "https://ctan.org/pkg/arara"),
            Source::Glossaries => ("glossaries", "https://ctan.org/pkg/glossaries"),
            Source::Graphics => ("graphics", "https://ctan.org/pkg/graphics"),
            Source::Bibtex => ("btxdoc", "https://ctan.org/pkg/bibtex"),
            Source::Lmodern => ("lmodern", "https://ctan.org/pkg/lm"),
            Source::Pdftex => ("pdftex", "https://ctan.org/pkg/pdftex"),
            Source::Luatex => ("luatex", "https://ctan.org/pkg/luatex"),
            Source::Pdf => {
                ("PDF 32000", "https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf")
            }
            Source::SiBrochure => ("SI brochure", "https://www.bipm.org/en/publications/si-brochure"),
            Source::TexLive => ("TeX Live manual", "https://tug.org/texlive/doc.html"),
            Source::MikTeX => ("MiKTeX manual", "https://docs.miktex.org/manual/"),
            Source::Xcolor => ("xcolor", "https://ctan.org/pkg/xcolor"),
            Source::Cleveref => ("cleveref", "https://ctan.org/pkg/cleveref"),
            Source::Booktabs => ("booktabs", "https://ctan.org/pkg/booktabs"),
        }
    }

    pub fn as_str(self) -> &'static str {
        self.entry().0
    }

    /// Whether `at` is a plain number rather than a section.
    pub fn numbered(self) -> bool {
        self == Source::Chktex
    }

    pub fn url(self) -> &'static str {
        self.entry().1
    }
}

/// One lint rule: its code, [`Category`] and [`Severity`], the explanation
/// `--explain` prints, the works it follows, and the function that runs it
/// against a [`Report`].
pub struct Rule {
    pub code: &'static str,
    pub category: Category,
    pub severity: Severity,
    pub summary: &'static str,
    pub explanation: &'static str,
    /// What the rule is derived from, for `--explain` and the coverage
    /// tables the documentation generates.
    pub references: &'static [Ref],
    /// The quick-fix kinds its findings can carry; empty when none.
    pub fixes: &'static [fix::Applicability],
    pub run: fn(&mut Report),
}

/// What one [`Rule`] can see and write while it runs: the finished
/// [`Analysis`], and the [`Record`]s it has produced so far via
/// [`Report::add`].
pub struct Report<'a> {
    analysis: &'a Analysis,
    sources: &'a fix::Sources,
    text: &'a text::View<'a>,
    index: &'a Index<'a>,
    rule: &'static Rule,
    /// [`cites`] for `rule`, built once instead of once per finding.
    reference: String,
    out: Vec<Record>,
}

impl<'a> Report<'a> {
    pub fn analysis(&self) -> &'a Analysis {
        self.analysis
    }

    pub fn add(&mut self, span: Span, name: &str, message: String, fix: Option<String>) {
        self.add_as(self.rule.severity, span, name, message, fix);
    }

    /// The same finding at a lower severity: a defect that the run never
    /// reaches cannot break the build.
    pub fn add_as(&mut self, severity: Severity, span: Span, name: &str, message: String, fix: Option<String>) {
        self.push(severity, span, name, message, fix, None);
    }

    /// A finding whose fix carries edits. [`Report::edits`] builds them.
    pub fn add_fix(&mut self, span: Span, name: &str, message: String, fix: fix::Fix) {
        self.add_fix_as(self.rule.severity, span, name, message, fix);
    }

    pub fn add_fix_as(&mut self, severity: Severity, span: Span, name: &str, message: String, fix: fix::Fix) {
        let edits = (!fix.edits.is_empty()).then_some((fix.applicability, fix.edits));
        self.push(severity, span, name, message, Some(fix.description), edits);
    }

    /// A finding over the characters `rewrite` covers, whose fix writes
    /// what belongs there in their place.
    pub fn rewrite(&mut self, rewrite: text::Rewrite) {
        let view = self.text;
        let Some(span) = view.span(rewrite.range.start) else { return };
        let edits = rewrite.want.as_ref().and_then(|want| text::replace(&self.edits(), view, rewrite.range, want));
        let description = rewrite.fix.unwrap_or_else(|| format!("write `{}`", rewrite.want.unwrap_or_default()));
        let fix = fix::Fix::new(description, rewrite.applicability, edits);
        match rewrite.severity {
            Some(severity) => self.add_fix_as(severity, span, &rewrite.name, rewrite.message, fix),
            None => self.add_fix(span, &rewrite.name, rewrite.message, fix),
        }
    }

    /// The characters the document set, built once for every rule.
    pub fn text(&self) -> &'a text::View<'a> {
        self.text
    }

    /// The fact indexes shared by every rule of one run.
    pub fn index(&self) -> &'a Index<'a> {
        self.index
    }

    pub fn edits(&self) -> fix::Edits<'a> {
        fix::Edits { analysis: self.analysis, sources: self.sources }
    }

    fn push(
        &mut self,
        severity: Severity,
        span: Span,
        name: &str,
        message: String,
        fix: Option<String>,
        edits: Option<(fix::Applicability, Vec<fix::Edit>)>,
    ) {
        let mut record = crate::query::place(self.analysis, span);
        record.insert("code".into(), json!(self.rule.code));
        record.insert("category".into(), json!(self.rule.category.as_str()));
        record.insert("severity".into(), json!(severity.as_str()));
        record.insert("name".into(), json!(name));
        record.insert("reference".into(), json!(self.reference));
        let origin = self.index.origin(span.file);
        record.insert("origin".into(), json!(origin));
        record.insert("message".into(), json!(message));
        // A fix only ever changes the user's own files: its edits all land
        // in files satex may change, and one with no edits, only advice, is
        // given where the finding is in the user's own files.
        let edits = edits.filter(|(_, edits)| {
            !edits.is_empty() && edits.iter().all(|e| self.sources.editable(self.analysis, &e.path))
        });
        let advice = edits.is_some() || origin == "document";
        record.insert("fix".into(), json!(fix.filter(|_| advice)));
        if let Some((applicability, edits)) = edits {
            debug_assert!(
                self.rule.fixes.contains(&applicability),
                "{} emits an undeclared {} fix",
                self.rule.code,
                applicability.as_str()
            );
            record.insert("applicability".into(), json!(applicability.as_str()));
            record.insert("edits".into(), edits.iter().map(fix::Edit::to_json).collect());
        }
        self.out.push(record);
    }

    /// A place for a message: the file it is in as well as the line and
    /// column, so that it names somewhere whichever file the finding is in.
    pub fn place(&self, span: Span) -> String {
        format!("{}:{}:{}", self.analysis.short_name(span.file), span.line, span.col)
    }

    /// Where a finding about an occurrence stands: the document's call it
    /// came out of, named in the returned note, or its own place.
    pub fn site(&self, o: &crate::facts::Occurrence) -> (Span, Option<String>) {
        let Some((sym, at)) = self.index().call(o) else { return (o.span, None) };
        let name = self.cs(sym);
        let defs = &self.analysis.facts.defs;
        let def = defs.iter().find(|d| d.name == sym && self.index().origin(d.span.file) == "document");
        let note = match def {
            Some(d) => format!(" (via {name}, defined at {})", self.place(d.span)),
            None => format!(" (via {name})"),
        };
        (at, Some(note))
    }

    /// A finding on an error raised after `cause`, which may have led to
    /// it: one severity lower, naming the cause, without edits.
    /// A finding that may only follow from an earlier gap: a possible one,
    /// so a note, never a warning or an error.
    pub fn add_doubted(&mut self, span: Span, name: &str, message: String, cause: &str) {
        self.add_as(Severity::Info, span, name, format!("{message} (may follow from {cause}: check that first)"), None);
    }

    /// The note for a finding that something is absent from the whole
    /// document, when the run did not read all of it: the part it left
    /// may hold what is missing.  `None` when the run read everything.
    pub fn unread(&self, later: &str) -> Option<String> {
        let unread = |d: &&crate::facts::Diagnostic| {
            matches!(d.code, "file-not-read" | "analysis-ended-early" | "limit-reached")
        };
        let d = self.analysis.facts.diagnostics.iter().find(unread)?;
        Some(format!(" (it may be {later} later: at {}, {})", self.place(d.span), d.message))
    }

    pub fn cs(&self, sym: Sym) -> String {
        self.analysis.interner.cs(sym)
    }

    /// A finding in a file the run did not read as TeX: a build
    /// configuration beside the document.
    pub fn add_in(&mut self, path: &std::path::Path, line: u32, name: &str, message: String, fix: Option<String>) {
        let short = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let mut record = serde_json::Map::new();
        record.insert("file".into(), json!(short));
        record.insert("path".into(), json!(path.display().to_string()));
        record.insert("line".into(), json!(line));
        record.insert("col".into(), json!(1));
        record.insert("code".into(), json!(self.rule.code));
        record.insert("category".into(), json!(self.rule.category.as_str()));
        record.insert("severity".into(), json!(self.rule.severity.as_str()));
        record.insert("name".into(), json!(name));
        record.insert("reference".into(), json!(self.reference));
        record.insert("origin".into(), json!("build"));
        record.insert("message".into(), json!(message));
        record.insert("fix".into(), json!(fix));
        self.out.push(record);
    }
}

pub fn lint(analysis: &Analysis) -> Vec<Record> {
    let mut out = Vec::new();
    let sources = fix::Sources::default();
    crate::progress::bar("lint: the text as set", 0, 2, (0, 0), (0, 0), None, &[]);
    let text = text::View::of(analysis, &sources);
    crate::progress::bar("lint: what the run defined and used", 1, 2, (0, 0), (0, 0), None, &[]);
    let index = Index::of(analysis);
    let mut raised: crate::fixed::HashSet<usize> = crate::fixed::HashSet::default();
    let rules: Vec<&Rule> =
        all().filter(|rule| !analysis.settings.lint_off.iter().any(|code| code == rule.code)).collect();
    for (i, &rule) in rules.iter().enumerate() {
        let label = format!("lint {}", rule.code);
        crate::progress::bar(&label, i, rules.len(), (0, 0), (0, 0), None, &[]);
        crate::progress::tell(crate::progress::Status { text: label, phase: "lint".into(), ..Default::default() });
        let mut report = Report {
            analysis,
            sources: &sources,
            text: &text,
            index: &index,
            rule,
            reference: cites(rule),
            out: Vec::new(),
        };
        (rule.run)(&mut report);
        // One finding per source site: a rule met again there (a macro body
        // used several times) says nothing new.
        let mut sites = crate::fixed::HashSet::default();
        report.out.retain(|r| match (r.get("path"), r.get("line").and_then(|v| v.as_u64())) {
            (Some(path), Some(line)) => sites.insert((path.clone(), line, r.get("col").and_then(|v| v.as_u64()))),
            _ => true,
        });
        let start = out.len();
        out.append(&mut report.out);
        if rule.code == "raised-error" {
            raised.extend(start..out.len());
        }
    }
    crate::progress::clear();
    // A raised error another rule explains at the same place is its.  The
    // places a raised error stands at are few, so the line alone sieves the
    // rest out before any place is built.
    if !raised.is_empty() {
        fn place(r: &Record) -> (Option<&str>, Option<u64>, Option<u64>) {
            (
                r.get("path").and_then(|v| v.as_str()),
                r.get("line").and_then(|v| v.as_u64()),
                r.get("col").and_then(|v| v.as_u64()),
            )
        }
        let line = |r: &Record| r.get("line").and_then(|v| v.as_u64());
        let lines: crate::fixed::HashSet<_> = raised.iter().map(|i| line(&out[*i])).collect();
        let mut explained: crate::fixed::HashSet<_> = crate::fixed::HashSet::default();
        for (i, record) in out.iter().enumerate() {
            if !raised.contains(&i) && lines.contains(&line(record)) {
                explained.insert(place(record));
            }
        }
        let mut keep = vec![true; out.len()];
        for &i in &raised {
            keep[i] = !explained.contains(&place(&out[i]));
        }
        drop(explained);
        let mut at = 0;
        out.retain(|_| {
            at += 1;
            keep[at - 1]
        });
    }
    // A file the run writes holds what LaTeX, BibTeX or makeindex wrote
    // there: only an error in it, which still stops the build, is the
    // document's to know about.
    out.retain(|r| {
        r.get("severity").and_then(|v| v.as_str()) == Some("error")
            || !r.get("path").and_then(|v| v.as_str()).is_some_and(written_by_run)
    });
    let frozen: Vec<globset::GlobMatcher> = analysis
        .settings
        .project
        .frozen
        .iter()
        .filter_map(|g| globset::Glob::new(g).ok().map(|g| g.compile_matcher()))
        .collect();
    // The repository a file belongs to: its nearest ancestor holding a
    // `.git` (a directory, or the file a submodule checkout has).
    let repository =
        |path: &std::path::Path| path.ancestors().skip(1).find(|d| d.join(".git").exists()).map(|d| d.to_path_buf());
    let main = std::path::Path::new(analysis.file_name(analysis.main_file));
    let main = main.canonicalize().unwrap_or_else(|_| main.to_path_buf());
    let home = analysis.settings.project.freeze_other_repositories.then(|| repository(&main)).flatten();
    if !frozen.is_empty() || home.is_some() {
        out.retain(|r| {
            let Some(path) = r.get("path").and_then(|v| v.as_str()) else { return true };
            let path = std::path::Path::new(path);
            let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
            let foreign = home.as_ref().is_some_and(|home| repository(&path).is_some_and(|r| &r != home));
            !foreign && !frozen.iter().any(|g| g.is_match(&path))
        });
    }
    suppress::apply(analysis, out)
}

/// Whether `path` is a file the run, BibTeX or makeindex writes, not the
/// author: `.aux`, `.bbl`, `.toc` and the like.
pub fn written_by_run(path: &str) -> bool {
    std::path::Path::new(path).extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e,
            "aux" | "bbl" | "toc" | "lof" | "lot" | "out" | "nav" | "snm" | "vrb" | "ind" | "gls" | "glo" | "acr"
        )
    })
}

/// Every rule, by category and then from the most severe down.
pub fn rules() -> Vec<Record> {
    let mut rules: Vec<&Rule> = all().collect();
    rules.sort_by_key(|rule| (rule.category as u8, rule.severity as u8, rule.code));
    rules
        .into_iter()
        .map(|rule| {
            let mut record = serde_json::Map::new();
            record.insert("code".into(), json!(rule.code));
            record.insert("category".into(), json!(rule.category.as_str()));
            record.insert("severity".into(), json!(rule.severity.as_str()));
            record.insert("message".into(), json!(rule.summary));
            record.insert("reference".into(), json!(cites(rule)));
            record.insert("fixes".into(), json!(rule.fixes.iter().map(|a| a.as_str()).collect::<Vec<_>>()));
            record
        })
        .collect()
}

/// The rule's references as one citation string, empty when it has none.
pub fn cites(rule: &Rule) -> String {
    rule.references.iter().map(Ref::cite).collect::<Vec<_>>().join(", ")
}

pub fn explanation(code: &str) -> Option<&'static Rule> {
    all().find(|rule| rule.code == code)
}
