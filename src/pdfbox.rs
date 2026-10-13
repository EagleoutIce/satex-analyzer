//! The page size `\pdfximage` gives an image of a one-page PDF file (the
//! pdfTeX manual, "Graphics"): its crop box, else its media box, in big
//! points. Compressed object streams are inflated (RFC 1950/1951).
//! Anything this reader cannot be sure of (a rotation, a user unit, boxes
//! that disagree, a crop box apart from the media box on several pages)
//! gives no size.

/// The width and height in scaled points of the box `which` ("media", or
/// "crop" which falls back to the media box) of the only page of `pdf`:
/// each corner rounded to scaled points, one big point 65781.76sp, as
/// pdfTeX's results show.
pub fn page_box(pdf: &[u8], which: &str) -> Option<(i64, i64)> {
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
        if find(text, b"/UserUnit").is_some() || numbers_after(text, b"/Rotate").iter().any(|r| r.first() != Some(&0.0))
        {
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
    let b = match which {
        "media" => media?,
        _ => crop.or(media)?,
    };
    let sp = |bp: f64| (bp * 65781.76).round() as i64;
    // Every box of the file agrees, so a page's own or inherited box is
    // that one; past one page only where no crop box differs from it.
    let certain = pages == 1 || (pages > 1 && crop.is_none_or(|c| Some(c) == media));
    certain.then(|| ((sp(b[2]) - sp(b[0])).abs(), (sp(b[3]) - sp(b[1])).abs()))
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
        let (w, h) = super::page_box(pdf.as_bytes(), "crop").unwrap();
        assert_eq!((w, h), ((486.0f64 * 65781.76).round() as i64, (720.0f64 * 65781.76).round() as i64));
        let cropped = format!("{pdf} << /Type /Page /CropBox [0 0 100 100] >>");
        assert_eq!(super::page_box(cropped.as_bytes(), "crop"), None);
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

