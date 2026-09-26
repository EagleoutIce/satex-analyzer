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

    /// The tool a package pulls into the build: biblatex runs biber unless
    /// its `backend=` option says otherwise, `makeidx` runs makeindex, and
    /// `glossaries` runs makeglossaries (each package's manual).
    pub fn implied_by(package: &str, options: &[String]) -> Option<Tool> {
        let backend = options
            .iter()
            .find_map(|o| o.split_once('=').filter(|(k, _)| k.trim() == "backend"))
            .map(|(_, v)| v.trim().to_string());
        match package {
            "biblatex" => Some(match backend.as_deref() {
                Some("bibtex") | Some("bibtex8") | Some("bibtexu") => Tool::Bibtex,
                _ => Tool::Biber,
            }),
            "natbib" | "cite" | "bibunits" => Some(Tool::Bibtex),
            "makeidx" | "index" | "imakeidx" => Some(Tool::Makeindex),
            "glossaries" | "glossaries-extra" => Some(Tool::Makeglossaries),
            _ => None,
        }
    }
}
