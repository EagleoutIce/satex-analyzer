//! TeX's basic scanning routines (tex.web part 26): ⟨number⟩, ⟨dimen⟩,
//! ⟨glue⟩ and keywords, read with `get_x_token` the way `scan_int`,
//! `scan_dimen`, `scan_glue` and `scan_keyword` read them.  A value satex
//! cannot know is `None`, but the tokens that spell it are consumed all the
//! same, so the input stays in step with the engine's.

use crate::builtins::Primitive;
use crate::machine::Machine;
use crate::tex::{Catcode, Meaning, RegKind, Span, Sym, Tok, Token};
use crate::value::{Glue, MAX_DIMEN, MAX_INT, Num, Scaled, Stretch, UNIT, Value, WORD_MAX, WORD_MIN, mult_and_add};

/// tex.web § 438: the radix a constant is written in.
const DECIMAL: i64 = 10;
const OCTAL: i64 = 8;
const HEXADECIMAL: i64 = 16;
/// tex.web § 452: `scan_dimen` keeps at most 17 digits of a fraction.
const MAX_FRACTION_DIGITS: usize = 17;
/// tex.web § 102: `round_decimals` computes in units of 2^-17.
const TWO: i64 = 1 << 17;
/// tex.web § 453: the integer part of a dimension in points must stay below
/// 2^14, and § 448: a dimension below 2^30 scaled points.
const MAX_POINTS: i64 = 1 << 14;
const DIMEN_LIMIT: i64 = 1 << 30;
/// tex.web § 454: `fil`, `fill`, `filll`.
const MAX_FIL_ORDER: u8 = 3;
/// tex.web § 236: `\mag` is 1000 unless changed.
const MAG_UNITY: i64 = 1000;
/// tex.web § 288 `prepare_mag`: a `\mag` outside `1..=32768` is "Illegal
/// magnification" and taken as 1000.
const MAX_MAG: i64 = 32768;
/// tex.web § 442: an improper alphabetic constant counts as `0`.
const IMPROPER_ALPHABETIC: i64 = '0' as i64;
/// tex.web § 458, with pdfTeX's `nd` and `nc`: ⟨unit⟩ as a fraction of a
/// point.  `sp` is handled apart because it takes no fraction.
const UNITS: [(&str, i64, i64); 9] = [
    ("in", 7227, 100),
    ("pc", 12, 1),
    ("cm", 7227, 254),
    ("mm", 7227, 2540),
    ("bp", 7227, 7200),
    ("dd", 1238, 1157),
    ("cc", 14856, 1157),
    ("nd", 685, 642),
    ("nc", 1370, 107),
];

/// tex.web § 102 (`round_decimals`): `.d₁d₂…dₖ` in units of 2^-16.
fn round_decimals(digits: &[i64]) -> i64 {
    let mut a = 0;
    for d in digits.iter().rev() {
        a = (a + d * TWO) / 10;
    }
    (a + 1) / 2
}

/// tex.web § 107 (`xn_over_d`): x·n/d truncated toward zero, and the
/// remainder with the sign of x.
pub fn xn_over_d(x: i64, n: i64, d: i64) -> (i64, i64) {
    let product = x.unsigned_abs() as i128 * n as i128;
    let (q, r) = ((product / d as i128) as i64, (product % d as i128) as i64);
    if x >= 0 { (q, r) } else { (-q, -r) }
}

/// tex.web § 455 `found`: `nx_plus_y(n, v, xn_over_d(v, f, 2^16))` for
/// `n.f` times the unit `v`; an overflow is "Dimension too large" at
/// `attach_sign`, which gives `\maxdimen` before the sign is attached —
/// so `20000` times a negative unit is `+\maxdimen`.
fn scale_unit(n: i64, v: i64, f: i64) -> Scaled {
    mult_and_add(n, v, xn_over_d(v, f, UNIT).0, MAX_DIMEN).unwrap_or(MAX_DIMEN)
}

fn negate(glue: Glue) -> Glue {
    let flip = |s: Stretch| Stretch { amount: -s.amount, order: s.order };
    Glue { width: -glue.width, stretch: flip(glue.stretch), shrink: flip(glue.shrink) }
}

fn glue_of(value: &Value) -> Option<Glue> {
    match value {
        Value::Glue(g) | Value::MuGlue(g) => Some(*g),
        Value::Int(n) | Value::Dimen(n) => Some(Glue { width: *n, ..Default::default() }),
        _ => None,
    }
}

impl Machine<'_> {
    /// `get_x_token` (tex.web § 380): the next token that expansion leaves.
    /// `\noexpand` hands over the token after it unexpanded.
    pub fn next_x_token(&mut self) -> Option<Token> {
        loop {
            let token = self.next_token()?;
            if self.read_noexpanded(token) {
                return Some(token);
            }
            let Some(sym) = token.cs().or_else(|| self.active_cs(token)) else {
                return Some(token);
            };
            let meaning = self.env.meaning(sym);
            if meaning.prim() == Some(Primitive::NoExpand) {
                return self.noexpanded_next();
            }
            if !self.expandable_meaning(&meaning) {
                return Some(token);
            }
            self.unread(token);
            if !self.step() {
                return None;
            }
        }
    }

    /// The token after `\noexpand`, read with the scanner normal (tex.web
    /// § 367), so a file may end before it.
    pub fn noexpanded_next(&mut self) -> Option<Token> {
        let outer = std::mem::replace(&mut self.scanner, crate::machine::Scanner::Normal);
        let next = self.next_token();
        self.scanner = outer;
        next
    }

    /// `cur_cmd = spacer`: a space token, or a control sequence `\let` to one.
    fn is_spacer(&self, token: Token) -> bool {
        match token.tok {
            Tok::Chr(_, Catcode::Space) => true,
            Tok::Cs(sym) => matches!(self.env.meaning(sym), Meaning::Char(_, Catcode::Space)),
            _ => false,
        }
    }

    /// "Scan an optional space" (tex.web § 443).
    fn scan_optional_x_space(&mut self) {
        if let Some(token) = self.next_x_token()
            && !self.is_spacer(token)
        {
            self.unread(token);
        }
    }

    /// The next non-blank token after expansion.
    fn next_non_blank_x_token(&mut self) -> Option<Token> {
        loop {
            let token = self.next_x_token()?;
            if !self.is_spacer(token) {
                return Some(token);
            }
        }
    }

    /// tex.web § 441: optional signs, spaces between them skipped; returns
    /// whether they negate and the first token after them.
    fn scan_signs(&mut self) -> Option<(bool, Token)> {
        let mut negative = false;
        loop {
            let token = self.next_non_blank_x_token()?;
            match token.tok {
                Tok::Chr('-', Catcode::Other) => negative = !negative,
                Tok::Chr('+', Catcode::Other) => {}
                // A name a join left one of several macros that each give
                // signs (pgfmath's `\pgfmath@sign`, empty or `-`): signs
                // of which it is not known whether they negate, so the
                // value is unknown and the number is read on.
                Tok::Cs(sym) if self.signs_only(sym) => self.scan_undecided = true,
                _ => return Some((negative, token)),
            }
        }
    }

    /// Whether `sym` holds one of several meanings, each a macro without
    /// parameters whose text is only signs and spaces.
    fn signs_only(&self, sym: Sym) -> bool {
        self.signs_within(sym, 4)
    }

    /// [`Machine::signs_only`], where a member's text may itself hold such
    /// names, `depth` deep.
    fn signs_within(&self, sym: Sym, depth: u8) -> bool {
        let Some(may) = self.env.slot(sym).and_then(|b| b.may.clone()) else { return false };
        depth > 0
            && may.iter().all(|m| {
                m.as_macro().is_some_and(|m| {
                    m.parameter_text.items.is_empty()
                        && !m.parameter_text.brace_end
                        && m.replacement_text.iter().all(|t| match t.tok {
                            Tok::Chr('-' | '+', Catcode::Other) | Tok::Chr(_, Catcode::Space) => true,
                            Tok::Cs(inner) => self.signs_within(inner, depth - 1),
                            _ => false,
                        })
                })
            })
    }

    /// The level of the internal quantity a meaning stands for (tex.web
    /// § 410: `int_val` … `tok_val`), with `RegKind` naming the level.
    pub fn internal_level(&self, meaning: &Meaning) -> Option<RegKind> {
        let level = |kind: RegKind| match kind {
            RegKind::Box | RegKind::Read | RegKind::Write | RegKind::Char | RegKind::MathChar => RegKind::Count,
            other => other,
        };
        match meaning {
            Meaning::Register(kind, _) => Some(level(*kind)),
            Meaning::Primitive(p) => match p {
                Primitive::Register(kind) | Primitive::Expr(kind) => Some(level(*kind)),
                Primitive::IntegerParameter | Primitive::CatcodeAssign | Primitive::CharCode(_) => Some(RegKind::Count),
                Primitive::DimenParameter => Some(RegKind::Dimen),
                other => Machine::extra_level(*other),
            },
            _ => None,
        }
    }

    /// `scan_something_internal` (tex.web § 413) on a token already read:
    /// `None` when it is not an internal quantity, otherwise its level and
    /// its value, if known.
    fn scan_internal(&mut self, token: Token) -> Option<(RegKind, Option<Value>)> {
        let sym = token.cs().or_else(|| self.active_cs(token))?;
        let level = self.internal_level(&self.env.meaning(sym))?;
        let value = self.internal_quantity(sym, token.span).filter(|v| !matches!(v, Value::Unknown));
        Some((level, value))
    }

    /// ⟨number⟩ (tex.web § 440, `scan_int`).
    pub fn scan_int(&mut self) -> Option<i64> {
        let (negative, token) = self.scan_signs()?;
        self.probe_scans(token, "number");
        // Only a register read as the whole number leaves its members to
        // the test that scans it (see `Machine::register_value`).
        let register = token.cs().is_some_and(|sym| {
            matches!(self.env.meaning(sym), Meaning::Register(..))
                || matches!(
                    self.env.meaning(sym).prim(),
                    Some(
                        Primitive::Register(_)
                            | Primitive::IntegerParameter
                            | Primitive::DimenParameter
                            | Primitive::Expr(_)
                    )
                )
        });
        self.abs = None;
        let magnitude = self.scan_int_from(token).0;
        let abs = self.abs.take().filter(|_| register && magnitude.is_none());
        self.abs = abs.map(|num| if negative { num.neg() } else { num });
        if negative || self.abs.is_none() {
            self.abs_from = None;
        }
        let magnitude = magnitude?;
        // tex.web § 440 `negate` on a 32-bit integer: -(-2^31) is -2^31.
        Some(if negative { crate::value::wrap(-magnitude) } else { magnitude })
    }

    /// The unsigned part of a ⟨number⟩, starting at `token`, and whether it
    /// was a decimal constant whose terminating token is back in the input
    /// (the case in which `scan_dimen` looks for a decimal point).
    fn scan_int_from(&mut self, token: Token) -> (Option<i64>, bool) {
        // A command whose meaning is unknown may be any quantity, or the
        // digits of any number, which a decimal point may follow.
        // Unknown digits are read on with the digits after them (the whole
        // number they are part of is unknown); other unknown text may be
        // anything, a quantity or digits.
        let digits = token.tok == Tok::Cs(self.unknown_digits) || token.tok == Tok::Cs(self.unknown_more);
        if digits {
            return self.scan_digits(DECIMAL, Some(token));
        }
        if self.is_unknown(token) {
            return (None, true);
        }
        match token.tok {
            Tok::Chr('`', Catcode::Other) => (self.scan_alphabetic_constant(), false),
            Tok::Chr('\'', Catcode::Other) => (self.scan_digits(OCTAL, None).0, false),
            Tok::Chr('"', Catcode::Other) => (self.scan_digits(HEXADECIMAL, None).0, false),
            _ => match self.scan_internal(token) {
                Some((RegKind::Toks, _)) => (None, false),
                Some((_, value)) => (value.and_then(|v| v.as_int()), false),
                None => {
                    let (value, backed_up) = self.scan_digits(DECIMAL, Some(token));
                    (value, backed_up)
                }
            },
        }
    }

    /// tex.web § 442: `` `a `` or `` `\a `` is a character code, read without
    /// expansion and followed by an optional space.
    fn scan_alphabetic_constant(&mut self) -> Option<i64> {
        let token = self.next_token()?;
        let code = match token.tok {
            Tok::Chr(c, _) => Some(c as i64),
            Tok::Cs(sym) => {
                let mut chars = self.name(sym).chars();
                chars.next().filter(|_| chars.next().is_none()).map(|c| c as i64)
            }
            Tok::Param(_) => None,
        };
        match code {
            Some(code) => {
                self.scan_optional_x_space();
                Some(code)
            }
            None => {
                self.unread(token);
                Some(IMPROPER_ALPHABETIC)
            }
        }
    }

    fn digit(&self, token: Token, radix: i64) -> Option<i64> {
        match token.tok {
            Tok::Chr(c @ '0'..='9', Catcode::Other) => Some(c as i64 - '0' as i64).filter(|d| *d < radix),
            Tok::Chr(c @ 'A'..='F', Catcode::Other | Catcode::Letter) if radix == HEXADECIMAL => {
                Some(c as i64 - 'A' as i64 + 10)
            }
            _ => None,
        }
    }

    /// tex.web §§ 444-445: the digits of a constant, the first possibly read
    /// already.  One space after them is part of the constant; any other
    /// token that ends it goes back, and then the second result is true.
    /// Past 2^31-1 TeX says "Number too big" and uses the limit.
    fn scan_digits(&mut self, radix: i64, first: Option<Token>) -> (Option<i64>, bool) {
        let mut value: i64 = 0;
        let mut any = false;
        // Unknown digits among the digits make the number unknown.
        let mut unknown = false;
        let mut next = first.or_else(|| self.next_x_token());
        while let Some(token) = next {
            let digits = match token.tok {
                Tok::Chr(crate::tex::UNKNOWN_DIGIT, Catcode::Other) => true,
                Tok::Cs(sym) => sym == self.unknown_digits || sym == self.unknown_more,
                _ => false,
            };
            // Unknown text after a digit may be more digits, so it is read
            // on with them and makes the number unknown; it may also be
            // empty, so it is no digit of its own.
            let maybe = !digits && any && self.is_unknown(token);
            if (digits || maybe) && radix == DECIMAL {
                any |= !maybe && token.tok != Tok::Cs(self.unknown_more);
                unknown = true;
                next = self.next_x_token();
                continue;
            }
            let Some(d) = self.digit(token, radix) else {
                if !any {
                    self.unread(token);
                    return (self.missing_number(Some(token)), true);
                }
                if self.is_spacer(token) {
                    return ((!unknown).then_some(value), false);
                }
                self.unread(token);
                return ((!unknown).then_some(value), true);
            };
            any = true;
            value = (value * radix + d).min(MAX_INT);
            next = self.next_x_token();
        }
        if unknown && any {
            return (None, false);
        }
        (any.then_some(value).or_else(|| self.missing_number(None)), false)
    }

    /// tex.web § 446: "Missing number, treated as zero".
    fn missing_number(&mut self, at: Option<Token>) -> Option<i64> {
        let span = at.map(|t| t.span).unwrap_or_default();
        let seen =
            at.map_or_else(|| "the end of the input".into(), |t| crate::tex::detokenize(&[t], &self.out.interner));
        self.diagnose(
            crate::facts::Severity::Warning,
            "missing-number",
            span,
            format!("missing number before `{seen}`, treated as zero"),
        );
        Some(0)
    }

    /// ⟨dimen⟩ (tex.web § 448, `scan_dimen`), in scaled points.
    pub fn scan_dimension(&mut self) -> Option<Scaled> {
        self.scan_dimen_with(false, false, None).0
    }

    /// ⟨mudimen⟩ (tex.web § 448 with `mu`).
    pub fn scan_mu_dimen(&mut self) -> Option<Scaled> {
        self.scanning_value(|m| m.scan_dimen_with(true, false, None).0)
    }

    /// `scan_dimen(mu, inf, shortcut)`: with `shortcut` the integer part is
    /// already known.  Returns the value and, when `inf`, its order of
    /// infinity.
    fn scan_dimen_with(&mut self, mu: bool, inf: bool, shortcut: Option<Option<i64>>) -> (Option<Scaled>, u8) {
        // Only a dimension register read as the whole value leaves its
        // interval (`Machine::abs`) to the test or assignment that scans it.
        self.abs_whole = false;
        let scanned = self.scan_dimen_parts(mu, inf, shortcut);
        if !std::mem::take(&mut self.abs_whole) {
            self.abs = None;
            self.abs_from = None;
        }
        scanned
    }

    fn scan_dimen_parts(&mut self, mu: bool, inf: bool, shortcut: Option<Option<i64>>) -> (Option<Scaled>, u8) {
        let mut negative = false;
        let mut fraction = 0;
        let integer = match shortcut {
            Some(integer) => integer,
            None => {
                let Some((signs, token)) = self.scan_signs() else { return (None, 0) };
                self.abs = None;
                self.probe_scans(token, if mu { "mudimen" } else { "dimen" });
                negative = signs;
                match self.scan_internal(token) {
                    Some((RegKind::Count, value)) => value.and_then(|v| v.as_int()),
                    Some((RegKind::Toks, _)) => None,
                    // tex.web § 449: an internal dimension or glue is the
                    // whole value; no unit follows.
                    Some((kind, value)) => {
                        let value = value.and_then(|v| v.as_dimen());
                        // A dimension register of unknown value leaves its
                        // interval to the test (see `scan_int`).
                        if value.is_some() || kind != RegKind::Dimen {
                            self.abs = None;
                        }
                        if negative {
                            self.abs = self.abs.take().map(|num| num.neg());
                            self.abs_from = None;
                        }
                        self.abs_whole = self.abs.is_some();
                        return (value.map(|v| self.attach_sign(v, negative)), 0);
                    }
                    None if matches!(token.tok, Tok::Chr('.' | ',', Catcode::Other)) => {
                        let digits = self.scan_fraction();
                        fraction = digits.unwrap_or(0);
                        digits.map(|_| 0)
                    }
                    None => {
                        let (mut integer, backed_up) = self.scan_int_from(token);
                        if backed_up && let Some(point) = self.next_token() {
                            if matches!(point.tok, Tok::Chr('.' | ',', Catcode::Other)) {
                                let digits = self.scan_fraction();
                                fraction = digits.unwrap_or(0);
                                integer = integer.filter(|_| digits.is_some());
                            } else {
                                self.unread(point);
                            }
                        }
                        integer
                    }
                }
            }
        };
        let (integer, negative) = match integer {
            Some(n) if n < 0 => (Some(-n), !negative),
            other => (other, negative),
        };
        let (value, order) = self.scan_units(mu, inf, integer, fraction);
        if negative {
            self.abs = self.abs.take().map(|num| num.neg());
        }
        (value.map(|v| self.attach_sign(v, negative)), order)
    }

    /// tex.web § 452: the digits after a decimal point, as a fraction in
    /// units of 2^-16; `None` when unknown digits are among them.
    fn scan_fraction(&mut self) -> Option<i64> {
        let mut digits = Vec::new();
        let mut known = true;
        while let Some(token) = self.next_x_token() {
            if self.is_unknown(token) {
                known = false;
                continue;
            }
            match self.digit(token, DECIMAL) {
                Some(d) => {
                    if digits.len() < MAX_FRACTION_DIGITS {
                        digits.push(d);
                    }
                }
                None => {
                    if !self.is_spacer(token) {
                        self.unread(token);
                    }
                    break;
                }
            }
        }
        known.then(|| round_decimals(&digits))
    }

    /// A token whose meaning is unknown: it may stand for any text.
    fn is_unknown(&mut self, token: Token) -> bool {
        let sym = token.cs().or_else(|| self.active_cs(token));
        sym.is_some_and(|sym| {
            sym == self.unknown_digits || sym == self.unknown_more || matches!(self.env.meaning(sym), Meaning::Unknown)
        })
    }

    /// tex.web §§ 453-458: the ⟨unit of measure⟩, applied to
    /// `integer + fraction/2^16`.
    fn scan_units(&mut self, mu: bool, inf: bool, integer: Option<i64>, fraction: i64) -> (Option<Scaled>, u8) {
        if inf && self.scan_keyword("fil") {
            let mut order = 1;
            while self.scan_keyword("l") {
                order = (order + 1).min(MAX_FIL_ORDER);
            }
            let value = integer.map(|n| attach_fraction(n, fraction));
            self.scan_optional_x_space();
            return (value, order);
        }
        // tex.web § 455: a unit may be an internal quantity, `em` or `ex`.
        if let Some(token) = self.next_non_blank_x_token() {
            match self.scan_internal(token) {
                Some((kind, unit)) => {
                    let unit = unit.and_then(|v| v.as_dimen());
                    let value = integer.zip(unit).map(|(n, v)| scale_unit(n, v, fraction));
                    // A known factor (at least zero: its sign is attached
                    // after) times a dimension in an interval: the product
                    // is monotone in the dimension, so its interval is that
                    // of the ends' products.
                    let abs = self.abs.take().filter(|_| value.is_none() && kind == RegKind::Dimen);
                    self.abs = abs.and_then(|num| {
                        let n = integer?;
                        // An end whose product is too large is an error
                        // (\maxdimen), which is not monotone: then any.
                        let fits = |v: i64| (n as i128 * v as i128).abs() + (v as i128).abs() < MAX_DIMEN as i128;
                        Some(if fits(num.lo) && fits(num.hi) {
                            crate::value::Num::range(scale_unit(n, num.lo, fraction), scale_unit(n, num.hi, fraction))
                        } else {
                            crate::value::Num::full(true)
                        })
                    });
                    self.abs_from = None;
                    self.abs_whole = self.abs.is_some();
                    return (value, 0);
                }
                None => self.unread(token),
            }
        }
        if mu {
            // tex.web § 456: "Illegal unit of measure (mu inserted)" when the
            // keyword is missing; the value is taken as mu either way.
            self.scan_keyword("mu");
            let value = integer.map(|n| attach_fraction(n, fraction));
            self.scan_optional_x_space();
            return (value, 0);
        }
        // tex.web § 455: `em` and `ex` are the current font's quad and
        // x-height, not scaled by `\mag`.
        let font_unit = if self.scan_keyword("em") {
            Some(6)
        } else if self.scan_keyword("ex") {
            Some(5)
        } else {
            None
        };
        if let Some(k) = font_unit {
            let unit = self.current_font_dimen(k);
            // tex.web § 460: "Dimension too large" gives `\maxdimen`.
            let value = integer.zip(unit).map(|(n, v)| scale_unit(n, v, fraction));
            self.scan_optional_x_space();
            return (value, 0);
        }
        // pdfTeX's `px` is the internal dimension `\pdfpxdimen`.
        if self.scan_keyword("px") {
            let sym = self.intern("pdfpxdimen");
            let unit = self.env.value(sym).as_dimen();
            self.scan_optional_x_space();
            let value = integer.zip(unit).map(|(n, v)| scale_unit(n, v, fraction));
            return (value, 0);
        }
        let (mut integer, mut fraction) = (integer, fraction);
        if self.scan_keyword("true") {
            let sym = self.intern("mag");
            let mag = self.env.value(sym).as_int().filter(|m| (1..=MAX_MAG).contains(m)).unwrap_or(MAG_UNITY);
            if mag != MAG_UNITY {
                integer = integer.map(|n| {
                    let (q, r) = xn_over_d(n, MAG_UNITY, mag);
                    let f = (MAG_UNITY * fraction + UNIT * r) / mag;
                    fraction = f % UNIT;
                    q + f / UNIT
                });
            }
        }
        let value = if self.scan_keyword("pt") {
            integer.map(|n| attach_fraction(n, fraction))
        } else if let Some(&(_, num, denom)) = UNITS.iter().find(|(unit, _, _)| self.scan_keyword(unit)) {
            integer.map(|n| {
                let (q, r) = xn_over_d(n, num, denom);
                let f = (num * fraction + UNIT * r) / denom;
                attach_fraction(q + f / UNIT, f % UNIT)
            })
        } else if self.scan_keyword("sp") {
            integer
        } else {
            // "Illegal unit of measure (pt inserted)" (tex.web § 459).
            integer.map(|n| attach_fraction(n, fraction))
        };
        self.scan_optional_x_space();
        (value, 0)
    }

    /// tex.web § 448 (`attach_sign`): a value of 2^30 or more is "Dimension
    /// too large" and becomes `\maxdimen`.
    fn attach_sign(&self, value: Scaled, negative: bool) -> Scaled {
        let value = if value.abs() >= DIMEN_LIMIT { MAX_DIMEN } else { value };
        if negative { -value } else { value }
    }

    /// ⟨glue⟩ or, with `mu`, ⟨muglue⟩ (tex.web § 461, `scan_glue`).
    pub fn scan_glue_spec(&mut self, mu: bool) -> Option<Glue> {
        let (negative, token) = self.scan_signs()?;
        self.probe_scans(token, if mu { "muglue" } else { "glue" });
        let width = match self.scan_internal(token) {
            Some((RegKind::Skip | RegKind::MuSkip, value)) => {
                let glue = value.as_ref().and_then(glue_of);
                return glue.map(|g| if negative { negate(g) } else { g });
            }
            Some((RegKind::Count, value)) => {
                let integer = value.and_then(|v| v.as_int()).map(|n| if negative { -n } else { n });
                self.scan_dimen_with(mu, false, Some(integer)).0
            }
            Some((RegKind::Toks, _)) => None,
            Some((_, value)) => value.and_then(|v| v.as_dimen()).map(|v| if negative { -v } else { v }),
            None => {
                self.unread(token);
                let width = self.scan_dimen_with(mu, false, None).0;
                width.map(|v| if negative { -v } else { v })
            }
        };
        let mut glue = Glue { width: width.unwrap_or_default(), ..Default::default() };
        let mut known = width.is_some();
        for (keyword, stretch) in [("plus", true), ("minus", false)] {
            if !self.scan_keyword(keyword) {
                continue;
            }
            let (amount, order) = self.scan_dimen_with(mu, true, None);
            known &= amount.is_some();
            let part = Stretch { amount: amount.unwrap_or_default(), order };
            if stretch {
                glue.stretch = part;
            } else {
                glue.shrink = part;
            }
        }
        known.then_some(glue)
    }

    /// tex.web § 407 (`scan_keyword`): the letters of `keyword`, in either
    /// case and any category but active, read with expansion; blanks are
    /// skipped only before the first letter.  On a mismatch everything read
    /// goes back.
    pub fn scan_keyword_x(&mut self, keyword: &str) -> bool {
        let mut read: Vec<Token> = Vec::new();
        let mut wanted = keyword.chars();
        let mut want = wanted.next();
        while let Some(letter) = want {
            let Some(token) = self.next_x_token() else {
                self.unread_all(&read);
                return false;
            };
            if read.is_empty() && !self.is_spacer(token) {
                self.probe_scans(token, keyword);
            }
            match token.tok {
                Tok::Chr(c, cat) if cat != Catcode::Active && (c == letter || c == letter.to_ascii_uppercase()) => {
                    read.push(token);
                    want = wanted.next();
                }
                _ if read.is_empty() && self.is_spacer(token) => {}
                _ => {
                    read.push(token);
                    self.unread_all(&read);
                    return false;
                }
            }
        }
        true
    }
}

/// tex.web § 453 (`attach_fraction`): an integer part of 2^14 points or
/// more is "Dimension too large".
fn attach_fraction(integer: i64, fraction: i64) -> Scaled {
    if integer >= MAX_POINTS { MAX_DIMEN } else { integer * UNIT + fraction }
}

/// `\ifnum`/`\ifdim` on absolute values (pdfTeX's `\ifpdfabsnum`,
/// `\ifpdfabsdim`); unknown when an operand or the relation is.
pub fn compare_abs(a: Option<i64>, relation: Option<char>, b: Option<i64>) -> Option<bool> {
    let (a, b) = (a?.abs(), b?.abs());
    match relation? {
        '<' => Some(a < b),
        '>' => Some(a > b),
        _ => Some(a == b),
    }
}

/// e-TeX's expression operators (etex.ch, `expr_none` … `expr_scale`).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
enum ExprOp {
    None,
    Add,
    Sub,
    Mult,
    Div,
    Scale,
}

/// tex.web § 108 `infinity`.
const INFINITY: i64 = (1 << 31) - 1;

/// etex.ch `add_or_sub`: `x±y` when within `±max_answer`, else `None`
/// (`num_error`).
fn add_or_sub(x: i64, y: i64, max_answer: i64, negative: bool) -> Option<i64> {
    let y = if negative { -y } else { y };
    ((x >= 0 && y <= max_answer - x) || (x < 0 && y >= -max_answer - x)).then_some(x + y)
}

/// etex.ch `quotient`: `n/d` rounded, ties away from zero; `None` for
/// `d = 0`.
fn quotient(n: i64, d: i64) -> Option<i64> {
    if d == 0 {
        return None;
    }
    let negative = (n < 0) != (d < 0);
    let (n, d) = (n.abs(), d.abs());
    let mut a = n / d;
    if 2 * (n - a * d) >= d {
        a += 1;
    }
    Some(if negative { -a } else { a })
}

/// `x*n/d` rounded, ties away from zero, on unbounded integers; `None`
/// for `d = 0`.
fn fract_ideal(x: i64, n: i64, d: i64) -> Option<i64> {
    if d == 0 {
        return None;
    }
    let negative = ((x < 0) != (n < 0)) != (d < 0);
    let (p, d) = ((i128::from(x) * i128::from(n)).abs(), i128::from(d).abs());
    let q = ((2 * p + d) / (2 * d)) as i64;
    Some(if negative { -q } else { q })
}

/// etex.ch `fract`: `x*n/d` rounded, ties away from zero, when at most
/// `max_answer` (its intermediate checks fail only when the result would:
/// every partial sum it tests is below the answer).  `x = 0` is 0 before
/// `n` is looked at.
fn fract(x: i64, n: i64, d: i64, max_answer: i64) -> Option<i64> {
    let q = fract_ideal(x, n, d)?;
    (q.abs() <= max_answer).then_some(q)
}

/// tex.web § 239 `normalize_glue`.
fn normalize_glue(mut g: Glue) -> Glue {
    if g.stretch.amount == 0 {
        g.stretch.order = 0;
    }
    if g.shrink.amount == 0 {
        g.shrink.order = 0;
    }
    g
}

/// An abstract step of `scan_expr` settled the way `num_error` settles a
/// concrete one: an error makes the value 0 and the whole expression 0.
/// `err` is (may err, must err).
fn settle((num, may): (Option<Num>, bool), members: usize, err: &mut (bool, bool)) -> Num {
    match num {
        None => {
            *err = (true, true);
            Num::exact(0)
        }
        Some(num) if may => {
            err.0 = true;
            num.join(&Num::exact(0), members)
        }
        Some(num) => num,
    }
}

fn unbounded() -> (i64, i64) {
    (i64::MIN, i64::MAX)
}

impl Machine<'_> {
    /// e-TeX's `scan_expr`: `\numexpr`, `\dimexpr`, `\glueexpr`,
    /// `\muexpr` (etex.ch "Declare procedures needed for expressions").
    /// Overflow anywhere makes the whole expression zero, with "Arithmetic
    /// overflow".  Alongside the concrete evaluation the natural widths of
    /// the count and dimension levels are evaluated on intervals
    /// ([`Num::lift`]), so `\numexpr` of registers of unknown value is the
    /// interval of its results (0 joined when some may overflow); a glue
    /// expression with an unknown operand is unknown.
    pub(crate) fn scan_expr(&mut self, kind: RegKind) -> Value {
        struct Frame {
            l: RegKind,
            s: ExprOp,
            r: ExprOp,
            e: Glue,
            t: Glue,
            n: i64,
            ea: Num,
            ta: Num,
            na: Num,
        }
        let level = |k: RegKind| match k {
            RegKind::Count => 0,
            RegKind::Dimen => 1,
            RegKind::Skip => 2,
            _ => 3,
        };
        let members = self.env.sets.values;
        let mut stack: Vec<Frame> = Vec::new();
        let mut l = kind;
        let mut arith = false;
        let mut err = (false, false);
        let mut unknown = false;
        let zero = Glue::default();
        let (e, ea) = 'restart: loop {
            let (mut r, mut e, mut s, mut t, mut n) = (ExprOp::None, zero, ExprOp::None, zero, 0i64);
            let (mut ea, mut ta, mut na) = (Num::exact(0), Num::exact(0), Num::exact(0));
            'next: loop {
                let o_level = if s == ExprOp::None { l } else { RegKind::Count };
                let Some(token) = self.next_non_blank_x_token() else { break 'restart (e, ea) };
                if token.is_char('(') && token.is_cat(Catcode::Other) {
                    stack.push(Frame { l, s, r, e, t, n, ea, ta, na });
                    l = o_level;
                    continue 'restart;
                }
                self.unread(token);
                self.abs = None;
                let f = match o_level {
                    RegKind::Count => self.scan_int().map(|v| Glue { width: v, ..zero }),
                    RegKind::Dimen => self.scan_dimension().map(|v| Glue { width: v, ..zero }),
                    RegKind::Skip => self.scan_glue_spec(false),
                    _ => self.scan_glue_spec(true),
                };
                // A register of unknown value leaves its interval; anything
                // else unknown may be any 32-bit number.
                let mut fa = match &f {
                    Some(g) => Num::exact(g.width),
                    None => self.abs.take().unwrap_or_else(|| Num::range(WORD_MIN, WORD_MAX)),
                };
                self.abs = None;
                let mut f = f.unwrap_or_else(|| {
                    unknown = true;
                    zero
                });
                'found: loop {
                    let token = self.next_non_blank_x_token();
                    let mut o = match token {
                        Some(t) if t.is_char('+') && t.is_cat(Catcode::Other) => ExprOp::Add,
                        Some(t) if t.is_char('-') && t.is_cat(Catcode::Other) => ExprOp::Sub,
                        Some(t) if t.is_char('*') && t.is_cat(Catcode::Other) => ExprOp::Mult,
                        Some(t) if t.is_char('/') && t.is_cat(Catcode::Other) => ExprOp::Div,
                        other => {
                            if let Some(t) = other {
                                let relax = t
                                    .cs()
                                    .is_some_and(|sym| matches!(self.env.meaning(sym).prim(), Some(Primitive::Relax)));
                                let close = t.is_char(')') && t.is_cat(Catcode::Other);
                                if stack.is_empty() {
                                    if !relax {
                                        self.unread(t);
                                    }
                                } else if !close {
                                    self.diagnose(
                                        crate::facts::Severity::Warning,
                                        "missing-paren",
                                        t.span,
                                        "missing ) inserted for expression".into(),
                                    );
                                    self.unread(t);
                                }
                            }
                            ExprOp::None
                        }
                    };
                    // "Make sure that f is in the proper range".
                    let lv = level(l);
                    let limit = if lv == 0 || s > ExprOp::Sub { INFINITY } else { MAX_DIMEN };
                    let out = |x: i64| x.abs() > limit;
                    if out(f.width) || (lv >= 2 && s <= ExprOp::Sub && (out(f.stretch.amount) || out(f.shrink.amount)))
                    {
                        arith = true;
                        f = zero;
                    }
                    fa = settle(
                        Num::lift(
                            &[&fa],
                            members,
                            unbounded(),
                            false,
                            |v| (v[0].abs() <= limit).then_some(v[0]),
                            |v| (v[0].abs() <= limit).then_some(v[0]),
                        ),
                        members,
                        &mut err,
                    );
                    let mut num_error = |v: Option<i64>| {
                        v.unwrap_or_else(|| {
                            arith = true;
                            0
                        })
                    };
                    let each = |g: Glue, op: &mut dyn FnMut(i64) -> i64| Glue {
                        width: op(g.width),
                        stretch: Stretch { amount: op(g.stretch.amount), ..g.stretch },
                        shrink: Stretch { amount: op(g.shrink.amount), ..g.shrink },
                    };
                    // "Cases for evaluation of the current term".
                    let max = if lv == 0 { INFINITY } else { MAX_DIMEN };
                    match s {
                        ExprOp::None => {
                            t = if lv >= 2 && o != ExprOp::None { normalize_glue(f) } else { f };
                            ta = fa.clone();
                        }
                        ExprOp::Mult if o == ExprOp::Div => {
                            n = f.width;
                            na = fa.clone();
                            o = ExprOp::Scale;
                        }
                        ExprOp::Mult => {
                            t = each(t, &mut |x| num_error(mult_and_add(x, f.width, 0, max)));
                            let lifted = Num::lift(
                                &[&ta, &fa],
                                members,
                                (-max, max),
                                false,
                                |v| mult_and_add(v[0], v[1], 0, max),
                                |v| Some(v[0] * v[1]),
                            );
                            ta = settle(lifted, members, &mut err);
                        }
                        ExprOp::Div => {
                            t = each(t, &mut |x| num_error(quotient(x, f.width)));
                            let lifted = Num::lift(
                                &[&ta, &fa],
                                members,
                                unbounded(),
                                false,
                                |v| quotient(v[0], v[1]),
                                |v| quotient(v[0], v[1]),
                            );
                            ta = settle(lifted, members, &mut err);
                        }
                        ExprOp::Scale => {
                            t = each(t, &mut |x| num_error(fract(x, n, f.width, max)));
                            let lifted = Num::lift(
                                &[&ta, &na, &fa],
                                members,
                                (-max, max),
                                false,
                                |v| fract(v[0], v[1], v[2], max),
                                |v| fract_ideal(v[0], v[1], v[2]),
                            );
                            ta = settle(lifted, members, &mut err);
                        }
                        ExprOp::Add | ExprOp::Sub => {}
                    }
                    if o > ExprOp::Sub {
                        s = o;
                    } else {
                        // "Evaluate the current expression".
                        s = ExprOp::None;
                        let sub = r == ExprOp::Sub;
                        if r == ExprOp::None {
                            e = t;
                            ea = ta.clone();
                        } else {
                            let max = if lv == 0 { INFINITY } else { MAX_DIMEN };
                            e.width = num_error(add_or_sub(e.width, t.width, max, sub));
                            let lifted = Num::lift(
                                &[&ea, &ta],
                                members,
                                (-max, max),
                                false,
                                |v| add_or_sub(v[0], v[1], max, sub),
                                |v| Some(if sub { v[0] - v[1] } else { v[0] + v[1] }),
                            );
                            ea = settle(lifted, members, &mut err);
                            if lv >= 2 {
                                // e-TeX's quirk: a higher order replaces a
                                // lower one unnegated, even when subtracted.
                                for (x, y) in [(&mut e.stretch, t.stretch), (&mut e.shrink, t.shrink)] {
                                    if x.order == y.order {
                                        x.amount = num_error(add_or_sub(x.amount, y.amount, MAX_DIMEN, sub));
                                    } else if x.order < y.order && y.amount != 0 {
                                        *x = y;
                                    }
                                }
                                e = normalize_glue(e);
                            }
                        }
                        r = o;
                    }
                    if o != ExprOp::None {
                        continue 'next;
                    }
                    let Some(saved) = stack.pop() else { break 'restart (e, ea) };
                    f = e;
                    fa = ea;
                    Frame { l, s, r, e, t, n, ea, ta, na } = saved;
                    continue 'found;
                }
            }
        };
        if arith && !unknown {
            self.diagnose(
                crate::facts::Severity::Warning,
                "arithmetic-overflow",
                Span::default(),
                "arithmetic overflow".into(),
            );
        }
        let dimen = match kind {
            RegKind::Count => false,
            RegKind::Dimen => true,
            _ if unknown => return Value::Unknown,
            _ => {
                let e = if arith { zero } else { e };
                return if kind == RegKind::Skip { Value::Glue(e) } else { Value::MuGlue(e) };
            }
        };
        let num = match err {
            (_, true) => Num::exact(0),
            (true, false) => ea.join(&Num::exact(0), members),
            _ => ea,
        };
        Value::from_num(dimen, num)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{WORD_MAX, WORD_MIN, mult_and_add, wrap, x_over_n};

    const EDGES: [i64; 14] =
        [0, 1, -1, 7, -7, 1 << 15, 46341, 65536, MAX_DIMEN, -MAX_DIMEN, WORD_MAX, WORD_MIN, WORD_MIN + 1, 1 << 30];

    /// Intervals around TeX's edge values, as ranges and as sets.
    fn boxes() -> Vec<(Num, Vec<i64>)> {
        let mut out = Vec::new();
        for &c in &EDGES {
            for w in [0i64, 1, 3, 9] {
                let lo = (c - w).max(WORD_MIN);
                let hi = (c + w).min(WORD_MAX);
                let all: Vec<i64> = (lo..=hi).collect();
                out.push((Num::range(lo, hi), all.clone()));
                out.push((Num { lo, hi, set: Some(vec![lo, hi]) }, vec![lo, hi]));
            }
        }
        out
    }

    /// Every concrete result lies in the abstract one, and every concrete
    /// error is flagged.
    fn sound(args: &[&(Num, Vec<i64>)], got: &(Option<Num>, bool), exact: &dyn Fn(&[i64]) -> Option<i64>) {
        let mut combos = vec![Vec::new()];
        for (_, xs) in args {
            combos =
                combos.iter().flat_map(|c: &Vec<i64>| xs.iter().map(move |&x| [c.clone(), vec![x]].concat())).collect();
        }
        for c in combos {
            match exact(&c) {
                Some(v) => {
                    let num = got.0.as_ref().unwrap_or_else(|| panic!("{c:?} gives {v}, abstract says error"));
                    let inside = num.lo <= v && v <= num.hi && num.set.as_ref().is_none_or(|s| s.contains(&v));
                    assert!(inside, "{c:?} gives {v}, not in {num:?}");
                }
                None => assert!(got.1, "{c:?} errs, not flagged"),
            }
        }
    }

    #[test]
    fn lifted_arithmetic_covers_every_concrete_result() {
        let bs = boxes();
        for a in &bs {
            for b in &bs {
                let two = [a, b];
                let lift2 = |range,
                             wrap_: bool,
                             exact: &dyn Fn(&[i64]) -> Option<i64>,
                             ideal: &dyn Fn(&[i64]) -> Option<i64>| {
                    let got = Num::lift(&[&a.0, &b.0], 64, range, wrap_, exact, ideal);
                    sound(&two, &got, exact);
                };
                // \advance, \multiply (count and dimen), \divide.
                lift2((WORD_MIN, WORD_MAX), true, &|v| Some(wrap(v[0] + v[1])), &|v| Some(v[0] + v[1]));
                for max in [MAX_INT, MAX_DIMEN] {
                    // -2^31 is `\multiply`'s special case (see `abs_arith`).
                    if [a, b].iter().all(|x| x.0.lo > WORD_MIN) {
                        lift2((-max, max), false, &|v| mult_and_add(v[0], v[1], 0, max), &|v| Some(v[0] * v[1]));
                    }
                    // `add_or_sub` only meets range-checked operands.
                    if [a, b].iter().all(|x| -max <= x.0.lo && x.0.hi <= max) {
                        lift2((-max, max), false, &|v| add_or_sub(v[0], v[1], max, true), &|v| Some(v[0] - v[1]));
                    }
                }
                if [a, b].iter().all(|x| x.0.lo > WORD_MIN) {
                    lift2((WORD_MIN, WORD_MAX), true, &|v| x_over_n(v[0], v[1]), &|v| (v[1] != 0).then(|| v[0] / v[1]));
                }
                lift2(unbounded(), false, &|v| quotient(v[0], v[1]), &|v| quotient(v[0], v[1]));
            }
        }
        // fract, on fewer boxes: three arguments.
        let few: Vec<_> = bs.iter().filter(|b| b.1.len() <= 3).step_by(2).collect();
        for a in &few {
            for b in &few {
                for d in &few {
                    let exact = |v: &[i64]| fract(v[0], v[1], v[2], MAX_DIMEN);
                    let got = Num::lift(&[&a.0, &b.0, &d.0], 64, (-MAX_DIMEN, MAX_DIMEN), false, exact, |v| {
                        fract_ideal(v[0], v[1], v[2])
                    });
                    sound(&[a, b, d], &got, &exact);
                }
            }
        }
    }

    #[test]
    fn fract_and_quotient_round_ties_away_from_zero() {
        assert_eq!(quotient(7, 2), Some(4));
        assert_eq!(quotient(-7, 2), Some(-4));
        assert_eq!(quotient(5, -2), Some(-3));
        assert_eq!(quotient(1, 0), None);
        assert_eq!(fract(7, 11, 3, INFINITY), Some(26));
        assert_eq!(fract(INFINITY, 2, 2, INFINITY), Some(INFINITY));
        assert_eq!(fract(65536, 32768, 2, INFINITY), Some(1 << 30));
        assert_eq!(fract(1, -1, 2, INFINITY), Some(-1));
        assert_eq!(fract(INFINITY, 2, 1, INFINITY), None);
        assert_eq!(mult_and_add(-65536, 32768, 0, INFINITY), None);
        assert_eq!(mult_and_add(1, WORD_MIN, 0, INFINITY), Some(WORD_MIN));
        assert_eq!(x_over_n(WORD_MIN, -1), Some(WORD_MIN));
        assert_eq!(x_over_n(-7, 2), Some(-3));
    }
}
