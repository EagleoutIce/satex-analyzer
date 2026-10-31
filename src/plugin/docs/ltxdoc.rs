//! The doc/ltxdoc convention (doc.dtx): `\DescribeMacro{\x}` before the
//! user documentation of `\x`, `\begin{macro}{\x}` before its code.

use std::path::Path;

use super::markup::{anchors, description, group, lines, names, render, usage, variants};
use super::{Audience, DocPlugin, Entry};

pub struct Ltxdoc;

/// How many lines after `\DescribeMacro` its usage line may stand.
const USAGE_LINES: usize = 12;

impl DocPlugin for Ltxdoc {
    fn mine(&self, file: &Path, text: &str, out: &mut Vec<Entry>) {
        let from = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let lines = lines(file, text);
        let doc = lines.join("\n");
        let entry = |name: String, usage: Option<String>, text: Option<String>, audience| Entry {
            name,
            usage,
            text,
            audience,
            from: from.clone(),
            link: None,
        };
        for (opts, list, after) in anchors(&doc, "macro") {
            let text = description(after, None);
            out.extend(variants(opts, names(list)).into_iter().map(|n| entry(n, None, text.clone(), Audience::Code)));
        }
        const ANCHOR: &str = "\\DescribeMacro";
        for (at, _) in doc.match_indices(ANCHOR) {
            let Some((list, after)) = group(&doc[at + ANCHOR.len()..], '{', '}') else { continue };
            let own = doc[..at].rfind('\n').map_or(0, |i| i + 1);
            let around = std::iter::once(&doc[own..at]).chain(after.split('\n').take(USAGE_LINES));
            let around: Vec<String> = around.map(|l| render(l, true)).collect();
            for name in names(list) {
                let usage = around.iter().find_map(|l| usage(l, &name));
                let text = description(after, Some(&name));
                out.push(entry(name, usage, text, Audience::User));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_macro_and_macro_blocks() {
        let dtx = "% \\DescribeMacro{\\ifthenelse}\n% |\\ifthenelse{|^^A\n%   \\meta{test}|}{|\\meta{then}|}|\n%\n% Evaluates \\meta{test} as a boolean. Then more.\n%\n% \\begin{macro}{\\@latex@error}\n% \\begin{macro}{\\@latex@info}\n% \\changes{v1}{1994}{Added}\n%    Errors and other info, for use in the \\LaTeX{} core.\n%    \\begin{macrocode}\n\\gdef\\@latex@error#1#2{}\n";
        let mut out = Vec::new();
        Ltxdoc.mine(Path::new("x.dtx"), dtx, &mut out);
        let get = |n: &str| out.iter().find(|e| e.name == n).cloned().expect(n);
        let ite = get("ifthenelse");
        assert_eq!(ite.usage.as_deref(), Some("\\ifthenelse{⟨test⟩}{⟨then⟩}"));
        assert_eq!(ite.text.as_deref(), Some("Evaluates ⟨test⟩ as a boolean."));
        let core = Some("Errors and other info, for use in the LaTeX core.");
        assert_eq!(get("@latex@error").text.as_deref(), core);
        assert_eq!(get("@latex@info").text.as_deref(), core);
    }
}
