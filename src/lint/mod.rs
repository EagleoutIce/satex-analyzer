//! Lint rules: each carries its documentation and implementation.

pub mod apply;
pub mod bibliography;
pub mod build;
pub mod correctness;
pub mod fix;
pub mod pdf;
pub mod performance;
pub mod precision;
mod shared;
pub mod style;
pub mod suppress;
pub mod typography;
pub mod units;

use serde_json::json;

use crate::lint::shared::document_start;
use crate::machine::Analysis;
use crate::query::Record;
use crate::tex::{Span, Sym};

fn all() -> impl Iterator<Item = &'static Rule> {
    correctness::RULES
        .iter()
        .chain(bibliography::RULES)
        .chain(pdf::RULES)
        .chain(style::RULES)
        .chain(typography::RULES)
        .chain(units::RULES)
        .chain(performance::RULES)
        .chain(precision::RULES)
        .chain(build::RULES)
        .chain(suppress::RULES)
        .chain(std::iter::once(&PREAMBLE_COST))
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

/// One lint rule: its code, [`Category`] and [`Severity`], the explanation
/// `--explain` prints, and the function that runs it against a [`Report`].
pub struct Rule {
    pub code: &'static str,
    pub category: Category,
    pub severity: Severity,
    pub summary: &'static str,
    pub explanation: &'static str,
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
