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
/// integers; `None` (`arith_error`) when `n` is zero.
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
    // Math units are stored in the same scaled units as points; only the
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
    /// A count (`dimen: false`) or dimension whose value is not known but
    /// lies in an interval, and is one of a few values when `set` says so:
    /// what a join, arithmetic on such a value, or a typesetting result
    /// leaves.  Read as a number it is unknown ([`crate::env::Env::value`]
    /// reads it so); a test that looks at it decides by the interval.
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

    fn of_set(mut set: Vec<i64>, members: usize) -> Num {
        set.sort_unstable();
        set.dedup();
        let (lo, hi) = (set[0], set[set.len() - 1]);
        Num { lo, hi, set: (set.len() <= members).then_some(set) }
    }

    /// Least upper bound, listing at most `members` values.
    pub fn join(&self, other: &Num, members: usize) -> Num {
        match (&self.set, &other.set) {
            (Some(a), Some(b)) => Num::of_set(a.iter().chain(b).copied().collect(), members),
            _ => Num::range(self.lo.min(other.lo), self.hi.max(other.hi)),
        }
    }

    /// Widening: a bound that grew goes to the end of TeX's 32-bit range
    /// — not `\maxdimen`, which `\advance` may pass (tex.web § 1238), so a
    /// loop that keeps advancing a dimension stays covered.
    pub fn widen(&self, next: &Num, _dimen: bool) -> Num {
        let lo = if next.lo < self.lo { WORD_MIN } else { self.lo };
        let hi = if next.hi > self.hi { WORD_MAX } else { self.hi };
        Num::range(lo, hi)
    }

    pub fn neg(&self) -> Num {
        match &self.set {
            Some(set) => Num::of_set(set.iter().map(|n| -n).collect(), usize::MAX),
            None => Num::range(-self.hi, -self.lo),
        }
    }

    /// `op` on every pair of members when both are few, else on the
    /// corners of the intervals (every `op` here is monotone in each
    /// argument on a sign-constant part); `None` for a pair means TeX's
    /// arithmetic error, which leaves the register as it was (`keep`);
    /// `max` bounds the kind.
    pub fn apply(
        &self,
        other: &Num,
        keep: &Num,
        max: i64,
        members: usize,
        op: impl Fn(i64, i64) -> Option<i64>,
    ) -> Num {
        if let (Some(a), Some(b)) = (&self.set, &other.set)
            && a.len() * b.len() <= 64
        {
            let mut out = Vec::new();
            let mut kept = false;
            for &x in a {
                for &y in b {
                    match op(x, y) {
                        Some(v) => out.push(v),
                        None => kept = true,
                    }
                }
            }
            let num = if out.is_empty() { keep.clone() } else { Num::of_set(out, members) };
            return if kept { num.join(keep, members) } else { num };
        }
        // Corners, with the divisor's interval cut at zero.
        let mut ys = vec![other.lo, other.hi];
        if other.lo < 0 && other.hi > 0 {
            ys.extend([-1, 1, 0]);
        }
        let mut lo = i64::MAX;
        let mut hi = i64::MIN;
        let mut kept = false;
        for &x in &[self.lo, self.hi] {
            for &y in &ys {
                match op(x, y) {
                    Some(v) => {
                        lo = lo.min(v);
                        hi = hi.max(v);
                    }
                    None => kept = true,
                }
            }
        }
        if lo > hi {
            return keep.clone();
        }
        // An error at a corner (an overflow, or a divisor of zero) may be
        // near any value inside: every value of the kind, and the register
        // kept, are possible.
        if kept { Num::range(-max, max) } else { Num::range(lo, hi) }
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
            return ((!out.is_empty()).then(|| Num::of_set(out, members)), err);
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
    /// answer; `None` when they differ.
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
            return (!set.is_empty()).then(|| Num::of_set(set, usize::MAX));
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

    /// Least upper bound: equal values stay, different known numbers
    /// become the set of them (up to [`MAY_VALUES`]), anything else unknown.
    /// Least upper bound: equal values stay, different counts or
    /// different dimensions become their [`Num`] join (listing at most
    /// `members` values), anything else
    /// unknown.
    pub fn join(self, other: &Value, members: usize) -> Value {
        if &self == other {
            return self;
        }
        match (self.num(), other.num()) {
            (Some((x, a)), Some((y, b))) if x == y => Value::Range { dimen: x, num: a.join(&b, members) },
            _ => Value::Unknown,
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
    // TeX rounds the decimal to scaled points, ties away from zero, and then
    // truncates the unit conversion toward zero: `0.1pt` is 6554sp and `1in`
    // is 4736286sp.
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
/// unit scaled by the factor — its integer part multiplies, and its fraction,
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

fn expr(l: &mut ExprLexer, dimen: bool) -> Option<i64> {
    let mut acc = term(l, dimen)?;
    loop {
        match l.peek() {
            Some(b'+') => {
                l.i += 1;
                acc += term(l, dimen)?;
            }
            Some(b'-') => {
                l.i += 1;
                acc -= term(l, dimen)?;
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
                acc = acc.checked_mul(factor(l, false)?)?;
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
    // A sign may open a factor; only a later one separates terms.
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
}
