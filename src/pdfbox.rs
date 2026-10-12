//! The page size `\pdfximage` gives an image of a one-page PDF file (the
//! pdfTeX manual, "Graphics"): its crop box, else its media box, in big
//! points. Compressed object streams are inflated (RFC 1950/1951).
//! Anything this reader cannot be sure of (several pages, a rotation, a
//! user unit, boxes that disagree) gives no size.

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
    (pages == 1).then(|| ((sp(b[2]) - sp(b[0])).abs(), (sp(b[3]) - sp(b[1])).abs()))
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

#[cfg(test)]
mod tests {
    #[test]
    fn stored_block_inflates() {
        // zlib header, one final stored block "abc".
        let data = [0x78, 0x01, 0x01, 0x03, 0x00, 0xfc, 0xff, b'a', b'b', b'c'];
        assert_eq!(super::inflate_zlib(&data).as_deref(), Some(&b"abc"[..]));
    }
}
