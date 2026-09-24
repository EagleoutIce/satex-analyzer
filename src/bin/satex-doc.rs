//! satex-doc: generates README.md and doc/wiki/*.md from the templates in
//! doc/src, by running real `satex` commands and capturing their output.
//! This keeps the documentation honest: every shown command actually runs
//! against the sample document, instead of being typed by hand.
//!
//! Directive syntax, written as `{{...}}` inside a `doc/src/**/*.md.in`
//! template:
//!
//!   {{run: satex lint -f samples/paper.tex}}
//!       Runs the command with the `satex` built next to this binary
//!       (the leading `satex` word is replaced by that binary's path),
//!       captures its stdout, and emits a fenced code block whose first
//!       line is the command as written, prefixed with `$ `.
//!
//!   {{run(10): satex lint -f samples/paper.tex}}
//!       The same, but keeps at most 10 lines of output and appends a line
//!       saying how many more were left out.
//!
//!   {{file: satex.yaml}}
//!       Inlines that file's content in a fenced code block.
//!
//!   {{rules}}
//!       A Markdown table of the lint rules (code, category, severity,
//!       summary), from `satex lint --rules --format json`.
//!
//!   {{queries}}
//!       The list of query names, from `satex query --list`.
//!
//!   {{version}}
//!       The crate version, from `satex --version`.
//!
//! Run with `--check` to verify the generated files are up to date without
//! writing anything; exits non-zero if any file is stale. Used by
//! tests/docs.rs and in CI.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command;

const TEMPLATES: &[(&str, &str)] = &[
    ("doc/src/README.md.in", "README.md"),
    ("doc/src/wiki/lints.md.in", "doc/wiki/lints.md"),
    ("doc/src/wiki/queries.md.in", "doc/wiki/queries.md"),
    ("doc/src/wiki/configuration.md.in", "doc/wiki/configuration.md"),
    ("doc/src/wiki/lsp.md.in", "doc/wiki/lsp.md"),
];

fn main() {
    let check = std::env::args().skip(1).any(|arg| arg == "--check");
    if let Err(message) = run(check) {
        eprintln!("satex-doc: {message}");
        std::process::exit(1);
    }
}

/// The built binary to run: `SATEX_DOC_BIN`, or the `satex` cargo built
/// beside this one (`cargo run --release --features doc --bin satex-doc` builds both).
fn bin() -> String {
    if let Ok(bin) = std::env::var("SATEX_DOC_BIN") {
        return bin;
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.join(format!("satex{}", std::env::consts::EXE_SUFFIX))))
        .filter(|bin| bin.exists())
        .map_or_else(|| "satex".to_string(), |bin| bin.display().to_string())
}

fn run(check: bool) -> Result<(), String> {
    let bin = bin();
    if !Path::new(&bin).exists() {
        return Err(format!("{bin} not found; run `cargo build --release` first"));
    }
    // The first run against a cold cache reports the kernel as interpreted
    // rather than cached, so the generated text would differ from the next
    // run's.  One warm-up run settles it.
    let _ = Command::new(&bin).args(["cache", "-f", "samples/paper.tex"]).output();
    let mut stale = Vec::new();
    let mut written = Vec::new();
    for (src, dst) in TEMPLATES {
        let template = fs::read_to_string(src).map_err(|e| format!("{src}: {e}"))?;
        let generated = render(&template).map_err(|e| format!("{src}: {e}"))?;
        let existing = fs::read_to_string(dst).unwrap_or_default();
        if existing == generated {
            continue;
        }
        if check {
            stale.push(*dst);
            continue;
        }
        fs::write(dst, &generated).map_err(|e| format!("{dst}: {e}"))?;
        written.push(*dst);
    }
    if check {
        return if stale.is_empty() {
            Ok(())
        } else {
            Err(format!("out of date, run `satex-doc`: {}", stale.join(", ")))
        };
    }
    for path in &written {
        println!("wrote {path}");
    }
    if written.is_empty() {
        println!("up to date");
    }
    Ok(())
}

/// Replace every `{{directive}}` in `template` with its expansion, passing
/// everything else through unchanged.
fn render(template: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let end = after.find("}}").ok_or("unterminated `{{` directive")?;
        let directive = after[..end].trim();
        // Expansions are built line by line and so end with a trailing
        // newline; trim it so the template's own surrounding blank lines
        // control spacing, rather than adding to it.
        out.push_str(expand(directive)?.trim_end_matches('\n'));
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

fn expand(directive: &str) -> Result<String, String> {
    if directive == "rules" {
        return rules_table();
    }
    if directive == "queries" {
        return queries_list();
    }
    if directive == "version" {
        return version();
    }
    if let Some(path) = directive.strip_prefix("file:") {
        return inline_file(path.trim());
    }
    if let Some(command) = directive.strip_prefix("run:") {
        return run_command(command.trim(), None);
    }
    if let Some(rest) = directive.strip_prefix("run(") {
        let (count, command) =
            rest.split_once(')').ok_or_else(|| format!("bad directive: {{{{{directive}}}}}"))?;
        let count: usize = count
            .trim()
            .parse()
            .map_err(|_| format!("bad line count in: {{{{{directive}}}}}"))?;
        let command = command
            .trim()
            .strip_prefix(':')
            .ok_or_else(|| format!("bad directive: {{{{{directive}}}}}"))?;
        return run_command(command.trim(), Some(count));
    }
    Err(format!("unknown directive: {{{{{directive}}}}}"))
}

/// Run `command` (whose first word must be `satex`) with the built binary,
/// and render its stdout as a fenced code block. `limit` caps the number of
/// output lines kept.
fn run_command(command: &str, limit: Option<usize>) -> Result<String, String> {
    let words = split_words(command);
    let (first, args) = words.split_first().ok_or("empty command")?;
    if first != "satex" {
        return Err(format!("command must start with `satex`: {command}"));
    }
    let output = Command::new(bin())
        .args(args)
        .output()
        .map_err(|e| format!("running `{command}`: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "`{command}` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // `satex 0.1.0, built ⟨time⟩`: the time changes with every build, so
    // the generated text would never be up to date.
    let mut lines: Vec<&str> = stdout
        .lines()
        .map(|line| match (line.starts_with("satex "), line.find(", built ")) {
            (true, Some(at)) => &line[..at],
            _ => line,
        })
        .collect();
    let omitted = match limit {
        Some(n) if lines.len() > n => {
            let omitted = lines.len() - n;
            lines.truncate(n);
            Some(omitted)
        }
        _ => None,
    };
    let mut block = String::new();
    let _ = writeln!(block, "```text");
    let _ = writeln!(block, "$ {command}");
    for line in &lines {
        let _ = writeln!(block, "{line}");
    }
    if let Some(omitted) = omitted {
        let _ = writeln!(block, "… {omitted} more line{} omitted", if omitted == 1 { "" } else { "s" });
    }
    let _ = writeln!(block, "```");
    Ok(block)
}

/// Minimal shell-like word splitting: whitespace separated, with single or
/// double quotes grouping a word that itself may contain whitespace. No
/// escaping, which every template command here can do without.
fn split_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match c {
            ' ' | '\t' => {
                if in_word {
                    words.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            '\'' | '"' => {
                in_word = true;
                for next in chars.by_ref() {
                    if next == c {
                        break;
                    }
                    current.push(next);
                }
            }
            _ => {
                in_word = true;
                current.push(c);
            }
        }
    }
    if in_word {
        words.push(current);
    }
    words
}

fn inline_file(path: &str) -> Result<String, String> {
    let content = fs::read_to_string(path).map_err(|_| format!("missing file: {path}"))?;
    let lang = Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("");
    let mut block = String::new();
    let _ = writeln!(block, "```{lang}");
    block.push_str(&content);
    if !content.ends_with('\n') {
        block.push('\n');
    }
    let _ = writeln!(block, "```");
    Ok(block)
}

fn rules_table() -> Result<String, String> {
    let output = Command::new(bin())
        .args(["lint", "--rules", "--format", "json"])
        .output()
        .map_err(|e| format!("running `satex lint --rules`: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "`satex lint --rules` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let rules: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout)
        .map_err(|e| format!("parsing `satex lint --rules --format json`: {e}"))?;
    let mut out = String::new();
    out.push_str("| code | category | severity | summary |\n");
    out.push_str("| --- | --- | --- | --- |\n");
    for rule in &rules {
        let code = rule["code"].as_str().unwrap_or("");
        let category = rule["category"].as_str().unwrap_or("");
        let severity = rule["severity"].as_str().unwrap_or("");
        let summary = rule["message"].as_str().unwrap_or("");
        let _ = writeln!(out, "| `{code}` | {category} | {severity} | {summary} |");
    }
    Ok(out)
}

fn queries_list() -> Result<String, String> {
    let output = Command::new(bin())
        .args(["query", "--list"])
        .output()
        .map_err(|e| format!("running `satex query --list`: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "`satex query --list` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut out = String::new();
    for name in String::from_utf8_lossy(&output.stdout).lines().map(str::trim) {
        // The listing's header row is not a query.
        if !name.is_empty() && name != "name" {
            let _ = writeln!(out, "- [{name}](#{name})");
        }
    }
    Ok(out)
}

fn version() -> Result<String, String> {
    let output = Command::new(bin())
        .arg("--version")
        .output()
        .map_err(|e| format!("running `satex --version`: {e}"))?;
    if !output.status.success() {
        return Err("`satex --version` failed".to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    // `satex 0.1.0, built …`: the word after the name, without its comma.
    text.split_whitespace()
        .nth(1)
        .map(|word| word.trim_end_matches(',').to_string())
        .ok_or_else(|| "could not parse `satex --version` output".to_string())
}
