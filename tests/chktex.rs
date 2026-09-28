//! satex's coverage of chktex's warnings; see doc/chktex-coverage.md for the
//! full mapping.  Both rules exercised here work at the macro-expansion
//! level, so no TeX installation is needed to run them.

use satex::config::Config;
use satex::machine::{Analysis, Machine};

fn analyze(source: &str) -> Analysis {
    let cfg = Config {
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        load_format: false,
        use_kpsewhich: false,
        ..Config::default()
    };
    Machine::analyze(source, None, &cfg)
}

fn findings(analysis: &Analysis, code: &str) -> Vec<String> {
    satex::lint::lint(analysis)
        .into_iter()
        .filter(|record| record["code"] == code)
        .filter_map(|record| record["message"].as_str().map(str::to_string))
        .collect()
}

// chktex warning 9: `'%s' expected, found '%s'`.
/// `\end{…}` is latex.ltx's, so these run the kernel: its `\@checkend`
/// raises `\begin{x} on input line n ended by \end{y}`.
fn latex(source: &str) -> Option<Analysis> {
    which::which("kpsewhich").ok()?;
    let cfg = Config { load_classes: true, ..Config::default() };
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

#[test]
fn properly_nested_environments_are_not_reported() {
    let analysis = analyze(r"\begin{document}\begin{itemize}\item one\end{itemize}\end{document}");
    assert!(findings(&analysis, "environment-mismatch").is_empty());
}

// chktex warning 41: `You ought to not use primitive TeX in LaTeX code.`
#[test]
fn plain_tex_primitives_with_a_latex_alternative_are_reported() {
    let analysis = analyze(r"\hskip 1pt \vskip 1pt");
    let found = findings(&analysis, "primitive-tex-command");
    assert!(found.iter().any(|m| m.contains("\\hskip")), "{found:?}");
    assert!(found.iter().any(|m| m.contains("\\vskip")), "{found:?}");
}

/// `\catcode` has no LaTeX interface at all (it is not in the built-in
/// table), so a finding for it could only say so and give nothing to act
/// on: satex reports none.  `\expandafter` is the same story.
#[test]
fn plain_tex_primitives_with_no_latex_alternative_are_not_reported() {
    let analysis = analyze(r"\catcode`\@=11 \expandafter\relax\expandafter\relax\relax");
    assert!(findings(&analysis, "primitive-tex-command").is_empty());
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
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        load_format: false,
        use_kpsewhich: false,
        latex_alternatives: std::collections::BTreeMap::from([("expandafter".to_string(), String::new())]),
        ..Config::default()
    };
    let analysis = Machine::analyze(r"\expandafter\relax\relax", None, &cfg);
    let found = findings(&analysis, "primitive-tex-command");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("\\expandafter"), "{found:?}");
}

#[test]
fn a_latex_style_document_is_not_reported() {
    let analysis = analyze(r"\newcommand{\greeting}{hello}\greeting");
    assert!(findings(&analysis, "primitive-tex-command").is_empty());
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
fn a_second_italic_correction_with_nothing_between_is_reported() {
    let analysis = analyze(r"word\/\/ more");
    let found = findings(&analysis, "stray-italic-correction");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("nothing was set"), "{found:?}");
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

#[test]
fn an_italic_correction_followed_by_a_letter_is_not_reported() {
    let analysis = analyze(r"word\/ more\/ still");
    assert!(findings(&analysis, "stray-italic-correction").is_empty());
}

// chktex warning 7: `Accent command 'command' needs use of 'command'`.
/// The accent rules read the source to see how far the accent's argument
/// reaches, so these run against a file on disk.
fn latex_file(name: &str, source: &str) -> Option<Analysis> {
    which::which("kpsewhich").ok()?;
    let dir = std::env::temp_dir().join(format!("satex-chktex-{}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join(name);
    std::fs::write(&path, source).ok()?;
    let cfg = Config { load_classes: true, ..Config::default() };
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

#[test]
fn a_text_accent_the_encoding_composes_is_not_reported() {
    let Some(analysis) = latex_file(
        "composed.tex",
        "\\documentclass{article}\n\\begin{document}\nna\\'ive and \\'{\\i}dem and \\^{}i\n\\end{document}\n",
    ) else {
        return;
    };
    assert!(findings(&analysis, "accent-on-dotted-letter").is_empty());
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

#[test]
fn a_math_accent_on_the_dotless_letter_is_not_reported() {
    let Some(analysis) = latex_file(
        "dotless.tex",
        "\\documentclass{article}\n\\begin{document}\n$\\hat\\imath$ and $\\bar x$\n\\end{document}\n",
    ) else {
        return;
    };
    assert!(findings(&analysis, "accent-on-dotted-letter").is_empty());
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
    let dir = std::env::temp_dir().join(format!("satex-chktex-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).ok()?;
    let path = dir.join("doc.tex");
    std::fs::write(&path, source).ok()?;
    let cfg = match kernel {
        true => Config { load_classes: true, ..Config::default() },
        false => Config {
            load_packages: false,
            load_classes: false,
            load_inputs: false,
            load_format: false,
            use_kpsewhich: false,
            ..Config::default()
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

#[test]
fn brackets_that_match_are_not_reported() {
    let doc = on_disk("match", "\\begin{document}\nBalanced (ok) and [ok].\n\\end{document}\n", false)
        .expect("temporary directory");
    assert!(findings(&doc.analysis, "unbalanced-brackets").is_empty());
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

// chktex warnings 20 and 44: `User-specified pattern found: %s` and
// `User Regex: %s`.
/// Both lists are empty until `satex.yaml` carries them, so the rule is
/// silent here; `lint::sourcetext`'s own tests drive the matcher with a list.
#[test]
fn no_user_pattern_is_configured_by_default() {
    let doc = on_disk("patterns", "\\begin{document}\nChkTeX is spelled Chk\\TeX.\n\\end{document}\n", false)
        .expect("temporary directory");
    assert!(findings(&doc.analysis, "user-pattern").is_empty());
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

#[test]
fn a_plain_period_after_a_command_is_not_reported() {
    let doc =
        on_disk("period", "\\begin{document}\nA \\TeX. Right?\n\\end{document}\n", false).expect("temporary directory");
    assert!(findings(&doc.analysis, "unintended-escape").is_empty());
}

/// `\,` is a thin space in math, where chktex reports it all the same.
#[test]
fn an_escaped_mark_in_math_is_not_reported() {
    let doc = on_disk("mathmark", "\\begin{document}\n$\\alpha\\, dx$\n\\end{document}\n", false)
        .expect("temporary directory");
    assert!(findings(&doc.analysis, "unintended-escape").is_empty());
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

#[test]
fn a_comment_without_a_marker_is_not_reported() {
    let doc = on_disk("remark", "\\begin{document}\nHello. % a remark about TODOS\n\\end{document}\n", false)
        .expect("temporary directory");
    assert!(findings(&doc.analysis, "comment-marker").is_empty());
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
fn an_acute_accent_for_an_apostrophe_is_reported() {
    let analysis = analyze("It is Andre\u{b4}s book.");
    let found = findings(&analysis, "wrong-quote-character");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("acute accent"), "{found:?}");
}

#[test]
fn proper_quotation_marks_are_not_reported() {
    let analysis = analyze("He said ``hello'' to me.");
    assert!(findings(&analysis, "wrong-quote-character").is_empty());
}

// chktex warnings 32, 33 and 34: a mark that points the wrong way.
#[test]
fn a_quotation_opened_with_an_apostrophe_is_reported() {
    let analysis = analyze("He said 'hello' to me.");
    let found = findings(&analysis, "quote-direction");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("opens a quotation"), "{found:?}");
}

#[test]
fn a_quotation_closed_with_a_backtick_is_reported() {
    let analysis = analyze("He said ``hello` to me.");
    let found = findings(&analysis, "quote-direction");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("a double closing quote belongs"), "{found:?}");
}

#[test]
fn a_run_mixing_both_marks_is_reported() {
    let analysis = analyze("A `' here.");
    let found = findings(&analysis, "quote-direction");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("empty quotation"), "{found:?}");
}

#[test]
fn apostrophes_and_elisions_are_not_reported() {
    let analysis = analyze("Don't stop the boys' rock 'n' roll of the '90s.");
    assert!(findings(&analysis, "quote-direction").is_empty());
}

#[test]
fn a_prime_in_math_mode_is_not_reported() {
    let Some(analysis) = latex(r"\documentclass{article}\begin{document}$f'(x)$ and $a''$\end{document}") else {
        return;
    };
    assert!(findings(&analysis, "quote-direction").is_empty());
    assert!(findings(&analysis, "csquotes-available").is_empty());
}

// chktex warning 23: three quotation marks in a row.
#[test]
fn three_quotation_marks_in_a_row_are_reported() {
    let analysis = analyze("A ``` run of three.");
    let found = findings(&analysis, "quote-ligature");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("nothing between them"), "{found:?}");
}

#[test]
fn marks_the_group_kept_apart_are_not_reported() {
    let Some(analysis) = latex(r"\documentclass{article}\begin{document}A ``{}` run.\end{document}") else { return };
    assert!(findings(&analysis, "quote-ligature").is_empty());
}

// chktex warning 38: punctuation inside the closing quotation mark.
#[test]
fn punctuation_inside_a_quotation_is_reported() {
    let analysis = analyze("He said ``hello.'' and left.");
    let found = findings(&analysis, "quote-punctuation");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("stands inside the quotation"), "{found:?}");
}

#[test]
fn punctuation_after_a_quotation_is_not_reported() {
    let analysis = analyze("He said ``hello''. Then he left.");
    assert!(findings(&analysis, "quote-punctuation").is_empty());
}

// csquotes covers all of the above at once.
#[test]
fn hand_set_quotation_marks_suggest_csquotes() {
    let analysis = analyze("He said ``hello'' and ``goodbye'' to me.");
    let found = findings(&analysis, "csquotes-available");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("4 quotation marks"), "{found:?}");
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

#[test]
fn latex_math_delimiters_are_not_reported() {
    let Some(doc) = on_disk("delimiters", &math_document(r"Display \[a+b\] and inline \(c+d\) here."), true) else {
        return;
    };
    assert!(findings(&doc.analysis, "math-shift-dollars").is_empty());
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

/// `\left … \right` is one atom, so the script takes all of it; chktex
/// reports this one.
#[test]
fn a_script_after_right_is_not_reported() {
    let Some(doc) = on_disk("right", &math_document(r"Math $\left(a+b\right)^2$ and $x^{12}$ here."), true) else {
        return;
    };
    assert!(findings(&doc.analysis, "math-script-braces").is_empty());
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

#[test]
fn closed_math_is_not_reported() {
    let Some(doc) = on_disk("closed", &math_document(r"Text $x + y$ here."), true) else {
        return;
    };
    assert!(findings(&doc.analysis, "math-still-open").is_empty());
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

#[test]
fn paired_math_delimiters_are_not_reported() {
    let Some(doc) = on_disk("paired", &math_document(r"Text \(a\) and \[b\] here."), true) else {
        return;
    };
    assert!(findings(&doc.analysis, "math-mode-mismatch").is_empty());
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

/// chktex reports `\mathrm{sin}`, which is already upright.
#[test]
fn an_operator_set_by_its_command_is_not_reported() {
    let Some(doc) = on_disk("upright", &math_document(r"Write $\sin(x)$ and $\mathrm{sin}(x)$ here."), true) else {
        return;
    };
    assert!(findings(&doc.analysis, "math-operator-name").is_empty());
}

#[test]
fn letters_with_no_command_behind_them_are_not_reported() {
    let Some(doc) = on_disk("letters", &math_document(r"Write $abc$ here."), true) else {
        return;
    };
    assert!(findings(&doc.analysis, "math-operator-name").is_empty());
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

#[test]
fn an_ellipsis_command_is_not_reported() {
    let Some(doc) = on_disk("ldots", &math_document(r"Text \dots{} here and $x_1, \ldots, x_n$ there."), true) else {
        return;
    };
    assert!(findings(&doc.analysis, "ellipsis-dots").is_empty());
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

#[test]
fn an_x_beside_letters_is_not_reported() {
    let Some(doc) = on_disk("letter-x", &math_document(r"Screen 640 by 480 and ax4 and 4xa here."), true) else {
        return;
    };
    assert!(findings(&doc.analysis, "dimension-times").is_empty());
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

#[test]
fn punctuation_on_the_right_side_is_not_reported() {
    let Some(doc) = on_disk("inside", &math_document("Inline $a+b$, here.\n\\[ c+d. \\]"), true) else {
        return;
    };
    assert!(findings(&doc.analysis, "math-punctuation").is_empty());
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

/// The space in front of it was read in vertical mode, so it set nothing.
#[test]
fn a_reference_that_opens_a_paragraph_is_not_reported() {
    let source =
        "\\documentclass{article}\n\\begin{document}\n\\label{a}\n\\ref{a} opens a paragraph.\n\\end{document}\n";
    let Some(doc) = on_disk("vertical-tie", source, true) else { return };
    assert!(findings(&doc.analysis, "missing-nonbreaking-space").is_empty());
    assert!(findings(&doc.analysis, "space-before-command").is_empty());
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

// chktex warning 13: `Intersentence spacing ('\@') should perhaps be used.`
#[test]
fn a_period_after_capitals_before_a_capital_is_reported() {
    let analysis = analyze(r"\begin{document}The ABC. Next one here.\end{document}");
    let found = findings(&analysis, "sentence-spacing");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("ABC") && found[0].contains("a sentence ends"), "{found:?}");
}

#[test]
fn a_correctly_spaced_sentence_is_not_reported() {
    let analysis = analyze(r"\begin{document}One thing here. Another thing there. A. Smith wrote it.\end{document}");
    assert!(findings(&analysis, "sentence-spacing").is_empty());
}

// chktex warning 26: `You ought to remove spaces in front of punctuation.`
#[test]
fn a_space_in_front_of_punctuation_is_reported() {
    let analysis = analyze(r"\begin{document}one , two ; three here.\end{document}");
    let found = findings(&analysis, "space-before-punctuation");
    assert_eq!(found.len(), 2, "{found:?}");
}

#[test]
fn punctuation_against_its_word_is_not_reported() {
    let analysis = analyze(r"\begin{document}one, two; three here.\end{document}");
    assert!(findings(&analysis, "space-before-punctuation").is_empty());
}

// chktex warning 30: `Multiple spaces detected in input.`  TeX reads a run
// of spaces as one token, so only one of them reached the page.
#[test]
fn spaces_the_run_collapsed_are_not_reported() {
    let analysis = analyze("\\begin{document}one  two   three here.\\end{document}");
    assert!(findings(&analysis, "double-space").is_empty());
}

// chktex warnings 36 and 37: the space around a parenthesis.
#[test]
fn spaces_inside_parentheses_are_reported() {
    let analysis = analyze(r"\begin{document}one ( two ) three.\end{document}");
    let found = findings(&analysis, "parenthesis-spacing");
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(found.iter().all(|f| f.contains("inside the parentheses")), "{found:?}");
}

/// Math needs the real catcode of `$`, so this one runs the kernel.
#[test]
fn a_parenthesis_in_math_is_not_reported() {
    let source = "\\documentclass{article}\n\\begin{document}\nThe value $f (x )$ is one.\n\\end{document}\n";
    let Some(doc) = on_disk("math-paren", source, true) else { return };
    assert!(findings(&doc.analysis, "parenthesis-spacing").is_empty());
}

#[test]
fn a_parenthesis_spaced_correctly_is_not_reported() {
    let analysis = analyze(r"\begin{document}The value (two) is one.\end{document}");
    assert!(findings(&analysis, "parenthesis-spacing").is_empty());
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

/// chktex reports this one, because `\textit` is not on its italic list.
#[test]
fn an_italic_correction_the_font_really_has_is_not_reported() {
    let Some(analysis) = latex_file(
        "wanted.tex",
        "\\documentclass{article}\n\\begin{document}\n\\textit{word\\/} after and \\emph{more\\/} here\n\\end{document}\n",
    ) else {
        return;
    };
    assert!(findings(&analysis, "zero-italic-correction").is_empty());
}

// chktex warning 6: leaving a slanted shape with no italic correction.
#[test]
fn leaving_a_slanted_shape_without_a_correction_is_reported() {
    let Some(analysis) = latex_file(
        "overhang.tex",
        "\\documentclass{article}\n\\begin{document}\nThe \\emph{f} before roman text\n\\end{document}\n",
    ) else {
        return;
    };
    let found = findings(&analysis, "missing-italic-correction");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].contains("cmti10") && found[0].contains("`f`"), "{found:?}");
}

#[test]
fn a_slanted_shape_that_ends_with_a_correction_is_not_reported() {
    let Some(analysis) = latex_file(
        "corrected.tex",
        "\\documentclass{article}\n\\begin{document}\nThe \\emph{f\\/} before roman text\n\\end{document}\n",
    ) else {
        return;
    };
    assert!(findings(&analysis, "missing-italic-correction").is_empty());
}

/// A correction in front of a period or a comma is chktex 28's business.
#[test]
fn a_slanted_shape_before_small_punctuation_is_not_reported() {
    let Some(analysis) = latex_file(
        "punctuated.tex",
        "\\documentclass{article}\n\\begin{document}\nThe \\emph{word}, and \\emph{another}.\n\\end{document}\n",
    ) else {
        return;
    };
    assert!(findings(&analysis, "missing-italic-correction").is_empty());
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
