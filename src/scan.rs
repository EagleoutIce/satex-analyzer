//! TeX's basic scanning routines (tex.web part 26): ⟨number⟩, ⟨dimen⟩,
//! ⟨glue⟩ and keywords, read with `get_x_token` like `scan_int`,
//! `scan_dimen`, `scan_glue` and `scan_keyword`. A value satex cannot know
//! is `None`, but its tokens are still consumed, keeping the input in step
//! with the engine's.

use crate::builtins::Primitive;
use crate::machine::Machine;
use crate::tex::{Catcode, Meaning, RegKind, Span, Sym, Tok, Token};
use crate::value::{
    Glue, GlueHull, MAX_DIMEN, MAX_INT, Num, Scaled, Stretch, UNIT, Value, WORD_MAX, WORD_MIN, mult_and_add,
};

/// The prefix of the names of unknown digits typed with their interval
/// ([`Machine::digits_in`]).
pub(crate) const TYPED_DIGITS: &str = "\u{2}unknown digits:";

/// The number places `[i, j)` of a `k`-digit number in `num` spell. A
/// length `k` of 0 is one not known, of at least two digits, and `j` of 0
/// then the end.
fn places(num: Num, (k, i, j): (u32, u32, u32)) -> Num {
    if k == 0 {
        return match (i, j) {
            (0, 0) => num,
            (0, j) => Num::range(10i64.pow(j - 1), 10i64.pow(j) - 1),
            (_, 0) => Num::range(0, num.hi.max(0)),
            _ => Num::range(0, 10i64.pow(j - i) - 1),
        };
    }
    let (p, m) = (10i64.pow(k - j), 10i64.pow(j - i));
    let (lo, hi) = (num.lo.max(0) / p, num.hi.max(0) / p);
    match lo / m == hi / m {
        true => Num::range(lo % m, hi % m),
        false => Num::range(0, m - 1),
    }
}

/// The digits a number in `num` may begin with, after its `-` when it is
/// negative: those of its non-negative part and of the magnitudes of its
/// negative part.
pub(crate) fn first_digits(num: Num) -> Vec<char> {
    let mut out: Vec<char> = Vec::new();
    if num.lo < 0 {
        let negative = Num::range(1.max(num.hi.saturating_neg()), num.lo.saturating_neg());
        out.extend(first_digits(negative));
        if num.hi < 0 {
            return out;
        }
    }
    let (lo, hi) = (num.lo.max(0), num.hi.max(0));
    if lo == 0 && !out.contains(&'0') {
        out.push('0');
    }
    // Numbers of one length at a time: their first digits are a run.
    let mut p = 1i64;
    while p <= hi {
        let (a, b) = (lo.max(p), hi.min(p.saturating_mul(10) - 1));
        if a <= b {
            for d in (a / p)..=(b / p) {
                let c = char::from(b'0' + d as u8);
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
        if p > i64::MAX / 10 {
            break;
        }
        p *= 10;
    }
    out
}

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

/// The glue a value holds. A number or dimension widens to glue without
/// stretch or shrink, as `scan_glue` coerces.
pub(crate) fn glue_of(value: &Value) -> Option<Glue> {
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

    /// `get_x_or_protected` (tex.web § 380): as [`Machine::next_x_token`],
    /// but a `\protected` macro is left unexpanded, as where an alignment
    /// looks for what begins a cell or row (§§ 785, 791).
    pub fn next_x_or_protected(&mut self) -> Option<Token> {
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
            if !self.expandable_meaning(&meaning) || matches!(&meaning, Meaning::Macro(m) if m.protected) {
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

    /// `cur_cmd = spacer`: a space token, or a control sequence or active
    /// character `\let` to one (tex.web § 344).
    fn is_spacer(&self, token: Token) -> bool {
        let sym = match token.tok {
            Tok::Chr(_, Catcode::Space) => return true,
            Tok::Cs(sym) => Some(sym),
            Tok::Chr(c, Catcode::Active) => self.out.interner.lookup(&format!("{}{c}", crate::tex::ACTIVE)),
            _ => None,
        };
        sym.is_some_and(|sym| matches!(self.env.meaning(sym), Meaning::Char(_, Catcode::Space)))
    }

    /// `cur_cmd = relax`: a token meaning `\relax`, or an expandable one
    /// behind `\noexpand`'s marker (tex.web § 358).
    fn is_relax(&mut self, token: Token) -> bool {
        let Some(sym) = self.active_cs(token).or(token.cs()) else { return false };
        let meaning = self.env.meaning(sym);
        matches!(meaning.prim(), Some(Primitive::Relax))
            || (self.read_noexpanded(token) && self.expandable_meaning(&meaning))
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
    pub(crate) fn next_non_blank_x_token(&mut self) -> Option<Token> {
        loop {
            let token = self.next_x_token()?;
            if !self.is_spacer(token) {
                return Some(token);
            }
        }
    }

    /// tex.web § 441: optional signs, spaces between them skipped.
    /// Returns whether they negate and the first token after them.
    fn scan_signs(&mut self) -> Option<(bool, Token)> {
        let mut negative = false;
        loop {
            let token = self.next_non_blank_x_token()?;
            match token.tok {
                Tok::Chr('-', Catcode::Other) => negative = !negative,
                Tok::Chr('+', Catcode::Other) => {}
                // A name-a-join may resolve to one of several sign macros
                // (pgfmath's `\pgfmath@sign`, empty or `-`). Not knowing
                // which makes the negation unknown, so the number is read on.
                Tok::Cs(sym) if self.signs_only(sym) => self.scan_undecided = true,
                // A digit or `-` (`\the` of an unknown dimension): the sign
                // is unknown, and so is the number its digits start.
                Tok::Chr(crate::tex::UNKNOWN_CHAR, Catcode::Other) => {
                    self.scan_undecided = true;
                    return Some((
                        negative,
                        Token::new(Tok::Chr(crate::tex::UNKNOWN_DIGIT, Catcode::Other), token.span),
                    ));
                }
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
            RegKind::Box | RegKind::Read | RegKind::Write | RegKind::Char | RegKind::MathChar | RegKind::UMathChar => {
                RegKind::Count
            }
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
        let register = token.tok == Tok::Chr('`', Catcode::Other)
            || token.cs().is_some_and(|sym| {
                self.typed_digits_of(sym).is_some()
                    || matches!(self.env.meaning(sym), Meaning::Register(..))
                    || matches!(
                        self.env.meaning(sym).prim(),
                        Some(
                            Primitive::Register(_)
                                | Primitive::LastItem(_)
                                | Primitive::IntegerParameter
                                | Primitive::DimenParameter
                                | Primitive::Expr(_)
                        )
                    )
            });
        self.abs = None;
        self.abs_exact = None;
        self.abs_parts = None;
        self.abs_whole = false;
        let magnitude = self.scan_int_from(token).0;
        let keep = (register || std::mem::take(&mut self.abs_whole)) && magnitude.is_none();

        let abs = self.abs.take().filter(|_| keep);
        let exact = self.abs_exact.take().filter(|_| keep && !negative);
        let parts = self.abs_parts.take().filter(|_| keep && !negative);
        self.abs = abs.map(|num| if negative { num.neg() } else { num });
        self.abs_exact = exact;
        self.abs_parts = parts;
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
        // An unknown command may stand for any quantity or for digits that
        // a decimal point may follow. Unknown digits are read on with the
        // rest of the number, making the whole number unknown.
        // Typed unknown digits leave their interval to the test or
        // assignment, as a register read as the whole number does.
        if let Some(mut num) = token.cs().and_then(|sym| self.typed_digits_of(sym)) {
            // Reading on may expand a scan of its own, which leaves its own
            // interval, so this one is set once the constant has ended.
            let mut next = self.next_x_token();
            let mut from = token.cs();
            // Slices of one number's spelling, in order from its first
            // place: that number, or the number its first places make.
            if let Some((whole, k, i, mut j)) = token.cs().and_then(|sym| self.typed_slice(sym)) {
                while let Some((w, l, i, e)) = next.and_then(|t| self.typed_slice(t.cs()?))
                    && (w, l, i) == (whole, k, j)
                {
                    j = e;
                    next = self.next_x_token();
                }
                if let Some(n) = self.typed_digits_of(whole) {
                    num = places(n, (k, i, j));
                    from = (i == 0 && j == k).then_some(whole);
                }
            }
            let more = next.is_some_and(|t| {
                self.digit(t, DECIMAL).is_some()
                    || matches!(t.tok, Tok::Chr(c, Catcode::Other) if crate::tex::is_unknown_digit(c))
                    || t.cs().is_some_and(|sym| self.is_unknown_digits(sym))
            });
            if more {
                // More digits after them: any number, unsigned where these are.
                let backed_up = self.scan_digits(DECIMAL, next).1;
                self.unsigned_number(num.lo >= 0);
                return (None, backed_up);
            }
            // tex.web § 445: one trailing space is part of the constant.
            let backed_up = match next {
                Some(t) if self.is_spacer(t) => false,
                Some(t) => {
                    self.unread(t);
                    true
                }
                None => true,
            };
            // The test that reads it narrows it on each arm, as a register's.
            let from = from.map(|sym| (sym, crate::value::Quantity::Int));
            (self.abs, self.abs_exact, self.abs_from, self.abs_odd) = (Some(num), None, from, None);
            return ((num.lo == num.hi).then_some(num.lo), backed_up);
        }
        let digits =
            token.tok == Tok::Cs(self.unknown_digits) || token.cs().is_some_and(|sym| self.is_more_digits(sym));
        if digits {
            return self.scan_digits(DECIMAL, Some(token));
        }
        if self.split_scanned(token) {
            return match self.next_x_token() {
                Some(token) => self.scan_int_from(token),
                None => (self.missing_number(None), false),
            };
        }
        if self.is_unknown(token) {
            // A name a join or a family write left one of several
            // meanings; a one-of token's texts may run on past it.
            let unsigned = token.cs().is_some_and(|sym| !self.is_unknown_name(sym)) && self.digits_first(token);
            self.unsigned_number(unsigned);
            self.abs_odd = None;
            return (None, true);
        }
        self.abs_odd = None;
        match token.tok {
            Tok::Chr('`', Catcode::Other) => (self.scan_alphabetic_constant(), false),
            Tok::Chr('\'', Catcode::Other) => (self.scan_digits(OCTAL, None).0, false),
            Tok::Chr('"', Catcode::Other) => (self.scan_digits(HEXADECIMAL, None).0, false),
            _ => match self.scan_internal(token) {
                Some((RegKind::Toks, _)) => (None, false),
                Some((_, value)) => {
                    self.abs_odd = None;
                    (value.and_then(|v| v.as_int()), false)
                }
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
        // A brace read as its code leaves `align_state` as it was (array's
        // `\ifnum0=`}\fi`).
        self.count_brace(&token, -1);
        let code = match token.tok {
            // An unknown character has no known code.
            Tok::Chr(crate::tex::UNKNOWN_DIGIT | crate::tex::UNKNOWN_NONZERO | crate::tex::UNKNOWN_CHAR, _) => None,
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
            // One of several characters, or one digit of typed digits: a
            // code in their hull, left as the interval read ([`Machine::abs`]),
            // with the exact codes while each member is one character.
            None if let Some((codes, exact)) = self.char_codes(token) => {
                self.scan_optional_x_space();
                (self.abs, self.abs_exact, self.abs_from) = (Some(codes), exact, None);
                None
            }
            None => {
                self.unread(token);
                Some(IMPROPER_ALPHABETIC)
            }
        }
    }

    /// The codes a token that stands for one character may have: their
    /// hull, and each code while every member is one known character.
    fn char_codes(&self, token: Token) -> Option<(Num, Option<Vec<i64>>)> {
        let digit = |t: &Token| {
            let num = self.typed_digits_of(t.cs()?)?;
            ((0..=9).contains(&num.lo) && num.hi <= 9).then(|| Num::range(48 + num.lo, 48 + num.hi))
        };
        if let Some(codes) = digit(&token) {
            return Some((codes, None));
        }
        let mut hull: Option<Num> = None;
        let mut exact = Some(Vec::new());
        for text in self.one_of_members(token)? {
            let n = match &*text {
                [Token { tok: Tok::Chr(c, _), .. }] if !crate::tex::is_unknown_char(*c) => Num::exact(*c as i64),
                [t] => digit(t)?,
                _ => return None,
            };
            exact = exact.filter(|_| n.lo == n.hi).map(|mut e: Vec<i64>| {
                e.push(n.lo);
                e
            });
            hull = Some(hull.map_or(n, |h| h.join(&n)));
        }
        Some((hull?, exact))
    }

    /// A token a join left one of several texts, or parameterless macros,
    /// met by a number scan: a path per text, each read as TeX would read
    /// it, so an error is the one path's and a name keeps the meaning its
    /// path read ([`Machine::split_meaning`]). Only texts satex read in
    /// full: one with unknown parts reads as unknown digits, as before.
    /// `true` when the run parted; the scan then reads on.
    fn split_scanned(&mut self, token: Token) -> bool {
        let texts: Option<Vec<std::rc::Rc<[Token]>>> = self.one_of_members(token).or_else(|| {
            let may = token.cs().and_then(|sym| self.env.slot(sym)?.may.clone())?;
            (may.iter())
                .map(|m| match m {
                    Meaning::Macro(d) if d.parameter_text.items.is_empty() => Some(d.replacement_text.clone()),
                    _ => None,
                })
                .collect()
        });
        if !texts.is_some_and(|texts| texts.iter().all(|t| self.exact_text(t))) {
            return false;
        }
        if self.split_one_of_refining(token) {
            return true;
        }
        let split = token.cs().is_some_and(|sym| self.split_meaning(sym, token.span));
        if split {
            self.unread(token);
        }
        split
    }

    /// Whether TeX would read `text` as satex has it: no unknown name,
    /// digits or character in it. A token for one of several texts is
    /// read as each of them.
    fn exact_text(&self, text: &[Token]) -> bool {
        text.iter().all(|t| match t.tok {
            Tok::Chr(c, _) => !crate::tex::is_unknown_char(c),
            Tok::Cs(_) if let Some(texts) = self.one_of_members(*t) => texts.iter().all(|t| self.exact_text(t)),
            Tok::Cs(sym) => {
                !self.is_unknown_digits(sym)
                    && !self.is_unknown_name(sym)
                    && !matches!(self.env.meaning(sym), Meaning::Unknown)
            }
            Tok::Param(_) => true,
        })
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
    /// already. One trailing space is part of the constant. Any other
    /// ending token goes back, and the second result is then true.
    /// Past 2^31-1 TeX says "Number too big" and uses the limit.
    fn scan_digits(&mut self, radix: i64, first: Option<Token>) -> (Option<i64>, bool) {
        let mut value: i64 = 0;
        let mut any = false;
        // Unknown digits among the digits make the number unknown.
        let mut unknown = false;
        // Begun with a digit, known or unknown: no sign hides in it, so an
        // unknown value is unsigned (tex.web § 445 caps it).
        let mut unsigned = None;
        let mut next = first.or_else(|| self.next_x_token());
        // The last digit, while it is known: it decides the parity.
        let mut last = None;
        let result = loop {
            let Some(token) = next else { break None };
            if self.split_scanned(token) {
                next = self.next_x_token();
                continue;
            }
            let digits = match token.tok {
                Tok::Chr(c, Catcode::Other) => crate::tex::is_unknown_digit(c),
                Tok::Cs(sym) => self.is_unknown_digits(sym),
                _ => false,
            };
            // Unknown text may be digits, read on and making the number
            // unknown. It may also be empty, so it is not itself a digit.
            unsigned.get_or_insert_with(|| self.digits_first(token));
            let maybe = !digits && self.is_unknown(token);
            if digits || maybe {
                any |= !maybe && !token.cs().is_some_and(|sym| self.is_more_digits(sym));
                unknown = true;
                last = None;
                next = self.next_x_token();
                continue;
            }
            let Some(d) = self.digit(token, radix) else {
                // Only unknown text so far: it may have been the digits.
                if !any && unknown {
                    self.unread(token);
                    break Some(true);
                }
                if !any {
                    self.unread(token);
                    self.abs_odd = None;
                    return (self.missing_number(Some(token)), true);
                }
                if self.is_spacer(token) {
                    break Some(false);
                }
                self.unread(token);
                break Some(true);
            };
            any = true;
            last = Some(d);
            value = (value * radix + d).min(MAX_INT);
            next = self.next_x_token();
        };
        // Radixes are even: the last digit's parity is the number's.
        self.abs_odd = last.map(|d| d % 2 != 0);
        if unknown {
            self.unsigned_number(unsigned == Some(true));
            return (None, result.unwrap_or(false));
        }
        match result {
            Some(backed_up) => (Some(value), backed_up),
            None => (any.then_some(value).or_else(|| self.missing_number(None)), false),
        }
    }

    /// tex.web §§ 456, 459: "Illegal unit of measure", where the token after
    /// the number is known and no unit.
    fn illegal_unit(&mut self, inserted: &str) {
        // Looked at, not expanded: what a name would expand to is no unit
        // satex can rule out, and an undefined one is an error of its own.
        let mut spaces = Vec::new();
        let token = loop {
            let Some(token) = self.next_token() else { return };
            if !matches!(token.tok, Tok::Chr(_, crate::tex::Catcode::Space)) {
                break token;
            }
            spaces.push(token);
        };
        let plain = match token.tok {
            Tok::Cs(sym) => matches!(self.env.meaning(sym), Meaning::Primitive(p) if !crate::builtins::expandable(p)),
            Tok::Chr(_, crate::tex::Catcode::Active) => false,
            Tok::Chr(c, _) => !crate::tex::is_unknown_char(c),
            _ => false,
        };
        self.unread(token);
        for space in spaces.into_iter().rev() {
            self.unread(space);
        }
        if plain {
            let message = format!("Illegal unit of measure ({inserted} inserted)");
            self.diagnose(crate::facts::Severity::Warning, "illegal-unit", token.span, message);
            self.misread(Some(token));
        }
    }

    /// A number scan met `t` where TeX wants a number or unit: past a
    /// possible error, a token other than the scan's own end (`\relax`, a
    /// space, the input's end) stood for the number, and the code after it
    /// runs its data as code ([`crate::machine::BREAKING`]).
    fn misread(&mut self, t: Option<Token>) {
        let end = t.is_none_or(|t| t.is_space() || self.is_relax(t));
        self.dropping_breaks |= self.dropping_possible && !end;
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
        self.misread(at);
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
            self.abs_exact = None;
            self.abs_from = None;
        }
        scanned
    }

    fn scan_dimen_parts(&mut self, mu: bool, inf: bool, shortcut: Option<Option<i64>>) -> (Option<Scaled>, u8) {
        let mut negative = false;
        let mut fraction = 0;
        // The integer part's interval where its digits are typed, and
        // whether the digits after the point are unknown.
        let mut range: Option<crate::value::Num> = None;
        let mut fraction_unknown = false;
        let integer = match shortcut {
            // A count of unknown value left its interval, signed.
            Some(integer) => {
                range = integer.map(crate::value::Num::exact).or_else(|| self.abs.take());
                integer
            }
            None => {
                let Some((signs, token)) = self.scan_signs() else { return (None, 0) };
                self.abs = None;
                self.abs_exact = None;
                self.probe_scans(token, if mu { "mudimen" } else { "dimen" });
                negative = signs;
                match self.scan_internal(token) {
                    Some((RegKind::Count, value)) => {
                        let integer = value.and_then(|v| v.as_int());
                        range = integer.map(crate::value::Num::exact).or_else(|| self.abs.take());
                        integer
                    }
                    Some((RegKind::Toks, _)) => None,
                    // tex.web § 449: an internal dimension or glue is the
                    // whole value; no unit follows.
                    Some((kind, value)) => {
                        let width = value.as_ref().and_then(GlueHull::of).map(|g| g.width);
                        let value = value.and_then(|v| v.as_dimen());
                        // A dimension register of unknown value leaves its
                        // interval to the test (see `scan_int`), glue in a
                        // hull its width's, which no test narrows.
                        if value.is_some() {
                            self.abs = None;
                        } else if kind != RegKind::Dimen {
                            self.abs = width;
                            self.abs_from = None;
                        }
                        self.abs_exact = None;
                        // `attach_sign` clamps before the sign is attached,
                        // so the interval is clamped here too. What it
                        // clamped is no longer the register's own value, so
                        // no test may narrow the register by it.
                        if let Some(num) = self.abs.take() {
                            let clamped = clamp_dimen(&num);
                            if clamped != num {
                                self.abs_from = None;
                            }
                            self.abs = Some(clamped);
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
                        fraction_unknown = digits.is_none();
                        range = Some(crate::value::Num::exact(0));
                        digits.map(|_| 0)
                    }
                    None => {
                        let (mut integer, backed_up) = self.scan_int_from(token);
                        range = match integer {
                            Some(n) => Some(crate::value::Num::exact(n)),
                            None => self.abs.take(),
                        };
                        if backed_up && let Some(point) = self.next_token() {
                            if matches!(point.tok, Tok::Chr('.' | ',', Catcode::Other)) {
                                let digits = self.scan_fraction();
                                fraction = digits.unwrap_or(0);
                                fraction_unknown = digits.is_none();
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
        self.abs = None;
        let fractions = if fraction_unknown { (0, UNIT - 1) } else { (fraction, fraction) };
        let range = range.filter(|_| integer.is_none()).map(|r| (r, fractions));
        let (value, order) = self.scan_units(mu, inf, integer, fraction, range);
        if negative {
            self.abs = self.abs.take().map(|num| num.neg());
            self.abs_exact = None;
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

    /// Whether the number `token` begins is surely a digit first, so no
    /// sign hides in it: a digit, unknown digits that are at least one
    /// and unsigned, or meanings each a macro without parameters whose
    /// text is digits beginning so, or `\relax`.
    fn digits_first(&self, token: Token) -> bool {
        let first = |t: &Token| match t.tok {
            Tok::Chr(c, Catcode::Other) => c.is_ascii_digit() || crate::tex::is_unknown_digit(c),
            Tok::Cs(sym) if self.is_more_digits(sym) => false,
            Tok::Cs(sym) => sym == self.unknown_digits || self.typed_digits_of(sym).is_some_and(|num| num.lo >= 0),
            _ => false,
        };
        if first(&token) {
            return true;
        }
        let Some(binding) = token.cs().and_then(|sym| self.env.slot(sym)) else { return false };
        let members = binding.may.as_deref().unwrap_or(std::slice::from_ref(&binding.meaning));
        // `\relax` is no digit: "Missing number" puts it back, and what
        // reads on after it (a relation, more of the number) is not what
        // a number here would read, so it is no member of this case.
        members.iter().all(|m| match m {
            // Digits alone: no unit or other text hides after them either.
            Meaning::Macro(d) => {
                let rest = |t: &Token| first(t) || t.cs().is_some_and(|sym| self.is_unknown_digits(sym));
                d.parameter_text.items.is_empty()
                    && d.replacement_text.first().is_some_and(first)
                    && d.replacement_text.iter().all(rest)
            }
            _ => false,
        })
    }

    /// An unknown number just read: unsigned digits lie in 0..=`MAX_INT`
    /// (tex.web § 445 caps them), the interval left for the test.
    fn unsigned_number(&mut self, unsigned: bool) {
        self.abs = unsigned.then(|| Num::range(0, MAX_INT));
        (self.abs_exact, self.abs_from, self.abs_whole) = (None, None, unsigned);
    }

    /// A token whose meaning is unknown: it may stand for any text.
    fn is_unknown(&mut self, token: Token) -> bool {
        let sym = token.cs().or_else(|| self.active_cs(token));
        let unknown = sym.is_some_and(|sym| {
            self.is_unknown_digits(sym)
                || self.is_unknown_name(sym)
                || matches!(self.env.meaning(sym), Meaning::Unknown)
        });
        self.approximated |= unknown;
        unknown
    }

    /// Whether `sym` stands for digits that are not known: the plain
    /// markers, or digits typed with the interval they lie in.
    pub(crate) fn is_unknown_digits(&self, sym: Sym) -> bool {
        sym == self.unknown_digits || self.is_more_digits(sym) || self.name(sym).starts_with(TYPED_DIGITS)
    }

    /// The interval typed unknown digits lie in.
    pub(crate) fn typed_digits_of(&self, sym: Sym) -> Option<Num> {
        if !self.name(sym).starts_with(TYPED_DIGITS) {
            return None;
        }
        // A test may have narrowed it on this path, to one value or few.
        self.env.slot(sym)?.value.num().map(|(_, num)| num)
    }

    /// A slice of typed digits: the number it is cut from, the length of
    /// that number's spelling, and the places `[i, j)` it holds.
    pub(crate) fn typed_slice(&self, sym: Sym) -> Option<(Sym, u32, u32, u32)> {
        let (_, cut) = self.name(sym).strip_prefix(TYPED_DIGITS)?.split_once('@')?;
        let mut it = cut.split(':').map(|x| x.parse::<u32>().ok());
        Some((Sym(it.next()??), it.next()??, it.next()??, it.next()??))
    }

    /// Places `[i, j)` of the `k`-digit spelling of the number `whole`
    /// names, which lies in `num`: one token that a scan joins back to
    /// `whole` when the slices before and after it stand next to it.
    pub(crate) fn slice_token(&mut self, whole: Sym, num: Num, (k, i, j): (u32, u32, u32), span: Span) -> Token {
        let num = places(num, (k, i, j));
        let sym = self.intern(&format!("{TYPED_DIGITS}{}..{}@{}:{k}:{i}:{j}", num.lo, num.hi, whole.0));
        if self.env.slot(sym).is_none() {
            let mut binding = crate::env::Binding::builtin(Meaning::Char(crate::tex::UNKNOWN_CHAR, Catcode::Other));
            binding.value = Value::Num { quantity: crate::value::Quantity::Int, num };
            self.env.constant(sym, binding);
        }
        Token::new(Tok::Cs(sym), span)
    }

    /// What TeX may read as the first token of the digits `whole` stands
    /// for, which lie in `num`: one arm for a sign, for one digit and for
    /// several, each with the interval `whole` has on it and the tokens it
    /// spells. Several digits are its first place and the rest, slices of
    /// `whole` ([`Machine::slice_token`]) of the length both ends have, or
    /// of one not known.
    pub(crate) fn digit_arms(&mut self, whole: Sym, num: Num, span: Span) -> Vec<(Num, Vec<Token>)> {
        let mut arms = Vec::new();
        if num.lo < 0 {
            let magnitude = Num::range(1.max(num.hi.saturating_neg()), num.lo.saturating_neg());
            let mut text = self.text_tokens("-", span);
            text.extend(self.digits_in(magnitude, span));
            arms.push((Num::range(num.lo, num.hi.min(-1)), text));
        }
        if num.hi >= 0 && num.lo <= 9 {
            arms.push((Num::range(num.lo.max(0), num.hi.min(9)), vec![Token::new(Tok::Cs(whole), span)]));
        }
        if num.hi >= 10 {
            let part = Num::range(num.lo.max(10), num.hi);
            let len = |n: i64| n.to_string().len() as u32;
            let k = if len(part.lo) == len(part.hi) { len(part.hi) } else { 0 };
            let text =
                vec![self.slice_token(whole, part, (k, 0, 1), span), self.slice_token(whole, part, (k, 1, k), span)];
            arms.push((part, text));
        }
        arms
    }

    /// The digits of a number in `num`: the number itself when it is one,
    /// else one token of unknown digits typed with the interval (its `-`,
    /// when the number may be negative, is part of the token), which a
    /// scan reading it back keeps ([`Machine::abs`]), so `\number\numexpr`
    /// of a bounded value stays bounded through text.
    pub(crate) fn digits_in(&mut self, num: Num, span: Span) -> Vec<Token> {
        if num.lo == num.hi {
            return self.text_tokens(&num.lo.to_string(), span);
        }
        // One name per number made: a test that narrows it narrows this
        // number on its path, never another one with the same interval.
        let sym = self.intern(&format!("{TYPED_DIGITS}{}..{}#{}", num.lo, num.hi, self.out.interner.len()));
        if self.typed_digits_of(sym).is_none() {
            let mut binding = crate::env::Binding::builtin(Meaning::Char(crate::tex::UNKNOWN_CHAR, Catcode::Other));
            binding.value = Value::Num { quantity: crate::value::Quantity::Int, num };
            self.env.constant(sym, binding);
        }
        vec![Token::new(Tok::Cs(sym), span)]
    }

    /// tex.web §§ 453-458: the ⟨unit of measure⟩, applied to
    /// `integer + fraction/2^16`.
    ///
    /// `range`: an integer part in an interval with the fraction in
    /// another; each unit is monotone in both, so the value's interval,
    /// left in [`Machine::abs`], is the hull of the corners' values.
    fn scan_units(
        &mut self,
        mu: bool,
        inf: bool,
        integer: Option<i64>,
        fraction: i64,
        range: Option<(crate::value::Num, (i64, i64))>,
    ) -> (Option<Scaled>, u8) {
        // The optional space after the unit may expand a scan of its own.
        let outer = self.ranged.take();
        let (value, order) = self.scan_unit_of(mu, inf, integer, fraction, range);
        let ranged = std::mem::replace(&mut self.ranged, outer);
        if value.is_none()
            && let Some(num) = ranged
        {
            self.abs = Some(num);
            self.abs_exact = None;
            self.abs_from = None;
            self.abs_whole = true;
        }
        (value, order)
    }

    /// The value at the corners of `range`, through `conv` of a magnitude
    /// and fraction, with the sign attached (tex.web § 448).
    fn set_ranged(
        &mut self,
        range: Option<(crate::value::Num, (i64, i64))>,
        conv: impl Fn(i64, i64) -> Option<Scaled>,
    ) {
        self.ranged = range.and_then(|(r, (flo, fhi))| {
            let at = |n: i64, f: i64| {
                let v = conv(n.checked_abs()?, f)?;
                let v = if v.abs() >= DIMEN_LIMIT { MAX_DIMEN } else { v };
                Some(if n < 0 { -v } else { v })
            };
            let ends = [at(r.lo, flo)?, at(r.lo, fhi)?, at(r.hi, flo)?, at(r.hi, fhi)?];
            Some(crate::value::Num::range(*ends.iter().min()?, *ends.iter().max()?))
        });
    }

    fn scan_unit_of(
        &mut self,
        mu: bool,
        inf: bool,
        integer: Option<i64>,
        fraction: i64,
        range: Option<(crate::value::Num, (i64, i64))>,
    ) -> (Option<Scaled>, u8) {
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
                    if let Some(v) = unit {
                        self.set_ranged(range, |n, f| Some(scale_unit(n, v, f)));
                    }
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
                    self.abs_exact = None;
                    self.abs_from = None;
                    self.abs_whole = self.abs.is_some();
                    return (value, 0);
                }
                None => self.unread(token),
            }
        }
        if mu {
            // tex.web § 456: "Illegal unit of measure (mu inserted)" when the
            // keyword is missing. The value is taken as mu either way.
            if !self.scan_keyword("mu") && integer.is_some() {
                self.illegal_unit("mu");
            }
            let value = integer.map(|n| attach_fraction(n, fraction));
            self.set_ranged(range, |n, f| Some(attach_fraction(n, f)));
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
            if let Some(v) = unit {
                self.set_ranged(range, |n, f| Some(scale_unit(n, v, f)));
            }
            self.scan_optional_x_space();
            return (value, 0);
        }
        // pdfTeX's `px` is the internal dimension `\pdfpxdimen`.
        if self.scan_keyword("px") {
            let sym = self.intern("pdfpxdimen");
            let unit = self.env.value(sym).as_dimen();
            self.scan_optional_x_space();
            let value = integer.zip(unit).map(|(n, v)| scale_unit(n, v, fraction));
            if let Some(v) = unit {
                self.set_ranged(range, |n, f| Some(scale_unit(n, v, f)));
            }
            return (value, 0);
        }
        let (mut integer, mut fraction) = (integer, fraction);
        let mut range = range;
        if self.scan_keyword("true") {
            let sym = self.intern("mag");
            let mut mag = self.env.value(sym).as_int();
            // tex.web § 288 `prepare_mag`: an illegal `\mag` becomes 1000, globally.
            if mag.is_some_and(|m| !(1..=MAX_MAG).contains(&m)) {
                self.env.set_value(sym, Value::Int(MAG_UNITY), true);
                mag = Some(MAG_UNITY);
            }
            if mag != Some(MAG_UNITY) {
                range = None;
                integer = integer.zip(mag).map(|(n, mag)| {
                    let (q, r) = xn_over_d(n, MAG_UNITY, mag);
                    let f = (MAG_UNITY * fraction + UNIT * r) / mag;
                    fraction = f % UNIT;
                    q + f / UNIT
                });
            }
        }
        let pt = self.scan_keyword("pt");
        let ratio = if pt { None } else { UNITS.iter().find(|(unit, _, _)| self.scan_keyword(unit)) };
        let value = if let Some(&(_, num, denom)) = ratio {
            let conv = |n: i64, fraction: i64| {
                let (q, r) = xn_over_d(n, num, denom);
                let f = (num * fraction + UNIT * r) / denom;
                Some(attach_fraction(q + f / UNIT, f % UNIT))
            };
            self.set_ranged(range, conv);
            integer.and_then(|n| conv(n, fraction))
        } else if !pt && self.scan_keyword("sp") {
            self.set_ranged(range, |n, _| Some(n));
            integer
        } else {
            // `pt`, or "Illegal unit of measure (pt inserted)" (tex.web § 459).
            if !pt && integer.is_some() {
                self.illegal_unit("pt");
            }
            self.set_ranged(range, |n, f| Some(attach_fraction(n, f)));
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

    /// ⟨glue⟩ or, with `mu`, ⟨muglue⟩ (tex.web § 461, `scan_glue`): one
    /// interval per component, the interval a scan left where its amount is
    /// not known, else any.
    pub fn scan_glue_spec(&mut self, mu: bool) -> Option<GlueHull> {
        let (negative, token) = self.scan_signs()?;
        self.probe_scans(token, if mu { "muglue" } else { "glue" });
        let sign = |num: Num| if negative { num.neg() } else { num };
        let width = match self.scan_internal(token) {
            Some((RegKind::Skip | RegKind::MuSkip, value)) => {
                let hull = value.as_ref().and_then(GlueHull::of).unwrap_or_else(GlueHull::full);
                return Some(if negative { hull.neg() } else { hull });
            }
            // The count's interval goes in signed, and comes out so.
            Some((RegKind::Count, value)) => {
                let integer = value.and_then(|v| v.as_int()).map(|n| if negative { -n } else { n });
                self.abs = self.abs.take().filter(|_| integer.is_none()).map(sign);
                let width = self.scan_dimen_with(mu, false, Some(integer)).0;
                width.map(Num::exact).or_else(|| self.abs.take())
            }
            Some((RegKind::Toks, _)) => None,
            Some((_, value)) => match value.and_then(|v| v.as_dimen()) {
                Some(v) => Some(Num::exact(if negative { -v } else { v })),
                None => self.abs.take().map(sign),
            },
            None => {
                self.unread(token);
                let width = self.scan_dimen_with(mu, false, None).0;
                width.map(Num::exact).or_else(|| self.abs.take()).map(sign)
            }
        };
        let mut hull = GlueHull::exact(&Glue::default());
        hull.width = width.unwrap_or(Num::full(true));
        for (keyword, stretch) in [("plus", true), ("minus", false)] {
            if !self.scan_keyword(keyword) {
                continue;
            }
            let (amount, order) = self.scan_dimen_with(mu, true, None);
            let (amount, order) = match (amount, self.abs.take()) {
                (Some(a), _) => (Num::exact(a), Num::exact(order.into())),
                (None, Some(num)) => (num, Num::exact(order.into())),
                (None, None) => (Num::full(true), GlueHull::full().stretch_order),
            };
            if stretch {
                (hull.stretch, hull.stretch_order) = (amount, order);
            } else {
                (hull.shrink, hull.shrink_order) = (amount, order);
            }
        }
        Some(hull)
    }

    /// tex.web § 407: the letters of `keyword`, in either
    /// case and any category but active, read with expansion. Blanks are
    /// skipped only before the first letter. On a mismatch everything read
    /// goes back.
    pub fn scan_keyword(&mut self, keyword: &str) -> bool {
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

/// tex.web § 460 (`attach_sign`): a magnitude of 2^30 or more is
/// "Dimension too large" and reads as `\maxdimen`, whatever its sign.
/// The clamp is not monotone, so an interval that passes it may only be
/// `\maxdimen`.
fn clamp_dimen(num: &Num) -> Num {
    if num.lo > -DIMEN_LIMIT && num.hi < DIMEN_LIMIT {
        return *num;
    }
    let (lo, hi) = (num.lo.max(-MAX_DIMEN), num.hi.min(MAX_DIMEN));
    if lo > hi { Num::exact(MAX_DIMEN) } else { Num::range(lo, hi).join(&Num::exact(MAX_DIMEN)) }
}

/// tex.web § 453 (`attach_fraction`): an integer part of 2^14 points or
/// more is "Dimension too large".
fn attach_fraction(integer: i64, fraction: i64) -> Scaled {
    if integer >= MAX_POINTS { MAX_DIMEN } else { integer * UNIT + fraction }
}

/// What a `scan_expr` term is in terms of one unknown operand `x` (a
/// register or typed digits, read twice as one value): `x + c`,
/// `round((x + c)/d)`, or that times `d`, with the interval of `x + c`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Form {
    None,
    Lin(Sym, i64),
    Quot(Sym, i64, i64, (i64, i64)),
    Rem(Sym, i64, i64, (i64, i64)),
}

/// The range of `y - round(y/d)*d` for `y` in `[lo, hi]`.  Ties round
/// away from zero (e-TeX's `\numexpr`), so for an even `|d|` the
/// remainder `|d|/2` needs `y <= -|d|/2` and `-|d|/2` needs `y >= |d|/2`.
fn remainder(d: i64, (lo, hi): (i64, i64)) -> (i64, i64) {
    let a = d.abs();
    let h = a / 2;
    if a % 2 == 1 {
        return (-h, h);
    }
    (if hi >= h { -h } else { 1 - h }, if lo <= -h { h } else { h - 1 })
}

/// The value of an interval holding one number.
fn exact(n: &Num) -> Option<i64> {
    (n.lo == n.hi).then_some(n.lo)
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

/// `x*n/d` rounded, ties away from zero, on unbounded integers.
/// `None` for `d = 0`.
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
/// concrete one: an error makes the whole expression 0, so what may err
/// is flagged and the step goes on with the results that do not.
/// `err` is (may err, must err).
fn settle((num, may): (Option<Num>, bool), err: &mut (bool, bool)) -> Num {
    match num {
        None => {
            *err = (true, true);
            Num::exact(0)
        }
        Some(num) => {
            err.0 |= may;
            num
        }
    }
}

fn unbounded() -> (i64, i64) {
    (i64::MIN, i64::MAX)
}

/// One abstract step of `scan_expr`: the exact result over every
/// combination of bounded exact operands, while there are few enough
/// (`Num::lift_points`, the set lift applied to the running example), else
/// the interval result on their hulls (`Num::lift`, then [`settle`]). An
/// error is only flagged: it zeroes the whole expression, which joins
/// that 0 in once, at the end.
#[allow(clippy::too_many_arguments)]
fn step(
    nums: &[&Num],
    points: &[Option<&[i64]>],
    members: usize,
    range: (i64, i64),
    wrap: bool,
    exact: impl Fn(&[i64]) -> Option<i64>,
    ideal: impl Fn(&[i64]) -> Option<i64>,
    err: &mut (bool, bool),
) -> (Num, Option<Vec<i64>>) {
    if let Some(sets) = points.iter().copied().collect::<Option<Vec<&[i64]>>>()
        && let Some((mut out, any_err)) = Num::lift_points(&sets, members, &exact)
    {
        err.0 |= any_err;
        if out.len() <= members {
            out.sort_unstable();
            return (Num::range(out[0], out[out.len() - 1]), Some(out));
        }
    }
    (settle(Num::lift(nums, range, wrap, exact, ideal), err), None)
}

impl Machine<'_> {
    /// e-TeX's `scan_expr`: `\numexpr`, `\dimexpr`, `\glueexpr`,
    /// `\muexpr` (etex.ch "Declare procedures needed for expressions").
    /// Overflow anywhere zeroes the whole expression, with "Arithmetic
    /// overflow". Alongside the concrete evaluation, count and dimen levels
    /// are also evaluated on intervals ([`Num::lift`]), so `\numexpr` of an
    /// unknown register is the interval of its results, joined with 0 when
    /// some may overflow. A glue expression with an unknown operand is
    /// unknown.
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
            ea_x: Option<Vec<i64>>,
            ta_x: Option<Vec<i64>>,
            na_x: Option<Vec<i64>>,
            ef: Form,
            tf: Form,
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
        let (e, ea, ea_x) = 'restart: loop {
            let (mut r, mut e, mut s, mut t, mut n) = (ExprOp::None, zero, ExprOp::None, zero, 0i64);
            let (mut ea, mut ta, mut na) = (Num::exact(0), Num::exact(0), Num::exact(0));
            let (mut ea_x, mut ta_x, mut na_x) = (Some(vec![0i64]), Some(vec![0i64]), Some(vec![0i64]));
            let (mut ef, mut tf) = (Form::None, Form::None);
            'next: loop {
                let o_level = if s == ExprOp::None { l } else { RegKind::Count };
                let Some(token) = self.next_non_blank_x_token() else { break 'restart (e, ea, ea_x) };
                if token.is_char('(') && token.is_cat(Catcode::Other) {
                    stack.push(Frame { l, s, r, e, t, n, ea, ta, na, ea_x, ta_x, na_x, ef, tf });
                    l = o_level;
                    continue 'restart;
                }
                self.unread(token);
                self.abs = None;
                self.abs_exact = None;
                self.abs_from = None;
                let f = match o_level {
                    RegKind::Count => self.scan_int().map(|v| Glue { width: v, ..zero }),
                    RegKind::Dimen => self.scan_dimension().map(|v| Glue { width: v, ..zero }),
                    RegKind::Skip => self.scan_glue_spec(false).and_then(|g| g.glue()),
                    _ => self.scan_glue_spec(true).and_then(|g| g.glue()),
                };
                // A register of unknown value leaves its interval; anything
                // else unknown may be any 32-bit number. A register (or
                // sub-expression) a join left one of a few exact values
                // keeps them (`fa_x`), the set lift for the arithmetic below.
                let mut fa_x = match &f {
                    Some(g) => Some(vec![g.width]),
                    None => self.abs_exact.take(),
                };
                let mut fa = match &f {
                    Some(g) => Num::exact(g.width),
                    None => self.abs.take().unwrap_or_else(|| Num::range(WORD_MIN, WORD_MAX)),
                };

                // One unknown read twice is one value: `x - (x/d)*d`.
                let mut ff = match self.abs_from {
                    Some((sym, _)) if f.is_none() && o_level == RegKind::Count => Form::Lin(sym, 0),
                    _ => Form::None,
                };
                self.abs = None;
                self.abs_exact = None;
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
                                let relax = self.is_relax(t);
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
                    (fa, fa_x) = step(
                        &[&fa],
                        &[fa_x.as_deref()],
                        members,
                        unbounded(),
                        false,
                        |v| (v[0].abs() <= limit).then_some(v[0]),
                        |v| (v[0].abs() <= limit).then_some(v[0]),
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
                            tf = ff;
                            ta = fa;
                            ta_x = fa_x.clone();
                        }
                        ExprOp::Mult if o == ExprOp::Div => {
                            n = f.width;
                            na = fa;
                            na_x = fa_x.clone();
                            o = ExprOp::Scale;
                        }
                        ExprOp::Mult => {
                            tf = match (tf, ff) {
                                (Form::Quot(x, c, d, y), _) if exact(&fa) == Some(d) => Form::Rem(x, c, d, y),
                                (_, Form::Quot(x, c, d, y)) if exact(&ta) == Some(d) => Form::Rem(x, c, d, y),
                                _ => Form::None,
                            };
                            t = each(t, &mut |x| num_error(mult_and_add(x, f.width, 0, max)));
                            (ta, ta_x) = step(
                                &[&ta, &fa],
                                &[ta_x.as_deref(), fa_x.as_deref()],
                                members,
                                (-max, max),
                                false,
                                |v| mult_and_add(v[0], v[1], 0, max),
                                |v| Some(v[0] * v[1]),
                                &mut err,
                            );
                        }
                        ExprOp::Div => {
                            tf = match (tf, exact(&fa)) {
                                (Form::Lin(x, c), Some(d)) if d != 0 => Form::Quot(x, c, d, (ta.lo, ta.hi)),
                                _ => Form::None,
                            };
                            t = each(t, &mut |x| num_error(quotient(x, f.width)));
                            (ta, ta_x) = step(
                                &[&ta, &fa],
                                &[ta_x.as_deref(), fa_x.as_deref()],
                                members,
                                unbounded(),
                                false,
                                |v| quotient(v[0], v[1]),
                                |v| quotient(v[0], v[1]),
                                &mut err,
                            );
                        }
                        ExprOp::Scale => {
                            tf = Form::None;
                            t = each(t, &mut |x| num_error(fract(x, n, f.width, max)));
                            (ta, ta_x) = step(
                                &[&ta, &na, &fa],
                                &[ta_x.as_deref(), na_x.as_deref(), fa_x.as_deref()],
                                members,
                                (-max, max),
                                false,
                                |v| fract(v[0], v[1], v[2], max),
                                |v| fract_ideal(v[0], v[1], v[2]),
                                &mut err,
                            );
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
                            ef = tf;
                            ea = ta;
                            ea_x = ta_x.clone();
                        } else {
                            // The plain `+` or `-` of two exact values; `None` past an `i64`.
                            let op = |a: i64, b: i64| if sub { a.checked_sub(b) } else { a.checked_add(b) };
                            // `x + c1 - round((x + c)/d)*d` is `c1 - c` plus
                            // the remainder `r` of rounding `y = x + c`.
                            let bound = match (ef, tf) {
                                (Form::Lin(x, c1), Form::Rem(z, c, d, y)) if sub && x == z => {
                                    let (lo, hi) = remainder(d, y);
                                    Some((c1 - c + lo, c1 - c + hi))
                                }
                                _ => None,
                            };
                            ef = match (ef, tf, exact(&ea), exact(&ta)) {
                                (Form::Lin(x, c), _, _, Some(k)) => op(c, k).map_or(Form::None, |c| Form::Lin(x, c)),
                                (_, Form::Lin(x, c), Some(k), _) if !sub => {
                                    op(c, k).map_or(Form::None, |c| Form::Lin(x, c))
                                }
                                _ => Form::None,
                            };
                            let max = if lv == 0 { INFINITY } else { MAX_DIMEN };
                            e.width = num_error(add_or_sub(e.width, t.width, max, sub));
                            (ea, ea_x) = step(
                                &[&ea, &ta],
                                &[ea_x.as_deref(), ta_x.as_deref()],
                                members,
                                (-max, max),
                                false,
                                |v| add_or_sub(v[0], v[1], max, sub),
                                |v| op(v[0], v[1]),
                                &mut err,
                            );
                            if let Some((lo, hi)) = bound
                                && !err.0
                                && lo.max(ea.lo) <= hi.min(ea.hi)
                            {
                                ea = Num::range(lo.max(ea.lo), hi.min(ea.hi));
                                if let Some(xs) = ea_x.as_mut() {
                                    xs.retain(|v| (lo..=hi).contains(v));
                                }
                            }
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
                    let Some(saved) = stack.pop() else { break 'restart (e, ea, ea_x) };
                    f = e;
                    fa = ea;
                    fa_x = ea_x;
                    ff = ef;
                    Frame { l, s, r, e, t, n, ea, ta, na, ea_x, ta_x, na_x, ef, tf } = saved;
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
        let quantity = match kind {
            RegKind::Count => crate::value::Quantity::Int,
            RegKind::Dimen => crate::value::Quantity::Dimen,
            _ if unknown => return Value::Unknown,
            _ => {
                let e = if arith { zero } else { e };
                return if kind == RegKind::Skip { Value::Glue(e) } else { Value::MuGlue(e) };
            }
        };
        let zero = Value::from_num(quantity, Num::exact(0));
        if err.1 {
            return zero;
        }
        let value = match ea_x {
            Some(members) => members
                .into_iter()
                .map(|n| Value::from_num(quantity, Num::exact(n)))
                .reduce(|a, b| a.join(&b, self.env.sets))
                .unwrap_or(Value::Unknown),
            None => Value::from_num(quantity, ea),
        };
        // Overflow zeroes the whole expression: that 0 is one more member,
        // not a bound the interval stretches to.
        if err.0 { value.join(&zero, self.env.sets) } else { value }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::{WORD_MAX, WORD_MIN, mult_and_add, wrap, x_over_n};

    const EDGES: [i64; 14] =
        [0, 1, -1, 7, -7, 1 << 15, 46341, 65536, MAX_DIMEN, -MAX_DIMEN, WORD_MAX, WORD_MIN, WORD_MIN + 1, 1 << 30];

    /// Intervals around TeX's edge values, each with the concrete values it
    /// stands for.
    fn boxes() -> Vec<(Num, Vec<i64>)> {
        let mut out = Vec::new();
        for &c in &EDGES {
            for w in [0i64, 1, 3, 9] {
                let lo = (c - w).max(WORD_MIN);
                let hi = (c + w).min(WORD_MAX);
                out.push((Num::range(lo, hi), (lo..=hi).collect()));
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
                    assert!(num.lo <= v && v <= num.hi, "{c:?} gives {v}, not in {num:?}");
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
                    let got = Num::lift(&[&a.0, &b.0], range, wrap_, exact, ideal);
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
                    let got = Num::lift(&[&a.0, &b.0, &d.0], (-MAX_DIMEN, MAX_DIMEN), false, exact, |v| {
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
