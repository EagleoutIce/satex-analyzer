//! Tables: the environments whose code reaches `\\@array`. Their features
//! live in [`crate::lsp::dialogs`]: a code lens and hover link to the grid
//! dialog (`satex/dialog`), booktabs and siunitx rewrites as code actions.

use super::TypePlugin;
use crate::analysis::machine::Analysis;
use crate::analysis::roles::Ty;
use crate::linter::fix::Text;
use crate::project::config::TypePlugins;
use lsp_types::CodeLens;

pub(super) struct Table;

impl TypePlugin for Table {
    fn name(&self) -> &'static str {
        "table"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "a grid dialog for each tabular, booktabs and siunitx rewrites"
    }
    // the run's environments trigger it, not a type of the text (`dialogs::targets`)
    fn types(&self) -> &'static [Ty] {
        &[]
    }
    fn features(&self) -> &'static [&'static str] {
        &["lens", "actions", "dialog"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.table.enabled
    }
    fn lenses(&self, analysis: &Analysis, doc: &Text, path: &str) -> Vec<CodeLens> {
        crate::lsp::dialogs::lenses(analysis, doc, path, crate::lsp::dialogs::Kind::Table)
    }
}
