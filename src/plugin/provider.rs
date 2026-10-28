use serde::{Deserialize, Serialize};

named_enum! {
    /// A TeX distribution provider: the package manager and locator program.
    /// [`Provider::locator`] is what [`crate::distribution::Request`] asks for
    /// the installation's roots.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize)]
    #[serde(rename_all = "lowercase")]
    pub enum Provider {
        TexLive => "texlive",
        MikTeX => "miktex",
        Tectonic => "tectonic",
    }
}

impl Provider {
    /// The program that locates files in the distribution.
    pub fn locator(self) -> &'static str {
        match self {
            Provider::TexLive => "kpsewhich",
            Provider::MikTeX => "miktex-kpsewhich",
            Provider::Tectonic => "tectonic",
        }
    }

    /// The distribution's kpathsea program, for the lookups satex makes
    /// outside [`crate::distribution`].  Tectonic has none: it resolves from
    /// its own bundle, so nothing can be asked of it.
    pub fn kpsewhich(self) -> Option<&'static str> {
        match self {
            Provider::TexLive | Provider::MikTeX => Some(self.locator()),
            Provider::Tectonic => None,
        }
    }

    /// The kpathsea program of `provider`, falling back to whichever
    /// distribution is installed when the run configured none.
    pub fn kpsewhich_of(provider: Option<Provider>) -> Option<&'static str> {
        provider.or_else(Provider::detect)?.kpsewhich()
    }

    /// The distribution's package manager, when it has one (TeX Live
    /// manual, `tlmgr`; MiKTeX manual, `miktex packages`).  Tectonic has
    /// none: it carries one bundle and takes what a document needs out of it.
    pub fn manager(self) -> Option<&'static str> {
        match self {
            Provider::TexLive => Some("tlmgr"),
            Provider::MikTeX => Some("miktex"),
            Provider::Tectonic => None,
        }
    }

    /// Detect the installed provider by checking which locator is available.
    pub fn detect() -> Option<Provider> {
        [Provider::TexLive, Provider::MikTeX, Provider::Tectonic]
            .into_iter()
            .find(|provider| crate::paths::which(provider.locator()).is_ok())
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
