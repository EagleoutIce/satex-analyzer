//! TeX's hyphenation (tex.web §§ 919-965): `\patterns` and `\hyphenation`
//! store their text per `\language`, and a paragraph's end marks where a
//! discretionary hyphen may go in each word.

use crate::analysis::machine::Machine;
use crate::engine::builtins::CodeTable;
use crate::engine::tex::{Catcode, Tok, Token};
use crate::engine::value::Value;
use std::collections::HashMap;
use std::rc::Rc;

/// Words by their letters, each with one value per gap: `\hyphenation`'s exceptions.
pub(crate) type Exceptions = HashMap<Vec<u32>, Vec<u8>>;

/// The current language's patterns and exceptions.
pub(crate) type HyphTables = (Rc<Patterns>, Exceptions);

/// Patterns by their letters, each with one value per gap (§ 923), and every
/// pattern's prefixes with none ([`parse_patterns`]).
pub(crate) struct Patterns {
    map: HashMap<Vec<u32>, Vec<u8>>,
    /// The gap values of words met before ([`apply`] of them): a function of the patterns
    /// alone, and a paragraph's words are hyphenated twice (its marks, its line breaks)
    /// and recur. At most `words` of them (`limits.hyph_words`).
    memo: std::cell::RefCell<HashMap<Vec<u32>, Rc<[u8]>>>,
    words: usize,
}

impl Patterns {
    /// § 923 of `word`, from the memo where it holds it.
    fn gaps(&self, word: &[u32]) -> Vec<u8> {
        if let Some(gaps) = self.memo.borrow().get(word) {
            return gaps.to_vec();
        }
        let gaps = apply(self, word);
        let mut memo = self.memo.borrow_mut();
        if memo.len() < self.words {
            memo.insert(word.to_vec(), Rc::from(gaps.as_slice()));
        }
        gaps
    }

    #[cfg(test)]
    fn get(&self, letters: &[u32]) -> Option<&Vec<u8>> {
        self.map.get(letters)
    }
}

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
                    Some(lc) if lc > 0 => char::from_u32(lc as u32).unwrap_or(crate::engine::tex::UNKNOWN_CHAR),
                    _ => crate::engine::tex::UNKNOWN_CHAR,
                },
                _ => crate::engine::tex::UNKNOWN_CHAR,
            };
            if c == crate::engine::tex::UNKNOWN_CHAR {
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
    /// set since the last end in [`crate::analysis::facts::Text::hyphens`], unless it
    /// is one line. Anything
    /// unknown marks every gap: the view then over-approximates.
    pub(crate) fn mark_hyphens(&mut self, one_line: bool) {
        let from = std::mem::replace(&mut self.out.facts.text.hyphenated, self.out.facts.text.at.len() as u32) as usize;
        if one_line {
            return;
        }
        // The paragraph's characters, read from the end: the text before it may be long.
        let text = &self.out.facts.text;
        let mut tail: Vec<char> = text.chars.chars().rev().take(text.at.len().saturating_sub(from)).collect();
        tail.reverse();
        let chars: Vec<(char, bool)> = (tail.into_iter().zip(&text.at[from.min(text.at.len())..]))
            .map(|(c, s)| (c, s.modes.within(crate::engine::mode::Modes::RESTRICTED_HORIZONTAL)))
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
            marks.extend(points(tables.as_ref(), &word, (left, right)).map(|j| (from + start + j) as u32));
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
    pub(crate) fn hyph_tables(&mut self) -> Option<HyphTables> {
        let lang = self.hyph_language().filter(|_| !self.engine_value_set("hyph.lost"))?;
        Some((self.parsed_patterns(lang)?, self.exceptions(lang)?))
    }

    /// The language's patterns, parsed once per stored text.
    fn parsed_patterns(&mut self, lang: i64) -> Option<Rc<Patterns>> {
        let text = self.hyph_text(&format!("patterns.{lang}"))?;
        if let Some((t, p)) = &self.hyph_cache
            && Rc::ptr_eq(t, &text)
        {
            return Some(p.clone());
        }
        let patterns = Rc::new(parse_patterns(&text, self.cfg.limits.hyph_words));
        self.hyph_cache = Some((text, patterns.clone()));
        Some(patterns)
    }

    /// The language's `\hyphenation` words, each with its marked gaps.
    fn exceptions(&mut self, lang: i64) -> Option<Exceptions> {
        Some(parse_exceptions(&self.hyph_text(&format!("hyphenation.{lang}"))?))
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

/// The gap values of a lowercased word (§ 923, § 931).
pub(crate) fn hyf((p, e): &HyphTables, w: &[u32]) -> Vec<u8> {
    e.get(w).cloned().unwrap_or_else(|| p.gaps(w))
}

/// Where a word of lowercased letters (`None` unknown) may take a discretionary hyphen: `j` is the gap
/// after letter `j` (§ 923), from `left` to `n - right` (§ 902). Unknown tables or letters: every gap.
pub(crate) fn points(
    tables: Option<&HyphTables>,
    word: &[Option<u32>],
    (left, right): (usize, usize),
) -> impl Iterator<Item = usize> {
    let n = word.len();
    let hyf = match (tables, word.iter().copied().collect::<Option<Vec<u32>>>()) {
        _ if n < left + right => Vec::new(),
        (Some(t), Some(w)) => hyf(t, &w),
        _ => vec![1u8; n + 1],
    };
    let gaps = if hyf.is_empty() { &[][..] } else { &hyf[left..=n - right] };
    (left..).zip(gaps.to_vec()).filter(|(_, h)| h % 2 == 1).map(|(j, _)| j)
}

/// `\patterns` text as stored: letters (`.` the word's edge, 0) and the gap values between.
/// `words`: how many words' gaps [`Patterns`] keeps.
pub(crate) fn parse_patterns(text: &[Token], words: usize) -> Patterns {
    let mut patterns = HashMap::new();
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
    // Each pattern's prefixes too, with no values: [`apply`] stops a scan where no pattern goes on,
    // as TeX's trie walk does (§ 923).
    let prefixes: Vec<Vec<u32>> = patterns.keys().flat_map(|p| (1..p.len()).map(|n| p[..n].to_vec())).collect();
    for p in prefixes {
        patterns.entry(p).or_default();
    }
    Patterns { map: patterns, memo: Default::default(), words }
}

/// `\hyphenation` text as stored: each word's letters, 1 at a gap marked `-`.
pub(crate) fn parse_exceptions(text: &[Token]) -> Exceptions {
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
    out
}

/// § 923: every pattern matching inside `.word.` raises the gaps it covers.
fn apply(patterns: &Patterns, word: &[u32]) -> Vec<u8> {
    let mut w = vec![0];
    w.extend_from_slice(word);
    w.push(0);
    let mut hyf = vec![0u8; word.len() + 1];
    for s in 0..w.len() {
        for e in s + 1..=w.len() {
            let Some(values) = patterns.map.get(&w[s..e]) else { break };
            for (k, &v) in values.iter().enumerate() {
                // Gap before `w[s+k]` is after word letter `s+k-1`.
                if let Some(g) = (s + k).checked_sub(1).and_then(|j| hyf.get_mut(j)) {
                    *g = (*g).max(v);
                }
            }
        }
    }
    hyf
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::tex::Span;

    fn text(s: &str) -> Vec<Token> {
        s.chars().map(|c| Token::new(Tok::Chr(c, Catcode::Other), Span::default())).collect()
    }

    /// Intuition: a scan from a word position stops once no pattern goes on, and still finds
    /// every pattern that matches: the prefixes stored with no values guide it as TeX's trie
    /// does. Pins [`apply`] against trying every substring, which it did per word before.
    #[test]
    fn stopping_where_no_pattern_goes_on_finds_every_match() {
        let patterns = parse_patterns(&text(".ab1c a1b b2c3d 4cd 1d. ab2cde x1y"), 0);
        let every = |word: &[u32]| {
            let w: Vec<u32> = [0].iter().chain(word).chain(&[0]).copied().collect();
            let mut hyf = vec![0u8; word.len() + 1];
            for s in 0..w.len() {
                for e in s + 1..=w.len() {
                    for (k, &v) in patterns.get(&w[s..e]).into_iter().flatten().enumerate() {
                        if let Some(g) = (s + k).checked_sub(1).and_then(|j| hyf.get_mut(j)) {
                            *g = (*g).max(v);
                        }
                    }
                }
            }
            hyf
        };
        for word in ["abcd", "abcde", "dcba", "abab", "cdcd", "xyab", "a", "bcdbcd"] {
            let w: Vec<u32> = word.chars().map(|c| c as u32).collect();
            assert_eq!(apply(&patterns, &w), every(&w), "{word}");
        }
        assert!(apply(&patterns, &[97, 98, 99, 100]).iter().any(|&v| v > 0));
    }

    /// Intuition: the words' memo is a cache of a function of the patterns: a word asked for
    /// again, before and after the memo is full, gets what the patterns give it. Pins
    /// [`Patterns::gaps`], which a paragraph's marks and its line breaks both ask.
    #[test]
    fn a_word_asked_for_again_gets_what_the_patterns_give() {
        let kept = parse_patterns(&text(".ab1c a1b b2c3d 4cd 1d."), 2);
        for word in ["abcd", "dcba", "abcd", "cdcd", "dcba", "cdcd", "bcd"] {
            let w: Vec<u32> = word.chars().map(|c| c as u32).collect();
            assert_eq!(kept.gaps(&w), apply(&kept, &w), "{word}");
        }
        assert_eq!(kept.memo.borrow().len(), 2);
    }
}
