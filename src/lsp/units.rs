//! A dimension written in the source (`2.5cm`, `-3pt`, `1in`): hover shows
//! what TeX makes of it, in scaled points, and the same length in every
//! unit TeX reads (tex.web § 458).

use crate::engine::value::{UNIT, UNITS, parse_dimen};

/// The dimension under column `col` (characters, from 1) of `line`, as
/// Markdown: TeX's scaled points and every unit's value.
pub fn hover(line: &str, col: u32) -> Option<String> {
    let chars: Vec<char> = line.chars().collect();
    let at = (col as usize).checked_sub(1).filter(|&i| i < chars.len())?;
    let number = |c: char| c.is_ascii_digit() || c == '.' || c == ',';
    let letter = |c: char| c.is_ascii_alphabetic();
    // the number and its unit around the cursor, the cursor on either
    let mut start = at;
    while start > 0
        && (letter(chars[start]) || number(chars[start]))
        && (letter(chars[start - 1]) || number(chars[start - 1]))
    {
        start -= 1;
    }
    while start < chars.len() && letter(chars[start]) {
        start += 1;
    }
    let digits = chars[start..].iter().take_while(|c| number(**c)).count();
    let units = chars[start + digits..].iter().take_while(|c| letter(**c)).count();
    if digits == 0 || units == 0 || start + digits + units <= at || (start > 0 && chars[start - 1] == '\\') {
        return None;
    }
    let negative = start > 0 && chars[start - 1] == '-';
    let written: String = chars[start..start + digits + units].iter().collect();
    let unit = written[digits..].to_ascii_lowercase();
    // `2truecm` is `2cm` before `\mag` (§ 457)
    let unit = unit.strip_prefix("true").unwrap_or(&unit);
    if !UNITS.iter().any(|(u, _, _)| *u == unit && *u != "mu") {
        return None;
    }
    let sp = parse_dimen(&format!("{}{unit}", &written[..digits]))? * if negative { -1 } else { 1 };
    let shown = if negative { format!("-{written}") } else { written };
    let mut out =
        format!("### `{shown}`\n\nTeX stores {sp} sp, the 65536th part of a point.\n\n| unit | value |\n|---|---|\n");
    for &(name, n, d) in UNITS.iter().filter(|(u, _, _)| *u != "mu") {
        let value = sp as f64 / UNIT as f64 * d as f64 / n as f64;
        out.push_str(&format!("| {name} | {} |\n", trimmed(value)));
    }
    out.push_str("\n`em` and `ex` depend on the font in force where the length is read.");
    Some(out)
}

/// `v` with at most five decimals and no trailing zeros.
fn trimmed(v: f64) -> String {
    let s = format!("{v:.5}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::hover;

    #[test]
    fn a_length_in_cm_converts_to_every_unit() {
        let line = "\\setlength{\\x}{72.27pt}";
        let col = line.find("pt").unwrap() as u32 + 1;
        let shown = hover(line, col).expect("a hover");
        assert!(shown.contains("| in | 1 |") && shown.contains("| cm | 2.54 |"), "{shown}");
        assert!(hover("\\vskip 1pt\\relax", 9).is_some());
        // a control sequence's letters are no unit
        assert!(hover("\\vskip 1pt\\relax", 12).is_none());
        assert!(hover("\\cm", 2).is_none());
    }
}
