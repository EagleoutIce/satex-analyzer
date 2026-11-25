//! When two texts count as the same: one ladder of levels every comparison
//! of text picks from, so a rule never folds case, spaces or punctuation by a
//! private recipe. Each level holds everything the stricter ones do.

/// How far two texts may differ and still be the same, strictest first.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    /// Character for character.
    Exact,
    /// The same glyphs on the page: a ligature character is its letters
    /// (`ﬁ` is `fi`), a curly quote its straight one, and any run of spaces,
    /// no-break spaces and line ends one space.
    #[default]
    Glyphs,
    /// [`Level::Glyphs`], and upper and lower case alike (`TinyNet`, `tinynet`).
    Case,
    /// Only the letters and digits, case alike: spaces, hyphens and
    /// punctuation do not count (`Tiny-Net`, `tiny net`).
    Letters,
}

/// `text` as `level` compares it: two texts are the same at `level` when
/// their keys are equal.
pub fn key(text: &str, level: Level) -> String {
    if level == Level::Exact {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    let fold = level >= Level::Case;
    for c in text.chars().flat_map(glyph).flat_map(|c| {
        let lower = fold.then(|| c.to_lowercase());
        lower.into_iter().flatten().chain((!fold).then_some(c))
    }) {
        if level == Level::Letters {
            if c.is_alphanumeric() {
                out.push(c);
            }
        } else if c.is_whitespace() {
            space = !out.is_empty();
        } else {
            if std::mem::take(&mut space) {
                out.push(' ');
            }
            out.push(c);
        }
    }
    out
}

/// Whether `a` and `b` are the same at `level`.
pub fn same(a: &str, b: &str, level: Level) -> bool {
    key(a, level) == key(b, level)
}

/// One character as its plain glyphs: the ligature code points (U+FB00–FB06)
/// spelled out, curly quotes straight, a no-break space a space.
fn glyph(c: char) -> impl Iterator<Item = char> {
    let s: &'static str = match c {
        'ﬀ' => "ff",
        'ﬁ' => "fi",
        'ﬂ' => "fl",
        'ﬃ' => "ffi",
        'ﬄ' => "ffl",
        'ﬅ' | 'ﬆ' => "st",
        '‘' | '’' => "'",
        '“' | '”' => "\"",
        '\u{a0}' | '\u{202f}' | '~' => " ",
        _ => "",
    };
    let one = s.is_empty().then_some(c);
    one.into_iter().chain(s.chars())
}

#[cfg(test)]
mod tests {
    use super::{Level, key, same};

    /// Each level holds the stricter ones: a pair the same at one level is the
    /// same at every looser one, and the ladder separates what it should.
    #[test]
    fn levels_nest() {
        let pairs = [
            ("TinyNet", "TinyNet"),
            ("e\u{fb03}cient", "efficient"),
            ("a  b", "a\u{a0}b"),
            ("TinyNet", "tinynet"),
            ("Tiny-Net", "tiny net"),
        ];
        let first = [Level::Exact, Level::Glyphs, Level::Glyphs, Level::Case, Level::Letters];
        let levels = [Level::Exact, Level::Glyphs, Level::Case, Level::Letters];
        for ((a, b), from) in pairs.iter().zip(first) {
            for l in levels {
                assert_eq!(same(a, b, l), l >= from, "{a:?} {b:?} at {l:?}");
            }
        }
        assert_eq!(key("  x  ", Level::Glyphs), "x");
    }
}
