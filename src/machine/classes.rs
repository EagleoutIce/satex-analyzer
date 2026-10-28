//! Token classes: the concrete tokens an abstract token may stand for.
//! One view for every kind (typed digits, typed text, one-of texts,
//! repetitions, unknown text), read by the conditionals and the scans.

use super::Machine;
use crate::tex::{Catcode, TextClass, Tok, Token, UNKNOWN_DIGIT, UNKNOWN_LETTER, UNKNOWN_OTHER};
use crate::tex::{Meaning, Sym};

/// The concrete tokens a token may be. Character codes are exact, or an
/// `UNKNOWN_*` code for the range it names
/// ([`crate::tex::may_equal_unknown_char`]).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TokenClass {
    pub chars: Vec<(u32, Catcode)>,
    /// Unknown characters it may be, by their class (typed text).
    pub text: TextClass,
    /// Control sequences it may be. `None`: any.
    pub cs: Option<Vec<Sym>>,
    /// It may be no token at all.
    pub empty: bool,
    /// It may be several tokens.
    pub several: bool,
}

/// What `\if` and `\ifcat` compare: a character code and category.
/// tex.web § 506 gives a control sequence code 256, past every 8-bit
/// character; XeTeX's `too_big_usv` is past every Unicode one.
pub(crate) type Operand = (u32, u8);
pub(crate) const CONTROL_SEQUENCE: Operand = (0x11_0000, 16);

/// No token: the unit of [`TokenClass::join`].
impl Default for TokenClass {
    fn default() -> Self {
        Self { chars: Vec::new(), text: TextClass::EMPTY, cs: Some(Vec::new()), empty: false, several: false }
    }
}

impl TokenClass {
    /// Any token, any number of them.
    pub fn any() -> Self {
        Self { chars: Vec::new(), text: TextClass::ANY, cs: None, empty: true, several: true }
    }

    fn one(tok: Tok) -> Self {
        match tok {
            Tok::Chr(c, k) => Self { chars: vec![(c as u32, k)], ..Self::default() },
            Tok::Cs(sym) => Self { cs: Some(vec![sym]), ..Self::default() },
            _ => Self { cs: None, ..Self::default() },
        }
    }

    pub fn join(&mut self, other: Self) {
        for c in other.chars {
            if !self.chars.contains(&c) {
                self.chars.push(c);
            }
        }
        self.cs = match (self.cs.take(), other.cs) {
            (Some(mut a), Some(b)) => {
                a.extend(b.into_iter().filter(|s| !a.contains(s)).collect::<Vec<_>>());
                Some(a)
            }
            _ => None,
        };
        self.text = self.text.union(other.text);
        self.empty |= other.empty;
        self.several |= other.several;
    }

    /// The tokens of typed unknown text of `class`.
    pub(crate) fn of_text(class: TextClass, one: bool, cs: Vec<Sym>) -> Self {
        let cs = (!class.contains(TextClass::CS)).then_some(cs);
        Self { chars: Vec::new(), text: class, cs, empty: !one, several: true }
    }

    /// The characters text of `class` may begin with, one code standing
    /// for each range.
    fn text_chars(class: TextClass) -> Vec<(u32, Catcode)> {
        let mut chars = Vec::new();
        let other = Catcode::Other;
        if class.meets(TextClass::DIGIT) {
            chars.push((UNKNOWN_DIGIT as u32, other));
        }
        if class.meets(TextClass::SIGN) {
            chars.extend("+-.,".chars().map(|c| (c as u32, other)));
        }
        if class.meets(TextClass::LETTER) {
            chars.push((UNKNOWN_LETTER as u32, Catcode::Letter));
        }
        if class.meets(TextClass::OTHER) {
            let cats = [other, Catcode::Math, Catcode::Tab, Catcode::Sup, Catcode::Sub];
            chars.extend(cats.map(|k| (UNKNOWN_OTHER as u32, k)));
            chars.extend(cats.map(|k| (UNKNOWN_LETTER as u32, k)));
        }
        if class.meets(TextClass::SPACE) {
            chars.push((' ' as u32, Catcode::Space));
        }
        if class.meets(TextClass::BRACE) {
            chars.extend([Catcode::Begin, Catcode::End].map(|k| (UNKNOWN_OTHER as u32, k)));
        }
        chars
    }

    /// Whether it may hold a brace.
    pub fn may_brace(&self) -> bool {
        self.text.contains(TextClass::BRACE) || self.chars.iter().any(|c| matches!(c.1, Catcode::Begin | Catcode::End))
    }

    /// Whether no token may be both.
    pub fn disjoint(&self, other: &Self) -> bool {
        let cs = match (&self.cs, &other.cs) {
            (Some(a), Some(b)) => a.iter().any(|s| b.contains(s)),
            (Some(x), None) | (None, Some(x)) => !x.is_empty(),
            (None, None) => true,
        };
        let chars = |c: &Self| c.chars.iter().chain(&Self::text_chars(c.text)).copied().collect::<Vec<_>>();
        let (xs, ys) = (chars(self), chars(other));
        let meet = |&(c, k): &(u32, Catcode)| {
            ys.iter().any(|&(d, l)| k == l && (c == d || crate::tex::may_equal_unknown_char(c, d)))
        };
        !cs && !xs.iter().any(meet)
    }

    /// Whether a scan for `tok` may find it here.
    pub fn may_be(&self, tok: Tok) -> bool {
        match tok {
            Tok::Cs(sym) => self.cs.as_ref().is_none_or(|cs| cs.contains(&sym)),
            Tok::Chr(c, k) => {
                self.text.holds(tok)
                    || self
                        .chars
                        .iter()
                        .any(|&(d, l)| l == k && (d == c as u32 || crate::tex::may_equal_unknown_char(d, c as u32)))
            }
            _ => true,
        }
    }
}

/// Whether `\if` (`by_code`) or `\ifcat` may and must find two operands
/// equal: `(may, must)`.
fn compare(a: Operand, b: Operand, by_code: bool) -> (bool, bool) {
    if !by_code {
        return (a.1 == b.1, a.1 == b.1);
    }
    let special = |c: u32| char::from_u32(c).is_some_and(crate::tex::is_unknown_char);
    let may = a.0 == b.0 || crate::tex::may_equal_unknown_char(a.0, b.0);
    (may, a.0 == b.0 && !special(a.0))
}

/// `\if` or `\ifcat` over operand sets: true when every pair agrees,
/// false when none can, else undecided (`None`).
pub(crate) fn decide(xs: &[Operand], ys: &[Operand], by_code: bool) -> Option<bool> {
    let (mut may, mut must) = (false, true);
    for &x in xs {
        for &y in ys {
            let (m, n) = compare(x, y, by_code);
            may |= m;
            must &= n;
        }
    }
    match (may, must) {
        (false, _) => Some(false),
        (true, true) => Some(true),
        _ => None,
    }
}

impl Machine<'_> {
    /// The classes of `t`: of its first token when `first`, else of every
    /// token it may hold.
    pub(crate) fn token_class(&self, t: Token, first: bool) -> TokenClass {
        self.class_at(t, first, 0)
    }

    /// Whether two token lists can never be equal: both hold a token,
    /// and their first ones differ in class.
    pub(crate) fn texts_differ(&self, x: &[Token], y: &[Token]) -> bool {
        let first = |text: &[Token]| {
            let mut class = TokenClass { empty: true, ..TokenClass::default() };
            for t in text {
                let c = self.class_at(*t, true, 0);
                let more = c.empty;
                class.join(TokenClass { empty: false, ..c });
                if !more {
                    class.empty = false;
                    break;
                }
            }
            class
        };
        let (a, b) = (first(x), first(y));
        !a.empty && !b.empty && a.disjoint(&b)
    }

    fn class_at(&self, t: Token, first: bool, depth: usize) -> TokenClass {
        let Tok::Cs(sym) = t.tok else { return TokenClass::one(t.tok) };
        if let Some(texts) = self.one_of_members(t) {
            if depth > 4 {
                return TokenClass::any();
            }
            let mut class = TokenClass::default();
            for text in &texts {
                class.several |= text.len() > 1;
                if text.is_empty() {
                    class.empty = true;
                }
                // A repetition holds itself: its own tokens are the rest.
                let mut may_be_empty = true;
                for x in text.iter().filter(|x| x.tok != t.tok) {
                    let c = self.class_at(*x, first, depth + 1);
                    let more = c.empty;
                    class.join(TokenClass { empty: false, ..c });
                    if !more {
                        may_be_empty = false;
                        if first {
                            break;
                        }
                    }
                }
                class.empty |= may_be_empty;
            }
            return class;
        }
        // Plain unknown digits may be signed: the character they mean.
        if sym == self.unknown_digits || sym == self.unknown_more {
            let chars = vec![(crate::tex::UNKNOWN_CHAR as u32, Catcode::Other)];
            let text = TextClass::DIGIT;
            return TokenClass { chars, text, cs: Some(Vec::new()), empty: sym == self.unknown_more, several: true };
        }
        if self.name(sym).starts_with(crate::scan::TYPED_DIGITS) {
            let other = |c: char| (c as u32, Catcode::Other);
            // A slice from place 0 spells a number with no leading zero.
            let num = self.typed_digits_of(sym).filter(|_| self.typed_slice(sym).is_none_or(|(.., i, _)| i == 0));
            let mut chars: Vec<(u32, Catcode)> = match (num, first) {
                (Some(num), true) => crate::scan::first_digits(num).into_iter().map(other).collect(),
                _ => ('0'..='9').map(other).collect(),
            };
            if num.is_none_or(|n| n.lo < 0) {
                chars.push(other('-'));
            }
            let several = num.is_none_or(|n| n.lo < 0 || n.hi > 9);
            return TokenClass { chars, cs: Some(Vec::new()), empty: false, several, ..TokenClass::default() };
        }
        if let Some((class, one, cs)) = self.typed_shape(t) {
            return TokenClass::of_text(class, one, cs);
        }
        if self.is_unknown_marker(sym) {
            return TokenClass::any();
        }
        TokenClass::one(t.tok)
    }

    /// The operands `\if` may read from `t` as its first token, a
    /// control sequence by its meaning; `None` where a member must be
    /// expanded first, or may be empty, or any token.
    pub(crate) fn class_operands(&self, t: Token, noexpanded: bool) -> Option<Vec<Operand>> {
        let class = self.token_class(t, true);
        if class.empty {
            return None;
        }
        if !noexpanded && class.chars.iter().any(|c| c.1 == Catcode::Active) {
            return None;
        }
        let chars = class.chars.iter().chain(&TokenClass::text_chars(class.text)).copied().collect::<Vec<_>>();
        let mut out: Vec<Operand> = chars.iter().map(|&(c, k)| (c, k as u8)).collect();
        for sym in class.cs? {
            let operand = match self.env.meaning(sym) {
                Meaning::Char(c, k) => (c as u32, k as u8),
                Meaning::Unknown => return None,
                Meaning::Undefined if !noexpanded => return None,
                m if !noexpanded && self.expandable_meaning(&m) => return None,
                _ if self.is_unknown_marker(sym) => return None,
                _ => CONTROL_SEQUENCE,
            };
            if !out.contains(&operand) {
                out.push(operand);
            }
        }
        Some(out)
    }
}
