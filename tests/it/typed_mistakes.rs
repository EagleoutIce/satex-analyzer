//! Everyday mistakes typed into small documents, against what pdflatex
//! (TeX Live 2026, `-interaction=nonstopmode -file-line-error`) says: an error
//! certain to satex is one pdflatex raises, on the line pdflatex names, and a
//! mutant pdflatex takes without error gets no certain error. The rows come
//! from the cex3 mutation sweep (a brace, `\\`, `&`, `$`, a line or a
//! backslash deleted, a command cut short, an `\end` swapped).

use crate::support::installed;
use satex::analysis::machine::Machine;
use satex::project::config::{Config, Load};

const TAB: &str = include_str!("typed_mistakes/tab.tex");
const LIST: &str = include_str!("typed_mistakes/list.tex");
const MATH: &str = include_str!("typed_mistakes/math.tex");
const CMD: &str = include_str!("typed_mistakes/cmd.tex");

/// The lines of the certain errors the document's own text gets.
fn error_lines(source: &str) -> Vec<u32> {
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(source, None, &cfg);
    let mut lines: Vec<u32> = satex::linter::lint(&analysis)
        .iter()
        .filter(|r| r["severity"] == "error" && r["origin"] == "document")
        .filter_map(|r| r["line"].as_u64().and_then(|l| u32::try_from(l).ok()))
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// (name, document, byte offset, text there, its replacement, pdflatex's first error line or 0 for none)
type Row = (&'static str, &'static str, usize, &'static str, &'static str, u32);

const ROWS: &[Row] = &[
    ("cmd_del_brace_130", CMD, 130, r"{", r"", 4),
    ("cmd_del_bs_269", CMD, 269, r"\", r"", 0),
    ("cmd_del_bs_320", CMD, 320, r"\", r"", 13),
    ("cmd_half_cmd_298", CMD, 298, r"\begin", r"\beg", 9),
    ("cmd_del_brace_198", CMD, 198, r"{", r"", 5),
    ("cmd_half_cmd_108", CMD, 108, r"\newcommand", r"\new", 4),
    ("list_del_bs_229", LIST, 229, r"\", r"", 14),
    ("list_del_brace_145", LIST, 145, r"{", r"", 8),
    ("list_del_brace_52", LIST, 52, r"{", r"", 3),
    ("list_del_bs_107", LIST, 107, r"\", r"", 0),
    ("list_half_cmd_155", LIST, 155, r"\begin", r"\beg", 9),
    ("list_half_cmd_24", LIST, 24, r"\usepackage", r"\use", 2),
    ("math_add_amp_257", MATH, 257, r"\\", r"& \\", 14),
    ("math_add_amp_142", MATH, 142, r"\\", r"& \\", 0),
    ("math_del_bs_300", MATH, 300, r"\", r"", 0),
    ("math_del_dollar_85", MATH, 85, r"$", r"", 4),
    ("math_del_amp_119", MATH, 119, r"&", r"", 0),
    ("math_del_brace_183", MATH, 183, r"{", r"", 9),
    ("math_del_brace_51", MATH, 51, r"{", r"", 3),
    ("math_del_dbs_199", MATH, 199, r"\\", r"", 0),
    ("tab_del_brace_222", TAB, 222, r"{", r"", 12),
    ("tab_del_amp_200", TAB, 200, r"&", r"", 0),
    ("tab_del_bs_251", TAB, 251, r"\", r"", 20),
    ("tab_del_dbs_236", TAB, 236, r"\\", r"", 13),
    ("tab_del_brace_76", TAB, 76, r"{", r"", 4),
    ("tab_add_amp_236", TAB, 236, r"\\", r"& \\", 12),
    ("tab_del_amp_163", TAB, 163, r"&", r"", 0),
    ("tab_del_bs_266", TAB, 266, r"\", r"", 16),
];

/// Each mutant's first certain error is pdflatex's first error, or there is none when pdflatex has none.
#[test]
fn a_typed_mistake_errs_where_pdflatex_does() {
    if !installed() {
        return;
    }
    crate::support::table(
        ROWS,
        |r| r.0,
        |&(_, doc, at, old, new, want)| {
            assert_eq!(&doc[at..at + old.len()], old, "the row's offset");
            let source = format!("{}{new}{}", &doc[..at], &doc[at + old.len()..]);
            let got = error_lines(&source);
            match (got.first().copied().unwrap_or(0), want) {
                (g, w) if g == w => None,
                (g, w) => Some(format!("first certain error on line {g}, pdflatex's on {w}: {got:?}")),
            }
        },
    );
}

/// Text before a list's first `\item` starts a paragraph the list's `\par`
/// refuses to end (latex.ltx `\@trivlist`); `\item`'s `\addvspace` puts
/// `\par` before its glue again and again (tex.web § 1094) until LaTeX's
/// `\par@deathcycles` passes `\@m` and says "Something's wrong--perhaps a
/// missing \item". Pinned: satex assumed the paragraph ended at the second
/// inserted `\par` and found no error.
#[test]
fn text_before_the_first_item_is_an_error_at_the_item() {
    if !installed() {
        return;
    }
    let doc = "\\documentclass{article}\n\\begin{document}\n\\begin{itemize}\nitem First.\n\\item Second.\n\\end{itemize}\n\\end{document}\n";
    assert_eq!(error_lines(doc), [5]);
}

/// `\sec` is `\mathop{…}` in latex.ltx: in text, TeX inserts `$` before the
/// `\mathop` (tex.web § 1046) and says "Missing $ inserted", reported once,
/// at the call. Pinned: math material in library code was never checked
/// against the mode, so the error was missed.
#[test]
fn a_math_operator_in_text_is_missing_its_dollar() {
    if !installed() {
        return;
    }
    let doc = "\\documentclass{article}\n\\begin{document}\n\\sec{Start}\n\\end{document}\n";
    assert_eq!(error_lines(doc), [3, 4]);
}

/// A footnote's text goes to an insert, away from the paragraph: a period
/// after `\footnote{Here.}` follows the mark, not the footnote's period.
/// Pinned: `doubled-punctuation` read `..` across the insert's end.
#[test]
fn a_period_after_a_footnote_ending_in_one_is_not_doubled() {
    if !installed() {
        return;
    }
    crate::support::assert_lint(
        "\\documentclass{article}\n\\begin{document}\nText with a footnote\\footnote{Here.}.\n\\end{document}\n",
        "doubled-punctuation",
        0,
    );
}
