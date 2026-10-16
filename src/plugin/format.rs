//! Output formats.  Each one renders the records a command produced; adding
//! one is a variant and a `render` arm.

use clap::ValueEnum;

use crate::query::Record;
use crate::render::{self, Links};

named_enum! {
    /// How `satex` prints a [`Record`]: an aligned table, JSON, CSV, Markdown,
    /// DOT, GitHub Actions annotations, or SARIF. [`Format::render`] does the
    /// printing.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, ValueEnum)]
    #[value(rename_all = "lowercase")]
    pub enum Format {
        /// Aligned table for a terminal, with color and links.
        Text => "text",
        /// A short text view. For `satex trace`: what each call of the main
        /// file expands to and each conditional's outcome, at most 8 lines.
        Compact => "compact",
        /// One JSON array of objects, one object per record.
        Json => "json",
        /// Comma-separated, one record per line.
        Csv => "csv",
        /// A Markdown table, for a report or a pull request.
        Markdown => "markdown",
        /// Graphviz, for `satex dependencies`.
        Dot => "dot",
        /// GitHub Actions annotations.
        Github => "github",
        /// SARIF 2.1.0, for code scanning.
        Sarif => "sarif",
        /// Language Server Protocol diagnostics and quick-fix code actions.
        Lsp => "lsp",
    }
}

impl Format {
    /// Whether this format carries the notes and trees that only read well in
    /// a terminal.
    pub fn is_text(self) -> bool {
        self == Format::Text
    }

    pub fn render(self, records: &[Record], links: Links, version: &str) -> String {
        match self {
            Format::Text | Format::Compact => render::table(records, links),
            Format::Json => crate::query::render_json(records),
            Format::Csv => render::csv(records),
            Format::Markdown => render::markdown(records),
            Format::Dot | Format::Github => render::github(records),
            Format::Sarif => render::sarif(records, version),
            Format::Lsp => render::lsp(records),
        }
    }
}
