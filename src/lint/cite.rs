//! Citations in running text: names of published sources (`tex.web § 1034`,
//! `The TeXbook, chapter 18`, the e-TeX manual) become markdown links.
//! One pass shared by `satex-doc` (the wiki) and `lint --explain --format json` (the playground).

use std::fmt::Write as _;
use std::sync::OnceLock;

use regex::Regex;

use super::{Source, tex_web_section_line};

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
    /// words this repo uses on their own (a color, a linter), so those
    /// need a section or quote after them before they are worth a link.
    always: bool,
}

/// eTeX has no [`Source`] variant of its own (nothing in `src/lint/mod.rs`
/// follows it directly), so its manual's URL is kept here, matching the one
/// `doc/src/wiki/primitive-coverage.md.in` already cites by hand.
const ETEX_MANUAL_URL: &str = "https://mirrors.ctan.org/systems/doc/etex/etex_man.pdf";
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
        ("cleveref", Source::Cleveref, false),
        ("chktex", Source::Chktex, false),
        ("ChkTeX", Source::Chktex, true),
        ("csquotes", Source::Csquotes, false),
    ];
    let mut aliases: Vec<Alias> = known
        .iter()
        .map(|(name, source, always)| Alias { name, url: source.url().to_string(), always: *always })
        .collect();
    aliases.push(Alias { name: "eTeX manual", url: ETEX_MANUAL_URL.to_string(), always: true });
    aliases.push(Alias { name: "e-TeX manual", url: ETEX_MANUAL_URL.to_string(), always: true });
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
pub fn linkify_citations(markdown: &str) -> String {
    let aliases = source_aliases();
    let mut by_len: Vec<&Alias> = aliases.iter().collect();
    by_len.sort_by_key(|a| std::cmp::Reverse(a.name.len()));
    let alias_pattern = by_len.iter().map(|a| regex::escape(a.name)).collect::<Vec<_>>().join("|");

    const NUM: &str = r"[0-9]+(?:\.[0-9]+)*";
    let range = format!(r"{NUM}(?:\s*[-\u{{2013}}]\s*{NUM})?");
    let list = format!(r"{range}(?:\s*(?:,|and)\s*{range})*");
    let word = r#"[^\s,)."]+"#;
    let quoted = r#""[^"]+""#;
    let cite = format!(
        r#"(?:,?\s*§§?\s*(?:{list}|{word}))|(?:,\s*{quoted})|(?:,\s*(?:chapters?|appendix|appendices)\s+[0-9A-Z][0-9.]*)"#
    );
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

/// `Source::TeXWeb.url()`, pinned to `section`'s own line via
/// [`tex_web_section_line`] (the table `Ref::url()` uses for
/// the same citations found structurally, in a `Rule`'s `references`).
/// Falls back to the whole file when the number is not a real module (a
/// version number the citation regex picked up, say), so a citation always
/// still resolves.
fn tex_web_url(section: u32) -> String {
    match tex_web_section_line(section) {
        Some(line) => format!("{}#L{line}", Source::TeXWeb.url()),
        None => Source::TeXWeb.url().to_string(),
    }
}

/// Every `§`-numbered module in a tex.web citation, in the order it was
/// written, with whether it is joined to the previous one by a range dash
/// (`–`/`-`) rather than a comma or "and".
fn tex_web_numbers(cite: &str) -> Vec<(u32, bool)> {
    static NUM: OnceLock<Regex> = OnceLock::new();
    let num_re = NUM.get_or_init(|| Regex::new(r"[0-9]+").expect("valid regex"));
    let mut out = Vec::new();
    let mut last_end = 0;
    for m in num_re.find_iter(cite) {
        let between = &cite[last_end..m.start()];
        let is_range = !out.is_empty() && (between.contains('-') || between.contains('\u{2013}'));
        if let Ok(n) = m.as_str().parse() {
            out.push((n, is_range));
        }
        last_end = m.end();
    }
    out
}

/// Renders a tex.web citation (`prefix` is `"tex.web"` or `""` for a bare
/// continuation) as one reference-style link per module, recording each
/// module used in `refs` so [`linkify_citations`] can emit its definition.
fn tex_web_citation(prefix: &str, cite: &str) -> Option<String> {
    let numbers = tex_web_numbers(cite);
    let (first, _) = *numbers.first()?;
    let mark = if numbers.len() > 1 { "§§" } else { "§" };
    let mut out = String::new();
    let first_label = if prefix.is_empty() { format!("{mark} {first}") } else { format!("{prefix} {mark} {first}") };
    let _ = write!(out, "[{first_label}]({})", tex_web_url(first));
    for &(n, is_range) in &numbers[1..] {
        out.push_str(if is_range { "\u{2013}" } else { ", " });
        let _ = write!(out, "[{n}]({})", tex_web_url(n));
    }
    Some(out)
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
            // A `/`-preceded match is a URL path segment shown as output
            // (`https://texdoc.org/serve/source2e/0`), not a citation: a
            // `\b` word boundary alone does not tell those apart.
            if text[..m.start()].ends_with('/') {
                out.push_str(m.as_str());
                continue;
            }
            let entry = aliases.iter().find(|a| a.name == alias.as_str()).expect("matched alias is in the table");
            let tex_web_cite = (alias.as_str() == "tex.web").then(|| caps.name("cite")).flatten().and_then(|cite| {
                *last_url = Some(entry.url.clone());
                tex_web_citation(alias.as_str(), cite.as_str())
            });
            if let Some(rendered) = tex_web_cite {
                out.push_str(&rendered);
            } else if entry.always || caps.name("cite").is_some() {
                *last_url = Some(entry.url.clone());
                let chapter = Regex::new(r"(chapter|appendix)\s+([0-9A-Z]+)").expect("valid regex");
                let url = match (entry.name, chapter.captures(m.as_str())) {
                    ("The TeXbook", Some(c)) => format!("{}#{}-{}", entry.url, &c[1], c[2].to_lowercase()),
                    _ => entry.url.clone(),
                };
                out.push_str(&format!("[{}]({url})", m.as_str()));
            } else {
                out.push_str(m.as_str());
            }
        } else if let Some(bare) = caps.name("bare") {
            if last_url.as_deref() == Some(Source::TeXWeb.url()) {
                let rendered = tex_web_citation("", bare.as_str());
                out.push_str(rendered.as_deref().unwrap_or(bare.as_str()));
            } else if let Some(url) = &last_url {
                out.push_str(&format!("[{}]({url})", bare.as_str()));
            } else {
                out.push_str(bare.as_str());
            }
        }
    }
    out.push_str(&text[last..]);
    out
}
