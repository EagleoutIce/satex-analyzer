//! `satex completions install` and the `--version` hint, in a scratch HOME.

use std::path::Path;
use std::process::{Command, Output};

fn run(home: &Path, shell: &str, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_satex"))
        .args(args)
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

/// An explicit shell wins over `$SHELL`; the output names shell and file.
#[test]
fn install_writes_the_named_shell_file() {
    let home = scratch("install");
    let out = run(&home, "/bin/bash", &["completions", "install", "zsh"]);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{text}{}", String::from_utf8_lossy(&out.stderr));
    assert!(text.contains("shell: zsh") && text.contains(".zshrc"), "{text}");
    let rc = std::fs::read_to_string(home.join(".zshrc")).expect(".zshrc");
    assert!(rc.contains("COMPLETE=zsh satex"), "{rc}");
    assert!(!home.join(".bashrc").exists());
    // Not run from a shell here: `$SHELL` decides.
    let out = run(&home, "/usr/bin/fish", &["completions", "install"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("shell: fish"));
    assert!(home.join(".config/fish/completions/satex.fish").exists());
}

/// `--version` hints on stderr for every used shell without the completion.
#[test]
fn version_hints_for_shells_without_completion() {
    let home = scratch("hint");
    std::fs::write(home.join(".zshrc"), "# empty\n").unwrap();
    let out = run(&home, "/bin/bash", &["--version"]);
    let err = String::from_utf8_lossy(&out.stderr);
    let stdout = String::from_utf8_lossy(&out.stdout);
    for shell in ["bash", "zsh"] {
        let hint = format!("shell completion is not set up for {shell}: run `satex completions install {shell}`");
        assert!(err.contains(&hint), "{err}");
        assert!(!stdout.contains(&hint));
    }
    assert!(!err.contains("for fish"), "{err}");
    for shell in ["bash", "zsh"] {
        assert!(run(&home, "/bin/bash", &["completions", "install", shell]).status.success());
    }
    let out = run(&home, "/bin/bash", &["--version"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!err.contains("shell completion"), "{err}");
}
