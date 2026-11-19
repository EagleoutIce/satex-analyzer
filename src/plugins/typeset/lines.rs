//! A paragraph's lines as the run's own breaker ([`crate::engine::linebreak::line_break`]) set
//! them: one place for the Typeset view, the echo and the comparison with pdfTeX. The run notes
//! where each line ends in its text ([`crate::analysis::facts::Text::lines`]), here mapped from
//! the broken list to characters and read back per paragraph.

use crate::analysis::facts::{LineEnd, Text};
#[cfg(test)]
use crate::engine::linebreak::{Breaking, Kp};

/// The lines of the paragraph set in characters `from..to` as `shown` shows each character (a
/// ligature at its first letter, the rest empty), a hyphen after one broken at a discretionary;
/// `None` where its lines are not all known. `off` says a character is not on the page with them
/// (a box lost unused, a footnote's text). A ligature a line break cuts shows its letters.
pub fn of_paragraph(
    text: &Text,
    shown: &[String],
    off: impl Fn(usize) -> bool,
    from: u32,
    to: u32,
) -> Option<Vec<String>> {
    let start = text.par_starts.iter().map(|s| s.0).find(|&s| (from..to).contains(&s))?;
    // nothing but space before the paragraph's start
    if (from..start).any(|i| !off(i as usize) && !shown[i as usize].trim().is_empty()) {
        return None;
    }
    let raw: Vec<char> = text.chars.chars().skip(start as usize).take((to - start) as usize).collect();
    let k = text.lines.partition_point(|l| l.0 <= start);
    let (mut lines, mut at) = (Vec::new(), start);
    for &(end, kind) in &text.lines[k..] {
        if kind == LineEnd::Unknown || end > to {
            return None;
        }
        // a ligature that runs on past the line's end: its letters
        let cut = (at..end)
            .rev()
            .find(|&i| !shown[i as usize].is_empty())
            .filter(|_| shown.get(end as usize).is_some_and(String::is_empty));
        let (mut line, mut head) = (String::new(), true);
        for i in (at..end).filter(|&i| !off(i as usize)) {
            let g = shown[i as usize].as_str();
            // the letters of a ligature begun on the line before, and of one cut at the end
            head &= g.is_empty();
            match head || cut.is_some_and(|c| i >= c) {
                true => line.push(raw[(i - start) as usize]),
                false => line.push_str(g),
            }
        }
        let mut line = line.trim().to_string();
        if kind == LineEnd::Hyphen {
            line.push('-');
        }
        lines.push(line);
        at = end;
        if end == to {
            return Some(lines);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::linebreak::Broken;

    const PT: crate::engine::value::Scaled = 1 << 16;

    fn letter(c: char) -> Kp {
        Kp::Box { w: 5 * PT, h: 0, d: 0, chr: Some((1, c as u32)), lig: 0 }
    }

    fn space() -> Kp {
        Kp::Glue { w: 3 * PT, st: PT, sto: 0, sh: PT, sho: 0 }
    }

    fn line(at: usize) -> Broken {
        Broken { height: 0, depth: 0, excess: 0, at, set: (0.0, 0) }
    }

    /// Intuition: "ab cd ef" with each glue pushed after its space was noted (counts 3 and 6): a line
    /// ending at glue ends after its space, one at a hyphen point after the letters before it, the
    /// last at the paragraph's end, past the `\parfillskip` the breaker appended. Pins line ends
    /// counted from the wrong item or lost on the appended glue.
    #[test]
    fn ends_count_characters_from_the_glue_before() {
        let items =
            vec![letter('a'), letter('b'), space(), letter('c'), letter('d'), space(), letter('e'), letter('f')];
        let chars = [1, 2, 3, 4, 5, 6, 7, 8];
        let mut broken = items.clone();
        broken.insert(7, Kp::Disc(Some((0, 0, 0)), 0, None));
        broken.extend([Kp::Penalty(10000), space()]);
        let b = Breaking {
            lines: vec![line(2), line(7), line(broken.len())],
            fit: 2,
            items: broken,
            points: vec![(7, 6, 1)],
        };
        assert_eq!(
            crate::engine::linebreak::line_ends(&items, &chars, &b, 0, 8),
            Some(vec![(3, LineEnd::Line), (7, LineEnd::Hyphen), (8, LineEnd::Line)])
        );
        // counts that do not rise (a list copied from a box at once) are no answer
        assert_eq!(crate::engine::linebreak::line_ends(&items, &[8; 8], &b, 0, 8), None);
    }

    /// Intuition: "waffles" set with the `ffl` ligature (shown at its first letter) and broken
    /// `waf-fles` inside it: each line shows the ligature's letters it holds. Pins `waffl-` / `es`.
    #[test]
    fn a_cut_ligature_shows_its_letters() {
        let text = Text {
            chars: "waffles".into(),
            par_starts: vec![(0, Default::default())],
            lines: vec![(3, LineEnd::Hyphen), (7, LineEnd::Line)],
            ..Text::default()
        };
        let shown: Vec<String> = ["w", "a", "ffl", "", "", "e", "s"].map(String::from).to_vec();
        assert_eq!(of_paragraph(&text, &shown, |_| false, 0, 7), Some(vec!["waf-".into(), "fles".into()]));
        let unknown = Text { lines: vec![(3, LineEnd::Unknown), (7, LineEnd::Line)], ..text };
        assert_eq!(of_paragraph(&unknown, &shown, |_| false, 0, 7), None);
    }
}
