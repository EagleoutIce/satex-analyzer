//! satex-vscode: builds satex and the VS Code extension from this tree,
//! bundles the binary into a VSIX and installs it into VS Code.
//!
//!   cargo vscode
//!
//! It needs Node.js with npm and VS Code's `code` command on the `PATH`.
//! Reload the VS Code window afterwards to run the new version.

use std::path::Path;
use std::process::{Command, ExitCode};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("satex-vscode: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    if let Some(arg) = std::env::args().nth(1) {
        return Err(format!("unknown argument `{arg}`; usage: cargo vscode"));
    }
    let extension = Path::new(env!("CARGO_MANIFEST_DIR")).join("editors/vscode");
    for tool in ["npm", "code"] {
        which::which(tool).map_err(|_| format!("`{tool}` is not on the PATH"))?;
    }
    if !extension.join("node_modules").is_dir() {
        step(Command::new("npm").args(["ci", "--no-audit", "--no-fund"]).current_dir(&extension))?;
    }
    // `scripts/try.mjs`: the release build of satex, bundled into the VSIX,
    // which `code --install-extension` installs.
    step(Command::new("npm").args(["run", "try"]).current_dir(&extension))?;
    println!("satex-vscode: installed; reload the VS Code window (Developer: Reload Window) to use it");
    Ok(())
}

fn step(command: &mut Command) -> Result<(), String> {
    let status = command.status().map_err(|e| format!("cannot run {:?}: {e}", command.get_program()))?;
    status.success().then_some(()).ok_or_else(|| format!("{:?} failed ({status})", command.get_program()))
}
