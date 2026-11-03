//! What the installed manuals give a package: its commands and a line about it.

use satex::config::DocsConfig;
use satex::plugin::docs::{Dirs, PLUGINS, about, mine};
use std::collections::HashMap;
use std::path::PathBuf;

fn installed(file: &str) -> PathBuf {
    let out = std::process::Command::new("kpsewhich").arg(file).output().expect("kpsewhich runs");
    PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
}

fn mined(file: &str) -> HashMap<String, satex::plugin::docs::Entry> {
    let config = DocsConfig::default();
    mine(PLUGINS, &config, &Dirs::of(&config, &installed(file)).expect("documented"))
}

#[test]
fn etoolbox_manual_gives_usage_and_text() {
    let e = mined("etoolbox.sty");
    let robust = &e["newrobustcmd"];
    assert!(
        robust.usage.as_deref().is_some_and(|u| u.starts_with("\\newrobustcmd{⟨command⟩}[⟨arguments⟩]")),
        "{robust:?}"
    );
    assert!(robust.text.as_deref().is_some_and(|t| t.starts_with("The syntax and behavior")), "{robust:?}");
    assert!(e.contains_key("robustify"));
}

#[test]
fn beamer_user_guide_documents_its_packages() {
    let e = mined("beamerbasetranslator.sty");
    let logo = &e["logo"];
    assert_eq!(logo.usage.as_deref(), Some("\\logo{⟨logo text⟩}"), "{logo:?}");
    assert!(logo.text.is_some(), "{logo:?}");
    assert!(e.contains_key("frame"));
}

#[test]
fn prose_examples_are_no_entries() {
    let e = mined("latex.ltx");
    // doc.dtx shows `|\DescribeMacro{\foo} @bar|` as an example
    assert!(!e.contains_key("foo"), "{:?}", e.get("foo"));
    assert!(e["footnotesize"].text.as_deref().is_some_and(|t| t.contains("normalsize")), "{:?}", e["footnotesize"]);
}

#[test]
fn every_package_has_a_line_about_it() {
    let config = DocsConfig::default();
    for file in ["upquote.sty", "etoolbox.sty", "beamerbasetranslator.sty", "shellesc.sty"] {
        let line = about(&config, &installed(file));
        assert!(line.as_deref().is_some_and(|l| l.split_whitespace().count() > 1), "{file}: {line:?}");
    }
}
