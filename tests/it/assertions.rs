//! `%\satex@assert`, `%\satex@assume` and `%\satex@configure`: property
//! checks and local settings written in the document's comments.

use crate::support::{PRELUDE, bare, installed, names};
use satex::machine::{Analysis, Machine};

/// With proved assertions reported, which the default leaves out.
fn analyze(source: &str) -> Analysis {
    let mut cfg = bare();
    cfg.lint_off.retain(|code| code != "assertion-proved");
    Machine::analyze(&format!("{PRELUDE}{source}"), None, &cfg)
}

/// The directive findings, as `code: message`.
fn outcomes(analysis: &Analysis) -> Vec<String> {
    let codes = ["assertion-refuted", "assertion-unknown", "assertion-proved", "assumption-made", "invalid-directive"];
    satex::lint::lint(analysis)
        .iter()
        .filter_map(|f| {
            let code = f.get("code")?.as_str()?;
            let message = f.get("message")?.as_str()?;
            codes.contains(&code).then(|| format!("{code}: {message}"))
        })
        .collect()
}

fn only(analysis: &Analysis, code: &str) -> String {
    let found = outcomes(analysis);
    match found.as_slice() {
        [one] if one.starts_with(&format!("{code}:")) => one.clone(),
        _ => panic!("want one {code}, got {found:?}"),
    }
}

/// A register set to a known value: the assertion holds on every path.
#[test]
fn an_assertion_on_a_known_value_is_proved() {
    let analysis = analyze("\\countdef\\x=10 \\x=3\n%\\satex@assert{\\x = 3 and \\x <= 3}\n\\relax\n");
    only(&analysis, "assertion-proved");
}

/// False on an exact path: an error naming what the operand is there.
#[test]
fn a_false_assertion_is_refuted_with_its_witness() {
    let analysis = analyze("\\countdef\\x=10 \\x=7\n% \\satex@assert{\\x < 5}\n\\relax\n");
    let found = only(&analysis, "assertion-refuted");
    assert!(found.contains("`\\x` is 7"), "{found}");
}

/// A value the run does not know: neither proved nor refuted.
#[test]
fn an_assertion_on_an_unknown_value_is_unknown() {
    let analysis = analyze("\\countdef\\x=10 \\x=\\pdfuniformdeviate 10\n%\\satex@assert{\\x < 5}\n\\relax\n");
    let found = only(&analysis, "assertion-unknown");
    assert!(found.contains("in [0, 9]"), "{found}");
}

/// The assumption narrows the register, so the assertion after it holds.
#[test]
fn an_assumption_narrows_what_follows() {
    let analysis = analyze(
        "\\countdef\\x=10 \\x=\\pdfuniformdeviate 10\n%\\satex@assume{\\x < 5}\n%\\satex@assert{\\x <= 4}\n\\relax\n",
    );
    let found = outcomes(&analysis);
    assert!(found.iter().any(|f| f.starts_with("assumption-made")), "{found:?}");
    assert!(found.iter().any(|f| f.starts_with("assertion-proved")), "{found:?}");
}

/// An assumption false on the path drops it: nothing after it runs there.
#[test]
fn a_false_assumption_drops_the_path() {
    let analysis = analyze("\\countdef\\x=10 \\x=1\n%\\satex@assume{\\x = 2}\n\\def\\after{}\n");
    assert!(!names(&analysis).contains(&"after".to_string()));
}

/// A setting changed in a group holds until the group ends.
#[test]
fn configure_is_scoped_to_its_group() {
    let analysis = analyze(
        "\\let\\iffoo\\iftrue\n\
         {%\\satex@configure{opaque_conditionals=[iffoo]}\n\
         \\iffoo\\def\\a{}\\else\\def\\b{}\\fi}\n\
         \\iffoo\\def\\c{}\\else\\def\\d{}\\fi\n",
    );
    let defined = names(&analysis);
    for (name, want) in [("a", true), ("b", true), ("c", true), ("d", false)] {
        assert_eq!(defined.contains(&name.to_string()), want, "{name}: {defined:?}");
    }
}

/// An unknown key names the keys of its section.
#[test]
fn configure_with_an_unknown_key_lists_the_keys() {
    let analysis = analyze("%\\satex@configure{limits.nope=1}\n\\relax\n");
    let found = only(&analysis, "invalid-directive");
    assert!(found.contains("limits.site_forks"), "{found}");
}

/// Text a conditional skips is never read: its assertion is not checked.
#[test]
fn an_assertion_in_skipped_text_is_not_reached() {
    let analysis = analyze("\\iffalse\n%\\satex@assert{1 = 2}\n\\fi\n\\relax\n");
    let found = only(&analysis, "assertion-unknown");
    assert!(found.contains("no path reads"), "{found}");
}

/// A TeX conditional is a condition too.
#[test]
fn a_conditional_is_a_condition() {
    let analysis = analyze("\\def\\x{}\n%\\satex@assert{\\ifdefined\\x and not \\ifdefined\\y}\n\\relax\n");
    only(&analysis, "assertion-proved");
}

/// `\thepage` is the page counter's value, printed in arabic or roman.
#[test]
fn thepage_compares_the_counter_whatever_prints_it() {
    if !installed() {
        return;
    }
    for numbering in ["arabic", "roman"] {
        let mut cfg = crate::support::config();
        cfg.lint_off.retain(|code| code != "assertion-proved");
        let analysis = Machine::analyze(
            &format!(
                "\\documentclass{{article}}\n\\pagenumbering{{{numbering}}}\n\\begin{{document}}\n\
                 \\setcounter{{page}}{{3}}\n%\\satex@assert{{\\thepage < 5}}\n%\\satex@assert{{\\thepage > 3}}\nx\n\\end{{document}}\n"
            ),
            None,
            &cfg,
        );
        let found = outcomes(&analysis);
        assert!(found.iter().any(|f| f.starts_with("assertion-proved: `\\thepage < 5`")), "{numbering}: {found:?}");
        assert!(found.iter().any(|f| f.starts_with("assertion-refuted: `\\thepage > 3`")), "{numbering}: {found:?}");
    }
}

/// An assertion on the file's last line, with nothing read after it, is
/// still checked.
#[test]
fn an_assertion_at_the_end_of_the_file_is_checked() {
    let analysis = analyze("\\countdef\\x=10 \\x=3\n%\\satex@assert{\\x = 3}\n");
    only(&analysis, "assertion-proved");
}

/// `>=` is the test `<` negated: assuming it narrows to the values at or
/// above the bound, so the same assertion after it holds.
#[test]
fn an_assumption_with_a_negated_relation_narrows_the_right_way() {
    let analysis = analyze(
        "\\countdef\\x=10 \\x=\\pdfuniformdeviate 10\n%\\satex@assume{\\x >= 5}\n%\\satex@assert{\\x >= 5}\n\\relax\n",
    );
    let found = outcomes(&analysis);
    assert!(found.iter().any(|f| f.starts_with("assertion-proved:")), "{found:?}");
}
