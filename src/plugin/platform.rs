use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The platform the document is being built on, which decides where
/// [`Platform::roots`] and [`Platform::home_trees`] expect a TeX installation.
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

    /// Pick the home directory out of the two variables that can name one.
    /// Git Bash sets `HOME` to an MSYS path no Win32 API resolves, so on
    /// Windows `USERPROFILE` is the one to believe.
    pub(crate) fn home_from(self, home: Option<PathBuf>, profile: Option<PathBuf>) -> Option<PathBuf> {
        match self {
            Platform::Windows => profile.or(home),
            _ => home.or(profile),
        }
    }

    /// The user's home directory as this platform names it.
    pub fn home_dir(self) -> Option<PathBuf> {
        let var = |name| std::env::var_os(name).map(PathBuf::from);
        self.home_from(var("HOME"), var("USERPROFILE"))
    }

    /// The user's own TEXMF trees under `home`, which take precedence over
    /// the installation.  A platform can have more than one, since the
    /// personal tree is the distribution's choice, not the system's: TeX
    /// Live uses `~/texmf` everywhere including Windows (TeX Live Guide,
    /// "Personal trees"), MiKTeX `%USERPROFILE%\AppData\Roaming\MiKTeX`
    /// (MiKTeX manual, "TEXMF root directories"), MacTeX `~/Library/texmf`.
    /// Which are installed is decided by probing them, not by guessing here.
    pub fn home_trees_in(self, home: &Path) -> Vec<PathBuf> {
        let texmf = home.join("texmf");
        match self {
            Platform::Windows => vec![texmf, home.join("AppData").join("Roaming").join("MiKTeX")],
            Platform::Macos => vec![texmf, home.join("Library").join("texmf")],
            Platform::Linux => vec![texmf],
        }
    }

    /// [`Platform::home_trees_in`] under the home directory this process has.
    pub fn home_trees(self) -> Vec<PathBuf> {
        self.home_dir().map(|home| self.home_trees_in(&home)).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: Option<&str> = Some("/c/Users/ada");
    const PROFILE: Option<&str> = Some(r"C:\Users\ada");

    fn home_of(platform: Platform, home: Option<&str>, profile: Option<&str>) -> Option<PathBuf> {
        platform.home_from(home.map(PathBuf::from), profile.map(PathBuf::from))
    }

    #[test]
    fn windows_believes_userprofile_over_the_msys_home() {
        assert_eq!(home_of(Platform::Windows, HOME, PROFILE), Some(PathBuf::from(r"C:\Users\ada")));
        assert_eq!(home_of(Platform::Linux, HOME, PROFILE), Some(PathBuf::from("/c/Users/ada")));
        // Either variable alone still names a home on either platform.
        assert_eq!(home_of(Platform::Windows, HOME, None), Some(PathBuf::from("/c/Users/ada")));
        assert_eq!(home_of(Platform::Linux, None, PROFILE), Some(PathBuf::from(r"C:\Users\ada")));
        assert_eq!(home_of(Platform::Windows, None, None), None);
    }

    #[test]
    fn every_platform_probes_the_texlive_personal_tree() {
        let home = Path::new("/home/ada");
        for platform in Platform::ALL {
            let trees = platform.home_trees_in(home);
            assert!(trees.contains(&home.join("texmf")), "{platform:?}: {trees:?}");
        }
    }

    #[test]
    fn windows_probes_the_miktex_tree_as_well() {
        let trees = Platform::Windows.home_trees_in(Path::new("/home/ada"));
        assert!(trees.contains(&Path::new("/home/ada").join("AppData").join("Roaming").join("MiKTeX")));
        assert!(!Platform::Linux.home_trees_in(Path::new("/home/ada")).iter().any(|t| t.ends_with("MiKTeX")));
    }
}
