//! What XeTeX reads of an OpenType or TrueType font, parsed by
//! `ttf-parser`: its character map for `\iffontchar` and
//! `\XeTeXcharglyph`, its layout scripts, languages and features, its
//! glyph names, and what its eight `\fontdimen`s are made of.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub struct CharMap {
    /// Character code to glyph id; glyph 0 (`.notdef`) is left out.
    pub glyphs: BTreeMap<u32, u32>,
    pub num_glyphs: u32,
    pub units_per_em: u32,
    /// `post` italic angle, in degrees.
    pub italic_angle: f64,
    /// Each glyph's advance width, in font units (`hmtx`).
    pub advances: Vec<u32>,
    /// `OS/2` version 2 heights, in font units.
    pub x_height: Option<u32>,
    pub cap_height: Option<u32>,
    /// Whether it has Graphite tables, which XeTeX may shape it with.
    pub graphite: bool,
    /// The `MATH` table's constants in HarfBuzz's `hb_ot_math_constant_t`
    /// order: the first two and the last are percentages, the rest font
    /// units.
    pub math: Option<Vec<i32>>,
    /// The script lists of `GSUB` and `GPOS`, in order.
    pub layout: [Vec<Script>; 2],
    /// The font file, for glyph names (`post`, else the `CFF` charset).
    data: Vec<u8>,
}

/// A layout table's script: its default language system's features and
/// its tagged ones (OpenType "ScriptList"; required features left out, as
/// HarfBuzz's `hb_ot_layout_language_get_feature_tags` does).
pub struct Script {
    pub tag: u32,
    pub default: Option<Vec<u32>>,
    pub languages: Vec<(u32, Vec<u32>)>,
}

/// XeTeX's `XeTeX_ext.c` answers, reproduced against `xetex -ini`: the
/// scripts are `GSUB`'s; a script's languages are `GSUB`'s then `GPOS`'s at
/// the same script index; its features are those `GSUB` then `GPOS` give
/// the language, found by tag in each table, language 0 being the default.
impl CharMap {
    /// `\XeTeXOTcountscripts`, `\XeTeXOTscripttag`.
    pub fn scripts(&self) -> impl Iterator<Item = u32> + '_ {
        self.layout[0].iter().map(|s| s.tag)
    }

    fn languages(&self, script: u32) -> impl Iterator<Item = u32> + '_ {
        let at = self.layout[0].iter().position(|s| s.tag == script);
        self.layout
            .iter()
            .flat_map(move |table| at.and_then(|i| table.get(i)))
            .flat_map(|s| s.languages.iter().map(|l| l.0))
    }

    /// `\XeTeXOTcountlanguages`.
    pub fn count_languages(&self, script: u32) -> usize {
        self.languages(script).count()
    }

    /// `\XeTeXOTlanguagetag`: past `GSUB`'s languages, `GPOS`'s at the same
    /// index (not continuing the count).
    pub fn language_tag(&self, script: u32, n: usize) -> u32 {
        let Some(i) = self.layout[0].iter().position(|s| s.tag == script) else { return 0 };
        self.layout.iter().find_map(|t| t.get(i).and_then(|s| s.languages.get(n))).map_or(0, |l| l.0)
    }

    /// `\XeTeXOTcountfeatures`, `\XeTeXOTfeaturetag`.
    pub fn features(&self, script: u32, language: u32) -> Vec<u32> {
        let mut out = Vec::new();
        for table in &self.layout {
            let Some(s) = table.iter().find(|s| s.tag == script) else { continue };
            match s.languages.iter().find(|l| l.0 == language) {
                Some(l) => out.extend(&l.1),
                None if language == 0 => out.extend(s.default.iter().flatten()),
                None => {}
            }
        }
        out
    }

    /// `\XeTeXglyphname`: FreeType's `FT_Get_Glyph_Name`, empty if none.
    pub fn glyph_name(&self, glyph: i64) -> String {
        let face = ttf_parser::Face::parse(&self.data, 0).ok();
        let id = u16::try_from(glyph).ok().map(ttf_parser::GlyphId);
        face.as_ref().zip(id).and_then(|(f, g)| f.glyph_name(g)).unwrap_or_default().to_string()
    }

    /// `\XeTeXglyphindex`: FreeType's `FT_Get_Name_Index`, 0 if none.
    pub fn glyph_index(&self, name: &str) -> u32 {
        let face = ttf_parser::Face::parse(&self.data, 0).ok();
        face.and_then(|f| f.glyph_index_by_name(name)).map_or(0, |g| g.0.into())
    }

    /// A character's advance in font units: `.notdef`'s for a character
    /// the font lacks, as XeTeX measures it.
    pub fn advance(&self, c: u32) -> Option<u32> {
        let gid = self.glyphs.get(&c).map_or(0, |&g| g as usize);
        self.advances.get(gid).or(self.advances.last()).copied()
    }

    /// Font units at `size`, the way XeTeX turns them into a dimension.
    pub fn points(&self, units: u32, size: i64) -> i64 {
        let p = units as f32 * (size as f32 / 65536.0) / self.units_per_em as f32;
        (f64::from(p) * 65536.0 + 0.5) as i64
    }

    /// xetex.web `load_native_font`: slant, space, stretch (space/2),
    /// shrink (space/3), x-height, quad (the size), extra space (space/3),
    /// cap height; XeTeX scales font units in single precision
    /// (`XeTeXFontInst::unitsToPoints`) and rounds with `D2Fix`.
    pub fn param(&self, k: usize, size: i64) -> Option<i64> {
        let points = |units: u32| self.points(units, size);
        let space = || self.advance(32).map(points);
        Some(match k {
            1 => (-(self.italic_angle.to_radians().tan()) * 65536.0 + 0.5) as i64,
            2 => space()?,
            3 => space()? / 2,
            4 | 7 => space()? / 3,
            5 => points(self.x_height?),
            6 => size,
            8 => points(self.cap_height?),
            // xetex.web `load_native_font`: a math font's ninth parameter
            // is their count, then `get_ot_math_constant` from the tenth.
            9 if self.math.is_some() => self.param_count() as i64,
            10.. => {
                let (n, math) = (k - 10, self.math.as_ref()?);
                let v = *math.get(n)?;
                match n {
                    0 | 1 | 55 => i64::from(v),
                    _ => self.signed_points(v, size),
                }
            }
            _ => return None,
        })
    }

    /// How many `\fontdimen`s XeTeX gives the font: 8, or 65 with `MATH`.
    pub fn param_count(&self) -> usize {
        self.math.as_ref().map_or(8, |m| 9 + m.len())
    }

    fn signed_points(&self, units: i32, size: i64) -> i64 {
        let p = units as f32 * (size as f32 / 65536.0) / self.units_per_em as f32;
        // `D2Fix`: adds a half and truncates toward zero.
        (f64::from(p) * 65536.0 + 0.5) as i64
    }
}

thread_local! {
    static LOADED: RefCell<HashMap<PathBuf, Option<Rc<CharMap>>>> = RefCell::new(HashMap::new());
}

pub fn load(path: &Path) -> Option<Rc<CharMap>> {
    LOADED.with(|cache| {
        cache
            .borrow_mut()
            .entry(path.to_path_buf())
            .or_insert_with(|| std::fs::read(path).ok().and_then(parse).map(Rc::new))
            .clone()
    })
}

/// A single font (not a collection).
pub fn parse(data: Vec<u8>) -> Option<CharMap> {
    use ttf_parser::{Face, Tag};
    let face = Face::parse(&data, 0).ok()?;
    // Unicode subtables, full repertoire first (the order XeTeX's
    // FreeType/HarfBuzz charmap selection prefers).
    let subtables = face.tables().cmap?.subtables;
    let rank = |s: &ttf_parser::cmap::Subtable| {
        use ttf_parser::cmap::Format;
        match (s.platform_id, s.encoding_id, &s.format) {
            (ttf_parser::PlatformId::Windows, 10, Format::SegmentedCoverage(_))
            | (ttf_parser::PlatformId::Unicode, 4 | 6, Format::SegmentedCoverage(_)) => Some(0),
            (ttf_parser::PlatformId::Windows, 1, Format::SegmentMappingToDeltaValues(_))
            | (ttf_parser::PlatformId::Unicode, 0..=3, Format::SegmentMappingToDeltaValues(_)) => Some(1),
            _ => None,
        }
    };
    let best = subtables.into_iter().filter(|s| rank(s).is_some()).min_by_key(|s| rank(s))?;
    let mut glyphs = BTreeMap::new();
    best.codepoints(|c| {
        if let Some(g) = best.glyph_index(c).filter(|g| g.0 != 0) {
            glyphs.insert(c, u32::from(g.0));
        }
    });
    let num_glyphs = u32::from(face.number_of_glyphs());
    let advances = (0..face.tables().hhea.number_of_metrics)
        .map(|g| face.glyph_hor_advance(ttf_parser::GlyphId(g)).map_or(0, u32::from))
        .collect();
    // `OS/2` version 2 on.
    let os2 = face.tables().os2.filter(|o| o.version >= 2);
    let height = |h: Option<i16>| h.and_then(|h| u32::try_from(h).ok());
    let math = face.tables().math.and_then(|m| m.constants).map(|c| {
        let v = |x: ttf_parser::math::MathValue| i32::from(x.value);
        vec![
            i32::from(c.script_percent_scale_down()),
            i32::from(c.script_script_percent_scale_down()),
            i32::from(c.delimited_sub_formula_min_height()),
            i32::from(c.display_operator_min_height()),
            v(c.math_leading()),
            v(c.axis_height()),
            v(c.accent_base_height()),
            v(c.flattened_accent_base_height()),
            v(c.subscript_shift_down()),
            v(c.subscript_top_max()),
            v(c.subscript_baseline_drop_min()),
            v(c.superscript_shift_up()),
            v(c.superscript_shift_up_cramped()),
            v(c.superscript_bottom_min()),
            v(c.superscript_baseline_drop_max()),
            v(c.sub_superscript_gap_min()),
            v(c.superscript_bottom_max_with_subscript()),
            v(c.space_after_script()),
            v(c.upper_limit_gap_min()),
            v(c.upper_limit_baseline_rise_min()),
            v(c.lower_limit_gap_min()),
            v(c.lower_limit_baseline_drop_min()),
            v(c.stack_top_shift_up()),
            v(c.stack_top_display_style_shift_up()),
            v(c.stack_bottom_shift_down()),
            v(c.stack_bottom_display_style_shift_down()),
            v(c.stack_gap_min()),
            v(c.stack_display_style_gap_min()),
            v(c.stretch_stack_top_shift_up()),
            v(c.stretch_stack_bottom_shift_down()),
            v(c.stretch_stack_gap_above_min()),
            v(c.stretch_stack_gap_below_min()),
            v(c.fraction_numerator_shift_up()),
            v(c.fraction_numerator_display_style_shift_up()),
            v(c.fraction_denominator_shift_down()),
            v(c.fraction_denominator_display_style_shift_down()),
            v(c.fraction_numerator_gap_min()),
            v(c.fraction_num_display_style_gap_min()),
            v(c.fraction_rule_thickness()),
            v(c.fraction_denominator_gap_min()),
            v(c.fraction_denom_display_style_gap_min()),
            v(c.skewed_fraction_horizontal_gap()),
            v(c.skewed_fraction_vertical_gap()),
            v(c.overbar_vertical_gap()),
            v(c.overbar_rule_thickness()),
            v(c.overbar_extra_ascender()),
            v(c.underbar_vertical_gap()),
            v(c.underbar_rule_thickness()),
            v(c.underbar_extra_descender()),
            v(c.radical_vertical_gap()),
            v(c.radical_display_style_vertical_gap()),
            v(c.radical_rule_thickness()),
            v(c.radical_extra_ascender()),
            v(c.radical_kern_before_degree()),
            v(c.radical_kern_after_degree()),
            i32::from(c.radical_degree_bottom_raise_percent()),
        ]
    });
    let table = |t: Option<ttf_parser::opentype_layout::LayoutTable>| t.map(|t| scripts(&t)).unwrap_or_default();
    let layout = [table(face.tables().gsub), table(face.tables().gpos)];
    let mut map = CharMap {
        glyphs,
        num_glyphs,
        units_per_em: u32::from(face.units_per_em()),
        italic_angle: f64::from(face.italic_angle()),
        advances,
        x_height: height(os2.and_then(|o| o.x_height())),
        cap_height: height(os2.and_then(|o| o.capital_height())),
        graphite: face.raw_face().table(Tag::from_bytes(b"Silf")).is_some(),
        math,
        layout,
        data: Vec::new(),
    };
    map.data = data;
    Some(map)
}

/// A `GSUB` or `GPOS` table's scripts, their features by tag.
fn scripts(table: &ttf_parser::opentype_layout::LayoutTable) -> Vec<Script> {
    let features = |sys: ttf_parser::opentype_layout::LanguageSystem| -> Vec<u32> {
        sys.feature_indices.into_iter().filter_map(|i| table.features.get(i)).map(|f| f.tag.0).collect()
    };
    table
        .scripts
        .into_iter()
        .map(|s| Script {
            tag: s.tag.0,
            default: s.default_language.map(features),
            languages: s.languages.into_iter().map(|l| (l.tag.0, features(l))).collect(),
        })
        .collect()
}
