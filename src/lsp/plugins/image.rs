//! Figures: the files `\\includegraphics` reads (`Ty::File(Graphic)`). Their
//! features live in [`crate::lsp::dialogs`]: a code lens and hover link to
//! the crop, rotate and size dialog (`satex/dialog`), rotate and fit actions.

use super::TypePlugin;
use crate::analysis::roles::{FileKind, Ty};
use crate::project::config::TypePlugins;

pub(super) struct Image;

impl TypePlugin for Image {
    fn name(&self) -> &'static str {
        "image"
    }
    fn about(&self) -> &'static str {
        "a crop, rotate and size dialog for each figure, written as graphicx options"
    }
    fn types(&self) -> &'static [Ty] {
        &[Ty::File(FileKind::Graphic)]
    }
    fn features(&self) -> &'static [&'static str] {
        &["lens", "actions", "dialog"]
    }
    fn enabled(&self, cfg: &TypePlugins) -> bool {
        cfg.image.enabled
    }
}
