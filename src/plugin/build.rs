/// Mapping of build configuration filenames to the build system that reads them.
pub const BUILD_FILES: [(&str, &str); 7] = [
    ("latexmkrc", "latexmk"),
    (".latexmkrc", "latexmk"),
    ("GNUmakefile", "make"),
    ("makefile", "make"),
    ("Makefile", "make"),
    ("Tectonic.toml", "tectonic"),
    ("tectonic.toml", "tectonic"),
];

/// Build systems recognized by satex: what a `latexmkrc` or `Makefile`
/// beside the document turns out to be. Named in
/// [`crate::plugin::Plugins::build`].
pub enum BuildSystem {
    Latexmk,
    Make,
    Tectonic,
    Arara,
}

impl BuildSystem {
    pub fn as_str(&self) -> &'static str {
        match self {
            BuildSystem::Latexmk => "latexmk",
            BuildSystem::Make => "make",
            BuildSystem::Tectonic => "tectonic",
            BuildSystem::Arara => "arara",
        }
    }
}
