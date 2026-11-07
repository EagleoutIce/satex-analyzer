//! `satex completions install` and the `--version` hint, in a scratch HOME.

use std::path::Path;
use std::process::Output;

/// Runs satex with `home/bin` as the whole `PATH`: the shells installed are
/// the ones a test puts there.
fn run(home: &Path, shell: &str, args: &[&str]) -> Output {
    let bin = home.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    crate::support::satex()
        .args(args)
        .env("PATH", &bin)
        .env("HOME", home)
        .env("SHELL", shell)
        .env_remove("XDG_CONFIG_HOME")
        .current_dir(home)
        .output()
        .unwrap_or_else(|e| panic!("failed to run satex {args:?}: {e}"))
}

fn scratch(name: &str) -> std::path::PathBuf {
    crate::support::scratch_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("target/scratch/completions").join(name))
}

/// An explicit shell is the one set up; without one, every shell installed
/// or in use is. The output says how to load it in this shell.
#[test]
fn install_writes_the_named_shell_file() {
    let home = scratch("install");
    let out = run(&home, "/bin/bash", &["completions", "install", "zsh"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}{}", String::from_utf8_lossy(&out.stderr));
    assert!(text.contains("zsh: added to") && text.contains("source ") && text.contains(".zshrc"), "{text}");
    let rc = std::fs::read_to_string(home.join(".zshrc")).expect(".zshrc");
    assert!(rc.contains("COMPLETE=zsh satex"), "{rc}");
    assert!(!home.join(".bashrc").exists());
    // `$SHELL` is fish and zsh is installed: both, zsh already done.
    installed(&home, "zsh");
    let out = run(&home, "/usr/bin/fish", &["completions", "install"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("fish: added to") && text.contains("zsh: already set up"), "{text}");
    assert!(home.join(".config/fish/completions/satex.fish").exists());
}

/// `--version` hints on stderr, in one line, at the shells without the
/// completion.
#[test]
fn version_hints_for_shells_without_completion() {
    let home = scratch("hint");
    installed(&home, "zsh");
    let out = run(&home, "/bin/bash", &["--version"]);
    let err = String::from_utf8_lossy(&out.stderr);
    let hint = "no shell completion for bash, zsh: run `satex completions install`";
    assert!(err.contains(hint), "{err}");
    assert!(!String::from_utf8_lossy(&out.stdout).contains(hint));
    assert_eq!(err.matches("shell completion").count(), 1, "{err}");
    assert!(run(&home, "/bin/bash", &["completions", "install"]).status.success());
    let out = run(&home, "/bin/bash", &["--version"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("shell completion"), "{err}");
}

/// An executable `shell` in the test's `PATH`.
fn installed(home: &Path, shell: &str) {
    use std::os::unix::fs::PermissionsExt;
    let path = home.join("bin").join(shell);
    std::fs::create_dir_all(home.join("bin")).unwrap();
    std::fs::write(&path, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// `satex completions json` exports clap's command tree: the console and
/// the docs' command lines show each flag's help from it.
#[test]
fn the_command_tree_names_each_flag_with_its_help() {
    let out = crate::support::satex().args(["completions", "json"]).output().expect("satex runs");
    let tree: serde_json::Value = serde_json::from_slice(&out.stdout).expect("JSON");
    let lint = tree["command"]["subcommands"].as_array().unwrap().iter().find(|c| c["name"] == "lint").expect("lint");
    let fix = lint["args"].as_array().unwrap().iter().find(|a| a["long"] == "fix").expect("--fix");
    assert!(fix["help"].as_str().is_some_and(|h| !h.is_empty()), "{fix}");
    let format = tree["command"]["args"].as_array().unwrap().iter().find(|a| a["long"] == "format").expect("--format");
    assert!(format["possible_values"].as_array().unwrap().iter().any(|v| v == "json"), "{format}");
    assert!(format["defaults"].as_array().unwrap().iter().any(|v| v == "text"), "{format}");
}
