//! Finding a completion item by what it is as well as by its name: a
//! package by its description, a citation by its title, a label by its
//! caption. Typos are forgiven by edit distance on word prefixes.
//!
//! An item ranks by the best way it matches, then by a cost inside that way:
//! the name itself, its start, a part of it, its letters in order or a near
//! spelling of it, then every typed word starting a word of the text, then
//! every typed word near one.

use lsp_types::CompletionItem;

/// How an item matched, best first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Tier {
    Exact,
    Prefix,
    Part,
    NameFuzzy,
    Word,
    WordFuzzy,
}

/// An item to find: its name and what it is, lowercased once.
pub struct Entry {
    name: String,
    words: Vec<String>,
}

impl Entry {
    pub fn new(name: &str, text: &str) -> Entry {
        Entry { name: name.to_lowercase(), words: words(text) }
    }

    /// How `query` finds this entry and at what cost, `None` when it does not.
    pub fn score(&self, query: &Query) -> Option<(Tier, u32)> {
        let (name, q) = (self.name.as_str(), query.whole.as_str());
        let len = name.chars().count() as u32;
        // nothing typed keeps the caller's order
        if q.is_empty() {
            return Some((Tier::Prefix, 0));
        }
        if name.starts_with(q) {
            return Some(if name == q { (Tier::Exact, 0) } else { (Tier::Prefix, len) });
        }
        if let Some(at) = name.find(q) {
            // a part that starts after a separator beats one inside a word
            let boundary = !name[..at].ends_with(char::is_alphanumeric);
            return Some((Tier::Part, len + if boundary { 0 } else { 100 }));
        }
        if let Some(d) = near(q, name) {
            return Some((Tier::NameFuzzy, d * 100 + len));
        }
        if subsequence(name, q, allowed(q.chars().count()) + 1) {
            return Some((Tier::NameFuzzy, 300 + len));
        }
        // every typed word must start a word of the text: the first one's place is the cost
        let at = |exact: bool| -> Option<u32> {
            let mut cost = 0;
            for (k, w) in query.words.iter().enumerate() {
                let (i, d) = self.words.iter().enumerate().find_map(|(i, t)| {
                    if t.starts_with(w.as_str()) {
                        Some((i, 0))
                    } else if exact {
                        None
                    } else {
                        near(w, t).map(|d| (i, d))
                    }
                })?;
                cost += d * 100 + if k == 0 { i as u32 } else { 0 };
            }
            Some(cost)
        };
        if query.words.is_empty() {
            return None;
        }
        at(true).map(|c| (Tier::Word, c)).or_else(|| at(false).map(|c| (Tier::WordFuzzy, c)))
    }
}

/// What was typed: whole, and as words.
pub struct Query {
    whole: String,
    words: Vec<String>,
}

impl Query {
    pub fn new(typed: &str) -> Query {
        Query { whole: typed.trim().to_lowercase(), words: words(typed) }
    }
}

/// The lowercased words of `text`: runs of letters and digits.
fn words(text: &str) -> Vec<String> {
    // braces only protect a letter's case (`{T}he`): no word ends there
    let text = text.replace(['{', '}'], "");
    text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(str::to_lowercase).collect()
}

/// Whether `typed`'s characters occur in `text` in order from its start, with
/// at most `gaps` characters skipped between them (`tkzcd` for `tikz-cd`):
/// more would let a long name hold any word's letters.
fn subsequence(text: &str, typed: &str, gaps: u32) -> bool {
    let mut it = text.chars();
    let mut skipped = 0;
    typed.chars().count() > 2
        && typed.chars().all(|c| {
            for t in it.by_ref() {
                if t == c {
                    return true;
                }
                skipped += 1;
            }
            false
        })
        && skipped <= gaps
}

/// The typos allowed in a word of `n` characters: none below four, one below seven, else two.
fn allowed(n: usize) -> u32 {
    match n {
        0..4 => 0,
        4..7 => 1,
        _ => 2,
    }
}

/// The longest typed word a typo is looked for in: rows of the distance table live on the stack.
const LONGEST: usize = 32;

/// The fewest edits (insert, delete, substitute, swap neighbours) that turn
/// `typed` into a prefix of `word`, when within what its length allows.
fn near(typed: &str, word: &str) -> Option<u32> {
    let mut q = ['\0'; LONGEST];
    let mut n = 0;
    for c in typed.chars() {
        *q.get_mut(n)? = c;
        n += 1;
    }
    let max = allowed(n);
    if max == 0 {
        return None;
    }
    // the prefixes of `word` a typo of `typed` can reach
    let mut w = ['\0'; LONGEST + 2];
    let mut m = 0;
    for c in word.chars().take(n + max as usize) {
        w[m] = c;
        m += 1;
    }
    // three rows of the table: two back for a swap, the last and this one
    let (mut back, mut last, mut row) = ([0u32; LONGEST + 3], [0u32; LONGEST + 3], [0u32; LONGEST + 3]);
    for (j, cell) in last.iter_mut().enumerate().take(m + 1) {
        *cell = j as u32;
    }
    for i in 1..=n {
        row[0] = i as u32;
        let mut best = row[0];
        for j in 1..=m {
            let cost = u32::from(q[i - 1] != w[j - 1]);
            row[j] = (last[j] + 1).min(row[j - 1] + 1).min(last[j - 1] + cost);
            if i > 1 && j > 1 && q[i - 1] == w[j - 2] && q[i - 2] == w[j - 1] {
                row[j] = row[j].min(back[j - 2] + 1);
            }
            best = best.min(row[j]);
        }
        if best > max {
            return None;
        }
        (back, last) = (last, row);
    }
    let d = *last[..=m].iter().min()?;
    (d <= max).then_some(d)
}

/// The items `typed` finds by their label or by `text` (what each is), best
/// first, at most `limit`. Each is sorted by its tier and cost, the caller's
/// order among equals, and filtered by what was typed, so an editor neither
/// drops nor reorders what the text matched.
pub fn found(
    items: Vec<CompletionItem>,
    typed: &str,
    limit: usize,
    text: impl Fn(&CompletionItem) -> String,
) -> Vec<CompletionItem> {
    let query = Query::new(typed);
    let mut scored: Vec<((Tier, u32), CompletionItem)> = items
        .into_iter()
        .filter_map(|mut it| {
            let text = text(&it);
            let score = Entry::new(&it.label, &text).score(&query)?;
            it.filter_text = Some(format!("{typed} {} {text}", it.label));
            Some((score, it))
        })
        .collect();
    scored.sort_by(|(a, x), (b, y)| a.cmp(b).then_with(|| x.sort_text.cmp(&y.sort_text)).then(x.label.cmp(&y.label)));
    scored.truncate(limit);
    scored
        .into_iter()
        .enumerate()
        .map(|(k, (_, mut it))| {
            it.sort_text = Some(format!("{k:05}"));
            it
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tier(name: &str, text: &str, typed: &str) -> Option<Tier> {
        Entry::new(name, text).score(&Query::new(typed)).map(|s| s.0)
    }

    /// The order the ways of matching promise: the name exactly, its start,
    /// a part, a near spelling, then the text's words exactly, then nearly.
    #[test]
    fn tiers_rank_name_before_text() {
        assert_eq!(tier("tikz", "", "tikz"), Some(Tier::Exact));
        assert_eq!(tier("tikz-network", "", "tikz"), Some(Tier::Prefix));
        assert_eq!(tier("tikz-network", "", "network"), Some(Tier::Part));
        assert_eq!(tier("networkx", "", "netwrok"), Some(Tier::NameFuzzy));
        assert_eq!(tier("pst-net", "Network diagrams in PSTricks", "network"), Some(Tier::Word));
        assert_eq!(tier("pst-net", "Network diagrams in PSTricks", "netwrk"), Some(Tier::WordFuzzy));
        assert_eq!(tier("amsmath", "AMS mathematical facilities", "network"), None);
        // letters in order with few gaps are an abbreviation, with many a coincidence
        assert_eq!(tier("tikz-cd", "", "tkzcd"), Some(Tier::NameFuzzy));
        assert_eq!(tier("interactiveworkbook", "", "network"), None);
    }

    /// Short words allow no typo (`ab` would find every two-letter word), longer ones one or two.
    #[test]
    fn typos_scale_with_length() {
        assert_eq!(near("ab", "ac"), None);
        assert_eq!(near("grpah", "graphics"), Some(1));
        assert_eq!(near("bibliogrphy", "bibliography"), Some(1));
        assert_eq!(near("tabular", "xyzxyzxyz"), None);
    }

    /// Several typed words must each start a word of the text.
    #[test]
    fn every_word_must_match() {
        assert_eq!(tier("x", "Knuth Literate Programming 1984", "literate 1984"), Some(Tier::Word));
        assert_eq!(tier("x", "Knuth Literate Programming 1984", "literate 1999"), None);
    }
}
