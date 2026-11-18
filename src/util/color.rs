//! Colors as red, green and blue, and the strings that spell them: hex, the models a document
//! writes, a driver's color specials.

/// Red, green and blue from 0 to 1.
pub type Rgb = [f64; 3];

/// The color a driver's `\current@color` sets, from the numbers it carries: `0 1 0 rg 0 1 0 RG`
/// (pdfTeX), `color push rgb 0 1 0` (dvips), `pdf: bc [0 1 0]` (dvipdfmx). One number is a gray,
/// three an RGB color, four CMYK. `None` for text without numbers: no color set, or a named one.
pub fn driver_rgb(spec: &str) -> Option<Rgb> {
    let number = |t: &str| t.trim_matches(['[', ']']).parse::<f64>().ok();
    let words: Vec<&str> = spec.split_whitespace().collect();
    let start = words.iter().position(|t| number(t).is_some())?;
    let run: Vec<f64> = words[start..].iter().map_while(|t| number(t)).collect();
    let rgb = match run[..] {
        [g] => [g; 3],
        [r, g, b] => [r, g, b],
        [c, m, y, k] => [(1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k)],
        _ => return None,
    };
    Some(rgb.map(|v| v.clamp(0.0, 1.0)))
}

/// `#rrggbb`.
pub fn hex(c: Rgb) -> String {
    let b = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

/// The color in each model the document can write (xcolor's or the color package's), as xcolor's
/// conversions give it (xcolor § 6.3).
pub fn specs(c: Rgb, xcolor: bool) -> Vec<(&'static str, String)> {
    let n = |v: f64| {
        let s = format!("{:.4}", v.clamp(0.0, 1.0));
        let s = s.trim_end_matches('0').trim_end_matches('.');
        if s.is_empty() { "0".to_string() } else { s.to_string() }
    };
    let join = |vs: &[f64]| vs.iter().map(|v| n(*v)).collect::<Vec<_>>().join(",");
    let [r, g, b] = c;
    let k = 1.0 - r.max(g).max(b);
    let cmyk = if k >= 1.0 {
        [0.0, 0.0, 0.0, 1.0]
    } else {
        [(1.0 - r - k) / (1.0 - k), (1.0 - g - k) / (1.0 - k), (1.0 - b - k) / (1.0 - k), k]
    };
    let gray = 0.3 * r + 0.59 * g + 0.11 * b;
    let mut out = vec![("rgb", join(&c)), ("cmyk", join(&cmyk)), ("gray", n(gray))];
    if xcolor {
        let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        out.insert(0, ("HTML", format!("{:02X}{:02X}{:02X}", byte(r), byte(g), byte(b))));
        out.insert(2, ("RGB", format!("{},{},{}", byte(r), byte(g), byte(b))));
        out.push(("hsb", join(&hsb(c))));
    }
    out
}

pub fn hsb([r, g, b]: Rgb) -> [f64; 3] {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    let h = match () {
        _ if d == 0.0 => 0.0,
        _ if max == r => ((g - b) / d).rem_euclid(6.0),
        _ if max == g => (b - r) / d + 2.0,
        _ => (r - g) / d + 4.0,
    } / 6.0;
    [h, if max == 0.0 { 0.0 } else { d / max }, max]
}
