use serde::{Deserialize, Serialize};

/// A TeX distribution provider: the package manager and locator program.
/// [`Provider::locator`] is what [`crate::distribution::Request`] asks for
/// the installation's roots.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    TexLive,
    MikTeX,
    Tectonic,
}

impl Provider {
    /// Every provider satex knows, for `--version`.
    pub const ALL: &'static [Provider] = &[Provider::TexLive, Provider::MikTeX, Provider::Tectonic];
    pub fn as_str(self) -> &'static str {
        match self {
            Provider::TexLive => "texlive",
            Provider::MikTeX => "miktex",
            Provider::Tectonic => "tectonic",
        }
    }

    /// The program that locates files in the distribution.
    pub fn locator(self) -> &'static str {
        match self {
            Provider::TexLive => "kpsewhich",
            Provider::MikTeX => "miktex-kpsewhich",
            Provider::Tectonic => "tectonic",
        }
    }

    /// Detect the installed provider by checking which locator is available.
    pub fn detect() -> Option<Provider> {
        [Provider::TexLive, Provider::MikTeX, Provider::Tectonic]
            .into_iter()
            .find(|provider| which::which(provider.locator()).is_ok())
    }

    /// Query the provider's version string.
    /// The installed version, asked once per installation: the answer is
    /// cached against the program's own stamp, since starting the engine
    /// costs more than everything else a warm run does.
    pub fn version(self, cache: Option<&std::path::Path>) -> Option<String> {
        let (program, argument) = match self {
            Provider::TexLive => ("tex", "--version"),
            Provider::MikTeX => ("miktex", "--version"),
            Provider::Tectonic => ("tectonic", "--version"),
        };
        let answer = crate::distribution::probed(program, "version", cache, || {
            let Ok(output) = std::process::Command::new(program).arg(argument).output() else {
                return Vec::new();
            };
            let Ok(text) = String::from_utf8(output.stdout) else { return Vec::new() };
            text.lines().next().map(|line| line.trim().to_string()).into_iter().collect()
        });
        answer.into_iter().next()
    }
}
