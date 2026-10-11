//! TeX's hyphenation (tex.web §§ 919-965): `\patterns` and `\hyphenation`
//! store their text per `\language`, and a paragraph's end marks where a
//! discretionary hyphen may go in each word.

use crate::builtins::CodeTable;
use crate::machine::Machine;
use crate::tex::{Catcode, Tok, Token};
use crate::value::Value;
use std::collections::HashMap;
use std::rc::Rc;

/// Patterns by their letters, each with one value per gap (§ 923).
pub(crate) type Patterns = HashMap<Vec<u32>, Vec<u8>>;

impl Machine<'_> {
    /// `\patterns` or `\hyphenation` (§§ 934, 960): the text, letters
    /// already lowercased by `\lccode`, is appended to the current
    /// language's. Text that is not plain characters makes it unknown.
    pub(crate) fn store_hyphenation(&mut self, patterns: bool, text: &[Token]) {
        let Some(lang) = self.hyph_language() else {
            self.set_engine("hyph.lost", Value::Unknown, true);
            return;
        };
        let key = format!("{}.{lang}", if patterns { "patterns" } else { "hyphenation" });
        let mut out: Vec<Token> = match self.engine_value(&key) {
            Value::Toks(t) => t.to_vec(),
            Value::Unknown if self.engine_value_set(&key) => return,
            _ => Vec::new(),
        };
        out.push(Token::new(Tok::Chr(' ', Catcode::Other), text.first().map(|t| t.span).unwrap_or_default()));
        for t in text {
            let c = match t.tok {
                Tok::Chr(_, Catcode::Space) => ' ',
                Tok::Chr(c, Catcode::Letter | Catcode::Other)
                    if c == '.' && patterns || c == '-' && !patterns || c.is_ascii_digit() && patterns =>
                {
                    c
                }
                Tok::Chr(c, Catcode::Letter | Catcode::Other) => match self.char_code_known(CodeTable::Lower, c) {
                    Some(lc) if lc > 0 => char::from_u32(lc as u32).unwrap_or(crate::tex::UNKNOWN_CHAR),
                    _ => crate::tex::UNKNOWN_CHAR,
                },
                _ => crate::tex::UNKNOWN_CHAR,
            };
            if c == crate::tex::UNKNOWN_CHAR {
                self.set_engine(&key, Value::Unknown, true);
                return;
            }
            out.push(Token::new(Tok::Chr(c, Catcode::Other), t.span));
        }
        self.set_engine(&key, Value::Toks(Rc::from(out)), true);
    }

    /// `\language` as § 934 reads it: 0 outside 1..255, `None` if unknown.
    fn hyph_language(&mut self) -> Option<i64> {
        let sym = self.intern("language");
        match self.env.value(sym) {
            Value::Unknown if self.env.get(sym).is_some() => None,
            v => Some(v.as_int().filter(|l| (1..=255).contains(l)).unwrap_or(0)),
        }
    }

    /// A paragraph ends (§ 891): mark the hyphenation points of the words it
    /// set since the last end in [`crate::facts::Text::hyphens`], unless it
    /// is one line. Anything
    /// unknown marks every gap: the view then over-approximates.
    pub(crate) fn mark_hyphens(&mut self, one_line: bool) {
        let from = std::mem::replace(&mut self.out.facts.text.hyphenated, self.out.facts.text.at.len() as u32) as usize;
        if one_line {
            return;
        }
        let chars: Vec<(char, bool)> = self
            .out
            .facts
            .text
            .chars
            .chars()
            .zip(&self.out.facts.text.at)
            .skip(from)
            .map(|(c, s)| (c, s.modes.within(crate::mode::Modes::RESTRICTED_HORIZONTAL)))
            .collect();
        if chars.is_empty() {
            return;
        }
        let (left, right) = self.hyphen_mins();
        let tables = self.hyph_tables();
        let mut marks = Vec::new();
        let mut i = 0;
        while i < chars.len() {
            let mut word = Vec::new();
            let start = i;
            while let Some(&(c, false)) = chars.get(i) {
                match self.char_code_known(CodeTable::Lower, c) {
                    Some(0) => break,
                    lc => word.push(lc.map(|v| v as u32)),
                }
                i += 1;
            }
            if word.is_empty() {
                i += 1;
                continue;
            }
            let n = word.len();
            if n < left + right {
                continue;
            }
            // Gap `j` lies after letter `j` (1-based), § 923.
            let hyf = match (&tables, word.iter().copied().collect::<Option<Vec<u32>>>()) {
                (Some(t), Some(w)) => self.hyf(t, &w),
                _ => vec![1u8; n + 1],
            };
            for j in left..=n - right {
                if hyf[j] % 2 == 1 {
                    marks.push((from + start + j) as u32);
                }
            }
        }
        self.out.facts.text.hyphens.extend(marks);
    }

    /// § 1091 `norm_min`: `\lefthyphenmin` and `\righthyphenmin`.
    pub(crate) fn hyphen_mins(&mut self) -> (usize, usize) {
        let mut int = |name: &str| {
            let sym = self.intern(name);
            self.env.value(sym).as_int().map_or(1, |v| v.clamp(1, 63)) as usize
        };
        (int("lefthyphenmin"), int("righthyphenmin"))
    }

    /// The current language's patterns and exceptions, `None` if unknown.
    pub(crate) fn hyph_tables(&mut self) -> Option<(Rc<Patterns>, Patterns)> {
        let lang = self.hyph_language().filter(|_| !self.engine_value_set("hyph.lost"))?;
        Some((self.parsed_patterns(lang)?, self.exceptions(lang)?))
    }

    /// The gap values of a lowercased word (§ 923, § 931).
    pub(crate) fn hyf(&self, (p, e): &(Rc<Patterns>, Patterns), w: &[u32]) -> Vec<u8> {
        e.get(w).cloned().unwrap_or_else(|| apply(p, w))
    }

    /// The language's patterns, parsed once per stored text.
    fn parsed_patterns(&mut self, lang: i64) -> Option<Rc<Patterns>> {
        let text = self.hyph_text(&format!("patterns.{lang}"))?;
        if let Some((t, p)) = &self.hyph_cache
            && Rc::ptr_eq(t, &text)
        {
            return Some(p.clone());
        }
        let mut patterns = Patterns::new();
        for word in text.split(|t| matches!(t.tok, Tok::Chr(' ', _))) {
            let (mut letters, mut values) = (Vec::new(), vec![0u8]);
            for t in word {
                if let Tok::Chr(c, _) = t.tok {
                    match c.to_digit(10) {
                        Some(d) => *values.last_mut().unwrap() = d as u8,
                        None => {
                            letters.push(if c == '.' { 0 } else { c as u32 });
                            values.push(0);
                        }
                    }
                }
            }
            if !letters.is_empty() {
                patterns.insert(letters, values);
            }
        }
        let patterns = Rc::new(patterns);
        self.hyph_cache = Some((text, patterns.clone()));
        Some(patterns)
    }

    /// The language's `\hyphenation` words, each with its marked gaps.
    fn exceptions(&mut self, lang: i64) -> Option<Patterns> {
        let text = self.hyph_text(&format!("hyphenation.{lang}"))?;
        let mut out = HashMap::new();
        for word in text.split(|t| matches!(t.tok, Tok::Chr(' ', _))) {
            let (mut letters, mut gaps) = (Vec::new(), vec![0u8]);
            for t in word {
                if let Tok::Chr(c, _) = t.tok {
                    if c == '-' {
                        *gaps.last_mut().unwrap() = 1;
                    } else {
                        letters.push(c as u32);
                        gaps.push(0);
                    }
                }
            }
            if !letters.is_empty() {
                out.insert(letters, gaps);
            }
        }
        Some(out)
    }

    fn engine_value(&mut self, key: &str) -> Value {
        let sym = self.engine_sym(key);
        self.env.value(sym)
    }

    fn engine_value_set(&mut self, key: &str) -> bool {
        let sym = self.engine_sym(key);
        self.env.get(sym).is_some()
    }

    /// A stored text, empty if never set, `None` if unknown.
    fn hyph_text(&mut self, key: &str) -> Option<Rc<[Token]>> {
        match self.engine_value(key) {
            Value::Toks(t) => Some(t),
            Value::Unknown if self.engine_value_set(key) => None,
            _ => Some(Rc::from(Vec::new())),
        }
    }
}

/// § 923: every pattern matching inside `.word.` raises the gaps it covers.
fn apply(patterns: &Patterns, word: &[u32]) -> Vec<u8> {
    let mut w = vec![0];
    w.extend_from_slice(word);
    w.push(0);
    let mut hyf = vec![0u8; word.len() + 1];
    for s in 0..w.len() {
        for e in s + 1..=w.len() {
            if let Some(values) = patterns.get(&w[s..e]) {
                for (k, &v) in values.iter().enumerate() {
                    // Gap before `w[s+k]` is after word letter `s+k-1`.
                    if let Some(g) = (s + k).checked_sub(1).and_then(|j| hyf.get_mut(j)) {
                        *g = (*g).max(v);
                    }
                }
            }
        }
    }
    hyf
}
