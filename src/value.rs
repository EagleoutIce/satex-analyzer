//! TeX base types: number, dimen, glue, muglue, token list (The TeXbook).

use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::tex::Token;

pub type Scaled = i64;

pub const UNIT: Scaled = 65536;
pub const MAX_DIMEN: Scaled = (1 << 30) - 1;
/// tex.web § 445: `scan_int` refuses to go past this and says "Number too
/// big", using the limit itself in place of what was written.
pub const MAX_INT: i64 = (1 << 31) - 1;
/// The range of TeX's `integer` (32 bits in web2c). Additions without an
/// overflow check (`\advance`, tex.web § 1238) wrap within it, so a count
/// or dimension register may hold any of these.
pub const WORD_MIN: i64 = i32::MIN as i64;
pub const WORD_MAX: i64 = i32::MAX as i64;

/// `n` as a 32-bit integer, wrapped the way web2c's C arithmetic wraps.
pub fn wrap(n: i64) -> i64 {
    i64::from(n as i32)
}

/// tex.web § 105 `mult_and_add`: `n*x+y` when it stays within
/// `±max_answer`, else `None` (TeX's `arith_error`), computed on 32-bit
/// integers as web2c does (`negate` of -2^31 stays -2^31).
/// `mult_integers` is `max_answer` 2^31-1 with `y = 0`, `nx_plus_y`
/// 2^30-1.
pub fn mult_and_add(n: i64, x: i64, y: i64, max_answer: i64) -> Option<i64> {
    let (mut n, mut x, y, m) = (n as i32, x as i32, y as i32, max_answer as i32);
    if n < 0 {
        x = x.wrapping_neg();
        n = n.wrapping_neg();
    }
    if n == 0 {
        return Some(i64::from(y));
    }
    (x <= m.wrapping_sub(y) / n && x.wrapping_neg() <= m.wrapping_add(y) / n)
        .then(|| i64::from(n.wrapping_mul(x).wrapping_add(y)))
}

/// tex.web § 106 `x_over_n`: `x/n` truncated toward zero on 32-bit
/// integers. `None` (`arith_error`) when `n` is zero.
pub fn x_over_n(x: i64, n: i64) -> Option<i64> {
    let (mut x, mut n) = (x as i32, n as i32);
    if n == 0 {
        return None;
    }
    if n < 0 {
        x = x.wrapping_neg();
        n = n.wrapping_neg();
    }
    // `-((-x) div n)` for a negative `x`: compiled C (pdfTeX's) folds it
    // to `x div n`, so -2^31 ÷ 2 is -2^30 although `-x` wraps.
    Some(i64::from(x.wrapping_div(n)))
}

/// tex.web § 1229 `trap_zero_glue`: a glue assigned to a register or
/// parameter whose three amounts are zero becomes `zero_glue`, of order
/// normal (`\gluestretchorder` then says 0).
pub fn trap_zero_glue(value: Value) -> Value {
    let zero = |g: &Glue| g.width == 0 && g.stretch.amount == 0 && g.shrink.amount == 0;
    match value {
        Value::Glue(g) if zero(&g) => Value::Glue(Glue::default()),
        Value::MuGlue(g) if zero(&g) => Value::MuGlue(Glue::default()),
        other => other,
    }
}
const MAX_DECIMAL_DIGITS: usize = 17;
const MAX_FIL_ORDER: u8 = 3;

const UNITS: [(&str, i64, i64); 12] = [
    ("pt", 1, 1),
    ("pc", 12, 1),
    ("in", 7227, 100),
    ("bp", 7227, 7200),
    ("cm", 7227, 254),
    ("mm", 7227, 2540),
    ("dd", 1238, 1157),
    ("cc", 14856, 1157),
    ("nd", 685, 642),
    ("nc", 1370, 107),
    ("sp", 1, 65536),
    // Math units are stored in the same scaled units as points. Only the
    // printed name differs (tex.web § 455).
    ("mu", 1, 1),
];

/// A glue's stretch or shrink component: an amount and an order of infinity
/// (`fil`, `fill`, `filll`). Half of a [`Glue`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Stretch {
    pub amount: Scaled,
    pub order: u8,
}

/// TeX glue: a natural [`Scaled`] width plus [`Stretch`] and shrink. What
/// `\skip` registers and rubber lengths like `\hfill` hold.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Glue {
    pub width: Scaled,
    pub stretch: Stretch,
    pub shrink: Stretch,
}

/// What kind of TeX quantity an abstract number stands for. A count and a
/// dimension are one [`Num`] (`Value::Num`); glue and muglue have
/// components (tex.web § 150), so their abstract form is one [`Num`] per
/// component ([`Value::GlueNum`]). The tag names the family both share.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum Quantity {
    Int,
    Dimen,
    Glue,
    MuGlue,
}

/// The value a `\count`/`\dimen`/`\skip`/`\toks`-family register can hold, or
/// `Unknown` when the interpreter cannot pin it down.
/// [`crate::env::Env::value`] reads one per [`crate::tex::Sym`].
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum Value {
    Int(i64),
    Dimen(Scaled),
    Glue(Glue),
    MuGlue(Glue),
    Toks(Rc<[Token]>),
    #[default]
    Unknown,
    /// A color, as xcolor stores one in `\\color@⟨name⟩` (xcolor § 2.5).
    Color(Colors),
    /// A count or dimension whose value is not known but lies in an
    /// interval: what a join past `limits.value_set`, arithmetic on such a
    /// value, or a typesetting result leaves. Read as a number it is
    /// unknown ([`crate::env::Env::value`] reads it so). A test that looks
    /// at it decides by the interval.
    Num {
        quantity: Quantity,
        num: Num,
    },
    /// A glue or muglue whose components are not known exactly: one
    /// interval per component. [`Value::Num`]'s counterpart for glue.
    GlueNum {
        quantity: Quantity,
        hull: GlueHull,
    },
    /// A few values of one kind this may be, in a fixed order: exact ones
    /// up to `limits.value_set`, or intervals and hulls among them up to
    /// `limits.interval_set`. This is TeX's one set lift ([`Value::join`]),
    /// for every kind above. Read as a value it is unknown, as a
    /// [`Value::Num`] is.
    OneOf(Rc<[Value]>),
}

/// How many values a join lists before it widens to an interval, unless
/// `limits.value_set` says otherwise.
pub const MAY_VALUES: usize = 5;
/// How many members a set with intervals among them keeps before it widens
/// to their hull, unless `limits.interval_set` says otherwise.
pub const MAY_INTERVALS: usize = 4;

/// An abstract number (a count or a dimension in scaled points, or one
/// component of a glue): the interval `[lo, hi]` it lies in. TeX's numbers
/// are bounded (`MAX_INT`, `MAX_DIMEN`), so the full interval of its kind is
/// the top of the lattice, which widening reaches. The exact values a
/// binding may be, while there are few, are not `Num`'s to keep: that is
/// [`Value::OneOf`], the one set lift every kind shares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Num {
    pub lo: i64,
    pub hi: i64,
}

impl Num {
    pub fn exact(n: i64) -> Num {
        Num { lo: n, hi: n }
    }

    pub fn range(lo: i64, hi: i64) -> Num {
        Num { lo: lo.min(hi), hi: hi.max(lo) }
    }

    /// Every value of the kind: what an unknown typesetting result is.
    pub fn full(dimen: bool) -> Num {
        let max = if dimen { MAX_DIMEN } else { MAX_INT };
        Num::range(-max, max)
    }

    /// Least upper bound: the interval spanning both.
    pub fn join(&self, other: &Num) -> Num {
        Num::range(self.lo.min(other.lo), self.hi.max(other.hi))
    }

    /// Widening: a bound that grew goes to the end of TeX's 32-bit range,
    /// not `\maxdimen`, which `\advance` may pass (tex.web § 1238), so a
    /// loop that keeps advancing a dimension stays covered.
    pub fn widen(&self, next: &Num) -> Num {
        let lo = if next.lo < self.lo { WORD_MIN } else { self.lo };
        let hi = if next.hi > self.hi { WORD_MAX } else { self.hi };
        Num::range(lo, hi)
    }

    pub fn neg(&self) -> Num {
        Num::range(-self.hi, -self.lo)
    }

    /// TeX arithmetic lifted to an interval: `exact` at a point when both
    /// ends agree, otherwise `ideal` (the unbounded-integer operation,
    /// `None` only at a zero divisor) taken at the box's corners cut at
    /// zero: each operation here is monotone per argument on a part of
    /// constant sign, so the corners bound it. An ideal result outside
    /// `range` is either an error (in-range results lie in the clipped
    /// hull) or, with `wrap`, a 32-bit wrap-around. `None` when every
    /// combination errors.
    pub fn lift(
        args: &[&Num],
        range: (i64, i64),
        wrap: bool,
        exact: impl Fn(&[i64]) -> Option<i64>,
        ideal: impl Fn(&[i64]) -> Option<i64>,
    ) -> (Option<Num>, bool) {
        if args.iter().all(|a| a.lo == a.hi) {
            let point: Vec<i64> = args.iter().map(|a| a.lo).collect();
            return match exact(&point) {
                Some(v) => (Some(Num::exact(v)), false),
                None => (None, true),
            };
        }
        let combos = |pts: &[Vec<i64>]| -> Vec<Vec<i64>> {
            pts.iter().fold(vec![Vec::new()], |acc, xs| {
                acc.iter().flat_map(|c| xs.iter().map(move |&x| [c.clone(), vec![x]].concat())).collect()
            })
        };
        let pts: Vec<Vec<i64>> = args
            .iter()
            .map(|a| {
                let mut p = vec![a.lo, a.hi];
                p.extend([-1, 0, 1].into_iter().filter(|v| a.lo <= *v && *v <= a.hi));
                p
            })
            .collect();
        let (mut lo, mut hi, mut err, mut any, mut wrapped) = (i64::MAX, i64::MIN, false, false, false);
        for c in combos(&pts) {
            let Some(v) = ideal(&c) else {
                err = true;
                continue;
            };
            if v < range.0 || v > range.1 {
                if wrap {
                    wrapped = true;
                } else {
                    err = true;
                }
            }
            any = true;
            lo = lo.min(v.clamp(range.0, range.1));
            hi = hi.max(v.clamp(range.0, range.1));
        }
        if wrapped {
            return (Some(Num::range(WORD_MIN, WORD_MAX)), err);
        }
        (any.then(|| Num::range(lo, hi)), err)
    }

    /// The set lift for arithmetic (`Set_k`, TeX's one set lift, applied to
    /// exact numbers rather than to whole [`Value`]s): every combination
    /// `exact` gives from bounded sets of exact numbers, deduplicated,
    /// while there are at most `members` of them and the combinations are
    /// few enough to try (≤ 64 combined points). `None` once that fails: the
    /// caller falls back to [`Num::lift`] on the sets' own hulls.
    pub fn lift_points(
        sets: &[&[i64]],
        members: usize,
        exact: impl Fn(&[i64]) -> Option<i64>,
    ) -> Option<(Vec<i64>, bool)> {
        if sets.iter().map(|s| s.len()).product::<usize>() > 64 {
            return None;
        }
        let combos = sets.iter().fold(vec![Vec::new()], |acc, xs| {
            acc.iter().flat_map(|c: &Vec<i64>| xs.iter().map(move |&x| [c.clone(), vec![x]].concat())).collect()
        });
        let mut out = Vec::new();
        let mut err = false;
        for c in combos {
            match exact(&c) {
                Some(v) => {
                    if !out.contains(&v) {
                        out.push(v);
                    }
                }
                None => err = true,
            }
        }
        (!out.is_empty() && out.len() <= members).then_some((out, err))
    }

    /// `self relation other` when the interval alone decides it. `None`
    /// otherwise: some pair of concrete values may disagree.
    pub fn compare(&self, relation: char, other: &Num) -> Option<bool> {
        match relation {
            '<' if self.hi < other.lo => Some(true),
            '<' if self.lo >= other.hi => Some(false),
            '>' if self.lo > other.hi => Some(true),
            '>' if self.hi <= other.lo => Some(false),
            '=' if self.lo == self.hi && other.lo == other.hi && self.lo == other.lo => Some(true),
            '=' if self.hi < other.lo || self.lo > other.hi => Some(false),
            _ => None,
        }
    }

    /// Whether it is odd, when the interval is a single value.
    pub fn odd(&self) -> Option<bool> {
        (self.lo == self.hi).then(|| self.lo % 2 != 0)
    }

    /// What `self relation bound` being `holds` leaves of `self`: the
    /// narrowing a test teaches the arm it chooses. `None` when no value
    /// satisfies it.
    pub fn narrow(&self, relation: char, bound: i64, holds: bool) -> Option<Num> {
        let (lo, hi) = match (relation, holds) {
            ('<', true) => (self.lo, self.hi.min(bound.saturating_sub(1))),
            ('<', false) => (self.lo.max(bound), self.hi),
            ('>', true) => (self.lo.max(bound.saturating_add(1)), self.hi),
            ('>', false) => (self.lo, self.hi.min(bound)),
            ('=', true) => (bound.max(self.lo), bound.min(self.hi)),
            // Not the bound: an end at it moves in.
            ('=', false) => (
                if self.lo == bound { self.lo.saturating_add(1) } else { self.lo },
                if self.hi == bound { self.hi.saturating_sub(1) } else { self.hi },
            ),
            _ => (self.lo, self.hi),
        };
        (lo <= hi).then(|| Num::range(lo, hi))
    }
}

/// A glue's abstract form (tex.web § 150): one interval per component.
/// [`Value::GlueNum`]'s payload, [`Num`] lifted componentwise the way
/// [`Colors`] lifts one per color component.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GlueHull {
    pub width: Num,
    pub stretch: Num,
    pub stretch_order: Num,
    pub shrink: Num,
    pub shrink_order: Num,
}

impl GlueHull {
    pub fn exact(g: &Glue) -> GlueHull {
        GlueHull {
            width: Num::exact(g.width),
            stretch: Num::exact(g.stretch.amount),
            stretch_order: Num::exact(i64::from(g.stretch.order)),
            shrink: Num::exact(g.shrink.amount),
            shrink_order: Num::exact(i64::from(g.shrink.order)),
        }
    }

    /// Every glue of the kind: what an unknown typesetting result is.
    pub fn full() -> GlueHull {
        GlueHull {
            width: Num::full(true),
            stretch: Num::full(true),
            stretch_order: Num::range(0, i64::from(MAX_FIL_ORDER)),
            shrink: Num::full(true),
            shrink_order: Num::range(0, i64::from(MAX_FIL_ORDER)),
        }
    }

    /// Least upper bound, componentwise.
    pub fn join(&self, other: &GlueHull) -> GlueHull {
        GlueHull {
            width: self.width.join(&other.width),
            stretch: self.stretch.join(&other.stretch),
            stretch_order: self.stretch_order.join(&other.stretch_order),
            shrink: self.shrink.join(&other.shrink),
            shrink_order: self.shrink_order.join(&other.shrink_order),
        }
    }
}

/// One of xcolor's color models (xcolor, table 3), with the two pseudo
/// models that name a color instead of measuring one.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum Model {
    Rgb,
    Cmy,
    Cmyk,
    Hsb,
    Gray,
    /// `RGB`, whole numbers up to `\rangeRGB`.
    IntRgb,
    /// `HTML`, six hexadecimal digits.
    Html,
    /// `HSB`, whole numbers up to `\rangeHSB`.
    IntHsb,
    /// `Gray`, whole numbers up to `\rangeGray`.
    IntGray,
    /// `Hsb`, a hue in degrees up to `\rangeHsb`.
    DegHsb,
    /// `tHsb`, `Hsb` with the hue transformed by `\rangetHsb`.
    DegTHsb,
    /// `wave`, a wavelength in nanometres.
    Wave,
    Named,
    Ps,
}

/// The models `\XC@coremodel` leaves alone, in the order [`Model`] lists
/// them. Every other model is converted into one of these before xcolor
/// stores it.
pub const CORE_MODELS: [Model; 5] = [Model::Rgb, Model::Cmy, Model::Cmyk, Model::Hsb, Model::Gray];

impl Model {
    pub fn as_str(self) -> &'static str {
        match self {
            Model::Rgb => "rgb",
            Model::Cmy => "cmy",
            Model::Cmyk => "cmyk",
            Model::Hsb => "hsb",
            Model::Gray => "gray",
            Model::IntRgb => "RGB",
            Model::Html => "HTML",
            Model::IntHsb => "HSB",
            Model::IntGray => "Gray",
            Model::DegHsb => "Hsb",
            Model::DegTHsb => "tHsb",
            Model::Wave => "wave",
            Model::Named => "named",
            Model::Ps => "ps",
        }
    }

    pub fn parse(name: &str) -> Option<Model> {
        [
            Model::Rgb,
            Model::Cmy,
            Model::Cmyk,
            Model::Hsb,
            Model::Gray,
            Model::IntRgb,
            Model::Html,
            Model::IntHsb,
            Model::IntGray,
            Model::DegHsb,
            Model::DegTHsb,
            Model::Wave,
            Model::Named,
            Model::Ps,
        ]
        .into_iter()
        .find(|m| m.as_str() == name)
    }

    /// How many components the model takes. `HTML` counts the three bytes
    /// its six digits spell.
    pub fn arity(self) -> usize {
        match self {
            Model::Cmyk => 4,
            Model::Gray | Model::IntGray | Model::Wave => 1,
            Model::Named | Model::Ps => 0,
            _ => 3,
        }
    }

    /// What `\XC@coremodel` converts it to before xcolor stores it.
    pub fn core(self) -> Model {
        match self {
            Model::IntRgb | Model::Html => Model::Rgb,
            Model::IntHsb | Model::DegHsb | Model::DegTHsb | Model::Wave => Model::Hsb,
            Model::IntGray => Model::Gray,
            other => other,
        }
    }

    /// The range component `i` lies in, in scaled points, with xcolor's own
    /// `\rangeRGB`, `\rangeHSB`, `\rangeHsb` and `\rangeGray` defaults where
    /// the model is scaled by one. `None` for a model without components.
    pub fn bounds(self, i: usize) -> Option<(Scaled, Scaled)> {
        let top = match self {
            Model::Rgb | Model::Cmy | Model::Cmyk | Model::Hsb | Model::Gray => UNIT,
            Model::IntRgb | Model::Html => 255 * UNIT,
            Model::IntHsb => 240 * UNIT,
            Model::IntGray => 15 * UNIT,
            Model::DegHsb | Model::DegTHsb if i == 0 => 360 * UNIT,
            Model::DegHsb | Model::DegTHsb => UNIT,
            // Every wavelength outside this is black (xcolor § 2.3).
            Model::Wave => return (i == 0).then_some((363 * UNIT, 814 * UNIT)),
            Model::Named | Model::Ps => return None,
        };
        (i < self.arity()).then_some((0, top))
    }

    /// Whether the model's components are written as whole numbers.
    pub fn whole(self) -> bool {
        matches!(self, Model::IntRgb | Model::Html | Model::IntHsb | Model::IntGray)
    }
}

/// A color as xcolor holds one: a model and one abstract number per
/// component, in scaled points, the units xcolor's own arithmetic works in
/// (`\strip@pt` of a `\dimen`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Color {
    pub model: Model,
    pub components: Rc<[Num]>,
}

impl Color {
    pub fn new(model: Model, components: Vec<Num>) -> Color {
        Color { model, components: components.into() }
    }

    /// The color exactly, when every component is one value.
    pub fn exact(&self) -> Option<Vec<Scaled>> {
        self.components.iter().map(|c| (c.lo == c.hi).then_some(c.lo)).collect()
    }

    /// `1,0.50195,0`, as xcolor writes the spec.
    pub fn spell(&self) -> String {
        self.components.iter().map(spell_component).collect::<Vec<_>>().join(",")
    }
}

/// `1`, `0.50195`, `?` for a component that is not one value.
fn spell_component(c: &Num) -> String {
    if c.lo != c.hi {
        return "?".into();
    }
    let mut s = render_dimen(c.lo);
    s.truncate(s.len() - "pt".len());
    match s.strip_suffix(".0") {
        Some(whole) => whole.to_string(),
        None => s,
    }
}

/// What a color-valued binding may hold: one model, each component bounded
/// by a [`Num`]. An exact color is one whose components are all points
/// ([`Color::exact`]); a few exact colors this may be are, like every
/// other kind, [`Value::OneOf`] of them, not a set of `Colors`'s own.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Colors {
    /// Component-wise bounds over every member.
    pub hull: Color,
}

impl Colors {
    pub fn exact(color: Color) -> Colors {
        Colors { hull: color }
    }

    pub fn model(&self) -> Model {
        self.hull.model
    }

    /// Least upper bound. Two models have no common bound: xcolor's
    /// `\XC@calc@` conversion rounds to scaled points, so a converted spec
    /// is not what the other model would have stored, and claiming one
    /// would invent a value. `None` is that, and the caller goes to
    /// [`Value::Unknown`].
    pub fn join(&self, other: &Colors) -> Option<Colors> {
        if self.hull.model != other.hull.model {
            return None;
        }
        let arity = self.hull.components.len();
        if arity != other.hull.components.len() {
            return None;
        }
        let hull: Vec<Num> = (0..arity).map(|i| self.hull.components[i].join(&other.hull.components[i])).collect();
        Some(Colors { hull: Color::new(self.hull.model, hull) })
    }
}

/// `⟨decimal constant⟩` as xcolor's arithmetic holds it: scaled points,
/// rounded the way `scan_decimal` rounds (tex.web § 102).
pub fn parse_component(s: &str) -> Option<Scaled> {
    scale_by(UNIT, s.trim())
}

/// `⟨model⟩{⟨spec⟩}` as the run wrote it, with each component an exact
/// [`Num`]. `None` when the model carries no components or the spec is not
/// the numbers the model takes.
pub fn parse_color(model: Model, spec: &str) -> Option<Color> {
    if model.arity() == 0 {
        return None;
    }
    if model == Model::Html {
        let digits = spec.trim();
        if digits.len() != 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let bytes = (0..3).map(|i| i64::from_str_radix(&digits[2 * i..2 * i + 2], 16).ok());
        let components: Option<Vec<Num>> = bytes.map(|b| Some(Num::exact(b? * UNIT))).collect();
        return Some(Color::new(model, components?));
    }
    let components: Option<Vec<Num>> = spec.split(',').map(|part| Some(Num::exact(parse_component(part)?))).collect();
    let components = components?;
    (components.len() == model.arity()).then(|| Color::new(model, components))
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Dimen(a), Value::Dimen(b)) => a == b,
            (Value::Glue(a), Value::Glue(b)) | (Value::MuGlue(a), Value::MuGlue(b)) => a == b,
            (Value::Toks(a), Value::Toks(b)) => {
                a.len() == b.len() && a.iter().zip(b.iter()).all(|(x, y)| x.tok == y.tok)
            }
            (Value::Num { quantity: x, num: a }, Value::Num { quantity: y, num: b }) => x == y && a == b,
            (Value::GlueNum { quantity: x, hull: a }, Value::GlueNum { quantity: y, hull: b }) => x == y && a == b,
            (Value::Color(a), Value::Color(b)) => a == b,
            (Value::OneOf(a), Value::OneOf(b)) => a == b,
            _ => false,
        }
    }
}

impl Value {
    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(n) | Value::Dimen(n) => Some(*n),
            Value::Glue(g) | Value::MuGlue(g) => Some(g.width),
            _ => None,
        }
    }

    pub fn as_dimen(&self) -> Option<Scaled> {
        match self {
            Value::Dimen(d) | Value::Int(d) => Some(*d),
            Value::Glue(g) | Value::MuGlue(g) => Some(g.width),
            _ => None,
        }
    }

    /// The exact values of one kind this may be, when there are few:
    /// itself, when it is one already; [`Value::OneOf`]'s members. `None`
    /// once it is only an interval or hull (already past `limits.value_set`
    /// or a typesetting result), or [`Value::Unknown`].
    pub(crate) fn exacts(&self) -> Option<Vec<Value>> {
        match self {
            Value::Unknown | Value::Num { .. } | Value::GlueNum { .. } => None,
            Value::Color(c) if c.hull.exact().is_none() => None,
            Value::OneOf(set) => Some(set.to_vec()),
            other => Some(vec![other.clone()]),
        }
    }

    /// [`Value::exacts`], as plain numbers: a count or dimension's exact
    /// members. `None` for anything that is not a bounded set of numbers.
    pub(crate) fn exact_ints(&self) -> Option<Vec<i64>> {
        self.exacts()?.iter().map(Value::as_dimen).collect()
    }

    /// A [`Value::OneOf`] of numbers as its members' intervals, exact ones
    /// as intervals of one: a test decides it member by member and narrows
    /// each. `None` for anything else.
    pub(crate) fn member_nums(&self) -> Option<Vec<Num>> {
        let Value::OneOf(set) = self else { return None };
        let nums: Vec<(Quantity, Num)> = set.iter().map(Value::num).collect::<Option<_>>()?;
        nums.iter().all(|(q, _)| *q == nums[0].0).then(|| nums.into_iter().map(|(_, n)| n).collect())
    }

    /// The interval or hull an exact value widens to; every other kind is
    /// already one. [`Value::OneOf`]'s widening once it grows past
    /// `limits.value_set`.
    fn hull(&self) -> Value {
        match self {
            Value::Int(n) => Value::Num { quantity: Quantity::Int, num: Num::exact(*n) },
            Value::Dimen(d) => Value::Num { quantity: Quantity::Dimen, num: Num::exact(*d) },
            Value::Glue(g) => Value::GlueNum { quantity: Quantity::Glue, hull: GlueHull::exact(g) },
            Value::MuGlue(g) => Value::GlueNum { quantity: Quantity::MuGlue, hull: GlueHull::exact(g) },
            // A token list has no interval domain: nothing narrower than
            // unknown once it is not one of a few exact lists.
            Value::Toks(_) => Value::Unknown,
            // A set of numbers joins with an interval as its own interval.
            Value::OneOf(_) => match self.num() {
                Some((quantity, num)) => Value::Num { quantity, num },
                None => self.clone(),
            },
            other => other.clone(),
        }
    }

    /// Least upper bound of two already-widened values (a [`Value::Num`],
    /// [`Value::GlueNum`], non-exact [`Value::Color`] or
    /// [`Value::Unknown`]) of the same kind. [`Value::Unknown`] when the
    /// kinds differ.
    fn join_hulls(a: &Value, b: &Value) -> Value {
        match (a, b) {
            (Value::Num { quantity: x, num: p }, Value::Num { quantity: y, num: q }) if x == y => {
                Value::Num { quantity: *x, num: p.join(q) }
            }
            (Value::GlueNum { quantity: x, hull: p }, Value::GlueNum { quantity: y, hull: q }) if x == y => {
                Value::GlueNum { quantity: *x, hull: p.join(q) }
            }
            (Value::Color(p), Value::Color(q)) => p.join(q).map_or(Value::Unknown, Value::Color),
            _ => Value::Unknown,
        }
    }

    /// The members of the set lift this is: [`Value::OneOf`]'s, else
    /// itself. `None` for [`Value::Unknown`], which no set holds.
    fn members(&self) -> Option<Vec<Value>> {
        match self {
            Value::Unknown => None,
            Value::OneOf(set) => Some(set.to_vec()),
            other => Some(vec![other.clone()]),
        }
    }

    /// The set lift every kind shares: the union of two sets. A number a
    /// member's interval holds is left out, and intervals that overlap or
    /// touch become one.
    fn union(mut a: Vec<Value>, b: Vec<Value>) -> Vec<Value> {
        for mut v in b {
            loop {
                let Some((q, n)) = v.num() else { break };
                let near = |w: &Value| {
                    w.num().is_some_and(|(p, m)| {
                        p == q
                            && m.lo <= n.hi.saturating_add(1)
                            && n.lo <= m.hi.saturating_add(1)
                            && (w.exacts().is_none() || v.exacts().is_none())
                    })
                };
                let Some(i) = a.iter().position(near) else { break };
                let (_, m) = a.swap_remove(i).num().expect("a number");
                v = Value::Num { quantity: q, num: m.join(&n) };
            }
            if !a.contains(&v) {
                a.push(v);
            }
        }
        a
    }

    /// Least upper bound: equal values stay; otherwise the union of both
    /// sides' members ([`Value::OneOf`], TeX's one set lift) while it has
    /// at most `sets.values` exact members, or `sets.intervals` with an
    /// interval or hull among them, else the join of every member's hull.
    /// Kinds that do not match join to [`Value::Unknown`].
    pub fn join(self, other: &Value, sets: crate::env::SetLimits) -> Value {
        if &self == other {
            return self;
        }
        let (Some(a), Some(b)) = (self.members(), other.members()) else { return Value::Unknown };
        let set = Value::union(a, b);
        let exact = set.iter().all(|v| v.exacts().is_some());
        if set.len() <= if exact { sets.values } else { sets.intervals } {
            let mut set = set;
            // One order for one set, so that equal joins compare equal.
            set.sort_by_cached_key(|v| format!("{v:?}"));
            return if set.len() == 1 { set.into_iter().next().unwrap() } else { Value::OneOf(set.into()) };
        }
        set.into_iter().map(|v| v.hull()).reduce(|a, b| Value::join_hulls(&a, &b)).unwrap_or(Value::Unknown)
    }

    /// The abstract number this is, with what kind of quantity it is: a
    /// known count or dimension exactly, a [`Value::Num`] as it stands, or
    /// the join of a [`Value::OneOf`] of numbers. `None` for anything
    /// else, including glue and muglue ([`Value::GlueNum`]'s width is not
    /// this: a test reads it through [`Value::as_dimen`]).
    pub fn num(&self) -> Option<(Quantity, Num)> {
        match self {
            Value::Int(n) => Some((Quantity::Int, Num::exact(*n))),
            Value::Dimen(d) => Some((Quantity::Dimen, Num::exact(*d))),
            Value::Num { quantity, num } => Some((*quantity, num.clone())),
            Value::OneOf(set) => {
                let mut nums = set.iter().map(Value::num).collect::<Option<Vec<_>>>()?.into_iter();
                let first = nums.next()?;
                nums.try_fold(first, |(q, acc), (q2, n2)| (q == q2).then(|| (q, acc.join(&n2))))
            }
            _ => None,
        }
    }

    /// Whether it is odd, decided by every exact member when there are
    /// few, else by the interval (only when that is a single value).
    pub fn odd(&self) -> Option<bool> {
        match self.exact_ints() {
            Some(ints) => {
                let mut odds = ints.iter().map(|n| n % 2 != 0);
                let first = odds.next()?;
                odds.all(|o| o == first).then_some(first)
            }
            None => self.num()?.1.odd(),
        }
    }

    /// The value an abstract number stands for: known when it is one.
    pub fn from_num(quantity: Quantity, num: Num) -> Value {
        match (num.lo == num.hi, quantity) {
            (true, Quantity::Int) => Value::Int(num.lo),
            (true, Quantity::Dimen) => Value::Dimen(num.lo),
            _ => Value::Num { quantity, num },
        }
    }

    pub fn render(&self) -> String {
        match self {
            Value::Int(n) => n.to_string(),
            Value::Dimen(d) => render_dimen(*d),
            Value::Glue(g) => render_glue(g, "pt"),
            Value::MuGlue(g) => render_glue(g, "mu"),
            Value::Toks(t) => format!("<{} tokens>", t.len()),
            Value::Color(c) => format!("{} {}", c.model().as_str(), c.hull.spell()),
            Value::Unknown | Value::Num { .. } | Value::GlueNum { .. } | Value::OneOf(_) => "?".into(),
        }
    }
}

/// tex.web § 103: a dimension prints with as many decimals as it needs and
/// never fewer than one, rounded so that reading it back gives the same
/// number of scaled points.
pub fn render_dimen(sp: Scaled) -> String {
    let mut out = String::new();
    let mut value = sp;
    if value < 0 {
        out.push('-');
        value = -value;
    }
    out.push_str(&(value / UNIT).to_string());
    out.push('.');
    let mut fraction = 10 * (value % UNIT) + 5;
    let mut delta = 10;
    loop {
        if delta > UNIT {
            fraction += UNIT / 2 - delta / 2;
        }
        out.push(char::from(b'0' + (fraction / UNIT) as u8));
        fraction = 10 * (fraction % UNIT);
        delta *= 10;
        if fraction <= delta {
            break;
        }
    }
    out.push_str("pt");
    out
}

fn render_glue(g: &Glue, unit: &str) -> String {
    let scalar = |sp: Scaled| {
        let mut s = render_dimen(sp);
        s.truncate(s.len() - "pt".len());
        s
    };
    let mut s = format!("{}{unit}", scalar(g.width));
    for (label, part) in [("plus", g.stretch), ("minus", g.shrink)] {
        if part.amount != 0 {
            s.push(' ');
            s.push_str(label);
            s.push(' ');
            s.push_str(&match part.order {
                0 => format!("{}{unit}", scalar(part.amount)),
                n => format!("{}fi{}", scalar(part.amount), "l".repeat(n as usize)),
            });
        }
    }
    s
}

fn clamp(sp: i128) -> Scaled {
    sp.clamp(-(MAX_DIMEN as i128), MAX_DIMEN as i128) as Scaled
}

/// `⟨optional signs⟩⟨digits⟩` with TeX's `` ` ``, `"` and `'` prefixes.
pub fn parse_int(s: &str) -> Option<i64> {
    let (sign, rest) = signs(s.trim());
    let rest = rest.trim();
    let mut chars = rest.chars();
    let n = match chars.next()? {
        '"' => i64::from_str_radix(chars.as_str().trim(), 16).ok()?,
        '\'' => i64::from_str_radix(chars.as_str().trim(), 8).ok()?,
        '`' => chars.next().map(|c| c as i64)?,
        _ => {
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if digits.is_empty() {
                return None;
            }
            digits.parse().unwrap_or(MAX_INT)
        }
    };
    Some(sign * n.min(MAX_INT))
}

fn signs(s: &str) -> (i64, &str) {
    let mut sign = 1;
    let mut rest = s;
    loop {
        rest = rest.trim_start();
        match rest.as_bytes().first() {
            Some(b'-') => {
                sign = -sign;
                rest = &rest[1..];
            }
            Some(b'+') => rest = &rest[1..],
            _ => return (sign, rest),
        }
    }
}

/// `⟨optional signs⟩⟨decimal constant⟩⟨unit⟩`, returning scaled points.
/// Font-relative units (`em`, `ex`) and `\fill` are not decidable statically.
pub fn parse_dimen(s: &str) -> Option<Scaled> {
    let (sign, rest) = signs(s.trim());
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ',').collect();
    let unit = rest[digits.len()..].trim().to_ascii_lowercase();
    let (num, den) = decimal(&digits)?;
    let &(_, ratio_n, ratio_d) = UNITS.iter().find(|(u, _, _)| unit.starts_with(u))?;
    // TeX rounds the decimal to scaled points, ties away from zero
    // (tex.web § 102), and then truncates the unit conversion toward zero:
    // `0.1pt` is 6554sp and `1in` is 4736286sp.
    let scaled = (num as i128 * UNIT as i128 * 2 + den as i128) / (den as i128 * 2);
    let sp = scaled * ratio_n as i128 / ratio_d as i128;
    Some(clamp(sign as i128 * sp))
}

fn decimal(s: &str) -> Option<(i64, i64)> {
    let s = s.replace(',', ".");
    let (int, frac) = s.split_once('.').unwrap_or((s.as_str(), ""));
    if int.is_empty() && frac.is_empty() {
        return None;
    }
    let frac: String = frac.chars().take(MAX_DECIMAL_DIGITS).collect();
    let den = 10i64.checked_pow(frac.len() as u32)?;
    let int: i64 = if int.is_empty() { 0 } else { int.parse().ok()? };
    let f: i64 = if frac.is_empty() { 0 } else { frac.parse().ok()? };
    Some((int.checked_mul(den)?.checked_add(f)?, den))
}

/// tex.web § 453: `⟨factor⟩⟨internal unit⟩`, as in `.5\textwidth`, is the
/// unit scaled by the factor. Its integer part multiplies, and its fraction,
/// rounded to 2^-16 the way `scan_decimal` builds it, contributes
/// `xn_over_d(unit, f, 2^16)`.
pub fn scale_by(unit: Scaled, factor: &str) -> Option<Scaled> {
    let (sign, rest) = signs(factor.trim());
    let (num, den) = decimal(rest)?;
    let (whole, part) = (num / den, num % den);
    let f = (part as i128 * UNIT as i128 * 2 + den as i128) / (den as i128 * 2);
    let sp = whole as i128 * unit as i128 + f * unit as i128 / UNIT as i128;
    Some(clamp(sign as i128 * sp))
}

/// `⟨dimen⟩ [plus ⟨dimen⟩] [minus ⟨dimen⟩]` with orders of infinity.
pub fn parse_glue(s: &str) -> Option<Glue> {
    let lower = s.to_ascii_lowercase();
    let cut = |key: &str| lower.find(key);
    let (base, rest) = match cut(" plus").or_else(|| cut(" minus")) {
        Some(i) => (&s[..i], &s[i..]),
        None => (s, ""),
    };
    let mut glue = Glue { width: parse_dimen(base)?, ..Default::default() };
    let lower = rest.to_ascii_lowercase();
    for (key, slot) in [("plus", 0), ("minus", 1)] {
        let Some(i) = lower.find(key) else { continue };
        let tail = &rest[i + key.len()..];
        let end = lower[i + key.len()..].find(if key == "plus" { "minus" } else { "plus" }).unwrap_or(tail.len());
        let part = parse_stretch(&tail[..end])?;
        if slot == 0 {
            glue.stretch = part;
        } else {
            glue.shrink = part;
        }
    }
    Some(glue)
}

pub fn parse_stretch(s: &str) -> Option<Stretch> {
    let lower = s.to_ascii_lowercase();
    if let Some(i) = lower.find("fi") {
        let order = lower[i + 2..].chars().take_while(|c| *c == 'l').count().max(1);
        let (sign, digits) = signs(&s[..i]);
        let (num, den) = decimal(digits.trim()).unwrap_or((1, 1));
        return Some(Stretch {
            amount: clamp(sign as i128 * num as i128 * UNIT as i128 / den as i128),
            order: (order as u8).min(MAX_FIL_ORDER),
        });
    }
    Some(Stretch { amount: parse_dimen(s)?, order: 0 })
}

/// `\numexpr` / `\dimexpr` (ε-TeX): `+ - * /` with left-to-right evaluation
/// and TeX's rounding division.
pub fn eval_expr(s: &str, dimen: bool) -> Option<i64> {
    let mut lexer = ExprLexer { s: s.as_bytes(), i: 0 };
    let v = expr(&mut lexer, dimen)?;
    Some(if dimen { clamp(v as i128) } else { v })
}

struct ExprLexer<'a> {
    s: &'a [u8],
    i: usize,
}

impl ExprLexer<'_> {
    fn skip(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn eat(&mut self, c: u8) -> bool {
        self.skip();
        if self.i < self.s.len() && self.s[self.i] == c {
            self.i += 1;
            return true;
        }
        false
    }
    fn peek(&mut self) -> Option<u8> {
        self.skip();
        self.s.get(self.i).copied()
    }
}

/// ε-TeX stops with "Arithmetic overflow" and yields nothing when a step
/// of an expression leaves the range of its kind (etex manual, `\numexpr`).
fn in_range(v: i64, dimen: bool) -> Option<i64> {
    let max = if dimen { MAX_DIMEN } else { MAX_INT };
    (-max..=max).contains(&v).then_some(v)
}

fn expr(l: &mut ExprLexer, dimen: bool) -> Option<i64> {
    let mut acc = term(l, dimen)?;
    loop {
        match l.peek() {
            Some(b'+') => {
                l.i += 1;
                acc = in_range(acc.checked_add(term(l, dimen)?)?, dimen)?;
            }
            Some(b'-') => {
                l.i += 1;
                acc = in_range(acc.checked_sub(term(l, dimen)?)?, dimen)?;
            }
            _ => return Some(acc),
        }
    }
}

fn term(l: &mut ExprLexer, dimen: bool) -> Option<i64> {
    let mut acc = factor(l, dimen)?;
    loop {
        match l.peek() {
            Some(b'*') => {
                l.i += 1;
                acc = in_range(acc.checked_mul(factor(l, false)?)?, dimen)?;
            }
            Some(b'/') => {
                l.i += 1;
                let d = factor(l, false)?;
                if d == 0 {
                    return None;
                }
                // ε-TeX rounds to nearest, ties away from zero.
                let sign = if (acc < 0) != (d < 0) { -1 } else { 1 };
                acc = sign * ((acc.abs() * 2 + d.abs()) / (d.abs() * 2));
            }
            _ => return Some(acc),
        }
    }
}

fn factor(l: &mut ExprLexer, dimen: bool) -> Option<i64> {
    if l.eat(b'(') {
        let v = expr(l, dimen)?;
        l.eat(b')');
        return Some(v);
    }
    l.skip();
    let start = l.i;
    // A sign may open a factor. Only a later one separates terms.
    if matches!(l.s.get(l.i), Some(b'+' | b'-')) {
        l.i += 1;
    }
    while l.i < l.s.len() && !matches!(l.s[l.i], b'+' | b'-' | b'*' | b'/' | b')') {
        l.i += 1;
    }
    let text = std::str::from_utf8(&l.s[start..l.i]).ok()?.trim();
    if text.is_empty() {
        return None;
    }
    if dimen { parse_dimen(text).or_else(|| parse_int(text).map(|n| n * UNIT)) } else { parse_int(text) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units() {
        assert_eq!(parse_dimen("1pt"), Some(65536));
        assert_eq!(parse_dimen("1in"), Some(4736286));
        assert_eq!(parse_dimen("1pc"), Some(786432));
        assert_eq!(parse_dimen("-0.5pt"), Some(-32768));
        assert_eq!(parse_dimen("1sp"), Some(1));
        assert_eq!(parse_dimen("1em"), None);
    }

    #[test]
    fn numbers() {
        assert_eq!(parse_int("`a"), Some(97));
        assert_eq!(parse_int("\"FF"), Some(255));
        assert_eq!(parse_int("'10"), Some(8));
        assert_eq!(parse_int("--3"), Some(3));
    }

    #[test]
    fn glue() {
        let g = parse_glue("10pt plus 2pt minus 1fil").unwrap();
        assert_eq!(g.width, 10 * UNIT);
        assert_eq!(g.stretch, Stretch { amount: 2 * UNIT, order: 0 });
        assert_eq!(g.shrink, Stretch { amount: UNIT, order: 1 });
    }

    #[test]
    fn expressions() {
        assert_eq!(eval_expr("2*3+4", false), Some(10));
        assert_eq!(eval_expr("7/2", false), Some(4));
        assert_eq!(eval_expr("1pt*2", true), Some(2 * UNIT));
    }

    /// Abstract numbers around TeX's edges, each with the concrete values
    /// it stands for.
    fn nums() -> Vec<(Num, Vec<i64>)> {
        let mut out = Vec::new();
        for c in [0i64, 1, -1, 5, -5, MAX_DIMEN, -MAX_DIMEN, MAX_INT, WORD_MIN, WORD_MIN + 1] {
            for w in [0i64, 1, 2] {
                let (lo, hi) = ((c - w).max(WORD_MIN), (c + w).min(WORD_MAX));
                out.push((Num::range(lo, hi), (lo..=hi).collect()));
            }
        }
        out
    }

    /// Whether `num` stands for `v`: inside the interval.
    fn holds(num: &Num, v: i64) -> bool {
        num.lo <= v && v <= num.hi
    }

    /// A [`Num`] keeps what it promises: `lo <= hi`.
    fn well_formed(num: &Num) {
        assert!(num.lo <= num.hi, "{num:?}");
    }

    #[test]
    fn join_is_an_upper_bound_of_both() {
        for (a, xs) in nums() {
            for (b, ys) in nums() {
                let j = a.join(&b);
                well_formed(&j);
                for v in xs.iter().chain(&ys) {
                    assert!(holds(&j, *v), "{v} lost joining {a:?} and {b:?}");
                }
            }
        }
    }

    #[test]
    fn widening_only_grows() {
        for (a, xs) in nums() {
            for (b, ys) in nums() {
                let w = a.widen(&a.join(&b));
                well_formed(&w);
                for v in xs.iter().chain(&ys) {
                    assert!(holds(&w, *v), "{v} lost widening {a:?} past {b:?}");
                }
            }
        }
    }

    #[test]
    fn negation_covers_every_member() {
        for (a, xs) in nums() {
            let n = a.neg();
            well_formed(&n);
            for v in &xs {
                assert!(holds(&n, -v), "{v} negated is lost from {a:?}");
            }
        }
    }

    /// A comparison is decided only when every pair of concrete values
    /// decides it alike.
    #[test]
    fn a_decided_comparison_holds_for_every_pair() {
        for (a, xs) in nums() {
            for (b, ys) in nums() {
                for relation in ['<', '>', '='] {
                    let Some(answer) = a.compare(relation, &b) else { continue };
                    for &x in &xs {
                        for &y in &ys {
                            let concrete = match relation {
                                '<' => x < y,
                                '>' => x > y,
                                _ => x == y,
                            };
                            assert_eq!(concrete, answer, "{relation} on {a:?} and {b:?} at ({x}, {y})");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn a_decided_parity_holds_for_every_member() {
        for (a, xs) in nums() {
            let Some(answer) = a.odd() else { continue };
            for v in &xs {
                assert_eq!(v % 2 != 0, answer, "parity of {a:?} at {v}");
            }
        }
    }

    /// Narrowing keeps every value the test leaves, and no arm loses one.
    #[test]
    fn narrowing_keeps_every_value_the_test_leaves() {
        for (a, xs) in nums() {
            for bound in [0i64, 1, -1, 5, MAX_DIMEN, WORD_MIN, MAX_INT] {
                for relation in ['<', '>', '='] {
                    for holds_test in [false, true] {
                        let narrowed = a.narrow(relation, bound, holds_test);
                        if let Some(n) = &narrowed {
                            well_formed(n);
                        }
                        for &v in &xs {
                            let concrete = match relation {
                                '<' => v < bound,
                                '>' => v > bound,
                                _ => v == bound,
                            };
                            if concrete != holds_test {
                                continue;
                            }
                            let n = narrowed
                                .as_ref()
                                .unwrap_or_else(|| panic!("{a:?} {relation} {bound} = {holds_test} dropped {v}"));
                            assert!(holds(n, v), "{v} lost narrowing {a:?} by {relation} {bound} = {holds_test}");
                        }
                    }
                }
            }
        }
    }

    /// `\advance`, `\multiply` and `\divide` on abstract numbers cover
    /// every concrete result, including the `negate` wrap at -2^31 that
    /// `Machine::abs_arith` joins `[-2^31, 0]` for (tex.web §§ 105-107,
    /// 1238-1240).
    #[test]
    fn register_arithmetic_covers_every_concrete_result() {
        let ns = nums();
        for (a, xs) in &ns {
            for (b, ys) in &ns {
                for (name, range, wrap_on, exact, ideal) in ops() {
                    let (got, err) = Num::lift(&[a, b], range, wrap_on, exact, ideal);
                    let mut got = got;
                    // `Machine::abs_arith`: -2^31 negated stays negative, so
                    // the corners do not bound the 32-bit answer.
                    let enumerated = a.lo == a.hi && b.lo == b.hi;
                    if name != "advance" && !enumerated && (a.lo == WORD_MIN || b.lo == WORD_MIN) {
                        got = got.map(|n| n.join(&Num::range(WORD_MIN, 0)));
                    }
                    for &x in xs {
                        for &y in ys {
                            match exact(&[x, y]) {
                                Some(v) => {
                                    let n = got
                                        .as_ref()
                                        .unwrap_or_else(|| panic!("{name}({x}, {y}) = {v}, abstract says error"));
                                    assert!(holds(n, v), "{name}({x}, {y}) = {v}, not in {n:?}");
                                }
                                None => assert!(err, "{name}({x}, {y}) errs, not flagged"),
                            }
                        }
                    }
                }
            }
        }
    }

    #[allow(clippy::type_complexity)]
    fn ops() -> Vec<(&'static str, (i64, i64), bool, fn(&[i64]) -> Option<i64>, fn(&[i64]) -> Option<i64>)> {
        vec![
            ("advance", (WORD_MIN, WORD_MAX), true, |v| Some(wrap(v[0] + v[1])), |v| Some(v[0] + v[1])),
            ("multiply", (-MAX_INT, MAX_INT), false, |v| mult_and_add(v[0], v[1], 0, MAX_INT), |v| Some(v[0] * v[1])),
            (
                "multiply-dimen",
                (-MAX_DIMEN, MAX_DIMEN),
                false,
                |v| mult_and_add(v[0], v[1], 0, MAX_DIMEN),
                |v| Some(v[0] * v[1]),
            ),
            ("divide", (WORD_MIN, WORD_MAX), true, |v| x_over_n(v[0], v[1]), |v| (v[1] != 0).then(|| v[0] / v[1])),
        ]
    }
}
