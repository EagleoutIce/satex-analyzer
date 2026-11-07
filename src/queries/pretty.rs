//! A macro body pretty-printed: one statement per line, groups and
//! `\if…\else…\fi` indented, long key lists one key per line. What a
//! statement spans comes from the meanings the run ended with: a macro's
//! parameter text (or its LaTeX argument spec), a primitive's syntax. A body
//! that fits `width` and holds no conditional stays one line.

use crate::engine::builtins::{Cond, Primitive as P};
use crate::engine::tex::{ArgType, Catcode, Interner, Meaning, ParamItem, Sym, Tok, Token, detokenize};

/// A token, or a braced group with its items (`closed`: its `}` is there).
struct Item {
    from: usize,
    to: usize,
    kids: Option<Vec<Item>>,
    closed: bool,
}

fn items(toks: &[Token], at: &mut usize, inner: bool) -> Vec<Item> {
    let mut out = Vec::new();
    while *at < toks.len() {
        let i = *at;
        if inner && toks[i].is_cat(Catcode::End) {
            return out;
        }
        *at += 1;
        if toks[i].is_cat(Catcode::Begin) {
            let kids = items(toks, at, true);
            let closed = *at < toks.len();
            *at += usize::from(closed);
            out.push(Item { from: i, to: *at, kids: Some(kids), closed });
        } else {
            out.push(Item { from: i, to: i + 1, kids: None, closed: true });
        }
    }
    out
}

/// What a conditional token does to the layout.
#[derive(PartialEq)]
enum Fold {
    Open,
    Mid,
    Close,
}

struct Printer<'a> {
    toks: &'a [Token],
    it: &'a Interner,
    meaning: &'a dyn Fn(Sym) -> Meaning,
    width: usize,
    lines: Vec<String>,
}

/// `toks` pretty-printed, each name read with `meaning`.
pub fn print(toks: &[Token], it: &Interner, meaning: &dyn Fn(Sym) -> Meaning, width: usize) -> String {
    let mut p = Printer { toks, it, meaning, width, lines: Vec::new() };
    let list = items(toks, &mut 0, false);
    let flat = p.join(&list);
    if flat.trim_end().chars().count() <= width && !toks.iter().any(|t| p.fold_tok(t).is_some()) {
        return flat.trim_end().to_string();
    }
    p.block(&list, 0);
    p.lines.join("\n")
}

impl Printer<'_> {
    fn flat(&self, item: &Item) -> String {
        match &item.kids {
            Some(kids) => {
                let mut inner = self.join(kids);
                if self.word_end(&inner) {
                    inner.pop();
                }
                format!("{{{inner}{}", if item.closed { "}" } else { "" })
            }
            None => detokenize(&self.toks[item.from..item.to], self.it),
        }
    }
    /// `items` on one line: a control word's display space only where a letter follows.
    fn join(&self, items: &[Item]) -> String {
        let mut out = String::new();
        for item in items {
            self.extend(&mut out, item);
        }
        out
    }
    fn extend(&self, out: &mut String, item: &Item) {
        let text = self.flat(item);
        let word = |c: char| c.is_alphanumeric() || "@_:".contains(c);
        if out.ends_with(' ') && !text.starts_with(word) && !self.space(item) && self.word_end(out) {
            out.pop();
        }
        out.push_str(&text);
    }
    /// Whether `out` ends with a control word and the space TeX shows after it.
    fn word_end(&self, out: &str) -> bool {
        let t = out.trim_end_matches(' ');
        out.len() == t.len() + 1
            && t.rfind('\\')
                .is_some_and(|i| t[i + 1..].chars().all(|c| c.is_alphabetic() || "@_:".contains(c)) && i + 1 < t.len())
    }
    fn push(&mut self, depth: usize, line: &str) {
        let line = line.trim();
        if !line.is_empty() {
            self.lines.push(format!("{}{line}", "  ".repeat(depth)));
        }
    }
    fn sym(&self, item: &Item) -> Option<Sym> {
        item.kids.is_none().then(|| self.toks[item.from].cs()).flatten()
    }
    fn prim(&self, item: &Item) -> Option<P> {
        self.sym(item).and_then(|s| (self.meaning)(s).prim())
    }
    fn chr(&self, item: &Item) -> Option<char> {
        match self.toks[item.from].tok {
            Tok::Chr(c, Catcode::Other | Catcode::Letter) if item.kids.is_none() => Some(c),
            _ => None,
        }
    }
    fn space(&self, item: &Item) -> bool {
        item.kids.is_none() && self.toks[item.from].is_space()
    }
    fn fold_tok(&self, t: &Token) -> Option<Fold> {
        match (self.meaning)(t.cs()?).prim()? {
            P::If(_) | P::Unless => Some(Fold::Open),
            P::Else | P::Or => Some(Fold::Mid),
            P::Fi => Some(Fold::Close),
            _ => None,
        }
    }
    fn fold(&self, item: &Item) -> Option<Fold> {
        item.kids.is_none().then(|| self.fold_tok(&self.toks[item.from])).flatten()
    }
    /// Whether a statement starts at `item`: a name that is no register or constant.
    fn starts(&self, item: &Item) -> bool {
        self.sym(item).is_some_and(|s| !matches!((self.meaning)(s), Meaning::Register(..) | Meaning::Char(..)))
    }

    fn skip(&self, list: &[Item], mut j: usize) -> usize {
        while j < list.len() && self.space(&list[j]) {
            j += 1;
        }
        j
    }
    /// After one argument at `j`: a token or a group, a `\csname` through its `\endcsname`.
    fn arg(&self, list: &[Item], j: usize) -> usize {
        let j = self.skip(list, j);
        match list.get(j).and_then(|i| self.prim(i)) {
            Some(P::Csname | P::BeginCsname) => {
                (j + 1..list.len()).find(|&k| self.prim(&list[k]) == Some(P::Endcsname)).map_or(list.len(), |k| k + 1)
            }
            _ => (j + 1).min(list.len()),
        }
    }
    /// After the text from `j` through the first `close` character.
    fn through(&self, list: &[Item], j: usize, close: char) -> usize {
        (j..list.len()).find(|&k| self.chr(&list[k]) == Some(close)).map_or(list.len(), |k| k + 1)
    }
    /// After a number or dimension a test reads at `j` (tex.web § 440, § 448), roughly: a name, or
    /// characters up to a space, a relation or a name; a dimension's factor takes the name after it.
    fn term(&self, list: &[Item], dim: bool) -> impl Fn(usize) -> usize {
        move |j| {
            let mut j = self.skip(list, j);
            while list.get(j).and_then(|i| self.chr(i)).is_some_and(|c| c == '-' || c == '+') {
                j = self.skip(list, j + 1);
            }
            if list.get(j).is_some_and(|i| i.kids.is_none() && !matches!(self.toks[i.from].tok, Tok::Chr(..))) {
                // a name, and the register number after it (`\wd0`)
                j += 1;
                while list.get(j).and_then(|i| self.chr(i)).is_some_and(|c| c.is_ascii_digit()) {
                    j += 1;
                }
                return if list.get(j).is_some_and(|i| self.space(i)) { j + 1 } else { j };
            }
            let from = j;
            while list.get(j).and_then(|i| self.chr(i)).is_some_and(|c| !"<=>".contains(c)) {
                j += 1;
            }
            if dim && j > from && list.get(j).is_some_and(|i| self.sym(i).is_some() && self.fold(i).is_none()) {
                j += 1;
            }
            if list.get(j).is_some_and(|i| self.space(i)) { j + 1 } else { j }
        }
    }
    /// After a conditional's test at `j`.
    fn test(&self, list: &[Item], cond: P, j: usize) -> usize {
        let Some(cond) = (match cond {
            P::If(c) => Some(c),
            _ => None,
        }) else {
            // `\unless` and the conditional after it
            return match list.get(j).and_then(|i| self.prim(i)) {
                Some(c @ P::If(_)) => self.test(list, c, j + 1),
                _ => j,
            };
        };
        let rel = |j: usize| {
            let j = self.skip(list, j);
            if list.get(j).and_then(|i| self.chr(i)).is_some_and(|c| "<=>".contains(c)) { j + 1 } else { j }
        };
        let arg = |j| self.arg(list, j);
        match cond {
            Cond::IfX | Cond::Chars | Cond::Catcodes => arg(arg(j)),
            Cond::Defined | Cond::Primitive => arg(j),
            Cond::Csname => self.through_endcsname(list, j),
            Cond::Num | Cond::AbsNum => self.term(list, false)(rel(self.term(list, false)(j))),
            Cond::Dim | Cond::AbsDim => self.term(list, true)(rel(self.term(list, true)(j))),
            Cond::Odd | Cond::Case | Cond::Box | Cond::Eof => self.term(list, false)(j),
            Cond::FontChar => self.term(list, false)(arg(j)),
            _ => j,
        }
    }
    fn through_endcsname(&self, list: &[Item], j: usize) -> usize {
        (j..list.len()).find(|&k| self.prim(&list[k]) == Some(P::Endcsname)).map_or(list.len(), |k| k + 1)
    }
    /// After the statement at `i`: the name and the arguments its meaning reads.
    fn takes(&self, list: &[Item], i: usize) -> usize {
        let Some(sym) = self.sym(&list[i]) else { return i + 1 };
        let arg = |j| self.arg(list, j);
        match (self.meaning)(sym) {
            Meaning::Primitive(P::Prefix(_) | P::ExpandAfter) if i + 1 < list.len() => {
                self.takes(list, self.skip(list, i + 1))
            }
            Meaning::Primitive(P::Def { .. }) => {
                let mut j = arg(i + 1);
                while j < list.len() && list[j].kids.is_none() {
                    j += 1;
                }
                (j + 1).min(list.len())
            }
            Meaning::Primitive(P::Let { future: true, .. }) => arg(arg(arg(i + 1))),
            Meaning::Primitive(P::Let { .. }) => {
                let mut j = self.skip(list, arg(i + 1));
                if list.get(j).and_then(|t| self.chr(t)) == Some('=') {
                    j += 1;
                }
                // one optional space after `=`, then the token itself, a space too
                if list.get(j).is_some_and(|t| self.space(t)) && list.get(j + 1).is_some() {
                    j += 1;
                }
                (j + 1).min(list.len())
            }
            Meaning::Primitive(P::NoExpand | P::Text(_) | P::Convert(_)) => arg(i + 1),
            Meaning::Primitive(P::Csname | P::BeginCsname) => self.through_endcsname(list, i + 1),
            Meaning::Macro(m) => {
                let mut j = i + 1;
                let at = |j: usize| list.get(self.skip(list, j)).and_then(|t| self.chr(t));
                match &m.arg_spec {
                    Some(spec) => {
                        for a in &spec.items {
                            j = match a {
                                ArgType::Mandatory => arg(j),
                                ArgType::Optional(_) if at(j) == Some('[') => self.through(list, j, ']'),
                                ArgType::Delimited { open, close, .. } if at(j) == Some(*open) => {
                                    self.through(list, j, *close)
                                }
                                ArgType::Star if at(j) == Some('*') => self.skip(list, j) + 1,
                                ArgType::TokenFlag(c) if at(j) == Some(*c) => self.skip(list, j) + 1,
                                _ => j,
                            };
                        }
                    }
                    None => {
                        let params = &m.parameter_text.items;
                        for (k, p) in params.iter().enumerate() {
                            j = match (p, params.get(k + 1)) {
                                // delimited: up to its delimiter's first token
                                (ParamItem::Param(_), Some(ParamItem::Lit(t))) => (j..list.len())
                                    .find(|&n| list[n].kids.is_none() && self.toks[list[n].from].tok == *t)
                                    .unwrap_or(list.len()),
                                (ParamItem::Param(_), _) => arg(j),
                                (ParamItem::Lit(t), _)
                                    if list.get(j).is_some_and(|n| n.kids.is_none() && self.toks[n.from].tok == *t) =>
                                {
                                    j + 1
                                }
                                (ParamItem::Lit(_), _) => break,
                            };
                        }
                    }
                }
                j.min(list.len())
            }
            _ => i + 1,
        }
    }

    /// `list` one statement per line at `depth`, conditionals folded.
    fn block(&mut self, list: &[Item], depth: usize) {
        let mut i = 0;
        while i < list.len() {
            i = self.skip(list, i);
            if i >= list.len() {
                break;
            }
            if self.fold(&list[i]) == Some(Fold::Open)
                && let Some(next) = self.conditional(list, i, depth)
            {
                i = next;
                continue;
            }
            // a statement, then the text after it up to the next statement
            let mut j = if self.fold(&list[i]).is_some() { i + 1 } else { self.takes(list, i).max(i + 1) };
            while j < list.len() && !self.starts(&list[j]) {
                j += 1;
            }
            self.statement(&list[i..j], depth);
            i = j;
        }
    }

    /// The conditional opening at `i`, its arms indented; `None` when its `\fi` is not in `list`.
    fn conditional(&mut self, list: &[Item], i: usize, depth: usize) -> Option<usize> {
        let head = self.test(list, self.prim(&list[i])?, i + 1);
        let (mut nest, mut marks) = (0, Vec::new());
        let end = (head..list.len()).find(|&k| {
            match self.fold(&list[k]) {
                Some(Fold::Open) => nest += 1,
                Some(Fold::Mid) if nest == 0 => marks.push(k),
                Some(Fold::Close) if nest == 0 => return true,
                Some(Fold::Close) => nest -= 1,
                _ => {}
            }
            false
        })?;
        let line = self.join(&list[i..head]);
        self.push(depth, &line);
        let mut from = head;
        for k in marks.into_iter().chain([end]) {
            self.block(&list[from..k], depth + 1);
            let mark = self.flat(&list[k]);
            self.push(depth, &mark);
            from = k + 1;
        }
        Some(end + 1)
    }

    /// One statement: on one line when it fits, else each group that does not fit opened over lines.
    fn statement(&mut self, items: &[Item], depth: usize) {
        let flat = self.join(items);
        if 2 * depth + flat.trim().chars().count() <= self.width {
            return self.push(depth, &flat);
        }
        let mut line = String::new();
        for item in items {
            let text = self.flat(item);
            match &item.kids {
                Some(kids)
                    if !kids.is_empty() && 2 * depth + line.chars().count() + text.chars().count() > self.width =>
                {
                    if self.word_end(&line) {
                        line.pop();
                    }
                    line.push('{');
                    self.push(depth, &line);
                    let commas: Vec<usize> = (0..kids.len()).filter(|&k| self.chr(&kids[k]) == Some(',')).collect();
                    if commas.is_empty() {
                        self.block(kids, depth + 1);
                    } else {
                        // a key list: one key per line
                        let mut from = 0;
                        for k in commas.into_iter().chain([kids.len()]) {
                            let key = self.join(&kids[from..(k + 1).min(kids.len())]);
                            self.push(depth + 1, &key);
                            from = k + 1;
                        }
                    }
                    line = if item.closed { "}".into() } else { String::new() };
                }
                _ => self.extend(&mut line, item),
            }
        }
        self.push(depth, &line);
    }
}

/// The pretty form of `mac`'s body when `type_plugins.pretty` is on and it differs from the one line.
pub fn of(analysis: &crate::analysis::machine::Analysis, mac: Option<&crate::engine::tex::MacroDef>) -> Option<String> {
    let cfg = &analysis.settings.type_plugins.pretty;
    let toks = &mac?.replacement_text;
    let shown = cfg.enabled.then(|| print(toks, &analysis.interner, &|s| analysis.env.meaning(s), cfg.width))?;
    shown.contains('\n').then_some(shown)
}
