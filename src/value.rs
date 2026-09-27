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
/// The range of TeX's `integer` (32 bits in web2c).  Additions without an
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
    /// A colour, as xcolor stores one in `\\color@⟨name⟩` (xcolor § 2.5).
    Color(Colors),
    /// A count (`dimen: false`) or dimension whose value is not known but
    /// lies in an interval, and is one of a few values when `set` says so:
    /// what a join, arithmetic on such a value, or a typesetting result
    /// leaves.  Read as a number it is unknown ([`crate::env::Env::value`]
    /// reads it so). A test that looks at it decides by the interval.
    Range {
        dimen: bool,
        num: Num,
    },
}

/// How many values a [`Num`] lists before it is only an interval, unless
/// `limits.value_set` says otherwise.
pub const MAY_VALUES: usize = 5;

/// An abstract number (a count or a dimension in scaled points): the
/// interval `[lo, hi]` it lies in and, when they are few, the values it may
/// be.  TeX's numbers are bounded (`MAX_INT`, `MAX_DIMEN`), so the full
/// interval of its kind is the top of the lattice, which widening reaches.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Num {
    pub lo: i64,
    pub hi: i64,
    /// Sorted, deduplicated, within `[lo, hi]` and holding both ends.
    pub set: Option<Vec<i64>>,
}

impl Num {
    pub fn exact(n: i64) -> Num {
        Num { lo: n, hi: n, set: Some(vec![n]) }
    }

    pub fn range(lo: i64, hi: i64) -> Num {
        Num { lo: lo.min(hi), hi: hi.max(lo), set: None }
    }

    /// Every value of the kind: what an unknown typesetting result is.
    pub fn full(dimen: bool) -> Num {
        let max = if dimen { MAX_DIMEN } else { MAX_INT };
        Num::range(-max, max)
    }

    /// Exactly the values in `set`, however many they are.
    pub fn of_set(set: Vec<i64>) -> Num {
        Num::of_set_capped(set, usize::MAX)
    }

    fn of_set_capped(mut set: Vec<i64>, members: usize) -> Num {
        set.sort_unstable();
        set.dedup();
        let (lo, hi) = (set[0], set[set.len() - 1]);
        Num { lo, hi, set: (set.len() <= members).then_some(set) }
    }

    /// Least upper bound, listing at most `members` values.
    pub fn join(&self, other: &Num, members: usize) -> Num {
        match (&self.set, &other.set) {
            (Some(a), Some(b)) => Num::of_set_capped(a.iter().chain(b).copied().collect(), members),
            _ => Num::range(self.lo.min(other.lo), self.hi.max(other.hi)),
        }
    }

    /// Widening: a bound that grew goes to the end of TeX's 32-bit range,
    /// not `\maxdimen`, which `\advance` may pass (tex.web § 1238), so a
    /// loop that keeps advancing a dimension stays covered.
    pub fn widen(&self, next: &Num, _dimen: bool) -> Num {
        let lo = if next.lo < self.lo { WORD_MIN } else { self.lo };
        let hi = if next.hi > self.hi { WORD_MAX } else { self.hi };
        Num::range(lo, hi)
    }

    pub fn neg(&self) -> Num {
        match &self.set {
            Some(set) => Num::of_set_capped(set.iter().map(|n| -n).collect(), usize::MAX),
            None => Num::range(-self.hi, -self.lo),
        }
    }

    /// TeX arithmetic lifted to abstract numbers: the values `exact` gives
    /// on every combination of `args`, and whether a combination may be
    /// TeX's `arith_error` (`exact` is `None`).  Few members: each
    /// combination.  Otherwise `ideal`, the operation on unbounded
    /// integers (`None` only at a zero divisor), is taken at the corners
    /// of the box with every argument cut at zero: each operation here is
    /// monotone in each argument on a part of constant sign (a product is
    /// bilinear, `x/n` and its roundings are monotone in both), so the
    /// corners bound it.  An ideal result outside `range` is an error
    /// (then the in-range results lie in the clipped hull) or, with
    /// `wrap`, a 32-bit wrap-around (then any 32-bit number).  `None`
    /// when every combination is an error.
    pub fn lift(
        args: &[&Num],
        members: usize,
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
        let sets: Option<Vec<&Vec<i64>>> = args.iter().map(|a| a.set.as_ref()).collect();
        let combos = |pts: &[Vec<i64>]| -> Vec<Vec<i64>> {
            pts.iter().fold(vec![Vec::new()], |acc, xs| {
                acc.iter().flat_map(|c| xs.iter().map(move |&x| [c.clone(), vec![x]].concat())).collect()
            })
        };
        if let Some(sets) = sets.filter(|s| s.iter().map(|v| v.len()).product::<usize>() <= 64) {
            let pts: Vec<Vec<i64>> = sets.into_iter().cloned().collect();
            let mut out = Vec::new();
            let mut err = false;
            for c in combos(&pts) {
                match exact(&c) {
                    Some(v) => out.push(v),
                    None => err = true,
                }
            }
            return ((!out.is_empty()).then(|| Num::of_set_capped(out, members)), err);
        }
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

    /// `self relation other` when every pair of values gives the same
    /// answer. `None` when they differ.
    pub fn compare(&self, relation: char, other: &Num) -> Option<bool> {
        if let (Some(a), Some(b)) = (&self.set, &other.set) {
            let mut answers = a.iter().flat_map(|&x| b.iter().map(move |&y| (x, y))).map(|(x, y)| match relation {
                '<' => x < y,
                '>' => x > y,
                _ => x == y,
            });
            let first = answers.next()?;
            return answers.all(|v| v == first).then_some(first);
        }
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

    /// Whether it is odd, when every value agrees.
    pub fn odd(&self) -> Option<bool> {
        if self.lo == self.hi {
            return Some(self.lo % 2 != 0);
        }
        match &self.set {
            Some(set) => {
                let first = set[0] % 2 != 0;
                set.iter().all(|n| (n % 2 != 0) == first).then_some(first)
            }
            None => None,
        }
    }

    /// What `self relation bound` being `holds` leaves of `self`: the
    /// narrowing a test teaches the arm it chooses.  `None` when no value
    /// satisfies it.
    pub fn narrow(&self, relation: char, bound: i64, holds: bool) -> Option<Num> {
        let keep = |n: i64| {
            let v = match relation {
                '<' => n < bound,
                '>' => n > bound,
                _ => n == bound,
            };
            v == holds
        };
        if let Some(set) = &self.set {
            let set: Vec<i64> = set.iter().copied().filter(|&n| keep(n)).collect();
            return (!set.is_empty()).then(|| Num::of_set_capped(set, usize::MAX));
        }
        let (lo, hi) = match (relation, holds) {
            ('<', true) => (self.lo, self.hi.min(bound.saturating_sub(1))),
            ('<', false) => (self.lo.max(bound), self.hi),
            ('>', true) => (self.lo.max(bound.saturating_add(1)), self.hi),
            ('>', false) => (self.lo, self.hi.min(bound)),
            ('=', true) => (bound.max(self.lo), bound.min(self.hi)),
            _ => (self.lo, self.hi),
        };
        (lo <= hi).then(|| Num::range(lo, hi))
    }
}

/// One of xcolor's colour models (xcolor, table 3), with the two pseudo
/// models that name a colour instead of measuring one.
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
    /// the model is scaled by one.  `None` for a model without components.
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

/// A colour as xcolor holds one: a model and one abstract number per
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

    /// The colour exactly, when every component is one value.
    pub fn exact(&self) -> Option<Vec<Scaled>> {
        self.components.iter().map(|c| (c.lo == c.hi).then_some(c.lo)).collect()
    }

    /// `1,0.50195,0`, as xcolor writes the spec.
    pub fn spell(&self) -> String {
        self.components.iter().map(spell_component).collect::<Vec<_>>().join(",")
    }

    /// The component that lies outside the model's range, with the range,
    /// when one does on every path.
    pub fn out_of_range(&self) -> Option<(usize, Num, (Scaled, Scaled))> {
        self.components.iter().enumerate().find_map(|(i, c)| {
            let (lo, hi) = self.model.bounds(i)?;
            (c.lo < lo || c.hi > hi).then(|| (i, c.clone(), (lo, hi)))
        })
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

/// What a colour-valued binding may hold: one model, each component bounded
/// by a [`Num`], and the exact colours it may be while they are few.
/// [`Num`]'s own shape lifted to a tuple, so a join keeps a set and a join
/// past the limit keeps the component bounds alone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Colors {
    /// Component-wise bounds over every member.
    pub hull: Color,
    /// The colours it may be, each of `hull`'s model.
    pub set: Option<Rc<[Rc<[Num]>]>>,
}

impl Colors {
    pub fn exact(color: Color) -> Colors {
        let members = Rc::from([Rc::clone(&color.components)]);
        Colors { hull: color, set: Some(members) }
    }

    pub fn model(&self) -> Model {
        self.hull.model
    }

    /// Least upper bound, listing at most `members` colours.  Two models
    /// have no common bound: xcolor converts between them through
    /// `\XC@calc@`, which rounds to scaled points, so a converted spec is
    /// not the spec the other model would have stored, and claiming it
    /// would invent a value.  `None` is that, and the caller goes to
    /// [`Value::Unknown`].
    pub fn join(&self, other: &Colors, members: usize) -> Option<Colors> {
        if self.hull.model != other.hull.model {
            return None;
        }
        let arity = self.hull.components.len();
        if arity != other.hull.components.len() {
            return None;
        }
        let hull: Vec<Num> =
            (0..arity).map(|i| self.hull.components[i].join(&other.hull.components[i], members)).collect();
        let set = match (&self.set, &other.set) {
            (Some(a), Some(b)) => {
                let mut out: Vec<Rc<[Num]>> = a.to_vec();
                for member in b.iter() {
                    if !out.iter().any(|m| m == member) {
                        out.push(Rc::clone(member));
                    }
                }
                (out.len() <= members).then(|| Rc::from(out))
            }
            _ => None,
        };
        Some(Colors { hull: Color::new(self.hull.model, hull), set })
    }
}

/// `⟨decimal constant⟩` as xcolor's arithmetic holds it: scaled points,
/// rounded the way `scan_decimal` rounds (tex.web § 102).
pub fn parse_component(s: &str) -> Option<Scaled> {
    scale_by(UNIT, s.trim())
}

/// `⟨model⟩{⟨spec⟩}` as the run wrote it, with each component an exact
/// [`Num`].  `None` when the model carries no components or the spec is not
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
            (Value::Range { dimen: x, num: a }, Value::Range { dimen: y, num: b }) => x == y && a == b,
            (Value::Color(a), Value::Color(b)) => a == b,
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

    /// Least upper bound: equal values stay, different counts or
    /// different dimensions become their [`Num`] join (listing at most
    /// `members` values), anything else unknown.
    pub fn join(self, other: &Value, members: usize) -> Value {
        if &self == other {
            return self;
        }
        match (self.num(), other.num()) {
            (Some((x, a)), Some((y, b))) if x == y => Value::Range { dimen: x, num: a.join(&b, members) },
            _ => match (&self, other) {
                (Value::Color(a), Value::Color(b)) => a.join(b, members).map_or(Value::Unknown, Value::Color),
                _ => Value::Unknown,
            },
        }
    }

    /// The colour this holds, when it is one.
    pub fn as_color(&self) -> Option<&Colors> {
        match self {
            Value::Color(c) => Some(c),
            _ => None,
        }
    }

    /// The abstract number this is, with whether it is a dimension: a
    /// known count or dimension exactly, a [`Value::Range`] as it stands.
    pub fn num(&self) -> Option<(bool, Num)> {
        match self {
            Value::Int(n) => Some((false, Num::exact(*n))),
            Value::Dimen(d) => Some((true, Num::exact(*d))),
            Value::Range { dimen, num } => Some((*dimen, num.clone())),
            _ => None,
        }
    }

    /// The value an abstract number stands for: known when it is one.
    pub fn from_num(dimen: bool, num: Num) -> Value {
        match (num.lo == num.hi, dimen) {
            (true, false) => Value::Int(num.lo),
            (true, true) => Value::Dimen(num.lo),
            (false, _) => Value::Range { dimen, num },
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
            Value::Unknown | Value::Range { .. } => "?".into(),
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
                let all: Vec<i64> = (lo..=hi).collect();
                out.push((Num::range(lo, hi), all.clone()));
                out.push((Num::of_set_capped(all.clone(), MAY_VALUES), all));
            }
        }
        out.push((Num::of_set_capped(vec![-7, 0, 9], MAY_VALUES), vec![-7, 0, 9]));
        out.push((Num::of_set_capped(vec![WORD_MIN, 0, MAX_INT], MAY_VALUES), vec![WORD_MIN, 0, MAX_INT]));
        out
    }

    /// Whether `num` stands for `v`: inside the interval, and listed when
    /// there is a list.
    fn holds(num: &Num, v: i64) -> bool {
        num.lo <= v && v <= num.hi && num.set.as_ref().is_none_or(|s| s.contains(&v))
    }

    /// A [`Num`] keeps what it promises: sorted, deduplicated, inside the
    /// interval, and holding both ends.
    fn well_formed(num: &Num) {
        assert!(num.lo <= num.hi, "{num:?}");
        if let Some(set) = &num.set {
            assert!(!set.is_empty() && set.windows(2).all(|w| w[0] < w[1]), "{num:?}");
            assert_eq!((set[0], set[set.len() - 1]), (num.lo, num.hi), "{num:?}");
        }
    }

    #[test]
    fn join_is_an_upper_bound_of_both() {
        for (a, xs) in nums() {
            for (b, ys) in nums() {
                for members in [0, 1, MAY_VALUES, 64] {
                    let j = a.join(&b, members);
                    well_formed(&j);
                    for v in xs.iter().chain(&ys) {
                        assert!(holds(&j, *v), "{v} lost joining {a:?} and {b:?} at {members}");
                    }
                }
            }
        }
    }

    #[test]
    fn widening_only_grows() {
        for (a, xs) in nums() {
            for (b, ys) in nums() {
                for dimen in [false, true] {
                    let w = a.widen(&a.join(&b, MAY_VALUES), dimen);
                    well_formed(&w);
                    for v in xs.iter().chain(&ys) {
                        assert!(holds(&w, *v), "{v} lost widening {a:?} past {b:?}");
                    }
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
                    let (got, err) = Num::lift(&[a, b], MAY_VALUES, range, wrap_on, exact, ideal);
                    let mut got = got;
                    // `Machine::abs_arith`: -2^31 negated stays negative, so
                    // the corners do not bound the 32-bit answer.
                    let enumerated = match (&a.set, &b.set) {
                        (Some(p), Some(q)) => p.len() * q.len() <= 64,
                        _ => a.lo == a.hi && b.lo == b.hi,
                    };
                    if name != "advance" && !enumerated && (a.lo == WORD_MIN || b.lo == WORD_MIN) {
                        got = got.map(|n| n.join(&Num::range(WORD_MIN, 0), MAY_VALUES));
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
