/// Build systems recognized by satex: what a `latexmkrc` or `Makefile`
/// beside the document turns out to be. Named in
/// [`crate::plugins::Plugins::build`].
pub enum BuildSystem {
    Latexmk,
    Make,
    Tectonic,
    Arara,
}

impl BuildSystem {
    /// Every build system satex knows, for `--version` and docs generation.
    pub const ALL: &'static [BuildSystem] =
        &[BuildSystem::Latexmk, BuildSystem::Make, BuildSystem::Tectonic, BuildSystem::Arara];

    pub fn as_str(&self) -> &'static str {
        match self {
            BuildSystem::Latexmk => "latexmk",
            BuildSystem::Make => "make",
            BuildSystem::Tectonic => "tectonic",
            BuildSystem::Arara => "arara",
        }
    }
}
