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
//!   {{primitives}}
//!       Every TeX82/eTeX/pdfTeX/XeTeX/LuaTeX primitive `src/builtins.rs`
//!       registers, sectioned by the engine that adds it, with the verdict
//!       `src/builtins.rs::verdict` gives from what `src/exec.rs`,
//!       `src/mode.rs` and `src/commands.rs` do with it. Each row links
//!       to the deciding arm, from the `file!()`/`line!()` that arm
//!       records in `builtins::Site`.
//!
//!   {{limits}}
//!       A table of `config::Limits`' fields, their `Default` values and
//!       their doc comments, so a tuned or added limit cannot go stale in
//!       the docs.
//!
//! Run with `--check` to verify the generated files are up to date without
//! writing anything; exits non-zero if any file is stale. Used by
//! tests/docs.rs and in CI.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command;

use regex::Regex;
use satex::builtins::{self, Primitive, Site, Verdict};
use satex::config::{Engine, Limits};
use satex::lint::Source;
use satex::tex::{Interner, Meaning};

const TEMPLATES: &[(&str, &str)] = &[
    ("doc/src/README.md.in", "README.md"),
    ("doc/src/wiki/lints.md.in", "doc/wiki/lints.md"),
    ("doc/src/wiki/queries.md.in", "doc/wiki/queries.md"),
    ("doc/src/wiki/configuration.md.in", "doc/wiki/configuration.md"),
    ("doc/src/wiki/lsp.md.in", "doc/wiki/lsp.md"),
    ("doc/src/wiki/domains.md.in", "doc/wiki/domains.md"),
    ("doc/src/chktex-coverage.md.in", "doc/chktex-coverage.md"),
    ("doc/src/wiki/primitive-coverage.md.in", "doc/wiki/primitive-coverage.md"),
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

/// The file recording which installation the checked-in output was generated
/// from.
const STAMP: &str = "doc/src/.installation";

/// What the samples' output depends on besides satex itself: the kernel and
/// the packages they load, which every TeX Live has its own versions of.
/// `satex summary` reports the first line, the loaded files the rest.
fn installation(bin: &str) -> Result<String, String> {
    let out = Command::new(bin)
        .args(["summary", "--all", "-f", "samples/paper.tex"])
        .output()
        .map_err(|e| format!("running `satex summary`: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout);
    let kernel = text
        .lines()
        .find_map(|line| line.trim().strip_prefix("installation "))
        .ok_or("`satex summary` reported no installation")?;
    let files: Vec<&str> = text.lines().filter(|line| line.contains("/texmf")).collect();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in files.concat().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    let kernel = kernel.split(", ").take(2).collect::<Vec<_>>().join(", ");
    Ok(format!("{kernel}, files {hash:016x}\n"))
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
    // Kernel line numbers, package versions and installed paths all reach the
    // generated text, so output from another TeX Live says nothing about
    // whether the checked-in files follow their templates.
    let here = installation(&bin)?;
    let generated_with = fs::read_to_string(STAMP).unwrap_or_default();
    if check && !generated_with.is_empty() && generated_with != here {
        println!("skipping: generated with {}, this installation is {}", generated_with.trim(), here.trim());
        return Ok(());
    }
    let mut stale = Vec::new();
    let mut written = Vec::new();
    for (src, dst) in TEMPLATES {
        let template = fs::read_to_string(src).map_err(|e| format!("{src}: {e}"))?;
        let generated = render(&template).map_err(|e| format!("{src}: {e}"))?;
        let generated = linkify_citations(&generated);
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
    if fs::read_to_string(STAMP).unwrap_or_default() != here {
        fs::write(STAMP, &here).map_err(|e| format!("{STAMP}: {e}"))?;
        written.push(STAMP);
    }
    for path in &written {
        println!("wrote {path}");
    }
    if written.is_empty() {
        println!("up to date");
    }
    Ok(())
}

/// One name a citation in the generated docs may use for a [`Source`], and
/// the URL that name resolves to (always `Source::url()`, never a URL typed
/// here by hand). `etex_url`/`xetex_url` cover the two names the docs cite
/// that have no `Source` variant at all: they reuse, verbatim, the URLs
/// `doc/src/wiki/primitive-coverage.md.in` already cited by hand for them.
struct Alias {
    name: &'static str,
    url: String,
    /// Whether the bare name, with no section or quoted title after it, is
    /// still unambiguously a citation. `tex.web` and `pdfTeX manual` only
    /// ever name the document; `xcolor` and `chktex` are also ordinary
    /// words this repo uses on their own (a colour, a linter), so those
    /// need a section or quote after them before they are worth a link.
    always: bool,
}

/// eTeX has no [`Source`] variant of its own (nothing in `src/lint/mod.rs`
/// follows it directly), so its manual's URL is kept here, matching the one
/// `doc/src/wiki/primitive-coverage.md.in` already cites by hand.
const ETEX_MANUAL_URL: &str = "https://mirror.ctan.org/systems/e-tex/base/doc/etex_man.pdf";
/// Same reasoning as [`ETEX_MANUAL_URL`], for the XeTeX reference.
const XETEX_REFERENCE_URL: &str = "https://tug.org/xetex/";

/// Every name the generated docs use for a source, built from
/// [`Source::as_str`]/[`Source::url`] so the URL always comes from that one
/// place. A source gets more than one entry here when the docs spell it
/// more than one way (`pdftex` inline, `pdfTeX manual` in a citation).
fn source_aliases() -> Vec<Alias> {
    let known: &[(&str, Source, bool)] = &[
        ("tex.web", Source::TeXWeb, true),
        ("The TeXbook", Source::TeXbook, true),
        ("source2e", Source::Source2e, true),
        ("fntguide", Source::Fntguide, true),
        ("usrguide", Source::Usrguide, true),
        ("clsguide", Source::Clsguide, true),
        ("interface3", Source::Interface3, true),
        ("l2tabu", Source::L2tabu, true),
        ("The LaTeX Companion", Source::Companion, true),
        ("siunitx", Source::Siunitx, false),
        ("microtype", Source::Microtype, false),
        ("babel", Source::Babel, false),
        ("biblatex", Source::Biblatex, false),
        ("amsmath", Source::Amsmath, false),
        ("hyperref", Source::Hyperref, false),
        ("latexmk", Source::Latexmk, false),
        ("arara", Source::Arara, false),
        ("glossaries", Source::Glossaries, false),
        ("graphics", Source::Graphics, false),
        ("btxdoc", Source::Bibtex, true),
        ("lmodern", Source::Lmodern, false),
        ("pdftex", Source::Pdftex, false),
        ("pdfTeX manual", Source::Pdftex, true),
        ("luatex", Source::Luatex, false),
        ("LuaTeX manual", Source::Luatex, true),
        ("PDF 32000", Source::Pdf, true),
        ("SI brochure", Source::SiBrochure, true),
        ("TeX Live manual", Source::TexLive, true),
        ("MiKTeX manual", Source::MikTeX, true),
        ("xcolor", Source::Xcolor, false),
        ("chktex", Source::Chktex, false),
        ("ChkTeX", Source::Chktex, true),
        ("csquotes", Source::Csquotes, false),
    ];
    let mut aliases: Vec<Alias> = known
        .iter()
        .map(|(name, source, always)| Alias { name, url: source.url().to_string(), always: *always })
        .collect();
    aliases.push(Alias { name: "eTeX manual", url: ETEX_MANUAL_URL.to_string(), always: true });
    aliases.push(Alias { name: "etex_man", url: ETEX_MANUAL_URL.to_string(), always: true });
    aliases.push(Alias { name: "XeTeX reference", url: XETEX_REFERENCE_URL.to_string(), always: true });
    aliases
}

/// The `url` an already-rendered `[label](url)` markdown link points to.
fn link_href(link: &str) -> Option<&str> {
    let open = link.find("](")?;
    link[open + 2..].strip_suffix(')')
}

/// Turns a citation of a [`Source`] into a markdown link, over the fully
/// rendered text of one page, so a reader can check a claim against the
/// actual page instead of trusting satex's word for it. Recognizes:
///
/// - `tex.web § 211`, and the double-section mark for more than one:
///   `tex.web §§ 1211, 1`, `tex.web §§ 448–460`, `tex.web §§ 232 and 344`.
/// - a quoted section title instead of a number: `LuaTeX manual, "Macros"`.
/// - a versioned source: `ChkTeX 1.7.9 manual, § 8`.
/// - a bare source name with no section at all (only for a name in
///   [`Alias`] that is unambiguously a citation on its own, `always: true`):
///   `Source: the LuaTeX manual.`
/// - a section mark with no source name of its own, read as citing
///   whichever source was last named: `hpack (tex.web § 649) and vpack
///   (§ 668)`.
///
/// Leaves an existing markdown link and an inline code span exactly as they
/// are, and never opens a second link inside one already there. A fenced
/// code block is skipped outright: a `§` inside shown command output is not
/// a citation. The "last named source" resets at every heading, so a stray
/// `§` cannot ever be attributed to a citation several sections back.
fn linkify_citations(markdown: &str) -> String {
    let aliases = source_aliases();
    let mut by_len: Vec<&Alias> = aliases.iter().collect();
    by_len.sort_by_key(|a| std::cmp::Reverse(a.name.len()));
    let alias_pattern = by_len.iter().map(|a| regex::escape(a.name)).collect::<Vec<_>>().join("|");

    const NUM: &str = r"[0-9]+(?:\.[0-9]+)*";
    let range = format!(r"{NUM}(?:\s*[-\u{{2013}}]\s*{NUM})?");
    let list = format!(r"{range}(?:\s*(?:,|and)\s*{range})*");
    let word = r#"[^\s,)."]+"#;
    let quoted = r#""[^"]+""#;
    let cite = format!(r#"(?:,?\s*§§?\s*(?:{list}|{word}))|(?:,\s*{quoted})"#);
    let master = Regex::new(&format!(
        r#"(?P<link>\[[^\]]*\]\([^)]*\))|(?P<code>`[^`]*`)|\b(?P<alias>{alias_pattern})\b(?P<qual>(?:\s+[0-9][0-9.]*)?(?:\s+(?:manual|reference))?)(?P<cite>{cite})?|(?P<bare>§§?\s*(?:{list}|{word}))"#
    ))
    .expect("linkify_citations: invalid citation regex");

    let mut out = String::with_capacity(markdown.len());
    let mut prose = String::new();
    let mut last_url: Option<String> = None;
    let mut in_fence = false;
    let mut is_run_fence = false;
    let mut fence_content = String::new();
    for line in markdown.split_inclusive('\n') {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") {
            out.push_str(&linkify_prose(&prose, &master, &aliases, &mut last_url));
            prose.clear();
            if in_fence {
                // Closing fence: the opening line (whatever its language tag
                // was) has already been written, so only the content and
                // the real closing line remain. A run transcript gets its
                // `§`-style citations linked; anything else passes through
                // byte for byte.
                if is_run_fence {
                    let linkified = linkify_prose(&fence_content, &master, &aliases, &mut last_url);
                    out.push_str(&linkified);
                } else {
                    out.push_str(&fence_content);
                }
                out.push_str(line);
                fence_content.clear();
                in_fence = false;
                is_run_fence = false;
            } else {
                // Opening fence
                out.push_str(line);
                in_fence = true;
            }
            continue;
        }
        if in_fence {
            // Check if this is the first line of a run fence
            if fence_content.is_empty() && trimmed.starts_with("$") && trimmed.contains("satex") {
                is_run_fence = true;
            }
            fence_content.push_str(line);
            continue;
        }
        if trimmed.starts_with('#') {
            out.push_str(&linkify_prose(&prose, &master, &aliases, &mut last_url));
            prose.clear();
            last_url = None;
        }
        prose.push_str(line);
    }
    out.push_str(&linkify_prose(&prose, &master, &aliases, &mut last_url));
    out
}

/// The citation-linking pass over one run of non-fenced markdown, threading
/// `last_url` through so a bare `§ N` after a real citation resolves to the
/// same source. See [`linkify_citations`] for the forms this handles.
fn linkify_prose(text: &str, master: &Regex, aliases: &[Alias], last_url: &mut Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for caps in master.captures_iter(text) {
        let m = caps.get(0).expect("regex match always has group 0");
        out.push_str(&text[last..m.start()]);
        last = m.end();
        if let Some(link) = caps.name("link") {
            if let Some(url) = link_href(link.as_str()) {
                *last_url = Some(url.to_string());
            }
            out.push_str(link.as_str());
        } else if let Some(code) = caps.name("code") {
            out.push_str(code.as_str());
        } else if let Some(alias) = caps.name("alias") {
            let entry = aliases.iter().find(|a| a.name == alias.as_str()).expect("matched alias is in the table");
            if entry.always || caps.name("cite").is_some() {
                *last_url = Some(entry.url.clone());
                out.push_str(&format!("[{}]({})", m.as_str(), entry.url));
            } else {
                out.push_str(m.as_str());
            }
        } else if let Some(bare) = caps.name("bare") {
            match last_url {
                Some(url) => out.push_str(&format!("[{}]({url})", bare.as_str())),
                None => out.push_str(bare.as_str()),
            }
        }
    }
    out.push_str(&text[last..]);
    out
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
    if directive == "explanations" {
        return explanations();
    }
    if directive == "chktex" {
        return chktex_table();
    }
    if directive == "queries" {
        return queries_list();
    }
    if directive == "engines" {
        return enum_list(&Engine::ALL.iter().map(|e| e.as_str()).collect::<Vec<_>>());
    }
    if directive == "kernels" {
        return enum_list(&satex::plugin::Kernel::ALL.iter().map(|k| k.as_str()).collect::<Vec<_>>());
    }
    if directive == "outputs" {
        return enum_list(&satex::plugin::output::Output::ALL.iter().map(|o| o.as_str()).collect::<Vec<_>>());
    }
    if directive == "build-systems" {
        return enum_list(&satex::plugin::build::BuildSystem::ALL.iter().map(|b| b.as_str()).collect::<Vec<_>>());
    }
    if directive == "version" {
        return version();
    }
    if directive == "primitives" {
        return primitives_table();
    }
    if directive == "limits" {
        return limits_table();
    }
    if let Some(path) = directive.strip_prefix("file:") {
        return inline_file(path.trim());
    }
    if let Some(command) = directive.strip_prefix("run:") {
        return run_command(command.trim(), None);
    }
    if let Some(rest) = directive.strip_prefix("run(") {
        let (count, command) = rest.split_once(')').ok_or_else(|| format!("bad directive: {{{{{directive}}}}}"))?;
        let count: usize = count.trim().parse().map_err(|_| format!("bad line count in: {{{{{directive}}}}}"))?;
        let command = command.trim().strip_prefix(':').ok_or_else(|| format!("bad directive: {{{{{directive}}}}}"))?;
        return run_command(command.trim(), Some(count));
    }
    Err(format!("unknown directive: {{{{{directive}}}}}"))
}

/// A `{{run: ...}}` transcript is real `satex` output against this
/// checkout, on this machine, so it genuinely contains this machine's
/// absolute paths: where the checkout sits, `$HOME`, the installation's
/// personal and per-year texmf trees. None of that is the same for another
/// reader, so it is rewritten to something everyone sees alike before it is
/// embedded: the checkout's own path becomes relative, the way the rest of
/// these docs already refer to `samples/paper.tex`; `$HOME` becomes `~`;
/// and a TeX Live year, which would otherwise date the page to whichever
/// installation generated it, becomes `YYYY`. A path that is the same on
/// every installation (`/usr/local/texlive/texmf-local`, with no year) is
/// left alone.
fn redact_paths(line: &str) -> String {
    let mut out = line.to_string();
    // The checkout's own absolute path: strip its prefix so what remains
    // reads the same as every other repo-relative path already in these
    // docs (`samples/paper.tex:12`, not `/home/.../satex/samples/...`).
    if let Ok(cwd) = std::env::current_dir() {
        let prefix = format!("{}/", cwd.display());
        out = out.replace(&prefix, "");
    }
    // Anything still under `$HOME` (a personal texmf tree, a per-user
    // texlive var directory) is this machine's alone.
    if let Some(home) = dirs::home_dir() {
        let home = home.display().to_string();
        if !home.is_empty() {
            out = out.replace(&home, "~");
        }
    }
    // `/usr/local/texlive/2026/...` and `.texlive2026` both name the year
    // of the installation that generated the page; a reader still sees
    // that there is a versioned texlive tree, just not which year.
    let year = Regex::new(r"(texlive/?)20\d{2}").expect("redact_paths: invalid year regex");
    year.replace_all(&out, "${1}YYYY").into_owned()
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
    let output = Command::new(bin()).args(args).output().map_err(|e| format!("running `{command}`: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "`{command}` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines: Vec<String> = stdout
        .lines()
        .map(|line| match line.starts_with("satex ") {
            true => line.find(" (").or_else(|| line.find(", built ")).map_or(line, |at| &line[..at]),
            false => line,
        })
        .map(redact_paths)
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

/// The rule list as a table, every code linked to its own section below.
fn rules_table() -> Result<String, String> {
    let rules = lint_json(&["--rules", "--format", "json"])?;
    let mut out = String::new();
    out.push_str("| code | category | severity | summary |\n");
    out.push_str("| --- | --- | --- | --- |\n");
    for rule in &rules {
        let code = rule["code"].as_str().unwrap_or("");
        let category = rule["category"].as_str().unwrap_or("");
        let severity = rule["severity"].as_str().unwrap_or("");
        let summary = rule["message"].as_str().unwrap_or("");
        let _ = writeln!(out, "| [`{code}`](#{code}) | {category} | {severity} | {summary} |");
    }
    Ok(out)
}

/// The JSON `satex lint ARGS` prints.
fn lint_json(args: &[&str]) -> Result<Vec<serde_json::Value>, String> {
    let output =
        Command::new(bin()).arg("lint").args(args).output().map_err(|e| format!("running `satex lint`: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "`satex lint {}` exited with {}: {}",
            args.join(" "),
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("parsing `satex lint {}`: {e}", args.join(" ")))
}

/// `satex lint --explain CODE --format json`: one rule's whole record.
fn explain_json(code: &str) -> Result<serde_json::Value, String> {
    let command = format!("satex lint --explain {code} --format json");
    let output = Command::new(bin())
        .args(["lint", "--explain", code, "--format", "json"])
        .output()
        .map_err(|e| format!("running `{command}`: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "`{command}` exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice(&output.stdout).map_err(|e| format!("parsing `{command}`: {e}"))
}

/// One collapsible group per category, one section per rule inside it. The
/// categories and the rules both come from the binary's own rule list, so a
/// new rule or a new category needs no edit here.
fn explanations() -> Result<String, String> {
    let rules = lint_json(&["--rules", "--format", "json"])?;
    let mut out = String::new();
    let mut open: Option<String> = None;
    for rule in &rules {
        let code = rule["code"].as_str().unwrap_or("");
        let category = rule["category"].as_str().unwrap_or("");
        if open.as_deref() != Some(category) {
            if open.is_some() {
                out.push_str("</details>\n\n");
            }
            let count = rules.iter().filter(|r| r["category"].as_str() == Some(category)).count();
            let plural = if count == 1 { "rule" } else { "rules" };
            let _ = writeln!(out, "<details>");
            // GitHub renders markdown inside `<details>` only when a blank
            // line follows the `<summary>`, so the headings below need it.
            let _ = writeln!(out, "<summary><b>{category}</b> — {count} {plural}</summary>\n");
            open = Some(category.to_string());
        }
        out.push_str(&explain_section(code)?);
    }
    if open.is_some() {
        out.push_str("</details>\n");
    }
    Ok(out)
}

/// One rule's section: a heading carrying its anchor, the label and summary
/// `--explain` prints above the text, the explanation as markdown, and the
/// works it follows. Everything comes from `--explain CODE --format json`,
/// so nothing here is written twice.
fn explain_section(code: &str) -> Result<String, String> {
    let rule = explain_json(code)?;
    let category = rule["category"].as_str().unwrap_or("");
    let severity = rule["severity"].as_str().unwrap_or("");
    let summary = rule["summary"].as_str().unwrap_or("");
    let mut out = String::new();
    let _ = writeln!(out, "### `{code}`\n");
    let _ = writeln!(out, "**{category} / {severity}** — {summary}\n");
    out.push_str(&explanation_markdown(rule["explanation"].as_str().unwrap_or("")));
    let sources: Vec<String> = rule["references"]
        .as_array()
        .map(|refs| {
            refs.iter()
                .map(|r| format!("[{}]({})", r["cite"].as_str().unwrap_or(""), r["url"].as_str().unwrap_or("")))
                .collect()
        })
        .unwrap_or_default();
    if !sources.is_empty() {
        let _ = writeln!(out, "\nSee {}.", sources.join(", "));
    }
    out.push('\n');
    Ok(out)
}

/// A rule's explanation as markdown. The text is already written in it —
/// code spans, links and `*…*` emphasis — but hard-wrapped, so the wraps
/// come out and an indented example becomes a fenced block. Nothing is
/// added to what the rule says.
fn explanation_markdown(text: &str) -> String {
    let mut out = String::new();
    for block in text.split("\n\n") {
        let block = block.trim_matches('\n');
        if block.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        if block.lines().all(|line| line.trim().is_empty() || line.starts_with("    ")) {
            let body: Vec<&str> = block.lines().map(|line| line.strip_prefix("    ").unwrap_or(line)).collect();
            let _ = writeln!(out, "```{}", fence_language(&body));
            for line in &body {
                let _ = writeln!(out, "{line}");
            }
            let _ = writeln!(out, "```");
        } else {
            let _ = writeln!(out, "{}", unwrap_paragraph(block));
        }
    }
    out
}

/// What an indented example is written in: `latex` once a control sequence
/// appears in it, plain text otherwise.
fn fence_language(body: &[&str]) -> &'static str {
    for line in body {
        let mut chars = line.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' && chars.peek().is_some_and(char::is_ascii_alphabetic) {
                return "latex";
            }
        }
    }
    "text"
}

/// One hard-wrapped paragraph as a single line. Runs of spaces collapse,
/// except inside a code span, where a space may be meant.
fn unwrap_paragraph(block: &str) -> String {
    let joined = block.lines().map(str::trim).collect::<Vec<_>>().join(" ");
    let mut out = String::with_capacity(joined.len());
    let mut code = false;
    let mut chars = joined.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '`' {
            code = !code;
        } else if c == ' ' && !code {
            while chars.peek() == Some(&' ') {
                chars.next();
            }
        }
        out.push(c);
    }
    out
}

/// Whether a lint rule fires by default: `satex.yaml`'s own `lint_off`
/// (what a `document` input, the baseline profile, gets), and the other
/// profiles (`package`, `class`, `literate`, `plain`) whose own `lint_off`
/// disagrees with that baseline. A rule several of those turn off but
/// `document` still fires is not fully "on"; a rule `document` itself
/// leaves off is just "off", full stop, regardless of the rest.
struct RuleDefault {
    off_by_default: bool,
    /// Non-`document` profiles that flip the rule the other way from the
    /// `document` baseline: off when `off_by_default` is false, on when it
    /// is true. Empty when every profile agrees with `document`.
    disagreeing_profiles: Vec<&'static str>,
}

/// One [`RuleDefault`] per rule code, computed from `Config::default()`
/// merged with `satex.yaml` and every `profiles.*` overlay applied on top
/// of it in turn ([`satex::config::Config::apply_profile`]) — the same
/// merge a real run does — so a rule's on/off state can never drift from
/// what `satex lint` actually does with it.
fn rule_defaults() -> Result<std::collections::HashMap<&'static str, RuleDefault>, String> {
    use satex::config::Profile;
    let mut base = satex::Config::default();
    base.merge(Path::new("satex.yaml")).map_err(|e| format!("satex.yaml: {e}"))?;
    let mut off_by_profile: Vec<(Profile, Vec<String>)> = Vec::new();
    for profile in Profile::ALL {
        let mut cfg = base.clone();
        cfg.apply_profile(*profile).map_err(|e| format!("profiles.{}: {e}", profile.as_str()))?;
        off_by_profile.push((*profile, cfg.lint_off));
    }
    let document_off: std::collections::HashSet<&str> = off_by_profile
        .iter()
        .find(|(p, _)| *p == Profile::Document)
        .map(|(_, off)| off.iter().map(String::as_str).collect())
        .unwrap_or_default();

    let mut result = std::collections::HashMap::new();
    for rule in satex::lint::all() {
        let off_by_default = document_off.contains(rule.code);
        let mut disagreeing_profiles = Vec::new();
        for (profile, off) in &off_by_profile {
            if *profile == Profile::Document {
                continue;
            }
            let off_here = off.iter().any(|c| c == rule.code);
            if off_here != off_by_default {
                disagreeing_profiles.push(profile.as_str());
            }
        }
        result.insert(rule.code, RuleDefault { off_by_default, disagreeing_profiles });
    }
    Ok(result)
}

/// `RuleDefault` as one short cell: `no` when `document` itself leaves the
/// rule off, `yes` when every profile agrees it fires, and `yes` plus which
/// profiles turn it back off otherwise — several typographic rules are
/// document-only this way, silent for `package`, `class` and `literate`.
fn satex_default_cell(default: &RuleDefault) -> String {
    if default.off_by_default {
        return "no".to_string();
    }
    if default.disagreeing_profiles.is_empty() {
        return "yes".to_string();
    }
    format!("yes, off for {}", default.disagreeing_profiles.join("/"))
}

/// chktex's warnings against the rules that cover them, and, distinctly
/// from whether chktex itself reports each one (`on`/`off`, chktex's own
/// wording), whether satex's covering rule fires by default (`yes`/`no`,
/// so the two columns are never mistaken for each other at a glance).
fn chktex_table() -> Result<String, String> {
    let rows = lint_json(&["--chktex", "--format", "json"])?;
    let defaults = rule_defaults()?;
    let mut out = String::new();
    out.push_str("| # | chktex reports | chktex enables it | satex rule | satex enables it |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for row in &rows {
        let number = row["warning"].as_u64().unwrap_or(0);
        let message = row["message"].as_str().unwrap_or("").replace('|', "\\|");
        let default = row["default"].as_str().unwrap_or("");
        let rules = row["rules"].as_str().unwrap_or("");
        let (covered, satex_default) = match rules.is_empty() {
            true => ("—".to_string(), "—".to_string()),
            false => {
                let codes: Vec<&str> = rules.split(", ").collect();
                let covered = codes.iter().map(|c| format!("[`{c}`](Lints#{c})")).collect::<Vec<_>>().join(", ");
                let satex_default = match codes.as_slice() {
                    [code] => defaults.get(code).map_or("?".to_string(), satex_default_cell),
                    codes => codes
                        .iter()
                        .map(|c| format!("`{c}`: {}", defaults.get(c).map_or("?".to_string(), satex_default_cell)))
                        .collect::<Vec<_>>()
                        .join("; "),
                };
                (covered, satex_default)
            }
        };
        let _ = writeln!(out, "| {number} | {message} | {default} | {covered} | {satex_default} |");
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
    let output =
        Command::new(bin()).arg("--version").output().map_err(|e| format!("running `satex --version`: {e}"))?;
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

fn display_primitive(name: &str) -> String {
    match name {
        " " => "`\\ ` (control space)".to_string(),
        _ => format!("`\\{name}`"),
    }
}

/// Every primitive `src/builtins.rs` registers, sectioned by the engine
/// block that adds it, with `builtins::verdict`'s judgment on each. Runs
/// `initial_meanings` in-process, so a renamed or added primitive cannot
/// leave this page stale.
fn primitives_table() -> Result<String, String> {
    let mut it = Interner::default();
    let core_table = builtins::initial_meanings(&mut it, Engine::Tex);
    let pdftex_table = builtins::initial_meanings(&mut it, Engine::PdfTeX);
    let xetex_table = builtins::initial_meanings(&mut it, Engine::XeTeX);
    let luatex_table = builtins::initial_meanings(&mut it, Engine::LuaTeX);

    let prim_map =
        |table: &std::collections::HashMap<satex::tex::Sym, Meaning>| -> std::collections::BTreeMap<String, Primitive> {
            table.iter().filter_map(|(s, m)| m.prim().map(|p| (it.name(*s).to_string(), p))).collect()
        };
    let core = prim_map(&core_table);
    let pdftex_all = prim_map(&pdftex_table);
    let xetex_all = prim_map(&xetex_table);
    let luatex_all = prim_map(&luatex_table);

    // Each primitive is listed once, under the earliest engine that adds it:
    // pdfTeX's own extensions, then what XeTeX adds on top, then LuaTeX's
    // rest — the same order `Engine::has_*` layers them in `builtins.rs`.
    let pdftex_extra: std::collections::BTreeMap<String, Primitive> =
        pdftex_all.iter().filter(|(n, _)| !core.contains_key(*n)).map(|(n, p)| (n.clone(), *p)).collect();
    let xetex_extra: std::collections::BTreeMap<String, Primitive> = xetex_all
        .iter()
        .filter(|(n, _)| !core.contains_key(*n) && !pdftex_extra.contains_key(*n))
        .map(|(n, p)| (n.clone(), *p))
        .collect();
    let luatex_extra: std::collections::BTreeMap<String, Primitive> = luatex_all
        .iter()
        .filter(|(n, _)| !core.contains_key(*n) && !pdftex_extra.contains_key(*n) && !xetex_extra.contains_key(*n))
        .map(|(n, p)| (n.clone(), *p))
        .collect();

    let bugs: std::collections::HashMap<&str, &str> = builtins::known_bugs().iter().copied().collect();
    // Link label -> URL, so the same arm is defined once however many rows cite it.
    let mut sites: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    let (mut total, mut modeled) = (0usize, 0usize);
    let mut bug_notes = Vec::new();
    let mut out = String::new();

    let sections: [(&str, &str, &std::collections::BTreeMap<String, Primitive>); 4] = [
        (
            "TeX82 & eTeX",
            "tex.web parts 24-49. eTeX is in every format satex analyzes (`Engine::has_etex`), so its primitives are listed with TeX82's, not apart",
            &core,
        ),
        ("pdfTeX extensions", "the pdfTeX manual, \"Primitives\"", &pdftex_extra),
        ("XeTeX extensions", "the XeTeX reference", &xetex_extra),
        ("LuaTeX extensions", "the LuaTeX manual", &luatex_extra),
    ];
    for (heading, source, map) in sections {
        let _ = writeln!(out, "### {heading}\n");
        let _ = writeln!(out, "Source: {source}.\n");
        let _ = writeln!(out, "| primitive | modeled as | verdict | how |");
        let _ = writeln!(out, "| --- | --- | --- | --- |");
        for (name, p) in map {
            total += 1;
            let display = display_primitive(name);
            if let Some(reason) = bugs.get(name.as_str()) {
                bug_notes.push((name.clone(), (*reason).to_string()));
                let _ = writeln!(out, "| {display} | `{p:?}` | known bug | not modeled soundly, see below |");
                continue;
            }
            modeled += 1;
            let (verdict, detail, site) = builtins::verdict(*p);
            let verdict = match verdict {
                Verdict::Exact => "exact",
                Verdict::Approximated => "approximated",
                Verdict::Unsupported => "unsupported + diagnostic",
            };
            let label = source_ref(&site);
            sites.insert(label.clone(), source_url(&site));
            let _ = writeln!(out, "| {display} | [`{p:?}`][{label}] | {verdict} | {} |", detail.replace('|', "\\|"));
        }
        out.push('\n');
    }

    let _ = writeln!(out, "**{modeled} of {total} primitives modeled soundly.**");
    if !bug_notes.is_empty() {
        let _ = writeln!(out, "\nKnown bugs, tracked for a fix and not counted above:\n");
        for (name, reason) in &bug_notes {
            let _ = writeln!(out, "- `\\{name}`: {reason}.");
        }
    }
    out.push('\n');
    for (label, url) in &sites {
        let _ = writeln!(out, "[{label}]: {url}");
    }
    Ok(out)
}

/// The reference-link label for a [`Site`]: the file's stem and the line.
/// Every row citing the same arm shares one label, so the table stays short
/// and the URL is written once.
fn source_ref(site: &Site) -> String {
    let stem = Path::new(site.file).file_stem().map_or("src", |s| s.to_str().unwrap_or("src"));
    format!("{stem}-{}", site.line)
}

/// The GitHub URL of the arm that decides a verdict, from `file!()`/`line!()`
/// and `CARGO_PKG_REPOSITORY`. Pinned to `main`, not to `SATEX_COMMIT`: a
/// commit in the URL would make this page stale the moment it is committed,
/// and `tests/docs.rs` compares the checked-in output byte for byte.
fn source_url(site: &Site) -> String {
    let repo = env!("CARGO_PKG_REPOSITORY").trim_end_matches('/');
    let file = site.file.replace('\\', "/");
    format!("{repo}/blob/main/{file}#L{}", site.line)
}

/// `config::Limits`' fields, their `Default` value and their doc comment.
/// `limits.*` is described in full on the configuration page; every row
/// links there, so this table only has to give the number.
fn limits_table() -> Result<String, String> {
    let source = fs::read_to_string("src/config.rs").map_err(|e| format!("src/config.rs: {e}"))?;
    let fields = struct_doc_fields(&source, "Limits")?;
    let limits = Limits::default();
    let defaults = serde_json::to_value(&limits).map_err(|e| format!("serializing Limits::default(): {e}"))?;
    let defaults = defaults.as_object().ok_or("Limits did not serialize to a JSON object")?;

    let mut out = String::new();
    let _ = writeln!(out, "| setting | default | what it bounds |");
    let _ = writeln!(out, "| --- | --- | --- |");
    for (name, doc) in fields {
        // `ByteSize` prints as "4 GiB"; everything else round-trips through JSON.
        let value = match name.as_str() {
            "memory" => limits.memory.to_string(),
            "cache_size" => limits.cache_size.to_string(),
            _ => match defaults.get(&name) {
                Some(serde_json::Value::String(s)) => s.clone(),
                Some(other) => other.to_string(),
                None => "?".to_string(),
            },
        };
        let _ = writeln!(
            out,
            "| [`limits.{name}`](Configuration#what-a-run-may-use) | {value} | {} |",
            doc.replace('|', "\\|")
        );
    }
    Ok(out)
}

/// The `pub name: Type,` fields of `struct NAME { ... }` in `source`, each
/// with its preceding `///` doc comment joined into one line. A minimal
/// scan over one struct's block, not a Rust parser, good enough for the
/// plain field lists this repo writes.
fn struct_doc_fields(source: &str, name: &str) -> Result<Vec<(String, String)>, String> {
    let start = source.find(&format!("struct {name} {{")).ok_or_else(|| format!("no `struct {name}` in the source"))?;
    let body = &source[start..];
    let end = body.find("\n}").ok_or_else(|| format!("unterminated `struct {name}`"))?;
    let mut fields = Vec::new();
    let mut doc = String::new();
    for line in body[..end].lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("///") {
            if !doc.is_empty() {
                doc.push(' ');
            }
            doc.push_str(rest.trim());
        } else if let Some(rest) = line.strip_prefix("pub ")
            && let Some((field, _)) = rest.split_once(':')
        {
            fields.push((field.trim().to_string(), std::mem::take(&mut doc)));
        } else if !line.starts_with('#') && !line.is_empty() {
            doc.clear();
        }
    }
    Ok(fields)
}

/// Generate a comma-and-or-separated list of enum variants in backticks.
/// For example, `["a", "b", "c"]` becomes `` `a`, `b`, or `c` ``.
fn enum_list(variants: &[&str]) -> Result<String, String> {
    match variants {
        [] => Ok(String::new()),
        [one] => Ok(format!("`{one}`")),
        [first, second] => Ok(format!("`{first}` or `{second}`")),
        [first, middle @ .., last] => {
            let mut out = String::new();
            out.push_str(&format!("`{first}`"));
            for v in middle {
                out.push_str(&format!(", `{v}`"));
            }
            out.push_str(&format!(", or `{last}`"));
            Ok(out)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::linkify_citations;

    /// A citation inside a `{{run: ...}}` transcript ("prints `tex.web §
    /// 211` on success") must become a markdown link, exactly like the same
    /// citation would outside a fenced block. No checked-in transcript
    /// happens to contain one today, so this constructs the case directly.
    #[test]
    fn citation_inside_run_fence_is_linked() {
        let markdown = "\
Some prose.

```text
$ satex lint -f samples/paper.tex
warning: see tex.web § 211 for the rule
```
";
        let expected = "\
Some prose.

```text
$ satex lint -f samples/paper.tex
warning: see [tex.web § 211](https://mirrors.ctan.org/systems/knuth/dist/tex/tex.web) for the rule
```
";
        assert_eq!(linkify_citations(markdown), expected);
    }

    /// A citation-looking string inside a fence that is *not* a `{{run:
    /// ...}}` transcript (no `$ satex ...` first line) must pass through
    /// byte for byte: no linking, and the fence markers unchanged.
    #[test]
    fn non_run_fence_is_left_verbatim() {
        let markdown = "\
```yaml
# see tex.web § 211
depp:
  file: DEPENDS.txt
```
";
        let got = linkify_citations(markdown);
        assert_eq!(got, markdown, "a non-run fence must be rendered byte for byte unchanged");
    }
}
