//! `satex new` (src/lsp/templates.rs, src/cli/new.rs): provider templates and imported projects.
//! Offline: the fixtures are zips in tests/fixtures/templates, a fetched template is served from a
//! seeded cache.

use satex::lsp::templates;
use std::path::{Path, PathBuf};

/// A fresh folder for one test.
fn folder(tag: &str) -> PathBuf {
    let dir = crate::support::tmp_root().join(format!("templates-{tag}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/templates").join(name)
}

/// `satex new ARGS`, its status, stdout and stderr.
fn new(args: &[&str], cache: Option<&Path>) -> (bool, String, String) {
    let mut c = crate::support::satex();
    c.arg("new").args(args);
    if let Some(cache) = cache {
        c.env("XDG_CACHE_HOME", cache);
    }
    let out = c.output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into(), String::from_utf8_lossy(&out.stderr).into())
}

/// Whether `pdflatex` builds `dir/main.tex` in one pass without an error.
fn builds(dir: &Path) -> Result<(), String> {
    let out = std::process::Command::new("pdflatex")
        .args(["-interaction=nonstopmode", "-halt-on-error", "main.tex"])
        .current_dir(dir)
        .output()
        .map_err(|e| e.to_string())?;
    let log = std::fs::read_to_string(dir.join("main.log")).unwrap_or_default();
    let errors: Vec<&str> = log.lines().filter(|l| l.starts_with('!')).collect();
    match out.status.success() {
        true => Ok(()),
        false => Err(format!("{}: {}", dir.display(), errors.join(" | "))),
    }
}

/// Intuition: every template the TeX installation serves, written by `satex new`, builds with
/// its engine. Pins templates whose sample text drifts from what the class accepts.
#[test]
fn each_installed_template_builds() {
    if !crate::support::installed() || satex::project::paths::which("pdflatex").is_err() {
        return;
    }
    let base = folder("builds");
    let mut failed = Vec::new();
    for t in templates::catalog().iter().filter(|t| t.fetch.is_none()) {
        let dir = base.join(&t.id);
        let (ok, _, err) = new(&["-t", &t.id, "--no-check", dir.to_str().unwrap()], None);
        assert!(ok, "{}: {err}", t.id);
        failed.extend(builds(&dir).err());
    }
    assert!(failed.is_empty(), "{failed:?}");
}

/// Intuition: options given to `satex new` are what the written header line passes, and the
/// run that reads the new project reports the class it loaded with the version its file says.
/// Pins the round trip options in, header line out, class seeing them.
#[test]
fn new_writes_the_options_and_reports_the_version() {
    if !crate::support::installed() {
        return;
    }
    let dir = folder("version").join("paper");
    let (ok, out, err) =
        new(&["-t", "lncs", "-o", "runningheads,orivec", "--format", "json", dir.to_str().unwrap()], None);
    assert!(ok, "{err}");
    let main = std::fs::read_to_string(dir.join("main.tex")).unwrap();
    assert!(main.starts_with("\\documentclass[runningheads, orivec]{llncs}"), "{main}");
    assert!(err.contains("llncs.cls from TeX installation"), "{err}");
    let records: serde_json::Value = serde_json::from_str(&out).expect("json records");
    let class = records.as_array().unwrap().iter().find(|r| r["kind"] == "class" && r["name"] == "llncs");
    let version = class.and_then(|c| c["detail"].as_str()).unwrap_or_default();
    let date = regex::Regex::new(r"^\d{4}/\d\d/\d\d").unwrap();
    assert!(date.is_match(version), "the class's version from the run: {out}");
    // a second write refuses to replace the files
    let (ok, _, err) = new(&["-t", "lncs", "--no-check", dir.to_str().unwrap()], None);
    assert!(!ok && err.contains("--force"), "{err}");
}

/// Intuition: an imported archive (a GitHub repository's zip, offline here) keeps its folders,
/// drops `.git`, strips the one top folder, and finds the main file by code, not by a comment.
#[test]
fn import_from_an_offline_zip() {
    let dir = folder("import");
    let zip = fixture("repo.zip");
    let (ok, _, err) = new(&["--from", zip.to_str().unwrap(), "--no-check", dir.to_str().unwrap()], None);
    assert!(ok, "{err}");
    assert!(err.contains("main file main.tex"), "{err}");
    assert!(dir.join("sections/intro.tex").is_file() && !dir.join(".git").exists(), "{err}");
    if crate::support::installed() && satex::project::paths::which("pdflatex").is_ok() {
        builds(&dir).unwrap();
    }
}

/// Intuition: an archive entry that points outside the project (`../x`) refuses the whole import:
/// nothing is written, inside or beside the target.
#[test]
fn an_escaping_archive_writes_nothing() {
    let base = folder("escape");
    let dir = base.join("out");
    let (ok, _, err) =
        new(&["--from", fixture("escape.zip").to_str().unwrap(), "--no-check", dir.to_str().unwrap()], None);
    assert!(!ok && err.contains("outside the project"), "{err}");
    assert!(!dir.exists() && !base.join("outside.tex").exists());
}

/// Intuition: offline, a template the provider serves comes from the cache, however old, and says
/// so. Pins `templates.fetch: false` never reaching the network and the cache being read back.
#[test]
fn a_fetched_template_serves_from_the_cache_offline() {
    let base = folder("cache");
    let cache = base.join("cache/satex/templates/neurips");
    std::fs::create_dir_all(&cache).unwrap();
    let sty = "\\ProvidesPackage{neurips_2025}[2025/05/01 a cached placeholder style]\n";
    std::fs::write(cache.join("neurips_2025.sty"), sty).unwrap();
    let manifest = serde_json::json!({ "url": "https://example.org/Styles.zip", "year": 2025,
        "sha256": "0", "fetched": 0, "files": ["neurips_2025.sty"] });
    std::fs::write(cache.join("manifest.json"), manifest.to_string()).unwrap();
    let dir = base.join("paper");
    let args = ["-t", "neurips", "--no-check", "--set", "templates.fetch=false", dir.to_str().unwrap()];
    let (ok, _, err) = new(&args, Some(&base.join("cache")));
    assert!(ok, "{err}");
    assert!(err.contains("neurips_2025.sty from cache") && err.contains("a cached placeholder style"), "{err}");
    let main = std::fs::read_to_string(dir.join("main.tex")).unwrap();
    assert!(main.contains("\\usepackage[preprint]{neurips_2025}"), "{main}");
    assert_eq!(std::fs::read_to_string(dir.join("neurips_2025.sty")).unwrap(), sty);
}
