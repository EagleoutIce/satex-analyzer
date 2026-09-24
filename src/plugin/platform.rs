use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The platform the document is being built on, which decides where
/// [`Platform::roots`] and [`Platform::home_tree`] expect a TeX installation.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Linux,
    Macos,
    Windows,
}

impl Platform {
    /// Every platform satex knows, for `--version`.
    pub const ALL: &'static [Platform] = &[Platform::Linux, Platform::Macos, Platform::Windows];
}

impl Default for Platform {
    fn default() -> Self {
        Platform::current()
    }
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Linux => "linux",
            Platform::Macos => "macos",
            Platform::Windows => "windows",
        }
    }

    /// Detect the current platform.
    pub fn current() -> Platform {
        if cfg!(target_os = "windows") {
            Platform::Windows
        } else if cfg!(target_os = "macos") {
            Platform::Macos
        } else {
            Platform::Linux
        }
    }

    /// Standard TEXMF installation roots for this platform.
    /// The `*` in paths represents a version directory (TeX Live Guide, MiKTeX manual).
    pub fn roots(self) -> &'static [&'static str] {
        match self {
            Platform::Linux => &[
                "/usr/local/texlive/*/texmf-dist",
                "/usr/local/texlive/texmf-local",
                "/usr/share/texlive/texmf-dist",
                "/usr/share/texmf-dist",
                "/usr/share/texmf",
                "/opt/texlive/*/texmf-dist",
                "/var/lib/texmf",
            ],
            Platform::Macos => &[
                "/usr/local/texlive/*/texmf-dist",
                "/usr/local/texlive/texmf-local",
                "/Library/TeX/Distributions/.DefaultTeX/Contents/Resources/texmf-dist",
                "/opt/homebrew/texlive/*/texmf-dist",
            ],
            Platform::Windows => &[
                "C:/texlive/*/texmf-dist",
                "C:/Program Files/MiKTeX/texmfs/install",
                "C:/Program Files/MiKTeX 2.9/texmfs/install",
            ],
        }
    }

    /// The user's own TEXMF tree, which takes precedence over the installation.
    pub fn home_tree(self) -> Option<PathBuf> {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
        let home = PathBuf::from(home);
        Some(match self {
            Platform::Windows => home.join("AppData").join("Roaming").join("MiKTeX"),
            _ => home.join("texmf"),
        })
    }
}
