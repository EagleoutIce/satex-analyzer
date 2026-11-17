//! What the installed manuals give a package: its commands and a line about it.

use satex::plugins::docs::{Dirs, PLUGINS, about, mine};
use satex::project::config::DocsConfig;
use std::collections::HashMap;
use std::path::PathBuf;

fn installed(file: &str) -> PathBuf {
    let out = std::process::Command::new("kpsewhich").arg(file).output().expect("kpsewhich runs");
    PathBuf::from(String::from_utf8_lossy(&out.stdout).trim())
}

/// What the manuals of `file` give, `None` where no documentation is
/// installed (TeX Live's docker image has none): nothing to check then.
fn mined(file: &str) -> Option<HashMap<String, satex::plugins::docs::Entry>> {
    let config = DocsConfig::default();
    let entries = mine(PLUGINS, &config, &Dirs::of(&config, &installed(file))?);
    (!entries.is_empty()).then_some(entries)
}

#[test]
fn etoolbox_manual_gives_usage_and_text() {
    let Some(e) = mined("etoolbox.sty") else { return };
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
    let Some(e) = mined("beamerbasetranslator.sty") else { return };
    let logo = &e["logo"];
    assert_eq!(logo.usage.as_deref(), Some("\\logo{⟨logo text⟩}"), "{logo:?}");
    assert!(logo.text.is_some(), "{logo:?}");
    assert!(e.contains_key("frame"));
}

#[test]
fn prose_examples_are_no_entries() {
    let Some(e) = mined("latex.ltx") else { return };
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

/// The texts of every `\changes` in `source`, each on one line.
fn change_notes(source: &str) -> Vec<String> {
    let doc: String = source.lines().map(|l| l.trim_start_matches('%').trim()).collect::<Vec<_>>().join(" ");
    doc.split("\\changes")
        .skip(1)
        .filter_map(|rest| {
            // the third of its groups, braces nested
            let mut groups = Vec::new();
            let mut rest = rest.trim_start();
            while groups.len() < 3 && rest.starts_with('{') {
                let mut depth = 0;
                let end = rest
                    .char_indices()
                    .find(|&(_, c)| {
                        depth += match c {
                            '{' => 1,
                            '}' => -1,
                            _ => 0,
                        };
                        depth == 0
                    })?
                    .0;
                groups.push(rest[1..end].to_string());
                rest = rest[end + 1..].trim_start();
            }
            groups.get(2).cloned()
        })
        .collect()
}

/// A description is a sentence of the documentation's prose: it starts and
/// ends as one, and it is no part of a `\changes`
/// note (`\changes{v1.0z}{1995/07/13}{… use \cs{relax} in the THEN case}`).
#[test]
fn descriptions_are_sentences_of_the_prose() {
    let words = |s: &str| {
        s.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect::<Vec<_>>()
    };
    for file in ["latex.ltx", "graphicx.sty", "xcolor.sty", "array.sty", "hyperref.sty", "expl3.sty"] {
        let Some(entries) = mined(file) else { continue };
        let mut notes: HashMap<String, Vec<Vec<String>>> = HashMap::new();
        for entry in entries.values() {
            let Some(text) = &entry.text else { continue };
            let first = text.chars().next().unwrap_or(' ');
            assert!(
                first.is_uppercase() || "`⟨*\\".contains(first),
                "{file}: \\{} starts mid-sentence: {text:?}",
                entry.name
            );
            assert!(text.ends_with(['.', '!', '?', ':', '…']), "{file}: \\{} is no sentence: {text:?}", entry.name);
            let Some(at) = &entry.at else { continue };
            let notes = notes.entry(at.path.clone()).or_insert_with(|| {
                change_notes(&std::fs::read_to_string(&at.path).unwrap_or_default()).iter().map(|n| words(n)).collect()
            });
            let said = words(text);
            assert!(
                said.len() < 3 || !notes.iter().any(|n| n.windows(said.len()).any(|w| w == said.as_slice())),
                "{file}: \\{}'s text is a change note: {text:?}",
                entry.name
            );
        }
    }
}

/// The site's hover cards read `scope --all` of a document loading every package its pages name
/// (doc/site/build.py `scope_cards`): `\draw` there is tikz-timing's `\let`, yet TikZ owns it and
/// pgf's manual documents it.
#[test]
fn site_card_of_draw_names_tikz_and_its_manual() {
    // the manual's source, which TeX Live's docker image leaves out
    let dist =
        std::process::Command::new("kpsewhich").args(["-var-value", "TEXMFDIST"]).output().expect("kpsewhich runs");
    let manual = PathBuf::from(String::from_utf8_lossy(&dist.stdout).trim())
        .join("doc/generic/pgf/pgfmanual-en-tikz-actions.tex");
    if !manual.is_file() || !installed("tikz-timing.sty").is_file() {
        return;
    }
    let dir = crate::support::tmp_root().join(format!("satex-site-draw-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let doc = dir.join("probe.tex");
    let text =
        "\\documentclass{article}\n\\usepackage{tikz}\n\\usepackage{tikz-timing}\n\\begin{document}\n\\end{document}\n";
    std::fs::write(&doc, text).unwrap();
    let out = crate::support::satex()
        .args(["--format", "json", "scope", "--all", "-f"])
        .arg(&doc)
        .output()
        .expect("satex runs");
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap_or_default();
    let draw = rows.iter().find(|r| r["name"] == "\\draw").expect("\\draw in scope");
    assert_eq!(draw["provider"], "tikz", "{draw}");
    assert!(draw["does"].as_str().is_some_and(|d| d.contains("\\path[draw]")), "{draw}");
    assert_eq!(draw["does_from"], "the pgfmanual manual", "{draw}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// pgf runs from `tex/latex/pgf` and `tex/generic/pgf`, its manual stands
/// in `doc/generic/pgf`: a package's documentation may sit under another
/// format folder. Its `command` and `key` entries document commands and
/// keys by their pgfkeys path, a key's `(initially …)` note left out.
#[test]
fn pgf_manual_documents_commands_and_keys() {
    let Some(e) = mined("tikz.sty") else { return };
    let moveto = &e["pgfpathmoveto"];
    assert_eq!(moveto.usage.as_deref(), Some("\\pgfpathmoveto{⟨coordinate⟩}"), "{moveto:?}");
    assert!(moveto.text.is_some(), "{moveto:?}");
    let xshift = &e["/tikz/xshift"];
    assert!(xshift.usage.as_deref().is_some_and(|u| u.starts_with("/tikz/xshift=⟨")), "{xshift:?}");
    assert!(xshift.text.is_some(), "{xshift:?}");
    assert!(e.keys().all(|k| !k.contains(" (")), "a note in a name");
    // the code files of the same package share its manual
    assert!(mined("pgfcore.sty").is_some_and(|c| c.contains_key("pgfpathmoveto")));
}

/// intcalc runs from `tex/generic`, its `.dtx` stands in `source/latex`;
/// makeidx's source announces `\DescribeMacro\printindex` without braces.
#[test]
fn sources_under_another_format_and_unbraced_anchors() {
    if let Some(e) = mined("intcalc.sty") {
        assert!(e.get("intcalcNum").is_some_and(|n| n.text.is_some()), "{:?}", e.get("intcalcNum"));
    }
    if let Some(e) = mined("makeidx.sty") {
        let p = &e["printindex"];
        assert!(p.text.as_deref().is_some_and(|t| t.starts_with("This command can be used")), "{p:?}");
    }
}

/// A primitive's manual is the engine's: the pdfTeX manual heads each of
/// its primitives with it, TeX by Topic's glossary labels an item with it.
#[test]
fn engine_manuals_document_primitives() {
    let config = DocsConfig::default();
    let dist = std::process::Command::new("kpsewhich").args(["-var-value", "TEXMFDIST"]).output().expect("kpsewhich");
    let root = PathBuf::from(String::from_utf8_lossy(&dist.stdout).trim());
    let dirs: Vec<PathBuf> = config.engine_manuals.iter().map(|m| root.join(m)).filter(|d| d.is_dir()).collect();
    // TeX Live's docker image installs no manuals
    if dirs.len() < config.engine_manuals.len() {
        return;
    }
    let mut e = std::collections::HashMap::new();
    for dir in dirs {
        e.extend(mine(PLUGINS, &config, &Dirs { source: None, doc: Some(dir) }));
    }
    for name in ["pdfpagewidth", "vskip"] {
        assert!(e.get(name).is_some_and(|e| e.text.is_some()), "{name}: {:?}", e.get(name));
    }
}
