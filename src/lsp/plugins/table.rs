//! Tables: the environments whose code reaches `\\@array`. Their features
//! live in [`crate::lsp::dialogs`]: a code lens and hover link to the grid
//! dialog (`satex/dialog`), booktabs and siunitx rewrites as code actions.

use super::TypePlugin;
use crate::analysis::roles::Ty;
use crate::project::config::TypePlugins;

pub(super) struct Table;

impl TypePlugin for Table {
    fn name(&self) -> &'static str {
        "table"
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
}
