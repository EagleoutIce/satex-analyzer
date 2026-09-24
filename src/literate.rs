//! Literate TeX sources: the `.dtx`/`.ins` pair docstrip works on.
//!
//! A `.dtx` mixes documentation with code, and docstrip decides line by line
//! which is which: a line that does not start with `%` is code, `%%` opens a
//! metacomment that is copied through, `%<…>` is a guard, and every other
//! line starting with `%` is documentation that is thrown away
//! (docstrip.dtx, `\processLine`, `\processLineX`, `\checkOption`).
//! [`code_view`] applies that decision to a whole file and keeps it line for
//! line, so what satex records still points at the line it came from.
//!
//! An `.ins` file is not stripped: it is a batch file that satex runs, with
//! the docstrip commands modeled by [`DocstripOp`].

use std::path::Path;

use crate::builtins::{DocstripOp, OccKind};
use crate::machine::Machine;
use crate::tex::Span;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Literate {
    /// A documented source: documentation and code in one file.
    Dtx,
    /// A batch file that drives the extraction.
    Ins,
}

impl Literate {
    pub fn of(path: &Path) -> Option<Literate> {
        match path.extension()?.to_str()? {
            "dtx" | "fdd" => Some(Literate::Dtx),
            "ins" => Some(Literate::Ins),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Literate::Dtx => "documented source (.dtx), read as docstrip extracts it",
            Literate::Ins => "docstrip installation script (.ins)",
        }
    }
}

/// The options a `\generate` passes docstrip, against which a guard
/// expression is evaluated.
pub enum Guards {
    /// No batch file said which parts to extract, so every guard holds but
    /// `driver`: docstrip.dtx ("Producing the documentation") reserves that
    /// one for the driver that typesets the documentation, and that is the
    /// part of a `.dtx` which is not the package's code.
    AllButDriver,
    Options(Vec<String>),
}

impl Guards {
    fn holds(&self, name: &str) -> bool {
        match self {
            Guards::AllButDriver => name != "driver",
            Guards::Options(options) => options.iter().any(|option| option == name),
        }
    }

    /// The boolean expression of a guard: `|` and `,` for disjunction, `&`
    /// for conjunction, `!` for negation, parentheses for grouping
    /// (docstrip.dtx, "Conditional inclusion of code").
    pub fn eval(&self, expr: &str) -> bool {
        let chars: Vec<char> = expr.chars().collect();
        let mut at = 0;
        self.expression(&chars, &mut at)
    }

    fn expression(&self, s: &[char], at: &mut usize) -> bool {
        let mut value = self.secondary(s, at);
        while matches!(s.get(*at), Some(',' | '|')) {
            *at += 1;
            value |= self.secondary(s, at);
        }
        value
    }

    fn secondary(&self, s: &[char], at: &mut usize) -> bool {
        let mut value = self.primary(s, at);
        while s.get(*at) == Some(&'&') {
            *at += 1;
            value &= self.primary(s, at);
        }
        value
    }

    fn primary(&self, s: &[char], at: &mut usize) -> bool {
        match s.get(*at) {
            Some('!') => {
                *at += 1;
                !self.primary(s, at)
            }
            Some('(') => {
                *at += 1;
                let value = self.expression(s, at);
                if s.get(*at) == Some(&')') {
                    *at += 1;
                }
                value
            }
            _ => {
                let start = *at;
                while s.get(*at).is_some_and(|c| !"(),|&!".contains(*c)) {
                    *at += 1;
                }
                let name: String = s[start..*at].iter().collect();
                self.holds(name.trim())
            }
        }
    }
}

/// What docstrip would write out for `src`, one output line per input line so
/// that line numbers keep pointing into the `.dtx`.  A line that is not code
/// comes out empty, and code that follows a guard keeps its column by being
/// padded to where it stood.
pub fn code_view(src: &str, guards: &Guards) -> String {
    let mut out = String::with_capacity(src.len());
    // docstrip's off-counter: how many enclosing blocks are excluded.  A
    // guard inside an excluded block is not evaluated, only counted
    // (docstrip.dtx, `\starOption`, `\closeguard@do`).
    let mut off = 0usize;
    let mut module: Option<String> = None;
    let mut verbatim: Option<String> = None;
    for line in src.lines() {
        // What of this line is code: its text, the column it starts in, and
        // whether the module substitution applies to it.
        let mut code: Option<(&str, bool)> = None;
        // Verbatim mode copies lines through without even looking for a
        // leading `%`, up to a line holding `%` and the stop tag
        // (docstrip.dtx, `\verbOption`).
        if let Some(stop) = &verbatim {
            match line == stop {
                true => verbatim = None,
                false => code = Some((line, false)),
            }
        } else {
            match line.strip_prefix('%') {
                None => code = Some((line, true)),
                // A metacomment is passed on unchanged, and is a comment in
                // the file docstrip writes as well.
                Some(rest) if rest.starts_with('%') => code = Some((line, false)),
                Some(rest) => {
                    if let Some(guard) = rest.strip_prefix('<') {
                        match guard.chars().next() {
                            Some('*') => {
                                let (expr, _) = split_guard(&guard[1..]);
                                if off > 0 || !guards.eval(expr) {
                                    off += 1;
                                }
                            }
                            Some('/') => off = off.saturating_sub(1),
                            Some('<') => verbatim = Some(format!("%{}", &guard[1..])),
                            // `%<@@=module>` names the expl3 module whose
                            // `@@` the extracted code spells out
                            // (docstrip.dtx, `\moduleOption`).
                            Some('@') => {
                                module = guard
                                    .strip_prefix("@@=")
                                    .map(|rest| split_guard(rest).0.trim().to_string())
                            }
                            // `+` and `-` are the older modifiers for a guard
                            // that covers one line; `-` inverts the test.
                            modifier => {
                                let negated = modifier == Some('-');
                                let body = match modifier {
                                    Some('+' | '-') => &guard[1..],
                                    _ => guard,
                                };
                                let (expr, tail) = split_guard(body);
                                if guards.eval(expr) != negated {
                                    // What follows the guard is the whole
                                    // line docstrip writes: the guard is not
                                    // replaced by spaces.
                                    code = Some((tail, true));
                                }
                            }
                        }
                    }
                }
            }
        }
        // `\endinput` on a line of its own ends the source: docstrip stops
        // reading there and does not copy it (docstrip.dtx, `\processLine`).
        if let (0, Some((text, _))) = (off, code)
            && text.trim() == "\\endinput"
        {
            break;
        }
        if let (0, Some((text, replace))) = (off, code) {
            match (replace, &module) {
                (true, Some(name)) => out.push_str(&substitute(text, name)),
                _ => out.push_str(text),
            }
        }
        out.push('\n');
    }
    out
}

/// A guard expression and what follows the `>` that ends it.
fn split_guard(s: &str) -> (&str, &str) {
    match s.split_once('>') {
        Some((expr, tail)) => (expr, tail),
        None => (s, ""),
    }
}

/// `__@@`, `_@@` and `@@` all become `__module`, with `@@@@` standing for a
/// literal `@@` (docstrip.dtx, `\prepareActiveModule`).
fn substitute(line: &str, module: &str) -> String {
    if !line.contains("@@") {
        return line.to_string();
    }
    const HIDDEN: &str = "\u{0}\u{0}";
    let replacement = format!("__{module}");
    line.replace("@@@@", HIDDEN)
        .replace("__@@", &replacement)
        .replace("_@@", &replacement)
        .replace("@@", &replacement)
        .replace(HIDDEN, "@@")
}

/// One file a batch file generates, and where it comes from.
pub struct Generated {
    pub file: String,
    pub from: String,
    pub guards: String,
}

/// The `\file{out}{\from{src}{guards}…}` list of a `\generate`, read from the
/// text of its argument (docstrip.dtx, "The user interface").
pub fn generated(text: &str) -> Vec<Generated> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some((_, tail)) = rest.split_once("\\file") {
        let Some((file, tail)) = crate::tex::brace_group(tail) else { break };
        let Some((sources, tail)) = crate::tex::brace_group(tail) else { break };
        let mut sources = sources;
        while let Some((_, next)) = sources.split_once("\\from") {
            let Some((from, next)) = crate::tex::brace_group(next) else { break };
            let Some((guards, next)) = crate::tex::brace_group(next) else { break };
            out.push(Generated { file: file.to_string(), from: from.to_string(), guards: guards.to_string() });
            sources = next;
        }
        rest = tail;
    }
    out
}

/// Run one docstrip batch-file command.
pub fn docstrip(m: &mut Machine, op: DocstripOp, span: Span) {
    match op {
        DocstripOp::Generate => {
            let list = m.read_undelimited();
            let text = crate::tex::text_with_groups(&list, &m.out.interner);
            record(m, &text, span);
        }
        // `\generateFile{out}{ask}{\from…}`: the output file and the
        // overwrite flag come first, and the `\from` list follows.
        DocstripOp::GenerateFile => {
            let file = m.read_text();
            let _ask = m.read_text();
            let list = m.read_undelimited();
            let text = crate::tex::text_with_groups(&list, &m.out.interner);
            record(m, &format!("\\file{{{file}}}{{{text}}}"), span);
        }
        DocstripOp::UseDir => {
            let dir = m.read_text();
            m.docstrip_dir = (!dir.is_empty()).then_some(dir);
        }
        DocstripOp::Preamble { named, post } => {
            if named {
                m.read_undelimited();
            }
            let stop = m.intern(match post {
                true => "endpostamble",
                false => "endpreamble",
            });
            // The preamble is text, not TeX to run: docstrip reads it line by
            // line and writes it into every file it generates.
            while let Some(token) = m.read_raw() {
                if token.cs() == Some(stop) {
                    break;
                }
            }
        }
        DocstripOp::BatchInput => {
            let name = m.read_text();
            m.load(&name, crate::builtins::LoadKind::Input, span, Vec::new(), None);
        }
        DocstripOp::Setting { arguments } => {
            for _ in 0..arguments {
                m.read_undelimited();
            }
        }
    }
}

fn record(m: &mut Machine, text: &str, span: Span) {
    for file in generated(text) {
        let directory = m.docstrip_dir.clone();
        let detail = match directory {
            Some(dir) => format!("from {} ({}) into {dir}", file.from, file.guards),
            None => format!("from {} ({})", file.from, file.guards),
        };
        m.occurrence(OccKind::Generate, file.file, Some(detail), span);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guards_select_the_code() {
        let src = "% doc\n%<*one>\n\\def\\a{1}\n%</one>\n%<*two>\n\\def\\b{2}\n%</two>\n";
        let one = code_view(src, &Guards::Options(vec!["one".into()]));
        assert_eq!(one.lines().nth(2), Some("\\def\\a{1}"));
        assert_eq!(one.lines().nth(5), Some(""));
        let both = code_view(src, &Guards::AllButDriver);
        assert_eq!(both.lines().nth(5), Some("\\def\\b{2}"));
    }

    #[test]
    fn a_guarded_line_starts_where_the_guard_ended() {
        // docstrip writes what follows the guard, with nothing in its place.
        let view = code_view("%<+package>\\def\\a{1}\n", &Guards::AllButDriver);
        assert_eq!(view.lines().next(), Some("\\def\\a{1}"));
    }

    #[test]
    fn expressions_follow_the_grammar() {
        let guards = Guards::Options(vec!["a".into(), "b".into()]);
        assert!(guards.eval("a&b"));
        assert!(!guards.eval("a&c"));
        assert!(guards.eval("c|a"));
        assert!(guards.eval("!c"));
        assert!(guards.eval("(a|c)&b"));
        assert!(!guards.eval("!(a|b)"));
    }

    #[test]
    fn modules_are_spelled_out() {
        let view = code_view("%<@@=foo>\n\\cs_new:Npn \\@@_bar:n #1 { \\__@@_baz:n {#1} }\n", &Guards::AllButDriver);
        assert_eq!(
            view.lines().nth(1),
            Some("\\cs_new:Npn \\__foo_bar:n #1 { \\__foo_baz:n {#1} }")
        );
    }

    #[test]
    fn verbatim_mode_copies_percent_lines() {
        let src = "%<<STOP\n% kept\n%STOP\n% dropped\n";
        let view = code_view(src, &Guards::AllButDriver);
        assert_eq!(view.lines().nth(1), Some("% kept"));
        assert_eq!(view.lines().nth(3), Some(""));
    }

    #[test]
    fn a_batch_file_names_what_it_generates() {
        let list = generated("\\file{a.sty}{\\from{a.dtx}{package}\\from{b.dtx}{extra}}\\file{c.tex}{\\from{a.dtx}{driver}}");
        assert_eq!(list.len(), 3);
        assert_eq!((list[0].file.as_str(), list[0].from.as_str(), list[0].guards.as_str()), ("a.sty", "a.dtx", "package"));
        assert_eq!(list[1].guards, "extra");
        assert_eq!(list[2].file, "c.tex");
    }
}
