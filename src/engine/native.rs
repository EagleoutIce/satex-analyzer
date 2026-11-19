//! Box sizes of text in a native font, the provider next to [`crate::engine::tfm`].
//! XeTeX (HarfBuzz) and LuaTeX (luaotfload) shape a word before they measure it: a
//! lookup may kern a pair, make a ligature or swap a glyph. A glyph is measured exactly
//! (its `hmtx` advance, its bounding box) where only lookups this module follows can
//! reach it: pair kerning and the ligature lookups of the features that are certainly on.
//! A glyph some other lookup may read is *loose*: its box is the font's hull, an
//! interval every glyph it may turn into fits. So a size is never one shaping cannot give.

use crate::engine::mode::Dim;
use crate::engine::otf::CharMap;
use crate::fixed::HashMap;
use std::cell::{OnceCell, RefCell};
use std::rc::Rc;
use ttf_parser::gpos::{PairAdjustment, PositioningSubtable as Pos, SingleAdjustment, ValueRecord};
use ttf_parser::gsub::SubstitutionSubtable as Sub;
use ttf_parser::opentype_layout::LayoutTable;
use ttf_parser::{GlyphId, Tag};

/// A pending token that is a glyph a ligature made, not a character: its id in
/// the low 16 bits, above them one more than the followed lookup that made it.
pub const GLYPH: u32 = 1 << 29;
/// A pending token shaping may change: it measures the font's hull.
pub const LOOSE: u32 = 1 << 30;

/// HarfBuzz's default features (`hb-ot-shape.cc`), on in XeTeX unless the name turns
/// them off. luaotfload turns on `liga` and `kern` for certain; the rest may be on.
const DEFAULTS: &[&[u8; 4]] = &[
    b"ccmp", b"locl", b"rlig", b"liga", b"clig", b"calt", b"kern", b"mark", b"mkmk", b"curs", b"dist", b"rclt",
    b"abvm", b"blwm", b"rvrn", b"ltra", b"ltrm",
];
const LUA_CERTAIN: &[&[u8; 4]] = &[b"liga", b"kern"];

/// What one step of a word gives: `left` and `c` make one glyph, or `left`
/// is set (its box the hull where `left_loose`), a kern, and `c` pends.
pub enum Step {
    Lig(u32),
    Next { left_loose: bool, kern: i64, next: u32 },
}

/// A pending glyph's size: its advance in font units where exact, else its width.
pub struct GlyphBox {
    pub advance: Option<i64>,
    pub width: Dim,
    pub height: Dim,
    pub depth: Dim,
}

#[derive(Clone, Copy, PartialEq)]
enum On {
    Certain,
    Maybe,
}

/// A ligature lookup's key: first glyph, characters consumed, next glyph.
type LigKey = (u16, usize, u16);

pub struct Shaping {
    map: Rc<CharMap>,
    /// `GPOS` pair lookups certainly applied, summed per pair.
    pairs: Vec<u16>,
    /// The `GSUB` ligature lookups certainly applied, in order.
    liga: Vec<u16>,
    /// Lookups (`GSUB` 0, `GPOS` 1) that may apply and are not followed: every glyph they read is loose.
    others: Vec<(u8, u16)>,
    /// Some lookup not followed may read any glyph (a contextual one).
    all_loose: bool,
    /// Characters a TECkit mapping (`mapping=`, luaotfload's `tlig`) may replace.
    mapped: Vec<u32>,
    /// `\XeTeXuseglyphmetrics` is on: heights and depths are the glyphs'.
    glyph_heights: bool,
    /// LuaTeX: each glyph and kern is a node of its own.
    lua: bool,
    /// The legacy `kern` table kerns where `GPOS` has no `kern` feature.
    legacy_kern: Option<bool>,
    touched: RefCell<HashMap<u16, bool>>,
    kerns: RefCell<HashMap<(u16, u16), Option<i64>>>,
    ligatures: RefCell<HashMap<LigKey, Option<Option<u32>>>>,
    /// Per glyph: a ligature that may apply takes it in after its first glyph
    /// (bit 0 the followed one's, bit 1 one not followed).
    absorbed: OnceCell<Vec<u8>>,
    bounds: OnceCell<Vec<(i16, i16)>>,
    hull: OnceCell<Option<[(i64, i64); 3]>>,
}

fn tag(s: &str) -> Option<Tag> {
    let b = s.as_bytes();
    (!b.is_empty() && b.len() <= 4).then(|| {
        let mut t = [b' '; 4];
        t[..b.len()].copy_from_slice(b);
        Tag::from_bytes(&t)
    })
}

/// The font's sizes in units at `size`: `units·size/upem`, as the interval of the
/// two scaled points around it (XeTeX rounds a float, luaotfload each glyph).
/// How an engine turns font units into scaled points.
#[derive(Clone, Copy)]
enum Rule {
    /// The two scaled points around the exact value.
    Interval,
    /// LuaTeX: the exact value rounded to the nearest scaled point (`lua_roundnumber`), a
    /// tie either way, as luaotfload's double-precision product may fall on either side.
    Nearest,
    /// XeTeX: single-precision points (`XeTeXFontInst::unitsToPoints`), then `D2Fix`:
    /// a half added, truncated toward zero.
    Single,
}

/// `units` at `size` in a font of `upem` units per em, as `rule` rounds it. XeTeX's
/// single-precision steps are taken in both orders the product may be formed in.
fn rounded(units: i64, size: i64, upem: i64, rule: Rule) -> Dim {
    match rule {
        Rule::Interval => scaled((units, units), size, upem),
        Rule::Nearest => {
            let (twice, den) = (2 * units * size + upem, 2 * upem);
            let k = twice.div_euclid(den);
            Dim::range(if twice.rem_euclid(den) == 0 { k - 1 } else { k }, k)
        }
        Rule::Single => {
            let (u, pt, em) = (units as f32, size as f32 / 65536.0, upem as f32);
            let fix = |p: f32| (f64::from(p) * 65536.0 + 0.5) as i64;
            let (a, b) = (fix(u * pt / em), fix(u * (pt / em)));
            Dim::range(a.min(b), a.max(b))
        }
    }
}

fn scaled(units: (i64, i64), size: i64, upem: i64) -> Dim {
    let lo = (units.0 * size).div_euclid(upem);
    let hi = -(-units.1 * size).div_euclid(upem);
    Dim::range(lo, hi)
}

/// Text the default shaper sets glyph by glyph: Latin, digits, punctuation
/// (no marks, ignorables or scripts with their own shapers).
fn plain(c: u32) -> bool {
    matches!(c, 0x21..=0x7e | 0xa1..=0xac | 0xae..=0x2af | 0x2010..=0x2027 | 0x2030..=0x205e)
}

impl Shaping {
    /// `features` is the part of the font name after `:`; `None` for a key whose effect on
    /// sizes is not followed (`extend`, `embolden`, a mapping not found, a variation).
    pub fn new(
        map: Rc<CharMap>,
        features: &str,
        lua: bool,
        glyph_heights: bool,
        mapping: impl Fn(&str) -> Option<Vec<u32>>,
    ) -> Option<Shaping> {
        let face = map.face()?;
        // A variable font's instance and AAT shaping (`morx`, `kerx`) are not followed.
        let raw = face.raw_face();
        if [b"fvar", b"morx", b"kerx", b"trak"].iter().any(|t| raw.table(Tag::from_bytes(t)).is_some()) {
            return None;
        }
        let mut on: HashMap<Tag, On> = HashMap::default();
        for t in DEFAULTS {
            let certain = !lua || LUA_CERTAIN.contains(t);
            on.insert(Tag::from_bytes(t), if certain { On::Certain } else { On::Maybe });
        }
        let (mut script, mut language, mut mapped) = (None, None, Vec::new());
        for item in features.split([';', ',']).map(str::trim).filter(|s| !s.is_empty()) {
            let (key, value) = item.split_once('=').map_or((item, None), |(k, v)| (k.trim(), Some(v.trim())));
            match (key, value) {
                ("mapping", Some(m)) => mapped.extend(mapping(m)?),
                ("script", Some(s)) => script = Some(tag(s)?),
                ("language", Some(l)) => language = Some(tag(l)?),
                ("mode", Some("node" | "base" | "harf")) | ("color", _) | ("shaper", Some("ot")) => {}
                // luaotfload's TeX ligatures and replacements, as `tex-text` maps them.
                ("+tlig" | "tlig" | "+trep" | "trep", None | Some("yes" | "true")) if lua => {
                    mapped.extend(mapping("tex-text")?)
                }
                (k, v) => {
                    let (name, state) = match (k.as_bytes().first(), v) {
                        (Some(b'+'), None) => (&k[1..], On::Certain),
                        (Some(b'-'), None) => (&k[1..], On::Certain),
                        (_, None | Some("yes" | "true")) => (k, On::Certain),
                        (_, Some("no" | "false")) => (k, On::Certain),
                        // An alternate's index, a value: which glyph it picks is not followed.
                        (_, Some(_)) => (k.trim_start_matches('+'), On::Maybe),
                    };
                    let t = tag(name)?;
                    if k.starts_with('-') || matches!(v, Some("no" | "false")) {
                        on.remove(&t);
                    } else {
                        on.insert(t, state);
                    }
                }
            }
        }
        mapped.sort_unstable();
        let mut s = Shaping {
            pairs: Vec::new(),
            liga: Vec::new(),
            others: Vec::new(),
            all_loose: false,
            mapped,
            glyph_heights,
            lua,
            legacy_kern: None,
            touched: RefCell::default(),
            kerns: RefCell::default(),
            ligatures: RefCell::default(),
            absorbed: OnceCell::new(),
            bounds: OnceCell::new(),
            hull: OnceCell::new(),
            map: map.clone(),
        };
        // HarfBuzz reads the legacy `kern` table only where `GPOS` has no `kern` feature.
        let gpos_kern =
            face.tables().gpos.is_some_and(|t| t.features.into_iter().any(|f| f.tag == Tag::from_bytes(b"kern")));
        for (k, table) in [face.tables().gsub, face.tables().gpos].into_iter().enumerate() {
            let Some(table) = table else { continue };
            if table.variations.is_some() {
                s.all_loose = true;
            }
            for (index, state) in lookups(&table, &on, script, language) {
                let Some(lookup) = table.lookups.get(index) else { continue };
                let plain_flags = !lookup.flags.ignore_base_glyphs() && !lookup.flags.ignore_ligatures();
                let kind = |i| {
                    if k == 0 {
                        lookup.subtables.get::<Sub>(i).map(sub_kind)
                    } else {
                        lookup.subtables.get::<Pos>(i).map(pos_kind)
                    }
                };
                let kinds: Vec<u8> = (0..lookup.subtables.len()).filter_map(kind).collect();
                let all = |want: u8| !kinds.is_empty() && kinds.iter().all(|&x| x == want);
                if kinds.contains(&CONTEXT) {
                    s.all_loose = true;
                } else if state == On::Certain && plain_flags && k == 1 && all(PAIR) {
                    s.pairs.push(index);
                } else if state == On::Certain && plain_flags && k == 0 && all(LIGA) {
                    s.liga.push(index);
                } else {
                    s.others.push((k as u8, index));
                }
            }
        }
        if s.chained() {
            let liga = std::mem::take(&mut s.liga);
            s.others.extend(liga.into_iter().map(|i| (0, i)));
        }
        if !gpos_kern && on.contains_key(&Tag::from_bytes(b"kern")) {
            let simple = face.tables().kern.is_some_and(|k| {
                k.subtables
                    .into_iter()
                    .all(|t| t.horizontal && !t.variable && !t.has_cross_stream && !t.has_state_machine)
            });
            s.legacy_kern = face.tables().kern.map(|_| simple && !lua);
            if !simple && face.tables().kern.is_some() {
                s.all_loose = true;
            }
        }
        Some(s)
    }

    /// Whether a followed ligature lookup makes a glyph a later one takes in after
    /// its first: the glyph before it was set by then.
    fn chained(&self) -> bool {
        let Some(gsub) = self.map.face().and_then(|f| f.tables().gsub) else { return false };
        let n = self.map.num_glyphs as usize;
        let mut made = vec![false; n];
        for &index in &self.liga {
            let Some(lookup) = gsub.lookups.get(index) else { return true };
            for sub in lookup.subtables.into_iter::<Sub>() {
                let Sub::Ligature(l) = sub else { return true };
                let read = |g: GlyphId| made.get(usize::from(g.0)).copied().unwrap_or(true);
                for set in l.ligature_sets {
                    if set.into_iter().any(|lig| lig.components.into_iter().any(read)) {
                        return true;
                    }
                }
            }
            for sub in lookup.subtables.into_iter::<Sub>() {
                if let Sub::Ligature(l) = sub {
                    for lig in l.ligature_sets.into_iter().flatten() {
                        if let Some(m) = made.get_mut(usize::from(lig.glyph.0)) {
                            *m = true;
                        }
                    }
                }
            }
        }
        false
    }

    fn glyph(&self, t: u32) -> Option<u16> {
        if t & GLYPH != 0 {
            return Some(t as u16);
        }
        self.map.glyphs.get(&(t & !LOOSE)).and_then(|&g| u16::try_from(g).ok())
    }

    /// Whether shaping may change token `t`'s glyph or its advance in ways not followed.
    fn loose(&self, t: u32) -> bool {
        if t & LOOSE != 0 || self.all_loose {
            return true;
        }
        let c = t & !GLYPH;
        if t & GLYPH == 0 && (!plain(c) || self.mapped.binary_search(&c).is_ok()) {
            return true;
        }
        let Some(g) = self.glyph(t) else { return true };
        if let Some(&v) = self.touched.borrow().get(&g) {
            return v;
        }
        let v = self.reads(g);
        self.touched.borrow_mut().insert(g, v);
        v
    }

    /// A lookup not followed reads glyph `g`, or it is a mark (HarfBuzz zeroes and moves marks).
    fn reads(&self, g: u16) -> bool {
        let Some(face) = self.map.face() else { return true };
        let gid = GlyphId(g);
        if face.tables().gdef.and_then(|d| d.glyph_class(gid)) == Some(ttf_parser::gdef::GlyphClass::Mark) {
            return true;
        }
        let tables = [face.tables().gsub, face.tables().gpos];
        self.others.iter().any(|&(k, i)| {
            let Some(lookup) = tables[usize::from(k)].and_then(|t| t.lookups.get(i)) else { return true };
            (0..lookup.subtables.len()).any(|j| {
                if k == 0 {
                    lookup.subtables.get::<Sub>(j).is_none_or(|s| s.coverage().contains(gid))
                } else {
                    // A mark lookup moves marks only, which are loose.
                    lookup.subtables.get::<Pos>(j).is_none_or(|s| match s {
                        Pos::MarkToBase(_) | Pos::MarkToLigature(_) | Pos::MarkToMark(_) => false,
                        Pos::Pair(p) => p.coverage().contains(gid) || pair_second(&p, gid),
                        s => s.coverage().contains(gid),
                    })
                }
            })
        }) || self.absorbed().get(usize::from(g)).is_none_or(|&b| b & 2 != 0)
    }

    /// Glyphs a ligature (followed or not) may take in after its first.
    fn absorbed(&self) -> &Vec<u8> {
        self.absorbed.get_or_init(|| {
            let mut out = vec![0; self.map.num_glyphs as usize];
            let Some(gsub) = self.map.face().and_then(|f| f.tables().gsub) else { return out };
            let others = self.others.iter().filter(|&&(k, _)| k == 0).map(|&(_, i)| (i, 2));
            for (i, bit) in self.liga.iter().map(|&i| (i, 1)).chain(others) {
                let Some(lookup) = gsub.lookups.get(i) else { continue };
                for sub in lookup.subtables.into_iter::<Sub>() {
                    if let Sub::Ligature(l) = sub {
                        for set in l.ligature_sets {
                            for lig in set {
                                for c in lig.components {
                                    if let Some(x) = out.get_mut(usize::from(c.0)) {
                                        *x |= bit;
                                    }
                                }
                            }
                        }
                    }
                }
            }
            out
        })
    }

    /// One more character `c` after the pending token `left` of the same word.
    pub fn step(&self, left: u32, c: u32) -> Step {
        let (ll, lc) = (self.loose(left), self.loose(c));
        // A loose glyph may become one that ligates or kerns with its neighbour, or takes it in.
        let absorbed =
            |c: u32| self.glyph(c).is_none_or(|g| self.absorbed().get(usize::from(g)).is_none_or(|&b| b != 0));
        if ll || lc {
            let next = if lc || absorbed(c) { c | LOOSE } else { c };
            return Step::Next { left_loose: true, kern: 0, next };
        }
        let (Some(gl), Some(gc)) = (self.glyph(left), self.glyph(c)) else { unreachable!("loose checks the map") };
        // A ligature lookup goes on after the glyph it made (OpenType `GSUB` type 4): only later ones read it.
        {
            let from = if left & GLYPH == 0 { 0 } else { (left >> 16 & 0x1fff) as usize };
            let lig = *self.ligatures.borrow_mut().entry((gl, from, gc)).or_insert_with(|| self.ligature(gl, from, gc));
            match lig {
                Some(Some(lig)) if !self.loose(lig) => return Step::Lig(lig),
                Some(_) => return Step::Next { left_loose: true, kern: 0, next: c | LOOSE },
                None => {}
            }
        }
        match self.kern(gl, gc) {
            Some(k) => Step::Next { left_loose: false, kern: k, next: c },
            None => Step::Next { left_loose: true, kern: 0, next: c },
        }
    }

    /// The followed lookups' ligature of `a` and `b`, which GSUB applies in order, each
    /// left to right: `None` where none takes them, `Some(None)` where one may take more
    /// glyphs not yet known, or an earlier one may take `b` with what follows.
    fn ligature(&self, a: u16, from: usize, b: u16) -> Option<Option<u32>> {
        let face = self.map.face()?;
        let gsub = face.tables().gsub?;
        let mut earlier_takes_b = false;
        for (k, &index) in self.liga.iter().enumerate() {
            let lookup = gsub.lookups.get(index)?;
            let subs = || {
                lookup
                    .subtables
                    .into_iter::<Sub>()
                    .filter_map(|s| if let Sub::Ligature(l) = s { Some(l) } else { None })
            };
            // Each subtable covering `a` is tried in turn; in it, the first entry that matches applies.
            for l in subs().filter(|_| k >= from) {
                let Some(i) = l.coverage.get(GlyphId(a)) else { continue };
                for lig in l.ligature_sets.get(i)? {
                    match lig.components.get(0) {
                        Some(x) if x.0 == b && lig.components.len() == 1 && !earlier_takes_b => {
                            return Some(Some(GLYPH | (k as u32 + 1) << 16 | u32::from(lig.glyph.0)));
                        }
                        Some(x) if x.0 == b => return Some(None),
                        None => return Some(None),
                        _ => {}
                    }
                }
            }
            earlier_takes_b |= subs().any(|l| l.coverage.contains(GlyphId(b)));
        }
        None
    }

    /// The kern between `a` and `b` in font units, `None` where not exact.
    fn kern_units(&self, a: u16, b: u16) -> Option<i64> {
        let face = self.map.face()?;
        let (ga, gb) = (GlyphId(a), GlyphId(b));
        let mut sum = 0;
        if let Some(gpos) = face.tables().gpos {
            for &i in &self.pairs {
                let lookup = gpos.lookups.get(i)?;
                let mut found: Option<i64> = None;
                for sub in lookup.subtables.into_iter::<Pos>() {
                    let Pos::Pair(p) = sub else { continue };
                    let Some(v) = pair_value(&p, ga, gb) else { continue };
                    match (found, v?) {
                        (None, v) => found = Some(v),
                        // A zero a class matrix gives may hide a later subtable's value.
                        (Some(0), v) if v != 0 => return None,
                        _ => {}
                    }
                    if found != Some(0) {
                        break;
                    }
                }
                sum += found.unwrap_or(0);
            }
        }
        match self.legacy_kern {
            Some(true) => {
                let subs: Vec<_> = face.tables().kern?.subtables.into_iter().collect();
                match subs.as_slice() {
                    [] => {}
                    [t] => sum += i64::from(t.glyphs_kerning(ga, gb).unwrap_or(0)),
                    _ => return None,
                }
            }
            Some(false) => {
                face.tables().kern?.subtables.into_iter().all(|t| t.glyphs_kerning(ga, gb).is_none()).then_some(())?
            }
            None => {}
        }
        Some(sum)
    }

    fn kern(&self, a: u16, b: u16) -> Option<i64> {
        *self.kerns.borrow_mut().entry((a, b)).or_insert_with(|| self.kern_units(a, b))
    }

    /// The box of pending token `t` at `size`: exact for a plain glyph, the hull for a loose one.
    pub fn glyph_box(&self, t: u32, size: i64) -> GlyphBox {
        let upem = i64::from(self.map.units_per_em.max(1));
        let Some(hull) = self.hull() else {
            return GlyphBox { advance: None, width: Dim::ANY, height: Dim::ANY, depth: Dim::ANY };
        };
        let any = |(lo, hi): (i64, i64)| {
            if hi == i64::MAX {
                Dim::range(scaled((lo, 0), size, upem).lo, Dim::ANY.hi)
            } else {
                scaled((lo, hi), size, upem)
            }
        };
        let (Some(g), false) = (self.glyph(t), self.loose(t)) else {
            // HarfBuzz may stack a mark (by Unicode class) over what comes before.
            let marked = t & GLYPH == 0 && !plain(t & !LOOSE);
            let up = |b: (i64, i64)| any(if marked { (b.0, i64::MAX) } else { b });
            return GlyphBox { advance: None, width: any(hull[0]), height: up(hull[1]), depth: up(hull[2]) };
        };
        let adv = i64::from(self.map.advances.get(usize::from(g)).or(self.map.advances.last()).copied().unwrap_or(0));
        let (lo, hi) = self.bounds().get(usize::from(g)).copied().unwrap_or_default();
        let width = self.item(adv, size);
        if !self.glyph_heights {
            return GlyphBox { advance: Some(adv), width, height: any(hull[1]), depth: any(hull[2]) };
        }
        // The depth is the bottom's negative, rounded as the bottom (`D2Fix` of a negative point value).
        let bottom = self.extent(lo.into(), size);
        let (height, depth) = (self.extent(hi.into(), size), Dim::range(-bottom.hi, -bottom.lo));
        GlyphBox { advance: Some(adv), width, height, depth }
    }

    /// luaotfload's `\fontdimen k` at `size` (`fontloader-font-otl.lua`): an interval over
    /// the choices it makes from metadata satex does not read (`syncspace`, `monospaced`,
    /// where the x-height comes from). `None` where not followed.
    pub fn lua_param(&self, k: usize, size: i64) -> Option<Dim> {
        let em = i64::from(self.map.units_per_em.max(1));
        let space = self.map.glyphs.get(&32).and_then(|&g| self.map.advances.get(g as usize)).map(|&a| i64::from(a));
        let x =
            self.map.glyphs.get(&u32::from('x')).and_then(|&g| self.bounds().get(g as usize)).map(|b| i64::from(b.1));
        // Twice the units, so that halves and thirds stay exact.
        let candidates: Vec<(i64, i64)> = match k {
            1 if self.map.italic_angle == 0.0 => return Some(Dim::exact(0)),
            // `-tan(italicangle)` per point (`fontloader-font-con.lua`), a scaled point either way.
            1 => {
                let x = -self.map.italic_angle.to_radians().tan() * 65536.0;
                return Some(Dim::range(x.floor() as i64 - 1, x.ceil() as i64 + 1));
            }
            2 => vec![(space?, 1)],
            3 => vec![(em, 2), (space?, 2), (0, 1)],
            4 | 7 => vec![(em, 3), (space?, 3), (0, 1)],
            5 => [self.map.x_height.map(i64::from), x]
                .into_iter()
                .flatten()
                .map(|u| (u, 1))
                .chain([(2 * em, 5)])
                .collect(),
            6 => return Some(Dim::exact(size)),
            _ => return None,
        };
        candidates.into_iter().map(|(u, d)| rounded(u * size, d * em, 1, Rule::Nearest)).reduce(Dim::join)
    }

    /// Whether the engine rounds each glyph and kern (LuaTeX's nodes) rather than
    /// a word's width once (XeTeX's native word node).
    pub fn per_glyph(&self) -> bool {
        self.lua
    }

    /// One glyph's or kern's `units` at `size`, as the engine rounds it.
    pub fn item(&self, units: i64, size: i64) -> Dim {
        let upem = i64::from(self.map.units_per_em.max(1));
        rounded(units, size, upem, if self.lua { Rule::Nearest } else { Rule::Interval })
    }

    /// A glyph's height or depth.
    fn extent(&self, units: i64, size: i64) -> Dim {
        let upem = i64::from(self.map.units_per_em.max(1));
        rounded(units, size, upem, if self.lua { Rule::Nearest } else { Rule::Single })
    }

    /// XeTeX's width of a word whose exact advances and kerns sum to `units`, at `size`,
    /// widened where loose glyphs (`loose`) were added on their own.
    pub fn word(&self, units: i64, size: i64, loose: bool) -> Dim {
        let upem = i64::from(self.map.units_per_em.max(1));
        let w = rounded(units, size, upem, Rule::Single);
        if loose { Dim::range(w.lo - 1, w.hi + 1) } else { w }
    }

    /// Each glyph's bounding box bottom and top (`glyf` or `CFF`), zero for an empty one.
    fn bounds(&self) -> &Vec<(i16, i16)> {
        self.bounds.get_or_init(|| {
            let Some(face) = self.map.face() else { return Vec::new() };
            (0..face.number_of_glyphs())
                .map(|g| face.glyph_bounding_box(GlyphId(g)).map_or((0, 0), |r| (r.y_min, r.y_max)))
                .collect()
        })
    }

    /// Width, height and depth in font units every glyph a loose token may turn into fits:
    /// advances, then each `GPOS` lookup's largest adjustments, times the most glyphs a
    /// multiple substitution makes; `i64::MAX` where a mark may stack on it.
    fn hull(&self) -> Option<[(i64, i64); 3]> {
        *self.hull.get_or_init(|| self.compute_hull())
    }

    fn compute_hull(&self) -> Option<[(i64, i64); 3]> {
        let face = self.map.face()?;
        let n = face.number_of_glyphs();
        let adv = self.map.advances.iter().map(|&a| i64::from(a));
        let (amin, amax) = (adv.clone().min().unwrap_or(0), adv.max().unwrap_or(0));
        let (mut kx, mut ky, mut grow, mut marks) = ((0, 0), (0, 0), 1i64, false);
        let add = |bound: &mut (i64, i64), lo: i64, hi: i64| {
            bound.0 += lo.min(0);
            bound.1 += hi.max(0);
        };
        if let Some(gsub) = face.tables().gsub {
            for lookup in gsub.lookups {
                let mut most = 1;
                for sub in lookup.subtables.into_iter::<Sub>() {
                    if let Sub::Multiple(m) = sub {
                        most = m.sequences.into_iter().map(|s| i64::from(s.substitutes.len())).fold(most, i64::max);
                    }
                }
                grow = grow.checked_mul(most).filter(|&g| g <= 16)?;
            }
        }
        if let Some(gpos) = face.tables().gpos {
            for lookup in gpos.lookups {
                let (mut x, mut y) = ((0i64, 0i64), (0i64, 0i64));
                let mut see = |v: &ValueRecord| {
                    let a = i64::from(v.x_advance);
                    let p = i64::from(v.y_placement);
                    (x.0, x.1, y.0, y.1) = (x.0.min(a), x.1.max(a), y.0.min(p), y.1.max(p));
                };
                for sub in lookup.subtables.into_iter::<Pos>() {
                    match sub {
                        Pos::Single(SingleAdjustment::Format1 { value, .. }) => see(&value),
                        Pos::Single(SingleAdjustment::Format2 { values, .. }) => {
                            (0..values.len()).filter_map(|i| values.get(i)).for_each(|v| see(&v))
                        }
                        Pos::Pair(PairAdjustment::Format1 { coverage, sets }) => {
                            for a in (0..n).filter(|&a| coverage.contains(GlyphId(a))) {
                                let set = coverage.get(GlyphId(a)).and_then(|i| sets.get(i))?;
                                for (v1, v2) in (0..n).filter_map(|b| set.get(GlyphId(b))) {
                                    see(&v1);
                                    see(&v2);
                                }
                            }
                        }
                        Pos::Pair(PairAdjustment::Format2 { matrix, .. }) => {
                            for c1 in (0..).take_while(|&c| matrix.get((c, 0)).is_some()) {
                                for (v1, v2) in (0..).map_while(|c2| matrix.get((c1, c2))) {
                                    see(&v1);
                                    see(&v2);
                                }
                            }
                        }
                        Pos::MarkToBase(_) | Pos::MarkToLigature(_) | Pos::MarkToMark(_) => marks = true,
                        // Cursive attachment and contextual lookups move glyphs by more than this follows.
                        _ => return None,
                    }
                }
                add(&mut kx, x.0, x.1);
                add(&mut ky, y.0, y.1);
            }
        }
        if self.legacy_kern.is_some() && n <= 2048 {
            for t in face.tables().kern?.subtables {
                let (mut lo, mut hi) = (0i64, 0i64);
                for a in 0..n {
                    for b in 0..n {
                        let k = t.glyphs_kerning(GlyphId(a), GlyphId(b)).map_or(0, i64::from);
                        (lo, hi) = (lo.min(k), hi.max(k));
                    }
                }
                add(&mut kx, lo, hi);
            }
        } else if self.legacy_kern.is_some() {
            return None;
        }
        let b = self.bounds();
        let tops = b.iter().map(|&(_, t)| i64::from(t));
        let bottoms = b.iter().map(|&(l, _)| -i64::from(l));
        let mut top = (tops.clone().min().unwrap_or(0).min(0), tops.max().unwrap_or(0));
        let mut bottom = (bottoms.clone().min().unwrap_or(0).min(0), bottoms.max().unwrap_or(0));
        if !self.glyph_heights {
            // XeTeX's heights are then the font's ascent and descent, from one of these.
            let os2 = face.tables().os2;
            let asc = [Some(face.ascender()), os2.map(|o| o.typographic_ascender()), os2.map(|o| o.windows_ascender())];
            let desc =
                [Some(face.descender()), os2.map(|o| o.typographic_descender()), os2.map(|o| -o.windows_descender())];
            for a in asc.into_iter().flatten().map(i64::from) {
                top = (top.0.min(a), top.1.max(a));
            }
            for d in desc.into_iter().flatten().map(|d| -i64::from(d)) {
                bottom = (bottom.0.min(d), bottom.1.max(d));
            }
        }
        let width = (grow * (amin + kx.0).min(0), grow * (amax + kx.1));
        let height = (top.0 + ky.0, if marks { i64::MAX } else { top.1 + ky.1 });
        let depth = (bottom.0 - ky.1, if marks { i64::MAX } else { bottom.1 - ky.0 });
        Some([width, height, depth])
    }
}

const PAIR: u8 = 2;
const LIGA: u8 = 4;
const CONTEXT: u8 = 7;

fn sub_kind(s: Sub) -> u8 {
    match s {
        Sub::Ligature(_) => LIGA,
        Sub::Context(_) | Sub::ChainContext(_) | Sub::ReverseChainSingle(_) => CONTEXT,
        _ => 1,
    }
}

fn pos_kind(s: Pos) -> u8 {
    match s {
        Pos::Pair(_) => PAIR,
        Pos::Context(_) | Pos::ChainContext(_) => CONTEXT,
        _ => 1,
    }
}

/// A pair subtable's kern for `a`, `b`: `None` where it does not cover them,
/// `Some(None)` where it moves them in a way other than the first's advance.
fn pair_value(p: &PairAdjustment, a: GlyphId, b: GlyphId) -> Option<Option<i64>> {
    let (v1, v2) = match p {
        PairAdjustment::Format1 { coverage, sets } => sets.get(coverage.get(a)?)?.get(b)?,
        PairAdjustment::Format2 { coverage, classes, matrix } => {
            coverage.get(a)?;
            matrix.get((classes.0.get(a), classes.1.get(b)))?
        }
    };
    let plain = |v: &ValueRecord| v.y_placement == 0 && v.x_advance_device.is_none() && v.y_placement_device.is_none();
    let second = v2.x_advance == 0 && v2.x_placement == 0 && v2.y_placement == 0 && plain(&v2);
    Some((plain(&v1) && second).then_some(i64::from(v1.x_advance)))
}

/// Whether a pair subtable not followed may move `b` as the second glyph.
fn pair_second(p: &PairAdjustment, b: GlyphId) -> bool {
    match p {
        PairAdjustment::Format2 { classes, .. } => classes.1.get(b) != 0,
        // A pair set lists its second glyphs only by search: any glyph may be one.
        PairAdjustment::Format1 { .. } => true,
    }
}

/// The lookups `on` reaches in table `table` under every script and language system
/// a word may be shaped with (`script`, else `latn` and the default script), with
/// whether each is certainly applied.
fn lookups(table: &LayoutTable, on: &HashMap<Tag, On>, script: Option<Tag>, language: Option<Tag>) -> Vec<(u16, On)> {
    let scripts = match script {
        Some(s) => vec![s],
        None => vec![Tag::from_bytes(b"latn"), Tag::from_bytes(b"DFLT"), Tag::from_bytes(b"dflt")],
    };
    let default_language = |l: Tag| l == Tag::from_bytes(b"dflt") || l == Tag::from_bytes(b"DFLT");
    let mut systems = Vec::new();
    for s in scripts.into_iter().filter_map(|t| table.scripts.find(t)) {
        let sys = match language.filter(|&l| !default_language(l)) {
            Some(l) => s.languages.find(l).or(s.default_language),
            None => s.default_language,
        };
        systems.extend(sys);
    }
    let mut per: Vec<HashMap<u16, On>> = Vec::new();
    for sys in &systems {
        let mut found: HashMap<u16, On> = HashMap::default();
        let required = sys.required_feature.map(|i| (i, On::Certain));
        let features = sys.feature_indices.into_iter().filter_map(|i| Some((i, *on.get(&table.features.get(i)?.tag)?)));
        for (i, state) in required.into_iter().chain(features) {
            let Some(f) = table.features.get(i) else { continue };
            for l in f.lookup_indices {
                let e = found.entry(l).or_insert(state);
                *e = if *e == On::Certain || state == On::Certain { On::Certain } else { On::Maybe };
            }
        }
        per.push(found);
    }
    let mut out: HashMap<u16, On> = HashMap::default();
    for found in &per {
        for &l in found.keys() {
            let everywhere = per.iter().all(|f| f.get(&l) == Some(&On::Certain));
            out.insert(l, if everywhere { On::Certain } else { On::Maybe });
        }
    }
    let mut out: Vec<_> = out.into_iter().collect();
    // `GSUB` and `GPOS` apply their lookups in list order.
    out.sort_unstable_by_key(|e| e.0);
    out
}

/// The characters a TECkit mapping source (`⟨name⟩.map`) may replace: those left of `>`.
pub fn mapping_inputs(source: &str) -> Option<Vec<u32>> {
    let mut out = Vec::new();
    for line in source.lines().map(|l| l.split(';').next().unwrap_or("")) {
        let Some((lhs, _)) = line.split_once('>') else { continue };
        for word in lhs.split_whitespace() {
            if let Some(hex) = word.strip_prefix("U+") {
                out.push(u32::from_str_radix(hex, 16).ok()?);
            } else if word != "<" {
                // Classes, contexts or byte passes: what they reach is not followed.
                return None;
            }
        }
    }
    Some(out)
}
