//! The engine primitives beyond the interpreter's core: glue and token-list
//! parameters, engine-computed quantities (`last_item`), font tables, and
//! commands whose effect is typesetting, output or diagnostics.
//! Each reads exactly the operands the engine reads (tex.web parts 30 and
//! 49, etex_man § 3, the pdfTeX manual) and answers with the engine's value
//! wherever that does not depend on typesetting.

use std::cell::Cell;
use std::rc::Rc;

use crate::builtins::DefMode;
use crate::builtins::{CodeTable, Cond, Convert, FontParam, LastItem, Primitive, Special, Syntax};
use crate::env::GroupKind;
use crate::machine::{CondLimit, Machine, Unk};
use crate::mode::group;
use crate::tex::{Catcode, Meaning, RegKind, Span, Sym, Tok, Token};
use crate::value::{Scaled, UNIT, Value};

/// Keeps font tables and other engine state out of the control-sequence namespace.
const ENGINE: char = '\u{4}';

/// `ENGINE` and `font`: the slot `\the\font` reads. A constant so reading it per character costs no formatting.
const FONT_SLOT: &str = "\u{4}font";

/// The five macros LaTeX keeps the current font in (fntguide § 2).
pub(crate) const NFSS: [&str; 5] = ["cf@encoding", "f@family", "f@series", "f@shape", "f@size"];
/// etex_man § 3: `\eTeXversion` and `\eTeXrevision`.
const ETEX_VERSION: i64 = 2;
const ETEX_REVISION: &str = ".6";
/// The pdfTeX manual: `\pdftexversion` is 140 for 1.40, and
/// `\pdftexrevision` the last part of the version of the TeX Live binary.
/// Read from the installed `pdftex` where there is one, these where not.
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

/// `\pdftexversion` and `\pdftexrevision` of the installed engine: version
/// 1.40.28 is 140 and "28" (the pdfTeX manual).
fn pdftex_release() -> (i64, String) {
    let parsed = crate::distribution::engine_release("pdftex").and_then(|v| {
        let mut parts = v.split('.');
        let (major, minor, revision) = (parts.next()?, parts.next()?, parts.next()?);
        let version = major.parse::<i64>().ok()? * 100 + minor.parse::<i64>().ok()?;
        revision.parse::<u32>().ok()?;
        Some((version, revision.to_string()))
    });
    parsed.unwrap_or((PDFTEX_VERSION, PDFTEX_REVISION.to_string()))
}

/// `\XeTeXversion` and `\XeTeXrevision` of the installed engine: version
/// 0.999997 is 0 and ".999997" (the XeTeX reference).
fn xetex_release() -> (i64, String) {
    let parsed = crate::distribution::engine_release("xetex").and_then(|v| {
        let (major, revision) = v.split_once('.')?;
        revision.parse::<u32>().ok()?;
        Some((major.parse::<i64>().ok()?, format!(".{revision}")))
    });
    parsed.unwrap_or((XETEX_VERSION, XETEX_REVISION.to_string()))
}

thread_local! {
    /// pdfTeX's `\ifincsname`: how many `\csname`/`\ifcsname` scans are open. Nest only while running, so a counter is exact.
    static IN_CSNAME: Cell<usize> = const { Cell::new(0) };
    /// Conditionals reading their test, already on the condition stack (tex.web § 495).
    static TESTING: Cell<usize> = const { Cell::new(0) };
}

/// Starts a `\csname`/`\ifcsname` scan. [`leave_csname`] ends it.
pub fn enter_csname() -> usize {
    IN_CSNAME.with(|depth| depth.replace(depth.get() + 1))
}

pub fn leave_csname(depth: usize) {
    IN_CSNAME.with(|d| d.set(depth));
}

/// A conditional's test is being read. See `Machine::eval_condition`.
pub fn enter_test() -> usize {
    TESTING.with(|depth| depth.replace(depth.get() + 1))
}

pub fn leave_test(depth: usize) {
    TESTING.with(|d| d.set(depth));
}

/// The engine keys `\pdfmatch` leaves for `\pdflastmatch`: the groups
/// kept and each group's `⟨position⟩->⟨text⟩`.
const LAST_MATCH: &str = "lastmatch.";
const LAST_MATCH_COUNT: &str = "lastmatch.count";
/// What `\pdflastmatch` is for a group that did not take part.
const NO_MATCH: &str = "-1->";

fn chars(text: &str, span: Span) -> Vec<Token> {
    text.chars()
        .map(|c| Token::new(Tok::Chr(c, if c == ' ' { Catcode::Space } else { Catcode::Other }), span))
        .collect()
}

/// The family of font table entries one write may reach: a font's
/// `\fontdimen`s, its `\lpcode`s, or one `\hyphenchar`/`\skewchar` entry.
fn font_field_family(font: u32, field: &str) -> String {
    if let Some(rest) = field.strip_prefix('p')
        && !rest.is_empty()
        && rest.bytes().all(|b| b.is_ascii_digit())
    {
        return format!("font{font}.p");
    }
    match field.split_once('.') {
        Some((name, _)) => format!("font{font}.{name}."),
        None => format!("font{font}.{field}"),
    }
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
    pub(crate) fn engine_sym(&mut self, key: &str) -> Sym {
        self.intern(&format!("{ENGINE}{key}"))
    }

    pub(crate) fn engine_int(&mut self, key: &str) -> Option<i64> {
        let sym = self.engine_sym(key);
        self.env.value(sym).as_int()
    }

    /// `\pdfrefximage⟨n⟩`: a box of the image's size, depth zero, where
    /// the size is known.
    fn ref_ximage(&mut self, n: Option<i64>) {
        let dim = |m: &mut Self, k: &str| Some(crate::mode::Dim::exact(m.engine_int(&format!("ximage.{}.{k}", n?))?));
        let item = match (dim(self, "w"), dim(self, "h")) {
            (Some(w), Some(h)) => crate::mode::Item::Box([w, h, crate::mode::Dim::exact(0)]),
            _ => crate::mode::Item::Unknown,
        };
        self.append_item(item);
    }

    pub(crate) fn set_engine(&mut self, key: &str, value: Value, global: bool) {
        let sym = self.engine_sym(key);
        self.env.set_value(sym, value, global);
    }

    /// The slot standing for a whole family of engine keys, e.g. every
    /// `charclass.⟨n⟩`. A real key never holds an `ENGINE` character, so this is none of them.
    fn unpinned_sym(&mut self, family: &str) -> Sym {
        self.engine_sym(&format!("{family}{ENGINE}any"))
    }

    /// A write through a number satex could not pin down: any key of the
    /// family may hold it, and an unwritten one is no longer at its default.
    fn weaken_engine(&mut self, family: &str, global: bool) {
        let prefix = format!("{ENGINE}{family}");
        let targets: Vec<Sym> = self.env.bound().into_iter().filter(|&s| self.name(s).starts_with(&prefix)).collect();
        for sym in targets {
            self.env.set_value(sym, Value::Unknown, global);
        }
        let any = self.unpinned_sym(family);
        self.env.set_value(any, Value::Unknown, global);
    }

    /// Whether such a write may have reached this family.
    fn engine_unpinned(&mut self, family: &str) -> bool {
        let sym = self.unpinned_sym(family);
        self.env.get(sym).is_some()
    }

    /// An engine integer, 0 for a never-set key.
    /// `None` if the key, or an unpinned write, left it unknown.
    pub(crate) fn engine_slot_int(&mut self, family: &str, key: &str) -> Option<i64> {
        if self.engine_unpinned(family) {
            return None;
        }
        let sym = self.engine_sym(key);
        match self.env.get(sym) {
            None => Some(0),
            Some(_) => self.env.value(sym).as_int(),
        }
    }

    /// Whether `\openout` left the stream open (tex.web § 1374).
    /// `None` if an unpinned number opened or closed one.
    pub(crate) fn write_open(&mut self, stream: i64) -> Option<bool> {
        self.engine_slot_int("write_open.", &format!("write_open.{stream}")).map(|n| n == 1)
    }

    /// `\globaldefs` (tex.web § 1214): positive makes every assignment
    /// global, negative every one local, `\global` and `\gdef` included.
    pub fn global_defs(&mut self) -> i64 {
        let sym = self.intern("globaldefs");
        self.env.value(sym).as_int().unwrap_or(0)
    }

    /// `None` if `\globaldefs` holds an unknown number, so it is not read
    /// back as the 0 that leaves the prefix alone.
    fn global_defs_known(&mut self) -> Option<i64> {
        let sym = self.intern("globaldefs");
        match self.env.get(sym) {
            None => Some(0),
            Some(_) => self.env.value(sym).as_int(),
        }
    }

    /// Whether the scope of the assignment that follows is unknown
    /// (tex.web § 1214: positive `\globaldefs` makes it global, negative
    /// local). Then made global and its write widened, since the name may
    /// still hold what it held before.
    pub fn apply_global_defs(&mut self) -> bool {
        match self.global_defs_known() {
            Some(0) => false,
            Some(n) => {
                self.prefixes.global = n > 0;
                false
            }
            None => {
                self.prefixes.global = true;
                true
            }
        }
    }

    pub(crate) fn take_global(&mut self) -> bool {
        std::mem::take(&mut self.prefixes).global
    }

    /// `scan_optional_equals` (tex.web § 405).
    pub fn scan_optional_equals(&mut self) {
        if let Some(token) = self.next_non_blank_x_token()
            && !matches!(token.tok, Tok::Chr('=', Catcode::Other))
        {
            self.probe_scans(token, "=");
            self.unread(token);
        }
    }

    /// tex.web §§ 1101, 1012-1014: a mark the page builder or `\vsplit`
    /// hands back is a copy of one put on a list, or empty: one of the
    /// texts of its class so far.
    fn note_mark(&mut self, class: Option<i64>, text: Vec<Token>, span: Span) {
        let sym = self.marks_sym(class);
        let mut texts: Vec<Rc<[Token]>> = vec![Rc::from(Vec::new()), Rc::from(text)];
        if let Some(Meaning::Macro(m)) = self.env.slot(sym).map(|b| b.meaning.clone()) {
            texts.push(m.replacement_text.clone());
        }
        let token = self.one_of(texts, span);
        let meaning = self.make_macro(Default::default(), vec![token]);
        self.env.set(sym, crate::env::Binding::builtin(meaning), true);
    }

    /// The text a mark of `class` reads: empty before any is put.
    fn marks_of(&mut self, class: Option<i64>, span: Span) -> Vec<Token> {
        let any = self.marks_sym(None);
        if class.is_none() || self.env.slot(any).is_some() {
            return vec![self.unknown_text(Unk::Any, span)];
        }
        let sym = self.marks_sym(class);
        match self.env.slot(sym).map(|b| b.meaning.clone()) {
            Some(Meaning::Macro(m)) => m.replacement_text.to_vec(),
            _ => Vec::new(),
        }
    }

    /// Where the texts of mark class `class` are kept; `None`, a class not
    /// known, which every class may then hold.
    fn marks_sym(&mut self, class: Option<i64>) -> Sym {
        match class {
            Some(n) => self.intern(&format!("\u{4}marks.{n}")),
            None => self.intern("\u{4}marks.any"),
        }
    }

    /// A ⟨general text⟩, expanded as an `\edef` body is (tex.web § 473, `scan_toks(false, true)`).
    fn expanded_general_text(&mut self) -> Vec<Token> {
        let body = self.read_general_text();
        self.expand_tokens(Rc::from(body))
    }

    /// The text of an expanded general text, `None` when it is unknown.
    fn expanded_text(&mut self) -> Option<String> {
        let body = self.expanded_general_text();
        self.text_known(&body)
    }

    /// What is known of the start of a text: the characters before its
    /// first unknown token, and whether that is all of it.
    pub(crate) fn known_prefix(&mut self, tokens: &[Token]) -> (String, bool) {
        let known = tokens.iter().take_while(|t| !self.has_unknown(std::slice::from_ref(t))).count();
        match self.text_known(&tokens[..known]) {
            Some(text) => (text, known == tokens.len()),
            None => (String::new(), false),
        }
    }

    /// Tokens as `\detokenize` writes them. `None` if unknown, or a
    /// control sequence under an unknown `\escapechar`.
    pub(crate) fn text_known(&mut self, tokens: &[Token]) -> Option<String> {
        if self.has_unknown(tokens) {
            return None;
        }
        let escape = match self.escape_state() {
            Some(escape) => escape,
            None if tokens.iter().any(|t| t.cs().is_some()) => return None,
            None => None,
        };
        Some(crate::tex::detokenize_in(
            tokens,
            &self.out.interner,
            escape,
            &self.catcodes,
            crate::tex::Selector::NewString,
        ))
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

    /// The value of one of this module's internal quantities, its operands
    /// read here. `None` if the engine's answer depends on typesetting.
    pub fn extra_quantity(&mut self, p: Primitive, sym: Sym) -> Option<Value> {
        match p {
            Primitive::GlueParameter { .. } | Primitive::TokensParameter => {
                Some(self.env.value(self.env.identity(sym)))
            }
            // pdfTeX's last object, annotation, link, form and image
            // numbers index its object table: at least 0 and, like every
            // index the engine keeps, under `\maxdimen`, so an expl3 store
            // of one (`\intarray_gset:Nnn`) does not overflow.
            Primitive::LastItem(_)
                if self.name(self.env.identity(sym)) == "pdflastximage"
                    && let Some(n) = self.engine_slot_int("ximages", "ximages") =>
            {
                Some(Value::Int(n))
            }
            Primitive::LastItem(_)
                if matches!(
                    self.name(self.env.identity(sym)),
                    "pdflastobj"
                        | "pdflastannot"
                        | "pdflastlink"
                        | "pdflastxform"
                        | "pdflastximage"
                        | "pdflastximagepages"
                ) =>
            {
                Some(Value::Num {
                    quantity: crate::value::Quantity::Int,
                    num: crate::value::Num::range(0, crate::value::MAX_DIMEN),
                })
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
            // etex.ch: 0 a character, 7 a ligature, 11 glue, 12 a kern, 13 a
            // penalty, read off the tail of the list.
            L::NodeType => {
                let n = self.natural?;
                // A word being set ends in its character (tex.web § 1034).
                let (tail, ligature) = match n.pending {
                    Some(_) => (crate::mode::Last::Char(None), None),
                    None => (n.tail[0], n.ligature),
                };
                match tail {
                    // Not known which: 0 or 7, never glue, a kern or a penalty.
                    crate::mode::Last::Char(_) => Some(ligature.map_or(
                        Value::Num { quantity: crate::value::Quantity::Int, num: crate::value::Num::range(0, 7) },
                        |lig| Value::Int(if lig { 7 } else { 0 }),
                    )),
                    crate::mode::Last::Glue(..) => int(11),
                    crate::mode::Last::Kern(_) => int(12),
                    crate::mode::Last::Penalty(_) => int(13),
                    // -1 an empty list, 1 an hlist.
                    crate::mode::Last::Empty => int(-1),
                    crate::mode::Last::Box => int(1),
                    crate::mode::Last::Whatsit => int(9),
                    // A box, rule, insertion, mark, adjust, discretionary or
                    // math node, and in math a noad (etex.ch: 15).
                    crate::mode::Last::Other => {
                        let hi = if self.mode.meets(crate::mode::Modes::ANY_MATH) { 15 } else { 10 };
                        Some(Value::Num { quantity: crate::value::Quantity::Int, num: crate::value::Num::range(1, hi) })
                    }
                    crate::mode::Last::Unknown(_) => None,
                }
            }
            L::Badness => {
                let sym = self.intern("badness");
                match self.env.value(sym) {
                    Value::Int(b) => int(b),
                    _ => None,
                }
            }
            L::InputLineNo => self.input_line().map(|line| Value::Int(line.into())),
            L::EtexVersion => int(ETEX_VERSION),
            L::PdftexVersion => int(pdftex_release().0),
            // A group standing for any number of groups: at least the rest.
            L::GroupLevel => match self.env.many_groups() {
                false => int(self.env.group_level() as i64),
                true => Some(Value::Num {
                    quantity: crate::value::Quantity::Int,
                    num: crate::value::Num::range(self.env.group_level() as i64, i32::MAX.into()),
                }),
            },
            L::GroupType => match (self.env.group_kind(), self.env.nest()) {
                (None, _) => int(0),
                (Some(_), Some(nest)) => nest.code.map(|code| Value::Int(code.into())),
                (Some(GroupKind::Simple), None) => int(group::SIMPLE.into()),
                (Some(GroupKind::SemiSimple), None) => int(group::SEMI_SIMPLE.into()),
                (Some(GroupKind::Math { .. }), None) => int(group::MATH_SHIFT.into()),
            },
            // A conditional standing for any number of them: at least the rest.
            L::IfLevel => {
                let level = (self.conds.iter().filter(|c| !c.many).count() + TESTING.with(Cell::get)) as i64;
                match self.conds.iter().any(|c| c.many) {
                    false => int(level),
                    true => Some(Value::Num {
                        quantity: crate::value::Quantity::Int,
                        num: crate::value::Num::range(level, i32::MAX.into()),
                    }),
                }
            }
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
                let Some((kind, limit)) = self.conds.last().map(|frame| (frame.kind, frame.limit)) else {
                    return int(0);
                };
                match item {
                    L::IfType => kind.map(|k| Value::Int(k.into())),
                    _ => int(match limit {
                        CondLimit::Else | CondLimit::Or => 1,
                        CondLimit::Fi => -1,
                    }),
                }
            }
            L::GlueStretch | L::GlueShrink | L::GlueStretchOrder | L::GlueShrinkOrder => {
                let glue = self.scan_glue_spec(false)?;
                use crate::value::Quantity;
                Some(match item {
                    L::GlueStretch => Value::from_num(Quantity::Dimen, glue.stretch),
                    L::GlueShrink => Value::from_num(Quantity::Dimen, glue.shrink),
                    L::GlueStretchOrder => Value::from_num(Quantity::Int, glue.stretch_order),
                    _ => Value::from_num(Quantity::Int, glue.shrink_order),
                })
            }
            L::GlueToMu => self.scan_glue_spec(false).map(|g| g.value(true)),
            L::MuToGlue => self.scan_glue_spec(true).map(|g| g.value(false)),
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
            L::ShellEscape => int(match self.out.plugins.shell_escape {
                crate::config::ShellEscape::None => 0,
                crate::config::ShellEscape::Full => 1,
                crate::config::ShellEscape::Restricted => 2,
            }),
            // pdfTeX's clock: scaled seconds since the run began, stopping
            // at 2^31-1, so never negative. Its interval is left to the
            // test or assignment that reads it, as a register's is.
            L::ElapsedTime => Some(Value::Num {
                quantity: crate::value::Quantity::Int,
                num: crate::value::Num::range(0, crate::value::MAX_INT),
            }),
            // The random generator and the output position.
            L::RandomSeed | L::LastXPos | L::LastYPos => None,
            L::XetexVersion => int(xetex_release().0),
            // Unlike the other engines' version primitives, LuaTeX's varies
            // meaningfully across installs, so it is read from the real
            // `luatex --version` for this run rather than a constant.
            L::LuatexVersion => crate::plugin::lua::engine_version(self.cfg).map(Value::Int),
            L::PdfPageCount => {
                self.read_file_name();
                None
            }
            L::Xetex(query) => self.xetex_query(query),
        }
    }

    /// XeTeX's font queries. A TFM font answers without reading further
    /// operands (xetex.web, `not_native_font_error`). An installed font
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
            // tex.web § 1248: `\parshape` reads back the number of lines it
            // was given, 0 until it is given one.
            Special::ParShape => Some(self.engine_slot_int("parshape", "parshape").map_or(Value::Unknown, Value::Int)),
            Special::UCode { math, num } => {
                let c = self.scan_int().and_then(|c| u32::try_from(c).ok()).and_then(char::from_u32)?;
                // "Can't use \Umathcode as a number (try \Umathcodenum)".
                if !num {
                    return Some(Value::Int(0));
                }
                let table = if math { CodeTable::Math } else { CodeTable::Delimiter };
                Some(self.char_code_known(table, c).map_or(Value::Unknown, Value::Int))
            }
            Special::CharClass => {
                let c = self.scan_int()?;
                let class = self.engine_slot_int("charclass.", &format!("charclass.{c}"));
                Some(class.map_or(Value::Unknown, Value::Int))
            }
            Special::InterCharToks => {
                let (a, b) = (self.scan_int(), self.scan_int());
                let (a, b) = (a?, b?);
                if self.engine_unpinned("interchartoks.") {
                    return Some(Value::Unknown);
                }
                let sym = self.engine_sym(&format!("interchartoks.{a}.{b}"));
                match self.env.value(sym) {
                    Value::Unknown if self.env.get(sym).is_none() => Some(Value::Toks(Rc::from(Vec::new()))),
                    value => Some(value),
                }
            }
            Special::Penalties => {
                let n = self.scan_int()?;
                let name = self.name(by).to_string();
                let family = format!("{name}.");
                let Some(count) = self.engine_slot_int(&family, &format!("{name}.count")) else {
                    return Some(Value::Unknown);
                };
                if n <= 0 {
                    return Some(Value::Int(if n == 0 { count } else { 0 }));
                }
                if count == 0 {
                    return Some(Value::Int(0));
                }
                let k = n.min(count);
                let value = self.engine_slot_int(&family, &format!("{name}.{k}"));
                Some(value.map_or(Value::Unknown, Value::Int))
            }
        }
    }

    fn assign_special(&mut self, special: Special, by: Sym, span: Span) {
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
                let scanned = self.scan_int();
                let global = self.take_global();
                // tex.web § 1248: the count says how many ⟨dimen⟩ pairs
                // follow. An unpinned count leaves both the shape and what
                // to read for it unknown.
                let Some(lines) = scanned.map(|n| n.max(0)) else {
                    self.weaken_engine("parshape", global);
                    return self.diagnose(
                        crate::facts::Severity::Unsupported,
                        "unknown-parshape",
                        span,
                        "\\parshape was given a number satex could not pin down; its lines were not read".into(),
                    );
                };
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
                let named = self.env.identity(by);
                let limit = crate::builtins::limit(self.name(named), self.out.plugins.engine);
                match c {
                    Some(c) => {
                        let sym = self.char_code_sym(table, c);
                        let stored = match limit {
                            Some(limit) => self.limited(named, sym, limit, code, span),
                            None => Some(code.map_or(Value::Unknown, Value::Int)),
                        };
                        if let Some(stored) = stored {
                            self.env.set_value(sym, stored, global);
                        }
                    }
                    // Any character may be the one assigned.
                    None => {
                        // A checked code may be stored as something else.
                        let value =
                            if limit.is_some() { Value::Unknown } else { code.map_or(Value::Unknown, Value::Int) };
                        self.weaken_code_table(table, None, &value, global)
                    }
                }
            }
            Special::CharClass => {
                let c = self.scan_int();
                self.scan_optional_equals();
                let class = self.scan_int().map_or(Value::Unknown, Value::Int);
                let global = self.take_global();
                match c {
                    Some(c) => self.set_engine(&format!("charclass.{c}"), class, global),
                    // Any character's class may be the one assigned.
                    None => self.weaken_engine("charclass.", global),
                }
            }
            Special::InterCharToks => {
                let (a, b) = (self.scan_int(), self.scan_int());
                self.scan_optional_equals();
                let toks = self.read_general_text();
                let global = self.take_global();
                match (a, b) {
                    (Some(a), Some(b)) => {
                        self.set_engine(&format!("interchartoks.{a}.{b}"), Value::Toks(Rc::from(toks)), global)
                    }
                    // Any pair of classes may be the one assigned.
                    _ => self.weaken_engine("interchartoks.", global),
                }
            }
            // etex_man § 3.12.
            Special::Penalties => {
                self.scan_optional_equals();
                let scanned = self.scan_int();
                let global = self.take_global();
                let name = self.name(by).to_string();
                // etex_man § 3.12: the count says how many penalties follow.
                let Some(count) = scanned.map(|n| n.max(0)) else {
                    self.weaken_engine(&format!("{name}."), global);
                    return self.diagnose(
                        crate::facts::Severity::Unsupported,
                        "unknown-penalties",
                        span,
                        format!("\\{name} was given a number satex could not pin down; its penalties were not read"),
                    );
                };
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

    pub(crate) fn font_field(&mut self, font: u32, field: &str) -> Value {
        let sym = self.font_key(font, field);
        self.env.value(sym)
    }

    /// Whether the entry was assigned at all. An entry set to an unknown
    /// value, or reachable by an unpinned write, is no longer the font
    /// file's value.
    fn font_field_set(&mut self, font: u32, field: &str) -> bool {
        if self.engine_unpinned(&font_field_family(font, field)) {
            return true;
        }
        let sym = self.font_key(font, field);
        self.env.get(sym).is_some()
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
        self.register_file(path.display().to_string(), crate::builtins::LoadKind::Font);
        crate::otf::load(&path)
    }

    fn find_tfm(&mut self, name: &str) -> Option<Rc<crate::tfm::Metrics>> {
        let base = self.base().to_path_buf();
        let roots = self.resolver_mut().distribution().roots.clone();
        let path = crate::tfm::locate(&base, &roots, name, "tfm")?;
        // A file the run reads, as `satex query files` lists it.
        self.register_file(path.display().to_string(), crate::builtins::LoadKind::Font);
        crate::tfm::load(&path)
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
        let token = self.next_non_blank_x_token()?;
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
            // A name of unknown meaning may be a font identifier: which
            // font it selects is not known.
            Meaning::Unknown => {
                self.unread(token);
                None
            }
            _ => {
                // "Missing font identifier": the token goes back and the
                // null font is used.
                self.unread(token);
                Some(0)
            }
        }
    }

    /// tex.web § 232: `\nullfont` until a font is selected; `None` where a
    /// selection satex could not follow left the current font unknown.
    pub(crate) fn current_font(&mut self) -> Option<u32> {
        u32::try_from(self.engine_slot_int("font", "font")?).ok()
    }

    /// Whether the current font is known to be `\nullfont`: no character
    /// set in it shows.
    pub(crate) fn in_nullfont(&mut self) -> bool {
        let Some(sym) = self.out.interner.lookup(FONT_SLOT) else { return false };
        self.env.slot_value(sym) == Some(0)
    }

    /// The font id for [`crate::facts::FontState`]. Read with
    /// `Env::slot_value` so observing adds no read a package cache would depend on.
    fn observed_font(&mut self) -> u16 {
        let Some(sym) = self.out.interner.lookup(FONT_SLOT) else { return 0 };
        self.env.slot_value(sym).and_then(|f| u16::try_from(f).ok()).unwrap_or(0)
    }

    /// The body of a macro, without recording a read.
    fn nfss_body(&self, name: Option<Sym>) -> Option<&Rc<[Token]>> {
        match &self.env.slot(name?)?.meaning {
            Meaning::Macro(m) => Some(&m.replacement_text),
            _ => None,
        }
    }

    /// Where that body lives, which is all the per-character check needs.
    fn nfss_at(&self, name: Option<Sym>) -> Option<*const u8> {
        self.nfss_body(name).map(|body| Rc::as_ptr(body).cast())
    }

    /// Which row of [`crate::machine::Analysis::font_states`] the character
    /// about to be set belongs to. Called once per character; the five
    /// NFSS macros are re-resolved only when a body's token list changed.
    pub(crate) fn observed_font_state(&mut self) -> u16 {
        if self.font_watch.names.is_none() {
            self.font_watch.names = Some(NFSS.map(|name| self.out.interner.lookup(name)));
        }
        let names = self.font_watch.names.unwrap_or([None; 5]);
        let at = names.map(|name| self.nfss_at(name));
        if at == self.font_watch.at {
            return self.font_watch.state;
        }
        let bodies = names.map(|name| self.nfss_body(name).cloned());
        let text = |body: &Option<Rc<[Token]>>| match body {
            Some(body) => Rc::from(crate::tex::text_of(body, &self.out.interner)),
            None => Rc::from(""),
        };
        let state = crate::facts::FontState {
            encoding: text(&bodies[0]),
            family: text(&bodies[1]),
            series: text(&bodies[2]),
            shape: text(&bodies[3]),
            size: text(&bodies[4]),
            font: self.observed_font(),
        };
        let states = &mut self.out.font_states;
        let row = match states.iter().position(|kept| *kept == state) {
            Some(row) => row,
            None if states.len() < usize::from(u16::MAX) => {
                states.push(state);
                states.len() - 1
            }
            // Too many states to fit: left at "not known" rather than
            // pointed at the wrong row.
            None => 0,
        };
        self.font_watch.bodies = bodies;
        self.font_watch.at = at;
        self.font_watch.state = row as u16;
        self.font_watch.state
    }

    /// The fonts the run loaded, by the number `\the\font` gives them, with
    /// TFM name and load size (tex.web § 1257). A native XeTeX font has no
    /// TFM and is left out.
    pub(crate) fn collect_fonts(&mut self) {
        let count = self.font_count();
        self.out.fonts = (0..=count)
            .map(|id| {
                let native = id > 0 && self.font_native(id) != Some(false);
                match native {
                    true => None,
                    false => Some(crate::machine::FontUse { name: self.font_name(id)?, size: self.font_size(id)? }),
                }
            })
            .collect();
    }

    /// tex.web § 232: every math family starts at `\nullfont`.
    pub(crate) fn family_font(&mut self, by: Sym, family: i64) -> Option<u32> {
        let name = self.name(by).to_string();
        u32::try_from(self.engine_slot_int(&format!("{name}."), &format!("{name}.{family}"))?).ok()
    }

    /// `\font⟨cs⟩=⟨file name⟩⟨at clause⟩` (tex.web §§ 1257-1260). A font
    /// already loaded at that size is shared, one that cannot load leaves
    /// `\nullfont`, else it loads anew.
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
        self.define(name_cs, Meaning::Primitive(Primitive::FontIdent(font)), by, DefMode::Declare, span);
    }

    /// Where the engine finds the font (tex.web § 563; xetex.web
    /// `load_native_font`). A quoted name is tried as an installed font
    /// first. Unquoted, only after no TFM file has it.
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
    /// else by name through fontconfig. `None` if fontconfig can't be asked.
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

    /// Which font-table entry `\fontdimen⟨n⟩⟨font⟩`, `\hyphenchar⟨font⟩`,
    /// `\skewchar⟨font⟩` or `\lpcode⟨font⟩⟨char⟩` names.
    fn font_entry(&mut self, param: FontParam, by: Sym) -> Option<(u32, String)> {
        match param {
            FontParam::Dimen => {
                let n = self.scan_int();
                let font = self.scan_font_ident()?;
                Some((font, format!("p{}", n?)))
            }
            FontParam::HyphenChar => Some((self.scan_font_ident()?, "hyphenchar".into())),
            FontParam::SkewChar => Some((self.scan_font_ident()?, "skewchar".into())),
            FontParam::CharCode(_) | FontParam::TagCode { .. } => {
                let font = self.scan_font_ident();
                let c = self.scan_int();
                // A `\let` copy writes the table of the primitive it copies.
                let by = self.env.identity(by);
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

    pub(crate) fn font_dimen(&mut self, font: u32, k: usize) -> Option<Value> {
        let stored = self.font_field(font, &format!("p{k}"));
        if !matches!(stored, Value::Unknown) {
            return Some(stored);
        }
        // An assignment satex could not follow put the font file's own
        // parameter out of reach (tex.web § 580).
        if self.font_field_set(font, &format!("p{k}")) {
            return Some(Value::Unknown);
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
        self.font_entry_value(param, font, &field)
    }

    /// `\lpcode⟨font⟩⟨c⟩`.
    pub(crate) fn lp_code(&mut self, font: u32, c: u32) -> Option<i64> {
        self.font_entry_value(FontParam::CharCode(0), font, &format!("lpcode.{c}"))?.as_int()
    }

    fn font_entry_value(&mut self, param: FontParam, font: u32, field: &str) -> Option<Value> {
        let stored = self.font_field(font, field);
        if !matches!(stored, Value::Unknown) {
            return Some(stored);
        }
        if self.font_field_set(font, field) {
            return Some(Value::Unknown);
        }
        match param {
            FontParam::Dimen => {
                let k: usize = field[1..].parse().ok()?;
                self.font_dimen(font, k)
            }
            FontParam::HyphenChar if font == 0 => Some(Value::Int('-' as i64)),
            FontParam::SkewChar if font == 0 => Some(Value::Int(-1)),
            FontParam::CharCode(default) => Some(Value::Int(default.into())),
            FontParam::TagCode { .. } if font == 0 => Some(Value::Int(-1)),
            FontParam::TagCode { .. } => {
                let c = field.rsplit('.').next()?.parse().ok()?;
                Some(Value::Int(self.font_metrics(font)?.tag_code(c)))
            }
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

    /// Assignments to the font tables are global (tex.web § 1253). A
    /// `\fontdimen` past the end extends the table, but only for the font
    /// loaded last (tex.web § 580).
    fn assign_font_param(&mut self, param: FontParam, by: Sym) {
        let entry = self.font_entry(param, by);
        self.scan_optional_equals();
        let value = match param {
            FontParam::Dimen => self.scan_dimen().map_or(Value::Unknown, Value::Dimen),
            _ => self.scan_int().map_or(Value::Unknown, Value::Int),
        };
        self.prefixes = Default::default();
        // A write whose font or entry satex could not pin down may have
        // reached any of them.
        let Some((font, field)) = entry else { return self.weaken_font_fields(param, by) };
        let mut value = value;
        if param == FontParam::Dimen {
            let Ok(k) = field[1..].parse::<i64>() else { return };
            match self.font_params(font) {
                Some(params) if k > params => {
                    // tex.web § 580: only the font loaded last grows.
                    if font < self.font_count() {
                        return;
                    }
                    self.set_font_field(font, "params", Value::Int(k));
                }
                Some(_) => {}
                // How many parameters the font has is not known, so
                // whether the write reached the table is not either.
                None => value = Value::Unknown,
            }
            if k <= 0 {
                return;
            }
        }
        if let FontParam::TagCode { lig } = param {
            // pdftex.web `set_tag_code`: -7 to -1 name the tags to clear as
            // bits, anything else clears none.
            value = match (self.font_entry_value(param, font, &field), value) {
                (Some(Value::Int(tag)), _) if tag <= 0 || (tag == 1 && !lig) => Value::Int(tag),
                (Some(Value::Int(tag)), Value::Int(v)) => {
                    Value::Int(if (-v.clamp(-7, 0)) & tag == 0 { tag } else { 0 })
                }
                _ => Value::Unknown,
            };
        }
        self.set_font_field(font, &field, value);
    }

    /// Every font-table entry an unpinned write may have reached
    /// (tex.web § 1253). Font tables are not grouped.
    fn weaken_font_fields(&mut self, param: FontParam, by: Sym) {
        // `font⟨n⟩.p` catches `params` too, which is right: a `\fontdimen`
        // past the end grows the table (tex.web § 580).
        let field = match param {
            FontParam::Dimen => "p1".to_string(),
            FontParam::HyphenChar => "hyphenchar".into(),
            FontParam::SkewChar => "skewchar".into(),
            FontParam::CharCode(_) | FontParam::TagCode { .. } => {
                let by = self.env.identity(by);
                format!("{}.0", self.name(by))
            }
        };
        for font in 0..=self.font_count() {
            let family = font_field_family(font, &field);
            self.weaken_engine(&family, true);
        }
    }

    pub(crate) fn font_name_text(&mut self, font: u32) -> String {
        match self.font_name_known(font) {
            Some(text) => text,
            None => self.font_name(font).unwrap_or_default(),
        }
    }

    /// What `\fontname` prints: the name, with an `at` clause if the size
    /// isn't the design size (tex.web § 1261). XeTeX quotes an installed font's name.
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
    /// it, or the gullet expands it for a [`Convert`].
    pub fn execute_extra(&mut self, p: Primitive, by: Sym, span: Span) {
        match p {
            Primitive::GlueParameter { mu } => {
                self.scan_optional_equals();
                let glue = self.scan_glue_spec(mu);
                let value = glue.map_or(Value::Unknown, |g| g.value(mu));
                let by = self.env.identity(by);
                self.assign_value(by, value, span);
            }
            Primitive::TokensParameter => {
                self.scan_optional_equals();
                let mut toks = self.read_general_text();
                let by = self.env.identity(by);
                // tex.web § 1226: the output routine is stored inside its
                // own brace pair, so `\the\output` shows it braced and
                // firing it opens a group.
                if self.name(by) == "output" {
                    let at = toks.first().map_or(span, |t| t.span);
                    toks.insert(0, Token::new(Tok::Chr('{', Catcode::Begin), at));
                    toks.push(Token::new(Tok::Chr('}', Catcode::End), toks.last().map_or(span, |t| t.span)));
                }
                self.assign_value(by, Value::Toks(Rc::from(toks)), span);
            }
            // "You can't use `\lastpenalty' in vertical mode": a last item
            // is only ever read.
            Primitive::LastItem(_) => {}
            Primitive::Special(special) => self.assign_special(special, by, span),
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
            Syntax::Nothing | Syntax::Whatsit | Syntax::Limits => {}
            Syntax::MathComp(t) => {
                if self.mode.meets(crate::mode::Modes::ANY_MATH) {
                    self.math_comp(t);
                }
            }
            Syntax::Number if self.name(self.env.identity(by)) == "pdfrefximage" => {
                let n = self.scan_int();
                self.ref_ximage(n);
            }
            Syntax::Number if let Some(shift) = self.math_char_command(by) => {
                let code = self.scan_int().map(|v| v >> shift);
                self.math_code_char(code);
            }
            Syntax::Number => {
                self.scan_int();
            }
            Syntax::NamesNumber => {
                let span = self.definition_site(span);
                let Some(name) = self.read_r_token() else { return };
                let relax = crate::env::Binding::builtin(Meaning::Primitive(Primitive::Relax));
                self.env.set(name, relax, self.prefixes.global);
                self.read_equals();
                self.scan_int();
                self.define(name, Meaning::Unknown, by, crate::builtins::DefMode::Declare, span);
            }
            Syntax::NumberIsNumber => {
                self.scan_int();
                self.read_equals();
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
            Syntax::GeneralText if self.name(self.env.identity(by)) == "mark" => {
                let text = self.expanded_general_text();
                self.note_mark(Some(0), text, span);
            }
            Syntax::GeneralText => {
                let text = self.read_general_text();
                let name = self.name(by);
                let patterns = name == "patterns";
                if patterns || name == "hyphenation" {
                    self.store_hyphenation(patterns, &text);
                }
            }
            // `\marks`, the only command of this syntax.
            Syntax::NumberText => {
                let class = self.scan_int();
                let text = self.expanded_general_text();
                self.note_mark(class, text, span);
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
                let register = self.scan_number();
                // An unpinned number names a register in the interval its scan left.
                let within = match register {
                    Some(_) => None,
                    // At most two intervals are kept: the first, and the hull of the rest.
                    None => self.take_index().and_then(|mut parts| {
                        parts.sort_by_key(|p| p.lo);
                        let first = parts.first()?;
                        let rest = parts.get(1..).filter(|r| !r.is_empty()).unwrap_or(&parts[..1]);
                        let lo = rest.iter().map(|p| p.lo).min()?;
                        let hi = rest.iter().map(|p| p.hi).max()?;
                        Some([(first.lo, first.hi), (lo, hi)])
                    }),
                };
                self.scan_optional_equals();
                let global = self.take_global();
                self.scan_box(crate::mode::BoxContext::Set { register, global, within }, span);
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
                match (immediate, stream) {
                    (true, Some(stream)) => {
                        self.set_engine(&format!("write_open.{stream}"), Value::Int(1), true);
                        // The file exists from here on, which `\input` of it
                        // (the `.aux` file at `\end{document}`) finds.
                        self.set_engine(&format!("written.{}", name.trim()), Value::Int(1), true);
                        self.stream_files.insert(stream, name.trim().to_string());
                        self.open_output(&name, true);
                    }
                    // Any stream may be the one opened.
                    (true, None) => {
                        self.set_engine(&format!("written.{}", name.trim()), Value::Int(1), true);
                        self.open_output(&name, false);
                        self.weaken_engine("write_open.", true);
                    }
                    _ => {}
                }
            }
            Syntax::CloseOut => {
                let immediate = std::mem::take(&mut self.prefixes.immediate);
                let stream = self.scan_int();
                match (immediate, stream) {
                    (true, Some(stream)) => {
                        if let Some(name) = self.stream_files.get(&stream).cloned() {
                            self.close_pending(&name);
                        }
                        self.set_engine(&format!("write_open.{stream}"), Value::Int(0), true)
                    }
                    // Any stream may be the one closed.
                    (true, None) => self.weaken_engine("write_open.", true),
                    _ => {}
                }
            }
            Syntax::FamilyFont => {
                let family = self.scan_int();
                self.scan_optional_equals();
                let font = self.scan_font_ident();
                let global = self.take_global();
                let name = self.name(by).to_string();
                match family {
                    Some(family) => {
                        let value = font.map_or(Value::Unknown, |f| Value::Int(f.into()));
                        self.set_engine(&format!("{name}.{family}"), value, global);
                    }
                    // Any family may be the one assigned.
                    None => self.weaken_engine(&format!("{name}."), global),
                }
            }
            Syntax::CopyFont { amount } => {
                let Some(name_cs) = self.read_r_token() else { return };
                let font = self.scan_font_ident();
                if amount {
                    self.scan_int();
                }
                let copy = font.map(|font| self.copy_font(font));
                // A copy of a font satex could not name is no font it
                // knows, and not `\nullfont` either.
                let Some(copy) = copy else {
                    self.define(name_cs, Meaning::Unknown, by, DefMode::Declare, span);
                    return;
                };
                self.set_font_field(copy, "cs", Value::Int(i64::from(name_cs.0)));
                self.define(name_cs, Meaning::Primitive(Primitive::FontIdent(copy)), by, DefMode::Declare, span);
            }
            Syntax::UMathCharDef { num } => {
                let Some(name) = self.read_r_token() else { return };
                self.scan_optional_equals();
                let code = if num { self.scan_int() } else { self.scan_umath_char() };
                // `\Umathcharnumdef` range-checks its packed code the way
                // `\Umathcodenum` does, and recovers the same way.
                let named = self.env.identity(by);
                let code = match crate::builtins::limit(self.name(named), self.out.plugins.engine).filter(|_| num) {
                    Some(limit) => match self.limited(named, name, limit, code, span) {
                        Some(stored) => stored.as_int(),
                        None => code,
                    },
                    None => code,
                };
                let Some(code) = code else {
                    self.define(name, Meaning::Unknown, by, DefMode::Declare, span);
                    return;
                };
                let meaning = Meaning::Register(RegKind::UMathChar, code as u16);
                self.define(name, meaning, by, DefMode::Declare, span);
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
                while ["fixed", "bottom", "top", "both", "overlay", "nooverflow"].iter().any(|k| self.scan_keyword(k)) {
                }
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
                self.scan_id();
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
                self.scan_id();
            }
            Syntax::PdfXForm => {
                self.scan_attr();
                if self.scan_keyword("resources") {
                    self.read_general_text();
                }
                self.scan_int();
            }
            Syntax::PdfXImage => {
                let spec = self.scan_rule_spec();
                self.scan_attr();
                let mut page = Some(1);
                if self.scan_keyword("named") {
                    self.read_general_text();
                    page = None;
                } else if self.scan_keyword("page") {
                    page = self.scan_int();
                }
                if self.scan_keyword("colorspace") {
                    self.scan_int();
                }
                // The pdfTeX manual: the crop box unless `\pdfpagebox` or a
                // keyword names another (1 media, 2 crop).
                let boxes = ["mediabox", "cropbox", "bleedbox", "trimbox", "artbox"];
                let param = self.intern("pdfpagebox");
                let mut which = match self.env.get(param) {
                    None => Some(0),
                    Some(_) => self.env.value(param).as_int().map(|n| n.max(0) as usize),
                };
                if let Some(k) = boxes.iter().position(|b| self.scan_keyword(b)) {
                    which = Some(k + 1);
                }
                // pdfTeX reads the file name as an expanded general text.
                let file = self.read_general_text();
                let file = self.expand_tokens(file.into());
                let name = self.text_of(&file);
                let path = crate::observe::image(self, &name, span);
                let which = match which {
                    Some(1) => Some("media"),
                    Some(0 | 2) => Some("crop"),
                    _ => None,
                };
                let pdf = path.as_ref().is_some_and(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")));
                // A PDF file this run made through `\write18` is what the
                // engine reads, not what was on disk before.
                let produced = self.produced_page(&name);
                // Logged, as every file the run reads (`crate::overlay::log`).
                let read = |p: std::path::PathBuf| crate::overlay::read_bytes(&p).ok();
                let size = match pdf {
                    _ if produced.is_some() => {
                        produced.flatten().filter(|_| page == Some(1) && spec == [None; 3] && which.is_some())
                    }
                    true => path
                        .filter(|_| spec == [None; 3])
                        .zip(which)
                        .zip(page)
                        .and_then(|((p, w), n)| crate::pdfbox::page_box(&read(p)?, w, n)),
                    // A PNG or JPEG: its pixels at its resolution, else at
                    // `\pdfimageresolution`.
                    false => {
                        let param = self.intern("pdfimageresolution");
                        let dpi = self.env.value(param).as_int();
                        path.filter(|_| spec == [None; 3])
                            .zip(dpi)
                            .and_then(|(p, dpi)| crate::pdfbox::raster_size(&read(p)?, dpi))
                    }
                };
                // pdfTeX numbers images from 1 and keeps their sizes for
                // `\pdfrefximage`.
                let n = self.engine_slot_int("ximages", "ximages").map(|n| n + 1);
                self.set_engine("ximages", n.map_or(Value::Unknown, Value::Int), true);
                if let Some(n) = n {
                    for (k, sp) in [("w", size.map(|s| s.0)), ("h", size.map(|s| s.1))] {
                        self.set_engine(&format!("ximage.{n}.{k}"), sp.map_or(Value::Unknown, Value::Int), true);
                    }
                }
            }
            Syntax::PdfColorStack => {
                let stack = self.scan_int();
                let op = if self.scan_keyword("push") {
                    self.read_general_text();
                    crate::facts::StackOp::Push
                } else if self.scan_keyword("pop") {
                    crate::facts::StackOp::Pop
                } else if self.scan_keyword("set") {
                    self.read_general_text();
                    crate::facts::StackOp::Set
                } else {
                    // `current` takes no general text (pdfTeX manual § 8.9.8):
                    // it re-issues the stack's own color, not a new one.
                    self.scan_keyword("current");
                    crate::facts::StackOp::Current
                };
                crate::observe::color_stack(self, stack, op, span);
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
            let Some(token) = self.next_non_blank_x_token() else { return };
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

    /// The pdfTeX manual's `num ⟨integer⟩` or `name ⟨general text⟩`, if either.
    fn scan_id(&mut self) {
        if self.scan_keyword("num") {
            self.scan_int();
        } else if self.scan_keyword("name") {
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
            self.scan_id();
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
        let stream = self.scan_int();
        self.scan_keyword("to");
        let Some(name) = self.read_r_token() else { return };
        let open = self.stream_open(stream);
        if open != Some(true) {
            // tex.web § 484: a stream that is not open is the terminal; a
            // number satex could not pin down may name an open stream
            // instead, so it never ends the job.
            if open == Some(false) && self.terminal_read_is_fatal(span) {
                return;
            }
            self.define(name, Meaning::Unknown, by, DefMode::Declare, span);
            return;
        }
        let stream = stream.expect("an open stream has a number");
        let mut line = self.take_stream_line(stream).unwrap_or_default();
        let sym = self.intern("endlinechar");
        let end_line = self.env.value(sym).as_int();
        if let Some(Ok(c)) = end_line.map(u8::try_from) {
            line.push(char::from(c));
        }
        let mut body = chars(&line, span);
        // tex.web § 482: the character appended is the one `\endlinechar`
        // names, which is not known.
        if end_line.is_none() {
            let unknown = self.unknown_text(Unk::Any, span);
            body.push(unknown);
        }
        let meaning = self.make_macro(Default::default(), body);
        self.define(name, meaning, by, DefMode::Declare, span);
    }

    // ---- expandable ---------------------------------------------------------

    fn convert(&mut self, convert: Convert, span: Span) -> Vec<Token> {
        use Convert as C;
        let text: Option<String> = match convert {
            C::EtexRevision => Some(ETEX_REVISION.into()),
            C::PdftexRevision => Some(pdftex_release().1),
            C::XetexRevision => Some(xetex_release().1),
            C::FontId => self.scan_font_ident().map(|f| f.to_string()),
            C::Xetex(crate::builtins::XeQuery::GlyphName) => self.xetex_glyph_name(),
            C::Xetex(query) => match self.xetex_query(query) {
                Some(Value::Toks(_)) => Some(String::new()),
                _ => None,
            },
            C::Uchar { catcode } => return self.uchar(catcode, span),
            C::FontName => self.scan_font_ident().and_then(|f| self.font_name_known(f)),
            C::PdfFontSize => self.scan_font_ident().and_then(|f| self.font_size(f)).map(crate::value::render_dimen),
            C::PdfFontName | C::PdfFontObjNum => {
                self.scan_font_ident();
                None
            }
            C::StrCmp => {
                let a = self.expanded_general_text();
                let b = self.expanded_general_text();
                let ((pa, ka), (pb, kb)) = (self.known_prefix(&a), self.known_prefix(&b));
                // Two texts whose known beginnings differ compare by them,
                // whatever their unknown rest holds.
                let order = match ka && kb {
                    true => Some(pa.cmp(&pb)),
                    false => pa.chars().zip(pb.chars()).find(|(x, y)| x != y).map(|(x, y)| x.cmp(&y)).or({
                        // A known text that is a proper prefix of the other's known part comes first.
                        match (ka, kb) {
                            (true, _) if pb.len() > pa.len() => Some(std::cmp::Ordering::Less),
                            (_, true) if pa.len() > pb.len() => Some(std::cmp::Ordering::Greater),
                            _ => None,
                        }
                    }),
                };
                // Undecided: one of the three results, one split where it is tested.
                if order.is_none() {
                    let texts = ["-1", "0", "1"].map(|r| {
                        r.chars().map(|c| Token::new(Tok::Chr(c, Catcode::Other), span)).collect::<Rc<[Token]>>()
                    });
                    return vec![self.one_of(texts.into(), span)];
                }
                order.map(|order| {
                    match order {
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
                Some(
                    text.bytes()
                        .map(|b| match b {
                            b'(' | b')' | b'\\' => format!("\\{}", b as char),
                            0x21..=0x7e => (b as char).to_string(),
                            _ => format!("\\{b:03o}"),
                        })
                        .collect(),
                )
            })(),
            C::EscapeName => (|| {
                let text = self.expanded_text()?;
                Some(
                    text.bytes()
                        .filter(|b| *b != 0)
                        .map(|b| match b {
                            b'#' | b'(' | b')' | b'<' | b'>' | b'[' | b']' | b'{' | b'}' | b'/' | b'%' => {
                                format!("#{b:02X}")
                            }
                            0x21..=0x7e => (b as char).to_string(),
                            _ => format!("#{b:02X}"),
                        })
                        .collect(),
                )
            })(),
            C::FileSize | C::FileModDate => {
                // web2c drops the quotes a name may carry, and a file the
                // engine cannot find gives the empty string.
                let name = self.expanded_text().map(|name| name.replace('"', ""));
                let bytes = name.and_then(|name| self.input_bytes(&name));
                match (convert, bytes) {
                    // A file the run made with what it holds unknown.
                    (C::FileSize, None) => {
                        return self.digits_in(crate::value::Num::range(0, crate::value::MAX_INT), span);
                    }
                    (_, Some(None)) => Some(String::new()),
                    (C::FileSize, Some(Some(b))) => Some(b.len().to_string()),
                    // The date of a file that exists: its time is not
                    // read, its form is known (unknown text instead lets a
                    // parse of it run past its end).
                    _ => return self.unknown_pdf_date(span),
                }
            }
            C::FileDump => {
                let offset = if self.scan_keyword("offset") { self.scan_int() } else { Some(0) };
                let length = if self.scan_keyword("length") { self.scan_int() } else { Some(0) };
                let name = self.expanded_text();
                let bytes = name.and_then(|name| self.input_bytes(&name)).flatten();
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
                    Some(text) if file => self.input_bytes(&text).flatten(),
                    Some(text) => Some(text.into_bytes()),
                    None => None,
                };
                bytes.map(|b| {
                    <md5::Md5 as md5::Digest>::digest(&b).iter().map(|x| format!("{x:02X}")).collect::<String>()
                })
            }
            C::Match => {
                let icase = self.scan_keyword("icase");
                let subcount = if self.scan_keyword("subcount") { self.scan_int() } else { Some(-1) };
                let pattern = self.expanded_text();
                let subject = self.expanded_text();
                return self.pdf_match(icase, subcount, pattern, subject, span);
            }
            C::LastMatch => {
                return match self.scan_int() {
                    Some(n) => self.pdf_last_match(n, span),
                    None => vec![self.unknown_text(Unk::Any, span)],
                };
            }
            // The pdfTeX manual: `\pdfuniformdeviate n` is a number in [0, n)
            // for n > 0 and in (n, 0] for n < 0. Which one is not known, so
            // it is digits typed with that interval.
            C::UniformDeviate => {
                return match self.scan_int() {
                    Some(n) if n > 0 => self.digits_in(crate::value::Num::range(0, n - 1), span),
                    Some(n) if n < 0 => self.digits_in(crate::value::Num::range(n + 1, 0), span),
                    Some(_) => self.text_tokens("0", span),
                    None => self.digits_in(crate::value::Num::range(-2_147_483_646, 2_147_483_646), span),
                };
            }
            // pdfTeX: the width of the left margin kern the box's list
            // starts with, past a `\leftskip`; 0pt with none. A line made
            // by the line breaker starts with the one it put there.
            C::LeftMarginKern => {
                let n = self.scan_int();
                let dims = self.box_dims(n);
                let hbox = n.is_some_and(|k| self.box_state(k) == crate::mode::BoxState::HBox);
                match self.box_line(n) {
                    Some(Some((w, h, d, Some(kern)))) if hbox && dims == [w, h, d].map(crate::mode::Dim::exact) => {
                        // No kern prints as `0pt` (pdfTeX's `print("0")`).
                        Some(if kern == 0 { "0pt".into() } else { crate::value::render_dimen(kern) })
                    }
                    _ => None,
                }
            }
            C::Marks => {
                let class = self.scan_int();
                return self.marks_of(class, span);
            }
            C::Mark => return self.marks_of(Some(0), span),
            C::PageRef | C::XFormName | C::InsertHt | C::MarginKern => {
                self.scan_int();
                None
            }
            C::ImageBBox => {
                self.scan_int();
                self.scan_int();
                None
            }
            C::CreationDate => return self.creation_date(span),
            C::NormalDeviate | C::PdftexBanner => None,
            // The pdfTeX manual: stacks are numbered from 1, 0 being the
            // default color stack.
            C::ColorStackInit => {
                self.scan_keyword("page");
                self.scan_keyword("direct");
                self.read_general_text();
                let next = self.engine_slot_int("colorstacks", "colorstacks").map(|n| n + 1);
                self.set_engine("colorstacks", next.map_or(Value::Unknown, Value::Int), true);
                next.map(|n| n.to_string())
            }
            C::Primitive => {
                self.primitive_meaning(span);
                return Vec::new();
            }
        };
        // What the engine gives and satex cannot know is unknown text.
        match text {
            Some(t) => chars(&t, span),
            None => vec![self.unknown_text(Unk::Any, span)],
        }
    }

    /// The pdfTeX manual: `\pdfmatch` is -1 for a pattern that does not
    /// compile (the last match stays), 0 for no match (it is cleared), 1
    /// for a match. `subcount` bounds the groups kept, -1 keeps them all.
    fn pdf_match(
        &mut self,
        icase: bool,
        subcount: Option<i64>,
        pattern: Option<String>,
        subject: Option<String>,
        span: Span,
    ) -> Vec<Token> {
        let (Some(pattern), Some(subject), Some(subcount)) = (pattern, subject, subcount) else {
            self.set_engine(LAST_MATCH_COUNT, Value::Unknown, true);
            return vec![self.unknown_text(Unk::Any, span)];
        };
        let Ok(re) = regex::RegexBuilder::new(&pattern).case_insensitive(icase).build() else {
            return chars("-1", span);
        };
        let groups: Vec<Option<(usize, String)>> = match re.captures(&subject) {
            None => Vec::new(),
            Some(c) => c.iter().map(|m| m.map(|m| (m.start(), m.as_str().to_string()))).collect(),
        };
        let kept = if subcount < 0 { groups.len() } else { groups.len().min(subcount as usize) };
        self.set_engine(LAST_MATCH_COUNT, Value::Int(kept as i64), true);
        for (i, group) in groups.iter().take(kept).enumerate() {
            let text = match group {
                Some((at, text)) => format!("{at}->{text}"),
                None => NO_MATCH.to_string(),
            };
            self.set_engine(&format!("{LAST_MATCH}{i}"), Value::Toks(Rc::from(chars(&text, span))), true);
        }
        chars(if groups.is_empty() { "0" } else { "1" }, span)
    }

    /// `\pdflastmatch n`: `⟨position⟩->⟨text⟩` of group `n` of the last
    /// match, `-1->` for a group there is none of.
    fn pdf_last_match(&mut self, n: i64, span: Span) -> Vec<Token> {
        match self.engine_slot_int(LAST_MATCH, LAST_MATCH_COUNT) {
            None => vec![self.unknown_text(Unk::Any, span)],
            Some(count) if n < 0 || n >= count => chars(NO_MATCH, span),
            Some(_) => {
                let sym = self.engine_sym(&format!("{LAST_MATCH}{n}"));
                match self.env.value(sym) {
                    Value::Toks(text) => text.iter().map(|t| Token::new(t.tok, span)).collect(),
                    _ => vec![self.unknown_text(Unk::Any, span)],
                }
            }
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
            (Some(c), Some(cat))
                if !matches!(
                    cat,
                    Catcode::Escape
                        | Catcode::Eol
                        | Catcode::Ignored
                        | Catcode::Active
                        | Catcode::Comment
                        | Catcode::Invalid
                ) =>
            {
                vec![Token::new(Tok::Chr(c, cat), span)]
            }
            _ => vec![self.unknown_text(Unk::Any, span)],
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

    /// The bytes of the file the engine finds by `name`: what this run wrote
    /// to it, else what is on disk. `Some(None)` when there is none, `None`
    /// when the run wrote it with what it holds unknown.
    pub(crate) fn input_bytes(&mut self, name: &str) -> Option<Option<Vec<u8>>> {
        match (self.written_text(name), self.locate_input(name)) {
            (Some(Some(text)), _) => Some(Some(text.into_bytes())),
            (_, Some(path)) => Some(crate::overlay::read_bytes(&path).ok()),
            (Some(None), None) => None,
            (None, None) => Some(None),
        }
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
        let token = self.next_non_blank_x_token()?;
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
        // tex.web § 465: `\the` of a font gives its identifier. One satex
        // cannot name is some identifier: unknown, never no token at all.
        let unknown = |m: &mut Self| Some(vec![Token::new(Tok::Cs(m.unknown), span)]);
        let font = match p {
            Primitive::FontDef => match self.current_font() {
                Some(font) => font,
                None => return self.current_font_identifiers(span).or_else(|| unknown(self)),
            },
            Primitive::FontIdent(font) => font,
            Primitive::Command(Syntax::FamilyFont) => {
                let font = self.scan_int().and_then(|family| self.family_font(sym, family));
                match font {
                    Some(font) => font,
                    None => return unknown(self),
                }
            }
            _ => return None,
        };
        match self.font_cs(font) {
            Some(cs) => Some(vec![Token::new(Tok::Cs(cs), span)]),
            None => unknown(self),
        }
    }

    /// The current font a join left one of a few: a token for one of
    /// their identifiers, which a path parts on where it matters.
    fn current_font_identifiers(&mut self, span: Span) -> Option<Vec<Token>> {
        if self.engine_unpinned("font") {
            return None;
        }
        let slot = self.engine_sym("font");
        let fonts = self.env.get(slot)?.value.exact_ints()?;
        let texts = (fonts.into_iter())
            .map(|font| Some(Rc::from([Token::new(Tok::Cs(self.font_cs(u32::try_from(font).ok()?)?), span)])))
            .collect::<Option<Vec<Rc<[Token]>>>>()?;
        Some(vec![self.one_of(texts, span)])
    }

    /// The identifier font `font` was loaded under.
    fn font_cs(&mut self, font: u32) -> Option<Sym> {
        if font == 0 {
            return Some(self.intern("nullfont"));
        }
        let index = self.font_field(font, "cs").as_int()?;
        Some(Sym(u32::try_from(index).ok()?))
    }
}
