//! The doc classes' markup as Markdown: `\cs{x}`, `\meta{x}`, `\marg{x}`,
//! `|verbatim|` (doc.dtx, l3doc.dtx).

use std::path::Path;

/// The documentation lines of `text`: a `.dtx`'s `%` lines without it, its
/// code lines empty, a `.tex`'s lines without comments. A `^^A` comment
/// joins the next line, as its line end is commented out.
pub fn lines(file: &Path, text: &str) -> Vec<String> {
    let dtx = file.extension().is_some_and(|e| e == "dtx");
    let mut out: Vec<String> = Vec::new();
    let mut join = false;
    for raw in text.lines() {
        let line = match dtx {
            true => raw.strip_prefix('%').filter(|l| !l.starts_with('%')).unwrap_or(""),
            false => cut_comment(raw),
        };
        let (line, comment) = match line.find("^^A") {
            Some(i) => (&line[..i], true),
            None => (line, false),
        };
        match out.last_mut().filter(|_| join) {
            Some(last) => last.push_str(line.trim()),
            None => out.push(line.trim().to_string()),
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
    const DROP: [&str; 12] =
        ["begin", "end", "label", "index", "changes", "footnote", "ref", "cite", "hspace", "vspace", "url", "item"];
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
        "meta" => {
            let (a, rest) = arg(rest);
            out.push_str(&meta(a));
            rest
        }
        "marg" | "Arg" | "oarg" | "parg" => {
            let (a, rest) = arg(rest);
            let (open, close) = match name {
                "oarg" => ('[', ']'),
                "parg" => ('(', ')'),
                _ => ('{', '}'),
            };
            out.push_str(&format!("{open}{}{close}", meta(a)));
            rest
        }
        "texttt" | "pkg" | "env" | "cls" | "opt" | "file" | "key" => {
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
    let mut para: Vec<&str> = Vec::new();
    for (n, line) in after.split('\n').enumerate() {
        let lead = para.is_empty();
        if lead && n > LEAD {
            return None;
        }
        if line.is_empty() || META.iter().any(|m| line.starts_with(m)) {
            if lead {
                continue;
            }
            break;
        }
        if line.starts_with("\\begin{") || line.starts_with("\\end{") || line.starts_with("\\section") {
            break;
        }
        if lead && usage_of.is_some_and(|name| usage(&render(line, true), name).is_some()) {
            continue;
        }
        para.push(line);
    }
    sentence(&render(&para.join(" "), false))
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

/// The anchors `\begin{env}[opts]{names}` in `doc`: options, names and
/// what follows.
pub fn anchors<'a>(doc: &'a str, env: &str) -> impl Iterator<Item = (&'a str, &'a str, &'a str)> {
    let open = format!("\\begin{{{env}}}");
    let at: Vec<usize> = doc.match_indices(&open).map(|(i, _)| i + open.len()).collect();
    at.into_iter().filter_map(move |i| {
        let rest = &doc[i..];
        let (opts, rest) = group(rest, '[', ']').unwrap_or(("", rest));
        let (list, after) = group(rest, '{', '}')?;
        Some((opts, list, after))
    })
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
        let text = render("Errors, for the \\LaTeX{} core (\\emph{cf.}~\\cs{q_nil}). More.", false);
        assert_eq!(sentence(&text).as_deref(), Some("Errors, for the LaTeX core (*cf.* `\\q_nil`)."));
    }

    #[test]
    fn a_line_comment_joins_the_next_line() {
        let lines = lines(Path::new("a.dtx"), "% |\\x{|^^A\n%  \\meta{y}|}|\n\\code\n%% meta\n");
        assert_eq!(lines, vec!["|\\x{|\\meta{y}|}|", "", ""]);
    }
}
