//! The engine primitives beyond the interpreter's core: TeX's glue and
//! token-list parameters, the quantities the engine computes (`last_item`),
//! the font tables, and the commands whose effect is typesetting, output or
//! diagnostics.  Each reads exactly the operands the engine reads (tex.web
//! parts 30 and 49, etex_man § 3, the pdfTeX manual), so the input stays in
//! step, and each answers with the engine's value wherever that value does
//! not depend on typesetting.

use std::cell::Cell;
use std::rc::Rc;

use crate::builtins::{CodeTable, Cond, Convert, FontParam, LastItem, Primitive, Special, Syntax};
use crate::builtins::DefMode;
use crate::env::GroupKind;
use crate::machine::{CondLimit, Machine};
use crate::mode::group;
use crate::tex::{Catcode, Meaning, RegKind, Span, Sym, Tok, Token};
use crate::value::{Scaled, Value, UNIT};

/// Keeps the font tables and the other engine state out of the namespace
/// control sequences live in, as the register names are kept out of it.
const ENGINE: char = '\u{4}';
/// etex_man § 3: `\eTeXversion` and `\eTeXrevision`.
const ETEX_VERSION: i64 = 2;
const ETEX_REVISION: &str = ".6";
/// The pdfTeX manual: `\pdftexversion` is 140 for 1.40, and
/// `\pdftexrevision` the last part of the version of the TeX Live binary.
const PDFTEX_VERSION: i64 = 140;
const PDFTEX_REVISION: &str = "29";
/// `\XeTeXversion` and `\XeTeXrevision` of XeTeX 0.999998.
const XETEX_VERSION: i64 = 0;
/// XeTeX's packing of a math code: family in bits 24 and up, class in
/// 21-23, the slot below (probed, XeTeX 0.999998).
const UMATH_FAMILY_SHIFT: i64 = 24;
const UMATH_CLASS_SHIFT: i64 = 21;
/// A delimiter code: family from bit 21, flagged by bit 30.
const UDEL_FLAG: i64 = 1 << 30;
const UDEL_FAMILY_SHIFT: i64 = 21;
const XETEX_REVISION: &str = ".999998";
/// tex.web § 1253 and § 1264: interaction levels, `\batchmode` = 0 up to
/// `\errorstopmode` = 3.
pub const ERROR_STOP_MODE: i64 = 3;
/// tex.web § 552: `\nullfont` has seven parameters, all zero.
const NULL_FONT_PARAMS: i64 = 7;
/// tex.web § 1259: `at` sizes must be below 2048pt.
const MAX_AT_SIZE: Scaled = 2048 * UNIT;
/// tex.web § 1259: `scaled` factors run from 1 to 32768.
const MAX_SCALED: i64 = 32768;
/// tex.web § 1259: 1000 means the design size.
const SCALE_UNITY: i64 = 1000;
/// tex.web § 1259: an improper `at` size is replaced by 10pt.
const DEFAULT_AT: Scaled = 10 * UNIT;

thread_local! {
    /// pdfTeX's `\ifincsname`: how many `\csname` or `\ifcsname` scans are
    /// open.  They nest only while one runs, so a counter is exact.
    static IN_CSNAME: Cell<usize> = const { Cell::new(0) };
    /// How many conditionals are reading their test, which the engine has
    /// already put on its condition stack (tex.web § 495).
    static TESTING: Cell<usize> = const { Cell::new(0) };
}

/// A `\csname` or `\ifcsname` starts scanning its name, which is what
/// `\ifincsname` asks about; [`leave_csname`] ends it.
pub fn enter_csname() -> usize {
    IN_CSNAME.with(|depth| depth.replace(depth.get() + 1))
}

pub fn leave_csname(depth: usize) {
    IN_CSNAME.with(|d| d.set(depth));
}

/// A conditional's test is being read; see `Machine::eval_condition`.
pub fn enter_test() -> usize {
    TESTING.with(|depth| depth.replace(depth.get() + 1))
}

pub fn leave_test(depth: usize) {
    TESTING.with(|d| d.set(depth));
}

fn chars(text: &str, span: Span) -> Vec<Token> {
    text.chars()
        .map(|c| Token::new(Tok::Chr(c, if c == ' ' { Catcode::Space } else { Catcode::Other }), span))
        .collect()
}

fn text_of_value(value: &Value) -> Option<String> {
    match value {
        Value::Toks(toks) => Some(
            toks.iter()
                .filter_map(|t| match t.tok {
                    Tok::Chr(c, _) => Some(c),
                    _ => None,
                })
                .collect(),
        ),
        _ => None,
    }
}

/// What `\font` finds for a name.
enum FontSource {
    Tfm(Rc<crate::tfm::Metrics>),
    /// An OpenType or AAT font: [`crate::otf`] reads what it can of it.
    Native,
    /// Found or not, satex cannot tell.
    Unknown,
    Missing,
}

impl Machine<'_> {
    fn engine_sym(&mut self, key: &str) -> Sym {
        self.intern(&format!("{ENGINE}{key}"))
    }

    pub(crate) fn engine_int(&mut self, key: &str) -> Option<i64> {
        let sym = self.engine_sym(key);
        self.env.value(sym).as_int()
    }

    fn set_engine(&mut self, key: &str, value: Value, global: bool) {
        let sym = self.engine_sym(key);
        self.env.set_value(sym, value, global);
    }

    /// `\globaldefs` (tex.web § 1214): positive makes every assignment
    /// global, negative every one local, `\global` and `\gdef` included.
    pub fn global_defs(&mut self) -> i64 {
        let sym = self.intern("globaldefs");
        self.env.value(sym).as_int().unwrap_or(0)
    }

    pub fn apply_global_defs(&mut self) {
        match self.global_defs() {
            0 => {}
            n => self.prefixes.global = n > 0,
        }
    }

    fn take_global(&mut self) -> bool {
        let global = self.prefixes.global;
        self.prefixes = Default::default();
        global
    }

    /// `scan_optional_equals` (tex.web § 405).
    pub fn scan_optional_equals(&mut self) {
        if let Some(token) = self.next_non_blank_x()
            && !matches!(token.tok, Tok::Chr('=', Catcode::Other))
        {
            self.probe_scans(token, "=");
            self.unread(token);
        }
    }

    fn next_non_blank_x(&mut self) -> Option<Token> {
        loop {
            let token = self.next_x_token()?;
            if !token.is_space() {
                return Some(token);
            }
        }
    }

    /// A ⟨general text⟩ read the way `scan_toks(false, true)` reads it:
    /// expanded as an `\edef` body is (tex.web § 473).
    fn expanded_general_text(&mut self) -> Vec<Token> {
        let body = self.read_general_text();
        self.expand_tokens(Rc::from(body))
    }

    /// The text of an expanded general text, `None` when it is unknown.
    fn expanded_text(&mut self) -> Option<String> {
        let body = self.expanded_general_text();
        self.text_known(&body)
    }

    /// Tokens as the characters `\detokenize` writes, `None` when they hold
    /// unknown text or a control sequence under an unknown `\escapechar`.
    pub(crate) fn text_known(&mut self, tokens: &[Token]) -> Option<String> {
        if self.has_unknown(tokens) {
            return None;
        }
        let escape = match self.escape_state() {
            Some(escape) => escape,
            None if tokens.iter().any(|t| t.cs().is_some()) => return None,
            None => None,
        };
        Some(crate::tex::detokenize_in(tokens, &self.out.interner, escape, &self.catcodes))
    }

    // ---- parameters ------------------------------------------------------

    fn assign_value(&mut self, by: Sym, value: Value, span: Span) {
        let value = crate::value::trap_zero_glue(value);
        let global = self.take_global();
        self.assign(by, value, global, span);
    }

    // ---- the level and value of an internal quantity ----------------------

    /// The level of the quantities this module defines, for
    /// [`Machine::internal_level`].
    pub fn extra_level(p: Primitive) -> Option<RegKind> {
        use LastItem as L;
        Some(match p {
            Primitive::GlueParameter { mu: false } => RegKind::Skip,
            Primitive::GlueParameter { mu: true } => RegKind::MuSkip,
            Primitive::TokensParameter => RegKind::Toks,
            Primitive::LastItem(item) => match item {
                L::Kern
                | L::GlueStretch
                | L::GlueShrink
                | L::FontCharWd
                | L::FontCharHt
                | L::FontCharDp
                | L::FontCharIc
                | L::ParShapeIndent
                | L::ParShapeLength
                | L::ParShapeDimen
                | L::Xetex(crate::builtins::XeQuery::GlyphBounds) => RegKind::Dimen,
                L::Skip | L::MuToGlue => RegKind::Skip,
                L::GlueToMu => RegKind::MuSkip,
                _ => RegKind::Count,
            },
            Primitive::Special(Special::BoxDimen) => RegKind::Dimen,
            Primitive::Special(Special::InterCharToks) => RegKind::Toks,
            Primitive::Special(_) => RegKind::Count,
            Primitive::FontParam(FontParam::Dimen) => RegKind::Dimen,
            Primitive::FontParam(_) => RegKind::Count,
            _ => return None,
        })
    }

    /// The value of one of this module's internal quantities; its operands
    /// are read here.  `None` when the engine's answer depends on
    /// typesetting.
    pub fn extra_quantity(&mut self, p: Primitive, sym: Sym) -> Option<Value> {
        match p {
            Primitive::GlueParameter { .. } | Primitive::TokensParameter => {
                Some(self.env.value(self.env.identity(sym)))
            }
            Primitive::LastItem(item) => self.last_item(item),
            Primitive::Special(special) => self.special_value(special, sym),
            Primitive::FontParam(param) => self.font_param_value(param, sym),
            _ => None,
        }
    }

    fn last_item(&mut self, item: LastItem) -> Option<Value> {
        use LastItem as L;
        let int = |n: i64| Some(Value::Int(n));
        match item {
            // What the last node of the current list is: typesetting.
            // etex.ch: `\lastnodetype` of an empty list is -1.
            L::NodeType if !self.material && self.mode == crate::mode::Modes::VERTICAL => int(-1),
            L::Skip => self.last_value(crate::mode::Removed::Glue),
            L::Kern => self.last_value(crate::mode::Removed::Kern),
            L::Penalty => self.last_value(crate::mode::Removed::Penalty),
            L::NodeType | L::Badness => None,
            L::InputLineNo => self.input_line().map(|line| Value::Int(line.into())),
            L::EtexVersion => int(ETEX_VERSION),
            L::PdftexVersion => int(PDFTEX_VERSION),
            L::GroupLevel => int(self.env.group_level() as i64),
            L::GroupType => match (self.env.group_kind(), self.env.nest()) {
                (None, _) => int(0),
                (Some(_), Some(nest)) => nest.code.map(|code| Value::Int(code.into())),
                (Some(GroupKind::Simple), None) => int(group::SIMPLE.into()),
                (Some(GroupKind::SemiSimple), None) => int(group::SEMI_SIMPLE.into()),
                (Some(GroupKind::Math { .. }), None) => int(group::MATH_SHIFT.into()),
            },
            L::IfLevel => int((self.conds.len() + TESTING.with(Cell::get)) as i64),
            // etex.ch: the innermost conditional, which is the one whose test
            // is being read if there is one.
            L::IfType | L::IfBranch => {
                let testing = self.testing.last() == Some(&self.conds.len()) && !self.testing_kind.is_empty();
                if testing {
                    return match item {
                        L::IfType => self.testing_kind.last().copied().flatten().map(|k| Value::Int(k.into())),
                        _ => int(0),
                    };
                }
                let Some(frame) = self.conds.last().copied() else { return int(0) };
                match item {
                    L::IfType => frame.kind.map(|k| Value::Int(k.into())),
                    _ if frame.undecided => None,
                    _ => int(match frame.limit {
                        CondLimit::Else | CondLimit::Or => 1,
                        CondLimit::Fi => -1,
                    }),
                }
            }
            L::GlueStretch | L::GlueShrink | L::GlueStretchOrder | L::GlueShrinkOrder => {
                let glue = self.scan_glue_spec(false)?;
                let part = if matches!(item, L::GlueStretch | L::GlueStretchOrder) {
                    glue.stretch
                } else {
                    glue.shrink
                };
                Some(match item {
                    L::GlueStretch | L::GlueShrink => Value::Dimen(part.amount),
                    _ => Value::Int(part.order.into()),
                })
            }
            L::GlueToMu => self.scan_glue_spec(false).map(Value::MuGlue),
            L::MuToGlue => self.scan_glue_spec(true).map(Value::Glue),
            L::FontCharWd | L::FontCharHt | L::FontCharDp | L::FontCharIc => {
                let font = self.scan_font_ident();
                let c = self.scan_int();
                let (font, c) = (font?, u32::try_from(c?).ok()?);
                let size = self.font_size(font)?;
                // A native font's width is its glyph's advance, `.notdef`'s
                // for a missing character; its height, depth and italic
                // correction are not read.
                if let Some(map) = self.native_charmap(font) {
                    return match item {
                        L::FontCharWd => map.advance(c).map(|units| Value::Dimen(map.points(units, size))),
                        _ => None,
                    };
                }
                let metrics = self.font_metrics(font);
                // A character the font lacks measures 0 (etex_man § 3.10).
                let b = match metrics {
                    Some(m) => m.char_box(c, size).unwrap_or_default(),
                    None if font == 0 => Default::default(),
                    None => return None,
                };
                Some(Value::Dimen(match item {
                    L::FontCharWd => b.width,
                    L::FontCharHt => b.height,
                    L::FontCharDp => b.depth,
                    _ => b.italic,
                }))
            }
            L::ParShapeLength | L::ParShapeIndent | L::ParShapeDimen => {
                let n = self.scan_int()?;
                self.par_shape_item(item, n).map(Value::Dimen)
            }
            L::ShellEscape => int(match self.cfg.shell_escape {
                crate::config::ShellEscape::None => 0,
                crate::config::ShellEscape::Full => 1,
                crate::config::ShellEscape::Restricted => 2,
            }),
            // The clock, the random generator and the output position.
            L::ElapsedTime | L::RandomSeed | L::LastXPos | L::LastYPos => None,
            L::XetexVersion => int(XETEX_VERSION),
            L::PdfPageCount => {
                self.read_file_name();
                None
            }
            L::Xetex(query) => self.xetex_query(query),
        }
    }

    /// XeTeX's font queries.  A TFM font answers without reading further
    /// operands (xetex.web, `not_native_font_error`); an installed font
    /// answers from the tables satex reads (`cmap`, `GSUB`'s scripts).
    fn xetex_query(&mut self, query: crate::builtins::XeQuery) -> Option<Value> {
        use crate::builtins::XeQuery as Q;
        let (has_font, numbers, name) = query.operands();
        let font = if has_font { self.scan_font_ident()? } else { self.current_font()? };
        if self.font_native(font) == Some(false) {
            let mut range = || self.font_metrics(font).map_or((1, 0), |m| m.char_range());
            return Some(match query {
                Q::GlyphBounds => Value::Dimen(0),
                Q::FirstFontChar => Value::Int(range().0),
                Q::LastFontChar => Value::Int(range().1),
                _ if query.is_text() => Value::Toks(Rc::from(Vec::new())),
                _ => Value::Int(query.tfm_answer()),
            });
        }
        // What the font's character map answers.
        if let Some(map) = self.native_charmap(font) {
            match query {
                Q::CharGlyph => {
                    let c = self.scan_int().and_then(|c| u32::try_from(c).ok());
                    return Some(Value::Int(c.and_then(|c| map.glyphs.get(&c)).map_or(0, |&g| g.into())));
                }
                Q::CountGlyphs => return Some(Value::Int(map.num_glyphs.into())),
                Q::OtCountScripts if !map.graphite => return Some(Value::Int(map.scripts().count() as i64)),
                Q::OtScriptTag if !map.graphite => {
                    let n = self.scan_int()?;
                    let tag = usize::try_from(n).ok().and_then(|n| map.scripts().nth(n));
                    return Some(Value::Int(tag.map_or(0, i64::from)));
                }
                Q::OtCountLanguages | Q::OtLanguageTag | Q::OtCountFeatures | Q::OtFeatureTag if !map.graphite => {
                    // Tags are read as 32-bit patterns.
                    let script = self.scan_int()? as u32;
                    return Some(Value::Int(match query {
                        Q::OtCountLanguages => map.count_languages(script) as i64,
                        Q::OtLanguageTag => {
                            let n = self.scan_int()?;
                            usize::try_from(n).map_or(0, |n| map.language_tag(script, n).into())
                        }
                        _ => {
                            let language = self.scan_int()? as u32;
                            let features = map.features(script, language);
                            if query == Q::OtCountFeatures {
                                features.len() as i64
                            } else {
                                let n = self.scan_int()?;
                                usize::try_from(n).ok().and_then(|n| features.get(n)).map_or(0, |&t| t.into())
                            }
                        }
                    }));
                }
                Q::GlyphIndex => {
                    let name = self.read_file_name();
                    return Some(Value::Int(map.glyph_index(&name).into()));
                }
                // XeTeX reference: 2 for a font laid out as OpenType.
                Q::FontType if !map.graphite => return Some(Value::Int(2)),
                Q::FirstFontChar => return Some(Value::Int(map.glyphs.keys().next().map_or(0, |&c| c.into()))),
                Q::LastFontChar => return Some(Value::Int(map.glyphs.keys().next_back().map_or(0, |&c| c.into()))),
                _ => {}
            }
        }
        for _ in 0..numbers {
            self.scan_int();
        }
        if name {
            self.read_file_name();
        }
        None
    }

    /// `\XeTeXglyphname font n`: empty for a TFM font or a glyph without
    /// a name.
    fn xetex_glyph_name(&mut self) -> Option<String> {
        let font = self.scan_font_ident()?;
        let map = self.native_charmap(font);
        let n = self.scan_int();
        match map {
            Some(map) => Some(map.glyph_name(n?).to_string()),
            None => (self.font_native(font) == Some(false)).then(String::new),
        }
    }

    /// etex_man § 3.11: `\parshapelength n`, `\parshapeindent n` and
    /// `\parshapedimen n` read the current `\parshape`, the last line
    /// standing for all after it.
    fn par_shape_item(&mut self, item: LastItem, n: i64) -> Option<Scaled> {
        let lines = self.engine_int("parshape")?;
        if n <= 0 || lines <= 0 {
            return Some(0);
        }
        let (line, indent) = match item {
            LastItem::ParShapeDimen => ((n + n % 2) / 2, n % 2 == 1),
            LastItem::ParShapeIndent => (n, true),
            _ => (n, false),
        };
        let line = line.min(lines);
        let key = format!("parshape.{}{line}", if indent { 'i' } else { 'l' });
        let sym = self.engine_sym(&key);
        self.env.value(sym).as_dimen()
    }

    fn special_value(&mut self, special: Special, by: Sym) -> Option<Value> {
        match special {
            Special::BoxDimen => {
                let n = self.scan_int();
                let which = self.name(self.env.identity(by)).to_string();
                self.box_dimen(&which, n)
            }
            Special::ParShape => self.engine_int("parshape").or(Some(0)).map(Value::Int),
            Special::UCode { math, num } => {
                let c = self.scan_int().and_then(|c| u32::try_from(c).ok()).and_then(char::from_u32)?;
                // "Can't use \Umathcode as a number (try \Umathcodenum)".
                if !num {
                    return Some(Value::Int(0));
                }
                let table = if math { CodeTable::Math } else { CodeTable::Delimiter };
                Some(Value::Int(self.char_code(table, c)))
            }
            Special::CharClass => {
                let c = self.scan_int()?;
                Some(Value::Int(self.engine_int(&format!("charclass.{c}")).unwrap_or(0)))
            }
            Special::InterCharToks => {
                let (a, b) = (self.scan_int(), self.scan_int());
                let sym = self.engine_sym(&format!("interchartoks.{}.{}", a?, b?));
                match self.env.value(sym) {
                    Value::Unknown if self.env.get(sym).is_none() => Some(Value::Toks(Rc::from(Vec::new()))),
                    value => Some(value),
                }
            }
            Special::Penalties => {
                let n = self.scan_int()?;
                let name = self.name(by).to_string();
                let count = self.engine_int(&format!("{name}.count")).unwrap_or(0);
                if n <= 0 {
                    return Some(Value::Int(if n == 0 { count } else { 0 }));
                }
                if count == 0 {
                    return Some(Value::Int(0));
                }
                let k = n.min(count);
                Some(Value::Int(self.engine_int(&format!("{name}.{k}")).unwrap_or(0)))
            }
        }
    }

    fn assign_special(&mut self, special: Special, by: Sym) {
        match special {
            // tex.web § 1247: `\wd⟨n⟩=⟨dimen⟩` changes a box satex does not
            // build.
            Special::BoxDimen => {
                let n = self.scan_int();
                self.scan_optional_equals();
                let value = self.scan_dimen();
                self.prefixes = Default::default();
                let which = self.name(self.env.identity(by)).to_string();
                self.set_box_dimen(&which, n, value);
            }
            // tex.web § 1248.
            Special::ParShape => {
                self.scan_optional_equals();
                let lines = self.scan_int().unwrap_or(0).max(0);
                let global = self.take_global();
                for k in 1..=lines {
                    let indent = self.scan_dimen().map_or(Value::Unknown, Value::Dimen);
                    let length = self.scan_dimen().map_or(Value::Unknown, Value::Dimen);
                    self.set_engine(&format!("parshape.i{k}"), indent, global);
                    self.set_engine(&format!("parshape.l{k}"), length, global);
                }
                self.set_engine("parshape", Value::Int(lines), global);
            }
            Special::UCode { math, num } => {
                let c = self.scan_int().and_then(|c| u32::try_from(c).ok()).and_then(char::from_u32);
                self.scan_optional_equals();
                let code = match (num, math) {
                    (true, _) => self.scan_int(),
                    (false, true) => self.scan_umath_char(),
                    (false, false) => {
                        let (family, slot) = (self.scan_int(), self.scan_int());
                        family.zip(slot).map(|(f, s)| UDEL_FLAG | f << UDEL_FAMILY_SHIFT | s)
                    }
                };
                let global = self.take_global();
                let table = if math { CodeTable::Math } else { CodeTable::Delimiter };
                if let Some(c) = c {
                    let sym = self.char_code_sym(table, c);
                    self.env.set_value(sym, code.map_or(Value::Unknown, Value::Int), global);
                }
            }
            Special::CharClass => {
                let c = self.scan_int();
                self.scan_optional_equals();
                let class = self.scan_int().map_or(Value::Unknown, Value::Int);
                let global = self.take_global();
                if let Some(c) = c {
                    self.set_engine(&format!("charclass.{c}"), class, global);
                }
            }
            Special::InterCharToks => {
                let (a, b) = (self.scan_int(), self.scan_int());
                self.scan_optional_equals();
                let toks = self.read_general_text();
                let global = self.take_global();
                if let (Some(a), Some(b)) = (a, b) {
                    self.set_engine(&format!("interchartoks.{a}.{b}"), Value::Toks(Rc::from(toks)), global);
                }
            }
            // etex_man § 3.12.
            Special::Penalties => {
                self.scan_optional_equals();
                let count = self.scan_int().unwrap_or(0).max(0);
                let global = self.take_global();
                let name = self.name(by).to_string();
                for k in 1..=count {
                    let value = self.scan_int().map_or(Value::Unknown, Value::Int);
                    self.set_engine(&format!("{name}.{k}"), value, global);
                }
                self.set_engine(&format!("{name}.count"), Value::Int(count), global);
            }
        }
    }

    // ---- fonts -------------------------------------------------------------

    fn font_key(&mut self, font: u32, field: &str) -> Sym {
        self.engine_sym(&format!("font{font}.{field}"))
    }

    fn font_field(&mut self, font: u32, field: &str) -> Value {
        let sym = self.font_key(font, field);
        self.env.value(sym)
    }

    /// Font data is not subject to grouping (tex.web § 1253).
    fn set_font_field(&mut self, font: u32, field: &str, value: Value) {
        let sym = self.font_key(font, field);
        self.env.set_value(sym, value, true);
    }

    fn font_count(&mut self) -> u32 {
        self.engine_int("fonts").and_then(|n| u32::try_from(n).ok()).unwrap_or(0)
    }

    fn font_name(&mut self, font: u32) -> Option<String> {
        if font == 0 {
            return Some("nullfont".into());
        }
        text_of_value(&self.font_field(font, "name"))
    }

    pub(crate) fn font_size(&mut self, font: u32) -> Option<Scaled> {
        if font == 0 {
            return Some(0);
        }
        self.font_field(font, "size").as_dimen()
    }

    pub(crate) fn font_metrics(&mut self, font: u32) -> Option<Rc<crate::tfm::Metrics>> {
        if font == 0 || self.font_native(font) != Some(false) {
            return None;
        }
        let name = self.font_name(font)?;
        self.find_tfm(&name)
    }

    /// The character map of a native font loaded by file name
    /// (`[file]:features`); `None` for one fontconfig finds by name.
    pub(crate) fn native_charmap(&mut self, font: u32) -> Option<Rc<crate::otf::CharMap>> {
        if font == 0 || self.font_native(font) != Some(true) {
            return None;
        }
        let name = self.font_name(font)?;
        let file = name.strip_prefix('[')?.split(']').next()?.to_string();
        let base = self.base().to_path_buf();
        let path = if base.join(&file).is_file() {
            base.join(&file)
        } else {
            let roots = self.resolver_mut().distribution().roots.clone();
            let bare = std::path::Path::new(&file).file_name()?.to_str()?.to_string();
            ["", "otf", "ttf"].iter().find_map(|ext| crate::tfm::locate(&base, &roots, &bare, ext))?
        };
        crate::otf::load(&path)
    }

    fn find_tfm(&mut self, name: &str) -> Option<Rc<crate::tfm::Metrics>> {
        let base = self.base().to_path_buf();
        let roots = self.resolver_mut().distribution().roots.clone();
        crate::tfm::locate(&base, &roots, name, "tfm").and_then(|path| crate::tfm::load(&path))
    }

    /// Whether the font is an OpenType or AAT font (XeTeX's native fonts),
    /// `None` when that is unknown.
    pub(crate) fn font_native(&mut self, font: u32) -> Option<bool> {
        if font == 0 {
            return Some(false);
        }
        self.font_field(font, "native").as_int().map(|n| n != 0)
    }

    /// A font whose loading satex could not decide may be `\nullfont`.
    pub(crate) fn font_may_be_null(&mut self, meaning: &Meaning) -> bool {
        match meaning {
            Meaning::Primitive(Primitive::FontIdent(font)) if *font > 0 => {
                matches!(self.font_field(*font, "exists"), Value::Unknown)
            }
            _ => false,
        }
    }

    /// The font the next font-valued operand names (tex.web § 577,
    /// `scan_font_ident`): `\font` for the current one, a font identifier,
    /// or `\textfont⟨n⟩` and its relatives.
    pub fn scan_font_ident(&mut self) -> Option<u32> {
        let token = self.next_non_blank_x()?;
        let meaning = match token.cs().or_else(|| self.active_cs(token)) {
            Some(sym) => self.env.meaning(sym),
            None => Meaning::Undefined,
        };
        match meaning {
            Meaning::Primitive(Primitive::FontDef) => self.current_font(),
            Meaning::Primitive(Primitive::FontIdent(font)) => Some(font),
            Meaning::Primitive(Primitive::Command(Syntax::FamilyFont)) => {
                let by = token.cs()?;
                let family = self.scan_int()?;
                self.family_font(by, family)
            }
            _ => {
                // "Missing font identifier": the token goes back and the
                // null font is used.
                self.unread(token);
                Some(0)
            }
        }
    }

    pub(crate) fn current_font(&mut self) -> Option<u32> {
        self.engine_int("font").map_or(Some(0), |f| u32::try_from(f).ok())
    }

    fn family_font(&mut self, by: Sym, family: i64) -> Option<u32> {
        let key = format!("{}.{family}", self.name(by));
        self.engine_int(&key).map_or(Some(0), |f| u32::try_from(f).ok())
    }

    /// `\font⟨cs⟩=⟨file name⟩⟨at clause⟩` (tex.web §§ 1257-1260): a font
    /// already loaded at that size is shared, a font that cannot be loaded
    /// leaves `\nullfont`, anything else is loaded anew.
    pub fn load_font(&mut self, by: Sym, span: Span) {
        let Some(name_cs) = self.read_r_token() else { return };
        let global = self.prefixes.global;
        self.scan_optional_equals();
        let (name, quoted) = self.read_file_name_quoted();
        // Positive: `at` the size; negative: `scaled` the factor.
        let requested = if self.scan_keyword("at") {
            self.scan_dimen().map(|s| if s <= 0 || s >= MAX_AT_SIZE { DEFAULT_AT } else { s })
        } else if self.scan_keyword("scaled") {
            self.scan_int().map(|n| if n <= 0 || n > MAX_SCALED { -SCALE_UNITY } else { -n })
        } else {
            Some(-SCALE_UNITY)
        };
        let font = self.find_or_load_font(&name, quoted, requested, span);
        if font > 0 {
            self.set_font_field(font, "cs", Value::Int(i64::from(name_cs.0)));
        }
        self.prefixes.global = global;
        self.define(name_cs, Meaning::Primitive(Primitive::FontIdent(font)), by, DefMode::Declare, span, None);
    }

    /// Where the engine finds the font (tex.web § 563; xetex.web
    /// `load_native_font`: a quoted name is tried as an installed font
    /// first, an unquoted one only once no TFM file has it).
    fn find_font(&mut self, name: &str, quoted: bool) -> FontSource {
        use crate::config::Engine;
        let engine = self.out.plugins.engine;
        if engine == Engine::XeTeX && quoted {
            match self.native_font_exists(name) {
                Some(true) => return FontSource::Native,
                None => return FontSource::Unknown,
                Some(false) => {}
            }
        }
        if let Some(metrics) = self.find_tfm(name) {
            return FontSource::Tfm(metrics);
        }
        match engine {
            Engine::XeTeX if !quoted => match self.native_font_exists(name) {
                Some(true) => FontSource::Native,
                Some(false) => FontSource::Missing,
                None => FontSource::Unknown,
            },
            Engine::XeTeX => FontSource::Missing,
            // luaotfload's `define_font` callback, which is Lua, may find it.
            Engine::LuaTeX => FontSource::Unknown,
            // mktextfm may make it from METAFONT source.
            _ => {
                let base = self.base().to_path_buf();
                let roots = self.resolver_mut().distribution().roots.clone();
                match crate::tfm::locate(&base, &roots, name, "mf") {
                    Some(_) => FontSource::Unknown,
                    None => FontSource::Missing,
                }
            }
        }
    }

    /// XeTeX's lookup of an installed font: `[file]` through kpathsea,
    /// anything else by name through fontconfig.  `None` when fontconfig
    /// cannot be asked.
    fn native_font_exists(&mut self, spec: &str) -> Option<bool> {
        let name = spec.split(':').next().unwrap_or(spec);
        if let Some(file) = name.strip_prefix('[') {
            let file = file.split(']').next().unwrap_or(file);
            let base = self.base().to_path_buf();
            if base.join(file).is_file() {
                return Some(true);
            }
            let roots = self.resolver_mut().distribution().roots.clone();
            let bare = std::path::Path::new(file).file_name().and_then(|f| f.to_str()).unwrap_or(file);
            let found = ["", "otf", "ttf", "otc", "ttc"]
                .iter()
                .any(|ext| crate::tfm::locate(&base, &roots, bare, ext).is_some());
            return Some(found);
        }
        let family = name.split('/').next().unwrap_or(name).trim();
        crate::tfm::installed_font(family)
    }

    fn find_or_load_font(&mut self, name: &str, quoted: bool, requested: Option<Scaled>, span: Span) -> u32 {
        let (metrics, native, exists) = match self.find_font(name, quoted) {
            FontSource::Tfm(metrics) => (Some(metrics), Value::Int(0), Value::Int(1)),
            FontSource::Native => (None, Value::Int(1), Value::Int(1)),
            FontSource::Unknown => (None, Value::Unknown, Value::Unknown),
            FontSource::Missing => {
                let suppress = self.intern("suppressfontnotfounderror");
                if self.env.value(suppress).as_int().unwrap_or(0) <= 0 {
                    let message = format!("font {name} not loadable: metric (TFM) file or installed font not found");
                    self.diagnose(crate::facts::Severity::Warning, "font-not-loadable", span, message);
                }
                return 0;
            }
        };
        let design = metrics.as_ref().map(|m| m.design_size);
        let size = match requested {
            Some(at) if at > 0 => Some(at),
            Some(factor) => design.map(|d| crate::scan::xn_over_d(d, -factor, SCALE_UNITY).0),
            None => None,
        };
        let request = requested.map_or(Value::Unknown, Value::Dimen);
        let count = self.font_count();
        for font in 1..=count {
            if self.font_name(font).as_deref() == Some(name) {
                let same_size = size.is_some() && self.font_size(font) == size;
                let same_request = requested.is_some() && self.font_field(font, "request") == request;
                if same_size || same_request {
                    return font;
                }
            }
        }
        let font = count + 1;
        self.set_engine("fonts", Value::Int(font.into()), true);
        self.set_font_field(font, "name", Value::Toks(Rc::from(chars(name, Span::default()))));
        self.set_font_field(font, "size", size.map_or(Value::Unknown, Value::Dimen));
        self.set_font_field(font, "dsize", design.map_or(Value::Unknown, Value::Dimen));
        self.set_font_field(font, "request", request);
        self.set_font_field(font, "native", native);
        self.set_font_field(font, "exists", exists);
        // An installed font's parameters are not read: unknown, and so is
        // how many it has.
        let params = metrics.as_ref().map_or(Value::Unknown, |m| Value::Int(m.param_count() as i64));
        self.set_font_field(font, "params", params);
        // xetex.web `load_native_font`: eight from the font's tables, 65
        // for a math font.
        if let Some(map) = self.native_charmap(font) {
            self.set_font_field(font, "params", Value::Int(map.param_count() as i64));
        }
        for (field, parameter) in [("hyphenchar", "defaulthyphenchar"), ("skewchar", "defaultskewchar")] {
            let sym = self.intern(parameter);
            let value = self.env.value(sym);
            self.set_font_field(font, field, value);
        }
        font
    }

    /// `\fontdimen⟨n⟩⟨font⟩`, `\hyphenchar⟨font⟩`, `\skewchar⟨font⟩`,
    /// `\lpcode⟨font⟩⟨char⟩`: which entry of the font tables they name.
    fn font_entry(&mut self, param: FontParam, by: Sym) -> Option<(u32, String)> {
        match param {
            FontParam::Dimen => {
                let n = self.scan_int();
                let font = self.scan_font_ident()?;
                Some((font, format!("p{}", n?)))
            }
            FontParam::HyphenChar => Some((self.scan_font_ident()?, "hyphenchar".into())),
            FontParam::SkewChar => Some((self.scan_font_ident()?, "skewchar".into())),
            FontParam::CharCode(_) => {
                let font = self.scan_font_ident();
                let c = self.scan_int();
                Some((font?, format!("{}.{}", self.name(by), c?)))
            }
        }
    }

    /// `\fontdimen k` of the current font: `em` is `quad` (6) and `ex`
    /// `x_height` (5), tex.web § 455.
    pub(crate) fn current_font_dimen(&mut self, k: usize) -> Option<Scaled> {
        let font = self.current_font()?;
        self.font_dimen(font, k)?.as_dimen()
    }

    fn font_dimen(&mut self, font: u32, k: usize) -> Option<Value> {
        let stored = self.font_field(font, &format!("p{k}"));
        if !matches!(stored, Value::Unknown) {
            return Some(stored);
        }
        let params = self.font_params(font)?;
        if k == 0 || k as i64 > params {
            // tex.web §§ 578-580: past the end, the font loaded last grows;
            // any other gets "Font has only n fontdimen parameters".  Both
            // read zero.
            if k > 0 && font == self.font_count() {
                self.set_font_field(font, "params", Value::Int(k as i64));
            }
            return Some(Value::Dimen(0));
        }
        let size = self.font_size(font)?;
        if let Some(map) = self.native_charmap(font) {
            return map.param(k, size).map(Value::Dimen);
        }
        let from_tfm = self.font_metrics(font).and_then(|m| m.param(k, size));
        match from_tfm {
            Some(value) => Some(Value::Dimen(value)),
            // An extension of the table starts at zero (tex.web § 580).
            None if font == 0 || k > self.tfm_params(font).unwrap_or(NULL_FONT_PARAMS as usize) => {
                Some(Value::Dimen(0))
            }
            None => None,
        }
    }

    fn font_param_value(&mut self, param: FontParam, by: Sym) -> Option<Value> {
        let (font, field) = self.font_entry(param, by)?;
        let stored = self.font_field(font, &field);
        if !matches!(stored, Value::Unknown) {
            return Some(stored);
        }
        match param {
            FontParam::Dimen => {
                let k: usize = field[1..].parse().ok()?;
                self.font_dimen(font, k)
            }
            FontParam::HyphenChar if font == 0 => Some(Value::Int('-' as i64)),
            FontParam::SkewChar if font == 0 => Some(Value::Int(-1)),
            FontParam::CharCode(default) => Some(Value::Int(default.into())),
            _ => None,
        }
    }

    fn font_params(&mut self, font: u32) -> Option<i64> {
        if font == 0 {
            return Some(self.engine_int("font0.params").unwrap_or(NULL_FONT_PARAMS));
        }
        self.font_field(font, "params").as_int()
    }

    fn tfm_params(&mut self, font: u32) -> Option<usize> {
        self.font_metrics(font).map(|m| m.param_count())
    }

    /// Assignments to the font tables are global (tex.web § 1253); a
    /// `\fontdimen` past the end of the table extends it, but only for the
    /// font loaded last (tex.web § 580).
    fn assign_font_param(&mut self, param: FontParam, by: Sym) {
        let entry = self.font_entry(param, by);
        self.scan_optional_equals();
        let value = match param {
            FontParam::Dimen => self.scan_dimen().map_or(Value::Unknown, Value::Dimen),
            _ => self.scan_int().map_or(Value::Unknown, Value::Int),
        };
        self.prefixes = Default::default();
        let Some((font, field)) = entry else { return };
        if param == FontParam::Dimen {
            let Ok(k) = field[1..].parse::<i64>() else { return };
            let params = self.font_params(font).unwrap_or(i64::MAX);
            if k > params {
                if font < self.font_count() {
                    return;
                }
                self.set_font_field(font, "params", Value::Int(k));
            }
            if k <= 0 {
                return;
            }
        }
        self.set_font_field(font, &field, value);
    }

    pub(crate) fn font_name_text(&mut self, font: u32) -> String {
        match self.font_name_known(font) {
            Some(text) => text,
            None => self.font_name(font).unwrap_or_default(),
        }
    }

    /// What `\fontname` prints: the name, with an `at` clause when the size
    /// is not the design size (tex.web § 1261); XeTeX quotes an installed
    /// font's name.
    pub(crate) fn font_name_known(&mut self, font: u32) -> Option<String> {
        let name = self.font_name(font)?;
        match self.font_native(font) {
            Some(false) => {}
            Some(true) if self.font_field(font, "request") == Value::Dimen(-SCALE_UNITY) => {
                return Some(format!("\"{name}\""));
            }
            _ => return None,
        }
        let (size, design) = (self.font_size(font), self.font_field(font, "dsize").as_dimen());
        Some(match (size, design) {
            (Some(size), Some(design)) if size != design => {
                format!("{name} at {}pt", crate::value::render_dimen(size).trim_end_matches("pt"))
            }
            _ => name,
        })
    }

    // ---- executing ----------------------------------------------------------

    /// The effect of one of this module's primitives when the stomach obeys
    /// it, or, for a [`Convert`], when the gullet expands it.
    pub fn execute_extra(&mut self, p: Primitive, by: Sym, span: Span) {
        match p {
            Primitive::GlueParameter { mu } => {
                self.scan_optional_equals();
                let glue = self.scan_glue_spec(mu);
                let value = match (glue, mu) {
                    (Some(g), false) => Value::Glue(g),
                    (Some(g), true) => Value::MuGlue(g),
                    (None, _) => Value::Unknown,
                };
                let by = self.env.identity(by);
                self.assign_value(by, value, span);
            }
            Primitive::TokensParameter => {
                self.scan_optional_equals();
                let toks = self.read_general_text();
                let by = self.env.identity(by);
                self.assign_value(by, Value::Toks(Rc::from(toks)), span);
            }
            // "You can't use `\lastpenalty' in vertical mode": a last item
            // is only ever read.
            Primitive::LastItem(_) => {}
            Primitive::Special(special) => self.assign_special(special, by),
            Primitive::FontParam(param) => self.assign_font_param(param, by),
            // tex.web § 1217: selecting a font is a local assignment.
            Primitive::FontIdent(font) => {
                let global = self.take_global();
                self.set_engine("font", Value::Int(font.into()), global);
            }
            Primitive::Command(syntax) => self.obey_command(syntax, by, span),
            Primitive::Convert(convert) => {
                let tokens = self.convert(convert, span);
                self.unread_all(&tokens);
            }
            _ => {}
        }
    }

    fn obey_command(&mut self, syntax: Syntax, by: Sym, span: Span) {
        match syntax {
            Syntax::Nothing => {}
            Syntax::Number => {
                self.scan_int();
            }
            Syntax::Dimen => {
                self.scan_dimen();
            }
            Syntax::Delimiters { dimen } => {
                self.scan_delimiter();
                self.scan_delimiter();
                if dimen {
                    self.scan_dimen();
                }
            }
            Syntax::Interaction(mode) => {
                let sym = self.intern("interactionmode");
                self.env.set_value(sym, Value::Int(mode.into()), true);
            }
            Syntax::GeneralText => {
                self.read_general_text();
            }
            Syntax::NumberText => {
                self.scan_int();
                self.read_general_text();
            }
            Syntax::FontText => {
                self.scan_font_ident();
                self.read_general_text();
            }
            Syntax::TwoTexts => {
                self.read_general_text();
                self.read_general_text();
            }
            Syntax::Font => {
                self.scan_font_ident();
            }
            Syntax::Token => {
                self.next_token();
            }
            Syntax::Internal => {
                self.the_tokens(span);
            }
            Syntax::BoxSpec => {
                if self.scan_keyword("to") || self.scan_keyword("spread") {
                    self.scan_dimen();
                }
            }
            Syntax::RuleSpec => {
                self.scan_rule_spec();
            }
            // tex.web § 1241: the box is scanned in `box_context`
            // `box_flag` (or `global_box_flag`), and an `\afterassignment`
            // token is read once it has begun.
            Syntax::SetBox => {
                let register = self.scan_int();
                self.scan_optional_equals();
                let global = self.prefixes.global;
                self.prefixes = Default::default();
                self.scan_box(crate::mode::BoxContext::Set { register, global }, span);
            }
            Syntax::VSplit => {
                self.scan_int();
                self.scan_keyword("to");
                self.scan_dimen();
            }
            // tex.web § 1374: a deferred `\openout` acts at shipout, which
            // satex never reaches.
            Syntax::OpenOut => {
                let immediate = std::mem::take(&mut self.prefixes.immediate);
                let stream = self.scan_int();
                self.scan_optional_equals();
                let name = self.read_file_name();
                if let (true, Some(stream)) = (immediate, stream) {
                    self.set_engine(&format!("write_open.{stream}"), Value::Int(1), true);
                    // The file exists from here on, which `\input` of it
                    // (the `.aux` file at `\end{document}`) finds.
                    self.set_engine(&format!("written.{}", name.trim()), Value::Int(1), true);
                    self.stream_files.insert(stream, name.trim().to_string());
                }
            }
            Syntax::CloseOut => {
                let immediate = std::mem::take(&mut self.prefixes.immediate);
                let stream = self.scan_int();
                if let (true, Some(stream)) = (immediate, stream) {
                    self.set_engine(&format!("write_open.{stream}"), Value::Int(0), true);
                }
            }
            Syntax::FamilyFont => {
                let family = self.scan_int();
                self.scan_optional_equals();
                let font = self.scan_font_ident();
                let global = self.take_global();
                if let Some(family) = family {
                    let key = format!("{}.{family}", self.name(by));
                    self.set_engine(&key, font.map_or(Value::Unknown, |f| Value::Int(f.into())), global);
                }
            }
            Syntax::CopyFont { amount } => {
                let Some(name_cs) = self.read_r_token() else { return };
                let font = self.scan_font_ident();
                if amount {
                    self.scan_int();
                }
                let copy = font.map(|font| self.copy_font(font));
                if let Some(copy) = copy {
                    self.set_font_field(copy, "cs", Value::Int(i64::from(name_cs.0)));
                }
                self.define(
                    name_cs,
                    Meaning::Primitive(Primitive::FontIdent(copy.unwrap_or(0))),
                    by,
                    DefMode::Declare,
                    span,
                    None,
                );
            }
            Syntax::UMathCharDef { num } => {
                let Some(name) = self.read_r_token() else { return };
                self.scan_optional_equals();
                let code = if num { self.scan_int() } else { self.scan_umath_char() };
                let Some(code) = code else {
                    self.define(name, Meaning::Unknown, by, DefMode::Declare, span, None);
                    return;
                };
                let meaning = Meaning::Register(RegKind::MathChar, code as u16);
                self.define(name, meaning, by, DefMode::Declare, span, None);
                self.env.set_value(name, Value::Int(code), true);
            }
            Syntax::SetFontId => {
                let font = self.scan_int();
                let global = self.take_global();
                let known = font.filter(|&f| f >= 0 && f <= i64::from(self.font_count()));
                self.set_engine("font", known.map_or(Value::Unknown, Value::Int), global);
            }
            Syntax::Numbers(n) => {
                for _ in 0..n {
                    self.scan_int();
                }
            }
            Syntax::MathAccent => {
                while ["fixed", "bottom", "top", "both", "overlay", "nooverflow"].iter().any(|k| self.scan_keyword(k)) {}
                for _ in 0..3 {
                    self.scan_int();
                }
            }
            Syntax::FileName => {
                self.read_file_name();
            }
            // XeTeX reference, "Graphics".
            Syntax::PicFile => {
                self.read_file_name();
                loop {
                    if ["page", "scaled", "xscaled", "yscaled"].iter().any(|k| self.scan_keyword(k)) {
                        self.scan_int();
                    } else if ["width", "height", "rotated"].iter().any(|k| self.scan_keyword(k)) {
                        self.scan_dimen();
                    } else if !["crop", "media", "bleed", "trim", "art"].iter().any(|k| self.scan_keyword(k)) {
                        break;
                    }
                }
            }
            Syntax::FontExpand => {
                self.scan_font_ident();
                self.scan_int();
                self.scan_int();
                self.scan_int();
                self.scan_keyword("autoexpand");
            }
            Syntax::ReadLine => self.read_line_verbatim(by, span),
            Syntax::PdfAnnot => {
                if self.scan_keyword("reserveobjnum") {
                    return;
                }
                if self.scan_keyword("useobjnum") {
                    self.scan_int();
                }
                self.scan_rule_spec();
                self.read_general_text();
            }
            Syntax::PdfStartLink => {
                self.scan_rule_spec();
                self.scan_attr();
                self.scan_action();
            }
            Syntax::PdfDest => {
                if self.scan_keyword("struct") {
                    self.scan_int();
                }
                if self.scan_keyword("num") {
                    self.scan_int();
                } else if self.scan_keyword("name") {
                    self.read_general_text();
                }
                if self.scan_keyword("xyz") {
                    if self.scan_keyword("zoom") {
                        self.scan_int();
                    }
                } else if self.scan_keyword("fitr") {
                    self.scan_rule_spec();
                } else {
                    for kind in ["fitbh", "fitbv", "fitb", "fith", "fitv", "fit"] {
                        if self.scan_keyword(kind) {
                            break;
                        }
                    }
                }
            }
            Syntax::PdfOutline => {
                self.scan_attr();
                self.scan_action();
                if self.scan_keyword("count") {
                    self.scan_int();
                }
                self.read_general_text();
            }
            Syntax::PdfThread => {
                self.scan_rule_spec();
                self.scan_attr();
                if self.scan_keyword("num") {
                    self.scan_int();
                } else if self.scan_keyword("name") {
                    self.read_general_text();
                }
            }
            Syntax::PdfXForm => {
                self.scan_attr();
                if self.scan_keyword("resources") {
                    self.read_general_text();
                }
                self.scan_int();
            }
            Syntax::PdfXImage => {
                self.scan_rule_spec();
                self.scan_attr();
                if self.scan_keyword("named") {
                    self.read_general_text();
                } else if self.scan_keyword("page") {
                    self.scan_int();
                }
                if self.scan_keyword("colorspace") {
                    self.scan_int();
                }
                for pagebox in ["mediabox", "cropbox", "bleedbox", "trimbox", "artbox"] {
                    if self.scan_keyword(pagebox) {
                        break;
                    }
                }
                // pdfTeX reads the file name as an expanded general text.
                let file = self.read_general_text();
                let file = self.expand_tokens(file.into());
                let name = self.text_of(&file);
                crate::observe::image(self, &name, span);
            }
            Syntax::PdfColorStack => {
                self.scan_int();
                if self.scan_keyword("set") || self.scan_keyword("push") {
                    self.read_general_text();
                } else if !self.scan_keyword("pop") {
                    self.scan_keyword("current");
                }
            }
            Syntax::PdfCatalog => {
                self.read_general_text();
                if self.scan_keyword("openaction") {
                    self.scan_action();
                }
            }
        }
    }


    /// tex.web § 1160 (`scan_delimiter`): a character, or `\delimiter` and
    /// its 27-bit code.
    fn scan_delimiter(&mut self) {
        loop {
            let Some(token) = self.next_non_blank_x() else { return };
            let meaning = token.cs().map(|sym| self.env.meaning(sym));
            match meaning {
                Some(Meaning::Primitive(Primitive::Relax)) => continue,
                Some(Meaning::Primitive(Primitive::Command(Syntax::Number))) => {
                    self.scan_int();
                }
                _ => {}
            }
            return;
        }
    }

    /// tex.web § 463 (`scan_rule_spec`), which pdfTeX's commands share.
    /// The width, height and depth a rule specification gives, the last
    /// one of each keyword winning; `None` where not given.
    pub(crate) fn scan_rule_spec(&mut self) -> [Option<Option<i64>>; 3] {
        let mut spec = [None; 3];
        while let Some(k) = ["width", "height", "depth"].iter().position(|k| self.scan_keyword(k)) {
            spec[k] = Some(self.scan_dimen());
        }
        spec
    }

    fn scan_attr(&mut self) {
        if self.scan_keyword("attr") {
            self.read_general_text();
        }
    }

    /// The pdfTeX manual's ⟨action spec⟩.
    fn scan_action(&mut self) {
        if self.scan_keyword("user") {
            self.read_general_text();
        } else if self.scan_keyword("goto") {
            if self.scan_keyword("file") {
                self.read_general_text();
            }
            if self.scan_keyword("page") {
                self.scan_int();
                self.read_general_text();
            } else if self.scan_keyword("name") {
                self.read_general_text();
            } else if self.scan_keyword("num") {
                self.scan_int();
            }
            if !self.scan_keyword("newwindow") {
                self.scan_keyword("nonewwindow");
            }
        } else if self.scan_keyword("thread") {
            if self.scan_keyword("file") {
                self.read_general_text();
            }
            if self.scan_keyword("num") {
                self.scan_int();
            } else if self.scan_keyword("name") {
                self.read_general_text();
            }
        }
    }

    fn copy_font(&mut self, font: u32) -> u32 {
        let copy = self.font_count() + 1;
        self.set_engine("fonts", Value::Int(copy.into()), true);
        for field in ["name", "size", "dsize", "request", "native", "exists", "params", "hyphenchar", "skewchar"] {
            let value = self.font_field(font, field);
            self.set_font_field(copy, field, value);
        }
        copy
    }

    /// e-TeX's `\readline⟨n⟩to⟨cs⟩` (etex_man § 3.2): the line as characters
    /// of category 12, a space as 10, with the `\endlinechar` at its end.
    fn read_line_verbatim(&mut self, by: Sym, span: Span) {
        let stream = self.scan_int().unwrap_or(-1);
        self.scan_keyword("to");
        let Some(name) = self.read_r_token() else { return };
        // tex.web § 484: a stream that is not open is the terminal.
        if !(0..16).contains(&stream) || self.stream_at_end(stream) {
            if self.terminal_read_is_fatal(span) {
                return;
            }
            self.define(name, Meaning::Unknown, by, DefMode::Declare, span, None);
            return;
        }
        let mut line = self.take_stream_line(stream).unwrap_or_default();
        let sym = self.intern("endlinechar");
        if let Some(c) = self.env.value(sym).as_int().and_then(|c| u8::try_from(c).ok()) {
            line.push(char::from(c));
        }
        let body = chars(&line, span);
        let meaning = self.make_macro(Default::default(), None, body);
        self.define(name, meaning, by, DefMode::Declare, span, None);
    }

    // ---- expandable ---------------------------------------------------------

    fn convert(&mut self, convert: Convert, span: Span) -> Vec<Token> {
        use Convert as C;
        let text: Option<String> = match convert {
            C::EtexRevision => Some(ETEX_REVISION.into()),
            C::PdftexRevision => Some(PDFTEX_REVISION.into()),
            C::XetexRevision => Some(XETEX_REVISION.into()),
            C::FontId => self.scan_font_ident().map(|f| f.to_string()),
            C::Xetex(crate::builtins::XeQuery::GlyphName) => self.xetex_glyph_name(),
            C::Xetex(query) => match self.xetex_query(query) {
                Some(Value::Toks(_)) => Some(String::new()),
                _ => None,
            },
            C::Uchar { catcode } => return self.uchar(catcode, span),
            C::FontName => self.scan_font_ident().and_then(|f| self.font_name_known(f)),
            C::PdfFontSize => self
                .scan_font_ident()
                .and_then(|f| self.font_size(f))
                .map(crate::value::render_dimen),
            C::PdfFontName | C::PdfFontObjNum => {
                self.scan_font_ident();
                None
            }
            C::StrCmp => {
                let a = self.expanded_text();
                let b = self.expanded_text();
                a.zip(b).map(|(a, b)| {
                    match a.cmp(&b) {
                        std::cmp::Ordering::Less => "-1",
                        std::cmp::Ordering::Equal => "0",
                        std::cmp::Ordering::Greater => "1",
                    }
                    .into()
                })
            }
            C::EscapeHex => {
                let text = self.expanded_text();
                text.map(|text| text.bytes().map(|b| format!("{b:02X}")).collect())
            }
            C::UnescapeHex => (|| {
                let text = self.expanded_text()?;
                let digits: Vec<u32> = text.chars().filter_map(|c| c.to_digit(16)).collect();
                Some(
                    digits
                        .chunks(2)
                        .map(|pair| char::from((pair[0] * 16 + pair.get(1).copied().unwrap_or(0)) as u8))
                        .collect(),
                )
            })(),
            C::EscapeString => (|| {
                let text = self.expanded_text()?;
                Some(text.bytes().map(|b| match b {
                    b'(' | b')' | b'\\' => format!("\\{}", b as char),
                    0x21..=0x7e => (b as char).to_string(),
                    _ => format!("\\{b:03o}"),
                }).collect())
            })(),
            C::EscapeName => (|| {
                let text = self.expanded_text()?;
                Some(text.bytes().filter(|b| *b != 0).map(|b| match b {
                    b'#' | b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%' => format!("#{b:02X}"),
                    0x21..=0x7e => (b as char).to_string(),
                    _ => format!("#{b:02X}"),
                }).collect())
            })(),
            C::FileSize | C::FileModDate => {
                // web2c drops the quotes a name may carry, and a file the
                // engine cannot find gives the empty string.
                let name = self.expanded_text().map(|name| name.replace('"', ""));
                let path = name.map(|name| self.locate_input(&name));
                match (convert, path) {
                    (_, None) => None,
                    (_, Some(None)) => Some(String::new()),
                    (C::FileSize, Some(Some(p))) => std::fs::metadata(p).ok().map(|m| m.len().to_string()),
                    _ => None,
                }
            }
            C::FileDump => {
                let offset = if self.scan_keyword("offset") { self.scan_int() } else { Some(0) };
                let length = if self.scan_keyword("length") { self.scan_int() } else { Some(0) };
                let name = self.expanded_text();
                let bytes = name.and_then(|name| self.locate_input(&name)).and_then(|p| std::fs::read(p).ok());
                match (bytes, offset, length) {
                    (Some(bytes), Some(offset), Some(length)) => {
                        let start = (offset.max(0) as usize).min(bytes.len());
                        let end = (start + length.max(0) as usize).min(bytes.len());
                        Some(bytes[start..end].iter().map(|b| format!("{b:02X}")).collect())
                    }
                    _ => None,
                }
            }
            C::MdFiveSum => {
                let file = self.scan_keyword("file");
                let text = self.expanded_text();
                let bytes = match text {
                    Some(text) if file => self.locate_input(&text).and_then(|p| std::fs::read(p).ok()),
                    Some(text) => Some(text.into_bytes()),
                    None => None,
                };
                bytes.map(|b| <md5::Md5 as md5::Digest>::digest(&b).iter().map(|x| format!("{x:02X}")).collect::<String>())
            }
            C::Match => {
                self.scan_keyword("icase");
                if self.scan_keyword("subcount") {
                    self.scan_int();
                }
                self.expanded_text();
                self.expanded_text();
                None
            }
            C::LastMatch | C::PageRef | C::XFormName | C::InsertHt | C::MarginKern | C::UniformDeviate | C::Marks => {
                self.scan_int();
                None
            }
            C::ImageBBox => {
                self.scan_int();
                self.scan_int();
                None
            }
            C::NormalDeviate | C::CreationDate | C::PdftexBanner | C::Mark => None,
            // The pdfTeX manual: stacks are numbered from 1, 0 being the
            // default colour stack.
            C::ColorStackInit => {
                self.scan_keyword("page");
                self.scan_keyword("direct");
                self.read_general_text();
                let next = self.engine_int("colorstacks").unwrap_or(0) + 1;
                self.set_engine("colorstacks", Value::Int(next), true);
                Some(next.to_string())
            }
            C::Primitive => {
                self.primitive_meaning(span);
                return Vec::new();
            }
        };
        // What the engine gives and satex cannot know is unknown text.
        match text {
            Some(t) => chars(&t, span),
            None => vec![self.unknown_token(span)],
        }
    }

    /// XeTeX's `\Uchar` and `\Ucharcat`: a character token made from its
    /// code and category, 1-4, 6-8 and 10-12 (13 as well in LuaTeX).
    fn uchar(&mut self, catcode: bool, span: Span) -> Vec<Token> {
        let code = self.scan_int();
        let cat = if catcode { self.scan_int() } else { Some(if code == Some(32) { 10 } else { 12 }) };
        let c = code.and_then(|c| u32::try_from(c).ok()).and_then(char::from_u32);
        let cat = cat.and_then(|k| u8::try_from(k).ok()).and_then(Catcode::from_u8);
        match (c, cat) {
            (Some(c), Some(cat)) if !matches!(cat, Catcode::Escape | Catcode::Eol | Catcode::Ignored | Catcode::Active | Catcode::Comment | Catcode::Invalid) => {
                vec![Token::new(Tok::Chr(c, cat), span)]
            }
            _ => vec![self.unknown_token(span)],
        }
    }

    /// ⟨class⟩⟨family⟩⟨slot⟩, packed as the engine packs them.
    fn scan_umath_char(&mut self) -> Option<i64> {
        let (class, family, slot) = (self.scan_int(), self.scan_int(), self.scan_int());
        Some(family? << UMATH_FAMILY_SHIFT | class? << UMATH_CLASS_SHIFT | slot?)
    }

    pub(crate) fn locate_input(&mut self, name: &str) -> Option<std::path::PathBuf> {
        let base = self.base().to_path_buf();
        self.resolver_mut().resolve(name, crate::builtins::LoadKind::Input, &base)
    }

    /// `\pdfprimitive⟨cs⟩`: the primitive of that name, whatever the name
    /// means now — expanded if it is expandable, otherwise put back as a
    /// token that means it.
    fn primitive_meaning(&mut self, span: Span) {
        let Some(token) = self.next_token() else { return };
        let Some(sym) = token.cs() else { return };
        let name = self.name(sym).to_string();
        let Some(p) = self.engine_primitive(&name) else { return };
        let frozen = self.intern(&format!("{ENGINE}primitive.{name}"));
        self.env.set(frozen, crate::env::Binding::builtin(Meaning::Primitive(p)), true);
        self.unread(Token::new(Tok::Cs(frozen), span));
    }

    fn engine_primitive(&mut self, name: &str) -> Option<Primitive> {
        let engine = self.out.plugins.engine;
        let mut interner = crate::tex::Interner::default();
        let table = crate::builtins::initial_meanings(&mut interner, engine);
        interner.lookup(name).and_then(|sym| table.get(&sym)).and_then(Meaning::prim)
    }

    // ---- conditionals ---------------------------------------------------------

    /// The conditionals this module adds (see [`Cond`]).
    pub fn eval_extra_condition(&mut self, cond: Cond) -> Option<bool> {
        match cond {
            Cond::AbsNum => {
                let a = self.scan_int();
                let relation = self.scan_relation_x();
                let b = self.scan_int();
                crate::scan::compare_abs(a, relation, b)
            }
            Cond::AbsDim => {
                let a = self.scan_dimen();
                let relation = self.scan_relation_x();
                let b = self.scan_dimen();
                crate::scan::compare_abs(a, relation, b)
            }
            Cond::FontChar => {
                let font = self.scan_font_ident();
                let c = self.scan_int();
                let (font, c) = (font?, u32::try_from(c?).ok()?);
                if font == 0 {
                    return Some(false);
                }
                // xetex.web `\iffontchar`: a native font has the character
                // when its map gives a glyph other than `.notdef`.
                if let Some(map) = self.native_charmap(font) {
                    return Some(map.glyphs.contains_key(&c));
                }
                self.font_metrics(font).map(|m| m.has_char(c))
            }
            Cond::InCsname => Some(IN_CSNAME.with(Cell::get) > 0),
            Cond::Primitive => {
                let token = self.next_token()?;
                let sym = token.cs()?;
                let name = self.name(sym).to_string();
                let meaning = self.env.meaning(sym);
                Some(self.engine_primitive(&name).is_some_and(|p| meaning == Meaning::Primitive(p)))
            }
            _ => None,
        }
    }

    /// tex.web § 503: the relation of `\ifnum`/`\ifdim`, a character of
    /// category 12 after expansion and blanks.
    pub fn scan_relation_x(&mut self) -> Option<char> {
        let token = self.next_non_blank_x()?;
        match token.tok {
            Tok::Chr(c @ ('<' | '=' | '>'), Catcode::Other) => Some(c),
            _ => {
                self.unread(token);
                None
            }
        }
    }

    /// `\the` of a font-valued quantity: the font identifier (tex.web § 465).
    pub fn font_identifier(&mut self, p: Primitive, sym: Sym, span: Span) -> Option<Vec<Token>> {
        let font = match p {
            Primitive::FontDef => self.current_font()?,
            Primitive::FontIdent(font) => font,
            Primitive::Command(Syntax::FamilyFont) => {
                let family = self.scan_int()?;
                self.family_font(sym, family)?
            }
            _ => return None,
        };
        let cs = if font == 0 {
            self.intern("nullfont")
        } else {
            let index = self.font_field(font, "cs").as_int()?;
            Sym(u32::try_from(index).ok()?)
        };
        Some(vec![Token::new(Tok::Cs(cs), span)])
    }

}
