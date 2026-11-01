/// An auxiliary tool recognized by satex: biber, bibtex, makeindex and the
/// rest that a build runs beside the engine. Collected into
/// [`crate::plugin::Plugins::tools`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Biber,
    Bibtex,
    Makeindex,
    Xindy,
    Makeglossaries,
    Dvips,
    Ps2pdf,
}

impl Tool {
    pub fn as_str(&self) -> &'static str {
        match self {
            Tool::Biber => "biber",
            Tool::Bibtex => "bibtex",
            Tool::Makeindex => "makeindex",
            Tool::Xindy => "xindy",
            Tool::Makeglossaries => "makeglossaries",
            Tool::Dvips => "dvips",
            Tool::Ps2pdf => "ps2pdf",
        }
    }

    /// File extensions this tool consumes.
    pub fn consumes(&self) -> &'static [&'static str] {
        match self {
            Tool::Biber => &["bcf"],
            Tool::Bibtex => &["aux"],
            Tool::Makeindex => &["idx"],
            Tool::Xindy => &["idx"],
            Tool::Makeglossaries => &["glo"],
            Tool::Dvips => &["dvi"],
            Tool::Ps2pdf => &["ps"],
        }
    }

    /// File extensions this tool produces.
    pub fn produces(&self) -> &'static [&'static str] {
        match self {
            Tool::Biber => &["bbl"],
            Tool::Bibtex => &["bbl"],
            Tool::Makeindex => &["ind"],
            Tool::Xindy => &["ind"],
            Tool::Makeglossaries => &["gls"],
            Tool::Dvips => &["ps"],
            Tool::Ps2pdf => &["pdf"],
        }
    }

    /// All recognized auxiliary tools.
    pub fn all() -> &'static [Tool] {
        &[Tool::Biber, Tool::Bibtex, Tool::Makeindex, Tool::Xindy, Tool::Makeglossaries, Tool::Dvips, Tool::Ps2pdf]
    }

    /// The tool a bibliography program's name stands for.
    pub fn bib(name: &str) -> Option<Tool> {
        match name.trim() {
            "biber" => Some(Tool::Biber),
            "bibtex" | "bibtex8" | "bibtexu" => Some(Tool::Bibtex),
            _ => None,
        }
    }

    /// The tool a package pulls into the build: biblatex runs `backend`
    /// unless its `backend=` option says otherwise, `makeidx` runs makeindex,
    /// and `glossaries` runs makeglossaries (each package's manual).
    pub fn implied_by(package: &str, options: &[String], backend: Tool) -> Option<Tool> {
        match package {
            "biblatex" => Some(
                options
                    .iter()
                    .find_map(|o| o.split_once('=').filter(|(k, _)| k.trim() == "backend"))
                    .and_then(|(_, v)| Tool::bib(v))
                    .unwrap_or(backend),
            ),
            "natbib" | "cite" | "bibunits" => Some(Tool::Bibtex),
            "makeidx" | "index" | "imakeidx" => Some(Tool::Makeindex),
            "glossaries" | "glossaries-extra" => Some(Tool::Makeglossaries),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Tool;

    #[test]
    fn biblatex_runs_its_backend_option_else_the_configured_one() {
        assert_eq!(Tool::implied_by("biblatex", &[], Tool::Bibtex), Some(Tool::Bibtex));
        assert_eq!(Tool::implied_by("biblatex", &["backend=biber".into()], Tool::Bibtex), Some(Tool::Biber));
        assert_eq!(Tool::implied_by("natbib", &[], Tool::Biber), Some(Tool::Bibtex));
    }
}
