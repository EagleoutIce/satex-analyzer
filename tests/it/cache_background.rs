//! The background builder (`cache.background`, `satex cache build
//! --background`, `satex cache stop`) against a tree of its own.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use satex::cli::cache::background;
use satex::project::config::Config;

/// A scratch directory under the crate's target, never `/tmp`.
fn scratch(name: &str) -> PathBuf {
    crate::support::scratch_dir(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/scratch/cache-background")
            .join(format!("{name}-{}", std::process::id())),
    )
}

/// The mini tree, with the commands of the preamble a package is prepared
/// in (`\\documentclass{article}` … `\\begin{document}`) doing nothing.
pub(crate) fn tree(dir: &Path) -> PathBuf {
    let tree = crate::support::mini_texmf(dir);
    let kernel = tree.join("tex/latex/base/latex.ltx");
    let text = std::fs::read_to_string(&kernel).unwrap();
    std::fs::write(&kernel, text + "\\def\\documentclass#1{}\\def\\begin#1{}\\def\\end#1{}\n").unwrap();
    tree
}

/// The mini tree with `demo` as the first package to prepare.
fn setup(name: &str) -> (PathBuf, Config) {
    let dir = scratch(name);
    let tree = tree(&dir);
    let mut cfg = crate::support::mini_config(&tree, &dir.join("cache"));
    cfg.cache.index.common = vec!["demo".into()];
    (dir, cfg)
}

fn files(dir: &Path, part: &str) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(dir)
        .map(|e| e.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    found.retain(|n| n.contains(part));
    found
}

/// The builder prepares the kernel and then the packages, and stamps the
/// directory done; run again it finds everything fresh, which is what makes
/// it resumable after a stop or a crash. It takes no build lock at all, so
/// a foreground run missing the same cache has nothing to wait on (it
/// builds the cache itself); a foreground run leaves its build lock behind,
/// which shows the check would see one.
#[test]
fn the_builder_prepares_kernel_and_packages_resumes_and_takes_no_build_lock() {
    let (dir, cfg) = setup("build");
    let cache = dir.join("cache");
    let mut out = Vec::new();
    background::run(&cfg, &[], true, &mut out).expect("the builder");
    let log = String::from_utf8(out).unwrap();
    assert!(log.contains("kernel (pdftex) … built"), "{log}");
    assert!(log.contains("demo (pdftex) … built"), "{log}");
    assert!(log.contains("done:"), "{log}");
    assert!(!files(&cache, "format-pdftex").is_empty() && !files(&cache, "package-demo").is_empty());
    assert_eq!(background::status(&cache).0, "done");
    assert!(files(&cache, "building.lock").is_empty(), "the builder took a build lock: {:?}", files(&cache, ""));

    let mut again = Vec::new();
    background::run(&cfg, &[], true, &mut again).expect("the builder again");
    let again = String::from_utf8(again).unwrap();
    assert!(again.contains("kernel (pdftex) … fresh") && again.contains("demo (pdftex) … fresh"), "{again}");

    // A foreground run that builds a cache does take its lock.
    let doc = dir.join("doc.tex");
    let source = "\\usepackage{demo}\n\\demomacro\n";
    std::fs::write(&doc, source).unwrap();
    let mut foreground = cfg.clone();
    foreground.cache.dir = Some(dir.join("foreground"));
    satex::analysis::machine::Machine::analyze(source, Some(&doc), &foreground);
    assert!(!files(&dir.join("foreground"), "building.lock").is_empty(), "the contrast: a foreground run locks");
}

/// One builder per cache directory: a second one, while the first holds the
/// background lock, ends at once without building anything, so a builder
/// started twice (two terminals, the editor) never doubles the work.
/// `satex cache stop` then marks it stopped, and the next builder run by
/// hand clears the mark and resumes.
#[test]
fn a_second_builder_ends_at_once_and_stop_holds_until_resumed() {
    let (dir, cfg) = setup("single");
    let cache = dir.join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    let held = std::fs::File::create(cache.join("background.run")).unwrap();
    held.lock().expect("the lock");
    let mut out = Vec::new();
    background::run(&cfg, &[], true, &mut out).expect("the second builder");
    assert!(String::from_utf8(out).unwrap().contains("runs already"));
    assert!(files(&cache, "format-").is_empty(), "nothing was built");
    assert!(background::stop(&cache).expect("stop"), "a builder runs");
    drop(held);
    assert_eq!(background::status(&cache).0, "stopped");
    let mut out = Vec::new();
    background::run(&cfg, &[], true, &mut out).expect("resumed");
    let log = String::from_utf8(out).unwrap();
    assert!(log.contains("done:"), "run by hand it resumes: {log}");
    assert_eq!(background::status(&cache).0, "done");
}

/// The global config of a run in `dir`, under its own HOME and XDG dirs.
fn command(dir: &Path, global: &str) -> std::process::Command {
    let config = dir.join("config");
    std::fs::create_dir_all(config.join("satex")).unwrap();
    std::fs::write(config.join("satex/satex.yaml"), global).unwrap();
    // The same in the run's folder, over the satex.yaml of this repository
    // above it; the builder, at the root, reads the global one alone.
    std::fs::write(dir.join("satex.yaml"), global).unwrap();
    let mut command = crate::support::satex();
    command
        .current_dir(dir)
        .env_remove("SATEX_NO_SETUP")
        .env_remove("CI")
        .env("HOME", dir)
        .env("XDG_CONFIG_HOME", &config)
        .env("APPDATA", &config)
        .env("SATEX_CACHE", dir.join("cache"));
    command
}

pub(crate) fn global(dir: &Path, background: &str) -> String {
    format!(
        "texmf: {{roots: [{}], kpsewhich: false, fallback: false}}\ncache:\n  document: false\n  background: {background}\n  index: {{common: [demo]}}\nproject: {{ignore_other_configs: false}}\n",
        crate::support::tex_path(&dir.join("texmf-dist"))
    )
}

/// Waits for the builder in `cache` to end, stopping it past the deadline
/// so no test leaves one running.
pub(crate) fn settle(cache: &Path) -> String {
    let deadline = Instant::now() + Duration::from_secs(120);
    // A start returns once the builder holds its lock, or it ended.
    while background::running(cache) {
        if Instant::now() > deadline {
            let _ = background::stop(cache);
            while background::running(cache) {
                std::thread::sleep(Duration::from_millis(100));
            }
            return "timed out".into();
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    background::status(cache).0
}

/// With `cache.background: yes` an ordinary run spawns the detached
/// builder and returns without waiting for it; the builder then prepares
/// the kernel and `demo` into the cache directory and stamps it done. A
/// later run with the stamp starts nothing. Without a terminal the
/// question (`ask`) is never put and the config never written.
#[test]
fn a_run_starts_the_builder_without_waiting_and_never_asks_off_a_terminal() {
    let dir = scratch("spawn");
    tree(&dir);
    std::fs::write(dir.join("doc.tex"), "\\usepackage{demo}\n\\demomacro\n").unwrap();
    let cache = dir.join("cache");

    let asked = command(&dir, &global(&dir, "ask")).args(["-f", "doc.tex", "lint"]).output().expect("satex");
    let config = std::fs::read_to_string(dir.join("config/satex/satex.yaml")).unwrap();
    assert!(config.contains("background: ask"), "untouched: {config}");
    assert!(!String::from_utf8_lossy(&asked.stderr).contains("[Y/n]"));
    assert!(!cache.join("background.log").exists(), "nothing started");

    let run = command(&dir, &global(&dir, "yes")).args(["-f", "doc.tex", "lint"]).output().expect("satex");
    assert!(String::from_utf8_lossy(&run.stderr).contains("in the background"), "{run:?}");
    assert_eq!(settle(&cache), "done", "{}", std::fs::read_to_string(cache.join("background.log")).unwrap_or_default());
    let log = std::fs::read_to_string(cache.join("background.log")).unwrap();
    assert!(log.contains("demo (pdftex) …"), "{log}");

    let again = command(&dir, &global(&dir, "yes")).args(["-f", "doc.tex", "lint"]).output().expect("satex");
    assert!(!String::from_utf8_lossy(&again.stderr).contains("in the background"), "done: nothing to start");
    assert!(!background::running(&cache));

    // `satex cache` shows it.
    let status = command(&dir, &global(&dir, "yes")).arg("cache").output().expect("satex cache");
    let text = String::from_utf8_lossy(&status.stdout);
    assert!(text.contains("background") && text.contains("done · cache.background: yes"), "{text}");
}

/// The size bound: with the directory at `limits.cache_size` the builder
/// stores no package and ends `full`, not `done`, and it evicts nothing to
/// make room: a cache a document left stays.
#[test]
fn a_full_directory_ends_the_builder_full_and_evicts_nothing() {
    let (dir, mut cfg) = setup("bound");
    let cache = dir.join("cache");
    std::fs::create_dir_all(&cache).unwrap();
    let theirs = cache.join("package-theirs-v1-aaaa-bbbb.postcard");
    std::fs::write(&theirs, vec![0u8; 4096]).unwrap();
    cfg.limits.cache_size = bytesize::ByteSize::b(4096);
    let mut out = Vec::new();
    background::run(&cfg, &[], true, &mut out).expect("the builder");
    let log = String::from_utf8(out).unwrap();
    assert!(log.contains("full:"), "{log}");
    assert!(background::status(&cache).0.starts_with("full"), "{:?}", background::status(&cache));
    assert!(theirs.exists(), "a document's cache was evicted");
    assert!(files(&cache, "package-demo").is_empty(), "stored past the bound");
}

/// A stop given while a run starts the builder is not lost: the builder a
/// run started ends at once on it, and only one run by hand clears it.
#[test]
fn a_stop_during_start_holds() {
    let (dir, cfg) = setup("stop-start");
    let cache = dir.join("cache");
    background::stop(&cache).expect("stop");
    let mut out = Vec::new();
    background::run(&cfg, &[], false, &mut out).expect("started by a run");
    assert!(String::from_utf8(out).unwrap().contains("stopped"));
    assert!(files(&cache, "format-").is_empty(), "nothing was built");
    assert_eq!(background::status(&cache).0, "stopped");
    let mut out = Vec::new();
    background::run(&cfg, &[], true, &mut out).expect("by hand");
    assert!(String::from_utf8(out).unwrap().contains("done:"));
}

/// `SATEX_CACHE` relative to the run's folder: the builder, which runs at
/// the root, still builds into that folder's cache, not one below `/`.
#[test]
fn a_relative_cache_directory_reaches_the_builder_absolute() {
    let dir = scratch("relative");
    tree(&dir);
    std::fs::write(dir.join("doc.tex"), "\\usepackage{demo}\n\\demomacro\n").unwrap();
    let run = command(&dir, &global(&dir, "yes"))
        .env("SATEX_CACHE", "cache")
        .args(["-f", "doc.tex", "lint"])
        .output()
        .expect("satex");
    let cache = dir.join("cache");
    assert_eq!(settle(&cache), "done", "{run:?}");
    assert!(!files(&cache, "package-demo").is_empty());
}
