//! Every prose rule against the contexts that are not prose.  Each rule's
//! trigger (`lint_triggers.txt`) is set once per context: in plain prose and
//! in a footnote it must be found, wrapped in math, code, a key, a URL or a
//! comment it must not, and neither where the word the rule decides by (`@`)
//! is a formula, code or a key.  One document per context holds every
//! trigger as a paragraph of its own, so a finding belongs to the trigger
//! whose line it stands on.  `scripts/lint-sweep.py` runs the same table
//! through the CLI and prints it per rule.

use satex::analysis::machine::Machine;
use satex::project::config::{Config, Load};

/// The rule, its trigger with `@` for the word it decides by, and that word.
fn triggers() -> Vec<(&'static str, &'static str, &'static str)> {
    include_str!("lint_triggers.txt")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut cells = line.split('\t');
            (cells.next().unwrap(), cells.next().unwrap_or(""), cells.next().unwrap_or(""))
        })
        .collect()
}

/// Rules that read formulas, a hand-set quantity (worst in math) and a dash
/// (minus signs in math): inside math they are right to speak.
const MATH_RULES: &[&str] =
    &["math-operator-name", "math-punctuation", "dimension-times", "ellipsis-dots", "hand-set-quantity", "wrong-dash"];
/// Rules that measure the font: an italic correction or an accent on a
/// dotted letter is wrong in math and in typewriter type too.
const FONT_RULES: &[&str] =
    &["accent-on-dotted-letter", "missing-italic-correction", "stray-italic-correction", "zero-italic-correction"];
/// Triggers that are a formula themselves: inside `\texttt` still math.
const FORMULA: &[&str] = &["math-operator-name", "math-punctuation"];
/// A filler that is itself the mistake (`\cref` at a sentence start, a
/// `\ref` with a space in front), or a number a range rightly reads.
const INTENDED: &[(&str, &str)] = &[
    ("sentence-capital", "w-cref"),
    ("missing-nonbreaking-space", "w-ref"),
    ("missing-nonbreaking-space", "w-cite"),
    ("wrong-dash", "w-ref"),
    ("wrong-dash", "w-cref"),
];

/// A context: its name and how it sets a trigger `<T>`.  A name starting
/// with `w-` replaces the trigger's `@` word instead.
type Context = (&'static str, &'static str);

/// Whether `rule` may speak in `context` about the trigger of `trigger`.
fn allowed(rule: &str, trigger: &str, context: &str) -> bool {
    MATH_RULES.contains(&rule) && matches!(context, "math" | "display")
        || FONT_RULES.contains(&rule) && matches!(context, "math" | "display" | "texttt")
        || rule == trigger && FORMULA.contains(&rule) && context == "texttt"
}

/// The document setting every trigger in `context`, and the lines each
/// trigger's paragraph covers.
fn document(context: Context, cases: &[(&str, &str, &str)]) -> (String, Vec<(u64, u64)>) {
    let mut text = "\\documentclass{article}\n\\usepackage[english]{babel}\n\\usepackage{listings}\n\
                    \\usepackage{url}\n\\usepackage{cleveref}\n\\begin{document}\n\\section{One}\\label{sec:a}\n"
        .to_string();
    let mut blocks = Vec::new();
    for (_, template, word) in cases {
        let body = match context.0.strip_prefix("w-") {
            Some(_) => template.replace('@', context.1),
            None => {
                let filled = template.replace('@', word);
                // `\verb` takes a delimiter the trigger does not hold.
                let wrap = match filled.contains('|') {
                    true => context.1.replace('|', "!"),
                    false => context.1.to_string(),
                };
                wrap.replace("<T>", &filled)
            }
        };
        let start = text.lines().count() as u64 + 1;
        blocks.push((start, start + body.matches('\n').count() as u64));
        text.push_str(&body);
        text.push_str("\n\n");
    }
    text.push_str("\\end{document}\n");
    (text, blocks)
}

/// Runs every trigger through each context and checks what the prose rules
/// found: `must` contexts need the trigger's own rule on its paragraph,
/// the others no prose rule at all (beside a filler, not the trigger's).
fn check(contexts: &[Context], must: bool) {
    if satex::project::paths::which("kpsewhich").is_err() {
        return;
    }
    let all = triggers();
    let prose: Vec<&str> = all.iter().map(|t| t.0).collect();
    crate::support::table(
        contexts,
        |c| c.0,
        |&context| {
            let beside = context.0.starts_with("w-");
            let cases: Vec<_> = all.iter().copied().filter(|t| !beside || t.1.contains('@')).collect();
            let (source, blocks) = document(context, &cases);
            let dir = crate::support::tmp_root().join(format!("satex-contexts-{}-{}", context.0, std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).ok()?;
            let path = dir.join("doc.tex");
            std::fs::write(&path, &source).ok()?;
            let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
            let analysis = Machine::analyze(&source, Some(&path), &cfg);
            let records = satex::linter::lint(&analysis);
            let _ = std::fs::remove_dir_all(&dir);
            let mut wrong = Vec::new();
            for (&(rule, template, _), &(first, last)) in cases.iter().zip(&blocks) {
                let here: Vec<(&str, String)> = records
                    .iter()
                    .filter(|r| r["path"].as_str().is_some_and(|p| p.ends_with("doc.tex")))
                    .filter(|r| r["line"].as_u64().is_some_and(|l| (first..=last).contains(&l)))
                    .filter_map(|r| Some((r["code"].as_str()?, r["message"].as_str()?.to_string())))
                    .filter(|(code, _)| prose.contains(code))
                    .collect();
                if must {
                    if !here.iter().any(|(code, _)| *code == rule) {
                        wrong.push(format!("{rule} missed `{template}`"));
                    }
                    continue;
                }
                for (code, message) in here {
                    let filler = beside && (code != rule || INTENDED.contains(&(rule, context.0)));
                    if !filler && !allowed(code, rule, context.0) {
                        wrong.push(format!("{code} on `{template}`: {message}"));
                    }
                }
            }
            (!wrong.is_empty()).then(|| wrong.join("; "))
        },
    );
}

/// The baseline: each trigger is a mistake in prose.  A rule that misses
/// it here makes the silent rows below prove nothing.
#[test]
fn prose_rules_find_their_trigger_in_prose() {
    check(&[("plain", "<T>")], true);
}

/// A footnote is prose too, read as a class macro's argument.  Pins that
/// the facts reach into it: a `\ref` there records no reference
/// occurrence (so `bare-reference` misses it and `unused-label` reports a
/// label only footnotes use), and the accent occurrence there is missed.
#[test]
fn prose_rules_find_their_trigger_in_footnotes() {
    check(&[("footnote", "Text.\\footnote{<T>}")], true);
}

/// Math is no prose: `$x_{42}$` after `?` starts no sentence (the bug that
/// started this table), and `etc..` in a formula is no doubled period.
#[test]
fn prose_rules_stay_silent_in_math() {
    check(&[("math", "$<T>$"), ("display", "\\[<T>\\]"), ("w-math", "$x_{42}$"), ("w-paren", "\\(x\\)")], false);
}

/// Code shown as text, verbatim or in the typewriter family, is no prose:
/// `\texttt{a , b}` is a program's spacing, not a slip.
#[test]
fn prose_rules_stay_silent_in_code() {
    check(
        &[
            ("verb", "\\verb|<T>|"),
            ("texttt", "\\texttt{<T>}"),
            ("listing", "\\begin{lstlisting}\n<T>\n\\end{lstlisting}"),
            ("verbatim", "\\begin{verbatim}\n<T>\n\\end{verbatim}"),
            ("w-verb", "\\verb|x|"),
            ("w-texttt", "\\texttt{x}"),
        ],
        false,
    );
}

/// Keys and URLs are names, not words: `\label{a , b}` sets nothing, and
/// `\url` sets its text in typewriter type.
#[test]
fn prose_rules_stay_silent_in_keys_and_urls() {
    check(
        &[
            ("url", "\\url{<T>}"),
            ("label", "\\label{<T>}"),
            ("cite", "\\cite{<T>}"),
            ("cref", "\\cref{<T>}"),
            ("w-url", "\\url{http://x.org}"),
            ("w-ref", "\\ref{sec:a}"),
            ("w-cite", "\\cite{knuth}"),
            ("w-cref", "\\cref{sec:a}"),
        ],
        false,
    );
}

/// A comment sets nothing, so no prose rule reads it.
#[test]
fn prose_rules_stay_silent_in_comments() {
    check(&[("comment", "% <T>")], false);
}
