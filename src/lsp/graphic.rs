//! The image plugin's model: an `\includegraphics`'s options as the crop,
//! rotation and size its dialog edits, and the options written back.
//! graphicx applies the keys in order: `angle` before `width` sizes the
//! rotated figure, `width` before `angle` the upright one.

use serde::{Deserialize, Serialize};

use super::tabular::group_end;

/// The dialog's state. Lengths stay as written (`0.8\linewidth`); a crop is
/// in bp, as `trim` reads a bare number.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
pub struct Image {
    pub file: String,
    #[serde(default)]
    pub star: bool,
    /// `[l, b, r, t]` in bp, cut from the figure.
    #[serde(default)]
    pub trim: Option<[f64; 4]>,
    /// The crop as written as a `viewport` (`llx lly urx ury`) rather than a `trim`.
    #[serde(default)]
    pub viewport: bool,
    #[serde(default)]
    pub clip: bool,
    #[serde(default)]
    pub angle: Option<f64>,
    #[serde(default)]
    pub width: Option<String>,
    #[serde(default)]
    pub height: Option<String>,
    #[serde(default)]
    pub scale: Option<f64>,
    #[serde(default)]
    pub keepaspectratio: bool,
    /// `decodearray`: `[Dmin Dmax]` per colour channel of a raster (PDF reference § 8.9.5.2).
    #[serde(default)]
    pub decode: Option<Vec<f64>>,
    /// Whether the size keys come before `angle`, so they size the upright figure.
    #[serde(default)]
    pub size_first: bool,
    /// Every key in the order written, those the dialog does not edit included.
    #[serde(default)]
    pub keys: Vec<(String, String)>,
    /// The figure's natural size in bp, `[w, h]`, as the engine reads the file.
    #[serde(default)]
    pub natural: Option<[f64; 2]>,
    /// The crop or size as written when the dialog cannot read it (`trim=\fboxsep 0 0 0`).
    #[serde(default)]
    pub unread: Vec<String>,
}

const SIZE: [&str; 4] = ["width", "height", "totalheight", "scale"];

/// A length in bp: a bare number is bp, as graphicx reads `trim` and `viewport`.
pub fn bp(text: &str) -> Option<f64> {
    let t = text.trim();
    let at = t.find(|c: char| c.is_ascii_alphabetic()).unwrap_or(t.len());
    let n: f64 = t[..at].trim().parse().ok()?;
    let per = match t[at..].trim() {
        "" | "bp" => 1.0,
        "pt" => 72.0 / 72.27,
        "mm" => 72.0 / 25.4,
        "cm" => 72.0 / 2.54,
        "in" => 72.0,
        "pc" => 12.0 * 72.0 / 72.27,
        "dd" => 1238.0 / 1157.0 * 72.0 / 72.27,
        "cc" => 12.0 * 1238.0 / 1157.0 * 72.0 / 72.27,
        "sp" => 72.0 / 72.27 / 65536.0,
        _ => return None,
    };
    Some(n * per)
}

/// A number as written: no trailing zeros, at most two decimals.
pub fn num(x: f64) -> String {
    let s = format!("{:.2}", (x * 100.0).round() / 100.0);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.into() }
}

/// The `\includegraphics` at the start of `s` and the offset past it.
pub fn parse(s: &str, natural: Option<[f64; 2]>) -> Option<(Image, usize)> {
    let rest = s.strip_prefix("\\includegraphics")?;
    let mut i = s.len() - rest.len();
    let star = rest.starts_with('*');
    i += usize::from(star);
    let mut opts = None;
    let skip = |i: usize| i + s[i..].len() - s[i..].trim_start().len();
    if s[skip(i)..].starts_with('[') {
        let j = skip(i);
        let e = group_end(s, j)?;
        opts = Some(&s[j + 1..e - 1]);
        i = e;
        // the old form `[llx,lly][urx,ury]` is a bounding box: not this dialog's
        if s[skip(i)..].starts_with('[') {
            return None;
        }
    }
    let j = skip(i);
    let e = group_end(s, j).filter(|_| s[j..].starts_with('{'))?;
    let mut img = Image { file: s[j + 1..e - 1].to_string(), star, natural, ..Default::default() };
    let keys = opts.map(|o| super::snapshot::read_keys(o, 0, o.len())).unwrap_or_default();
    img.keys = keys.into_iter().filter(|k| !k.key.is_empty()).map(|k| (k.key, k.value.unwrap_or_default())).collect();
    read(&mut img);
    Some((img, e))
}

/// The dialog's fields from `keys`.
fn read(img: &mut Image) {
    let mut angle_at = None;
    let mut size_at = Vec::new();
    for (k, (key, v)) in img.keys.clone().iter().enumerate() {
        let lens = || v.split_whitespace().map(bp).collect::<Option<Vec<f64>>>().filter(|l| l.len() == 4);
        match key.as_str() {
            "trim" | "viewport" => match (lens(), key == "viewport", img.natural) {
                (Some(l), false, _) => img.trim = Some([l[0], l[1], l[2], l[3]]),
                (Some(l), true, Some([w, h])) => {
                    img.trim = Some([l[0], l[1], w - l[2], h - l[3]]);
                    img.viewport = true;
                }
                _ => img.unread.push(key.clone()),
            },
            "clip" => img.clip = v.is_empty() || v == "true",
            "angle" => match v.trim().parse() {
                Ok(a) => {
                    img.angle = Some(a);
                    angle_at = Some(k);
                }
                Err(_) => img.unread.push(key.clone()),
            },
            "width" => img.width = Some(v.clone()),
            "height" => img.height = Some(v.clone()),
            "scale" => match v.trim().parse() {
                Ok(x) => img.scale = Some(x),
                Err(_) => img.unread.push(key.clone()),
            },
            "keepaspectratio" => img.keepaspectratio = v.is_empty() || v == "true",
            "decodearray" => match v.split_whitespace().map(str::parse).collect::<Result<Vec<f64>, _>>() {
                Ok(d) if !d.is_empty() && d.len() % 2 == 0 => img.decode = Some(d),
                _ => img.unread.push(key.clone()),
            },
            _ => {}
        }
        if SIZE.contains(&key.as_str()) {
            size_at.push(k);
        }
    }
    img.size_first = match (angle_at, size_at.first()) {
        (Some(a), Some(&s)) => s < a,
        _ => false,
    };
}

/// The options the dialog's state writes: the keys as written, each
/// changed in place, new ones in graphicx's order (crop, then rotation and
/// size as `size_first` says), keys the dialog does not edit kept.
pub fn options(img: &Image) -> Vec<(String, String)> {
    let crop = img.trim.filter(|t| t.iter().any(|x| *x != 0.0));
    let crop_value = |key: &str| {
        let t = crop?;
        let v = match (key, img.natural) {
            ("viewport", Some([w, h])) => [t[0], t[1], w - t[2], h - t[3]],
            _ => t,
        };
        Some(v.map(num).join(" "))
    };
    let crop_key = if img.viewport && img.natural.is_some() { "viewport" } else { "trim" };
    let flag = |b: bool| b.then(String::new);
    let wanted = |key: &str| -> Option<Option<String>> {
        Some(match key {
            "trim" | "viewport" if img.unread.iter().any(|u| u == key) => return None,
            "trim" | "viewport" => (key == crop_key).then(|| crop_value(key)).flatten(),
            "clip" => flag(
                img.clip && crop.is_some() || img.clip && img.unread.iter().any(|u| u == "trim" || u == "viewport"),
            ),
            "angle" if img.unread.iter().any(|u| u == "angle") => return None,
            "angle" => img.angle.filter(|a| *a != 0.0).map(num),
            "width" => img.width.clone().filter(|w| !w.trim().is_empty()),
            "height" => img.height.clone().filter(|w| !w.trim().is_empty()),
            "scale" if img.unread.iter().any(|u| u == "scale") => return None,
            "scale" => img.scale.filter(|s| *s != 1.0).map(num),
            "keepaspectratio" => flag(img.keepaspectratio && img.width.is_some() && img.height.is_some()),
            "decodearray" if img.unread.iter().any(|u| u == "decodearray") => return None,
            "decodearray" => img
                .decode
                .as_ref()
                .filter(|d| !d.is_empty())
                .map(|d| d.iter().map(|x| num(*x)).collect::<Vec<_>>().join(" ")),
            _ => return None,
        })
    };
    let mut out: Vec<(String, String)> = Vec::new();
    let mut done = Vec::new();
    for (k, v) in &img.keys {
        match wanted(k) {
            None => out.push((k.clone(), v.clone())),
            Some(Some(nv)) => {
                // a flag stays as written (`clip`, `clip=true`), a crop in its units while it is the same
                let same = |v: &str| {
                    v.split_whitespace().map(|x| bp(x).map(num)).collect::<Option<Vec<_>>>().map(|l| l.join(" "))
                };
                let keep = (nv.is_empty() && (v.is_empty() || v == "true")) || same(v).is_some_and(|s| s == nv);
                out.push((k.clone(), if keep { v.clone() } else { nv }));
                done.push(k.clone());
            }
            Some(None) => done.push(k.clone()),
        }
    }
    let order = ["trim", "viewport", "clip", "angle", "width", "height", "scale", "keepaspectratio", "decodearray"];
    for key in order {
        if done.iter().any(|d| d == key) {
            continue;
        }
        if let Some(Some(v)) = wanted(key) {
            let at = match key {
                "trim" | "viewport" | "clip" => {
                    out.iter().position(|(k, _)| !["trim", "viewport", "clip", "page"].contains(&k.as_str()))
                }
                _ => None,
            };
            out.insert(at.unwrap_or(out.len()), (key.to_string(), v));
        }
    }
    // graphicx reads them in order: the angle after the size when the size is of the upright figure
    let angle = out.iter().position(|(k, _)| k == "angle");
    let sizes: Vec<usize> =
        out.iter().enumerate().filter(|(_, (k, _))| SIZE.contains(&k.as_str())).map(|(i, _)| i).collect();
    if let (Some(a), Some(&first), Some(&last)) = (angle, sizes.first(), sizes.last()) {
        let item = out[a].clone();
        if img.size_first && a < last {
            out.remove(a);
            out.insert(last, item);
        } else if !img.size_first && a > first {
            out.remove(a);
            out.insert(first, item);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::snapshot::{Arg, Key, Node, canonical};

    /// The command as the printer writes it new.
    fn emit(img: &Image) -> String {
        let keys = options(img).into_iter().map(|(key, v)| Key { key, value: (!v.is_empty()).then_some(v) }).collect();
        let args = vec![Arg::keys('[', keys), Arg::text('{', &img.file)];
        canonical(&Node { name: "includegraphics".into(), star: img.star, args, ..Node::default() })
    }

    fn round(s: &str, natural: Option<[f64; 2]>) -> Image {
        let (img, end) = parse(s, natural).unwrap();
        assert_eq!(end, s.len());
        let again = parse(&emit(&img), natural).unwrap().0;
        assert_eq!(options(&again), options(&img), "{s}");
        img
    }

    #[test]
    fn options_round_trip_through_the_dialog() {
        let img = round("\\includegraphics[trim=1cm 0 0 2bp, clip, angle=90, width=0.5\\linewidth]{fig.pdf}", None);
        assert_eq!(img.trim.map(|t| t.map(num)), Some(["28.35", "0", "0", "2"].map(String::from)));
        assert!(img.clip && !img.size_first);
        let img = round("\\includegraphics*[width=3cm,angle=45,page=2,draft]{a}", None);
        assert!(img.star && img.size_first);
    }

    #[test]
    fn a_viewport_reads_and_writes_as_a_viewport() {
        let img = round("\\includegraphics[viewport=10 20 300 200,clip]{x.png}", Some([320.0, 240.0]));
        assert_eq!(img.trim, Some([10.0, 20.0, 20.0, 40.0]));
        let mut img = img;
        img.trim = Some([0.0, 0.0, 20.0, 40.0]);
        assert_eq!(emit(&img), "\\includegraphics[viewport=0 0 300 200, clip]{x.png}");
    }

    #[test]
    fn the_angle_moves_to_the_side_the_size_applies_to() {
        let (mut img, _) = parse("\\includegraphics[angle=90,width=3cm]{a}", None).unwrap();
        img.size_first = true;
        assert_eq!(emit(&img), "\\includegraphics[width=3cm, angle=90]{a}");
        img.angle = None;
        img.trim = Some([1.0, 2.0, 3.0, 4.0]);
        img.clip = true;
        assert_eq!(emit(&img), "\\includegraphics[trim=1 2 3 4, clip, width=3cm]{a}");
    }

    #[test]
    fn a_decode_array_reads_writes_and_drops() {
        let mut img = round("\\includegraphics[width=3cm, decodearray={1 0 1 0 1 0}]{a.png}", None);
        assert_eq!(img.decode, Some(vec![1.0, 0.0, 1.0, 0.0, 1.0, 0.0]));
        img.decode = Some(vec![0.0, 0.5]);
        assert_eq!(emit(&img), "\\includegraphics[width=3cm, decodearray=0 0.5]{a.png}");
        img.decode = None;
        assert_eq!(emit(&img), "\\includegraphics[width=3cm]{a.png}");
        let (img, _) = parse("\\includegraphics[decodearray=\\mine]{a.png}", None).unwrap();
        assert_eq!(
            (img.unread.as_slice(), emit(&img).as_str()),
            (["decodearray".to_string()].as_slice(), "\\includegraphics[decodearray=\\mine]{a.png}")
        );
        assert!(img.decode.is_none());
        let mut fresh = parse("\\includegraphics{a.png}", None).unwrap().0;
        fresh.decode = Some(vec![0.2, 1.0, 0.0, 0.8, 0.0, 1.0]);
        assert_eq!(emit(&fresh), "\\includegraphics[decodearray=0.2 1 0 0.8 0 1]{a.png}");
    }

    #[test]
    fn an_unread_crop_is_kept() {
        let (img, _) = parse("\\includegraphics[trim=\\fboxsep{} 0 0 0,clip]{a}", None).unwrap();
        assert_eq!(img.unread, ["trim"]);
        assert_eq!(emit(&img), "\\includegraphics[trim=\\fboxsep{} 0 0 0, clip]{a}");
    }
}
