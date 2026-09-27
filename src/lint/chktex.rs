//! chktex's warnings, so the documentation can state which satex rule
//! covers each one instead of a table kept by hand.  Numbers, wording and
//! default state are chktex's own (ChkTeX 1.7.9 manual, § 8).

use serde_json::json;

use crate::lint::{Rule, Source};

/// One of chktex's numbered warnings.
pub struct Warning {
    pub number: u8,
    /// chktex's own message, shortened to what it is about.
    pub title: &'static str,
    /// Whether chktex reports it without `-w`.
    pub on: bool,
}

/// ChkTeX 1.7.9, § 8 "Explanation of error messages".
pub static WARNINGS: &[Warning] = &[
    Warning { number: 1, title: "Command terminated with space.", on: true },
    Warning { number: 2, title: "Non-breaking space (`~`) should have been used.", on: true },
    Warning { number: 3, title: "You should enclose the previous parenthesis with `{}`.", on: true },
    Warning { number: 4, title: "Italic correction (`\\/`) found in non-italic buffer.", on: true },
    Warning { number: 5, title: "Italic correction (`\\/`) found more than once.", on: true },
    Warning { number: 6, title: "No italic correction (`\\/`) found.", on: true },
    Warning { number: 7, title: "Accent command needs use of the dotless letter.", on: true },
    Warning { number: 8, title: "Wrong length of dash may have been used.", on: true },
    Warning { number: 9, title: "`'%s'` expected, found `'%s'`.", on: true },
    Warning { number: 10, title: "Solo `'%s'` found.", on: true },
    Warning { number: 11, title: "You should use `\\ldots` to achieve an ellipsis.", on: true },
    Warning { number: 12, title: "Interword spacing (`\\ `) should perhaps be used.", on: true },
    Warning { number: 13, title: "Intersentence spacing (`\\@`) should perhaps be used.", on: true },
    Warning { number: 14, title: "Could not find argument for command.", on: false },
    Warning { number: 15, title: "No match found for `'%s'`.", on: true },
    Warning { number: 16, title: "Mathmode still on at end of LaTeX file.", on: true },
    Warning { number: 17, title: "Number of one character doesn't match the number of another.", on: true },
    Warning { number: 18, title: "You should use either ``` `` ``` or `''` as an alternative to `\"`.", on: true },
    Warning { number: 19, title: "You should use `'` (ASCII 39) instead of `´` (ASCII 180).", on: false },
    Warning { number: 20, title: "User-specified pattern found.", on: true },
    Warning { number: 21, title: "This command might not be intended.", on: false },
    Warning { number: 22, title: "Comment displayed.", on: false },
    Warning { number: 23, title: "Either ``` `` ``` or `''` will look better.", on: true },
    Warning { number: 24, title: "Delete this space to maintain correct page references.", on: true },
    Warning { number: 25, title: "You might wish to put this between a pair of `{}`.", on: true },
    Warning { number: 26, title: "You ought to remove spaces in front of punctuation.", on: true },
    Warning { number: 27, title: "Could not execute LaTeX command.", on: true },
    Warning { number: 28, title: "Don't use `\\/` in front of small punctuation.", on: true },
    Warning { number: 29, title: "`$\\times$` may look prettier here.", on: true },
    Warning { number: 30, title: "Multiple spaces detected in output.", on: false },
    Warning { number: 31, title: "This text may be ignored.", on: true },
    Warning { number: 32, title: "Use ``` ` ``` to begin quotation, not `'`.", on: true },
    Warning { number: 33, title: "Use `'` to end quotation, not ``` ` ```.", on: true },
    Warning { number: 34, title: "Don't mix quotes.", on: true },
    Warning { number: 35, title: "You should perhaps use a command instead.", on: true },
    Warning { number: 36, title: "You should put a space in front of/after parenthesis.", on: true },
    Warning { number: 37, title: "You should avoid spaces in front of/after parenthesis.", on: true },
    Warning { number: 38, title: "You should not use punctuation in front of/after quotes.", on: true },
    Warning { number: 39, title: "Double space found.", on: true },
    Warning { number: 40, title: "You should put punctuation outside inner/inside display math mode.", on: true },
    Warning { number: 41, title: "You ought to not use primitive TeX in LaTeX code.", on: false },
    Warning { number: 42, title: "You should remove spaces in front of a command.", on: true },
    Warning { number: 43, title: "A command is normally not followed by this character.", on: true },
    Warning { number: 44, title: "User regex matched.", on: true },
    Warning { number: 45, title: "Use `\\[ … \\]` instead of `$$ … $$`.", on: true },
    Warning { number: 46, title: "Use `\\( … \\)` instead of `$ … $`.", on: false },
    Warning { number: 47, title: "`'%s'` expected, found `'%s'` (ConTeXt).", on: false },
    Warning { number: 48, title: "Solo `'%s'` found (ConTeXt).", on: true },
    Warning { number: 49, title: "Expected math mode to be in another state here.", on: true },
];

/// The satex rules that cite chktex warning `number`.
pub fn covered_by(number: u8) -> Vec<&'static Rule> {
    let wanted = number.to_string();
    crate::lint::all()
        .filter(|rule| rule.references.iter().any(|r| r.source == Source::Chktex && r.at == wanted))
        .collect()
}

/// One row per chktex warning: what it is, whether chktex reports it by
/// default, and the satex rules that cite it.
pub fn coverage() -> Vec<crate::query::Record> {
    WARNINGS
        .iter()
        .map(|warning| {
            let rules = covered_by(warning.number);
            let mut record = serde_json::Map::new();
            record.insert("warning".into(), json!(warning.number));
            record.insert("default".into(), json!(if warning.on { "on" } else { "off" }));
            record.insert("message".into(), json!(warning.title));
            record.insert("rules".into(), json!(rules.iter().map(|r| r.code).collect::<Vec<_>>().join(", ")));
            record.insert("covered".into(), json!(!rules.is_empty()));
            record
        })
        .collect()
}
