//! The doc classes' markup as Markdown: `\cs{x}`, `\meta{x}`, `\marg{x}`,
//! `|verbatim|` (doc.dtx, l3doc.dtx).

use std::path::{Path, PathBuf};

/// The documentation lines of `text`: a `.dtx`'s `%` lines without it, its
/// code lines empty, a `.tex`'s lines without comments. A `^^A` comment
/// joins the next line, as its line end is commented out.
pub fn lines(file: &Path, text: &str) -> Vec<String> {
    numbered(file, text).into_iter().map(|(l, _)| l).collect()
}

/// [`lines`], each with the 1-based line of `text` it starts on.
pub fn numbered(file: &Path, text: &str) -> Vec<(String, usize)> {
    let dtx = file.extension().is_some_and(|e| e == "dtx");
    let mut out: Vec<(String, usize)> = Vec::new();
    let mut join = false;
    for (n, raw) in text.lines().enumerate() {
        let line = match dtx {
            // a `%` in the documentation ends its line, as in any TeX source
            true => cut_comment(raw.strip_prefix('%').filter(|l| !l.starts_with('%')).unwrap_or("")),
            false => cut_comment(raw),
        };
        let (line, comment) = match line.find("^^A") {
            Some(i) => (&line[..i], true),
            None => (line, false),
        };
        match out.last_mut().filter(|_| join) {
            Some((last, _)) => last.push_str(line.trim()),
            None => out.push((line.trim().to_string(), n + 1)),
        }
        join = comment;
    }
    out
}

fn cut_comment(line: &str) -> &str {
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match c {
            '%' if !escaped => return &line[..i],
            '\\' => escaped = !escaped,
            _ => escaped = false,
        }
    }
    line
}

/// The group `open`…`close` that `s` starts with after spaces: its content
/// and what follows it.
pub fn group(s: &str, open: char, close: char) -> Option<(&str, &str)> {
    let s = s.trim_start();
    let body = s.strip_prefix(open)?;
    let mut depth = 0usize;
    for (i, c) in body.char_indices() {
        if c == open {
            depth += 1;
        } else if c == close {
            if depth == 0 {
                return Some((&body[..i], &body[i + c.len_utf8()..]));
            }
            depth -= 1;
        }
    }
    None
}

/// The names of `\a, \b`, without backslashes.
pub fn names(list: &str) -> Vec<String> {
    list.split(',').map(|n| n.trim().trim_start_matches('\\').to_string()).filter(|n| !n.is_empty()).collect()
}

/// Commands dropped with their arguments.
fn dropped(name: &str) -> bool {
    const DROP: [&str; 11] =
        ["begin", "end", "label", "index", "changes", "footnote", "cite", "hspace", "vspace", "url", "item"];
    DROP.contains(&name) || name.starts_with("Describe") || name.starts_with("Special")
}

/// `s` as text: Markdown when `code` is false, the plain characters of a
/// usage line when it is true.
pub fn render(s: &str, code: bool) -> String {
    let tick = if code { "" } else { "`" };
    let mut out = String::new();
    let mut rest = s;
    while let Some(c) = rest.chars().next() {
        rest = &rest[c.len_utf8()..];
        match c {
            '|' if rest.contains('|') => {
                let (verb, after) = rest.split_once('|').unwrap_or_default();
                out.push_str(&format!("{tick}{verb}{tick}"));
                rest = after;
            }
            '\\' => rest = command(rest, code, &mut out),
            '~' => out.push(' '),
            '{' | '}' | '$' => {}
            _ => out.push(c),
        }
    }
    let out = out.replace("``", "“").replace("''", "”");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Renders the command `rest` starts with, after its backslash, into `out`;
/// what follows it.
fn command<'a>(rest: &'a str, code: bool, out: &mut String) -> &'a str {
    let tick = if code { "" } else { "`" };
    let len = rest.chars().take_while(char::is_ascii_alphabetic).count();
    if len == 0 {
        let Some(c) = rest.chars().next() else { return rest };
        out.push_str(match c {
            '{' | '}' | '%' | '_' | '&' | '#' | '$' => &rest[..1],
            '\\' | ' ' | ',' => " ",
            _ => "",
        });
        return &rest[c.len_utf8()..];
    }
    let (name, rest) = rest.split_at(len);
    let arg = |rest: &'a str| group(rest, '{', '}').unwrap_or(("", rest));
    let meta = |s: &str| format!("⟨{}⟩", render(s, true));
    match name {
        "cs" | "tn" | "cmd" => {
            let (a, rest) = arg(rest);
            out.push_str(&format!("{tick}\\{}{tick}", a.trim().trim_start_matches('\\')));
            rest
        }
        "meta" | "prm" => {
            let (a, rest) = arg(rest);
            out.push_str(&meta(a));
            rest
        }
        "marg" | "Arg" | "oarg" | "parg" | "sarg" => {
            let (a, rest) = arg(rest);
            let (open, close) = match name {
                "oarg" => ('[', ']'),
                "parg" => ('(', ')'),
                // beamer's guide: an overlay specification
                "sarg" => ('<', '>'),
                _ => ('{', '}'),
            };
            out.push_str(&format!("{open}{}{close}", meta(a)));
            rest
        }
        "texttt" | "pkg" | "env" | "cls" | "opt" | "file" | "key" | "ref" | "pageref" => {
            let (a, rest) = arg(rest);
            out.push_str(&format!("{tick}{}{tick}", render(a, true)));
            rest
        }
        "emph" | "textit" | "textbf" if !code => {
            let (a, rest) = arg(rest);
            out.push_str(&format!("*{}*", render(a, false)));
            rest
        }
        "verb" => {
            let Some(d) = rest.chars().next() else { return rest };
            let (verb, after) = rest[d.len_utf8()..].split_once(d).unwrap_or((rest, ""));
            out.push_str(&format!("{tick}{verb}{tick}"));
            after
        }
        _ if dropped(name) => {
            let mut rest = rest;
            while let Some((_, after)) = group(rest, '{', '}').or_else(|| group(rest, '[', ']')) {
                rest = after;
            }
            rest
        }
        "LaTeXe" => {
            out.push_str("LaTeX2e");
            rest
        }
        "eTeX" => {
            out.push_str("e-TeX");
            rest
        }
        _ if name.ends_with("TeX") => {
            out.push_str(name);
            rest
        }
        "ldots" | "dots" => {
            out.push('…');
            rest
        }
        // A command the manual's class defines (`\tikzname`, `\xcolor`): shown, not lost from the
        // sentence. One with an argument (`\Meta{x}`) stands for that argument.
        _ if !code && !rest.starts_with('{') => {
            out.push_str(&format!("`\\{name}`"));
            rest
        }
        _ => rest,
    }
}

/// The usage of `\name` in `line`, rendered as code: the name and the
/// arguments right after it, when it has one.
pub fn usage(line: &str, name: &str) -> Option<String> {
    let cs = format!("\\{name}");
    let mut from = 0;
    while let Some(i) = line[from..].find(&cs).map(|i| i + from) {
        let mut rest = &line[i + cs.len()..];
        from = i + cs.len();
        if rest.starts_with(|c: char| c.is_alphanumeric() || "@_:".contains(c)) {
            continue;
        }
        let mut out = cs.clone();
        loop {
            let spaced = rest.trim_start();
            let gap = if spaced.len() < rest.len() { " " } else { "" };
            let close = match spaced.chars().next() {
                Some('{') => '}',
                Some('[') => ']',
                Some('⟨') => '⟩',
                Some('(') => ')',
                Some('<') => '>',
                Some('*') => {
                    out.push_str(gap);
                    out.push('*');
                    rest = &spaced[1..];
                    continue;
                }
                _ => break,
            };
            let open = spaced.chars().next().unwrap_or_default();
            let Some((body, after)) = group(spaced, open, close) else { break };
            out.push_str(&format!("{gap}{open}{body}{close}"));
            rest = after;
        }
        if out.len() > cs.len() && out.contains('⟨') {
            return Some(out);
        }
    }
    None
}

/// The first sentence of `text`, at most about three lines long.
pub fn sentence(text: &str) -> Option<String> {
    const MAX: usize = 300;
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let end = chars
        .windows(3)
        .find(|w| ".!?".contains(w[0].1) && w[1].1 == ' ' && (w[2].1.is_uppercase() || w[2].1 == '`'))
        .map_or(text.len(), |w| w[1].0);
    let mut s = text[..end].trim().to_string();
    if s.len() > MAX {
        let cut = s.char_indices().take_while(|(i, _)| *i < MAX).filter(|(_, c)| *c == ' ').last();
        s.truncate(cut.map_or(s.len(), |(i, _)| i));
        s.push('…');
    }
    (!s.is_empty()).then_some(s)
}

/// The description that follows an anchor (`\begin{macro}{\x}`,
/// `\DescribeMacro{\x}`): its first sentence, past stacked anchors and,
/// for `usage_of`, the usage lines of that name.
pub fn description(after: &str, usage_of: Option<&str>) -> Option<String> {
    const META: [&str; 6] = ["\\changes", "\\begin{macro}", "\\Special", "\\Describe", "\\label", "\\index"];
    const LEAD: usize = 6;
    let meta = |line: &str| META.iter().any(|m| line.starts_with(m));
    // Past the lead: metadata with all its arguments, however many lines
    // they span (`\changes{v1}{date}{text\n text}`), and usage lines.
    let mut rest = after;
    // The names anchored below this one (`\begin{macro}{\patch@level}`): a paragraph naming one is theirs.
    let mut stacked: Vec<String> = Vec::new();
    loop {
        let start = rest.trim_start_matches([' ', '\n']);
        if after[..after.len() - start.len()].matches('\n').count() > LEAD {
            return None;
        }
        let line = start.split('\n').next().unwrap_or_default();
        if meta(line) {
            let len = start[1..].chars().take_while(char::is_ascii_alphabetic).count() + 1;
            let mut tail = &start[len..];
            let mut args = Vec::new();
            while let Some((a, t)) = group(tail, '{', '}').or_else(|| group(tail, '[', ']')) {
                args.push(a);
                tail = t;
            }
            if line.starts_with("\\begin{macro}") {
                stacked.extend(args.get(1).map(|a| names(a)).unwrap_or_default());
            }
            rest = tail;
        } else if !line.is_empty()
            && (usage_of.is_some_and(|name| usage(&render(line, true), name).is_some())
                || placeholders_only(&render(line, true)))
        {
            rest = &start[line.len()..];
        } else {
            rest = start;
            break;
        }
    }
    let mut para: Vec<&str> = Vec::new();
    for line in rest.split('\n') {
        let ends = ["\\begin{", "\\end{", "\\section"].iter().any(|m| line.starts_with(m));
        if line.is_empty() || meta(line) || ends {
            break;
        }
        para.push(line);
    }
    let para = para.join(" ");
    // A paragraph of prose is balanced: an unbalanced one is the tail of some argument.
    let names = |s: &String, n: &str| s.contains(&format!("`\\{n}`"));
    let theirs = |s: &String| stacked.iter().any(|n| names(s, n)) && usage_of.is_none_or(|own| !names(s, own));
    balanced(&para).then(|| sentence(&render(&para, false))).flatten().filter(|s| is_sentence(s) && !theirs(s))
}

/// Whether `s` reads as a sentence: it starts as one (a capital, code, a
/// placeholder or emphasis) and ends as one (`.`, `!`, `?`, `:` or a cut).
/// A fragment (`in the THEN case`, `kernel`, `New macro`) does not.
pub fn is_sentence(s: &str) -> bool {
    let starts = s.chars().next().is_some_and(|c| c.is_uppercase() || "`⟨*\\".contains(c));
    starts && s.trim_end().ends_with(['.', '!', '?', ':', '…']) && s.split_whitespace().count() > 1
}

/// A usage line without the name: `{⟨color⟩}[⟨model⟩]`, groups of placeholders only.
fn placeholders_only(line: &str) -> bool {
    let mut rest = line.trim();
    let mut any = false;
    while let Some(open @ ('{' | '[' | '(')) = rest.chars().next() {
        let close = match open {
            '{' => '}',
            '[' => ']',
            _ => ')',
        };
        let Some((inner, after)) = group(rest, open, close) else { return false };
        any |= inner.contains('⟨');
        rest = after.trim_start();
    }
    any && rest.is_empty()
}

/// Whether every `}` of `s` closes a `{` before it and every `{` is
/// closed, outside `|verbatim|` and past escapes (`\{`).
fn balanced(s: &str) -> bool {
    let (mut depth, mut verb, mut escaped) = (0i32, false, false);
    for c in s.chars() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '|' => verb = !verb,
            '{' if !verb => depth += 1,
            '}' if !verb => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

/// The commands an l3doc anchor names: `[TF]` stands for the `T`, `F` and
/// `TF` forms, `[pTF]` adds the predicate `_p`, `[noTF]` the name itself
/// (l3doc.dtx, `function` environment).
pub fn variants(opts: &str, names: Vec<String>) -> Vec<String> {
    let opts: Vec<&str> = opts.split(',').map(str::trim).collect();
    let has = |o: &str| opts.contains(&o);
    if !has("TF") && !has("pTF") && !has("noTF") {
        return names;
    }
    let mut out = Vec::new();
    for n in names {
        out.extend(["TF", "T", "F"].map(|s| format!("{n}{s}")));
        if has("pTF")
            && let Some((base, spec)) = n.split_once(':')
        {
            out.push(format!("{base}_p:{spec}"));
        }
        if has("noTF") {
            out.push(n);
        }
    }
    out
}

/// The anchors `\begin{env}[opts]{names}` in `doc`: where each starts,
/// options, names and what follows.
pub fn anchors<'a>(doc: &'a str, env: &str) -> impl Iterator<Item = (usize, &'a str, &'a str, &'a str)> {
    let open = format!("\\begin{{{env}}}");
    let at: Vec<usize> = doc.match_indices(&open).map(|(i, _)| i).collect();
    at.into_iter().filter(|&i| !quoted(doc, i)).filter_map(move |i| {
        let rest = &doc[i + open.len()..];
        let (opts, rest) = group(rest, '[', ']').unwrap_or(("", rest));
        let (list, after) = group(rest, '{', '}')?;
        Some((i, opts, list, after))
    })
}

/// Whether byte `at` of `doc` stands in verbatim text: an example the
/// prose shows (`|\DescribeMacro{\foo}|`), no anchor.
pub fn quoted(doc: &str, at: usize) -> bool {
    let line = doc[..at].rfind('\n').map_or(0, |i| i + 1);
    let open = doc[..at].rfind("\\begin{verbatim}");
    doc[line..at].matches('|').count() % 2 == 1 || open > doc[..at].rfind("\\end{verbatim}")
}

/// The arguments after an anchor's name, each as its placeholder:
/// `{command}[arguments]` gives `{⟨command⟩}[⟨arguments⟩]`.
pub fn placeholders(mut rest: &str) -> String {
    let mut out = String::new();
    while let Some(open @ ('{' | '[')) = rest.chars().next() {
        let close = if open == '{' { '}' } else { ']' };
        let Some((arg, after)) = group(rest, open, close) else { break };
        out.push_str(&format!("{open}⟨{}⟩{close}", render(arg, true)));
        rest = after;
    }
    out
}

/// The headings of the manual `file`: the commands its class (`\documentclass`,
/// then each `\LoadClass`) defines through `\@startsection` or `\secdef`,
/// the kernel's two ways to start a section. Read from the class files of
/// the installation `file` lies in, once per class.
fn sectioning(file: &Path, text: &str) -> Vec<String> {
    use std::collections::HashMap;
    use std::sync::Mutex;
    static CLASSES: Mutex<Option<HashMap<PathBuf, Vec<String>>>> = Mutex::new(None);
    let Some(root) = file.ancestors().find(|d| d.join("tex").is_dir()) else { return Vec::new() };
    let mut names = Vec::new();
    let mut class = loaded(text, "\\documentclass");
    for _ in 0..8 {
        let Some(path) =
            class.and_then(|c| crate::caches::distribution::tex_file(&[root.to_path_buf()], &format!("{c}.cls")))
        else {
            break;
        };
        let Ok(code) = std::fs::read_to_string(&path) else { break };
        let mut cache = CLASSES.lock().unwrap_or_else(|e| e.into_inner());
        names.extend(
            cache.get_or_insert_with(HashMap::new).entry(path).or_insert_with(|| starts_sections(&code)).clone(),
        );
        class = loaded(&code, "\\LoadClass");
    }
    names
}

/// The class `\documentclass` or `\LoadClass` (`cmd`) names in `code`.
fn loaded(code: &str, cmd: &str) -> Option<String> {
    let rest = code[code.find(cmd)? + cmd.len()..].trim_start();
    let rest = group(rest, '[', ']').map_or(rest, |(_, r)| r);
    Some(group(rest.trim_start(), '{', '}')?.0.trim().to_string())
}

/// The commands `code` defines whose definition reaches `\@startsection` or `\secdef`.
fn starts_sections(code: &str) -> Vec<String> {
    const DEFINE: [&str; 5] = ["\\newcommand", "\\renewcommand", "\\providecommand", "\\DeclareRobustCommand", "\\def"];
    let mut starts: Vec<(usize, &str)> = DEFINE.iter().flat_map(|d| code.match_indices(d)).collect();
    starts.sort();
    let mut names = Vec::new();
    for (i, &(at, define)) in starts.iter().enumerate() {
        let end = starts.get(i + 1).map_or(code.len(), |n| n.0);
        let rest = code[at + define.len()..end].trim_start_matches(['*', ' ', '{']);
        let Some(name) = rest.strip_prefix('\\') else { continue };
        let name: String = name.chars().take_while(|c| c.is_ascii_alphabetic() || *c == '@').collect();
        let body = &code[at..end];
        if !name.is_empty() && (body.contains("\\@startsection") || body.contains("\\secdef")) && !names.contains(&name)
        {
            names.push(name);
        }
    }
    names
}

/// The places in `file` of its [`numbered`] lines joined as `doc`: an
/// offset's line and the last heading before it (a command of the
/// manual's class that starts a section, [`sectioning`]).
pub struct Places {
    path: String,
    starts: Vec<usize>,
    numbers: Vec<usize>,
    headings: Vec<(usize, String)>,
}

impl Places {
    /// The [`lines`] of `file` joined, with their places.
    pub fn of(file: &Path, text: &str) -> (String, Places) {
        let sections = sectioning(file, text);
        let (lines, numbers): (Vec<String>, Vec<usize>) = numbered(file, text).into_iter().unzip();
        let doc = lines.join("\n");
        let starts = std::iter::once(0).chain(doc.match_indices('\n').map(|(i, _)| i + 1)).collect();
        let mut headings: Vec<(usize, String)> = sections
            .iter()
            .flat_map(|s| {
                let cmd = format!("\\{s}");
                doc.match_indices(&cmd)
                    .filter_map(|(i, _)| {
                        let rest = doc[i + cmd.len()..].trim_start_matches('*');
                        if rest.starts_with(|c: char| c.is_ascii_alphabetic()) {
                            return None;
                        }
                        let rest = group(rest, '[', ']').map_or(rest, |(_, r)| r);
                        Some((i, render(group(rest, '{', '}')?.0, false)))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        headings.sort();
        let places = Places { path: file.to_string_lossy().into_owned(), starts, numbers, headings };
        (doc, places)
    }

    pub fn at(&self, offset: usize) -> super::Place {
        let before = self.headings.partition_point(|(i, _)| *i <= offset);
        super::Place {
            path: self.path.clone(),
            line: self
                .numbers
                .get(self.starts.partition_point(|s| *s <= offset).saturating_sub(1))
                .copied()
                .unwrap_or(1),
            section: before.checked_sub(1).map(|i| self.headings[i].1.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_lines_and_text() {
        let l3 = render("\\cs{tl_set:Nn} \\meta{tl~var} \\Arg{tokens}", true);
        assert_eq!(usage(&l3, "tl_set:Nn").as_deref(), Some("\\tl_set:Nn ⟨tl var⟩ {⟨tokens⟩}"));
        let doc = render("|\\ifthenelse{|\\meta{test}|}{|\\meta{then}|}{|\\meta{else}|}|", true);
        assert_eq!(usage(&doc, "ifthenelse").as_deref(), Some("\\ifthenelse{⟨test⟩}{⟨then⟩}{⟨else⟩}"));
        assert_eq!(usage("\\iffoo{⟨x⟩}", "if"), None);
        let beamer = render("\\cs{alt}\\sarg{overlay specification}\\marg{default text}\\marg{alternative text}", true);
        let sig = "\\alt<⟨overlay specification⟩>{⟨default text⟩}{⟨alternative text⟩}";
        assert_eq!(usage(&beamer, "alt").as_deref(), Some(sig));
        let text = render("Errors, for the \\LaTeX{} core (\\emph{cf.}~\\cs{q_nil}). More.", false);
        assert_eq!(sentence(&text).as_deref(), Some("Errors, for the LaTeX core (*cf.* `\\q_nil`)."));
    }

    #[test]
    fn a_line_comment_joins_the_next_line() {
        let lines = lines(Path::new("a.dtx"), "% |\\x{|^^A\n%  \\meta{y}|}|\n\\code\n%% meta\n");
        assert_eq!(lines, vec!["|\\x{|\\meta{y}|}|", "", ""]);
    }
}
