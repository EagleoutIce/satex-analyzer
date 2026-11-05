//! Floats: the environments whose code reaches `\\@float`. Their placement `[htbp!]` has a hover
//! per letter, completion of the letters and a dialog of toggles, in [`crate::lsp::dialogs`].

use super::TypePlugin;
use crate::analysis::machine::Analysis;
use crate::analysis::roles::Ty;
use crate::project::config::TypePlugins;
use crate::text::Text;
use lsp_types::CodeLens;

pub(super) struct Float;

impl TypePlugin for Float {
    fn name(&self) -> &'static str {
        "float"
    }
    fn category(&self) -> &'static str {
        "widget"
    }
    fn about(&self) -> &'static str {
        "a float's placement: what each letter lets LaTeX do, completion and a dialog of toggles"
    }
    // the run's environments trigger it, not a type of the text (`dialogs::targets`)
    fn types(&self) -> &'static [Ty] {
        &[]
    }
    fn features(&self) -> &'static [&'static str] {
        &["lens", "hover", "complete", "dialog"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.float.enabled
    }
    fn lenses(&self, analysis: &Analysis, doc: &Text, path: &str) -> Vec<CodeLens> {
        crate::lsp::dialogs::lenses(analysis, doc, path, crate::lsp::dialogs::Kind::Float)
    }
}
