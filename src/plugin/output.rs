use serde::{Deserialize, Serialize};


/// The document output format: PDF, DVI, PostScript or HTML.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Output {
    Pdf,
    Dvi,
    PostScript,
    Html,
}

impl Output {
    /// Every output satex knows, for `--version`.
    pub const ALL: &'static [Output] = &[Output::Pdf, Output::Dvi, Output::PostScript, Output::Html];
    pub fn as_str(self) -> &'static str {
        match self {
            Output::Pdf => "pdf",
            Output::Dvi => "dvi",
            Output::PostScript => "ps",
            Output::Html => "html",
        }
    }

    /// The `\pdfoutput` register value for this output format.
    /// Positive means PDF, zero means DVI (pdfTeX manual, "\pdfoutput").
    /// XeTeX has no such parameter and always writes XDV, which xdvipdfmx converts to PDF.
    pub fn pdfoutput(self) -> i64 {
        match self {
            Output::Pdf => 1,
            _ => 0,
        }
    }

    /// Parse latexmk's `$pdf_mode` setting (latexmk manual, "$pdf_mode").
    /// 0 produces DVI, 1 pdfLaTeX, 2 PostScript via ps2pdf, 3 DVI via dvipdf, 4 LuaLaTeX, 5 XeLaTeX.
    pub fn from_pdf_mode(mode: &str) -> Option<Output> {
        Some(match mode {
            "0" => Output::Dvi,
            "2" => Output::PostScript,
            "1" | "3" | "4" | "5" => Output::Pdf,
            _ => return None,
        })
    }
}
