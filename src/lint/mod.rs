//! Lint rules: each carries its documentation and implementation.

pub mod apply;
pub mod bibliography;
pub mod build;
pub mod chktex;
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
            true => format!("{} {}", self.source.as_str(), self.at),
            false => format!("{} § {}", self.source.as_str(), self.at),
        }
    }

    pub fn url(&self) -> String {
        self.source.url().to_string()
    }
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
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Chktex => "chktex",
            Source::Csquotes => "csquotes",
            Source::TeXbook => "The TeXbook",
            Source::TeXWeb => "tex.web",
            Source::Source2e => "source2e",
            Source::Fntguide => "fntguide",
            Source::Usrguide => "usrguide",
            Source::Clsguide => "clsguide",
            Source::Interface3 => "interface3",
            Source::L2tabu => "l2tabu",
            Source::Companion => "The LaTeX Companion",
            Source::Siunitx => "siunitx",
            Source::Microtype => "microtype",
            Source::Babel => "babel",
            Source::Biblatex => "biblatex",
            Source::Amsmath => "amsmath",
            Source::Hyperref => "hyperref",
            Source::Latexmk => "latexmk",
            Source::Arara => "arara",
            Source::Glossaries => "glossaries",
            Source::Graphics => "graphics",
            Source::Bibtex => "btxdoc",
            Source::Lmodern => "lmodern",
            Source::Pdftex => "pdftex",
            Source::Luatex => "luatex",
            Source::Pdf => "PDF 32000",
            Source::SiBrochure => "SI brochure",
            Source::TexLive => "TeX Live manual",
            Source::MikTeX => "MiKTeX manual",
            Source::Xcolor => "xcolor",
        }
    }

    /// Whether `at` is a plain number rather than a section.
    pub fn numbered(self) -> bool {
        self == Source::Chktex
    }

    pub fn url(self) -> &'static str {
        match self {
            Source::Chktex => "https://www.nongnu.org/chktex/",
            Source::Csquotes => "https://ctan.org/pkg/csquotes",
            Source::TeXbook => "https://ctan.org/pkg/texbook",
            Source::TeXWeb => "https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web",
            Source::Source2e => "https://ctan.org/pkg/source2e",
            Source::Fntguide => "https://ctan.org/pkg/fntguide",
            Source::Usrguide => "https://ctan.org/pkg/usrguide",
            Source::Clsguide => "https://ctan.org/pkg/clsguide",
            Source::Interface3 => "https://ctan.org/pkg/l3kernel",
            Source::L2tabu => "https://ctan.org/pkg/l2tabu",
            Source::Companion => "https://ctan.org/pkg/tlc3-examples",
            Source::Siunitx => "https://ctan.org/pkg/siunitx",
            Source::Microtype => "https://ctan.org/pkg/microtype",
            Source::Babel => "https://ctan.org/pkg/babel",
            Source::Biblatex => "https://ctan.org/pkg/biblatex",
            Source::Amsmath => "https://ctan.org/pkg/amsmath",
            Source::Hyperref => "https://ctan.org/pkg/hyperref",
            Source::Latexmk => "https://ctan.org/pkg/latexmk",
            Source::Arara => "https://ctan.org/pkg/arara",
            Source::Glossaries => "https://ctan.org/pkg/glossaries",
            Source::Graphics => "https://ctan.org/pkg/graphics",
            Source::Bibtex => "https://ctan.org/pkg/bibtex",
            Source::Lmodern => "https://ctan.org/pkg/lm",
            Source::Pdftex => "https://ctan.org/pkg/pdftex",
            Source::Luatex => "https://ctan.org/pkg/luatex",
            Source::Pdf => "https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf",
            Source::SiBrochure => "https://www.bipm.org/en/publications/si-brochure",
            Source::TexLive => "https://tug.org/texlive/doc.html",
            Source::MikTeX => "https://docs.miktex.org/manual/",
            Source::Xcolor => "https://ctan.org/pkg/xcolor",
        }
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
    pub run: fn(&mut Report),
}

/// What one [`Rule`] can see and write while it runs: the finished
/// [`Analysis`], and the [`Record`]s it has produced so far via
/// [`Report::add`].
pub struct Report<'a> {
    analysis: &'a Analysis,
    sources: &'a fix::Sources,
    rule: &'static Rule,
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

    /// A finding whose fix carries edits; [`Report::edits`] builds them.
    pub fn add_fix(&mut self, span: Span, name: &str, message: String, fix: fix::Fix) {
        self.add_fix_as(self.rule.severity, span, name, message, fix);
    }

    pub fn add_fix_as(&mut self, severity: Severity, span: Span, name: &str, message: String, fix: fix::Fix) {
        let edits = (!fix.edits.is_empty()).then_some((fix.applicability, fix.edits));
        self.push(severity, span, name, message, Some(fix.description), edits);
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
        record.insert("origin".into(), json!(crate::query::origin(self.analysis, span.file)));
        record.insert("message".into(), json!(message));
        record.insert("fix".into(), json!(fix));
        if let Some((applicability, edits)) = edits {
            record.insert("applicability".into(), json!(applicability.as_str()));
            record.insert("edits".into(), edits.iter().map(fix::Edit::to_json).collect());
        }
        self.out.push(record);
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
        record.insert("origin".into(), json!("build"));
        record.insert("message".into(), json!(message));
        record.insert("fix".into(), json!(fix));
        self.out.push(record);
    }
}

pub fn lint(analysis: &Analysis) -> Vec<Record> {
    let mut out = Vec::new();
    let sources = fix::Sources::default();
    for rule in all().filter(|rule| !analysis.settings.lint_off.iter().any(|code| code == rule.code)) {
        let mut report = Report { analysis, sources: &sources, rule, out: Vec::new() };
        (rule.run)(&mut report);
        out.append(&mut report.out);
    }
    // A raised error another rule explains at the same place is its.
    let place = |r: &Record| (r.get("path").cloned(), r.get("line").cloned(), r.get("col").cloned());
    let explained: std::collections::HashSet<_> =
        out.iter().filter(|r| r.get("code").is_some_and(|c| c != "raised-error")).map(place).collect();
    out.retain(|r| r.get("code").is_none_or(|c| c != "raised-error") || !explained.contains(&place(r)));
    suppress::apply(analysis, out)
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
            record
        })
        .collect()
}

pub fn explanation(code: &str) -> Option<&'static Rule> {
    all().find(|rule| rule.code == code)
}

static PREAMBLE_COST: Rule = Rule {
    code: "preamble-cost",
    category: Category::Performance,
    severity: Severity::Info,
    summary: "what the preamble costs on every build",
    explanation: "\
The packages the document asks for and the files read before
`\\begin{document}`.  This work repeats on every compilation.

Fix by precompiling the preamble into a format with `mylatexformat` or
`precompiled preamble` support in your editor, which skips it entirely.",
    run: preamble_cost,
};

fn preamble_cost(report: &mut Report) {
    let analysis = report.analysis;
    let direct =
        analysis.facts.loads.iter().filter(|l| l.kind.is_package() && l.span.file == analysis.main_file).count();
    let span = document_start(analysis).unwrap_or_default();
    report.add(
        span,
        "",
        format!(
            "{direct} packages requested directly, {} files read",
            analysis.preamble_files.unwrap_or(analysis.files.len()),
        ),
        Some("precompile the preamble into a format to skip this on every build".into()),
    );
}
