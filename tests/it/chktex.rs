//! satex's coverage of chktex's warnings; see doc/chktex-coverage.md for the
//! full mapping.  Both rules exercised here work at the macro-expansion
//! level, so no TeX installation is needed to run them.

use satex::config::{Config, Load, Texmf};
use satex::machine::{Analysis, Machine};

fn analyze(source: &str) -> Analysis {
    Machine::analyze(source, None, &crate::support::bare())
}

fn findings(analysis: &Analysis, code: &str) -> Vec<String> {
    satex::lint::lint(analysis)
        .into_iter()
        .filter(|record| record["code"] == code)
        .filter_map(|record| record["message"].as_str().map(str::to_string))
        .collect()
}

/// Tests that the lint `code` has no finding on a document run with the kernel.
macro_rules! file_none {
    ($($(#[$doc:meta])* $name:ident($file:literal, $source:literal, $code:literal);)*) => {$(
        $(#[$doc])*
        #[test]
        fn $name() {
            let Some(analysis) = latex_file($file, $source) else { return };
            assert!(findings(&analysis, $code).is_empty());
        }
    )*};
}

/// Tests that the lint `code` has no finding on a document run without one.
macro_rules! bare_none {
    ($($(#[$doc:meta])* $name:ident($dir:literal, $source:literal, $code:literal);)*) => {$(
        $(#[$doc])*
        #[test]
        fn $name() {
            let doc = on_disk($dir, $source, false).expect("temporary directory");
            assert!(findings(&doc.analysis, $code).is_empty());
        }
    )*};
}

/// Tests that the lint `code` has no finding on a math document.
macro_rules! math_none {
    ($($(#[$doc:meta])* $name:ident($dir:literal, $body:expr, $code:literal);)*) => {$(
        $(#[$doc])*
        #[test]
        fn $name() {
            let Some(doc) = on_disk($dir, &math_document($body), true) else { return };
            assert!(findings(&doc.analysis, $code).is_empty());
        }
    )*};
}

/// Name, document, lint code, and the texts its one finding holds (`None`:
/// no finding).
type FindingRow = (&'static str, &'static str, &'static str, Option<&'static [&'static str]>);

/// Checks each row on the analysis `read` gives; a row `read` gives `None`
/// for (no installation) is skipped.
fn finding_rows(rows: &[FindingRow], read: impl Fn(&str) -> Option<Analysis>) {
    crate::support::table(
        rows,
        |r| r.0,
        |&(_, source, code, texts)| {
            let found = findings(&read(source)?, code);
            match texts {
                None => (!found.is_empty()).then(|| format!("{code} reported {found:?}")),
                Some(texts) => (found.len() != 1 || !texts.iter().all(|t| found[0].contains(t)))
                    .then(|| format!("{code}: expected one finding holding {texts:?}, got {found:?}")),
            }
        },
    );
}

// chktex warning 9: `'%s' expected, found '%s'`.
/// `\end{…}` is latex.ltx's, so these run the kernel: its `\@checkend`
/// raises `\begin{x} on input line n ended by \end{y}`.
fn latex(source: &str) -> Option<Analysis> {
    which::which("kpsewhich").ok()?;
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    Some(Machine::analyze(source, None, &cfg))
}

#[test]
fn end_naming_the_wrong_environment_is_reported() {
    let Some(analysis) =
        latex(r"\documentclass{article}\begin{document}\begin{itemize}\item one\end{enumerate}\end{document}")
    else {
        return;
    };
    let found = findings(&analysis, "environment-mismatch");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("itemize"), "{found:?}");
    assert!(found[0].contains("enumerate"), "{found:?}");
}

#[test]
fn a_solo_end_is_reported() {
    let Some(analysis) = latex(r"\documentclass{article}\begin{document}\end{itemize}\end{document}") else {
        return;
    };
    let found = findings(&analysis, "environment-mismatch");
    assert!(
        found.iter().any(|f| f.contains(r"\begin{document}") && f.contains(r"ended by \end{itemize}")),
        "{found:?}"
    );
}

#[test]
fn an_environment_never_closed_is_reported() {
    let Some(analysis) = latex(r"\documentclass{article}\begin{document}\begin{itemize}\item one\end{document}") else {
        return;
    };
    let found = findings(&analysis, "environment-mismatch");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains(r"\begin{itemize}") && found[0].contains(r"ended by \end{document}"), "{found:?}");
}

/// An error inside the environment may have ended it: the mismatch after
/// it is a note that names the error, never a certain warning.
#[test]
fn a_mismatch_after_an_error_names_the_error() {
    let Some(analysis) = latex(
        "\\documentclass{article}\n\\begin{document}\n\\begin{center}\n\\count0=x\n\\end{itemize}\n\\end{document}\n",
    ) else {
        return;
    };
    let rows: Vec<_> =
        satex::lint::lint(&analysis).into_iter().filter(|r| r["code"] == "environment-mismatch").collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["severity"], "info", "{rows:?}");
    assert!(rows[0]["message"].as_str().is_some_and(|m| m.contains("may follow from the error at")), "{rows:?}");
}

/// Documents read without an installation, each with a lint code and what it reports: `None` for
/// nothing, else one finding whose message holds every listed text.
const BARE: &[FindingRow] = &[
    (
        "properly_nested_environments_are_not_reported",
        r"\begin{document}\begin{itemize}\item one\end{itemize}\end{document}",
        "environment-mismatch",
        None,
    ),
    // `\catcode` has no LaTeX interface at all (it is not in the built-in
    // table), so a finding for it could only say so and give nothing to act
    // on: satex reports none.  `\expandafter` is the same story.
    (
        "plain_tex_primitives_with_no_latex_alternative_are_not_reported",
        r"\catcode`\@=11 \expandafter\relax\expandafter\relax\relax",
        "primitive-tex-command",
        None,
    ),
    (
        "a_latex_style_document_is_not_reported",
        r"\newcommand{\greeting}{hello}\greeting",
        "primitive-tex-command",
        None,
    ),
    (
        "a_second_italic_correction_with_nothing_between_is_reported",
        r"word\/\/ more",
        "stray-italic-correction",
        Some(&["nothing was set"]),
    ),
    (
        "an_italic_correction_followed_by_a_letter_is_not_reported",
        r"word\/ more\/ still",
        "stray-italic-correction",
        None,
    ),
    (
        "an_acute_accent_for_an_apostrophe_is_reported",
        "It is Andre\u{b4}s book.",
        "wrong-quote-character",
        Some(&["acute accent"]),
    ),
    ("proper_quotation_marks_are_not_reported", "He said ``hello'' to me.", "wrong-quote-character", None),
    // chktex warnings 32, 33 and 34: a mark that points the wrong way.
    (
        "a_quotation_opened_with_an_apostrophe_is_reported",
        "He said 'hello' to me.",
        "quote-direction",
        Some(&["opens a quotation"]),
    ),
    (
        "a_quotation_closed_with_a_backtick_is_reported",
        "He said ``hello` to me.",
        "quote-direction",
        Some(&["a double closing quote belongs"]),
    ),
    ("a_run_mixing_both_marks_is_reported", "A `' here.", "quote-direction", Some(&["empty quotation"])),
    (
        "apostrophes_and_elisions_are_not_reported",
        "Don't stop the boys' rock 'n' roll of the '90s.",
        "quote-direction",
        None,
    ),
    // chktex warning 23: three quotation marks in a row.
    (
        "three_quotation_marks_in_a_row_are_reported",
        "A ``` run of three.",
        "quote-ligature",
        Some(&["nothing between them"]),
    ),
    // chktex warning 38: punctuation inside the closing quotation mark.
    (
        "punctuation_inside_a_quotation_is_reported",
        "He said ``hello.'' and left.",
        "quote-punctuation",
        Some(&["stands inside the quotation"]),
    ),
    ("punctuation_after_a_quotation_is_not_reported", "He said ``hello''. Then he left.", "quote-punctuation", None),
    // csquotes covers all of the above at once.
    (
        "hand_set_quotation_marks_suggest_csquotes",
        "He said ``hello'' and ``goodbye'' to me.",
        "csquotes-available",
        Some(&["4 quotation marks"]),
    ),
    (
        "a_correctly_spaced_sentence_is_not_reported",
        r"\begin{document}One thing here. Another thing there. A. Smith wrote it.\end{document}",
        "sentence-spacing",
        None,
    ),
    (
        "punctuation_against_its_word_is_not_reported",
        r"\begin{document}one, two; three here.\end{document}",
        "space-before-punctuation",
        None,
    ),
    // chktex warning 30: `Multiple spaces detected in input.`  TeX reads a run
    // of spaces as one token, so only one of them reached the page.
    (
        "spaces_the_run_collapsed_are_not_reported",
        "\\begin{document}one  two   three here.\\end{document}",
        "double-space",
        None,
    ),
    (
        "a_parenthesis_spaced_correctly_is_not_reported",
        r"\begin{document}The value (two) is one.\end{document}",
        "parenthesis-spacing",
        None,
    ),
];

#[test]
fn bare_documents_report_what_chktex_would() {
    finding_rows(BARE, |source| Some(analyze(source)));
}

// chktex warning 41: `You ought to not use primitive TeX in LaTeX code.`
#[test]
fn plain_tex_primitives_with_a_latex_alternative_are_reported() {
    let analysis = analyze(r"\hskip 1pt \vskip 1pt");
    let found = findings(&analysis, "primitive-tex-command");
    assert!(found.iter().any(|m| m.contains("\\hskip")), "{found:?}");
    assert!(found.iter().any(|m| m.contains("\\vskip")), "{found:?}");
}

/// `\csname` genuinely has a LaTeX interface (`\@nameuse`, `\@namedef`, …),
/// but the specific role satex saw this one in — naming a control sequence
/// for `\let` — only has an alternative in etoolbox, which this document
/// never loads.  A finding that only repeats "this is plain TeX" back would
/// be exactly the noise `\expandafter` used to be: satex reports nothing
/// for it either, unless `latex_alternatives` names it explicitly.
#[test]
fn a_csname_role_with_no_loaded_alternative_is_not_reported() {
    let analysis = analyze(r"\expandafter\let\csname foo\endcsname\relax");
    assert!(
        findings(&analysis, "primitive-tex-command").is_empty(),
        "{:?}",
        findings(&analysis, "primitive-tex-command")
    );
}

/// A `latex_alternatives` entry naming a primitive that has none built in
/// is the user asking for it by name: satex still reports it, with no
/// suggestion attached, exactly as it always has for an explicit override.
#[test]
fn an_explicit_override_with_no_alternative_is_still_reported() {
    let cfg = Config {
        load: Load { packages: false, classes: false, inputs: false, format: false, ..Default::default() },
        texmf: Texmf { kpsewhich: false, ..Default::default() },
        latex_alternatives: std::collections::BTreeMap::from([("expandafter".to_string(), String::new())]),
        ..crate::support::config()
    };
    let analysis = Machine::analyze(r"\expandafter\relax\relax", None, &cfg);
    let found = findings(&analysis, "primitive-tex-command");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("\\expandafter"), "{found:?}");
}

// chktex warnings 4, 5 and 28: the italic correction `\/`.
#[test]
fn an_italic_correction_in_math_mode_is_reported() {
    let Some(analysis) = latex(r"\documentclass{article}\begin{document}text $x\/y$ more\end{document}") else {
        return;
    };
    let found = findings(&analysis, "zero-italic-correction");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("math mode"), "{found:?}");
}

#[test]
fn an_italic_correction_in_front_of_a_period_is_reported() {
    let analysis = analyze(r"word\/. More");
    let found = findings(&analysis, "stray-italic-correction");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains('.'), "{found:?}");
}

/// chktex reads the source and sees a `}`; satex reads what was set.
#[test]
fn an_italic_correction_a_closing_brace_hides_the_period_from_is_reported() {
    let Some(analysis) = latex(r"\documentclass{article}\begin{document}\textit{word\/}. More\end{document}") else {
        return;
    };
    let found = findings(&analysis, "stray-italic-correction");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains('.'), "{found:?}");
}

// chktex warning 7: `Accent command 'command' needs use of 'command'`.
/// The accent rules read the source to see how far the accent's argument
/// reaches, so these run against a file on disk.
fn latex_file(name: &str, source: &str) -> Option<Analysis> {
    which::which("kpsewhich").ok()?;
    let dir = crate::support::tmp_root().join(format!("satex-chktex-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(name);
    std::fs::write(&path, source).ok()?;
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    Some(Machine::analyze(source, Some(&path), &cfg))
}

#[test]
fn a_text_accent_on_a_dotted_letter_is_reported() {
    let Some(analysis) =
        latex_file("accent.tex", "\\documentclass{article}\n\\begin{document}\nna\\^jve\n\\end{document}\n")
    else {
        return;
    };
    let found = findings(&analysis, "accent-on-dotted-letter");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains('j'), "{found:?}");
}

file_none! {
    a_text_accent_the_encoding_composes_is_not_reported("composed.tex", "\\documentclass{article}\n\\begin{document}\nna\\'ive and \\'{\\i}dem and \\^{}i\n\\end{document}\n", "accent-on-dotted-letter");
}

#[test]
fn a_math_accent_on_a_dotted_letter_is_reported() {
    let Some(analysis) =
        latex_file("mathaccent.tex", "\\documentclass{article}\n\\begin{document}\n$\\hat i$\n\\end{document}\n")
    else {
        return;
    };
    let found = findings(&analysis, "accent-on-dotted-letter");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains('i'), "{found:?}");
}

file_none! {
    a_math_accent_on_the_dotless_letter_is_not_reported("dotless.tex", "\\documentclass{article}\n\\begin{document}\n$\\hat\\imath$ and $\\bar x$\n\\end{document}\n", "accent-on-dotted-letter");
}

// chktex warning 17: `Number of 'character' doesn't match the number of
// 'character'`.
/// The rules below read the source file itself, so they need a document on
/// disk rather than a string.
struct OnDisk {
    dir: std::path::PathBuf,
    analysis: Analysis,
}

impl Drop for OnDisk {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A one-file project in a fresh directory, analyzed.  `kernel` runs the
/// real LaTeX2e kernel and gives `None` without a TeX installation.
fn on_disk(name: &str, source: &str, kernel: bool) -> Option<OnDisk> {
    if kernel && which::which("kpsewhich").is_err() {
        return None;
    }
    let dir = crate::support::tmp_root().join(format!("satex-chktex-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).ok()?;
    let cfg = match kernel {
        true => Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() },
        false => Config {
            load: Load { packages: false, classes: false, inputs: false, format: false, ..Default::default() },
            texmf: Texmf { kpsewhich: false, ..Default::default() },
            ..crate::support::config()
        },
    };
    let analysis = Machine::analyze(source, Some(&path), &cfg);
    Some(OnDisk { dir, analysis })
}

#[test]
fn a_file_with_more_openers_than_closers_is_reported() {
    let doc = on_disk("open", "\\begin{document}\nThis (is a test. And [this too.\n\\end{document}\n", false)
        .expect("temporary directory");
    let found = findings(&doc.analysis, "unbalanced-brackets");
    assert!(found.iter().any(|m| m.contains("1 `(` and 0 `)`")), "{found:?}");
    assert!(found.iter().any(|m| m.contains("1 `[` and 0 `]`")), "{found:?}");
}

bare_none! {
    brackets_that_match_are_not_reported("match", "\\begin{document}\nBalanced (ok) and [ok].\n\\end{document}\n", "unbalanced-brackets");
}

/// A bracket in math mode is a delimiter, so a half-open interval is not a
/// mismatch.  chktex counts it and reports one.
#[test]
fn a_half_open_interval_is_not_a_mismatch() {
    let Some(doc) = on_disk(
        "interval",
        "\\documentclass{article}\n\\begin{document}\nThe interval $[a,b)$ is half-open.\n\\end{document}\n",
        true,
    ) else {
        return;
    };
    assert!(findings(&doc.analysis, "unbalanced-brackets").is_empty());
}

bare_none! {
    // chktex warnings 20 and 44: `User-specified pattern found: %s` and
    // `User Regex: %s`.
    /// Both lists are empty until `satex.yaml` carries them, so the rule is
    /// silent here; `lint::sourcetext`'s own tests drive the matcher with a list.
    no_user_pattern_is_configured_by_default("patterns", "\\begin{document}\nChkTeX is spelled Chk\\TeX.\n\\end{document}\n", "user-pattern");
}

// chktex warning 21: `This command might not be intended`.
#[test]
fn an_escaped_period_after_a_command_is_reported() {
    let doc = on_disk("escape", "\\begin{document}\nA \\TeX\\. Right?\n\\end{document}\n", false)
        .expect("temporary directory");
    let found = findings(&doc.analysis, "unintended-escape");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("\\TeX") && found[0].contains("\\."), "{found:?}");
}

bare_none! {
    a_plain_period_after_a_command_is_not_reported("period", "\\begin{document}\nA \\TeX. Right?\n\\end{document}\n", "unintended-escape");
    /// `\,` is a thin space in math, where chktex reports it all the same.
    an_escaped_mark_in_math_is_not_reported("mathmark", "\\begin{document}\n$\\alpha\\, dx$\n\\end{document}\n", "unintended-escape");
}

// chktex warning 22: `Comment displayed.`
/// satex reports the markers a reader wants back rather than every comment.
#[test]
fn a_todo_marker_in_a_comment_is_reported() {
    let doc = on_disk("todo", "\\begin{document}\nHello. % TODO: fix this later\n\\end{document}\n", false)
        .expect("temporary directory");
    let found = findings(&doc.analysis, "comment-marker");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("TODO") && found[0].contains("fix this later"), "{found:?}");
}

bare_none! {
    a_comment_without_a_marker_is_not_reported("remark", "\\begin{document}\nHello. % a remark about TODOS\n\\end{document}\n", "comment-marker");
}

// chktex warning 31: `This text may be ignored`.
/// `fancyvrb` reads its body line by line and drops the rest of the closing
/// line; the kernel's `verbatim` keeps it, and chktex cannot tell them apart.
#[test]
fn text_a_verbatim_environment_drops_is_reported() {
    let Some(doc) = on_disk(
        "fancyvrb",
        "\\documentclass{article}\n\\usepackage{fancyvrb}\n\\begin{document}\n\
\\begin{Verbatim}\ncode\n\\end{Verbatim} tail two\n\\end{document}\n",
        true,
    ) else {
        return;
    };
    let found = findings(&doc.analysis, "ignored-verbatim-tail");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("tail two"), "{found:?}");
}

#[test]
fn text_the_kernel_verbatim_keeps_is_not_reported() {
    let Some(doc) = on_disk(
        "verbatim",
        "\\documentclass{article}\n\\begin{document}\n\
\\begin{verbatim}\ncode\n\\end{verbatim} tail three\n\\end{document}\n",
        true,
    ) else {
        return;
    };
    assert!(findings(&doc.analysis, "ignored-verbatim-tail").is_empty());
}

// chktex warnings 18 and 19: a character that is not a quotation mark
// standing in for one.
#[test]
fn a_straight_double_quote_is_reported() {
    let analysis = analyze(r#"He said "hello" to me."#);
    let found = findings(&analysis, "wrong-quote-character");
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found[0].contains("opens a quotation"), "{found:?}");
    assert!(found[1].contains("closes a quotation"), "{found:?}");
}

#[test]
fn a_prime_in_math_mode_is_not_reported() {
    let Some(analysis) = latex(r"\documentclass{article}\begin{document}$f'(x)$ and $a''$\end{document}") else {
        return;
    };
    assert!(findings(&analysis, "quote-direction").is_empty());
    assert!(findings(&analysis, "csquotes-available").is_empty());
}

#[test]
fn marks_the_group_kept_apart_are_not_reported() {
    let Some(analysis) = latex(r"\documentclass{article}\begin{document}A ``{}` run.\end{document}") else { return };
    assert!(findings(&analysis, "quote-ligature").is_empty());
}

#[test]
fn a_document_with_csquotes_is_not_asked_to_load_it() {
    let Some(analysis) =
        latex(r"\documentclass{article}\usepackage{csquotes}\begin{document}\enquote{hello}\end{document}")
    else {
        return;
    };
    assert!(findings(&analysis, "csquotes-available").is_empty());
}

#[test]
fn a_language_that_does_not_use_english_quotes_gets_no_english_advice() {
    let Some(analysis) = latex(
        r#"\documentclass{article}\usepackage[french]{babel}\begin{document}Il a dit 'ceci` et "cela".\end{document}"#,
    ) else {
        return;
    };
    assert!(findings(&analysis, "quote-direction").is_empty());
    assert!(findings(&analysis, "wrong-quote-character").is_empty());
}

/// A short article around `body`, for the math rules below.
fn math_document(body: &str) -> String {
    format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\n\\end{{document}}\n")
}

// chktex warnings 45 and 46: `Use \[ ... \] instead of $$ ... $$.` and
// `Use \( ... \) instead of $ ... $.`
#[test]
fn dollars_around_math_are_reported() {
    let Some(doc) = on_disk("dollars", &math_document(r"Display $$a+b$$ and inline $c+d$ here."), true) else {
        return;
    };
    let found = findings(&doc.analysis, "math-shift-dollars");
    assert!(found.iter().any(|m| m.contains("$$ … $$") && m.contains(r"\[ … \]")), "{found:?}");
    assert!(found.iter().any(|m| m.contains("$ … $") && m.contains(r"\( … \)")), "{found:?}");
}

math_none! {
    latex_math_delimiters_are_not_reported("delimiters", r"Display \[a+b\] and inline \(c+d\) here.", "math-shift-dollars");
}

// chktex warnings 3 and 25: `You should enclose the previous parenthesis
// with '{}'.` and `You might wish to put this between a pair of '{}'`.
#[test]
fn a_script_of_more_than_one_token_is_reported() {
    let Some(doc) = on_disk("script", &math_document(r"Math $x_12$ and $y^abc$ here."), true) else {
        return;
    };
    let found = findings(&doc.analysis, "math-script-braces");
    assert!(found.iter().any(|m| m.contains("`_12`")), "{found:?}");
    assert!(found.iter().any(|m| m.contains("`^abc`")), "{found:?}");
}

#[test]
fn a_script_on_a_closing_bracket_is_reported() {
    let Some(doc) = on_disk("bracket", &math_document(r"Math $(a+b)^2$ and $[c]_i$ here."), true) else {
        return;
    };
    let found = findings(&doc.analysis, "math-script-braces");
    assert!(found.iter().any(|m| m.contains("`^` attaches only to the `)`")), "{found:?}");
    assert!(found.iter().any(|m| m.contains("`_` attaches only to the `]`")), "{found:?}");
}

math_none! {
    /// `\left … \right` is one atom, so the script takes all of it; chktex
    /// reports this one.
    a_script_after_right_is_not_reported("right", r"Math $\left(a+b\right)^2$ and $x^{12}$ here.", "math-script-braces");
}

// chktex warning 43: `'%s' is normally not followed by '%c'.`
#[test]
fn a_brace_after_left_is_reported() {
    let Some(doc) = on_disk("left", &math_document(r"Math $\left{a\right}$ here."), true) else {
        return;
    };
    let found = findings(&doc.analysis, "delimiter-not-followed");
    assert!(found.iter().any(|m| m.contains(r"`\left` scans a delimiter, and `{`")), "{found:?}");
    assert!(found.iter().any(|m| m.contains(r"`\right` scans a delimiter, and `}`")), "{found:?}");
}

#[test]
fn an_escaped_brace_after_left_is_not_reported() {
    let source = math_document(r"Math $\left\{a\right\}$ and $\left(b\right)$ here.");
    let Some(doc) = on_disk("escaped", &source, true) else {
        return;
    };
    assert!(findings(&doc.analysis, "delimiter-not-followed").is_empty());
}

// chktex warning 16: `Mathmode still on at end of LaTeX file.`
#[test]
fn math_left_open_is_reported() {
    let Some(doc) = on_disk("open", &math_document("Text $x + y\n\nnext paragraph"), true) else {
        return;
    };
    let found = findings(&doc.analysis, "math-still-open");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("math mode was still on"), "{found:?}");
}

math_none! {
    closed_math_is_not_reported("closed", r"Text $x + y$ here.", "math-still-open");
}

/// A run a limit stops inside a formula has not reached the end of the
/// text: the formula may close after it.
#[test]
fn math_open_where_a_limit_stopped_the_run_is_not_reported() {
    let source = "\\catcode`\\$=3 \\catcode`\\{=1 \\catcode`\\}=2 \\count1=0\n\
                  \\def\\b{\\advance\\count1 by 1 \\ifnum\\count1<100000000 \\expandafter\\b\\fi}\n\
                  Text $x \\b y$ here.\n";
    let mut cfg = crate::support::bare();
    cfg.limits.steps = 100_000;
    let analysis = Machine::analyze(source, None, &cfg);
    assert!(findings(&analysis, "math-still-open").is_empty());
}

// chktex warning 49: `Expected math mode to be %s here.`
#[test]
fn a_math_delimiter_in_the_wrong_mode_is_reported() {
    let Some(doc) = on_disk("badmath", &math_document(r"Text $a$ \) here."), true) else {
        return;
    };
    let found = findings(&doc.analysis, "math-mode-mismatch");
    assert!(found.iter().any(|m| m.contains(r"`\)` expects math mode to be on here")), "{found:?}");
}

math_none! {
    /// `\protected@edef` stores `\(x\)` without running it, as a glossary
    /// stores an entry's description: no `\@badmath`, whatever the mode around.
    a_math_delimiter_that_is_only_stored_is_not_reported("stored", r"\makeatletter\protected@edef\e{\(x\)}\makeatother Text.", "math-mode-mismatch");
    paired_math_delimiters_are_not_reported("paired", r"Text \(a\) and \[b\] here.", "math-mode-mismatch");
}

// chktex warning 35: `You should perhaps use 'cmd' instead.`
#[test]
fn an_operator_name_in_math_italic_is_reported() {
    let Some(doc) = on_disk("operator", &math_document(r"Write $sin(x)$ here."), true) else {
        return;
    };
    let found = findings(&doc.analysis, "math-operator-name");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains(r"`\sin`"), "{found:?}");
}

math_none! {
    /// chktex reports `\mathrm{sin}`, which is already upright.
    an_operator_set_by_its_command_is_not_reported("upright", r"Write $\sin(x)$ and $\mathrm{sin}(x)$ here.", "math-operator-name");
    letters_with_no_command_behind_them_are_not_reported("letters", r"Write $abc$ here.", "math-operator-name");
}

// chktex warning 11: `You should use '%s' to achieve an ellipsis.`
#[test]
fn three_periods_are_reported() {
    let Some(doc) = on_disk("dots", &math_document(r"Text ... here and $1 + ... + 2$ there."), true) else {
        return;
    };
    let found = findings(&doc.analysis, "ellipsis-dots");
    assert!(found.iter().any(|m| m.contains(r"`\dots`")), "{found:?}");
    assert!(found.iter().any(|m| m.contains(r"`\cdots`")), "{found:?}");
}

math_none! {
    an_ellipsis_command_is_not_reported("ldots", r"Text \dots{} here and $x_1, \ldots, x_n$ there.", "ellipsis-dots");
}

// chktex warning 29: `$\times$ may look prettier here.`
#[test]
fn an_x_between_numbers_is_reported() {
    let Some(doc) = on_disk("times", &math_document(r"Screen 640x480 and $3x4$ here."), true) else {
        return;
    };
    let found = findings(&doc.analysis, "dimension-times");
    assert!(found.iter().any(|m| m.contains("`640x480`") && m.contains(r"`$\times$`")), "{found:?}");
    assert!(found.iter().any(|m| m.contains("`3x4`") && m.contains(r"`\times`")), "{found:?}");
}

math_none! {
    an_x_beside_letters_is_not_reported("letter-x", r"Screen 640 by 480 and ax4 and 4xa here.", "dimension-times");
}

// chktex warning 40: `You should put punctuation outside inner/inside
// display math mode.`
#[test]
fn punctuation_on_the_wrong_side_of_a_formula_is_reported() {
    let Some(doc) = on_disk("punctuation", &math_document("Inline $a+b.$ here.\n\\[ c+d \\]."), true) else {
        return;
    };
    let found = findings(&doc.analysis, "math-punctuation");
    assert!(found.iter().any(|m| m.contains("set inside the formula")), "{found:?}");
    assert!(found.iter().any(|m| m.contains("set after the display")), "{found:?}");
}

/// chktex follows `\]` and `$$` only, so it misses the environment.
#[test]
fn punctuation_after_an_equation_environment_is_reported() {
    let Some(doc) = on_disk("equation", &math_document(r"\begin{equation} a+b \end{equation}."), true) else {
        return;
    };
    let found = findings(&doc.analysis, "math-punctuation");
    assert!(found.iter().any(|m| m.contains("set after the display")), "{found:?}");
}

math_none! {
    punctuation_on_the_right_side_is_not_reported("inside", "Inline $a+b$, here.\n\\[ c+d. \\]", "math-punctuation");
}

// chktex warnings 1, 2, 12, 13, 24, 26, 30, 36, 37, 39 and 42: the spacing
// rules in `src/lint/spacing.rs`.

// chktex warning 1: `Command terminated with space.`
#[test]
fn a_command_that_ate_its_space_is_reported() {
    let source = "\\documentclass{article}\n\\begin{document}\nQ \\TeX ok.\n\\end{document}\n";
    let Some(doc) = on_disk("terminated", source, true) else { return };
    let found = findings(&doc.analysis, "command-terminated-space");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("TEXok"), "{found:?}");
}

#[test]
fn a_command_that_sets_nothing_where_the_space_stood_is_not_reported() {
    let source = "\\documentclass{article}\n\\begin{document}\nR {\\rm rm} q.\nS \\noindent nn.\n\
                  \\begin{itemize}\n\\item ii jj\n\\end{itemize}\nU $\\alpha x$ v.\n\\end{document}\n";
    let Some(doc) = on_disk("silent-space", source, true) else { return };
    assert!(findings(&doc.analysis, "command-terminated-space").is_empty());
}

// chktex warning 2: `Non-breaking space ('~') should have been used.`
#[test]
fn a_space_in_front_of_a_reference_is_reported() {
    let source = "\\documentclass{article}\n\\begin{document}\nSection \\ref{a} is next.\\label{a}\n\\end{document}\n";
    let Some(doc) = on_disk("tie", source, true) else { return };
    let found = findings(&doc.analysis, "missing-nonbreaking-space");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("the reference"), "{found:?}");
}

/// `\Cref` and `\autoref` typeset the name themselves, so the space is right.
#[test]
fn a_space_in_front_of_a_naming_reference_is_not_reported() {
    let source = "\\documentclass{article}\n\\usepackage{cleveref}\n\\begin{document}\n\
                  Section \\label{a} see \\Cref{a} and \\cref{a} here.\n\\end{document}\n";
    let Some(doc) = on_disk("naming-ref", source, true) else { return };
    assert!(findings(&doc.analysis, "missing-nonbreaking-space").is_empty());
}

/// The space in front of it was read in vertical mode, so it set nothing.
#[test]
fn a_reference_that_opens_a_paragraph_is_not_reported() {
    let source =
        "\\documentclass{article}\n\\begin{document}\n\\label{a}\n\\ref{a} opens a paragraph.\n\\end{document}\n";
    let Some(doc) = on_disk("vertical-tie", source, true) else { return };
    assert!(findings(&doc.analysis, "missing-nonbreaking-space").is_empty());
    assert!(findings(&doc.analysis, "space-before-command").is_empty());
}

fn cleveref_records(name: &str, body: &str) -> Vec<satex::query::Record> {
    let source = format!("\\documentclass{{article}}\n{body}");
    let Some(doc) = on_disk(name, &source, true) else { return Vec::new() };
    satex::lint::lint(&doc.analysis).into_iter().filter(|r| r["code"] == "prefer-cleveref").collect()
}

fn replacements(record: &satex::query::Record) -> Vec<String> {
    let edits = record.get("edits").and_then(|e| e.as_array()).cloned().unwrap_or_default();
    edits.iter().filter_map(|e| e["replacement"].as_str().map(str::to_string)).collect()
}

#[test]
fn a_hand_written_name_is_fixed_to_cref_when_cleveref_is_loaded() {
    if !crate::support::installed() {
        return;
    }
    let body = "\\usepackage{cleveref}\n\\begin{document}\n\\section{A}\\label{sec:x}\n\
                Section~\\ref{sec:x}, Sec.~\\ref{sec:x}, see section~\\ref{sec:x} here.\n\
                Sections~\\ref{sec:x} and~\\ref{sec:x}.\n\\end{document}\n";
    let found = cleveref_records("cleveref-loaded", body);
    let fixes: Vec<String> = found.iter().flat_map(replacements).collect();
    assert_eq!(fixes, [r"\Cref{sec:x}", r"\Cref{sec:x}", r"\cref{sec:x}", r"\Cref{sec:x,sec:x}"], "{found:?}");
    assert!(found[0]["message"].as_str().unwrap().contains(r"`\Cref{sec:x}`"), "{found:?}");
}

#[test]
fn a_hand_written_name_asks_for_cleveref_when_it_is_missing() {
    if !crate::support::installed() {
        return;
    }
    let body = "\\usepackage{hyperref}\n\\begin{document}\n\\section{A}\\label{sec:x}\nSection~\\ref{sec:x}.\n\\end{document}\n";
    let found = cleveref_records("cleveref-missing", body);
    assert_eq!(found.len(), 1, "{found:?}");
    let message = found[0]["message"].as_str().unwrap();
    assert!(message.contains("load cleveref (after hyperref)") && message.contains(r"\Cref{sec:x}"), "{message}");
    assert!(replacements(&found[0]).is_empty(), "{found:?}");
}

#[test]
fn the_name_must_fit_the_kind_of_the_label() {
    if !crate::support::installed() {
        return;
    }
    let body = "\\usepackage{cleveref}\n\\begin{document}\n\\begin{figure}\\caption{c}\\label{f:y}\\end{figure}\n\
                Fig.~\\ref{f:y} is one.\nSee table~\\ref{f:y} and in~\\ref{f:y}.\n\\end{document}\n";
    let found = cleveref_records("cleveref-kind", body);
    let fixes: Vec<String> = found.iter().flat_map(replacements).collect();
    assert_eq!(fixes, [r"\Cref{f:y}"], "{found:?}");
}

#[test]
fn names_and_lists_follow_the_kind_and_the_plural() {
    if !crate::support::installed() {
        return;
    }
    let body = "\\usepackage{hyperref}\n\\usepackage{cleveref}\n\\begin{document}\n\\section{A}\\label{sec:a}\n\
                \\begin{figure}\\caption{F}\\label{fig:f}\\end{figure}\n\\section{B}\\label{sec:b}\n\
                Section~\\ref{sec:a} and Fig.~\\ref{fig:f}, see~\\ref{sec:b}, \
                Sections~\\ref{sec:a} and~\\ref{sec:b}.\n\\end{document}\n";
    let found = cleveref_records("cleveref-repro", body);
    let fixes: Vec<String> = found.iter().flat_map(replacements).collect();
    assert_eq!(fixes, [r"\Cref{sec:a}", r"\Cref{fig:f}", r"\Cref{sec:a,sec:b}"], "{found:?}");
}

#[test]
fn a_plain_document_is_told_to_load_cleveref_for_a_named_reference() {
    if !crate::support::installed() {
        return;
    }
    let body = "\\begin{document}\n\\section{A}\\label{sec:a}\nSection~\\ref{sec:a}.\nWe see~\\ref{sec:a} and in~\\ref{sec:a}.\n\\end{document}\n";
    let found = cleveref_records("cleveref-plain", body);
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0]["message"].as_str().unwrap().contains("load cleveref"), "{found:?}");
    assert!(replacements(&found[0]).is_empty(), "{found:?}");
}

#[test]
fn a_reference_without_a_name_is_not_a_cleveref_candidate() {
    let body = "\\usepackage{amsmath}\n\\begin{document}\n\\section{A}\\label{x}\n\
                \\eqref{x} (\\ref{x}) \\pageref{x} \\ref{x} and e.g.~\\ref{x} \\foo~\\ref{x} see~\\ref{x} in~\\ref{x}.\n\\end{document}\n";
    let found = cleveref_records("cleveref-negative", body);
    assert!(found.is_empty(), "{found:?}");
}

// chktex warnings 24 and 42: `Delete this space to maintain correct
// pagereferences.` and `You should remove spaces in front of '%s'`.
#[test]
fn a_space_in_front_of_a_label_or_a_footnote_is_reported() {
    let source =
        "\\documentclass{article}\n\\begin{document}\nWord \\label{a} and text \\footnote{f} here.\n\\end{document}\n";
    let Some(doc) = on_disk("hugging", source, true) else { return };
    let found = findings(&doc.analysis, "space-before-command");
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().any(|f| f.contains("the label")), "{found:?}");
    assert!(found.iter().any(|f| f.contains("\\footnote")), "{found:?}");
}

// chktex warning 39: `Double space found.`
#[test]
fn a_tie_next_to_an_ordinary_space_is_reported() {
    let source = "\\documentclass{article}\n\\begin{document}\nDr.~ Smith and Mr. ~Jones.\n\\end{document}\n";
    let Some(doc) = on_disk("tie-space", source, true) else { return };
    let found = findings(&doc.analysis, "double-space");
    assert_eq!(found.len(), 2, "{found:?}");
}

// chktex warning 12: `Interword spacing ('\ ') should perhaps be used.`
#[test]
fn an_abbreviation_before_a_lowercase_word_is_reported() {
    let analysis = analyze(r"\begin{document}etc. and so on and on.\end{document}");
    let found = findings(&analysis, "sentence-spacing");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("etc.") && found[0].contains("runs on"), "{found:?}");
}

/// The abbreviation is named from the start of its line, not run on into
/// the text before a `\\` break.
#[test]
fn an_abbreviation_after_a_line_break_is_named_alone() {
    let analysis = analyze(r"\begin{document}\parbox{3cm}{per cell\\e.g. flowR and so on}\end{document}");
    let found = findings(&analysis, "sentence-spacing");
    assert!(found.iter().any(|f| f.contains("`e.g.`")) && !found.iter().any(|f| f.contains("celle")), "{found:?}");
}

// chktex warning 13: `Intersentence spacing ('\@') should perhaps be used.`
#[test]
fn a_period_after_capitals_before_a_capital_is_reported() {
    let analysis = analyze(r"\begin{document}The ABC. Next one here.\end{document}");
    let found = findings(&analysis, "sentence-spacing");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("ABC") && found[0].contains("a sentence ends"), "{found:?}");
}

// chktex warning 26: `You ought to remove spaces in front of punctuation.`
#[test]
fn a_space_in_front_of_punctuation_is_reported() {
    let analysis = analyze(r"\begin{document}one , two ; three here.\end{document}");
    let found = findings(&analysis, "space-before-punctuation");
    assert_eq!(found.len(), 2, "{found:?}");
}

// chktex warnings 36 and 37: the space around a parenthesis.
#[test]
fn spaces_inside_parentheses_are_reported() {
    let analysis = analyze(r"\begin{document}one ( two ) three.\end{document}");
    let found = findings(&analysis, "parenthesis-spacing");
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|f| f.contains("inside the parentheses")), "{found:?}");
}

/// A tie or a control space is interword glue: a space on the page.
#[test]
fn a_tie_after_a_parenthesis_is_a_space() {
    let source =
        "\\documentclass{article}\n\\begin{document}\nSee (a)~one and (b)\\ two, but (c)three.\n\\end{document}\n";
    let Some(doc) = on_disk("tie-paren", source, true) else { return };
    let found = findings(&doc.analysis, "parenthesis-spacing");
    assert_eq!(found.len(), 1, "{found:?}");
}

/// An inline list sets its label in a box placed later: the label's `)`
/// and the item's first word are not neighbours in the order TeX set them.
#[test]
fn an_inline_list_label_is_not_set_against_the_item() {
    let source = "\\documentclass{article}\n\\usepackage[inline]{enumitem}\n\\begin{document}\n\\begin{enumerate*}[label=(\\arabic*)]\n\\item one \\item two\n\\end{enumerate*}\n\\end{document}\n";
    let Some(doc) = on_disk("inline-list-paren", source, true) else { return };
    assert!(findings(&doc.analysis, "parenthesis-spacing").is_empty());
}

/// Math needs the real catcode of `$`, so this one runs the kernel.
#[test]
fn a_parenthesis_in_math_is_not_reported() {
    let source = "\\documentclass{article}\n\\begin{document}\nThe value $f (x )$ is one.\n\\end{document}\n";
    let Some(doc) = on_disk("math-paren", source, true) else { return };
    assert!(findings(&doc.analysis, "parenthesis-spacing").is_empty());
}

// chktex warning 4: an italic correction that sets a kern of zero, which
// satex decides from the font the character was really set in.
#[test]
fn an_italic_correction_after_an_upright_letter_is_reported() {
    let Some(analysis) = latex_file(
        "upright.tex",
        "\\documentclass{article}\n\\begin{document}\n{\\bfseries bold\\/} text\n\\end{document}\n",
    ) else {
        return;
    };
    let found = findings(&analysis, "zero-italic-correction");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("cmbx10") && found[0].contains("kern of zero"), "{found:?}");
}

#[test]
fn an_italic_correction_after_a_space_is_reported() {
    let Some(analysis) =
        latex_file("glue.tex", "\\documentclass{article}\n\\begin{document}\nword \\/ more\n\\end{document}\n")
    else {
        return;
    };
    let found = findings(&analysis, "zero-italic-correction");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("glue"), "{found:?}");
}

file_none! {
    /// chktex reports this one, because `\textit` is not on its italic list.
    an_italic_correction_the_font_really_has_is_not_reported("wanted.tex", "\\documentclass{article}\n\\begin{document}\n\\textit{word\\/} after and \\emph{more\\/} here\n\\end{document}\n", "zero-italic-correction");
}

// chktex warning 6: leaving a slanted shape with no italic correction.
// `\emph` is a `\DeclareTextFontCommand`, whose `\check@icr` adds the `\/`
// itself (ltfntcmd.dtx), so only the declaration form lacks it.
#[test]
fn leaving_a_slanted_shape_without_a_correction_is_reported() {
    let Some(analysis) = latex_file(
        "overhang.tex",
        "\\documentclass{article}\n\\begin{document}\nThe \\emph{f} and {\\em f} before roman text\n\\end{document}\n",
    ) else {
        return;
    };
    let found = findings(&analysis, "missing-italic-correction");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("cmti10") && found[0].contains("`f`"), "{found:?}");
}

file_none! {
    a_slanted_shape_that_ends_with_a_correction_is_not_reported("corrected.tex", "\\documentclass{article}\n\\begin{document}\nThe \\emph{f\\/} before roman text\n\\end{document}\n", "missing-italic-correction");
    /// A correction in front of a period or a comma is chktex 28's business.
    a_slanted_shape_before_small_punctuation_is_not_reported("punctuated.tex", "\\documentclass{article}\n\\begin{document}\nThe \\emph{word}, and \\emph{another}.\n\\end{document}\n", "missing-italic-correction");
}

#[test]
fn upright_text_alone_is_not_reported() {
    let Some(analysis) = latex_file(
        "upright-only.tex",
        "\\documentclass{article}\n\\begin{document}\nPlain \\textbf{bold} and \\textsf{sans} text\n\\end{document}\n",
    ) else {
        return;
    };
    assert!(findings(&analysis, "missing-italic-correction").is_empty());
    assert!(findings(&analysis, "zero-italic-correction").is_empty());
}

// satex's own rule, with no chktex counterpart: `unreferenced-numbered-display`.
/// A display that stepped a counter nothing references.  These run the real
/// kernel and amsmath, since the rule reads `\@currentlabel` off the run.
fn display(name: &str, body: &str) -> Option<OnDisk> {
    let source =
        format!("\\documentclass{{article}}\n\\usepackage{{amsmath}}\n\\begin{{document}}\n{body}\\end{{document}}\n");
    on_disk(name, &source, true)
}

#[test]
fn a_referenced_equation_is_not_reported() {
    let Some(doc) =
        display("eq-used", "\\begin{equation}\n  a = b\n  \\label{eq:used}\n\\end{equation}\nSee \\eqref{eq:used}.\n")
    else {
        return;
    };
    assert!(findings(&doc.analysis, "unreferenced-numbered-display").is_empty());
}

#[test]
fn an_unreferenced_equation_is_reported() {
    let Some(doc) = display("eq-unused", "\\begin{equation}\n  a = b\n  \\label{eq:unused}\n\\end{equation}\n") else {
        return;
    };
    let found = findings(&doc.analysis, "unreferenced-numbered-display");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("eq:unused"), "{found:?}");
}

/// A numbered display with no label at all can never be referenced.
#[test]
fn an_unlabelled_numbered_equation_is_reported() {
    let Some(doc) = display("eq-bare", "\\begin{equation}\n  a = b\n\\end{equation}\n") else {
        return;
    };
    let found = findings(&doc.analysis, "unreferenced-numbered-display");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("carries no label"), "{found:?}");
}

/// The other two rows may still want their numbers.
#[test]
fn an_align_with_one_row_referenced_is_not_reported() {
    let Some(doc) = display(
        "align-row",
        "\\begin{align}\n  x &= 1 \\label{eq:r1} \\\\\n  y &= 2 \\\\\n  z &= 3\n\\end{align}\nRow \\eqref{eq:r1}.\n",
    ) else {
        return;
    };
    assert!(findings(&doc.analysis, "unreferenced-numbered-display").is_empty());
}

#[test]
fn a_starred_equation_is_not_reported() {
    let Some(doc) = display("eq-star", "\\begin{equation*}\n  a = b\n\\end{equation*}\n") else {
        return;
    };
    assert!(findings(&doc.analysis, "unreferenced-numbered-display").is_empty());
}

/// A float's number belongs to its `\caption`, and `figure*` is the
/// two-column float, not an unnumbered one.
#[test]
fn a_figure_with_an_unreferenced_caption_is_not_reported() {
    let Some(doc) = display("float", "\\begin{figure}\n  \\caption{A figure}\n  \\label{fig:one}\n\\end{figure}\n")
    else {
        return;
    };
    assert!(findings(&doc.analysis, "unreferenced-numbered-display").is_empty());
}

// chktex warnings 8, 11, 18, 29 and the spacing family: text the run read
// verbatim is program text, so no typographic rule may fire on it.

/// The characters the rules below all disagree with, in one line.
const PROGRAM_TEXT: &str = "printf( \"[\" ) -- a ... 640x480";

/// One rule per kind of typography the line above trips.
const TYPOGRAPHY: [&str; 6] = [
    "wrong-quote-character",
    "wrong-dash",
    "ellipsis-dots",
    "dimension-times",
    "parenthesis-spacing",
    "space-before-punctuation",
];

#[test]
fn typography_is_not_reported_inside_a_verbatim_body() {
    let source = format!(
        "\\documentclass{{article}}\n\\begin{{document}}\n\\begin{{verbatim}}\n{PROGRAM_TEXT}\n\\end{{verbatim}}\n\\end{{document}}\n"
    );
    let Some(doc) = on_disk("verbatim-body", &source, true) else {
        return;
    };
    for code in TYPOGRAPHY {
        let found = findings(&doc.analysis, code);
        assert!(found.is_empty(), "{code}: {found:?}");
    }
    assert!(findings(&doc.analysis, "unbalanced-brackets").is_empty());
}

#[test]
fn typography_is_not_reported_after_an_inline_verbatim_reader() {
    let source =
        format!("\\documentclass{{article}}\n\\begin{{document}}\nA line: \\verb|{PROGRAM_TEXT}|\n\\end{{document}}\n");
    let Some(doc) = on_disk("verbatim-inline", &source, true) else {
        return;
    };
    for code in TYPOGRAPHY {
        let found = findings(&doc.analysis, code);
        assert!(found.is_empty(), "{code}: {found:?}");
    }
}

/// The same characters as prose: every rule that the verbatim body silenced
/// has to speak here, or the test above proves nothing.
#[test]
fn the_same_characters_outside_a_verbatim_body_are_reported() {
    let source = format!("\\documentclass{{article}}\n\\begin{{document}}\n{PROGRAM_TEXT}\n\\end{{document}}\n");
    let Some(doc) = on_disk("verbatim-none", &source, true) else {
        return;
    };
    for code in TYPOGRAPHY {
        assert!(!findings(&doc.analysis, code).is_empty(), "{code} did not fire");
    }
    assert_eq!(findings(&doc.analysis, "unbalanced-brackets").len(), 1);
}

/// A `)` right after a macro's argument is read by a look ahead and given
/// back: it must keep its place in the text.
#[test]
fn a_bracket_after_a_macro_argument_is_counted() {
    let source = "\\documentclass{article}\n\\usepackage{cleveref}\n\\usepackage{xspace}\n\\begin{document}\n\
                  \\section{S}\\label{x}\n\
                  Text (see \\Cref{x}) and (cf.~\\Cref{x}), then (\\cite{z}) and (\\emph{y}), (\\textbf{w}\\/).\n\
                  \\begin{thebibliography}{9}\\bibitem{z} Z.\\end{thebibliography}\n\\end{document}\n";
    let Some(doc) = on_disk("bracket-lookahead", source, true) else { return };
    let found = findings(&doc.analysis, "unbalanced-brackets");
    assert!(found.is_empty(), "{found:?}");
}

/// A part the run never read may reference the label: the finding says
/// so and deletes nothing.
#[test]
fn an_unused_label_may_be_used_in_a_file_not_read() {
    if which::which("kpsewhich").is_err() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-chktex-unread-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temporary directory");
    std::fs::write(dir.join("part.tex"), "See \\ref{s}.\n").expect("part");
    let source = "\\documentclass{article}\n\\begin{document}\n\\section{S}\\label{s}\n\\iffalse\n\\input{part}\n\\fi\n\\end{document}\n";
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).expect("doc");
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let doc = OnDisk { analysis: Machine::analyze(source, Some(&path), &cfg), dir };
    let rows: Vec<_> = satex::lint::lint(&doc.analysis).into_iter().filter(|r| r["code"] == "unused-label").collect();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert!(rows[0]["message"].as_str().is_some_and(|m| m.contains("may be referenced later")), "{rows:?}");
    assert!(rows[0].get("edits").is_none(), "{rows:?}");
}

/// A word with a suffix in parentheses is one word, not a parenthetical
/// remark set against it.
#[test]
fn a_word_with_a_part_in_parentheses_is_not_reported() {
    let source = "\\documentclass{article}\n\\begin{document}\nCells hold file(s) and print(x) too, but text(like this) is not.\n\\end{document}\n";
    let Some(doc) = on_disk("paren-word", source, true) else { return };
    let found = findings(&doc.analysis, "parenthesis-spacing");
    assert_eq!(found.len(), 1, "{found:?}");
}

/// The next `\item`'s label is set in its own box: a sentence ending
/// before it does not run on.
#[test]
fn a_sentence_before_an_item_label_ends() {
    let source = "\\documentclass{article}\n\\begin{document}\n\\begin{description}\n\\item[error] It is set by default.\n\\item[eval] The eval option.\n\\end{description}\nSee e.g. the end.\n\\end{document}\n";
    let Some(doc) = on_disk("sentence-item", source, true) else { return };
    let found = findings(&doc.analysis, "sentence-spacing");
    assert!(found.len() == 1 && found[0].contains("e.g."), "{found:?}");
}

/// `::` and `:=` are operators: the space in front is not one before a colon.
#[test]
fn a_space_before_a_doubled_colon_is_not_reported() {
    let source = "\\documentclass{article}\n\\begin{document}\nThe input is pointers :: List, x := 1, and the output : none.\n\\end{document}\n";
    let Some(doc) = on_disk("double-colon", source, true) else { return };
    let found = findings(&doc.analysis, "space-before-punctuation");
    assert_eq!(found.len(), 1, "{found:?}");
}

/// An inline reader covers its argument, not the rest of the line, and a
/// macro that sets `\` back to an escape reads nothing verbatim.
#[test]
fn a_bracket_after_an_inline_reader_is_counted() {
    let source = "\\documentclass{article}\n\\newcommand\\esc[1]{{\\catcode`\\\\=0 #1}}\n\\begin{document}\n\
                  A (\\verb|x|) b and (see \\esc{y}) c.\n\\end{document}\n";
    let Some(doc) = on_disk("bracket-inline-reader", source, true) else { return };
    let found = findings(&doc.analysis, "unbalanced-brackets");
    assert!(found.is_empty(), "{found:?}");
}

#[test]
fn an_open_bracket_is_still_reported_after_lookahead_macros() {
    let source = "\\documentclass{article}\n\\usepackage{cleveref}\n\\begin{document}\n\
                  \\section{S}\\label{x}\nText (open \\Cref{x}.\n\\end{document}\n";
    let Some(doc) = on_disk("bracket-open", source, true) else { return };
    assert_eq!(findings(&doc.analysis, "unbalanced-brackets").len(), 1);
}

/// `on_disk` with more files beside `doc.tex`.
fn with_files(name: &str, source: &str, files: &[(&str, &str)]) -> Option<OnDisk> {
    if which::which("kpsewhich").is_err() {
        return None;
    }
    let dir = crate::support::tmp_root().join(format!("satex-chktex-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    for (file, text) in files {
        std::fs::write(dir.join(file), text).ok()?;
    }
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).ok()?;
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(source, Some(&path), &cfg);
    Some(OnDisk { dir, analysis })
}

#[test]
fn an_internal_name_of_a_package_file_is_not_an_unused_definition() {
    let source = "\\documentclass{article}\n\\usepackage{mine}\n\\begin{document}\nx\n\\end{document}\n";
    let sty = "\\ProvidesPackage{mine}\n\\def\\mine@internal{y}\n";
    let Some(doc) = with_files("sty-private", source, &[("mine.sty", sty)]) else { return };
    assert!(findings(&doc.analysis, "unused-definition").is_empty());
}

/// A name a macro builds with `\csname` where the document calls it (a
/// `\cite` wrapper storing one macro per key) is not written on that line:
/// nothing there to delete, and it is looked up the same way.
#[test]
fn a_name_built_with_csname_is_not_an_unused_definition() {
    let source = "\\documentclass{article}\n\\makeatletter\n\\newcommand\\mycite[1]{\\expandafter\\xdef\\csname mcited#1\\endcsname{x}}\n\\makeatother\n\\begin{document}\nText \\mycite{jupyter2016}.\n\\def\\literal{y}\n\\end{document}\n";
    let Some(doc) = with_files("csname-built", source, &[]) else { return };
    let found = findings(&doc.analysis, "unused-definition");
    assert!(found.iter().all(|f| !f.contains("mcited")), "{found:?}");
    assert!(found.iter().any(|f| f.contains("literal")), "a written name is still reported: {found:?}");
}

/// An ISBN or an ISO date keeps its hyphens: only a lone `digits-digits`
/// word is a range that wants an en-dash.
#[test]
fn hyphens_in_an_identifier_are_not_a_wrong_dash() {
    let source = "\\documentclass{article}\n\\begin{document}\nISBN 978-1-4503-XXXX-X/2018/06, on 2018-06-01, pages 10-20.\n\\end{document}\n";
    let Some(doc) = with_files("isbn", source, &[]) else { return };
    let found = findings(&doc.analysis, "wrong-dash");
    assert_eq!(found.len(), 1, "only the page range: {found:?}");
}

#[test]
fn a_graphic_epstopdf_converts_is_not_missing() {
    let source = "\\documentclass{article}\n\\usepackage{graphicx}\n\\begin{document}\n\
                  \\includegraphics{logo-eps-converted-to.pdf}\n\\includegraphics{nope.pdf}\n\\end{document}\n";
    let Some(doc) = with_files("eps", source, &[("logo.eps", "%!PS-Adobe-3.0 EPSF-3.0\n")]) else { return };
    let found = findings(&doc.analysis, "missing-graphic");
    assert!(found.iter().all(|f| !f.contains("logo")), "{found:?}");
}

#[test]
fn a_frozen_path_gets_no_findings() {
    if which::which("kpsewhich").is_err() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-chktex-frozen-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("frozen")).unwrap();
    std::fs::write(dir.join("frozen/a.tex"), "\\def\\frozenunused{y}\n").unwrap();
    let source =
        "\\documentclass{article}\n\\begin{document}\n\\input{frozen/a}\n\\def\\freeunused{y}\nx\n\\end{document}\n";
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).unwrap();
    let names = |cfg: &Config| {
        let analysis = Machine::analyze(source, Some(&path), cfg);
        findings(&analysis, "unused-definition").join("\n")
    };
    let open = names(&crate::support::discover(&dir));
    assert!(open.contains("frozenunused") && open.contains("freeunused"), "{open}");
    std::fs::write(dir.join("satex.yaml"), "project: {frozen: [\"frozen/**\"]}\n").unwrap();
    let shut = names(&crate::support::discover(&dir));
    assert!(shut.contains("freeunused") && !shut.contains("frozenunused"), "{shut}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A git submodule (a nested `.git`) is someone else's code: its findings
/// go unless `project.freeze_other_repositories` is off.
#[test]
fn a_git_submodule_is_frozen() {
    if which::which("kpsewhich").is_err() {
        return;
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("target/scratch/it-submodule-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join(".git")).unwrap();
    std::fs::create_dir_all(dir.join("vendor")).unwrap();
    std::fs::write(dir.join("vendor/.git"), "gitdir: ../.git/modules/vendor\n").unwrap();
    std::fs::write(dir.join("vendor/a.tex"), "\\def\\vendorunused{y}\n").unwrap();
    let source =
        "\\documentclass{article}\n\\begin{document}\n\\input{vendor/a}\n\\def\\ownunused{y}\nx\n\\end{document}\n";
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).unwrap();
    let names = |cfg: &Config| findings(&Machine::analyze(source, Some(&path), cfg), "unused-definition").join("\n");
    let mut cfg = crate::support::discover(&dir);
    let shut = names(&cfg);
    assert!(shut.contains("ownunused") && !shut.contains("vendorunused"), "{shut}");
    cfg.project.freeze_other_repositories = false;
    let open = names(&cfg);
    assert!(open.contains("vendorunused"), "{open}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A line end between two words, or right before a paragraph end, is no
/// stray space; one before a closing brace is.
#[test]
fn a_line_end_between_words_is_not_a_stray_space() {
    let source = "\\documentclass{article}\n\\def\\blk#1{\\par\\noindent#1: }\n\
                  \\def\\txt{one two\nthree four.\n\\blk{X}five}\n\\def\\bad{x\n}\n\
                  \\begin{document}\n\\txt\\ \\bad y\n\\end{document}\n";
    let Some(doc) = with_files("line-end-words", source, &[]) else { return };
    let found = findings(&doc.analysis, "stray-space");
    assert!(found.iter().all(|f| !f.contains("txt")), "{found:?}");
    assert!(found.iter().any(|f| f.contains("bad")), "{found:?}");
}

/// An engine error in package code that a document line causes is shown at
/// that line, naming where in the package it arose.
#[test]
fn a_package_error_the_document_causes_is_shown_at_the_cause() {
    let Some(analysis) = latex(
        r"\documentclass{article}
\ExplSyntaxOn
\int_set:Nn \l_tmpa_int { 1 + }
\ExplSyntaxOff
\begin{document}x\end{document}",
    ) else {
        return;
    };
    let rows: Vec<_> = satex::lint::lint(&analysis)
        .into_iter()
        .filter(|r| r["code"] == "caused-library-error" && r["origin"] == "document")
        .collect();
    // pdflatex: "! Missing number, treated as zero." at l.3, raised while
    // expl3's integer expression reads `1 +`.
    let missing = rows.iter().find(|r| r["message"].as_str().is_some_and(|m| m.contains("missing number")));
    let missing = missing.unwrap_or_else(|| panic!("no missing-number row: {rows:?}"));
    assert_eq!(missing["line"], 3, "{missing:?}");
    assert!(missing["message"].as_str().unwrap().starts_with("error in expl3-code.tex:"), "{missing:?}");
    // One row per origin line and code.
    let mut keys: Vec<_> = rows.iter().map(|r| (r["line"].to_string(), r["name"].to_string())).collect();
    let all = keys.len();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), all, "{rows:?}");
}

/// An error that only a path through an unknown value raises (fit around
/// a box of unknown width) is a possible one: pdflatex raises none here, so
/// satex gives at most a note, never a warning or an error.
#[test]
fn an_error_only_an_approximation_raises_is_a_note() {
    let Some(analysis) = latex(
        r"\documentclass{article}
\usepackage{tikz}
\usetikzlibrary{fit}
\begin{document}
\begin{tikzpicture}
\node (n) {\rule{\pdfuniformdeviate 100 pt}{1cm}};
\node[draw, fit=(n)] {};
\end{tikzpicture}
\end{document}",
    ) else {
        return;
    };
    let rows: Vec<_> = satex::lint::lint(&analysis)
        .into_iter()
        .filter(|r| r["code"] == "caused-library-error" && r["severity"] != "info")
        .collect();
    assert!(rows.is_empty(), "{rows:?}");
}

/// A document that calls package code correctly causes no such rows.
#[test]
fn a_clean_document_causes_no_package_error() {
    let Some(analysis) = latex(
        r"\documentclass{article}
\ExplSyntaxOn
\str_set:Nn \l_tmpb_str { ab }
\tl_set:Nx \l_tmpa_tl { \str_upper_case:n { \l_tmpb_str } }
\ExplSyntaxOff
\begin{document}x\end{document}",
    ) else {
        return;
    };
    assert_eq!(findings(&analysis, "caused-library-error"), Vec::<String>::new());
}

/// Each finding of `code` as `line:col message`.
fn placed(analysis: &Analysis, code: &str) -> Vec<String> {
    satex::lint::lint(analysis)
        .into_iter()
        .filter(|r| r["code"] == code)
        .map(|r| format!("{}:{} {}", r["line"], r["col"], r["message"].as_str().unwrap_or_default()))
        .collect()
}

const GREET: &str =
    "\\documentclass{article}\n\\newcommand{\\greet}[1]{\\label{hello:#1}Hello, #1!}\n\\begin{document}\n";

/// A label a macro body builds stands at the document's call.
#[test]
fn a_label_from_a_macro_body_is_reported_at_the_call() {
    let Some(analysis) = latex(&format!("{GREET}\\greet{{world}}\n\\end{{document}}\n")) else { return };
    let found = placed(&analysis, "unused-label");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("4:1 ") && found[0].contains("via \\greet"), "{found:?}");
}

#[test]
fn a_label_written_in_the_text_is_reported_where_it_stands() {
    let Some(analysis) = latex("\\documentclass{article}\n\\begin{document}\nx \\label{own}\n\\end{document}\n") else {
        return;
    };
    let found = placed(&analysis, "unused-label");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("3:3 ") && !found[0].contains("via"), "{found:?}");
}

#[test]
fn a_duplicate_label_from_two_calls_is_reported_at_the_second() {
    let Some(analysis) = latex(&format!("{GREET}\\greet{{x}}\n\\greet{{x}}\n\\end{{document}}\n")) else { return };
    let found = placed(&analysis, "duplicate-label");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("5:1 ") && found[0].contains("via \\greet"), "{found:?}");
}

/// A call that writes two labels, made twice: one finding at the second
/// call, naming both labels, as pdflatex warns about each.
#[test]
fn every_label_a_repeated_call_defines_again_is_named() {
    let source = "\\documentclass{article}\n\\newcommand\\two[1]{\\section{#1}\\label{a:#1}\\label{b:#1}}\n\
                  \\begin{document}\n\\two{x}\n\\two{x}\n\\end{document}\n";
    let Some(analysis) = latex(source) else { return };
    let found = placed(&analysis, "duplicate-label");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("5:1 ") && found[0].contains("`a:x`") && found[0].contains("`b:x`"), "{found:?}");
}

/// A reference ahead of its label (a figure further down) is defined:
/// pdflatex resolves it on its second run.
#[test]
fn a_reference_ahead_of_its_label_is_defined() {
    let source = "\\documentclass{article}\n\\usepackage{hyperref}\n\\begin{document}\nSee \\ref{fig:a}.\n\
                  \\begin{figure}\nX\\caption{C}\n\\label{fig:a}\n\\end{figure}\n\\end{document}\n";
    let Some(analysis) = latex(source) else { return };
    let found = placed(&analysis, "undefined-reference");
    assert!(found.is_empty(), "{found:?}");
}

/// A reference leaves `\r@⟨key⟩` `\relax` when it is not defined yet
/// (nameref's `\NR@@setref`): that declares no label, so a `\label` read
/// before it from an input file still defines the key, as pdflatex finds on
/// its second run.
#[test]
fn a_label_in_an_input_file_defines_the_key_a_later_reference_names() {
    if which::which("kpsewhich").is_err() {
        return;
    }
    let dir = crate::support::scratch_dir(crate::support::tmp_root().join("satex-chktex-ref-ahead"));
    std::fs::write(dir.join("part.tex"), "Text\\label{sec:a}\n").unwrap();
    let source = "\\documentclass{article}\n\\usepackage{hyperref}\n\\begin{document}\n\\input{part}\n\
                  \\ref{sec:a}\n\\end{document}\n";
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(source, Some(&path), &cfg);
    let _ = std::fs::remove_dir_all(&dir);
    let found = placed(&analysis, "undefined-reference");
    assert!(found.is_empty(), "{found:?}");
    let label = |o: &&satex::facts::Occurrence| o.kind == satex::builtins::OccKind::Label && o.key == "sec:a";
    assert_eq!(analysis.facts.occurrences.iter().filter(label).count(), 1);
}

/// An error in a call's argument (a beamer frame's body) stands at the
/// command that raised it, not at the call: pdflatex names the end of the
/// argument, where its reader is.
#[test]
fn an_error_in_an_argument_stands_at_its_command() {
    let source = "\\documentclass{article}\n\\newcommand\\wrap[1]{#1}\n\\begin{document}\n\\wrap{\n  Text\n  \\begin{nosuch}\n  y}\n\\end{document}\n";
    let Some(analysis) = latex(source) else { return };
    let found = placed(&analysis, "undefined-environment");
    assert!(found.len() == 1 && found[0].starts_with("6:3 "), "{found:?}");
}
