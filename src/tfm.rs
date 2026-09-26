//! TeX font metric files (tex.web part 30): what `\font` loads, so that
//! `\fontdimen`, `\fontcharwd` and `\iffontchar` answer with the numbers the
//! engine would read.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::value::Scaled;

/// tex.web § 540: the twelve 16-bit lengths that open a TFM file.
const PREAMBLE_HALFWORDS: usize = 12;
/// tex.web § 568: a fix_word has 20 fractional bits, a scaled value 16.
const FIX_TO_SCALED_SHIFT: u32 = 4;
/// tex.web § 568: the design size is the second header word.
const DESIGN_SIZE_WORD: usize = 1;
/// tex.web § 575: parameter 1, the slant, is a pure number, not a length.
const SLANT: usize = 1;
/// tex.web § 572: `store_scaled` keeps z below 2^23 by halving it.
const Z_LIMIT: i64 = 1 << 23;

/// The parts of a TFM file TeX's font tables hold.
pub struct Metrics {
    pub design_size: Scaled,
    first_char: usize,
    char_info: Vec<[u8; 4]>,
    widths: Vec<i64>,
    heights: Vec<i64>,
    depths: Vec<i64>,
    italics: Vec<i64>,
    lig_kern: Vec<[u8; 4]>,
    kerns: Vec<i64>,
    params: Vec<i64>,
}

/// What a font's ligature/kerning program does between two characters
/// (tex.web § 545).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LigKern {
    Kern(Scaled),
    /// A ligature by its operation (`=:` is 0) and character.
    Lig {
        op: u8,
        char: u32,
    },
}

/// tex.web § 545: `stop_flag` and `kern_flag`.
const STOP_FLAG: u8 = 128;
const KERN_FLAG: u8 = 128;
/// tex.web § 544: the tag of a character with a ligature/kerning program.
const LIG_TAG: u8 = 1;

/// One character's box, scaled to a size.
#[derive(Clone, Copy, Default)]
pub struct CharBox {
    pub width: Scaled,
    pub height: Scaled,
    pub depth: Scaled,
    pub italic: Scaled,
}

fn fix_word(bytes: &[u8], at: usize) -> Option<i64> {
    let word: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
    Some(i64::from(i32::from_be_bytes(word)))
}

impl Metrics {
    pub fn parse(bytes: &[u8]) -> Option<Metrics> {
        let half = |i: usize| -> Option<usize> {
            let b = bytes.get(2 * i..2 * i + 2)?;
            Some(usize::from(u16::from_be_bytes([b[0], b[1]])))
        };
        let [_lf, lh, bc, ec, nw, nh, nd, ni, nl, nk, ne, np] =
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11].map(|i| half(i).unwrap_or(0));
        let header = 4 * PREAMBLE_HALFWORDS / 2;
        let design_size = fix_word(bytes, header + 4 * DESIGN_SIZE_WORD)? >> FIX_TO_SCALED_SHIFT;
        let chars = if ec >= bc { ec - bc + 1 } else { 0 };
        let char_base = header + 4 * lh;
        let words = |start: usize, count: usize| -> Option<Vec<i64>> {
            (0..count).map(|k| fix_word(bytes, start + 4 * k)).collect()
        };
        let char_info = (0..chars)
            .map(|k| bytes.get(char_base + 4 * k..char_base + 4 * k + 4)?.try_into().ok())
            .collect::<Option<Vec<[u8; 4]>>>()?;
        let width_base = char_base + 4 * chars;
        let height_base = width_base + 4 * nw;
        let depth_base = height_base + 4 * nh;
        let italic_base = depth_base + 4 * nd;
        let lig_kern_base = italic_base + 4 * ni;
        let kern_base = lig_kern_base + 4 * nl;
        let param_base = italic_base + 4 * (ni + nl + nk + ne);
        let lig_kern = (0..nl)
            .map(|k| bytes.get(lig_kern_base + 4 * k..lig_kern_base + 4 * k + 4)?.try_into().ok())
            .collect::<Option<Vec<[u8; 4]>>>()?;
        Some(Metrics {
            design_size,
            first_char: bc,
            char_info,
            widths: words(width_base, nw)?,
            heights: words(height_base, nh)?,
            depths: words(depth_base, nd)?,
            italics: words(italic_base, ni)?,
            lig_kern,
            kerns: words(kern_base, nk)?,
            params: words(param_base, np)?,
        })
    }

    /// The smallest and largest character code (tex.web § 540, `bc` and
    /// `ec`).
    pub fn char_range(&self) -> (i64, i64) {
        let bc = self.first_char as i64;
        (bc, bc + self.char_info.len() as i64 - 1)
    }

    pub fn param_count(&self) -> usize {
        self.params.len()
    }

    /// Parameter `k` (from 1) at size `z`; the slant is not scaled
    /// (tex.web § 575).
    pub fn param(&self, k: usize, z: Scaled) -> Option<Scaled> {
        let fix = *self.params.get(k.checked_sub(1)?)?;
        Some(if k == SLANT { fix >> FIX_TO_SCALED_SHIFT } else { store_scaled(fix, z) })
    }

    fn info(&self, c: u32) -> Option<[u8; 4]> {
        let index = (c as usize).checked_sub(self.first_char)?;
        self.char_info.get(index).copied().filter(|info| info[0] != 0)
    }

    /// tex.web § 554: a character exists when its width index is not zero.
    pub fn has_char(&self, c: u32) -> bool {
        self.info(c).is_some()
    }

    /// Whether the font has a boundary character or a program for the left
    /// boundary (tex.web § 545: a first or last instruction with a
    /// `skip_byte` of 255).
    pub fn has_boundary(&self) -> bool {
        self.lig_kern.first().is_some_and(|i| i[0] == 255) || self.lig_kern.last().is_some_and(|i| i[0] == 255)
    }

    /// The instruction of `left`'s ligature/kerning program for `right`
    /// (tex.web §§ 557, 1039), its kern scaled to `z`.
    pub fn lig_kern(&self, left: u32, right: u32, z: Scaled) -> Option<LigKern> {
        let info = self.info(left)?;
        if info[2] & 3 != LIG_TAG {
            return None;
        }
        let mut k = usize::from(info[3]);
        let first = self.lig_kern.get(k)?;
        if first[0] > STOP_FLAG {
            k = 256 * usize::from(first[2]) + usize::from(first[3]);
        }
        loop {
            let [skip, next, op, rem] = *self.lig_kern.get(k)?;
            if u32::from(next) == right && skip <= STOP_FLAG {
                return Some(if op >= KERN_FLAG {
                    let kern = *self.kerns.get(256 * usize::from(op - KERN_FLAG) + usize::from(rem))?;
                    LigKern::Kern(store_scaled(kern, z))
                } else {
                    LigKern::Lig { op, char: u32::from(rem) }
                });
            }
            if skip >= STOP_FLAG {
                return None;
            }
            k += usize::from(skip) + 1;
        }
    }

    pub fn char_box(&self, c: u32, z: Scaled) -> Option<CharBox> {
        let info = self.info(c)?;
        let pick = |table: &Vec<i64>, index: u8| table.get(usize::from(index)).copied().unwrap_or(0);
        Some(CharBox {
            width: store_scaled(pick(&self.widths, info[0]), z),
            height: store_scaled(pick(&self.heights, info[1] >> 4), z),
            depth: store_scaled(pick(&self.depths, info[1] & 0x0f), z),
            italic: store_scaled(pick(&self.italics, info[2] >> 2), z),
        })
    }
}

/// tex.web §§ 571-572 (`store_scaled`): a fix_word times the size, computed
/// in the engine's own steps so that the rounding agrees.
pub fn store_scaled(fix: i64, z: Scaled) -> Scaled {
    let bytes = (fix as i32).to_be_bytes().map(i64::from);
    let (mut z, mut alpha) = (z, 16);
    while z >= Z_LIMIT {
        z /= 2;
        alpha += alpha;
    }
    let beta = 256 / alpha;
    let alpha = alpha * z;
    let sw = (((bytes[3] * z) / 256 + bytes[2] * z) / 256 + bytes[1] * z) / beta;
    match bytes[0] {
        0 => sw,
        _ => sw - alpha,
    }
}

/// Font basenames found under a search path list, cached by that list.
type SearchIndex = HashMap<Vec<PathBuf>, Rc<HashMap<String, PathBuf>>>;

thread_local! {
    static INDEX: RefCell<SearchIndex> = RefCell::new(HashMap::new());
    static LOADED: RefCell<HashMap<PathBuf, Option<Rc<Metrics>>>> = RefCell::new(HashMap::new());
    static INSTALLED: RefCell<Option<Option<HashSet<String>>>> = const { RefCell::new(None) };
}

/// The font directories of a TeX tree kpathsea searches for `\font`:
/// TFM files, METAFONT sources mktextfm builds them from, and the
/// OpenType and TrueType fonts XeTeX and LuaTeX load.
const FONT_DIRECTORIES: [&str; 4] = ["fonts/tfm", "fonts/source", "fonts/opentype", "fonts/truetype"];

/// `name.ext` beside the document or under the font directories of the
/// installation's trees, found the way kpathsea finds it: through each
/// tree's `ls-R`, or by walking the tree when it has none.  An empty
/// `ext` looks the name up as it is.
pub fn locate(base: &Path, roots: &[PathBuf], name: &str, ext: &str) -> Option<PathBuf> {
    let file =
        if ext.is_empty() || name.ends_with(&format!(".{ext}")) { name.to_string() } else { format!("{name}.{ext}") };
    let beside = base.join(&file);
    if beside.is_file() {
        return Some(beside);
    }
    let index =
        INDEX.with(|cache| cache.borrow_mut().entry(roots.to_vec()).or_insert_with(|| Rc::new(index(roots))).clone());
    index.get(&file).cloned()
}

fn index(roots: &[PathBuf]) -> HashMap<String, PathBuf> {
    let mut found = HashMap::new();
    let wanted = |relative: &str| FONT_DIRECTORIES.iter().any(|d| relative.starts_with(d));
    for root in roots.iter().rev() {
        if let Ok(text) = std::fs::read_to_string(root.join("ls-R")) {
            let mut directory: Option<PathBuf> = None;
            for line in text.lines() {
                if let Some(relative) = line.strip_suffix(':') {
                    let relative = relative.trim_start_matches("./");
                    directory = wanted(relative).then(|| root.join(relative));
                } else if let Some(directory) = &directory
                    && line.contains('.')
                {
                    found.insert(line.to_string(), directory.join(line));
                }
            }
        } else {
            for directory in FONT_DIRECTORIES {
                for entry in walkdir::WalkDir::new(root.join(directory)).into_iter().flatten() {
                    if let Some(file) = entry.file_name().to_str().filter(|_| entry.file_type().is_file()) {
                        found.insert(file.to_string(), entry.path().to_path_buf());
                    }
                }
            }
        }
    }
    found
}

/// Whether fontconfig, which XeTeX asks for a font by name, knows a family,
/// full name or PostScript name; `None` when `fc-list` cannot be run.
pub fn installed_font(name: &str) -> Option<bool> {
    INSTALLED.with(|cell| {
        let mut cell = cell.borrow_mut();
        let names = cell.get_or_insert_with(|| {
            let output = std::process::Command::new("fc-list")
                .args(["--format", "%{family}\t%{fullname}\t%{postscriptname}\n"])
                .output()
                .ok()
                .filter(|o| o.status.success())?;
            let text = String::from_utf8_lossy(&output.stdout);
            Some(text.split(['\n', '\t', ',']).map(|n| n.trim().to_lowercase()).filter(|n| !n.is_empty()).collect())
        });
        names.as_ref().map(|names| names.contains(&name.to_lowercase()))
    })
}

pub fn load(path: &Path) -> Option<Rc<Metrics>> {
    LOADED.with(|cache| {
        cache
            .borrow_mut()
            .entry(path.to_path_buf())
            .or_insert_with(|| std::fs::read(path).ok().and_then(|b| Metrics::parse(&b)).map(Rc::new))
            .clone()
    })
}
