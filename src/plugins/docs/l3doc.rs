//! The l3doc convention (l3doc.dtx): `\begin{function}{\x:n, …}` with a
//! `syntax` block and the description after it, `\begin{variable}`.

use std::path::Path;

use super::markup::{Places, anchors, description, names, render, usage, variants};
use super::{Audience, DocPlugin, Entry};
use crate::project::config::DocsConfig;

pub struct L3doc;

impl DocPlugin for L3doc {
    fn mine(&self, config: &DocsConfig, file: &Path, text: &str, out: &mut Vec<Entry>) {
        let from = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let (doc, places) = Places::of(file, text);
        for env in config.l3doc_environments.iter().map(String::as_str) {
            for (at, opts, list, after) in anchors(&doc, env) {
                let body = after.split_once(&format!("\\end{{{env}}}")).map_or(after, |(b, _)| b);
                let (syntax, text) = match body.split_once("\\begin{syntax}") {
                    Some((_, rest)) => rest.split_once("\\end{syntax}").unwrap_or((rest, "")),
                    None => ("", body),
                };
                let syntax: Vec<String> = syntax.lines().map(|l| render(l, true)).collect();
                let text = description(text, None);
                for name in variants(opts, names(list)) {
                    out.push(Entry {
                        usage: syntax.iter().find_map(|l| usage(l, &name)),
                        name,
                        text: text.clone(),
                        audience: Audience::User,
                        from: from.clone(),
                        link: None,
                        at: Some(places.at(at)),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn functions_with_syntax_and_variants() {
        let dtx = "% \\begin{function}[EXP]\n%   {\n%     \\tl_set:Nn, \\tl_set:NV,\n%   }\n%   \\begin{syntax}\n%     \\cs{tl_set:Nn} \\meta{tl~var} \\Arg{tokens}\n%   \\end{syntax}\n%   Sets \\meta{tl~var} to contain \\meta{tokens}.\n% \\end{function}\n% \\begin{function}[pTF]{\\tl_if_empty:N}\n%   Tests emptiness.\n% \\end{function}\n";
        let mut out = Vec::new();
        L3doc.mine(&DocsConfig::default(), Path::new("l3tl.dtx"), dtx, &mut out);
        let get = |n: &str| out.iter().find(|e| e.name == n).cloned().expect(n);
        let set = get("tl_set:Nn");
        assert_eq!(set.usage.as_deref(), Some("\\tl_set:Nn ⟨tl var⟩ {⟨tokens⟩}"));
        assert_eq!(set.text.as_deref(), Some("Sets ⟨tl var⟩ to contain ⟨tokens⟩."));
        assert_eq!(get("tl_set:NV").usage, None);
        assert_eq!(get("tl_if_empty_p:N").text.as_deref(), Some("Tests emptiness."));
        assert!(out.iter().any(|e| e.name == "tl_if_empty:NTF"));
    }
}
