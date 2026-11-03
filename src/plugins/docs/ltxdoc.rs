//! The doc/ltxdoc convention (doc.dtx): `\DescribeMacro{\x}` before the
//! user documentation of `\x`, `\begin{macro}{\x}` before its code.

use std::path::Path;

use super::markup::{Places, anchors, description, group, names, placeholders, quoted, render, usage, variants};
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
                let text = description(after, None);
                out.extend(
                    variants(opts, names(list)).into_iter().map(|n| entry(n, None, text.clone(), Audience::Code, at)),
                );
            }
        }
        // `\begin{command}{\x\marg{a}}`: the name, then its usage.
        for env in &config.usage_environments {
            for (at, _, head, after) in anchors(&doc, env) {
                let (name, args) = match group(head, '{', '}') {
                    Some((env, args)) => {
                        (env.trim().to_string(), format!("\\begin{{{}}}{}", env.trim(), render(args, true)))
                    }
                    None => {
                        let cs = head.trim_start().trim_start_matches('\\');
                        let len = cs.chars().take_while(|c| c.is_alphanumeric() || "@*".contains(*c)).count();
                        (cs[..len].to_string(), format!("\\{}{}", &cs[..len], render(&cs[len..], true)))
                    }
                };
                // Without a usage it is doc.sty's implementation `environment`, no user's manual.
                if !name.is_empty() && args.contains('⟨') {
                    out.push(entry(name, Some(args), description(after, None), Audience::User, at));
                }
            }
        }
        for anchor in config.describe.iter().map(|c| format!("\\{c}")) {
            for (at, _) in doc.match_indices(&anchor).filter(|(at, _)| !quoted(&doc, *at)) {
                let Some((list, after)) = group(&doc[at + anchor.len()..], '{', '}') else { continue };
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
}
