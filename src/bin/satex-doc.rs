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
//!   {{example: doc/src/wiki/challenges/01-catcode.tex}}
//!       Inlines the file, then shows side by side the text pdflatex
//!       typesets (one run, random seed 7) and what `satex lint` reports.
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
//!   {{fields: STRUCT key}}
//!       One paragraph per field of `config::STRUCT`: its dotted key under
//!       `key`, its default from `Config::default()` and its doc comment,
//!       so a setting's explanation lives next to its code.
//!
//!   {{limits}}
//!       A table of `config::Limits`' fields, their `Default` values and
//!       their doc comments, so a tuned or added limit cannot go stale in
//!       the docs.
//!
//! Run with `--check` to verify the generated files are up to date without
//! writing anything; exits non-zero if any file is stale. Used by
//! `tests/it/docs.rs` and in CI.
//!
//! `--refresh-citations` / `--check-citations` keep the pinned tex.web,
//! etex.ch, latex2e and expl3 links current: a `tex.web § N` (or
//! `etex.ch \`identifier\``) citation names a module number or an
//! identifier, never a line, and this resolves it against the *live*
//! upstream file (the `trunk`/`main` branch, fetched fresh each run) to
//! build a commit-pinned permalink. `--refresh-citations` writes the
//! result (`doc/src/tex-web-sections.tsv`/`tex-web-sha.txt`, the build-time
//! cache `src/lint/mod.rs` embeds, and the `[tw-N]:`/`[ech-<name>]:`
//! definitions in SEMANTICS.md); `--check-citations` (CI) only reports
//! drift and fails if a citation no longer resolves, or resolves to a
//! different line than what is checked in. See [`refresh_citations`].

use std::collections::{BTreeMap, HashMap};
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;

use regex::Regex;
use satex::builtins::{self, Primitive, Site, Verdict};
use satex::config::{Engine, Limits};
use satex::lint::cite::linkify_citations;
use satex::tex::{Interner, Meaning};

const TEMPLATES: &[(&str, &str)] = &[
    ("doc/src/README.md.in", "README.md"),
    ("doc/src/wiki/lints.md.in", "doc/wiki/lints.md"),
    ("doc/src/wiki/queries.md.in", "doc/wiki/queries.md"),
    ("doc/src/wiki/configuration.md.in", "doc/wiki/configuration.md"),
    ("doc/src/wiki/lsp.md.in", "doc/wiki/lsp.md"),
    ("doc/src/wiki/domains.md.in", "doc/wiki/domains.md"),
    ("doc/src/wiki/challenges.md.in", "doc/wiki/challenges.md"),
    ("doc/src/chktex-coverage.md.in", "doc/chktex-coverage.md"),
    ("doc/src/wiki/primitive-coverage.md.in", "doc/wiki/primitive-coverage.md"),
];

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = if args.iter().any(|a| a == "--refresh-citations") {
        refresh_citations(false)
    } else if args.iter().any(|a| a == "--check-citations") {
        refresh_citations(true)
    } else {
        run(args.iter().any(|a| a == "--check"))
    };
    if let Err(message) = result {
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

// ---------------------------------------------------------------------
// Citation resolution: `tex.web § N` / `etex.ch \`identifier\`` and the
// LaTeX2e/expl3 kernel links resolve the *name* (a module number or an
// identifier, the source of truth kept in prose) against the current
// upstream commit, rather than a line number hand-typed once and left to
// rot. See the module doc comment for `--refresh-citations`/`--check-citations`.
// ---------------------------------------------------------------------

/// One upstream source these docs cite by module number (`tex.web`) or by
/// identifier (`etex.ch`), or without either (`expl3-code`/`latex.ltx`,
/// cited as a directory: LaTeX2e/expl3 are many `.dtx` files, not one
/// numbered source).
struct CiteSource {
    name: &'static str,
    repo: &'static str,
    git_ref: &'static str,
    path: &'static str,
    kind: CiteKind,
}

#[derive(PartialEq, Eq)]
enum CiteKind {
    /// Has WEB modules; `tw-N` citations resolve against it.
    Web,
    /// Addressed by identifier; `ech-<name>` citations resolve against it.
    Ident,
    /// A directory: no per-line citations, only the bare source name.
    Tree,
}

const CITE_SOURCES: &[CiteSource] = &[
    CiteSource {
        name: "tex.web",
        repo: "TeX-Live/texlive-source",
        git_ref: "trunk",
        path: "texk/web2c/tex.web",
        kind: CiteKind::Web,
    },
    CiteSource {
        name: "etex.ch",
        repo: "TeX-Live/texlive-source",
        git_ref: "trunk",
        path: "texk/web2c/etexdir/etex.ch",
        kind: CiteKind::Ident,
    },
    CiteSource { name: "expl3-code", repo: "latex3/latex3", git_ref: "main", path: "l3kernel", kind: CiteKind::Tree },
    CiteSource { name: "latex.ltx", repo: "latex3/latex2e", git_ref: "main", path: "base", kind: CiteKind::Tree },
];

/// A WEB module (what tex.web calls a "section") starts on a line
/// beginning with `@ ` or `@*` (also a bare `@` at end of line or
/// followed by a tab).
fn is_web_module_start(line: &str) -> bool {
    line == "@" || line.starts_with("@*") || line.starts_with("@ ") || line.starts_with("@\t")
}

/// `{module number: 1-based line it starts on}`, numbered from 1 in file order.
fn tex_web_sections(content: &str) -> BTreeMap<u32, u32> {
    let mut sections = BTreeMap::new();
    let mut n = 0u32;
    for (i, line) in content.lines().enumerate() {
        if is_web_module_start(line) {
            n += 1;
            sections.insert(n, i as u32 + 1);
        }
    }
    sections
}

/// The line `identifier` is defined on in a WEB source, preferring an
/// actual declaration (`function`/`procedure identifier`, `@d identifier=`,
/// `@!identifier:`) to an incidental mention; `None` if it does not appear
/// at all.
fn find_identifier_line(content: &str, identifier: &str) -> Option<u32> {
    let def = Regex::new(&format!(
        r"^\s*(?:(?:function|procedure)\s+{ident}\b|@d\s+{ident}\s*=|@!{ident}\s*:)",
        ident = regex::escape(identifier)
    ))
    .expect("valid regex");
    let any = Regex::new(&format!(r"\b{}\b", regex::escape(identifier))).expect("valid regex");
    let mut fallback = None;
    for (i, line) in content.lines().enumerate() {
        if def.is_match(line) {
            return Some(i as u32 + 1);
        }
        if fallback.is_none() && any.is_match(line) {
            fallback = Some(i as u32 + 1);
        }
    }
    fallback
}

/// `curl`s `url`, failing on a non-2xx response or a transport error. The
/// only network access in this binary; everything else in `satex-doc` (and
/// the `satex` binary it drives) works offline.
fn curl(url: &str) -> Result<String, String> {
    let mut cmd = Command::new("curl");
    cmd.args(["-sSL", "--max-time", "30", "--fail"]);
    if url.starts_with("https://api.github.com/")
        && let Some(token) = std::env::var("GITHUB_TOKEN").ok().filter(|t| !t.is_empty())
    {
        cmd.args(["-H", &format!("Authorization: Bearer {token}")]);
    }
    cmd.arg(url);
    let out = cmd.output().map_err(|e| format!("curl {url}: {e}"))?;
    if !out.status.success() {
        return Err(format!("curl {url}: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn github_commit_sha(repo: &str, git_ref: &str) -> Result<String, String> {
    let url = format!("https://api.github.com/repos/{repo}/commits/{git_ref}");
    let body = curl(&url)?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| format!("{url}: {e}"))?;
    json["sha"].as_str().map(str::to_string).ok_or_else(|| format!("{url}: response had no `sha`"))
}

/// Fetches and caches each [`CiteSource`]'s current commit and content, so
/// resolving many citations against the same source hits the network once.
#[derive(Default)]
struct CitationResolver {
    shas: HashMap<String, String>,
    contents: HashMap<String, String>,
    sections: Option<BTreeMap<u32, u32>>,
}

impl CitationResolver {
    fn source(name: &str) -> Result<&'static CiteSource, String> {
        CITE_SOURCES.iter().find(|s| s.name == name).ok_or_else(|| format!("unknown citation source `{name}`"))
    }

    /// Caches `f()`'s result in `map` under `key`, so a repeated lookup skips
    /// the network; shared by [`Self::sha`] and [`Self::content`].
    fn cached<'a>(
        map: &'a mut HashMap<String, String>,
        key: &str,
        f: impl FnOnce() -> Result<String, String>,
    ) -> Result<&'a str, String> {
        if !map.contains_key(key) {
            map.insert(key.to_string(), f()?);
        }
        Ok(&map[key])
    }

    fn sha(&mut self, name: &str) -> Result<String, String> {
        let src = Self::source(name)?;
        Self::cached(&mut self.shas, name, || github_commit_sha(src.repo, src.git_ref)).map(str::to_string)
    }

    fn content(&mut self, name: &str) -> Result<&str, String> {
        let sha = self.sha(name)?;
        let src = Self::source(name)?;
        Self::cached(&mut self.contents, name, || {
            curl(&format!("https://raw.githubusercontent.com/{}/{sha}/{}", src.repo, src.path))
        })
    }

    fn sections(&mut self) -> Result<&BTreeMap<u32, u32>, String> {
        if self.sections.is_none() {
            let sections = tex_web_sections(self.content("tex.web")?);
            self.sections = Some(sections);
        }
        Ok(self.sections.as_ref().expect("just set"))
    }

    /// `name`'s GitHub URL at its current commit: a `blob` link to its file,
    /// or (for a [`CiteKind::Tree`] source) a `tree` link to its directory.
    fn source_url(&mut self, name: &str, tree: bool) -> Result<String, String> {
        let sha = self.sha(name)?;
        let src = Self::source(name)?;
        Ok(format!("https://github.com/{}/{}/{sha}/{}", src.repo, if tree { "tree" } else { "blob" }, src.path))
    }

    /// `ref_id` is `tw-N`, `ech-<identifier>`, or one of [`CITE_SOURCES`]'
    /// own names (the bare, whole-document form).
    fn resolve(&mut self, ref_id: &str) -> Result<String, String> {
        if let Some(rest) = ref_id.strip_prefix("tw-") {
            let n: u32 = rest.parse().map_err(|_| format!("bad citation id `{ref_id}`"))?;
            let line = *self
                .sections()?
                .get(&n)
                .ok_or_else(|| format!("tex.web § {n}: no such module at the current upstream commit"))?;
            return Ok(format!("{}#L{line}", self.source_url("tex.web", false)?));
        }
        if let Some(identifier) = ref_id.strip_prefix("ech-") {
            let line = find_identifier_line(self.content("etex.ch")?, identifier).ok_or_else(|| {
                format!("etex.ch: identifier `{identifier}` not found at the current upstream commit")
            })?;
            return Ok(format!("{}#L{line}", self.source_url("etex.ch", false)?));
        }
        let src = Self::source(ref_id)?;
        self.source_url(ref_id, src.kind == CiteKind::Tree)
    }
}

static REF_ID_RE: OnceLock<Regex> = OnceLock::new();
static DEF_LINE_RE: OnceLock<Regex> = OnceLock::new();

fn ref_id_re() -> &'static Regex {
    REF_ID_RE.get_or_init(|| {
        let names = CITE_SOURCES.iter().map(|s| regex::escape(s.name)).collect::<Vec<_>>().join("|");
        Regex::new(&format!(r"\]\[(tw-\d+|ech-[A-Za-z_][A-Za-z0-9_]*|{names})\]")).expect("valid regex")
    })
}

fn def_line_re() -> &'static Regex {
    DEF_LINE_RE.get_or_init(|| Regex::new(r"(?m)^\[([A-Za-z0-9_.-]+)\]:[ \t]*(\S+)[ \t]*$").expect("valid regex"))
}

/// Re-resolves every `[tw-N]`/`[ech-<identifier>]`/named-source citation
/// used in `path` against the current upstream commit, and rewrites (or,
/// with `write: false`, just diffs) that file's `[id]: url` definitions.
/// Returns whether anything would change; fails outright if any citation
/// used in the file can no longer be resolved at all.
fn resolve_file_citations(path: &str, resolver: &mut CitationResolver, write: bool) -> Result<bool, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
    let mut used: Vec<String> = ref_id_re().captures_iter(&text).map(|c| c[1].to_string()).collect();
    used.sort();
    used.dedup();
    let mut new_urls: HashMap<String, String> = HashMap::new();
    for ref_id in &used {
        let url = resolver.resolve(ref_id).map_err(|e| format!("{path}: {e}"))?;
        new_urls.insert(ref_id.clone(), url);
    }
    let new_text = def_line_re()
        .replace_all(&text, |caps: &regex::Captures| match new_urls.get(&caps[1]) {
            Some(url) => format!("[{}]: {url}", &caps[1]),
            None => caps[0].to_string(),
        })
        .into_owned();
    let changed = new_text != text;
    if write && changed {
        fs::write(path, &new_text).map_err(|e| format!("{path}: {e}"))?;
    }
    Ok(changed)
}

/// The tex.web module -> line table `src/lint/mod.rs` and this binary
/// embed with `include_str!`, so a normal `satex` build and a normal
/// `satex-doc` run need no network access: only `--refresh-citations`
/// (writing it) and `--check-citations` (verifying it is not stale) fetch
/// tex.web itself.
const TEX_WEB_SECTIONS_TSV: &str = "doc/src/tex-web-sections.tsv";
const TEX_WEB_SHA_TXT: &str = "doc/src/tex-web-sha.txt";
/// Hand-written files (not rendered from a `doc/src/**/*.md.in` template)
/// that carry their own `[tw-N]:`/`[ech-<identifier>]:` citations.
const HAND_WRITTEN_CITATION_FILES: &[&str] = &["SEMANTICS.md"];

fn refresh_citations(check_only: bool) -> Result<(), String> {
    let mut resolver = CitationResolver::default();
    let sections = resolver.sections()?.clone();
    let sha = resolver.sha("tex.web")?;
    let mut tsv = String::new();
    for (n, line) in &sections {
        let _ = writeln!(tsv, "{n}\t{line}");
    }
    let sha_txt = format!("{sha}\n");

    let stale = fs::read_to_string(TEX_WEB_SECTIONS_TSV).unwrap_or_default() != tsv
        || fs::read_to_string(TEX_WEB_SHA_TXT).unwrap_or_default() != sha_txt;
    const RERUN: &str = "run `cargo run --features doc --bin satex-doc -- --refresh-citations`";
    if check_only && stale {
        return Err(format!(
            "{TEX_WEB_SECTIONS_TSV}/{TEX_WEB_SHA_TXT} are stale against the current upstream commit ({sha}); {RERUN}"
        ));
    } else if stale {
        fs::write(TEX_WEB_SECTIONS_TSV, &tsv).map_err(|e| format!("{TEX_WEB_SECTIONS_TSV}: {e}"))?;
        fs::write(TEX_WEB_SHA_TXT, &sha_txt).map_err(|e| format!("{TEX_WEB_SHA_TXT}: {e}"))?;
        println!("wrote {TEX_WEB_SECTIONS_TSV}, {TEX_WEB_SHA_TXT} (tex.web trunk {sha}, {} modules)", sections.len());
    }

    let mut any_changed = false;
    for path in HAND_WRITTEN_CITATION_FILES {
        let changed = resolve_file_citations(path, &mut resolver, !check_only)?;
        any_changed |= changed;
        if check_only && changed {
            return Err(format!("{path}: citations resolve differently against the current upstream commit; {RERUN}"));
        }
        if !check_only && changed {
            println!("updated {path}");
        }
    }

    if check_only {
        println!("citations up to date with the current upstream commit");
    } else if !stale && !any_changed {
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
    if directive == "fixes" {
        return fixes_list();
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
    if let Some(rest) = directive.strip_prefix("fields:") {
        let (name, key) = rest.trim().split_once(' ').ok_or_else(|| format!("bad directive: {{{{{directive}}}}}"))?;
        return fields_text(name, key.trim());
    }
    if let Some(path) = directive.strip_prefix("example:") {
        return example(path.trim());
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

/// A document, then one row per paragraph: the text pdflatex ships, the
/// text satex computes, and how they compare. A compact trace follows.
/// Both run on a copy in `target/doc-examples`.
fn example(path: &str) -> Result<String, String> {
    let source = inline_file(path)?;
    let name = Path::new(path).file_stem().and_then(|s| s.to_str()).ok_or_else(|| format!("bad path: {path}"))?;
    let dir = Path::new("target/doc-examples");
    fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    fs::copy(path, dir.join(format!("{name}.tex"))).map_err(|e| format!("{path}: {e}"))?;
    // Both sides read the same `.aux`: the one kept beside the example, or
    // none. The file a previous build left behind is the next run's input.
    let aux = || {
        let (kept, staged) = (Path::new(path).with_extension("aux"), dir.join(format!("{name}.aux")));
        let _ = if kept.exists() { fs::copy(&kept, &staged).map(drop) } else { fs::remove_file(&staged) };
    };
    aux();
    let shipped = typeset_text(path, name, dir);
    aux();
    let file = format!("{name}.tex");
    let json = |args: &[&str]| -> Vec<serde_json::Value> {
        let args: Vec<&str> =
            args.iter().copied().chain(["-f", &file, "--format", "json", "--set", "limits.seconds=60"]).collect();
        Command::new(bin())
            .current_dir(dir)
            .args(&args)
            .output()
            .ok()
            .and_then(|o| serde_json::from_slice(&o.stdout).ok())
            .unwrap_or_default()
    };
    let rows = json(&["query", "typeset"]);
    let of = |kind: &str| -> Vec<String> {
        rows.iter().filter(|r| r["kind"] == kind).filter_map(|r| r["detail"].as_str().map(String::from)).collect()
    };
    let computed = of("paragraph");
    let paragraphs: Vec<&serde_json::Value> = rows.iter().filter(|r| r["kind"] == "paragraph").collect();
    let record = |c: &str| paragraphs.iter().find(|r| r["detail"] == c).copied();
    let mark = |v: &str| match v {
        "equal" => "✓",
        "covers" => "⊇",
        _ => "✗",
    };
    // Pair paragraphs in order. A computed one that may be empty and does
    // not match the next shipped one stands alone.
    let mut pairs: Vec<(String, String)> = Vec::new();
    let mut engine = shipped.iter().peekable();
    for c in &computed {
        let alone = satex::query::typeset_verdict(c, "") != "DIFFERS"
            && engine.peek().is_none_or(|e| satex::query::typeset_verdict(c, e) == "DIFFERS");
        let e = if alone { String::new() } else { engine.next().cloned().unwrap_or_default() };
        pairs.push((e, c.clone()));
    }
    pairs.extend(engine.map(|e| (e.clone(), String::new())));
    // An assumption the document's own code made, not one a package made
    // while loading (the clock, for example).
    let assumed: Vec<String> = rows
        .iter()
        .filter(|r| r["kind"] == "assumption" && r["file"] == file.as_str())
        .filter_map(|r| r["detail"].as_str().map(String::from))
        .collect();
    if !assumed.is_empty() {
        pairs.push((String::new(), format!("assumes {}", assumed.join(", "))));
    }
    let escape = |l: &str| l.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    // satex's notation (choice, repeat, box, unknown) is bold, text is plain.
    let styled = |c: &str| {
        let marked = record(c).and_then(|r| r["marked"].as_str()).unwrap_or(c);
        let mut html = String::new();
        for (i, part) in marked.split('\u{E000}').enumerate() {
            match part.split_once('\u{E001}') {
                Some((notation, text)) if i > 0 => {
                    let _ = write!(html, "<b>{}</b>{}", escape(notation), escape(text));
                }
                _ => html.push_str(&escape(part)),
            }
        }
        html
    };
    // A box that is never shipped is not on the page.
    let boxes = Regex::new("⟦[^⟧]*⟧").expect("boxes regex");
    let boxed_only = |c: &str| c.contains('⟦') && boxes.replace_all(c, "").trim().is_empty();
    pairs.retain(|(e, c)| (!e.is_empty() || !c.is_empty()) && !(e.is_empty() && boxed_only(c)));
    let mut table = String::from("<table><tr><th>pdflatex ships</th><th>satex computes</th><th></th></tr>\n");
    let mut wrong = None;
    let mut why: Vec<String> = Vec::new();
    for (e, c) in &pairs {
        let m = if c.starts_with("assumes ") { "" } else { mark(satex::query::typeset_verdict(c, e)) };
        if m == "✗" && wrong.is_none() {
            wrong = Some((e.clone(), c.clone()));
        }
        if !m.is_empty() && m != "✓" {
            for cause in record(c).and_then(|r| r["causes"].as_array()).into_iter().flatten() {
                let (mark, line) = (cause["mark"].as_str().unwrap_or(""), &cause["line"]);
                let test = cause["test"].as_str().unwrap_or("");
                let reads: Vec<String> = cause["reads"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|r| r.as_str())
                    .map(|r| format!("`{r}`"))
                    .collect();
                why.push(match (test.is_empty(), reads.is_empty()) {
                    (true, _) => format!("- `{mark}`: l.{line} sets a character the run could not compute."),
                    (false, true) => format!("- `{mark}`: l.{line} `{test}` is undecided."),
                    (false, false) => {
                        format!("- `{mark}`: l.{line} `{test}` is undecided, it reads {}.", reads.join(", "))
                    }
                });
            }
        }
        let _ = writeln!(
            table,
            "<tr><td><code>{}</code></td><td><code>{}</code></td><td>{m}</td></tr>",
            escape(e),
            styled(c)
        );
    }
    table.push_str("</table>\n");
    why.truncate(5);
    let mut out = format!("{source}\n{table}\n");
    if let Some((e, c)) = wrong {
        let _ = writeln!(out, "Differs: pdflatex ships `{e}` where satex computes `{c}`.\n");
    }
    if !why.is_empty() {
        let _ = writeln!(out, "Why:\n\n{}\n", why.join("\n"));
    }
    // The commands a reader can run, with their text output verbatim.
    let text = |args: &[&str]| -> (String, String) {
        let args: Vec<&str> = args.iter().copied().chain(["-f", &file, "--set", "limits.seconds=60"]).collect();
        let output = Command::new(bin()).current_dir(dir).args(&args).output().expect("satex runs");
        // A command that fails must break the build, never show up as an empty block.
        assert!(output.status.success(), "satex {}: {}", args.join(" "), String::from_utf8_lossy(&output.stderr));
        let stdout = output.stdout;
        let shown = String::from_utf8_lossy(&stdout).lines().map(redact_paths).collect::<Vec<_>>().join("\n");
        (format!("satex {}", args.join(" ")), shown)
    };
    for (title, args) in
        [("Computed text", &["query", "typeset"][..]), ("Trace", &["trace", "--format", "compact"][..])]
    {
        let (command, shown) = text(args);
        let shown = if shown.trim().is_empty() { "(no output)".to_string() } else { shown };
        let _ =
            write!(out, "<details><summary>{title}</summary>\n\n```sh\n$ {command}\n{shown}\n```\n\n</details>\n\n");
    }
    Ok(out)
}

/// The text one pdflatex run ships out from `path`, one entry per
/// paragraph, read from the boxes `\tracingoutput` writes to the log, so
/// characters are the font's char codes, not glyph names. Kept beside the
/// example as `<name>.pdflatex.txt`: without pdflatex the kept text is
/// used, so the docs build anywhere.
fn typeset_text(path: &str, name: &str, dir: &Path) -> Vec<String> {
    let kept = Path::new(path).with_extension("pdflatex.txt");
    let tex = format!(
        "\\showboxdepth=\\maxdimen \\showboxbreadth=\\maxdimen \\tracingoutput=1 \\tracingonline=0 \\pdfsetrandomseed 7 \\input{{{name}.tex}}"
    );
    let ran = Command::new("pdflatex")
        .current_dir(dir)
        .env("max_print_line", "100000")
        .args(["-interaction=nonstopmode", &tex])
        .output()
        .is_ok();
    let log = ran.then(|| fs::read_to_string(dir.join(format!("{name}.log"))).ok()).flatten();
    let Some(log) = log.filter(|l| l.contains("Completed box being shipped out")) else {
        return match fs::read_to_string(&kept) {
            Ok(k) => k.lines().map(String::from).collect(),
            Err(_) => vec!["(pdflatex output not available)".into()],
        };
    };
    let mut lines: Vec<String> =
        shipped(&log).into_iter().filter(|l| !l.is_empty() && !l.chars().all(|c| c.is_ascii_digit())).collect();
    if lines.is_empty() {
        lines.push("(no text)".into());
    }
    let _ = fs::write(&kept, lines.join("\n") + "\n");
    lines
}

/// One node of a shipped box as the log shows it: its nesting depth and
/// what follows the dots. Box dimensions stay in `item` for a later reader.
fn box_nodes(log: &str) -> Vec<(usize, &str)> {
    let mut nodes = Vec::new();
    let mut inside = false;
    for line in log.lines() {
        if line.starts_with("Completed box being shipped out") {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        let item = line.trim_start_matches('.');
        if !item.starts_with('\\') {
            inside = !line.is_empty() && line.starts_with('.');
            continue;
        }
        nodes.push((line.len() - item.len(), item));
    }
    nodes
}

/// The text of shipped boxes, one entry per paragraph: characters by char
/// code (`^^` notation decoded, ligatures as their characters), interword
/// glue as a space, `\parskip` and each part of the page as a break.
fn shipped(log: &str) -> Vec<String> {
    let mut paragraphs = vec![String::new()];
    // The depth of the line being read, and of the last line read.
    let (mut line, mut last_line) = (None::<usize>, None::<usize>);
    for (depth, item) in box_nodes(log) {
        let (head, rest) = item.split_once(' ').unwrap_or((item, ""));
        let starts_line = head.starts_with("\\hbox") && line.is_none_or(|d| depth <= d);
        if line.is_some_and(|d| depth <= d) && !starts_line {
            line = None;
        }
        if depth == 0 || head == "\\glue(\\parskip)" || (starts_line && last_line.is_some_and(|d| depth < d)) {
            paragraphs.push(String::new());
        }
        let last = paragraphs.last_mut().expect("one paragraph");
        if starts_line {
            (line, last_line) = (Some(depth), Some(depth));
            if !last.is_empty() && !last.ends_with(' ') {
                last.push(' ');
            }
        } else if head == "\\glue" || head == "\\glue(\\spaceskip)" || head == "\\glue(\\xspaceskip)" {
            if !last.ends_with(' ') {
                last.push(' ');
            }
        } else if head.contains('/') && !rest.is_empty() {
            match rest.split_once(" (ligature ") {
                Some((_, lig)) => last.push_str(lig.trim_end_matches(')')),
                None => last.push_str(&caret_decode(rest)),
            }
        }
    }
    paragraphs.into_iter().map(|p| p.trim().to_string()).collect()
}

/// TeX's `^^` notation: `^^3e` or `^^M` back to the character.
fn caret_decode(s: &str) -> String {
    let b = s.as_bytes();
    if b.len() == 4 && s.starts_with("^^") && b[2..].iter().all(u8::is_ascii_hexdigit) {
        return u8::from_str_radix(&s[2..], 16).map_or(s.to_string(), |c| char::from(c).to_string());
    }
    if b.len() == 3 && s.starts_with("^^") {
        return char::from(b[2] ^ 0x40).to_string();
    }
    s.to_string()
}

/// The chktex warnings each rule follows, from `--chktex`.
fn chktex_origins() -> Result<std::collections::BTreeMap<String, Vec<String>>, String> {
    let mut chktex: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for row in lint_json(&["--chktex", "--format", "json"])? {
        for code in row["rules"].as_str().unwrap_or("").split(", ").filter(|c| !c.is_empty()) {
            chktex.entry(code.to_string()).or_default().push(row["warning"].to_string());
        }
    }
    Ok(chktex)
}

/// A rule's badges: its default severity as a shape (and its title), its
/// category, its quick-fix kinds and the chktex warnings it follows. `wide`
/// also spells the severity out, for a rule's own section.
fn badges(
    rule: &serde_json::Value,
    chktex: &std::collections::BTreeMap<String, Vec<String>>,
    wide: bool,
) -> [String; 3] {
    let [code, category, severity] = ["code", "category", "severity"].map(|k| rule[k].as_str().unwrap_or(""));
    let shape = match severity {
        "error" => "◆",
        "warning" => "▲",
        _ => "●",
    };
    let label = if wide { format!("{shape} default: {severity}") } else { shape.to_string() };
    let origin = chktex.get(code).map_or("—".into(), |n| {
        format!(
            "<a class=\"from\" href=\"Chktex-Coverage\" title=\"follows chktex warning {}\">chktex {}</a>",
            n.join(", "),
            n.join(", ")
        )
    });
    [
        format!(
            "<span class=\"sev sev-{severity}\" title=\"default: {severity}\">{label}</span> <span class=\"cat\">{category}</span>"
        ),
        fix_kinds(rule).unwrap_or_else(|| "—".into()),
        origin,
    ]
}

/// The rule list as a table, every code linked to its own section below.
/// Severity reads as a shape (and its title), not only as a color.
fn rules_table() -> Result<String, String> {
    let chktex = chktex_origins()?;
    let mut out = String::from("| code | kind | quick fix | origin | summary |\n| --- | --- | --- | --- | --- |\n");
    for rule in lint_json(&["--rules", "--format", "json"])? {
        let [code, summary] = ["code", "message"].map(|k| rule[k].as_str().unwrap_or(""));
        let [kind, fixes, origin] = badges(&rule, &chktex, false);
        let _ = writeln!(out, "| [`{code}`](#{code}) | {kind} | {fixes} | {origin} | {summary} |");
    }
    Ok(out)
}

/// The rule's quick-fix kinds as badges, `None` when it has none.
fn fix_kinds(rule: &serde_json::Value) -> Option<String> {
    let kinds: Vec<String> = rule["fixes"]
        .as_array()?
        .iter()
        .filter_map(|k| k.as_str())
        .map(|k| format!("<span class=\"fix fix-{k}\">{k}</span>"))
        .collect();
    (!kinds.is_empty()).then(|| kinds.join(" "))
}

/// The rules whose findings carry a quick fix, with its kinds.
fn fixes_list() -> Result<String, String> {
    let mut out = String::new();
    for rule in lint_json(&["--rules", "--format", "json"])? {
        if let Some(kinds) = fix_kinds(&rule) {
            let code = rule["code"].as_str().unwrap_or("");
            let _ = writeln!(out, "* [`{code}`](#{code}) {kinds} — {}", rule["message"].as_str().unwrap_or(""));
        }
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
    let chktex = chktex_origins()?;
    let mut out = String::new();
    let mut open: Option<String> = None;
    for rule in &rules {
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
        out.push_str(&explain_section(rule, &chktex)?);
    }
    if open.is_some() {
        out.push_str("</details>\n");
    }
    Ok(out)
}

/// One rule's section: a heading carrying its anchor, the rules table's
/// badges, the summary in bold, the explanation as markdown, and the works
/// it follows. Everything comes from `--explain CODE --format json`, so
/// nothing here is written twice.
fn explain_section(
    listed: &serde_json::Value,
    chktex: &std::collections::BTreeMap<String, Vec<String>>,
) -> Result<String, String> {
    let code = listed["code"].as_str().unwrap_or("");
    let rule = explain_json(code)?;
    let summary = rule["summary"].as_str().unwrap_or("");
    let mut out = String::new();
    let _ = writeln!(out, "### {code}\n");
    let shown: Vec<String> = badges(listed, chktex, true).into_iter().filter(|b| b != "—").collect();
    let _ = writeln!(out, "<p class=\"badges\">{}</p>\n", shown.join(" "));
    let _ = writeln!(out, "**{summary}**\n");
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
        |table: &satex::fixed::HashMap<satex::tex::Sym, Meaning>| -> std::collections::BTreeMap<String, Primitive> {
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
/// and `tests/it/docs.rs` compares the checked-in output byte for byte.
fn source_url(site: &Site) -> String {
    let repo = env!("CARGO_PKG_REPOSITORY").trim_end_matches('/');
    let file = site.file.replace('\\', "/");
    format!("{repo}/blob/main/{file}#L{}", site.line)
}

/// The `limits` that trade precision for a bounded search: past them a value
/// goes up the lattice. The others (time, memory, threads, record sizes) only
/// stop or pace a run and never make a result coarser.
const PRECISION_LIMITS: &[&str] = &[
    "choices",
    "site_forks",
    "paths",
    "meaning_splits",
    "widen_after",
    "recursion_depth",
    "loop_heads",
    "call_summary_after",
    "meaning_set",
    "value_set",
];

/// `config::Limits`' precision fields, their `Default` value and their doc
/// comment. `limits.*` is described in full on the configuration page; every
/// row links there, so this table only has to give the number.
fn limits_table() -> Result<String, String> {
    let source = fs::read_to_string("src/config.rs").map_err(|e| format!("src/config.rs: {e}"))?;
    let fields = struct_doc_fields(&source, "Limits")?;
    let limits = Limits::default();
    let defaults = serde_json::to_value(&limits).map_err(|e| format!("serializing Limits::default(): {e}"))?;
    let defaults = defaults.as_object().ok_or("Limits did not serialize to a JSON object")?;

    let mut out = String::new();
    let _ = writeln!(out, "| setting | default | what it bounds |");
    let _ = writeln!(out, "| --- | --- | --- |");
    for (name, doc) in fields.into_iter().filter(|(name, _)| PRECISION_LIMITS.contains(&name.as_str())) {
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

/// `{{fields: NAME key}}`: each field of `config::NAME` as `` `key.field`
/// (default `…`): doc ``, the default read from `Config::default()` at the
/// dotted `key`.
fn fields_text(name: &str, key: &str) -> Result<String, String> {
    fn strip_nulls(value: &mut serde_json::Value) {
        match value {
            serde_json::Value::Object(map) => {
                map.retain(|_, v| !v.is_null());
                map.values_mut().for_each(strip_nulls);
            }
            serde_json::Value::Array(items) => items.iter_mut().for_each(strip_nulls),
            _ => {}
        }
    }
    /// A value the way `satex.yaml` would write it, in flow style.
    fn flow(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(items) => format!("[{}]", items.iter().map(flow).collect::<Vec<_>>().join(", ")),
            serde_json::Value::Object(map) => {
                format!("{{{}}}", map.iter().map(|(k, v)| format!("{k}: {}", flow(v))).collect::<Vec<_>>().join(", "))
            }
            other => other.to_string(),
        }
    }
    let source = fs::read_to_string("src/config.rs").map_err(|e| format!("src/config.rs: {e}"))?;
    let mut defaults =
        serde_json::to_value(satex::config::Config::default()).map_err(|e| format!("Config::default(): {e}"))?;
    strip_nulls(&mut defaults);
    let block = key
        .split('.')
        .try_fold(&defaults, |v, k| v.get(k))
        .ok_or_else(|| format!("no `{key}` in Config::default()"))?;
    let mut out = String::new();
    for (field, doc) in struct_doc_fields(&source, name)? {
        let value = block.get(&field).map(|v| format!(" (default `{}`)", flow(v))).unwrap_or_default();
        if doc.is_empty() {
            return Err(format!("`{name}::{field}` has no doc comment"));
        }
        let _ = writeln!(out, "`{key}.{field}`{value}: {doc}\n");
    }
    Ok(out.trim_end().to_string())
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
warning: see [tex.web § 211](https://github.com/TeX-Live/texlive-source/blob/3d3a4b2b66ed37e1fc16078b26451e1d9402f86b/texk/web2c/tex.web#L4220) for the rule
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
