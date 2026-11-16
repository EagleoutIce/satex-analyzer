//! Package augmentations: what satex knows about one package, kept in that
//! package's module instead of in the lint rules.  A rule drafts a finding in
//! neutral terms, and each module `plugins.augment` names may veto it or
//! rewrite its message and fix, reading the package's state from the run.

mod siunitx;

use crate::analysis::machine::Analysis;

/// What a finding is about, in the rule's terms, for a module to judge.
pub enum Subject<'a> {
    /// A column whose numbers differ in decimal places, its type as written
    /// (`S[round-mode = places]`).
    Precision { kind: &'a str, column: usize, low: usize, high: usize },
    /// A numeric column not aligned on its decimal separator.
    Alignment { kind: &'a str, column: usize, header: &'a str, decimals: bool },
    /// A hyphen between two quantities.
    QuantityRange,
}

/// A finding a rule is about to report.  `keep` false drafts one the rule
/// cannot judge, such as a column type it does not know, for a module to keep.
pub struct Draft<'a> {
    pub subject: Subject<'a>,
    pub message: String,
    pub fix: Option<String>,
    pub keep: bool,
}

/// One package's module: the package as `\usepackage` names it, and its hook.
pub struct Module {
    pub package: &'static str,
    pub review: fn(&Analysis, &mut Draft),
}

/// Every module.  All are on unless `plugins.augment` leaves them out.
pub const MODULES: &[Module] = &[siunitx::MODULE];

/// Runs the switched-on modules on `draft`: whether it is still kept.
pub fn review(analysis: &Analysis, draft: &mut Draft) -> bool {
    let on = &analysis.settings.plugins.augment;
    for module in MODULES.iter().filter(|m| on.iter().any(|p| p == m.package)) {
        (module.review)(analysis, draft);
    }
    draft.keep
}
