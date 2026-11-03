//! Rules a project breaks so often that they may be its style
//! (`style_prompt` in satex.yaml), and the answer written to its satex.yaml.

use satex::project::config::{Config, StyleChoice, StylePrompt, record_style_choice};

/// `\hskip` where LaTeX has `\hspace`, in `times` paragraphs.
pub(crate) fn hskips(times: usize) -> String {
    let body: String = (0..times).map(|i| format!("a\\hskip 1pt b{i}\n\n")).collect();
    format!("\\documentclass{{article}}\n\\begin{{document}}\n{body}\\end{{document}}\n")
}

fn candidates(times: usize, prompt: &StylePrompt) -> Vec<String> {
    let records = satex::linter::lint(&crate::support::analyze_latex(&hskips(times)));
    satex::linter::style_candidates(&records, prompt)
        .iter()
        .map(|c| format!("{} {}", c["code"].as_str().unwrap_or(""), c["count"]))
        .collect()
}

#[test]
fn a_rule_past_the_threshold_is_a_candidate() {
    let at = |min_findings| StylePrompt { min_findings, ..StylePrompt::default() };
    let times = 12;
    assert_eq!(candidates(times, &at(times)), [format!("primitive-tex-command {times}")]);
    assert!(candidates(times, &at(times + 1)).is_empty());
    assert!(candidates(times, &StylePrompt { enabled: false, ..at(1) }).is_empty());
    let kept = StylePrompt { keep: vec!["primitive-tex-command".into()], ..at(1) };
    assert!(candidates(times, &kept).is_empty());
}

#[test]
fn the_question_names_the_construct() {
    let records = satex::linter::lint(&crate::support::analyze_latex(&hskips(3)));
    let prompt = StylePrompt { min_findings: 3, ..StylePrompt::default() };
    let found = satex::linter::style_candidates(&records, &prompt);
    let question = found[0]["question"].as_str().unwrap_or_default();
    assert!(question.contains("hskip") && question.contains("3 times"), "{question}");
}

fn project(tag: &str, yaml: Option<&str>) -> std::path::PathBuf {
    let dir = crate::support::tmp_root().join(format!("satex-style-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    if let Some(yaml) = yaml {
        std::fs::write(dir.join("satex.yaml"), yaml).unwrap();
    }
    dir
}

fn merged(dir: &std::path::Path) -> Config {
    let mut cfg = Config::default();
    cfg.merge(&dir.join("satex.yaml")).unwrap();
    cfg
}

#[test]
fn yes_adds_the_rule_to_the_projects_lint_off() {
    let dir = project("off", Some("# the project's own\nlint_off: [unused-label]  # mine\n"));
    record_style_choice(&dir, "primitive-tex-command", StyleChoice::Off).unwrap();
    let text = std::fs::read_to_string(dir.join("satex.yaml")).unwrap();
    assert!(text.contains("# the project's own\nlint_off: [unused-label, primitive-tex-command]  # mine"), "{text}");
    assert_eq!(merged(&dir).lint_off, ["unused-label", "primitive-tex-command"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn yes_without_a_satex_yaml_makes_one_that_extends_the_defaults() {
    let dir = project("new", None);
    record_style_choice(&dir, "primitive-tex-command", StyleChoice::Off).unwrap();
    record_style_choice(&dir, "primitive-tex-command", StyleChoice::Off).unwrap();
    let cfg = merged(&dir);
    let defaults = Config::default().lint_off;
    assert_eq!(cfg.lint_off, [defaults, vec!["primitive-tex-command".to_string()]].concat());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn never_ask_keeps_the_rule() {
    let dir = project("keep", Some("style_prompt:\n  min_findings: 5\nlint_off: []\n"));
    record_style_choice(&dir, "primitive-tex-command", StyleChoice::Keep).unwrap();
    let cfg = merged(&dir);
    assert_eq!(cfg.style_prompt.keep, ["primitive-tex-command"]);
    assert_eq!(cfg.style_prompt.min_findings, 5);
    assert!(cfg.lint_off.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}
