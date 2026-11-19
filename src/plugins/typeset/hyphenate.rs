//! `satex/hyphenate`: where each of a list of words may take a discretionary
//! hyphen, as the run's own paragraph end marks a set word
//! ([`crate::engine::hyph`]): the patterns and exceptions of the language in
//! force, `\lefthyphenmin`, `\righthyphenmin` and the `\lccode`s.

use super::Facts;
use crate::engine::linebreak::Tables;

/// Per word the characters a hyphen may precede, each letter run a word of its
/// own (a character of `\lccode` 0 ends one); anything unknown marks every gap.
pub fn points(facts: &mut Facts, words: &[String]) -> Vec<Vec<u32>> {
    let tables = facts.patterns();
    let mins = facts.mins();
    words
        .iter()
        .map(|w| {
            let chars: Vec<char> = w.chars().collect();
            let mut out = Vec::new();
            let mut i = 0;
            while i < chars.len() {
                let start = i;
                let mut word = Vec::new();
                while let Some(&c) = chars.get(i) {
                    match facts.lower(c as u32) {
                        Some(0) => break,
                        l => word.push(l.map(|v| v as u32)),
                    }
                    i += 1;
                }
                i += usize::from(word.is_empty());
                out.extend(crate::engine::hyph::points(tables.as_ref(), &word, mins).map(|j| (start + j) as u32));
            }
            out
        })
        .collect()
}
