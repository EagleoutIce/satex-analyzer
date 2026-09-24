//! Output formats.  Each one renders the records a command produced; adding
//! one is a variant and a `render` arm.

use clap::ValueEnum;

use crate::query::Record;
use crate::render::{self, Links};

/// How `satex` prints a [`Record`]: an aligned table, JSON, CSV, Markdown,
/// DOT, GitHub Actions annotations, or SARIF. [`Format::render`] does the
/// printing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
#[value(rename_all = "lowercase")]
pub enum Format {
    /// Aligned table for a terminal, with color and links.
    Text,
    /// One JSON array of objects, one object per record.
    Json,
    /// Comma-separated, one record per line.
    Csv,
    /// A Markdown table, for a report or a pull request.
    Markdown,
    /// Graphviz, for `satex dependencies`.
    Dot,
    /// GitHub Actions annotations.
    Github,
    /// SARIF 2.1.0, for code scanning.
    Sarif,
    /// Language Server Protocol diagnostics and quick-fix code actions.
    Lsp,
}

impl Format {
    /// Every output format, for `--version`.
    pub const ALL: &'static [Format] = &[
        Format::Text,
        Format::Json,
        Format::Csv,
        Format::Markdown,
        Format::Dot,
        Format::Github,
        Format::Sarif,
        Format::Lsp,
    ];
    pub fn as_str(self) -> &'static str {
        match self {
            Format::Text => "text",
            Format::Json => "json",
            Format::Csv => "csv",
            Format::Markdown => "markdown",
            Format::Dot => "dot",
            Format::Github => "github",
            Format::Sarif => "sarif",
            Format::Lsp => "lsp",
        }
    }

    /// Whether this format carries the notes and trees that only read well in
    /// a terminal.
    pub fn is_text(self) -> bool {
        self == Format::Text
    }

    pub fn render(self, records: &[Record], links: Links, version: &str) -> String {
        match self {
            Format::Text => render::table(records, links),
            Format::Json => crate::query::render_json(records),
            Format::Csv => render::csv(records),
            Format::Markdown => render::markdown(records),
            Format::Dot | Format::Github => render::github(records),
            Format::Sarif => render::sarif(records, version),
            Format::Lsp => render::lsp(records),
        }
    }
}
