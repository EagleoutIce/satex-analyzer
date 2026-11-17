//! The doc/ltxdoc convention (doc.dtx): `\DescribeMacro{\x}` before the
//! user documentation of `\x`, `\begin{macro}{\x}` before its code.

use std::path::Path;

use super::markup::{
    Places, anchors, argument, description, group, names, placeholders, quoted, render, usage, variants,
};
use super::{Audience, DocPlugin, Entry};
use crate::project::config::DocsConfig;

pub struct Ltxdoc;

impl DocPlugin for Ltxdoc {
    fn mine(&self, config: &DocsConfig, file: &Path, text: &str, out: &mut Vec<Entry>) {
        let from = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let (doc, places) = Places::of(file, text);
        let entry = |name: String, usage: Option<String>, text: Option<String>, audience, at| Entry {
            name,
            usage,
            text,
            audience,
            from: from.clone(),
            link: None,
            at: Some(places.at(at)),
        };
        for env in &config.ltxdoc_environments {
            for (at, opts, list, after) in anchors(&doc, env) {
                let own = names(list);
                let text = description(after, own.first().map(String::as_str));
                out.extend(
                    variants(opts, names(list)).into_iter().map(|n| entry(n, None, text.clone(), Audience::Code, at)),
                );
            }
        }
        // `\begin{command}{\x\marg{a}}`: the name, then its usage.
        for env in &config.usage_environments {
            for (at, _, head, after) in anchors(&doc, env) {
                let head = head.trim_start();
                let (name, args) = match group(head, '{', '}') {
                    _ if head.starts_with('/') => key(head),
                    Some((env, args)) => {
                        (env.trim().to_string(), format!("\\begin{{{}}}{}", env.trim(), render(args, true)))
                    }
                    None => {
                        let cs = head.trim_start_matches('\\');
                        let len = cs.chars().take_while(|c| c.is_alphanumeric() || "@*".contains(*c)).count();
                        (cs[..len].to_string(), format!("\\{}{}", &cs[..len], render(&cs[len..], true)))
                    }
                };
                // Without a usage and a backslash it is doc.sty's implementation `environment`, no user's manual.
                if !name.is_empty() && (args.contains('⟨') || head.starts_with(['\\', '/'])) {
                    out.push(entry(name, Some(args), description(after, None), Audience::User, at));
                }
            }
        }
        // oberdiek's `\begin{declcs}{x} \M{a}\end{declcs}`: the name without its backslash, its arguments, then prose
        for env in &config.declare_environments {
            let end = format!("\\end{{{env}}}");
            for (at, _, list, after) in anchors(&doc, env) {
                let Some((args, prose)) = after.split_once(&end) else { continue };
                let args = ["} {", "} [", "] {", "] ["]
                    .iter()
                    .fold(render(args, true), |a, gap| a.replace(gap, &gap.replace(' ', "")));
                for name in names(list) {
                    let usage = format!("\\{name}{}{args}", if args.starts_with(['{', '[']) { "" } else { " " });
                    let text = description(prose, Some(&name));
                    out.push(entry(name, Some(usage.trim_end().to_string()), text, Audience::User, at));
                }
            }
        }
        for anchor in config.describe.iter().map(|c| format!("\\{c}")) {
            for (at, _) in doc.match_indices(&anchor).filter(|(at, _)| !quoted(&doc, *at)) {
                let Some((list, after)) = argument(&doc[at + anchor.len()..]) else { continue };
                // an anchor in an environment's option (`\begin{definition}[\DescribeMacro\x]`)
                let after = after.strip_prefix(']').unwrap_or(after);
                let own = doc[..at].rfind('\n').map_or(0, |i| i + 1);
                let around = std::iter::once(&doc[own..at]).chain(after.split('\n').take(config.usage_lines));
                let around: Vec<String> = around.map(|l| render(l, true)).collect();
                // A bare name (`\cmditem{x}{arg}`) is followed by its arguments.
                let bare = !list.trim_start().starts_with('\\');
                let (line, rest) = after.split_once('\n').unwrap_or((after, ""));
                // its variants' anchors stacked below (`\cmditem*{x*}`) are no description
                let rest = match bare {
                    true => rest
                        .trim_start_matches('\n')
                        .split_inclusive('\n')
                        .skip_while(|l| l.starts_with(&anchor))
                        .collect(),
                    false => after.to_string(),
                };
                for name in names(list) {
                    let own = bare.then(|| format!("\\{name}{}", placeholders(line)));
                    let usage = own.filter(|u| u.contains('⟨')).or_else(|| around.iter().find_map(|l| usage(l, &name)));
                    let text = description(&rest, Some(&name));
                    out.push(entry(name, usage, text, Audience::User, at));
                }
            }
        }
    }
}

/// pgf's `\begin{key}{/tikz/xshift=\meta{dimension} (default 0pt)}`: the
/// key's path, then its usage without the trailing `(default …)` note.
fn key(head: &str) -> (String, String) {
    // the name ends at its value or at its note: `/tikz/help lines (initially {line width=…})`
    let end = [head.find('='), head.find(" (")].into_iter().flatten().min().unwrap_or(head.len());
    let key = head[..end].trim().to_string();
    let value = head[end..].strip_prefix('=').unwrap_or("");
    let value = match value.rfind(" (") {
        Some(at) if value.trim_end().ends_with(')') => &value[..at],
        _ => value,
    };
    let usage = if value.trim().is_empty() { key.clone() } else { format!("{key}={}", render(value.trim(), true)) };
    (key, usage)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_macro_and_macro_blocks() {
        // A manual's headings are what its class defines to start a section.
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/scratch/ltxdoc-headings");
        let classes = root.join("tex/latex/base");
        std::fs::create_dir_all(&classes).unwrap();
        std::fs::write(classes.join("doclike.cls"), "\\LoadClass{bookish}").unwrap();
        std::fs::write(classes.join("bookish.cls"), "\\newcommand\\chapter{\\secdef\\@chapter\\@schapter}\n\\newcommand\\section{\\@startsection{section}{1}{0pt}{1ex}{1ex}{}}").unwrap();
        let dtx = "% \\documentclass{doclike}\n% \\chapter{Use}\n% \\section{Tests}\n% \\DescribeMacro{\\ifthenelse}\n% |\\ifthenelse{|^^A\n%   \\meta{test}|}{|\\meta{then}|}|\n%\n% Evaluates \\meta{test} as a boolean. Then more.\n%\n% \\begin{macro}{\\@latex@error}\n% \\begin{macro}{\\@latex@info}\n% \\changes{v1}{1994}{Added}\n%    Errors and other info, for use in the \\LaTeX{} core.\n%    \\begin{macrocode}\n\\gdef\\@latex@error#1#2{}\n";
        let mut out = Vec::new();
        Ltxdoc.mine(&DocsConfig::default(), &root.join("source/latex/x/x.dtx"), dtx, &mut out);
        let get = |n: &str| out.iter().find(|e| e.name == n).cloned().expect(n);
        let ite = get("ifthenelse");
        assert_eq!(ite.usage.as_deref(), Some("\\ifthenelse{⟨test⟩}{⟨then⟩}"));
        assert_eq!(ite.text.as_deref(), Some("Evaluates ⟨test⟩ as a boolean."));
        let core = Some("Errors and other info, for use in the LaTeX core.");
        assert_eq!(get("@latex@error").text.as_deref(), core);
        assert_eq!(get("@latex@info").text.as_deref(), core);
        // Each entry knows its line, past a joined `^^A` line, and the section it stands in.
        let line = |s: &str| dtx.lines().position(|l| l.contains(s)).unwrap() + 1;
        let at = |n: &str| get(n).at.map(|p| (p.line, p.section));
        assert_eq!(at("ifthenelse"), Some((line("DescribeMacro"), Some("Tests".into()))));
        assert_eq!(at("@latex@info"), Some((line("{\\@latex@info}"), Some("Tests".into()))));
    }

    /// A `\changes` text spanning lines is metadata to its end: its tail
    /// (`in the THEN case}`) never stands in for a description.
    #[test]
    fn metadata_arguments_are_no_description() {
        let dtx = "% \\begin{macro}{\\@writefile}\n% \\changes{v1}{1995}{Added missing percent and use \\cs{relax}\n%  in the THEN case}\n% \\changes{v2}{2018}{Mask the endline\n%    char (github/73)}\n%    \\begin{macrocode}\n\\long\\def\\@writefile#1#2{}\n%    \\end{macrocode}\n%    If we write to the file we first prepare it.\n";
        let mut out = Vec::new();
        Ltxdoc.mine(&DocsConfig::default(), Path::new("ltmiscen.dtx"), dtx, &mut out);
        let entry = out.iter().find(|e| e.name == "@writefile").expect("anchored");
        assert_eq!(entry.text, None);
        let after = "% \\begin{macro}{\\x}\n% \\changes{v1}{1995}{Split\n%  over lines}\n% Sets the {value}.\n";
        let mut out = Vec::new();
        Ltxdoc.mine(&DocsConfig::default(), Path::new("x.dtx"), after, &mut out);
        assert_eq!(out[0].text.as_deref(), Some("Sets the value."));
    }

    /// oberdiek's manuals declare a command by its bare name, its arguments
    /// inside the environment, and describe it in the prose after.
    #[test]
    fn declared_commands_take_their_usage_and_prose() {
        let dtx = "% \\begin{declcs}{intcalcMin} \\M{x} \\M{y}\n% \\end{declcs}\n% Macro \\cs{intcalcMin} returns the smaller of both. More.\n";
        let mut out = Vec::new();
        Ltxdoc.mine(&DocsConfig::default(), Path::new("intcalc.dtx"), dtx, &mut out);
        let min = out.iter().find(|e| e.name == "intcalcMin").expect("declared");
        assert_eq!(min.usage.as_deref(), Some("\\intcalcMin{⟨x⟩}{⟨y⟩}"));
        assert_eq!(min.text.as_deref(), Some("Macro `\\intcalcMin` returns the smaller of both."));
    }

    /// A pgf key's name ends at its value or at its note, whichever comes first.
    #[test]
    fn pgf_keys_lose_their_notes() {
        assert_eq!(key("/tikz/xshift=\\meta{dimension}"), ("/tikz/xshift".into(), "/tikz/xshift=⟨dimension⟩".into()));
        assert_eq!(key("/tikz/cs/y=\\meta{dimension} (initially 0pt)").1, "/tikz/cs/y=⟨dimension⟩");
        assert_eq!(
            key("/tikz/help lines (initially {line width=0.2pt})"),
            ("/tikz/help lines".into(), "/tikz/help lines".into())
        );
    }

    /// An unbraced anchor takes one command (`\DescribeMacro\see`, makeidx);
    /// stacked unbraced anchors share the prose below; an anchor in an
    /// environment's option (datetime2) is described after that environment.
    #[test]
    fn unbraced_anchors_find_their_prose() {
        let dtx = "% \\DescribeMacro\\see\n%    The command |\\see| cross references.\n%\n% \\DescribeMacro\\emph\n% \\DescribeMacro\\emforce\n%\n% This declaration sets emphasis.\n%\n%\\begin{definition}[\\DescribeMacro\\DTMcentury]\n%\\cs{DTMcentury}\\marg{year}\n%\\end{definition}\n%This converts \\meta{year} to the century.\n";
        let mut out = Vec::new();
        Ltxdoc.mine(&DocsConfig::default(), Path::new("x.dtx"), dtx, &mut out);
        let text = |n: &str| out.iter().find(|e| e.name == n).and_then(|e| e.text.clone());
        assert_eq!(text("see").as_deref(), Some("The command `\\see` cross references."));
        assert_eq!(text("emph").as_deref(), Some("This declaration sets emphasis."));
        assert_eq!(text("DTMcentury").as_deref(), Some("This converts ⟨year⟩ to the century."));
    }
}
