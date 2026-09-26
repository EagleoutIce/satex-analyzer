//! Every command should honor `--format`, not just the ones that happened to
//! route through the shared renderer already: `--format json` must parse for
//! every subcommand, and a format a command has no rows for (`--format dot`
//! on `lint`, say) must fail clearly rather than silently falling back to
//! text.

use std::process::{Command, Output};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_satex")
}

fn run(args: &[&str]) -> Output {
    Command::new(bin())
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap_or_else(|e| panic!("failed to run satex {args:?}: {e}"))
}

const SAMPLE: &str = "tests/fixtures/project/paper.tex";

/// Every subcommand, with the flags it needs against the sample document,
/// given `--format json`: stdout must be one valid JSON value.
#[test]
fn every_command_honors_format_json() {
    let commands: &[&[&str]] = &[
        &["query", "definitions", "-f", SAMPLE],
        &["query", "--list"],
        &["trace", "-f", SAMPLE],
        &["lint", "-f", SAMPLE],
        &["lint", "--rules"],
        &["lint", "-f", SAMPLE, "--explain", "unguarded-recursion"],
        &["summary", "-f", SAMPLE],
        &["scope", "-f", SAMPLE],
        &["explain", "-f", SAMPLE, "\\usepackage"],
        &["slice", "-f", SAMPLE, "\\ifdraft"],
        &["controls", "-f", SAMPLE],
        &["dependencies", "-f", SAMPLE],
        &["cache", "-f", SAMPLE],
        &["tokens", "-f", SAMPLE],
        &["--version"],
    ];
    for args in commands {
        let mut full: Vec<&str> = args.to_vec();
        full.push("--format");
        full.push("json");
        let output = run(&full);
        assert!(output.status.success(), "{full:?} failed: {}", String::from_utf8_lossy(&output.stderr));
        let stdout = String::from_utf8_lossy(&output.stdout);
        serde_json::from_str::<serde_json::Value>(&stdout)
            .unwrap_or_else(|e| panic!("{full:?} did not print valid JSON ({e}):\n{stdout}"));
    }
}

/// A format that genuinely doesn't apply to a command errors clearly instead
/// of silently printing text (or something else) anyway.
#[test]
fn inapplicable_formats_error_clearly() {
    let cases: &[&[&str]] = &[
        &["tokens", "-f", SAMPLE, "--format", "sarif"],
        &["tokens", "-f", SAMPLE, "--format", "dot"],
        &["summary", "-f", SAMPLE, "--format", "csv"],
        &["summary", "-f", SAMPLE, "--format", "sarif"],
        &["cache", "-f", SAMPLE, "--format", "markdown"],
        &["dependencies", "-f", SAMPLE, "--format", "csv"],
        &["dependencies", "-f", SAMPLE, "--format", "github"],
        &["--version", "--format", "csv"],
        &["lint", "-f", SAMPLE, "--format", "dot"],
        &["lint", "-f", SAMPLE, "--explain", "unguarded-recursion", "--format", "csv"],
        &["slice", "-f", SAMPLE, "\\ifdraft", "--format", "dot"],
    ];
    for args in cases {
        let output = run(args);
        assert!(!output.status.success(), "{args:?} unexpectedly succeeded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("does not apply"), "{args:?}: expected a clear format error, got: {stderr}");
    }
}

/// `--format github`/`--format sarif` are for diagnostics: `lint` takes
/// them as well as `--format json` (covered above).
#[test]
fn diagnostics_formats_are_accepted_by_lint() {
    for command in ["lint"] {
        for format in ["github", "sarif"] {
            let output = run(&[command, "-f", SAMPLE, "--format", format]);
            assert!(
                output.status.success(),
                "{command} --format {format} failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
