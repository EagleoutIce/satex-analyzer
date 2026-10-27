//! Concrete TeX object model: category codes, tokens, the mouth, and meanings.
//! References: The TeXbook, tex.web, xparse, ltcmd.

use crate::fixed::HashMap;
use std::fmt;
use std::rc::Rc;

use serde::{Deserialize, Serialize};

use crate::builtins::Primitive;

pub type FileId = u16;

/// A position in a source file: which [`FileId`], which line, which column.
/// Every [`Token`] carries one, and it is what every fact and diagnostic
/// points back to.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default, PartialOrd, Ord)]
pub struct Span {
    pub file: FileId,
    pub line: u32,
    pub col: u32,
}

/// How a [`Span`] and a [`Sym`] are written by serde. Without a
/// [`codec::Mode`] in force they are written as they are; a package cache
/// writes them against its own name and file tables instead, so that it can
/// be installed into a run that numbers names and files differently.
pub mod codec {
    use crate::fixed::HashMap;
    use std::cell::RefCell;
    use std::rc::Rc;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::{FileId, Span, Sym};

    pub enum Mode {
        /// Names and paths are written out in full: equal bytes mean equal
        /// values in any run.
        Hash { names: Vec<Rc<str>>, files: Vec<Rc<str>> },
        /// Numbers are written as indices into tables built on the way.
        Encode { syms: HashMap<u32, u32>, sym_order: Vec<u32>, files: HashMap<FileId, u32>, file_order: Vec<FileId> },
        /// Indices are read back through the tables of the run installing it.
        Decode { syms: Vec<Sym>, files: Vec<FileId> },
    }

    thread_local! {
        static MODE: RefCell<Option<Mode>> = const { RefCell::new(None) };
    }

    /// Run `f` with `mode` in force and hand the mode back with its result.
    pub fn with<R>(mode: Mode, f: impl FnOnce() -> R) -> (R, Mode) {
        let previous = MODE.with(|m| m.borrow_mut().replace(mode));
        let result = f();
        let mode = MODE.with(|m| std::mem::replace(&mut *m.borrow_mut(), previous));
        (result, mode.expect("the codec mode is still in force"))
    }

    enum Out {
        Plain,
        Text(Rc<str>),
        Index(u32),
    }

    fn sym_out(sym: Sym) -> Out {
        MODE.with(|m| match &mut *m.borrow_mut() {
            None | Some(Mode::Decode { .. }) => Out::Plain,
            Some(Mode::Hash { names, .. }) => {
                Out::Text(names.get(sym.0 as usize).cloned().unwrap_or_else(|| Rc::from("")))
            }
            Some(Mode::Encode { syms, sym_order, .. }) => {
                let next = syms.len() as u32;
                Out::Index(*syms.entry(sym.0).or_insert_with(|| {
                    sym_order.push(sym.0);
                    next
                }))
            }
        })
    }

    fn file_out(file: FileId) -> Out {
        MODE.with(|m| match &mut *m.borrow_mut() {
            None | Some(Mode::Decode { .. }) => Out::Plain,
            Some(Mode::Hash { files, .. }) => {
                Out::Text(files.get(file as usize).cloned().unwrap_or_else(|| Rc::from(format!("#{file}"))))
            }
            Some(Mode::Encode { files, file_order, .. }) => {
                let next = files.len() as u32;
                Out::Index(*files.entry(file).or_insert_with(|| {
                    file_order.push(file);
                    next
                }))
            }
        })
    }

    #[derive(Serialize, Deserialize)]
    #[serde(rename = "Span")]
    struct SpanRepr<F> {
        file: F,
        line: u32,
        col: u32,
    }

    impl Serialize for Sym {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            match sym_out(*self) {
                Out::Plain => s.serialize_newtype_struct("Sym", &self.0),
                Out::Text(name) => s.serialize_str(&name),
                Out::Index(i) => s.serialize_newtype_struct("Sym", &i),
            }
        }
    }

    impl<'de> Deserialize<'de> for Sym {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Sym, D::Error> {
            #[derive(Deserialize)]
            #[serde(rename = "Sym")]
            struct Raw(u32);
            let Raw(n) = Raw::deserialize(d)?;
            Ok(MODE.with(|m| match &*m.borrow() {
                Some(Mode::Decode { syms, .. }) => syms.get(n as usize).copied().unwrap_or(Sym(0)),
                _ => Sym(n),
            }))
        }
    }

    impl Serialize for Span {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let (line, col) = (self.line, self.col);
            match file_out(self.file) {
                Out::Plain => SpanRepr { file: self.file, line, col }.serialize(s),
                Out::Text(path) => SpanRepr { file: &*path, line, col }.serialize(s),
                Out::Index(i) => SpanRepr { file: i, line, col }.serialize(s),
            }
        }
    }

    impl<'de> Deserialize<'de> for Span {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Span, D::Error> {
            let raw = SpanRepr::<u32>::deserialize(d)?;
            let file = MODE.with(|m| match &*m.borrow() {
                Some(Mode::Decode { files, .. }) => files.get(raw.file as usize).copied().unwrap_or(0),
                _ => raw.file as FileId,
            });
            Ok(Span { file, line: raw.line, col: raw.col })
        }
    }
}

impl Span {
    pub fn new(file: FileId, line: u32, col: u32) -> Self {
        Self { file, line, col }
    }
    /// Source order within a document, used by positional filters.
    pub fn before(&self, line: u32, col: u32) -> bool {
        self.line < line || (self.line == line && self.col < col)
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.col)
    }
}

/// The interned name of a control sequence: an index into an [`Interner`].
/// [`Tok::Cs`] and [`Meaning`] carry a `Sym` rather than the name's text.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, PartialOrd, Ord)]
pub struct Sym(pub u32);

/// Active characters have their own region of the meaning table (tex.web
/// § 344), so `~` and `\~` never share a meaning. satex keeps that region
/// under a prefix that cannot be written in a source file.
pub const ACTIVE: char = '\u{1}';

/// The code of a character satex does not know, such as a digit of an
/// unknown `\the\count0`: U+10FFFF is a noncharacter no document spells.
pub const UNKNOWN_CHAR: char = '\u{10FFFF}';

/// The name of the token that stands for text satex cannot know
/// ([`Machine::unknown`](crate::machine::Machine)): no file can spell it
/// with the catcodes a run starts with.
pub const UNKNOWN_CS: &str = "\u{2}unknown:";

/// What the names of every kind of unknown text satex makes begin with:
/// [`UNKNOWN_CS`], typed digits, unknown names and the rest.
pub const UNKNOWN_PREFIX: &str = "\u{2}unknown";

/// One digit, `0` to `9`, of an unknown number: what taking one token
/// from unknown digits yields (a character of category 12, tex.web § 465).
pub const UNKNOWN_DIGIT: char = '\u{10FFFE}';

/// One digit, `1` to `9`, of an unknown number: the first of a spelling
/// that `\the` and `\number` print, which never has a leading zero
/// (tex.web §§ 103, 104).
pub const UNKNOWN_NONZERO: char = '\u{10FFFB}';

/// Any one character: what `\lowercase` makes of a character whose code
/// is not known, its category kept.
pub const UNKNOWN_ANY: char = '\u{10FFFA}';

/// A line of only this character parts the texts a file the build wrote
/// may hold: one per path that wrote it otherwise, up to `limits.meaning_set`.
pub const TEXT_OR: char = '\u{10FFF9}';

/// One letter (category 11) of unknown text.
pub const UNKNOWN_LETTER: char = '\u{10FFFD}';

/// One character of category 12 of unknown text, not known to be a digit.
pub const UNKNOWN_OTHER: char = '\u{10FFFC}';

/// Unknown text of a known class written to a file: one character per
/// class and lower bound (`class * 2 + one`), read back as that text.
const WRITTEN_UNKNOWN: u32 = 0x10_0000;

/// The character that writes unknown text of `class` to a file.
pub fn written_unknown(class: TextClass, one: bool) -> char {
    char::from_u32(WRITTEN_UNKNOWN + u32::from(class.0) * 2 + u32::from(one)).unwrap_or(UNKNOWN_ANY)
}

/// The class and lower bound a [`written_unknown`] character stands for.
pub fn written_class(c: char) -> Option<(TextClass, bool)> {
    let i = (c as u32).checked_sub(WRITTEN_UNKNOWN).filter(|i| *i < 256)?;
    Some((TextClass((i / 2) as u8), i % 2 == 1))
}

/// Whether `c` is one character of unknown text.
pub fn is_unknown_char(c: char) -> bool {
    matches!(c, UNKNOWN_CHAR | UNKNOWN_DIGIT | UNKNOWN_NONZERO | UNKNOWN_LETTER | UNKNOWN_OTHER | UNKNOWN_ANY)
}

/// Whether `c` is one unknown digit.
pub fn is_unknown_digit(c: char) -> bool {
    matches!(c, UNKNOWN_DIGIT | UNKNOWN_NONZERO)
}

/// The characters an unknown character of a number may be.
pub fn unknown_number_chars(c: char) -> Option<&'static str> {
    match c {
        UNKNOWN_DIGIT => Some("0123456789"),
        UNKNOWN_NONZERO => Some("123456789"),
        UNKNOWN_CHAR => Some("0123456789-"),
        _ => None,
    }
}

/// Whether character codes `a` and `b` may be equal when one is
/// [`UNKNOWN_CHAR`] (the digits of a number or its sign),
/// [`UNKNOWN_DIGIT`] (one digit), [`UNKNOWN_NONZERO`] (one digit but `0`),
/// [`UNKNOWN_LETTER`] or [`UNKNOWN_OTHER`].
pub fn may_equal_unknown_char(a: u32, b: u32) -> bool {
    let (letter, other) = (UNKNOWN_LETTER as u32, UNKNOWN_OTHER as u32);
    if a == UNKNOWN_ANY as u32 || b == UNKNOWN_ANY as u32 {
        return true;
    }
    let is = |c: u32, f: fn(char) -> bool| char::from_u32(c).is_some_and(f);
    // What each unknown character may be, the unknown ones included.
    let may_be = |c: u32, f: fn(char) -> bool| is(c, is_unknown_char) || is(c, f);
    if a == letter || b == letter {
        let x = if a == letter { b } else { a };
        return may_be(x, char::is_alphabetic);
    }
    if a == other || b == other {
        let x = if a == other { b } else { a };
        return may_be(x, |c| !c.is_alphabetic());
    }
    // Otherwise the characters of a number each may be meet.
    let (Some(x), Some(y)) = (char::from_u32(a), char::from_u32(b)) else { return false };
    match (unknown_number_chars(x), unknown_number_chars(y)) {
        (Some(xs), Some(ys)) => xs.chars().any(|c| ys.contains(c)),
        (Some(xs), None) => xs.contains(y),
        (None, Some(ys)) => ys.contains(x),
        (None, None) => false,
    }
}

/// What unknown text may hold: a set of token classes, ordered by
/// inclusion. Digits ⊑ a number (sign, point) ⊑ a dimension (unit letters)
/// ⊑ balanced text without control sequences ⊑ any text.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct TextClass(pub u8);

impl TextClass {
    pub const EMPTY: Self = Self(0);
    /// `0`–`9` of category 12.
    pub const DIGIT: Self = Self(1);
    /// `+`, `-`, `.` and `,` of category 12.
    pub const SIGN: Self = Self(2);
    /// Category 11.
    pub const LETTER: Self = Self(4);
    /// Any other character of category 12, or of another character
    /// category that does not open a group.
    pub const OTHER: Self = Self(8);
    pub const SPACE: Self = Self(16);
    /// Balanced braces.
    pub const BRACE: Self = Self(32);
    /// Control sequences, and so anything.
    pub const CS: Self = Self(64);
    /// What nothing known types to: characters neither active nor braces.
    pub const WIDE: Self = Self(Self::PRINTED.0 | Self::LETTER.0);
    pub const NUMBER: Self = Self(Self::DIGIT.0 | Self::SIGN.0);
    /// What `\string`, `\meaning` and `\detokenize` print: characters of
    /// category 12 and spaces (tex.web § 464).
    pub const PRINTED: Self = Self(Self::NUMBER.0 | Self::OTHER.0 | Self::SPACE.0);
    pub const ANY: Self = Self(127);
    /// The one-letter spellings of the classes, in bit order: how a typed
    /// token names its class.
    const LETTERS: [char; 7] = ['d', 's', 'l', 'o', '_', 'b', 'c'];

    /// What `\detokenize` makes of text of this class: letters and
    /// braces become category 12, a name its characters and perhaps a
    /// space (tex.web § 262, § 464).
    pub fn printed(self) -> Self {
        let mut c = self.0 & Self::PRINTED.0;
        if self.meets(Self(Self::LETTER.0 | Self::BRACE.0)) {
            c |= Self::OTHER.0;
        }
        if self.meets(Self::CS) {
            c |= Self::PRINTED.0;
        }
        Self(c)
    }

    pub fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn meets(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// The class of one token that is known, or an unknown character.
    pub fn of(tok: Tok) -> Self {
        match tok {
            Tok::Chr(c, Catcode::Other) if c.is_ascii_digit() || is_unknown_digit(c) => Self::DIGIT,
            Tok::Chr(c, Catcode::Other) if "+-.,".contains(c) => Self::SIGN,
            Tok::Chr(c, Catcode::Other) if c == UNKNOWN_CHAR => Self::NUMBER,
            Tok::Chr(_, Catcode::Letter) => Self::LETTER,
            Tok::Chr(_, Catcode::Space) => Self::SPACE,
            Tok::Chr(_, Catcode::Begin | Catcode::End) => Self::BRACE,
            Tok::Chr(_, Catcode::Active | Catcode::Param) => Self::CS,
            Tok::Chr(..) => Self::OTHER,
            _ => Self::CS,
        }
    }

    /// Whether text of this class may hold `tok`.
    pub fn holds(self, tok: Tok) -> bool {
        self.meets(Self::of(tok))
    }

    /// The class as its letters.
    pub fn spell(self) -> String {
        Self::LETTERS.iter().enumerate().filter(|(i, _)| self.0 & (1 << i) != 0).map(|(_, c)| *c).collect()
    }

    pub fn parse(text: &str) -> Option<Self> {
        let mut class = Self::EMPTY;
        for c in text.chars() {
            class.0 |= 1 << Self::LETTERS.iter().position(|l| *l == c)?;
        }
        Some(class)
    }

    /// One character of this class, when every character it may hold has
    /// the same category: what taking one token from such text yields.
    pub fn one_char(self) -> Option<Tok> {
        if self == Self::EMPTY || self.meets(Self(Self::SPACE.0 | Self::BRACE.0 | Self::CS.0)) {
            return None;
        }
        Some(match self {
            Self::DIGIT => Tok::Chr(UNKNOWN_DIGIT, Catcode::Other),
            c if Self::NUMBER.contains(c) => Tok::Chr(UNKNOWN_CHAR, Catcode::Other),
            Self::LETTER => Tok::Chr(UNKNOWN_LETTER, Catcode::Letter),
            c if c.meets(Self::LETTER) => return None,
            _ => Tok::Chr(UNKNOWN_OTHER, Catcode::Other),
        })
    }
}

/// Hashes names for the interner: fast, as the table is never iterated.
#[derive(Default, Clone, Copy)]
pub struct NameHash;

pub struct NameHasher(u64);

impl std::hash::BuildHasher for NameHash {
    type Hasher = NameHasher;
    fn build_hasher(&self) -> NameHasher {
        NameHasher(0)
    }
}

impl std::hash::Hasher for NameHasher {
    fn write(&mut self, bytes: &[u8]) {
        const K: u64 = 0x517c_c1b7_2722_0a95;
        let (chunks, remainder) = bytes.as_chunks::<8>();
        for &c in chunks {
            self.0 = (self.0.rotate_left(5) ^ u64::from_le_bytes(c)).wrapping_mul(K);
        }
        let mut tail = 0u64;
        for &b in remainder {
            tail = tail << 8 | u64::from(b);
        }
        self.0 = (self.0.rotate_left(5) ^ tail).wrapping_mul(K);
    }
    fn write_u64(&mut self, w: u64) {
        self.0 = (self.0.rotate_left(5) ^ w).wrapping_mul(0x517c_c1b7_2722_0a95);
    }
    fn write_u8(&mut self, w: u8) {
        self.write_u64(u64::from(w));
    }
    fn write_u32(&mut self, w: u32) {
        self.write_u64(u64::from(w));
    }
    fn write_usize(&mut self, w: usize) {
        self.write_u64(w as u64);
    }
    fn write_isize(&mut self, w: isize) {
        self.write_u64(w as u64);
    }
    fn finish(&self) -> u64 {
        self.0 ^ (self.0 >> 29)
    }
}

/// The table of control sequence names: every [`Sym`] is an index into it,
/// and [`Token`] carries the [`Sym`] rather than the text.
#[derive(Default, Clone)]
pub struct Interner {
    map: std::collections::HashMap<Rc<str>, Sym, NameHash>,
    names: Vec<Rc<str>>,
    /// While a package cache is being captured: every name asked for, in the
    /// order it was first asked for, which is the order a run that does not
    /// have them yet would number them in.
    touched: Option<(Vec<u64>, Vec<Sym>)>,
    /// Numbered register names already interned, so a hot read formats none.
    registers: std::collections::HashMap<(&'static str, i64), Sym, NameHash>,
}

impl Interner {
    /// Rebuild an interner from a stored name list, so that symbols recorded
    /// against it stay valid.
    pub fn from_names(names: Vec<String>) -> Interner {
        let mut interner = Interner::default();
        interner.map.reserve(names.len());
        interner.names.reserve(names.len());
        for name in names {
            interner.intern(&name);
        }
        interner
    }

    pub fn intern(&mut self, s: &str) -> Sym {
        let sym = match self.map.get(s) {
            Some(&sym) => sym,
            None => {
                let rc: Rc<str> = Rc::from(s);
                let sym = Sym(self.names.len() as u32);
                self.names.push(rc.clone());
                self.map.insert(rc, sym);
                sym
            }
        };
        self.touch(sym);
        sym
    }

    /// The name of a numbered register: `prefix`, `kind`, then `index`.
    pub fn register(&mut self, prefix: char, kind: &'static str, index: i64) -> Sym {
        let sym = match self.registers.get(&(kind, index)) {
            Some(&sym) => sym,
            None => {
                let sym = self.intern(&format!("{prefix}{kind}{index}"));
                self.registers.insert((kind, index), sym);
                sym
            }
        };
        self.touch(sym);
        sym
    }

    fn touch(&mut self, sym: Sym) {
        if let Some((seen, order)) = &mut self.touched {
            let word = sym.0 as usize / 64;
            if seen.len() <= word {
                seen.resize(word + 1, 0);
            }
            if seen[word] & (1 << (sym.0 % 64)) == 0 {
                seen[word] |= 1 << (sym.0 % 64);
                order.push(sym);
            }
        }
    }

    /// The names asked for so far while recording, in order.
    pub fn touches(&self) -> &[Sym] {
        self.touched.as_ref().map_or(&[][..], |(_, order)| &order[..])
    }

    /// Start recording the names asked for, or stop and hand them back.
    pub fn record_touches(&mut self, on: bool) -> Vec<Sym> {
        let previous = std::mem::replace(&mut self.touched, on.then(|| (Vec::new(), Vec::new())));
        previous.map(|(_, order)| order).unwrap_or_default()
    }
    pub fn lookup(&self, s: &str) -> Option<Sym> {
        self.map.get(s).copied()
    }
    pub fn name(&self, s: Sym) -> &str {
        self.names.get(s.0 as usize).map(|r| &**r).unwrap_or("?")
    }
    /// `\foo`, as TeX prints it (tex.web § 49).
    pub fn cs(&self, s: Sym) -> String {
        let name = self.name(s);
        // An active character prints as itself, not as a control sequence.
        if let Some(rest) = name.strip_prefix(ACTIVE) {
            return rest.to_string();
        }
        let mut out = String::from("\\");
        for ch in self.name(s).chars() {
            match ch {
                '\u{0}'..='\u{1f}' => {
                    out.push_str("^^");
                    out.push(char::from(ch as u8 + 64));
                }
                '\u{7f}' => out.push_str("^^?"),
                _ => out.push(ch),
            }
        }
        out
    }

    pub fn len(&self) -> usize {
        self.names.len()
    }
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
    /// Every name, in symbol order, sharing the interner's strings.
    pub fn names(&self) -> Vec<Rc<str>> {
        self.names.clone()
    }
}

/// The 16 category codes of The TeXbook § 232, the property a [`CatcodeTable`]
/// assigns to every character before the [`Mouth`] can turn it into a
/// [`Token`].
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum Catcode {
    Escape = 0,
    Begin = 1,
    End = 2,
    Math = 3,
    Tab = 4,
    Eol = 5,
    Param = 6,
    Sup = 7,
    Sub = 8,
    Ignored = 9,
    Space = 10,
    Letter = 11,
    Other = 12,
    Active = 13,
    Comment = 14,
    Invalid = 15,
}

impl Catcode {
    const BY_CODE: [Catcode; 16] = {
        use Catcode::*;
        [Escape, Begin, End, Math, Tab, Eol, Param, Sup, Sub, Ignored, Space, Letter, Other, Active, Comment, Invalid]
    };

    pub fn from_u8(n: u8) -> Option<Catcode> {
        Self::BY_CODE.get(n as usize).copied()
    }
}

/// The catcode regime in force: every character maps to a [`Catcode`], which
/// the [`Mouth`] consults while it tokenizes. Local like other assignments,
/// so `\catcode` changes are undone by [`Env`](crate::env::Env)'s save stack.
/// The characters past 255 are shared between copies: a path through an
/// undecided conditional copies the table and compares it at every join,
/// and unicode-math gives thousands of them a category.
#[derive(Clone)]
pub struct CatcodeTable {
    ascii: [Catcode; 256],
    wide: Rc<HashMap<char, Catcode>>,
    /// A character past 255 not in `wide`: 12 in INITEX, 11 once a format
    /// has classified the letters.
    wide_default: Catcode,
    /// Characters whose category satex could not pin down, each with the
    /// categories it may have, one bit each (all for the top of the
    /// lattice). tex.web has no such state. [`CatcodeTable::get`] still
    /// answers with the last one known, a guess no reader may trust
    /// without asking [`CatcodeTable::is_unknown`] first.
    unknown: Rc<std::collections::BTreeMap<char, u16>>,
    /// The hull of the character codes a `\catcode` through a number satex
    /// could not pin down may have named: those categories are unknown but
    /// the ones assigned since, which `known` holds.
    reach: Option<(u32, u32)>,
    /// The categories those writes may have stored, one bit each: a character
    /// in reach is its last known category or one of these.
    reach_cats: u16,
    known: Rc<std::collections::BTreeSet<char>>,
}

impl PartialEq for CatcodeTable {
    fn eq(&self, other: &Self) -> bool {
        self.ascii == other.ascii
            && self.wide_default == other.wide_default
            && (Rc::ptr_eq(&self.wide, &other.wide) || self.wide == other.wide)
            && (Rc::ptr_eq(&self.unknown, &other.unknown) || self.unknown == other.unknown)
            && self.reach == other.reach
            && self.reach_cats == other.reach_cats
            && (Rc::ptr_eq(&self.known, &other.known) || self.known == other.known)
    }
}

impl Default for CatcodeTable {
    fn default() -> Self {
        Self::latex()
    }
}

impl CatcodeTable {
    /// INITEX's table (tex.web § 232): letters 11, `\\` 0, `%` 14, `^^@` 9,
    /// `^^M` 5, space 10, `^^?` 15, everything else 12.
    pub fn initex() -> Self {
        let mut ascii = [Catcode::Other; 256];
        for c in (b'a'..=b'z').chain(b'A'..=b'Z') {
            ascii[c as usize] = Catcode::Letter;
        }
        ascii[b'\\' as usize] = Catcode::Escape;
        ascii[b'%' as usize] = Catcode::Comment;
        ascii[0] = Catcode::Ignored;
        ascii[b'\r' as usize] = Catcode::Eol;
        ascii[b' ' as usize] = Catcode::Space;
        ascii[0x7f] = Catcode::Invalid;
        Self {
            ascii,
            wide: Rc::default(),
            wide_default: Catcode::Other,
            unknown: Rc::default(),
            reach: None,
            reach_cats: 0,
            known: Rc::default(),
        }
    }

    /// INITEX defaults plus the assignments `latex.ltx` makes.
    pub fn latex() -> Self {
        let mut table = Self::initex();
        for (c, cat) in [
            ('{', Catcode::Begin),
            ('}', Catcode::End),
            ('$', Catcode::Math),
            ('&', Catcode::Tab),
            ('#', Catcode::Param),
            ('^', Catcode::Sup),
            ('_', Catcode::Sub),
            ('\t', Catcode::Space),
            ('~', Catcode::Active),
        ] {
            table.set(c, cat);
        }
        table.wide_default = Catcode::Letter;
        table
    }

    pub fn get(&self, c: char) -> Catcode {
        let n = c as u32;
        if n < 256 { self.ascii[n as usize] } else { *self.wide.get(&c).unwrap_or(&self.wide_default) }
    }

    pub fn set(&mut self, c: char, cat: Catcode) {
        let n = c as u32;
        if n < 256 {
            self.ascii[n as usize] = cat;
        } else if self.wide.get(&c) != Some(&cat) {
            Rc::make_mut(&mut self.wide).insert(c, cat);
        }
        // An assignment satex could follow makes the category known again.
        if self.unknown.contains_key(&c) {
            Rc::make_mut(&mut self.unknown).remove(&c);
        }
        if self.reaches(c) && !self.known.contains(&c) {
            Rc::make_mut(&mut self.known).insert(c);
        }
    }

    /// Whether the category of `c` is not known: [`CatcodeTable::get`]'s
    /// answer for it is a guess, and everything that dispatches on it must
    /// widen instead.
    #[inline]
    pub fn is_unknown(&self, c: char) -> bool {
        self.may(c) != 0
    }

    /// The categories `c` may have, one bit each.
    pub fn cats(&self, c: char) -> u16 {
        match self.may(c) {
            0 => 1 << self.get(c) as u8,
            may => may,
        }
    }

    /// [`CatcodeTable::cats`] where more than one, else 0: what the save
    /// stack keeps beside the category.
    pub fn may(&self, c: char) -> u16 {
        let reach = self.reaches(c) && !self.known.contains(&c);
        let own = self.unknown.get(&c).copied().unwrap_or(1 << self.get(c) as u8);
        let cats = if reach { own | self.reach_cats } else { own };
        if cats.count_ones() > 1 { cats } else { 0 }
    }

    /// `\catcode` was given a number satex does not know: the character's
    /// category is anything from here on.
    pub fn set_unknown(&mut self, c: char) {
        self.set_may(c, u16::MAX);
    }

    /// The character's category is one of `cats`, one bit each.
    pub fn set_may(&mut self, c: char, cats: u16) {
        if let Some(cat) = (cats.count_ones() == 1).then(|| Catcode::from_u8(cats.trailing_zeros() as u8)).flatten() {
            return self.set(c, cat);
        }
        Rc::make_mut(&mut self.unknown).insert(c, cats);
        if self.known.contains(&c) {
            Rc::make_mut(&mut self.known).remove(&c);
        }
    }

    /// `\catcode` assigned one of `cats` (one bit per category) through a
    /// character number satex could not pin down (tex.web § 1232): each
    /// character in `lo..=hi` is its old category or one of `cats`.
    pub fn set_reach(&mut self, lo: u32, hi: u32, cats: u16) {
        let reach = self.reach.map_or((lo, hi), |(a, b)| (a.min(lo), b.max(hi)));
        self.reach = Some(reach);
        self.reach_cats |= cats;
        let known: std::collections::BTreeSet<char> =
            self.known.iter().copied().filter(|&c| !(lo..=hi).contains(&(c as u32))).collect();
        self.known = Rc::new(known);
    }

    fn reaches(&self, c: char) -> bool {
        self.reach.is_some_and(|(lo, hi)| (lo..=hi).contains(&(c as u32)))
    }

    /// Put back what [`Env`](crate::env::Env)'s save stack saved: `may` as
    /// [`CatcodeTable::may`] gave it.
    pub fn restore(&mut self, c: char, cat: Catcode, may: u16) {
        self.set(c, cat);
        if may != 0 {
            self.set_may(c, may);
        }
    }

    pub fn diff(&self, other: &CatcodeTable) -> Vec<(char, u8)> {
        let mut out = Vec::new();
        for (code, cat) in self.ascii.iter().enumerate() {
            if other.ascii[code] != *cat
                && let Some(c) = char::from_u32(code as u32)
            {
                out.push((c, *cat as u8));
            }
        }
        for (c, cat) in self.wide.iter() {
            if other.get(*c) != *cat {
                out.push((*c, *cat as u8));
            }
        }
        out
    }

    /// Whether the two tables say different things about `c`.
    fn differs(&self, other: &CatcodeTable, c: char) -> bool {
        self.cats(c) != other.cats(c)
    }

    /// The characters the two tables do not agree about: over the category
    /// lattice of tex.web § 232 the join knows one only where both sides
    /// know it and name the same one.
    pub fn disagreements(&self, other: &CatcodeTable) -> Vec<char> {
        let mut out = Vec::new();
        for code in 0..256u32 {
            let c = char::from_u32(code).expect("below the surrogates");
            if self.differs(other, c) {
                out.push(c);
            }
        }
        // The wide characters two tables may part about: those either one
        // gave a category or marked, the rest being the shared default.
        let marked = [&self.known, &other.known];
        let mut wide: std::collections::BTreeSet<char> = marked
            .iter()
            .flat_map(|set| set.iter().copied())
            .chain(self.unknown.keys().chain(other.unknown.keys()).copied())
            .filter(|c| *c as u32 >= 256)
            .collect();
        if !Rc::ptr_eq(&self.wide, &other.wide) {
            wide.extend(self.wide.keys().chain(other.wide.keys()).copied());
        }
        for c in wide {
            if self.differs(other, c) {
                out.push(c);
            }
        }
        out
    }

    /// Widen this table to cover `other` as well: the reach is the hull of
    /// both (widened, a bound that moved goes to the end of the codes), and
    /// a character in it is still known only where both sides know it and
    /// name the same category.
    fn join_reach(&mut self, other: &CatcodeTable, widen: bool) {
        let Some((lo, hi)) = other.reach else { return };
        let reach = match self.reach {
            None => (lo, hi),
            Some((a, b)) if widen => (if lo < a { 0 } else { a }, if hi > b { char::MAX as u32 } else { b }),
            Some((a, b)) => (a.min(lo), b.max(hi)),
        };
        let candidates: Vec<char> = self.known.union(&other.known).copied().collect();
        let known: std::collections::BTreeSet<char> = candidates
            .into_iter()
            .filter(|c| !self.is_unknown(*c) && !other.is_unknown(*c) && self.get(*c) == other.get(*c))
            .collect();
        self.reach = Some(reach);
        self.reach_cats |= other.reach_cats;
        self.known = Rc::new(known);
    }

    /// The join of the two tables over the category lattice, written here.
    pub fn join(&mut self, other: &CatcodeTable) {
        self.join_widen(other, false);
    }

    /// [`CatcodeTable::join`], widening the reach of unpinned writes.
    pub fn join_widen(&mut self, other: &CatcodeTable, widen: bool) {
        self.join_reach(other, widen);
        for c in self.disagreements(other) {
            self.set_may(c, self.cats(c) | other.cats(c));
        }
    }

    pub fn apply(&mut self, deltas: &[(char, u8)]) {
        for (c, code) in deltas {
            if let Some(cat) = Catcode::from_u8(*code) {
                self.set(*c, cat);
            }
        }
    }

    pub fn at_letter(&mut self, yes: bool) {
        self.set('@', if yes { Catcode::Letter } else { Catcode::Other });
    }
}

/// The value half of a [`Token`]: a control sequence, a character with its
/// [`Catcode`], or `#n` inside a macro body's parameter text.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum Tok {
    Cs(Sym),
    Chr(char, Catcode),
    Param(u8),
}

impl Tok {
    /// The token as one word, for hashing many fast.
    pub fn word(self) -> u64 {
        match self {
            Tok::Cs(sym) => u64::from(sym.0),
            Tok::Chr(c, cat) => 1 << 62 | u64::from(c) << 8 | cat as u64,
            Tok::Param(n) => 2 << 62 | u64::from(n),
        }
    }
}

/// One token as the [`Mouth`] produces it: a [`Tok`] paired with the [`Span`]
/// it was read from. [`Meaning`] and [`MacroDef`] work in terms of these once
/// the gullet and stomach take over.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

/// TeX tokens compare by value, not source (`\ifx` requirement).
impl PartialEq for Token {
    fn eq(&self, other: &Self) -> bool {
        self.tok == other.tok
    }
}

impl Eq for Token {}

impl Token {
    pub fn new(tok: Tok, span: Span) -> Self {
        Self { tok, span }
    }
    pub fn cs(&self) -> Option<Sym> {
        match self.tok {
            Tok::Cs(s) => Some(s),
            _ => None,
        }
    }
    pub fn cat(&self) -> Option<Catcode> {
        match self.tok {
            Tok::Chr(_, c) => Some(c),
            _ => None,
        }
    }
    pub fn is_char(&self, ch: char) -> bool {
        matches!(self.tok, Tok::Chr(c, _) if c == ch)
    }
    pub fn is_cat(&self, cat: Catcode) -> bool {
        matches!(self.tok, Tok::Chr(_, c) if c == cat)
    }
    pub fn is_space(&self) -> bool {
        self.is_cat(Catcode::Space)
    }
}

/// A token list as satex shows it: on the terminal (tex.web § 59).
pub fn detokenize(toks: &[Token], it: &Interner) -> String {
    detokenize_by(toks, it, Some('\\'), &|c: char| c.is_alphabetic(), Selector::File { newline: None })
}

/// A name satex made for its own bookkeeping rather than one a document
/// wrote: a register, a code table entry or a marker for something unknown.
/// They start with a character no control sequence name can.
fn is_internal(name: &str) -> bool {
    matches!(name.chars().next(), Some('\u{1}'..='\u{8}'))
}

/// tex.web § 49: whether the engine can show a character as itself, which
/// is `' '` through `'~'`. A Unicode engine shows anything past 255 too.
pub fn is_printable(c: char) -> bool {
    let k = c as u32;
    (0x20..=0x7e).contains(&k) || k > 0xff
}

/// tex.web § 49: a character the engine cannot show prints as `^^X`, with
/// `X` the character 64 above it below `@` and 64 below it above, and as
/// `^^` with two lowercase hex digits from 128 up.
pub fn printable(c: char) -> String {
    let k = c as u32;
    if is_printable(c) {
        c.to_string()
    } else if k < 0x40 {
        format!("^^{}", char::from(k as u8 + 0x40))
    } else if k < 0x80 {
        format!("^^{}", char::from(k as u8 - 0x40))
    } else {
        format!("^^{k:02x}")
    }
}

/// A token list as LuaTeX hands it to Lua (`\directlua`, `\luaescapestring`):
/// every character as itself, never as `^^X` (probed: `a^^Jb` arrives as
/// the bytes 97, 10, 98).
pub fn lua_string(toks: &[Token], it: &Interner) -> String {
    detokenize_by(toks, it, Some('\\'), &|c: char| c.is_alphabetic(), Selector::NewString)
}

/// Where tex.web § 59 `print` sends text.
#[derive(Clone, Copy)]
pub enum Selector {
    /// A string (`\detokenize`, `\meaning`, `\scantokens`): every
    /// character as itself.
    NewString,
    /// A file or the terminal: `^^X` for what the engine cannot show, and a
    /// line break at `\newlinechar`, tested before that conversion.
    File { newline: Option<char> },
}

/// A token list as the engine prints it (tex.web § 292 `show_token_list`),
/// with `\escapechar` `escape` (§ 63; `None` prints none). A one-character control sequence gets a space after it when that
/// character's category is letter now (§ 262), so `\_ ` under expl3's catcodes.
pub fn detokenize_in(toks: &[Token], it: &Interner, escape: Option<char>, cats: &CatcodeTable, to: Selector) -> String {
    detokenize_by(toks, it, escape, &|c| cats.get(c) == Catcode::Letter, to)
}

fn detokenize_by(
    toks: &[Token],
    it: &Interner,
    escape: Option<char>,
    letter: &dyn Fn(char) -> bool,
    to: Selector,
) -> String {
    let mut out = String::new();
    // tex.web § 59: a string takes each character as it is; a file breaks
    // the line at `\newlinechar`, else shows what it cannot as `^^X` (§ 49).
    let put = |out: &mut String, c: char| match to {
        Selector::File { newline } if newline == Some(c) => out.push('\n'),
        Selector::File { .. } if !is_printable(c) => out.push_str(&printable(c)),
        _ => out.push(c),
    };
    for t in toks {
        match t.tok {
            Tok::Cs(sym) => {
                let name = it.name(sym);
                // tex.web § 63 `print_esc`.
                escape.into_iter().for_each(|c| put(&mut out, c));
                // tex.web § 262: the control sequence with the empty name
                // prints as `\csname\endcsname`.
                if name.is_empty() {
                    out.push_str("csname");
                    escape.into_iter().for_each(|c| put(&mut out, c));
                    out.push_str("endcsname ");
                    continue;
                }
                // A satex-own name is left as it stands, so its markers stay
                // recognizable.
                if is_internal(name) {
                    out.push_str(name);
                } else {
                    name.chars().for_each(|c| put(&mut out, c));
                }
                // tex.web § 262 (`print_cs`): a multi-letter name always ends
                // with a space, and a one-character name does when that
                // character's category is letter — `\a ` but `\!`.
                let mut chars = name.chars();
                let single = chars.next().filter(|_| chars.next().is_none());
                if single.is_none_or(letter) {
                    out.push(' ');
                }
            }
            // tex.web § 294: a parameter character prints doubled.
            Tok::Chr(c, Catcode::Param) => {
                put(&mut out, c);
                put(&mut out, c);
            }
            Tok::Chr(c, _) => put(&mut out, c),
            Tok::Param(n) => {
                out.push('#');
                out.push((b'0' + n) as char);
            }
        }
    }
    out
}

/// Like `text_of`, but keep braces for structure (args, key-value lists).
pub fn text_with_groups(toks: &[Token], it: &Interner) -> String {
    let mut out = String::new();
    for t in toks {
        match t.tok {
            Tok::Cs(sym) => {
                out.push('\\');
                out.push_str(it.name(sym));
            }
            Tok::Chr(c, Catcode::Param) => {
                out.push(c);
                out.push(c);
            }
            Tok::Chr(c, _) => out.push(c),
            Tok::Param(n) => {
                out.push('#');
                out.push((b'0' + n) as char);
            }
        }
    }
    out
}

/// Text value of a token list (braces dropped, control sequences bare).
pub fn text_of(toks: &[Token], it: &Interner) -> String {
    let mut out = String::new();
    for t in toks {
        match t.tok {
            Tok::Cs(sym) => out.push_str(it.name(sym)),
            Tok::Chr(_, Catcode::Begin | Catcode::End) => {}
            Tok::Chr(c, _) => out.push(c),
            Tok::Param(_) => {}
        }
    }
    out.trim().to_string()
}

/// Turn `#1` into placeholder and `##` into literal `#` (The TeXbook, ch. 20).
/// `mark` says which control sequences stand for `#` here: none but the
/// character in TeX, a token meaning `\alignmark` as well in LuaTeX.
pub fn parameterize(tokens: Vec<Token>, mark: impl Fn(Sym) -> bool) -> Vec<Token> {
    let param = |t: &Token| t.is_cat(Catcode::Param) || t.cs().is_some_and(&mark);
    if !tokens.iter().any(param) {
        return tokens;
    }
    let mut out = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let token = tokens[i];
        if param(&token) {
            match tokens.get(i + 1) {
                Some(next) if param(next) => {
                    out.push(token);
                    i += 2;
                    continue;
                }
                Some(Token { tok: Tok::Chr(d, _), .. }) if d.is_ascii_digit() && *d != '0' => {
                    out.push(Token::new(Tok::Param(*d as u8 - b'0'), token.span));
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        out.push(token);
        i += 1;
    }
    out
}

/// The content of the next `{…}` in `s`, braces balanced, and what follows.
pub fn brace_group(s: &str) -> Option<(&str, &str)> {
    let start = s.find('{')?;
    let mut depth = 0usize;
    for (at, c) in s[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    let end = start + at;
                    return Some((&s[start + 1..end], &s[end + 1..]));
                }
            }
            _ => {}
        }
    }
    None
}

/// Where a line's comment starts: its first `%` not escaped by a `\`
/// (The TeXbook, ch. 2). A document that gives `%` another category code
/// is beyond what this says.
pub fn comment_start(line: &str) -> Option<usize> {
    let mut chars = line.char_indices();
    while let Some((at, c)) = chars.next() {
        match c {
            '\\' => {
                chars.next();
            }
            '%' => return Some(at),
            _ => {}
        }
    }
    None
}

pub fn comma_split(s: &str) -> Vec<String> {
    s.split(',').map(|p| p.trim().to_string()).filter(|p| !p.is_empty()).collect()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum LineState {
    New,
    Skipping,
    Middle,
}

/// The mouth of tex.web's three-stage model: turns source characters into
/// [`Token`]s under a [`CatcodeTable`] the caller supplies, since catcodes are
/// group-local and the mouth itself is not.
#[derive(Clone)]
pub struct Mouth {
    /// Shared, so that a path through an undecided conditional copies the
    /// position and not the file.
    src: std::rc::Rc<[char]>,
    pos: usize,
    line: u32,
    col: u32,
    state: LineState,
    /// Line `\endinput` appeared on, if any (tex.web § 362).
    force_eof: Option<u32>,
    /// The `\endlinechar` in force when the current line was read: TeX
    /// appends it then, so a change later on the same line only affects the
    /// lines after it (tex.web § 362).
    line_end: EndLineChar,
    /// The character the last unknown-text token stood for. Only catcode
    /// 13 runs a meaning (tex.web §§ 232, 344), so the character is what
    /// bounds what that token may do.
    pub last_unknown: Option<char>,
    /// The character of several possible categories the last token met:
    /// the reader may split the path on it and read again.
    pub undecided: Option<char>,
    /// The class of the typed unknown text a run wrote that the last token
    /// stands for ([`written_unknown`]).
    pub written: Option<(TextClass, bool)>,
    pub file: FileId,
}

/// A position a [`Mouth`] can be resumed at: the start of a line, or the
/// end of the file.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LineMark {
    pub pos: usize,
    pub line: u32,
    pub col: u32,
    pub force_eof: Option<u32>,
    pub ended: bool,
}

/// The character appended to each input line, `\endlinechar` (tex.web § 240).
/// [`Mouth::next_with`] appends it before tokenizing, which is how a blank
/// line becomes `\par`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct EndLineChar(pub i32);

impl EndLineChar {
    /// No value satex could follow: what a line ends with, or whether
    /// anything is appended, is not known. tex.web has no such state. It
    /// is the top of the lattice for this parameter.
    pub const UNKNOWN: Self = Self(i32::MIN);

    pub fn is_unknown(self) -> bool {
        self.0 == Self::UNKNOWN.0
    }

    /// The character appended, if the value names one (tex.web § 362:
    /// outside 0..=255 nothing is appended). Ask
    /// [`EndLineChar::is_unknown`] first, where `None` means not known.
    pub fn char(self) -> Option<char> {
        u8::try_from(self.0).ok().map(char::from)
    }

    /// The join of two values: a line ends the same way on both sides or
    /// it is not known how it ends.
    pub fn join(self, other: Self) -> Self {
        if self == other { self } else { Self::UNKNOWN }
    }
}

impl Default for EndLineChar {
    fn default() -> Self {
        EndLineChar(13)
    }
}

impl Mouth {
    pub fn new(src: &str, file: FileId) -> Self {
        Self {
            src: src.chars().collect::<Vec<_>>().into(),
            pos: 0,
            line: 1,
            col: 1,
            state: LineState::New,
            force_eof: None,
            line_end: EndLineChar::default(),
            last_unknown: None,
            undecided: None,
            written: None,
            file,
        }
    }

    /// Where the mouth stands, when that is the start of a line or the end
    /// of the file: a mouth [`Mouth::resumed`] there reads on the same.
    /// The source text from `line`:`col` up to where the mouth stands.
    pub fn text_since(&self, line: u32, col: u32) -> String {
        let (mut l, mut c) = (1u32, 1u32);
        let mut start = None;
        for (i, ch) in self.src[..self.pos.min(self.src.len())].iter().enumerate() {
            if (l, c) >= (line, col) {
                start = Some(i);
                break;
            }
            if *ch == '\n' {
                l += 1;
                c = 1;
            } else {
                c += 1;
            }
        }
        start.map_or_else(String::new, |i| self.src[i..self.pos.min(self.src.len())].iter().collect())
    }

    /// Where the mouth stands: its line, and the column it has read to.
    pub fn position(&self) -> (u32, u32) {
        (self.line, self.col)
    }

    pub fn line_mark(&self) -> Option<LineMark> {
        let ended = self.pos >= self.src.len() || self.force_eof.is_some_and(|line| self.line > line);
        (self.col == 1 || ended).then_some(LineMark {
            pos: self.pos,
            line: self.line,
            col: self.col,
            force_eof: self.force_eof,
            ended,
        })
    }

    /// A mouth over the file `src`: its last line ends even without a
    /// final newline, and gets the `\endlinechar` (tex.web § 31 `input_ln`).
    pub fn file(src: &str, file: FileId) -> Self {
        match src.ends_with(['\n', '\r']) || src.is_empty() {
            true => Mouth::new(src, file),
            false => Mouth::new(&format!("{src}\n"), file),
        }
    }

    /// A mouth over the file `src` standing at `mark`.
    pub fn resumed(src: &str, file: FileId, mark: LineMark) -> Self {
        let mut mouth = Mouth::file(src, file);
        mouth.seek(mark);
        mouth
    }

    /// This mouth standing at `mark` of its file.
    pub fn seek(&mut self, mark: LineMark) {
        self.pos = mark.pos.min(self.src.len());
        self.line = mark.line;
        self.col = mark.col;
        self.force_eof = mark.force_eof;
    }

    /// This mouth reading what `fresh` reads, which begins with what this
    /// one has read so far; else the line where it differs first.
    pub fn retext(&self, fresh: &Mouth) -> Result<Self, u32> {
        match self.src[..self.pos].iter().zip(fresh.src.iter()).position(|(a, b)| a != b) {
            None if fresh.src.len() >= self.pos => Ok(Mouth { src: fresh.src.clone(), ..self.clone() }),
            at => {
                let at = at.unwrap_or(fresh.src.len());
                Err(1 + self.src[..at].iter().filter(|&&c| c == '\n').count() as u32)
            }
        }
    }

    /// `\endinput` sets `force_eof` (tex.web §§ 362, 360).
    pub fn end_input(&mut self) {
        self.force_eof.get_or_insert(self.line);
    }

    /// The characters still to read.
    pub fn left(&self) -> usize {
        self.src.len().saturating_sub(self.pos)
    }

    /// Whether `\endinput` has ended the file at the end of its line.
    pub fn ending(&self) -> bool {
        self.force_eof.is_some()
    }

    /// Whether the flag `\endinput` set is still waiting for a new line.
    /// tex.web's `force_eof` is one global, so a file opened before the flag
    /// fires takes it over (tex.web § 362).
    pub fn pending_eof(&self) -> bool {
        self.force_eof.is_some_and(|line| line >= self.line)
    }

    /// Hand the pending flag to this mouth. A file `start_input` opened
    /// reads its first line without asking, so it ends after that line. A
    /// pseudo file asks before its first line, so it yields nothing
    /// (tex.web § 362, etex.ch § 362).
    pub fn inherit_eof(&mut self, asks_first_line: bool) {
        self.force_eof = Some(match asks_first_line {
            true => self.line.saturating_sub(1),
            false => self.line,
        });
    }

    /// Give up the pending flag, which another file has taken over.
    pub fn release_eof(&mut self) {
        self.force_eof = None;
    }

    /// How many characters of the file have been read.
    pub fn offset(&self) -> usize {
        self.pos
    }

    /// The text this mouth reads: held, its address tells two texts apart.
    pub fn text(&self) -> &std::rc::Rc<[char]> {
        &self.src
    }

    /// The text from `line`, `col` up to what was read last.
    pub fn since(&self, line: u32, col: u32) -> String {
        let end = self.pos.min(self.src.len());
        let mut start = end;
        let mut at = self.line;
        while start > 0 && at >= line {
            if self.src[start - 1] == '\n' {
                if at == line {
                    break;
                }
                at -= 1;
            }
            start -= 1;
        }
        let from = (start + col.saturating_sub(1) as usize).min(end);
        self.src[from..end].iter().collect()
    }

    pub fn here(&self) -> Span {
        Span::new(self.file, self.line, self.col)
    }

    /// Characters read so far and in all.
    pub fn read(&self) -> (usize, usize) {
        (self.pos, self.src.len())
    }

    fn bump(&mut self) -> Option<char> {
        let c = *self.src.get(self.pos)?;
        self.pos += 1;
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn peek_at(&self, k: usize) -> Option<char> {
        self.src.get(self.pos + k).copied()
    }

    /// `^^A` / `^^41` notation (The TeXbook § 355); returns (char, width).
    fn sup_escape_at(&self, at: usize, cats: &CatcodeTable) -> Option<(char, usize)> {
        let first = self.src.get(at).copied()?;
        if cats.get(first) != Catcode::Sup || self.src.get(at + 1).copied()? != first {
            return None;
        }
        let a = self.src.get(at + 2).copied()?;
        let hex = |x: char| x.is_ascii_digit() || ('a'..='f').contains(&x);
        if hex(a)
            && let Some(b) = self.src.get(at + 3).copied().filter(|b| hex(*b))
        {
            let value = (a.to_digit(16)? * 16 + b.to_digit(16)?) as u8;
            return Some((value as char, 4));
        }
        const FLIP: u32 = 0x40;
        let n = a as u32;
        (n < 0x80).then(|| char::from_u32(if n < FLIP { n + FLIP } else { n - FLIP })).flatten().map(|c| (c, 3))
    }

    /// Whether only spaces, which `input_ln` drops (tex.web § 31), stand
    /// between `at` and the newline.
    fn line_ends_at(&self, at: usize) -> bool {
        self.src[at.min(self.src.len())..].iter().find(|c| **c != ' ').is_some_and(|c| matches!(c, '\n' | '\r'))
    }

    /// Steps over the trailing spaces `input_ln` drops.
    fn skip_trailing_spaces(&mut self) {
        if self.peek_at(0) == Some(' ') && self.line_ends_at(self.pos) {
            while self.peek_at(0) == Some(' ') {
                self.bump();
            }
        }
    }

    /// Drop the rest of the line the mouth stands in: where that began,
    /// or `None` at the start of a line.
    pub fn skip_line(&mut self) -> Option<(u32, u32)> {
        if self.col == 1 {
            return None;
        }
        let from = (self.line, self.col);
        while let Some(n) = self.peek_at(0) {
            if n == '\n' || n == '\r' {
                break;
            }
            self.bump();
        }
        self.end_line();
        self.state = LineState::New;
        Some(from)
    }

    fn end_line(&mut self) {
        if self.peek_at(0) == Some('\r') && self.peek_at(1) == Some('\n') {
            self.bump();
        }
        self.bump();
    }

    fn take(&mut self, count: usize) {
        for _ in 0..count {
            self.bump();
        }
    }

    /// Read control sequence name after escape (The TeXbook § 355).
    fn read_cs_name(&mut self, cats: &CatcodeTable, it: &mut Interner) -> Sym {
        self.read_cs_name_inner(cats, it).unwrap_or_else(|| it.intern(""))
    }

    fn read_cs_name_inner(&mut self, cats: &CatcodeTable, it: &mut Interner) -> Option<Sym> {
        let first = match self.sup_escape_at(self.pos, cats) {
            Some((decoded, width)) => {
                self.take(width);
                decoded
            }
            // An escape character at the end of a line names the appended
            // `\endlinechar`, or, when none is appended, nothing: the result
            // is `\csname\endcsname` (tex.web § 354).
            None if self.line_ends_at(self.pos) => {
                self.skip_trailing_spaces();
                self.end_line();
                // The name is the character the line ends with, which is
                // not known here (tex.web § 240).
                if self.line_end.is_unknown() {
                    self.state = LineState::Middle;
                    return Some(it.intern(UNKNOWN_CS));
                }
                let appended = self.line_end.char()?;
                self.state = match cats.get(appended) {
                    Catcode::Letter | Catcode::Space => LineState::Skipping,
                    _ => LineState::Middle,
                };
                return Some(it.intern(&appended.to_string()));
            }
            None => self.bump()?,
        };
        // Whether the name is this one character or goes on is the category
        // of `first`, which is not known: the name is not either.
        if cats.is_unknown(first) {
            self.state = LineState::Middle;
            self.undecided = Some(first);
            return Some(it.intern(UNKNOWN_CS));
        }
        if cats.get(first) != Catcode::Letter {
            self.state = if cats.get(first) == Catcode::Space { LineState::Skipping } else { LineState::Middle };
            let mut buf = [0u8; 4];
            return Some(it.intern(first.encode_utf8(&mut buf)));
        }
        let mut name = String::from(first);
        while let Some(c) = self.peek_at(0) {
            // A character that may or may not carry the name on.
            if cats.is_unknown(c) {
                self.state = LineState::Middle;
                self.undecided = Some(c);
                return Some(it.intern(UNKNOWN_CS));
            }
            if cats.get(c) == Catcode::Letter {
                self.bump();
                name.push(c);
                continue;
            }
            match self.sup_escape_at(self.pos, cats) {
                Some((decoded, width)) if cats.get(decoded) == Catcode::Letter => {
                    self.take(width);
                    name.push(decoded);
                }
                _ => {
                    // Whether the appended character carries the name on
                    // is not known, so the name is not either.
                    if self.line_ends_at(self.pos) && self.line_end.is_unknown() {
                        self.state = LineState::Middle;
                        return Some(it.intern(UNKNOWN_CS));
                    }
                    // The appended `\endlinechar` is the buffer's last
                    // character: a letter ends the name (tex.web § 356).
                    if self.line_ends_at(self.pos)
                        && let Some(appended) = self.line_end.char().filter(|a| cats.get(*a) == Catcode::Letter)
                    {
                        self.skip_trailing_spaces();
                        self.end_line();
                        name.push(appended);
                    }
                    break;
                }
            }
        }
        self.state = LineState::Skipping;
        Some(it.intern(&name))
    }

    /// Next token under the current catcode table, or `None` at EOF.
    pub fn next(&mut self, cats: &CatcodeTable, it: &mut Interner) -> Option<Token> {
        self.next_with(cats, it, EndLineChar::default())
    }

    /// `end_line` is appended to each line; if not `Eol`, produces that category instead.
    pub fn next_with(&mut self, cats: &CatcodeTable, it: &mut Interner, end_line: EndLineChar) -> Option<Token> {
        loop {
            if self.force_eof.is_some_and(|line| self.line > line) {
                return None;
            }
            // Every line starts in state N (tex.web § 360).
            if self.col == 1 {
                self.line_end = end_line;
                self.state = LineState::New;
            }
            self.skip_trailing_spaces();
            let span = self.here();
            let (c, written) = match self.sup_escape_at(self.pos, cats) {
                Some((decoded, width)) => {
                    self.take(width);
                    (decoded, false)
                }
                None => (self.bump()?, true),
            };
            // A line of the file ends where the file says, whatever the
            // catcode of character 10 or 13; what TeX sees there is the
            // appended `\endlinechar` (tex.web § 362). `^^J` written out is
            // an ordinary character, of category 12 in LaTeX.
            let cat = match written && (c == '\n' || c == '\r') {
                true => Catcode::Eol,
                false => cats.get(c),
            };
            // A category satex could not follow: what this character does to
            // the input is not known, so it stands for text that is not.
            if cat != Catcode::Eol && cats.is_unknown(c) {
                self.state = LineState::Middle;
                self.last_unknown = Some(c);
                self.undecided = Some(c);
                return Some(Token::new(Tok::Cs(it.intern(UNKNOWN_CS)), span));
            }
            // Text a run wrote to a file it did not know: the same unknown
            // text read back.
            if written && (c == UNKNOWN_ANY || written_class(c).is_some()) {
                self.state = LineState::Middle;
                self.written = written_class(c);
                return Some(Token::new(Tok::Cs(it.intern(UNKNOWN_CS)), span));
            }
            match cat {
                Catcode::Ignored | Catcode::Invalid => continue,
                Catcode::Comment => {
                    while let Some(n) = self.peek_at(0) {
                        if n == '\n' || n == '\r' {
                            break;
                        }
                        self.bump();
                    }
                    self.end_line();
                    self.state = LineState::New;
                    continue;
                }
                Catcode::Eol => {
                    if c == '\r' && self.peek_at(0) == Some('\n') {
                        self.bump();
                    }
                    // What the line ends with is not known, so what it puts
                    // in the input is text that is not (tex.web § 240).
                    if self.line_end.is_unknown() {
                        self.state = LineState::New;
                        return Some(Token::new(Tok::Cs(it.intern(UNKNOWN_CS)), span));
                    }
                    let appended = match self.line_end.char() {
                        None => {
                            self.state = LineState::New;
                            continue;
                        }
                        Some(appended) => appended,
                    };
                    if cats.get(appended) != Catcode::Eol {
                        let state = std::mem::replace(&mut self.state, LineState::New);
                        match cats.get(appended) {
                            Catcode::Ignored | Catcode::Comment | Catcode::Invalid => continue,
                            // tex.web § 348: a character of category 10 is a
                            // space token, character code 32, and only in
                            // state M; in states N and S it is skipped.
                            Catcode::Space if state == LineState::Middle => {
                                return Some(Token::new(Tok::Chr(' ', Catcode::Space), span));
                            }
                            Catcode::Space => continue,
                            cat => return Some(Token::new(Tok::Chr(appended, cat), span)),
                        }
                    }
                    match self.state {
                        LineState::New => {
                            self.state = LineState::New;
                            return Some(Token::new(Tok::Cs(it.intern("par")), span));
                        }
                        LineState::Middle => {
                            self.state = LineState::New;
                            return Some(Token::new(Tok::Chr(' ', Catcode::Space), span));
                        }
                        LineState::Skipping => {
                            self.state = LineState::New;
                            continue;
                        }
                    }
                }
                Catcode::Space => match self.state {
                    LineState::Middle => {
                        self.state = LineState::Skipping;
                        return Some(Token::new(Tok::Chr(' ', Catcode::Space), span));
                    }
                    _ => continue,
                },
                Catcode::Escape => {
                    let sym = self.read_cs_name(cats, it);
                    return Some(Token::new(Tok::Cs(sym), span));
                }
                cat => {
                    self.state = LineState::Middle;
                    return Some(Token::new(Tok::Chr(c, cat), span));
                }
            }
        }
    }
}

/// One element of a [`ParameterText`]: a literal token to match, or a `#n`
/// parameter placeholder.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ParamItem {
    Lit(Tok),
    Param(u8),
}

/// The `#1,#2.` part of a `\def`'s parameter text, matched against the call
/// to bind arguments before [`MacroDef::replacement_text`] is substituted.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ParameterText {
    pub items: Vec<ParamItem>,
    pub arity: u8,
    /// `#1#{…}`: final argument delimited by `{`, left in input.
    pub brace_end: bool,
}

impl ParameterText {
    /// `mark` says which control sequences stand for `#` here, as in
    /// [`parameterize`].
    pub fn from_tokens(toks: &[Token], mark: impl Fn(Sym) -> bool) -> ParameterText {
        let mut items = Vec::new();
        let mut arity = 0u8;
        let mut brace_end = false;
        let mut i = 0;
        while i < toks.len() {
            let t = toks[i];
            if t.is_cat(Catcode::Param) || t.cs().is_some_and(&mark) {
                match toks.get(i + 1).map(|n| n.tok) {
                    // tex.web § 476 (`hash_brace`): a `#` at the end of the
                    // parameter text takes the body's `{` with it.
                    None => {
                        brace_end = true;
                        i += 1;
                        continue;
                    }
                    // tex.web § 479: the parameters are numbered as they
                    // come, whatever digit stands there. The digit is read
                    // only when it is the one expected.
                    Some(Tok::Chr(d, _)) if d.is_ascii_digit() && d as u8 - b'0' == arity + 1 && arity < 9 => {
                        arity += 1;
                        items.push(ParamItem::Param(arity));
                        i += 2;
                        continue;
                    }
                    // "Parameters must be numbered consecutively": a token
                    // that is not the next number goes back and is read on,
                    // while the parameter takes the number it should have
                    // had. Past nine, "You already have nine parameters"
                    // drops the `#` and the token read with it.
                    Some(_) => {
                        if arity < 9 {
                            arity += 1;
                            items.push(ParamItem::Param(arity));
                            i += 1;
                        } else {
                            i += 2;
                        }
                        continue;
                    }
                }
            }
            items.push(ParamItem::Lit(t.tok));
            i += 1;
        }
        ParameterText { items, arity, brace_end }
    }
    /// Render parameter text as it appears in source.
    pub fn render(&self, it: &Interner) -> String {
        let mut out = String::new();
        for item in &self.items {
            match item {
                ParamItem::Param(n) => {
                    out.push('#');
                    out.push((b'0' + n) as char);
                }
                ParamItem::Lit(tok) => out.push_str(&detokenize(&[Token::new(*tok, Span::default())], it)),
            }
        }
        if self.brace_end {
            out.push_str("#{");
        }
        out
    }

    pub fn is_simple(&self) -> bool {
        self.items.iter().all(|i| matches!(i, ParamItem::Param(_)))
    }
}

/// One argument type from an xparse argument specification (`m`, `o`, `s`,
/// …), the vocabulary `\NewDocumentCommand` reads its arguments with.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ArgType {
    Mandatory,
    Optional(Option<String>),
    Star,
    TokenFlag(char),
    Delimited {
        open: char,
        close: char,
        required: bool,
        default: Option<String>,
    },
    Embellishment(String),
    /// `u{…}`: everything up to the delimiter, a `#n` a `\def` delimits.
    Until(String),
    /// Text a `\def`'s parameter text requires before its first argument.
    Literal(String),
    /// A keyword TeX's scanners accept here, with the quantity it takes:
    /// `\hbox to ⟨dimen⟩`, `\hskip … plus ⟨dimen⟩`, the `=` of `\count0=`.
    Keyword {
        word: String,
        value: Option<String>,
    },
    /// A quantity TeX scans here: `number`, `dimen`, `glue`, …
    Quantity(String),
}

/// An xparse argument signature: the list of [`ArgType`]s a
/// `\NewDocumentCommand`-family macro reads its arguments with.
/// [`MacroDef::arg_spec`] carries one when the macro came from that layer.
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct ArgSpec {
    pub items: Vec<ArgType>,
    pub raw: String,
}

impl ArgSpec {
    /// A `\def` whose parameter text has delimiters reads exactly that
    /// (tex.web §§ 391-399): each `#n` is undelimited or runs to the text
    /// after it. `None` for a parameter text of bare `#n`s.
    pub fn from_parameter_text(text: &ParameterText, interner: &Interner) -> Option<ArgSpec> {
        if !text.brace_end && !text.items.iter().any(|i| matches!(i, ParamItem::Lit(_))) {
            return None;
        }
        let lit = |tok: Tok| detokenize(&[Token::new(tok, Span::default())], interner);
        let mut items = Vec::new();
        let mut pending: Option<String> = None;
        let mut prefix = String::new();
        for item in &text.items {
            match item {
                ParamItem::Lit(tok) => match &mut pending {
                    Some(delimiter) => delimiter.push_str(&lit(*tok)),
                    None => prefix.push_str(&lit(*tok)),
                },
                ParamItem::Param(_) => {
                    if !prefix.is_empty() {
                        items.push(ArgType::Literal(std::mem::take(&mut prefix)));
                    }
                    if let Some(delimiter) = pending.replace(String::new()) {
                        items.push(if delimiter.is_empty() { ArgType::Mandatory } else { ArgType::Until(delimiter) });
                    }
                }
            }
        }
        if let Some(mut delimiter) = pending {
            // `#1#{`: the argument runs to a `{`, which stays.
            if delimiter.is_empty() && text.brace_end {
                delimiter.push('{');
            }
            items.push(if delimiter.is_empty() { ArgType::Mandatory } else { ArgType::Until(delimiter) });
        }
        if !prefix.is_empty() {
            items.push(ArgType::Literal(prefix));
        }
        Some(ArgSpec { items, raw: text.render(interner) })
    }

    pub fn arity(&self) -> u8 {
        self.items.len() as u8
    }
}

/// expl3 name signature gives arity per letter; `p`, `w`, `D` are unpredictable (interface3).
pub fn expl3_arity(name: &str) -> Option<u8> {
    const ARGUMENT_TYPES: &str = "NncVvoxefTF";
    const UNPREDICTABLE: &str = "pwD";
    let (stem, signature) = name.split_once(':')?;
    if stem.is_empty() || !stem.contains('_') {
        return None;
    }
    if !signature.chars().all(|c| ARGUMENT_TYPES.contains(c)) {
        let _ = UNPREDICTABLE;
        return None;
    }
    Some(signature.len() as u8)
}

named_enum! {
    /// Which TeX register kind a `\countdef`-style name addresses: count, dimen,
    /// skip, muskip, toks, box, or a read/write stream. [`Meaning::Register`]
    /// pairs it with the register number.
    #[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
    pub enum RegKind {
        Count => "count",
        Dimen => "dimen",
        Skip => "skip",
        MuSkip => "muskip",
        Toks => "toks",
        Box => "box",
        Read => "read",
        Write => "write",
        /// `\chardef` and `\mathchardef`: a constant, not a register, whose
        /// number is its value (tex.web § 1224). The two are different
        /// meanings, so `\ifx` tells them apart.
        Char => "char",
        MathChar => "mathchar",
        /// `\Umathchardef` and `\Umathcharnumdef`: a `\Umathchar` constant,
        /// which `\ifx` and `\meaning` tell from a `\mathchar` one (XeTeX
        /// reference; LuaTeX manual, "Math").
        UMathChar => "Umathchar",
    }
}

/// The number of a register allocated by a modeled `\newcount` and its
/// relatives, which satex does not number: its value lives on its own name
/// rather than on the numbered register.
pub const UNNUMBERED: u16 = u16::MAX;

/// The compiled meaning of a macro: [`ParameterText`] to match, replacement
/// [`Token`]s to substitute into, and the `\long`/`\outer`/`\protected`
/// prefixes tex.web tracks per definition. [`Meaning::Macro`] wraps one.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MacroDef {
    pub parameter_text: ParameterText,
    /// Present when the macro came from the LaTeX/xparse layer.
    pub arg_spec: Option<ArgSpec>,
    pub replacement_text: Rc<[Token]>,
    pub long: bool,
    pub outer: bool,
    pub protected: bool,
}

impl MacroDef {
    pub fn arity(&self) -> u8 {
        self.arg_spec.as_ref().map_or(self.parameter_text.arity, ArgSpec::arity)
    }

    /// How many undelimited parameters the macro takes, when that is all its
    /// parameter text is: `\def\x#1#2` and `\newcommand\x[2]` read the
    /// same arguments.
    fn undelimited(&self) -> Option<u8> {
        match &self.arg_spec {
            Some(spec) => spec.items.iter().all(|item| *item == ArgType::Mandatory).then(|| spec.arity()),
            None => {
                let text = &self.parameter_text;
                (!text.brace_end && text.items.iter().all(|item| matches!(item, ParamItem::Param(_))))
                    .then_some(text.arity)
            }
        }
    }
}

/// tex.web § 507: `\ifx` compares two macros by their prefixes, parameter
/// text and replacement text. How satex came to record the parameters is
/// not part of the meaning.
impl PartialEq for MacroDef {
    fn eq(&self, other: &Self) -> bool {
        let same_parameters = match (self.undelimited(), other.undelimited()) {
            (Some(a), Some(b)) => a == b,
            _ => self.parameter_text == other.parameter_text && self.arg_spec == other.arg_spec,
        };
        same_parameters
            && self.long == other.long
            && self.outer == other.outer
            && self.protected == other.protected
            && self.replacement_text == other.replacement_text
    }
}

/// What a control sequence currently means: undefined, a [`Primitive`], a
/// [`MacroDef`], a `\let`-to-character, a register, or merely known to exist.
/// [`Env`](crate::env::Env) maps every [`Sym`] to one; `\let` copies it.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum Meaning {
    Undefined,
    Primitive(Primitive),
    Macro(Rc<MacroDef>),
    /// `\chardef`, `\mathchardef`, and `\let\x=a`.
    Char(char, Catcode),
    /// `\countdef\foo=12` and friends — a named register.
    Register(RegKind, u16),
    /// The abstract top element: we know the name is defined, not what it is.
    Unknown,
}

impl Meaning {
    pub fn kind(&self) -> &'static str {
        match self {
            Meaning::Undefined => "undefined",
            Meaning::Primitive(_) => "primitive",
            Meaning::Macro(_) => "macro",
            Meaning::Char(..) => "char",
            Meaning::Register(..) => "register",
            Meaning::Unknown => "unknown",
        }
    }
    pub fn as_macro(&self) -> Option<&Rc<MacroDef>> {
        match self {
            Meaning::Macro(m) => Some(m),
            _ => None,
        }
    }
    pub fn prim(&self) -> Option<Primitive> {
        match self {
            Meaning::Primitive(p) => Some(*p),
            _ => None,
        }
    }
}
