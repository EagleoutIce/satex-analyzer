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

/// Templates written for a class newer than some installations: elsarticle before 3.3 and
/// els-cas-templates of TeX Live 2019 have no `\affiliation` (the latter also lacks the
/// `thumbnails/cas-email.jpeg` its `\ead` includes), so the sample text cannot build there.
const NEWER: [(&str, &str, &str); 2] =
    [("elsarticle", "elsarticle.cls", "\\def\\affiliation"), ("elsevier-cas", "cas-common.sty", "\\affiliation")];

/// Whether the installed class is as new as template `id`'s text.
fn current(id: &str) -> bool {
    NEWER.iter().all(|&(t, file, text)| t != id || crate::support::installed_file_has(file, text))
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
    // A class the installation does not ship yet (llncs before TeX Live 2020) has nothing to build.
    let ships = |t: &&templates::Template| {
        t.class.as_ref().is_none_or(|c| crate::support::installed_file_has(&format!("{c}.cls"), ""))
            && t.package.as_ref().is_none_or(|p| crate::support::installed_file_has(&format!("{p}.sty"), ""))
            && current(&t.id)
    };
    for t in templates::catalog().iter().filter(|t| t.fetch.is_none()).filter(ships) {
        let dir = base.join(&t.id);
        let (ok, _, err) = new(&["-t", &t.id, "--no-check", dir.to_str().unwrap()], None);
        assert!(ok, "{}: {err}", t.id);
        failed.extend(builds(&dir).err());
    }
    assert!(failed.is_empty(), "{failed:?}");
}

/// Steps of the dearest template's last build run (acm-sigconf, measured 2026-11-22).
/// A shipped document fails past +20% of it, as the budgets do.
const DEAREST: u64 = 106_000_000;
const SLACK: u64 = 120;

/// `satex lint` on `dir/main.tex` the way a user runs it: the whole build, each run reading
/// the files the run before wrote, on the default stack. No stored run, and no wall clock:
/// only the step limits can stop it, so a loaded machine fails nothing. Says why this was
/// no run to the end within the budget or, where the document builds (`clean`), one
/// with an error.
fn reaches_its_end(tag: &str, dir: &Path, clean: bool) -> Result<(), String> {
    let args = ["lint", "-f", "main.tex", "--format", "json", "--stats-json"];
    let limits = ["--set", "limits.seconds=0", "--set", "cache.document=false"];
    let (out, err) = crate::crash::child(dir, &[&args[..], &limits[..]].concat(), "", None)?;
    let stats = err.lines().rev().find_map(|l| serde_json::from_str::<serde_json::Value>(l).ok());
    let stats = stats.filter(|s| s["steps"].is_u64()).ok_or_else(|| format!("no statistics: {err}"))?;
    let steps = stats["steps"].as_u64().unwrap_or(u64::MAX);
    if stats["exhausted"] != false || stats["limits_hit"] != 0 {
        return Err(format!("a limit stopped or cut the run ({} hit, {steps} steps)", stats["limits_hit"]));
    }
    crate::support::record_steps(&format!("templates::{tag}"), steps);
    if steps * 100 > DEAREST * SLACK {
        return Err(format!("{steps} steps, over the budget of {DEAREST} + {}%", SLACK - 100));
    }
    let findings: Vec<serde_json::Value> = serde_json::from_str(&out).map_err(|e| format!("findings: {e}"))?;
    let errors: Vec<String> = (findings.iter().filter(|f| clean && f["severity"] == "error"))
        .map(|f| format!("{}:{} {} {}", f["file"], f["line"], f["number"], f["message"]))
        .collect();
    match errors.is_empty() {
        true => Ok(()),
        false => Err(format!("errors in a document that builds: {}", errors.join(" | "))),
    }
}

/// Intuition: a template is the first document a user analyses, so each must reach its end,
/// within a budget, with no error (each builds: `each_installed_template_builds`), on the
/// path the user is on: the whole build, where run 2 reads what run 1 wrote. Pins acm-sigconf,
/// whose second run (the page count read back from the `.aux`, unknown) forked 95k times at
/// `\maketitle` until `limits.line_steps` cut it, unseen by every test: none ran a template
/// through its build.
#[test]
fn each_template_reaches_its_end_without_an_error() {
    if !crate::support::installed() {
        return;
    }
    let failed: Vec<String> = (crate::crash::templates("templates-end").iter())
        .filter_map(|(id, dir)| reaches_its_end(id, dir, current(id)).err().map(|why| format!("{id}: {why}")))
        .collect();
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// Intuition: the playground's examples are what a visitor runs first. Each reaches its end
/// within the budget. Several show an error on purpose, so errors are not counted here.
#[test]
fn each_site_example_reaches_its_end() {
    if !crate::support::installed() {
        return;
    }
    let mut failed = Vec::new();
    for (name, path) in crate::crash::site_examples() {
        let dir = crate::crash::folder(&format!("templates-site-{name}"), path.parent());
        std::fs::copy(&path, dir.join("main.tex")).expect("the example is copied");
        failed.extend(reaches_its_end(&format!("site-{name}"), &dir, false).err().map(|why| format!("{name}: {why}")));
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

/// Intuition: options given to `satex new` are what the written header line passes, and the
/// run that reads the new project reports the class it loaded with the version its file says.
/// Pins the round trip options in, header line out, class seeing them.
#[test]
fn new_writes_the_options_and_reports_the_version() {
    if !crate::support::installed() || !crate::support::installed_file_has("llncs.cls", "") {
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
/// so. Pins `templates.fetch: false` never reaching the network and the cache being read back,
/// and the fetched style being frozen in the new project's `satex.yaml` (robust: S053/C010/C061
/// inside acl.sty and neurips_2025.sty were reported as the project's own).
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
    // the provider's style is the provider's: frozen, so its findings are not the project's
    let yaml = std::fs::read_to_string(dir.join("satex.yaml")).unwrap_or_default();
    assert!(yaml.contains("frozen: ['neurips_2025.sty']"), "{yaml}");
}
