//! The list rules: an `itemize` that counts its items by hand is an
//! `enumerate`, one labelled with words a `description`; the fixes keep the
//! look with enumitem keys where the default labels differ.

use crate::support::installed;
use satex::analysis::machine::Machine;
use satex::project::config::{Config, Load};

/// `code: message` and the fixed document, for every list finding on a document with `body`.
fn found(name: &str, preamble: &str, body: &str) -> Vec<(String, String)> {
    let directory = crate::support::tmp_root().join("satex-lists").join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    let document = format!("\\documentclass{{article}}\n{preamble}\\begin{{document}}\n{body}\n\\end{{document}}\n");
    let main = directory.join("main.tex");
    std::fs::write(&main, &document).unwrap();
    let cfg = Config { load: Load { classes: true, ..Default::default() }, ..crate::support::config() };
    let analysis = Machine::analyze(&document, Some(&main), &cfg);
    let out = satex::linter::lint(&analysis)
        .into_iter()
        .filter(|r| r["code"].as_str().is_some_and(|c| c.starts_with("itemize-")))
        .map(|r| {
            let edits: Vec<satex::text::Edit> =
                r["edits"].as_array().into_iter().flatten().filter_map(satex::text::Edit::from_json).collect();
            let fixed = apply(&document, edits, &main.to_string_lossy());
            (format!("{}: {}", r["code"].as_str().unwrap(), r["message"].as_str().unwrap()), fixed)
        })
        .collect();
    let _ = std::fs::remove_dir_all(&directory);
    out
}

/// `document` with the `edits` to file `path`, the last first so earlier offsets hold.
fn apply(document: &str, mut edits: Vec<satex::text::Edit>, path: &str) -> String {
    let text = satex::text::Text::new(document.to_string());
    edits.retain(|e| std::path::Path::new(&e.path).file_name() == std::path::Path::new(path).file_name());
    edits.sort_by_key(|e| std::cmp::Reverse((e.start.line, e.start.col)));
    let mut out = document.to_string();
    for e in edits {
        let (start, end) = (text.offset(e.start).unwrap(), text.offset(e.end).unwrap());
        out.replace_range(start..end, &e.replacement);
    }
    out
}

/// The body of a fixed document, between `\begin{document}` and `\end{document}`.
fn body(fixed: &str) -> &str {
    let start = fixed.find("\\begin{document}\n").unwrap() + "\\begin{document}\n".len();
    fixed[start..fixed.rfind("\n\\end{document}").unwrap()].trim_end()
}

/// `1.`, `2.` by hand is enumerate's own top-level label: plain `enumerate`, safe.
#[test]
fn an_itemize_numbered_by_hand_becomes_an_enumerate() {
    if !installed() {
        return;
    }
    let found = found("plain", "", "\\begin{itemize}\n\\item[1.] One\n\\item[2.]Two\n\\end{itemize}");
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "itemize-numbered: this `itemize` counts its items by hand: use `enumerate`");
    assert_eq!(body(&found[0].1), "\\begin{enumerate}\n\\item One\n\\item Two\n\\end{enumerate}");
}

/// Another shape and a later start keep their look with enumitem's keys,
/// and enumitem is loaded for them.
#[test]
fn other_labels_keep_their_shape_with_enumitem() {
    if !installed() {
        return;
    }
    let found = found("shape", "", "\\begin{itemize}\n\\item[(c)] One\n\\item[(d)] Two\n\\end{itemize}");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].0.ends_with("use `enumerate` with enumitem's `[label=(\\alph*), start=3]`"), "{found:?}");
    assert!(found[0].1.contains("\\usepackage{enumitem}"), "{}", found[0].1);
    assert!(
        body(&found[0].1).starts_with("\\begin{enumerate}[label=(\\alph*), start=3]\n\\item One"),
        "{}",
        found[0].1
    );
}

/// A count that goes on from the enumerate before it resumes that list.
#[test]
fn a_count_that_goes_on_resumes() {
    if !installed() {
        return;
    }
    let found = found(
        "resume",
        "\\usepackage{enumitem}\n",
        "\\begin{enumerate}\n\\item A\n\\item B\n\\end{enumerate}\nText.\n\\begin{itemize}\n\\item[3.] C\n\\item[4.] D\n\\end{itemize}",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].0.ends_with("with enumitem's `[resume]`"), "{found:?}");
}

/// Inside an enumerate the default labels are `(a)`, `(b)`: no keys needed.
#[test]
fn a_nested_list_counts_at_its_depth() {
    if !installed() {
        return;
    }
    let found = found(
        "nested",
        "",
        "\\begin{enumerate}\n\\item Outer\n\\begin{itemize}\n\\item[(a)] x\n\\item[(b)] y\n\\end{itemize}\n\\end{enumerate}",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "itemize-numbered: this `itemize` counts its items by hand: use `enumerate`");
}

/// Labels of words are a description; symbols, gaps and unlabelled items are fine.
#[test]
fn word_labels_make_a_description_and_symbols_stay() {
    if !installed() {
        return;
    }
    let found = found(
        "words",
        "",
        "\\begin{itemize}\n\\item[\\textbf{Input:}] a file\n\\item[Output] a report\n\\end{itemize}",
    );
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].0, "itemize-described: every item of this `itemize` is labelled with words: use `description`");
    assert!(body(&found[0].1).starts_with("\\begin{description}\n\\item[\\textbf{Input:}] a file"), "{}", found[0].1);
    for (name, list) in [
        ("symbols", "\\begin{itemize}\n\\item[$\\star$] a\n\\item[--] b\n\\end{itemize}"),
        ("gap", "\\begin{itemize}\n\\item[1.] a\n\\item[3.] b\n\\end{itemize}"),
        ("some", "\\begin{itemize}\n\\item[1.] a\n\\item b\n\\end{itemize}"),
        ("short", "\\begin{itemize}\n\\item[Pro] a\n\\item[Con] b\n\\end{itemize}"),
    ] {
        let found = self::found(name, "", list);
        assert!(found.is_empty(), "{name}: {found:?}");
    }
}
