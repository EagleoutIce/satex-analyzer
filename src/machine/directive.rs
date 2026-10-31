//! Directives in the document's comments: `%\satex@assert{…}`,
//! `%\satex@assume{…}` and `%\satex@configure{…}`. TeX skips them. The run
//! checks or assumes a condition, or changes a setting, where it reads them
//! (doc/src/wiki/lints.md.in, "Assertions").

use super::*;
use crate::builtins::Cond;
use crate::exec::Narrowing;
use crate::tex::Directive;
use crate::value::{Num, Value};
use std::borrow::Cow;

/// The diagnostic codes a directive raises, one per outcome on a path.
/// `crate::lint::assertions` joins them over the paths.
pub const HOLDS: &str = "assert-holds";
pub const FAILS: &str = "assert-fails";
pub const OPEN: &str = "assert-open";
pub const ASSUMED: &str = "assumed";
pub const INVALID: &str = "directive-invalid";
pub const SKIPPED: &str = "directive-skipped";
pub const CODES: &[&str] = &[HOLDS, FAILS, OPEN, ASSUMED, INVALID, SKIPPED];

/// An operand of a comparison: its source text and its tokens.
type Operand = (String, Vec<Token>);

/// One test of a condition: its answer on this path, and what an assumption
/// learns of the register it read.
struct Atom {
    holds: Option<bool>,
    narrow: Option<(Sym, Option<Value>, Option<Value>)>,
    /// The operands of a comparison, for the witness.
    operands: Option<([Operand; 2], bool)>,
}

impl Machine<'_> {
    /// Runs the directives the reader passed in the document's own file
    /// before `token`, which is read again after them.
    pub(super) fn run_directives(&mut self, token: Token) -> bool {
        self.restore_settings();
        // A look-ahead or an argument may have read past the comment: the
        // directive still comes before what is read next.
        let Some(mouth) = self.file_mouth() else { return false };
        if mouth.directives.is_empty() {
            return false;
        }
        let (file, directives) = (mouth.file, std::mem::take(&mut mouth.directives));
        if !self.own_file(file) || self.halted {
            return false;
        }
        self.unread(token);
        for d in directives {
            self.directive(&d);
        }
        true
    }

    /// A file ends with directives the main loop has not reached yet: the
    /// file that read it runs them next, the main file's run now.
    pub(super) fn pass_directives_on(&mut self, mouth: &mut Mouth) {
        if mouth.directives.is_empty() {
            return;
        }
        if let Some(outer) = self.file_mouth() {
            outer.directives.append(&mut mouth.directives);
        } else if self.own_file(mouth.file) && !self.halted {
            for d in std::mem::take(&mut mouth.directives) {
                self.directive(&d);
            }
        }
    }

    /// The innermost file being read.
    fn file_mouth(&mut self) -> Option<&mut Mouth> {
        self.input.iter_mut().rev().find_map(|f| match f {
            Frame::File { mouth, .. } => Some(mouth),
            _ => None,
        })
    }

    /// Directives in text a conditional skips: no run reads them there.
    pub(super) fn skip_directives(&mut self) {
        let Some(Frame::File { mouth, .. }) = self.input.last_mut() else { return };
        if mouth.directives.is_empty() {
            return;
        }
        let (file, directives) = (mouth.file, std::mem::take(&mut mouth.directives));
        if self.own_file(file) {
            for d in directives {
                self.diagnose(Severity::Info, SKIPPED, d.span, format!("\\satex@{}", d.text.trim()));
            }
        }
    }

    /// A group a `%\satex@configure` changed the settings in has ended.
    fn restore_settings(&mut self) {
        while let Some((depth, ..)) = self.settings_scopes.last()
            && self.env.depth() < *depth
        {
            let (_, cfg, limits) = self.settings_scopes.pop().expect("checked above");
            (self.cfg, self.limits) = (cfg, limits);
        }
    }

    fn directive(&mut self, d: &Directive) {
        let invalid = |m: &mut Self, message: String| m.diagnose(Severity::Info, INVALID, d.span, message);
        let Some((kind, arg)) = split_directive(&d.text) else {
            return invalid(self, format!("`\\satex@{}` needs its argument in braces", d.text.trim()));
        };
        match kind {
            "assert" => self.assert(d.span, arg),
            "assume" => self.assume(d.span, arg),
            "configure" => self.configure(d.span, arg),
            _ => invalid(self, format!("`\\satex@{kind}` is no directive: assert, assume or configure")),
        }
    }

    /// `%\satex@assert{cond}`: whether `cond` holds on this path.
    fn assert(&mut self, span: Span, text: &str) {
        let Some(atoms) = self.condition(span, text) else { return };
        let (code, message) = match combine(&atoms) {
            Some(true) => (HOLDS, format!("`{text}` holds")),
            Some(false) if self.exact_path() => (FAILS, format!("`{text}` fails{}", self.witness(&atoms))),
            Some(false) => {
                (OPEN, format!("`{text}` fails on a path the analysis approximated{}", self.witness(&atoms)))
            }
            None => (OPEN, format!("`{text}` is not decided{}", self.witness(&atoms))),
        };
        self.diagnose(Severity::Info, code, span, message);
    }

    /// `%\satex@assume{cond}`: paths where `cond` fails are dropped, and a
    /// conjunction of comparisons narrows the registers they read.
    fn assume(&mut self, span: Span, text: &str) {
        let Some(atoms) = self.condition(span, text) else { return };
        let note = match combine(&atoms) {
            Some(true) => "already holds".to_string(),
            Some(false) => {
                self.diagnose(Severity::Info, ASSUMED, span, format!("`{text}` fails here: the path is dropped"));
                return self.no_arm();
            }
            None if atoms.len() == 1 => {
                let mut narrowed = Vec::new();
                for (negate, atom) in atoms[0].iter().filter(|(_, a)| a.holds.is_none()) {
                    let Some((sym, yes, no)) = &atom.narrow else { continue };
                    let value = if *negate { no } else { yes };
                    if let Some(value) = value.clone() {
                        self.refine_value(*sym, value);
                        narrowed.push(format!("`\\{}`", self.name(*sym)));
                    }
                }
                match narrowed.is_empty() {
                    true => "not decided, and nothing is narrowed by it".into(),
                    false => format!("narrows {}", narrowed.join(", ")),
                }
            }
            None => "not decided, and a disjunction narrows nothing".into(),
        };
        self.diagnose(Severity::Info, ASSUMED, span, format!("`{text}` assumed: {note}"));
    }

    /// `%\satex@configure{key=value, …}`: settings changed until the
    /// current group ends.
    fn configure(&mut self, span: Span, text: &str) {
        let mut cfg = self.cfg.clone().into_owned();
        for setting in top_level_split(text, ',').into_iter().filter(|s| !s.trim().is_empty()) {
            let Some((key, value)) = setting.split_once('=') else {
                let message = format!("`{}` is no `key=value` setting", setting.trim());
                return self.diagnose(Severity::Info, INVALID, span, message);
            };
            let key = key.trim().replace('/', ".");
            if let Err(e) = cfg.set(&key, value.trim()) {
                let message = format!("{e}. Settings: {}", setting_keys(&key));
                return self.diagnose(Severity::Info, INVALID, span, message);
            }
        }
        let depth = self.env.depth();
        if self.settings_scopes.last().is_none_or(|(d, ..)| *d < depth) {
            self.settings_scopes.push((depth, self.cfg.clone(), self.limits.clone()));
        }
        // The limits in force may be lowered already ([`Machine::degrade`]):
        // only a changed limit replaces them.
        if cfg.limits != self.cfg.limits {
            self.limits = cfg.limits.clone();
        }
        self.cfg = Cow::Owned(cfg);
    }

    /// `text` as an `or` of `and`s of tests, each run on this path.
    #[allow(clippy::type_complexity)]
    fn condition(&mut self, span: Span, text: &str) -> Option<Vec<Vec<(bool, Atom)>>> {
        let mut out = Vec::new();
        for conjunction in clauses(text) {
            let mut atoms = Vec::new();
            for (negate, test) in conjunction {
                match self.atom(span, test) {
                    Ok(atom) => atoms.push((negate, atom)),
                    Err(message) => {
                        self.diagnose(Severity::Info, INVALID, span, format!("`{test}`: {message}"));
                        return None;
                    }
                }
            }
            out.push(atoms);
        }
        (!out.is_empty()).then_some(out)
    }

    /// One test: `a rel b` for ⟨number⟩s, or ⟨dimen⟩s when one is written
    /// with a unit, or a TeX conditional such as `\ifdefined\x`.
    fn atom(&mut self, span: Span, text: &str) -> Result<Atom, String> {
        if let Some((lhs, relation, negate, rhs)) = comparison(text) {
            let dimen = has_unit(lhs) || has_unit(rhs);
            let sides = [lhs, rhs].map(|text| (text.trim().to_string(), self.operand(text, span)));
            let mut toks = sides[0].1.clone();
            toks.push(Token::new(Tok::Chr(relation, Catcode::Other), span));
            toks.extend_from_slice(&sides[1].1);
            let (cond, by) = if dimen { (Cond::Dim, "ifdim") } else { (Cond::Num, "ifnum") };
            let by = self.intern(by);
            let (holds, narrowing) = self.test(cond, by, toks)?;
            let narrow = narrowing.map(|n| self.narrowed(n, negate));
            return Ok(Atom { holds: holds.map(|h| h != negate), narrow, operands: Some((sides, dimen)) });
        }
        let mut toks = self.directive_tokens(text, span);
        let first = (!toks.is_empty()).then(|| toks.remove(0));
        let Some((by, Meaning::Primitive(Primitive::If(cond)))) =
            first.and_then(|t| t.cs()).map(|s| (s, self.env.meaning(s)))
        else {
            return Err("neither a comparison (`a < b`, `<=`, `=`, `!=`, `>=`, `>`) nor a TeX conditional".into());
        };
        let (holds, _) = self.test(cond, by, toks)?;
        Ok(Atom { holds, narrow: None, operands: None })
    }

    /// An operand's tokens. `\the<counter>` is the counter's value: how the
    /// counter is printed (`\pagenumbering{roman}`) does not change it.
    fn operand(&mut self, text: &str, span: Span) -> Vec<Token> {
        let text = text.trim();
        if let Some(counter) = text.strip_prefix("\\the").filter(|c| !c.is_empty()) {
            let register = self.intern(&format!("c@{counter}"));
            if matches!(self.env.meaning(register), Meaning::Register(crate::tex::RegKind::Count, _)) {
                return vec![Token::new(Tok::Cs(register), span)];
            }
        }
        self.directive_tokens(text, span)
    }

    /// `text` read as TeX under the current category codes, `@` a letter.
    fn directive_tokens(&mut self, text: &str, span: Span) -> Vec<Token> {
        let mut cats = self.catcodes.clone();
        cats.set('@', Catcode::Letter);
        let mut mouth = Mouth::new(text, span.file);
        std::iter::from_fn(|| mouth.next(&cats, &mut self.out.interner)).map(|t| Token::new(t.tok, span)).collect()
    }

    /// The conditional `cond` on `toks`, which must read them all: its
    /// answer and what an undecided one learns. Never forks.
    fn test(&mut self, cond: Cond, by: Sym, toks: Vec<Token>) -> Result<(Option<bool>, Option<Narrowing>), String> {
        let (result, rest) = self.isolated(toks, |m| {
            let conds = m.conds.len();
            m.testing.push(conds);
            m.testing_kind.push(None);
            m.narrowing = None;
            let holds = m.eval_condition(cond, &mut vec![by]);
            m.testing_kind.pop();
            m.testing.pop();
            (holds, m.narrowing.take())
        });
        match rest.is_empty() {
            true => Ok(result),
            false => Err(format!("`{}` is left over after the test", self.text_of(&rest))),
        }
    }

    /// `f` reading `toks` and nothing else, and what it left of them.
    fn isolated<T>(&mut self, toks: Vec<Token>, f: impl FnOnce(&mut Self) -> T) -> (T, Vec<Token>) {
        self.input.push(Frame::Boundary);
        let base = self.input.len();
        self.push_tokens(toks.into());
        let result = f(self);
        let rest: Vec<Token> =
            std::iter::from_fn(|| self.read(false)).filter(|t| !matches!(t.tok, Tok::Chr(_, Catcode::Space))).collect();
        while self.input.len() >= base {
            self.pop_frame();
        }
        (result, rest)
    }

    /// Whether this path approximated nothing: what fails on it fails in
    /// some run of the document.
    fn exact_path(&mut self) -> bool {
        !self.approximated && !self.after_cut() && !self.took_unknown_arm()
    }

    /// What the operands of each comparison are on this path, and the
    /// choices it made.
    fn witness(&mut self, atoms: &[Vec<(bool, Atom)>]) -> String {
        let mut parts = Vec::new();
        for (_, atom) in atoms.iter().flatten() {
            if let Some((sides, dimen)) = &atom.operands {
                for (text, toks) in sides {
                    let value = self.value_text(toks.clone(), *dimen);
                    if value != *text {
                        parts.push(format!("`{text}` is {value}"));
                    }
                }
            }
        }
        if self.on_paths() {
            parts.push("on one of several paths".into());
        }
        match parts.is_empty() {
            true => String::new(),
            false => format!(": {}", parts.join(", ")),
        }
    }

    /// The value an operand scans to, as an interval when not known.
    fn value_text(&mut self, toks: Vec<Token>, dimen: bool) -> String {
        let (value, _) = self.isolated(toks, |m| {
            m.abs = None;
            let exact = if dimen { m.scan_dimen() } else { m.scan_number() };
            exact.map(Num::exact).or_else(|| m.abs.take())
        });
        let show = |n: i64| match dimen {
            true => format!("{}pt", n as f64 / 65536.0),
            false => n.to_string(),
        };
        match value {
            Some(n) if n.lo == n.hi => show(n.lo),
            Some(n) => format!("in [{}, {}]", show(n.lo), show(n.hi)),
            None => "not known".into(),
        }
    }
}

/// `assert{…}` as `("assert", "…")`: the letters, then a braced argument.
fn split_directive(text: &str) -> Option<(&str, &str)> {
    let kind_end = text.find(|c: char| !c.is_ascii_alphabetic()).unwrap_or(text.len());
    let (kind, rest) = text.split_at(kind_end);
    let rest = rest.trim_start().strip_prefix('{')?;
    let mut depth = 0usize;
    let mut chars = rest.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '\\' => drop(chars.next()),
            '{' => depth += 1,
            '}' if depth == 0 => return Some((kind, &rest[..i])),
            '}' => depth -= 1,
            _ => {}
        }
    }
    None
}

/// `text` split at `sep` outside braces and brackets.
fn top_level_split(text: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start, mut escaped) = (0usize, 0, false);
    for (i, c) in text.char_indices() {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '{' | '[' => depth += 1,
            '}' | ']' => depth = depth.saturating_sub(1),
            c if c == sep && depth == 0 => {
                out.push(&text[start..i]);
                start = i + c.len_utf8();
            }
            _ => {}
        }
    }
    out.push(&text[start..]);
    out
}

/// `a or b and not c` as `[[a], [b, ¬c]]`: the words `or`, `and` and
/// `not` outside braces join tests, `and` binding tighter.
fn clauses(text: &str) -> Vec<Vec<(bool, &str)>> {
    let mut out: Vec<Vec<(bool, &str)>> = vec![Vec::new()];
    // The test being read: whether it is negated, and where it starts and ends.
    let (mut negate, mut test) = (false, None::<(usize, usize)>);
    let mut at = 0;
    for word in top_level_split(text, ' ') {
        let pos = at;
        at += word.len() + 1;
        let close = matches!(word, "or" | "and");
        if close && let Some((s, e)) = test.take() {
            out.last_mut().expect("one clause").push((std::mem::take(&mut negate), &text[s..e]));
        }
        match word {
            "" | "and" => {}
            "or" => out.push(Vec::new()),
            "not" if test.is_none() => negate = !negate,
            _ => test = Some((test.map_or(pos, |t| t.0), pos + word.len())),
        }
    }
    if let Some((s, e)) = test {
        out.last_mut().expect("one clause").push((negate, &text[s..e]));
    }
    out.retain(|c| !c.is_empty());
    out
}

/// `a rel b` outside braces and control sequence names: the operands, the
/// relation `\ifnum` reads, and whether its answer is negated (`<=` is
/// not `>`).
fn comparison(text: &str) -> Option<(&str, char, bool, &str)> {
    let bytes = text.as_bytes();
    let (mut depth, mut i) = (0usize, 0);
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                let name = bytes[i + 1..].iter().take_while(|b| b.is_ascii_alphabetic() || **b == b'@').count();
                i += 1 + name.max(1);
                continue;
            }
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b'<' | b'>' | b'=' | b'!' if depth == 0 => {
                let two = text.get(i..i + 2).unwrap_or_default();
                let (relation, negate, width) = match two {
                    "<=" => ('>', true, 2),
                    ">=" => ('<', true, 2),
                    "!=" => ('=', true, 2),
                    "==" => ('=', false, 2),
                    _ => match bytes[i] {
                        b'!' => return None,
                        b => (char::from(b), false, 1),
                    },
                };
                return Some((&text[..i], relation, negate, &text[i + width..]));
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Whether an operand is written with a unit: letters outside braces and
/// control sequence names (`10pt`, `2\baselineskip` is a coerced register).
fn has_unit(text: &str) -> bool {
    let mut chars = text.chars().peekable();
    let mut depth = 0usize;
    while let Some(c) = chars.next() {
        match c {
            '\\' => while chars.next_if(|c| c.is_ascii_alphabetic() || *c == '@').is_some() {},
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            c if depth == 0 && c.is_ascii_alphabetic() => return true,
            _ => {}
        }
    }
    false
}

/// Three-valued `or` of `and`s.
fn combine(clauses: &[Vec<(bool, Atom)>]) -> Option<bool> {
    let mut any_open = false;
    for clause in clauses {
        let answers: Vec<Option<bool>> = clause.iter().map(|(negate, a)| a.holds.map(|h| h != *negate)).collect();
        if answers.iter().all(|a| *a == Some(true)) {
            return Some(true);
        }
        any_open |= !answers.contains(&Some(false));
    }
    (!any_open).then_some(false)
}

/// The keys `satex.yaml` takes under `key`'s section, or its sections.
fn setting_keys(key: &str) -> String {
    let keys = crate::config::keys();
    let section = key.split('.').next().unwrap_or_default();
    let under: Vec<&str> = keys
        .keys()
        .map(String::as_str)
        .filter(|k| k.strip_prefix(section).is_some_and(|r| r.starts_with('.')))
        .collect();
    match under.is_empty() {
        true => {
            let mut sections: Vec<&str> = keys.keys().map(|k| k.split('.').next().unwrap_or_default()).collect();
            sections.dedup();
            sections.join(", ")
        }
        false => under.join(", "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clauses_bind_and_tighter_than_or() {
        assert_eq!(
            clauses(r"\a < 1 or not \b = 2 and \c > 3"),
            vec![vec![(false, r"\a < 1")], vec![(true, r"\b = 2"), (false, r"\c > 3")]]
        );
    }

    #[test]
    fn comparison_reads_two_character_relations_outside_names() {
        assert_eq!(comparison(r"\c@page<=5"), Some((r"\c@page", '>', true, "5")));
        assert_eq!(comparison(r"\value{a=b} != 2"), Some((r"\value{a=b} ", '=', true, " 2")));
        assert_eq!(comparison(r"\ifdefined\x"), None);
    }

    #[test]
    fn units_are_letters_outside_names() {
        assert!(has_unit("10pt") && !has_unit(r"\wd0") && !has_unit(r"\value{page}"));
    }
}
