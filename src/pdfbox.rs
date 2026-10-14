//! The page size `\pdfximage` gives an image of a PDF file (the pdfTeX
//! manual, "Graphics"): the page's crop box, else its media box, in big
//! points, the crop box clipped to the media box, width and height swapped
//! by a `/Rotate` of 90 or 270, as pdfTeX (through its PDF library) reads
//! them. The page is found in the page tree (`/Kids` in order, `/MediaBox`,
//! `/CropBox` and `/Rotate` inherited, PDF 1.7 § 7.7.3), objects in
//! compressed object streams included (RFC 1950/1951). Anything this
//! reader cannot be sure of (a user unit, an object defined twice over
//! differently, a tree whose `/Count` disagrees with its pages, a box or
//! rotation it cannot read) gives no size.

use std::collections::HashMap;

/// A length in big points as pdfTeX makes it scaled points: through a
/// single-precision `float`, then times 65781.76 rounded, which
/// `pdflatex` shows (439.363bp is 28902072sp, not 28902071sp).
fn bp_to_sp(bp: f64) -> i64 {
    (f64::from(bp as f32) * 65781.76 + 0.5).floor() as i64
}

/// The width and height in scaled points of the box `which` ("media", or
/// "crop", which falls back to the media box) of page `page` (from 1) of
/// `pdf`.
pub fn page_box(pdf: &[u8], which: &str, page: i64) -> Option<(i64, i64)> {
    if find(pdf, b"/UserUnit").is_some() || page < 1 {
        return None;
    }
    match Objects::read(pdf).and_then(|objects| objects.page(page)) {
        Some(Some(attrs)) => attrs.size(which),
        // The tree was read and the page is not in it, or a box or rotation
        // there could not be.
        Some(None) => None,
        None if page == 1 => only_box(pdf, which),
        None => None,
    }
}

/// The page attributes a page inherits from the tree above it.
#[derive(Clone, Copy, Default)]
struct Attrs {
    media: Option<[f64; 4]>,
    crop: Option<[f64; 4]>,
    rotate: i64,
    /// One of them, here or above, could not be read.
    unread: bool,
}

impl Attrs {
    fn size(&self, which: &str) -> Option<(i64, i64)> {
        let media = self.media?;
        let b = match which {
            "media" => media,
            "crop" => {
                let c = self.crop.unwrap_or(media);
                [c[0].max(media[0]), c[1].max(media[1]), c[2].min(media[2]), c[3].min(media[3])]
            }
            _ => return None,
        };
        let (w, h) = (b[2] - b[0], b[3] - b[1]);
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        match self.rotate.rem_euclid(360) {
            0 | 180 => Some((bp_to_sp(w), bp_to_sp(h))),
            90 | 270 => Some((bp_to_sp(h), bp_to_sp(w))),
            _ => None,
        }
    }
}

/// The objects of a PDF file by number, each with every distinct body it
/// was given, and the catalog the trailers name.
struct Objects<'a> {
    bodies: HashMap<u32, Vec<std::borrow::Cow<'a, [u8]>>>,
    root: u32,
}

impl<'a> Objects<'a> {
    fn read(pdf: &'a [u8]) -> Option<Self> {
        let mut bodies: HashMap<u32, Vec<std::borrow::Cow<'a, [u8]>>> = HashMap::new();
        let mut roots = Vec::new();
        let mut add = |n: u32, body: std::borrow::Cow<'a, [u8]>| {
            let list = bodies.entry(n).or_default();
            if !list.iter().any(|b| trim(b) == trim(&body)) {
                list.push(body);
            }
        };
        let mut at = 0;
        let mut streams = Vec::new();
        while let Some(i) = find(&pdf[at..], b"obj") {
            let k = at + i;
            at = k + 3;
            let Some(n) = object_number(pdf, k) else { continue };
            if pdf.get(k + 3).is_some_and(|c| !is_delimiter(*c)) {
                continue;
            }
            let rest = &pdf[k + 3..];
            let end = find(rest, b"endobj").unwrap_or(rest.len());
            let body = &rest[..end];
            match stream_start(body) {
                Some(s) => {
                    let dict = &body[..s.0];
                    // The stream's data, skipped whole: no `obj` in it is one.
                    let len = value(dict, b"/Length").and_then(|v| int(v).filter(|_| reference(v).is_none()));
                    let data_end = match len.and_then(|l| usize::try_from(l).ok()) {
                        Some(l) if s.1 + l <= rest.len() => s.1 + l,
                        _ => find(&rest[s.1..], b"endstream").map(|e| s.1 + e)?,
                    };
                    at = k + 3 + data_end;
                    let kind = value(dict, b"/Type").and_then(name);
                    if kind == Some(&b"XRef"[..]) {
                        roots.extend(value(dict, b"/Root").and_then(reference));
                    }
                    if kind == Some(&b"ObjStm"[..]) {
                        streams.push((dict, &rest[s.1..data_end]));
                    }
                    add(n, std::borrow::Cow::Borrowed(dict));
                }
                None => {
                    at = k + 3 + end;
                    add(n, std::borrow::Cow::Borrowed(body));
                }
            }
        }
        let mut t = 0;
        while let Some(i) = find(&pdf[t..], b"trailer") {
            t += i + 7;
            roots.extend(value(skip_space(&pdf[t..]), b"/Root").and_then(reference));
        }
        for (dict, data) in streams {
            // Only plain deflate: a predictor or another filter is not read.
            if value(dict, b"/DecodeParms").is_some() {
                continue;
            }
            let data = match value(dict, b"/Filter").map(name) {
                None => data.to_vec(),
                Some(Some(b"FlateDecode")) => match inflate_zlib(data) {
                    Some(d) => d,
                    None => continue,
                },
                _ => continue,
            };
            let (Some(count), Some(first)) = (value(dict, b"/N").and_then(int), value(dict, b"/First").and_then(int))
            else {
                continue;
            };
            let first = usize::try_from(first).ok()?;
            let head: Vec<i64> = std::str::from_utf8(data.get(..first)?)
                .ok()?
                .split_ascii_whitespace()
                .map(|w| w.parse().ok())
                .collect::<Option<_>>()?;
            if head.len() != 2 * usize::try_from(count).ok()? {
                return None;
            }
            for (j, pair) in head.chunks(2).enumerate() {
                let start = first + usize::try_from(pair[1]).ok()?;
                let end =
                    head.get(2 * j + 3).map_or(Some(data.len()), |o| usize::try_from(*o).ok().map(|o| first + o))?;
                let body = data.get(start..end)?.to_vec();
                add(u32::try_from(pair[0]).ok()?, std::borrow::Cow::Owned(body));
            }
        }
        roots.sort_unstable();
        roots.dedup();
        let [root] = roots[..] else { return None };
        Some(Self { bodies, root })
    }

    /// The one body of object `n`.
    fn get(&self, n: u32) -> Option<&[u8]> {
        match self.bodies.get(&n)?.as_slice() {
            [body] => Some(body),
            _ => None,
        }
    }

    /// `v`, or what the object it refers to holds.
    fn resolve<'b>(&'b self, v: &'b [u8]) -> Option<&'b [u8]> {
        match reference(v) {
            Some(n) => self.get(n).map(trim),
            None => Some(v),
        }
    }

    /// The attributes of page `page`: `None` where the tree cannot be read,
    /// `Some(None)` where the page or its attributes cannot.
    fn page(&self, page: i64) -> Option<Option<Attrs>> {
        let catalog = self.get(self.root)?;
        let pages = reference(value(catalog, b"/Pages")?)?;
        let mut found = Vec::new();
        let mut seen = std::collections::HashSet::new();
        self.walk(pages, Attrs::default(), &mut found, &mut seen, 0)?;
        let count = self.resolve(value(self.get(pages)?, b"/Count")?).and_then(int)?;
        if usize::try_from(count).ok()? != found.len() {
            return None;
        }
        Some(found.get(usize::try_from(page - 1).ok()?).copied().flatten())
    }

    fn walk(
        &self,
        node: u32,
        mut attrs: Attrs,
        found: &mut Vec<Option<Attrs>>,
        seen: &mut std::collections::HashSet<u32>,
        depth: usize,
    ) -> Option<()> {
        if depth > 64 || !seen.insert(node) {
            return None;
        }
        let dict = self.get(node)?;
        for (key, slot) in [(&b"/MediaBox"[..], &mut attrs.media), (&b"/CropBox"[..], &mut attrs.crop)] {
            if let Some(v) = value(dict, key) {
                match self.resolve(v).and_then(rectangle) {
                    Some(r) => *slot = Some(r),
                    None => attrs.unread = true,
                }
            }
        }
        if let Some(v) = value(dict, b"/Rotate") {
            match self.resolve(v).and_then(int) {
                Some(r) => attrs.rotate = r,
                None => attrs.unread = true,
            }
        }
        let kind = value(dict, b"/Type").and_then(name);
        match value(dict, b"/Kids") {
            Some(kids) if kind != Some(&b"Page"[..]) => {
                let kids = self.resolve(kids)?;
                let inner = kids.strip_prefix(b"[")?.strip_suffix(b"]")?;
                for kid in references(inner)? {
                    self.walk(kid, attrs, found, seen, depth + 1)?;
                }
            }
            _ if kind == Some(&b"Page"[..]) => found.push((!attrs.unread).then_some(attrs)),
            _ => return None,
        }
        Some(())
    }
}

/// The page box when every page of the file has one and the same, read
/// without the page tree: the one page, or pages that all have one
/// media box and no crop box apart from it.
fn only_box(pdf: &[u8], which: &str) -> Option<(i64, i64)> {
    let mut texts = vec![pdf.to_vec()];
    let mut at = 0;
    while let Some(i) = find(&pdf[at..], b"stream") {
        let start = at + i + 6;
        at = start;
        let body = match pdf.get(start..)? {
            [b'\r', b'\n', ..] => start + 2,
            [b'\n', ..] => start + 1,
            _ => continue,
        };
        if let Some(text) = inflate_zlib(&pdf[body..]) {
            texts.push(text);
        }
    }
    let (mut pages, mut media, mut crop) = (0, None, None);
    for text in &texts {
        let mut k = 0;
        while let Some(i) = find(&text[k..], b"/Type") {
            k += i + 5;
            let rest = skip_space(&text[k..]);
            if rest.starts_with(b"/Page") && !rest[5..].first().is_some_and(|c| c.is_ascii_alphanumeric()) {
                pages += 1;
            }
        }
        if numbers_after(text, b"/Rotate").iter().any(|r| r.first() != Some(&0.0)) {
            return None;
        }
        for (key, slot) in [(&b"/MediaBox"[..], &mut media), (&b"/CropBox"[..], &mut crop)] {
            for b in numbers_after(text, key) {
                if b.len() != 4 || slot.is_some_and(|s: [f64; 4]| s[..] != b[..]) {
                    return None;
                }
                *slot = Some([b[0], b[1], b[2], b[3]]);
            }
        }
    }
    let certain = pages == 1 || (pages > 1 && crop.is_none_or(|c| Some(c) == media));
    let norm = |b: [f64; 4]| [b[0].min(b[2]), b[1].min(b[3]), b[0].max(b[2]), b[1].max(b[3])];
    certain.then_some(())?;
    Attrs { media: media.map(norm), crop: crop.map(norm), ..Attrs::default() }.size(which)
}

fn is_delimiter(c: u8) -> bool {
    c.is_ascii_whitespace() || b"()<>[]{}/%".contains(&c)
}

fn trim(s: &[u8]) -> &[u8] {
    let s = skip_space(s);
    let n = s.iter().rev().take_while(|c| c.is_ascii_whitespace()).count();
    &s[..s.len() - n]
}

/// `N G` before the `obj` at `k`, the object number.
fn object_number(pdf: &[u8], k: usize) -> Option<u32> {
    let before = &pdf[..k];
    let digits = |s: &[u8]| s.iter().rev().take_while(|c| c.is_ascii_digit()).count();
    let ws = |s: &[u8]| s.iter().rev().take_while(|c| c.is_ascii_whitespace()).count();
    let a = ws(before);
    if a == 0 {
        return None;
    }
    let generation = digits(&before[..k - a]);
    let b = ws(&before[..k - a - generation]);
    let num_end = k - a - generation - b;
    let num = digits(&before[..num_end]);
    if generation == 0 || b == 0 || num == 0 || (num_end > num && !is_delimiter(before[num_end - num - 1])) {
        return None;
    }
    std::str::from_utf8(&before[num_end - num..num_end]).ok()?.parse().ok()
}

/// Where a stream's dictionary ends and its data begins, if the body is a
/// stream.
fn stream_start(body: &[u8]) -> Option<(usize, usize)> {
    let close = rfind_dict_end(body)?;
    let rest = &body[close..];
    let s = skip_space(rest);
    let at = close + (rest.len() - s.len());
    let data = match s.strip_prefix(b"stream")? {
        [b'\r', b'\n', ..] => at + 8,
        [b'\n', ..] => at + 7,
        _ => return None,
    };
    Some((close, data))
}

/// The end of the dictionary a body starts with.
fn rfind_dict_end(body: &[u8]) -> Option<usize> {
    let s = skip_space(body);
    let off = body.len() - s.len();
    s.starts_with(b"<<").then_some(())?;
    Some(off + skip_value(s)?)
}

/// The length of the object at the start of `s`.
fn skip_value(s: &[u8]) -> Option<usize> {
    let mut depth = 0i32;
    let mut i = 0;
    while i < s.len() {
        match s[i] {
            b'(' => {
                let mut nest = 0;
                while i < s.len() {
                    match s[i] {
                        b'\\' => i += 1,
                        b'(' => nest += 1,
                        b')' => {
                            nest -= 1;
                            if nest == 0 {
                                break;
                            }
                        }
                        _ => {}
                    }
                    i += 1;
                }
            }
            b'%' => {
                while i < s.len() && s[i] != b'\n' && s[i] != b'\r' {
                    i += 1;
                }
            }
            b'<' if s.get(i + 1) == Some(&b'<') => {
                depth += 1;
                i += 1;
            }
            b'>' if s.get(i + 1) == Some(&b'>') => {
                depth -= 1;
                i += 1;
            }
            b'<' => {
                while i < s.len() && s[i] != b'>' {
                    i += 1;
                }
            }
            b'[' => depth += 1,
            b']' => depth -= 1,
            _ => {}
        }
        i += 1;
        if depth == 0 {
            // A dictionary or array ends here; anything else at the first
            // delimiter after it.
            if matches!(s[0], b'<' | b'[' | b'(') {
                return Some(i);
            }
            let n = s[1..].iter().position(|c| is_delimiter(*c)).map_or(s.len(), |p| p + 1);
            return Some(n);
        }
    }
    None
}

/// The value of `key` in the dictionary `dict` itself (not in one inside
/// it), as its raw text: a number, a name, an array, a dictionary, or an
/// indirect reference `N G R`.
fn value<'a>(dict: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let s = skip_space(dict);
    let mut i = s.strip_prefix(b"<<").map(|_| 2)?;
    loop {
        i += s[i..].len() - skip_space(&s[i..]).len();
        let rest = &s[i..];
        if rest.starts_with(b">>") || rest.is_empty() {
            return None;
        }
        // A key: a name.
        rest.starts_with(b"/").then_some(())?;
        let klen = 1 + rest[1..].iter().position(|c| is_delimiter(*c)).unwrap_or(rest.len() - 1);
        let this = &rest[..klen];
        i += klen;
        i += s[i..].len() - skip_space(&s[i..]).len();
        let v = &s[i..];
        let mut vlen = if v.starts_with(b"/") {
            1 + v[1..].iter().position(|c| is_delimiter(*c)).unwrap_or(v.len() - 1)
        } else {
            skip_value(v)?
        };
        if let Some(n) = reference_len(v) {
            vlen = n;
        }
        if this == key {
            return Some(&v[..vlen]);
        }
        i += vlen;
    }
}

/// The length of the indirect reference `N G R` at the start of `v`.
fn reference_len(v: &[u8]) -> Option<usize> {
    let digits = |at: usize| v[at..].iter().take_while(|c| c.is_ascii_digit()).count();
    let spaces = |at: usize| v[at..].iter().take_while(|c| c.is_ascii_whitespace()).count();
    let mut at = digits(0);
    for want_digits in [false, true, false] {
        let n = if want_digits { digits(at) } else { spaces(at) };
        if n == 0 {
            return None;
        }
        at += n;
    }
    (v.get(at) == Some(&b'R') && v.get(at + 1).is_none_or(|c| is_delimiter(*c)) && at > 0).then_some(at + 1)
}

fn reference(v: &[u8]) -> Option<u32> {
    let mut it = std::str::from_utf8(v).ok()?.split_ascii_whitespace();
    let (n, g, r) = (it.next()?, it.next()?, it.next()?);
    (it.next().is_none() && r == "R" && g.parse::<u32>().is_ok()).then_some(())?;
    n.parse().ok()
}

fn references(v: &[u8]) -> Option<Vec<u32>> {
    let toks: Vec<&str> = std::str::from_utf8(v).ok()?.split_ascii_whitespace().collect();
    toks.len().is_multiple_of(3).then_some(())?;
    toks.chunks(3).map(|t| (t[2] == "R" && t[1].parse::<u32>().is_ok()).then(|| t[0].parse().ok()).flatten()).collect()
}

fn name(v: &[u8]) -> Option<&[u8]> {
    let v = trim(v).strip_prefix(b"/")?;
    Some(&v[..v.iter().position(|c| is_delimiter(*c)).unwrap_or(v.len())])
}

fn int(v: &[u8]) -> Option<i64> {
    std::str::from_utf8(trim(v)).ok()?.parse().ok()
}

/// `[a b c d]`, normalized so the first corner is the lower left one.
fn rectangle(v: &[u8]) -> Option<[f64; 4]> {
    let inner = trim(v).strip_prefix(b"[")?.strip_suffix(b"]")?;
    let n: Vec<f64> =
        std::str::from_utf8(inner).ok()?.split_ascii_whitespace().map(|w| w.parse().ok()).collect::<Option<_>>()?;
    let [a, b, c, d] = n[..] else { return None };
    Some([a.min(c), b.min(d), a.max(c), b.max(d)])
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

fn skip_space(s: &[u8]) -> &[u8] {
    let n = s.iter().take_while(|c| c.is_ascii_whitespace()).count();
    &s[n..]
}

/// The numbers after each `key`, bracketed or one alone.
fn numbers_after(text: &[u8], key: &[u8]) -> Vec<Vec<f64>> {
    let mut out = Vec::new();
    let mut k = 0;
    while let Some(i) = find(&text[k..], key) {
        k += i + key.len();
        let rest = skip_space(&text[k..]);
        let (body, bracketed) = match rest.strip_prefix(b"[") {
            Some(r) => (&r[..r.iter().position(|&c| c == b']').unwrap_or(r.len())], true),
            None => (&rest[..rest.iter().position(|&c| c == b'/' || c == b'>').unwrap_or(rest.len())], false),
        };
        let nums: Vec<f64> = std::str::from_utf8(body)
            .unwrap_or("")
            .split_ascii_whitespace()
            .map_while(|w| w.parse().ok())
            .take(if bracketed { 4 } else { 1 })
            .collect();
        // An indirect reference (`/MediaBox 3 0 R`) is not followed.
        if nums.is_empty() || (!bracketed && body.contains(&b'R')) {
            out.push(Vec::new());
        } else {
            out.push(nums);
        }
    }
    out
}

/// RFC 1950: a two-byte header, then deflate data.
fn inflate_zlib(data: &[u8]) -> Option<Vec<u8>> {
    let (cmf, flg) = (*data.first()?, *data.get(1)?);
    if cmf & 0x0f != 8 || (u16::from(cmf) << 8 | u16::from(flg)) % 31 != 0 || flg & 0x20 != 0 {
        return None;
    }
    inflate(&data[2..])
}

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    bit: u32,
}

impl Bits<'_> {
    fn get(&mut self, n: u32) -> Option<u32> {
        let mut v = 0;
        for i in 0..n {
            let byte = *self.data.get(self.pos)?;
            v |= u32::from((byte >> self.bit) & 1) << i;
            self.bit += 1;
            if self.bit == 8 {
                self.bit = 0;
                self.pos += 1;
            }
        }
        Some(v)
    }

    /// One symbol of a canonical Huffman code given as counts per length
    /// and symbols in order.
    fn decode(&mut self, h: &(Vec<u16>, Vec<u16>)) -> Option<u16> {
        let (mut code, mut first, mut index) = (0i32, 0i32, 0i32);
        for len in 1..h.0.len() {
            code |= self.get(1)? as i32;
            let count = i32::from(h.0[len]);
            if code - count < first {
                return h.1.get((index + code - first) as usize).copied();
            }
            index += count;
            first = (first + count) << 1;
            code <<= 1;
        }
        None
    }
}

fn huffman(lengths: &[u8]) -> (Vec<u16>, Vec<u16>) {
    let mut counts = vec![0u16; 16];
    for &l in lengths {
        counts[l as usize] += 1;
    }
    counts[0] = 0;
    let mut offs = [0u16; 16];
    for len in 1..15 {
        offs[len + 1] = offs[len] + counts[len];
    }
    let mut symbols = vec![0u16; lengths.len()];
    for (s, &l) in lengths.iter().enumerate() {
        if l != 0 {
            symbols[offs[l as usize] as usize] = s as u16;
            offs[l as usize] += 1;
        }
    }
    (counts, symbols)
}

const LBASE: [u16; 29] =
    [3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258];
const LEXT: [u8; 29] = [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DBASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
    8193, 12289, 16385, 24577,
];
const DEXT: [u8; 30] =
    [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

/// RFC 1951, at most 64 MiB out.
fn inflate(data: &[u8]) -> Option<Vec<u8>> {
    let mut b = Bits { data, pos: 0, bit: 0 };
    let mut out: Vec<u8> = Vec::new();
    loop {
        let last = b.get(1)?;
        match b.get(2)? {
            0 => {
                if b.bit != 0 {
                    (b.bit, b.pos) = (0, b.pos + 1);
                }
                let len = usize::from(u16::from_le_bytes([*data.get(b.pos)?, *data.get(b.pos + 1)?]));
                out.extend_from_slice(data.get(b.pos + 4..b.pos + 4 + len)?);
                b.pos += 4 + len;
            }
            kind @ (1 | 2) => {
                let (lit, dist) = match kind {
                    1 => {
                        let mut l = [8u8; 288];
                        l[144..256].fill(9);
                        l[256..280].fill(7);
                        (huffman(&l), huffman(&[5u8; 30]))
                    }
                    _ => {
                        let (nlen, ndist, ncode) =
                            (b.get(5)? as usize + 257, b.get(5)? as usize + 1, b.get(4)? as usize + 4);
                        const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
                        let mut cl = [0u8; 19];
                        for &o in &ORDER[..ncode] {
                            cl[o] = b.get(3)? as u8;
                        }
                        let ch = huffman(&cl);
                        let mut lengths = Vec::with_capacity(nlen + ndist);
                        while lengths.len() < nlen + ndist {
                            match b.decode(&ch)? {
                                s @ 0..=15 => lengths.push(s as u8),
                                16 => {
                                    let prev = *lengths.last()?;
                                    lengths.extend(std::iter::repeat_n(prev, 3 + b.get(2)? as usize));
                                }
                                17 => lengths.extend(std::iter::repeat_n(0, 3 + b.get(3)? as usize)),
                                _ => lengths.extend(std::iter::repeat_n(0, 11 + b.get(7)? as usize)),
                            }
                        }
                        (huffman(lengths.get(..nlen)?), huffman(lengths.get(nlen..nlen + ndist)?))
                    }
                };
                loop {
                    match b.decode(&lit)? {
                        s @ 0..=255 => out.push(s as u8),
                        256 => break,
                        s => {
                            let i = usize::from(s - 257);
                            let len = usize::from(*LBASE.get(i)?) + b.get(u32::from(LEXT[i]))? as usize;
                            let d = usize::from(b.decode(&dist)?);
                            let back = usize::from(*DBASE.get(d)?) + b.get(u32::from(DEXT[d]))? as usize;
                            let from = out.len().checked_sub(back)?;
                            for k in 0..len {
                                out.push(out[from + k]);
                            }
                        }
                    }
                }
            }
            _ => return None,
        }
        if out.len() > 64 << 20 {
            return None;
        }
        if last == 1 {
            return Some(out);
        }
    }
}

/// The size in scaled points `\pdfximage` gives a PNG or JPEG file: its
/// pixels at the file's resolution (PNG `pHYs` in pixels per meter, JFIF
/// in dots per inch or per centimetre, each rounded as C's `(int)(x+0.5)`),
/// else at `default_dpi` (`\pdfimageresolution`), one pixel at r dpi being
/// 7227·65536/(100·r) sp rounded (pdfTeX's `ext_xn_over_d`). Resolutions
/// below 5 dpi count as none (pdfTeX test 37). Checked against pdfTeX
/// 1.40.28 on 296 TeX Live images. `None` where pdfTeX might read it
/// otherwise: one axis tiny and the other not, a resolution past 65535,
/// EXIF data without JFIF density, any header this reader does not know.
pub fn raster_size(data: &[u8], default_dpi: i64) -> Option<(i64, i64)> {
    let (w, h, xr, yr) = png_info(data).or_else(|| jpeg_info(data))?;
    let tiny = |r: i64| (1..5).contains(&r);
    let (xr, yr) = match (xr, yr) {
        (x, y) if x > 65535 || y > 65535 => return None,
        (x, y) if tiny(x) != tiny(y) => return None,
        (x, y) if tiny(x) && tiny(y) => (0, 0),
        (x, 0) if x > 0 => (x, x),
        (0, y) if y > 0 => (y, y),
        xy => xy,
    };
    let (xr, yr) = if xr == 0 { (default_dpi, default_dpi) } else { (xr, yr) };
    if xr <= 0 || yr <= 0 {
        return None;
    }
    let scale = |px: i64, r: i64| (7227 * 65536 * px * 2 + 100 * r) / (2 * 100 * r);
    Some((scale(w, xr), scale(h, yr)))
}

/// The width and height in scaled points of the page `epstopdf` (TeX
/// Live's `epstopdf.pl`, without `--hires` or `--exact`) makes of the EPS
/// file `eps`: the header's `%%BoundingBox: llx lly urx ury` moved to the
/// origin, so a media box `[0 0 urx-llx ury-lly]` in big points, read as
/// [`page_box`] reads it. Only what the script's header scan surely finds:
/// a file that starts with `%!`, `\n` line ends, a box of four integers
/// before the header ends (a line not starting with `%`, or
/// `%%EndComments`). A DOS EPS binary file (Adobe TN 5002: `C5D0D3C6`, then
/// the PostScript section's offset and length, little-endian) is that
/// section. An `(atend)` box or anything else gives no size.
pub fn eps_page_size(eps: &[u8]) -> Option<(i64, i64)> {
    if eps.len() > 30 && eps.starts_with(b"\xC5\xD0\xD3\xC6") {
        let word = |at: usize| u32::from_le_bytes([eps[at], eps[at + 1], eps[at + 2], eps[at + 3]]) as usize;
        let (offset, length) = (word(4), word(8));
        return eps_page_size(eps.get(offset..offset.checked_add(length)?)?);
    }
    if !eps.starts_with(b"%!") {
        return None;
    }
    for line in eps.split(|&b| b == b'\n').skip(1) {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        if line.contains(&b'\r') {
            return None;
        }
        if line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if !line.starts_with(b"%") || line.starts_with(b"%%EndComments") {
            return None;
        }
        let Some(rest) = line.strip_prefix(b"%%BoundingBox:") else { continue };
        let rest = String::from_utf8_lossy(rest);
        let n: Vec<i64> = rest.split_whitespace().take(4).map_while(|v| v.parse().ok()).collect();
        // `(atend)`, fractions, or anything else the script may read otherwise.
        let [llx, lly, urx, ury] = n[..] else { return None };
        let (w, h) = (urx - llx, ury - lly);
        let sp = |bp: i64| (bp as f64 * 65781.76).round() as i64;
        return (w > 0 && h > 0).then(|| (sp(w), sp(h)));
    }
    None
}

/// Width, height and resolution (0 none) of a PNG file.
fn png_info(b: &[u8]) -> Option<(i64, i64, i64, i64)> {
    if !b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    let be = |at: usize| -> Option<i64> { Some(i64::from(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))) };
    let (w, h) = (be(16)?, be(20)?);
    let (mut xr, mut yr, mut i) = (0, 0, 8);
    while i + 8 <= b.len() {
        let n = usize::try_from(be(i)?).ok()?;
        match b.get(i + 4..i + 8)? {
            b"pHYs" if *b.get(i + 16)? == 1 => {
                let r = |ppm: i64| (ppm * 254 + 5000) / 10000;
                (xr, yr) = (r(be(i + 8)?), r(be(i + 12)?));
            }
            b"IDAT" | b"IEND" => break,
            _ => {}
        }
        i += 12 + n;
    }
    Some((w, h, xr, yr))
}

/// Width, height and resolution (0 none) of a JPEG file.
fn jpeg_info(b: &[u8]) -> Option<(i64, i64, i64, i64)> {
    if !b.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let be = |at: usize| -> Option<i64> { Some(i64::from(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))) };
    let (mut xr, mut yr, mut jfif, mut exif, mut i) = (0, 0, false, false, 2);
    while i + 4 <= b.len() {
        if b[i] != 0xff {
            return None;
        }
        let m = b[i + 1];
        let n = usize::try_from(be(i + 2)?).ok()?;
        if m == 0xe0 && b.get(i + 4..i + 9) == Some(b"JFIF\0") {
            jfif = true;
            let (dx, dy) = (be(i + 12)?, be(i + 14)?);
            (xr, yr) = match b.get(i + 11)? {
                1 => (dx, dy),
                2 => ((dx * 254 + 50) / 100, (dy * 254 + 50) / 100),
                _ => (0, 0),
            };
        }
        if m == 0xe1 && b.get(i + 4..i + 8) == Some(b"Exif") {
            exif = true;
        }
        if matches!(m, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
            if exif && (!jfif || xr == 0) {
                return None;
            }
            return Some((be(i + 7)?, be(i + 5)?, xr, yr));
        }
        i += 2 + n;
    }
    None
}

#[cfg(test)]
mod tests {
    #[test]
    fn stored_block_inflates() {
        // zlib header, one final stored block "abc".
        let data = [0x78, 0x01, 0x01, 0x03, 0x00, 0xfc, 0xff, b'a', b'b', b'c'];
        assert_eq!(super::inflate_zlib(&data).as_deref(), Some(&b"abc"[..]));
    }

    /// Pages that all have one media box give it; a crop box on some
    /// page only leaves the size unknown.
    #[test]
    fn pages_of_one_box_give_it() {
        let page = "<< /Type /Page /Parent 1 0 R /MediaBox [0 0 486 720] >>";
        let pdf = format!("%PDF-1.5 << /Type /Pages /Count 2 >> {page} {page}");
        let (w, h) = super::page_box(pdf.as_bytes(), "crop", 1).unwrap();
        assert_eq!((w, h), ((486.0f64 * 65781.76).round() as i64, (720.0f64 * 65781.76).round() as i64));
        let cropped = format!("{pdf} << /Type /Page /CropBox [0 0 100 100] >>");
        assert_eq!(super::page_box(cropped.as_bytes(), "crop", 1), None);
    }

    /// Objects in an object stream, written without spaces between
    /// tokens (`/Count 2/Kids[...]`), and a page past the tree's last.
    #[test]
    fn a_page_tree_in_an_object_stream_gives_each_page() {
        let objs = [
            "<</Type/Pages/Count 2/Kids[3 0 R 4 0 R]/MediaBox[0 0 100.3 50]>>",
            "<</Type/Page/Parent 2 0 R/Rotate 90>>",
            "<</Type/Page/Parent 2 0 R/CropBox[10 10 20 20]>>",
        ];
        let mut head = String::new();
        let mut data = String::new();
        for (i, o) in objs.iter().enumerate() {
            head.push_str(&format!("{} {} ", i + 2, data.len()));
            data.push_str(o);
            data.push(' ');
        }
        let stream = format!("{head}{data}");
        let pdf = format!(
            "%PDF-1.5\n1 0 obj\n<</Type/Catalog/Pages 2 0 R>>\nendobj\n5 0 obj\n<</Type/ObjStm/N 3/First {}/Length {}>>stream\n{stream}\nendstream\nendobj\ntrailer\n<</Root 1 0 R/Size 6>>\n%%EOF\n",
            head.len(),
            stream.len()
        );
        let sp = |bp: f64| super::bp_to_sp(bp);
        assert_eq!(super::page_box(pdf.as_bytes(), "crop", 1), Some((sp(50.0), sp(100.3))));
        assert_eq!(super::page_box(pdf.as_bytes(), "crop", 2), Some((sp(10.0), sp(10.0))));
        assert_eq!(super::page_box(pdf.as_bytes(), "media", 2), Some((sp(100.3), sp(50.0))));
        assert_eq!(super::page_box(pdf.as_bytes(), "crop", 3), None);
        assert_eq!(sp(100.3), 6597911);
    }

    /// epstopdf 2.36 makes `%%BoundingBox: 10 -20 295 61` a media box
    /// `[0 0 285 81]` (its `%%HiResBoundingBox` unread); a box the header
    /// does not hold surely gives no size.
    #[test]
    fn an_eps_box_moves_to_the_origin() {
        let eps = |bb: &str| {
            format!(
                "%!PS-Adobe-3.0 EPSF-3.0\n{bb}\n%%HiResBoundingBox: 10.2 -19.5 294.9 60.8\n%%EndComments\nshowpage\n"
            )
        };
        let bp = |n: f64| (n * 65781.76).round() as i64;
        assert_eq!(super::eps_page_size(eps("%%BoundingBox: 10 -20 295 61").as_bytes()), Some((bp(285.0), bp(81.0))));
        // The same as the PostScript section of a DOS EPS binary file.
        let ps = eps("%%BoundingBox: 10 -20 295 61").into_bytes();
        let mut dos = b"\xC5\xD0\xD3\xC6".to_vec();
        for word in [32u32, ps.len() as u32, 0, 0, 0, 0] {
            dos.extend(word.to_le_bytes());
        }
        dos.extend([0xff, 0xff, 0, 0]);
        dos.extend(&ps);
        dos.extend(b"II*\0 preview");
        assert_eq!(super::eps_page_size(&dos), Some((bp(285.0), bp(81.0))));
        for bb in ["%%BoundingBox: (atend)", "%%BoundingBox: 0 0 1.5 2", "%%Title: x", "x %%BoundingBox: 0 0 9 9"] {
            assert_eq!(super::eps_page_size(eps(bb).as_bytes()), None, "{bb}");
        }
    }

    /// A PNG header: `w`×`h` pixels, with a `pHYs` chunk of `ppm` pixels
    /// per meter when given.
    fn png(w: u32, h: u32, ppm: Option<u32>) -> Vec<u8> {
        let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
        let chunk = |b: &mut Vec<u8>, t: &[u8], data: &[u8]| {
            b.extend((data.len() as u32).to_be_bytes());
            b.extend(t);
            b.extend(data);
            b.extend([0; 4]);
        };
        let mut ihdr = w.to_be_bytes().to_vec();
        ihdr.extend(h.to_be_bytes());
        ihdr.extend([8, 2, 0, 0, 0]);
        chunk(&mut b, b"IHDR", &ihdr);
        if let Some(ppm) = ppm {
            let mut phys = ppm.to_be_bytes().to_vec();
            phys.extend(ppm.to_be_bytes());
            phys.push(1);
            chunk(&mut b, b"pHYs", &phys);
        }
        chunk(&mut b, b"IDAT", &[]);
        b
    }

    /// pdfTeX 1.40.28's sizes: 72 dpi without a resolution, 17500 pixels
    /// per meter rounding up to 445 dpi.
    #[test]
    fn png_sizes_are_pdftex_s() {
        let pt = |sp: i64| sp as f64 / 65536.0;
        let (w, h) = super::raster_size(&png(1000, 500, None), 72).unwrap();
        assert_eq!((w, h), (1000 * 7227 * 65536 / 7200, 500 * 7227 * 65536 / 7200));
        let (w, _) = super::raster_size(&png(2608, 221, Some(17500)), 72).unwrap();
        assert!((pt(w) - 423.55092).abs() < 1e-4, "{}", pt(w));
    }

    /// A JFIF density below 5 dpi counts as none; 5 counts.
    #[test]
    fn tiny_jpeg_resolutions_are_ignored() {
        let jpeg = |dpi: u16| {
            let mut b = vec![0xff, 0xd8, 0xff, 0xe0, 0, 16];
            b.extend(b"JFIF\0");
            b.extend([1, 1, 1]);
            b.extend(dpi.to_be_bytes());
            b.extend(dpi.to_be_bytes());
            b.extend([0, 0]);
            b.extend([0xff, 0xc0, 0, 11, 8]);
            b.extend(1000u16.to_be_bytes());
            b.extend(1000u16.to_be_bytes());
            b.extend([1, 1, 0x11, 0]);
            b
        };
        let pt = |dpi| super::raster_size(&jpeg(dpi), 72).map(|s| s.0 as f64 / 65536.0);
        assert_eq!(pt(4), Some(1003.75));
        assert_eq!(pt(5), Some(14454.0));
    }
}
