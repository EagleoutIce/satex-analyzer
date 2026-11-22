//! TeX's line breaker (tex.web §§ 813-890, Knuth-Plass) for a paragraph
//! whose items, metrics and parameters are all known. Anything else keeps
//! the line-count interval of [`crate::engine::mode`].

use crate::analysis::facts::LineEnd;
use crate::analysis::machine::Machine;
use crate::engine::builtins::CodeTable;
use crate::engine::mode::Natural;
use crate::engine::tfm::LigKern;
use crate::engine::value::{Glue, Scaled, Value};
use std::collections::HashMap;
use std::rc::Rc;

/// One item of a paragraph's horizontal list, as `line_break` reads it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum Kp {
    /// A box, rule or character (`chr`: font and code; `lig`: a
    /// ligature's letters, see [`lig_letter`]).
    Box {
        w: Scaled,
        h: Scaled,
        d: Scaled,
        chr: Option<(u32, u32)>,
        lig: u32,
    },
    /// Width, stretch and shrink, each with its order.
    Glue {
        w: Scaled,
        st: Scaled,
        sto: u8,
        sh: Scaled,
        sho: u8,
    },
    /// `explicit` from `\kern` or `\/`, else a font kern.
    Kern {
        w: Scaled,
        explicit: bool,
    },
    Penalty(i64),
    Whatsit,
    /// A discretionary with this pre-break box (none after a
    /// `\hyphenchar`, § 1039), this many items after it as its replacement
    /// (§ 145 `replace_count`) and this post-break box.
    Disc(Option<Dims>, u8, Option<Dims>),
    /// A mark of a class, its text by index in [`Arena::texts`] or a
    /// [`VList`]'s texts (§ 141).
    Mark(i64, u32),
}

impl Kp {
    pub(crate) fn glue(g: Glue) -> Kp {
        Kp::Glue { w: g.width, st: g.stretch.amount, sto: g.stretch.order, sh: g.shrink.amount, sho: g.shrink.order }
    }
    pub(crate) fn width(self) -> Scaled {
        match self {
            Kp::Box { w, .. } | Kp::Glue { w, .. } | Kp::Kern { w, .. } => w,
            _ => 0,
        }
    }
    /// § 148 `precedes_break` and characters: glue after it is a breakpoint.
    fn non_discardable(self) -> bool {
        matches!(self, Kp::Box { .. } | Kp::Whatsit | Kp::Disc(..) | Kp::Mark(..))
    }
}

/// The recorded lists, hash-consed: a list is the id of its last node.
/// Copies share the lists recorded before them (a checkpoint holds one).
#[derive(Clone, Default)]
pub(crate) struct Arena {
    nodes: crate::fixed::Chunked<(Kp, u32, u32)>,
    /// Per node: the characters the run had set when it was last pushed, so where a line breaks
    /// in the text. A note, not part of the node: lists stay shared.
    chars: crate::fixed::Chunked<u32>,
    index: crate::fixed::SharedMap<(Kp, u32), u32>,
    /// Per list (the empty one first): the list a push onto it gave last. A run pushes
    /// what it pushed before (a second pass, a repeated text), so the next push mostly
    /// finds its node here, checked against the node, without the walk through `index`.
    next: crate::fixed::Chunked<u32>,
    pub(crate) texts: crate::fixed::Chunked<std::rc::Rc<[crate::engine::tex::Token]>>,
}

impl Arena {
    /// `self` with every list `other` holds, each under `other`'s name: a run that goes on
    /// from a state of the run that made `other` names every list that run made as it did,
    /// and its own new ones past them. Lists are only ever asked for by name, so more of
    /// them change nothing, short of the bound on their number (`most`, `limits.list_items`):
    /// `None` where `other` holds over half of it, or is not `self` grown.
    pub(crate) fn grown(&self, other: &Arena, most: usize) -> Option<Arena> {
        let same = other.nodes.starts_with(&self.nodes) && other.texts.starts_with(&self.texts);
        (same && other.nodes.len() <= most / 2).then(|| other.clone())
    }

    /// Whether `self` holds every list of `other` under the same name.
    pub(crate) fn holds(&self, other: &Arena) -> bool {
        self.nodes.starts_with(&other.nodes) && self.texts.starts_with(&other.texts)
    }
}

/// A box's width, height and depth.
type Dims = (Scaled, Scaled, Scaled);

/// `Kp::Box`'s `lig` of a character that is no ligature.
pub(crate) const LIG_NONE: u32 = 0;
/// A ligature whose letters are not followed.
pub(crate) const LIG_OPEN: u32 = u32::MAX;

/// Ligature `had` with letter `c` joined on (§ 143 `lig_ptr`): up to three
/// codes below 256, their count in the top byte.
pub(crate) fn lig_letter(had: u32, c: u32) -> u32 {
    let n = had >> 24;
    if had == LIG_OPEN || n >= 3 || c > 255 {
        return LIG_OPEN;
    }
    (had & 0xff_ffff) | c << (8 * n) | (n + 1) << 24
}

/// The letters of character `c` with `lig`; `None` where not followed.
pub(crate) fn letters(c: u32, lig: u32) -> Option<Vec<u32>> {
    match lig {
        LIG_NONE => Some(vec![c]),
        LIG_OPEN => None,
        l => Some((0..l >> 24).map(|i| l >> (8 * i) & 0xff).collect()),
    }
}

const INF_BAD: i64 = 10000;
const EJECT: i64 = -10000;
const AWFUL: i64 = 0o7777777777;
pub(crate) const DECENT: u8 = 2;
const DEPLORABLE: i64 = 100000;
/// A line's fitness class is not known.
pub(crate) const UNFIT: u8 = 4;

impl Machine<'_> {
    /// The last item of the recorded list `items` and the list before it.
    pub(crate) fn kp_last(&self, items: Option<u32>) -> Option<(Kp, u32)> {
        let id = items.filter(|&i| i > 0)?;
        let (item, prev, _) = self.kp.nodes[id as usize - 1];
        Some((item, prev))
    }

    /// `item` goes onto the paragraph's recorded list `items`.
    pub(crate) fn kp_push(&mut self, items: &mut Option<u32>, item: Kp) {
        let Some(prev) = *items else { return };
        let chars = self.out.facts.text.at.len() as u32;
        let a = &mut self.kp;
        let len = if prev == 0 { 0 } else { a.nodes[prev as usize - 1].2 };
        let full = a.nodes.len() >= self.cfg.limits.list_items;
        if full || len as usize >= self.cfg.limits.paragraph_items {
            // This list (or every later one) is not recorded: said, with the setting to raise.
            let key = if full { "list_items" } else { "paragraph_items" };
            self.limit_reached(key, self.position_span());
            *items = None;
            return;
        }
        let next = a.nodes.len() as u32 + 1;
        if a.next.len() < next as usize {
            a.next.resize(next as usize, 0);
        }
        let hint = a.next[prev as usize];
        let id = match hint.checked_sub(1).map(|h| a.nodes[h as usize]) {
            Some((k, p, _)) if k == item && p == prev => hint,
            _ => {
                let id = match a.index.get(&(item, prev)) {
                    Some(&id) => id,
                    None => {
                        a.index.insert_mut((item, prev), next);
                        a.nodes.push((item, prev, len + 1));
                        a.chars.push(0);
                        a.next.push(0);
                        next
                    }
                };
                a.next[prev as usize] = id;
                id
            }
        };
        // Only a new count unshares the chunk.
        if a.chars[id as usize - 1] != chars {
            a.chars[id as usize - 1] = chars;
        }
        *items = Some(id);
    }

    /// tex.web §§ 656-657: the stretch and the shrink of the recorded list
    /// `items`, per order.
    pub(crate) fn kp_glue(items: &[Kp]) -> [[Scaled; 4]; 2] {
        let mut total = [[0; 4]; 2];
        for k in items {
            if let Kp::Glue { st, sto, sh, sho, .. } = *k {
                total[0][usize::from(sto.min(3))] += st;
                total[1][usize::from(sho.min(3))] += sh;
            }
        }
        total
    }

    /// `\unskip` and relatives take the recorded tail away where it is
    /// the item `last` the list's tail says.
    pub(crate) fn kp_pop(
        &mut self,
        items: &mut Option<u32>,
        removed: crate::engine::mode::Removed,
        last: crate::engine::mode::Last,
    ) {
        use crate::engine::mode::{Last, Removed};
        let Some((k, prev)) = self.kp_last(*items) else { return };
        let hit = matches!(
            (removed, last),
            (Removed::Glue, Last::Glue(..))
                | (Removed::Kern, Last::Kern(_))
                | (Removed::Penalty, Last::Penalty(_))
                | (_, Last::Unknown(_))
        );
        if hit {
            *items = match (last, k) {
                (Last::Glue(w, ..), Kp::Glue { w: v, .. }) if w.known() == Some(v) => Some(prev),
                (Last::Kern(w), Kp::Kern { w: v, .. }) | (Last::Penalty(w), Kp::Penalty(v)) if w == v => Some(prev),
                _ => None,
            };
        }
    }

    /// The characters set when each item of list `id` was pushed.
    fn kp_chars(&self, mut id: u32) -> Vec<u32> {
        let mut out = Vec::new();
        while id > 0 {
            out.push(self.kp.chars[id as usize - 1]);
            id = self.kp.nodes[id as usize - 1].1;
        }
        out.reverse();
        out
    }

    pub(crate) fn kp_list(&self, mut id: u32) -> Vec<Kp> {
        let mut out = Vec::new();
        while id > 0 {
            let (k, prev, _) = self.kp.nodes[id as usize - 1];
            out.push(k);
            id = prev;
        }
        out.reverse();
        out
    }

    /// The lines the segment of `n` ending here breaks into, and the last
    /// line's fitness class, where all is known.
    /// `last`: at `\par` (§ 816), else at a forced break.
    pub(crate) fn break_lines(&mut self, n: &Natural, last: bool) -> Option<(Vec<Broken>, u8)> {
        let p = n.para;
        if p.lost || p.rough && p.lines.1 > 0 {
            return None;
        }
        let width = self.line_width().known()?;
        let emergency = self.dimen_parameter("emergencystretch").known();
        let params = Params::read(width, emergency, self, |m, n| m.int_parameter(n), |m, n| m.glue_parameter(n))?;
        let breaks = &mut self.out.facts.text.breaks;
        let at = self.out.facts.text.at.len() as u32;
        if breaks.last() != Some(&(at, params)) {
            breaks.push((at, params));
        }
        let items = self.kp_list(p.items?);
        let first = p.lines == (0, 0);
        let start = if first { 0 } else { skip_discardable(&items, 0) };
        // The summary must agree with the recorded list.
        let (w, h, d) = items[start..].iter().fold((0, 0, 0), |(w, h, d), k| {
            let (kh, kd) = if let Kp::Box { h, d, .. } = k { (*h, *d) } else { (0, 0) };
            (w + k.width(), h.max(kh), d.max(kd))
        });
        if [n.along, n.height.at_least_zero(), n.depth.at_least_zero()].map(|x| x.known())
            != [Some(w), Some(h), Some(d)]
        {
            return None;
        }
        if p.fit == UNFIT && params.dem[1] != 0 {
            return None;
        }
        let fit0 = if p.fit == UNFIT { DECENT } else { p.fit };
        let chars = self.kp_chars(p.items?);
        let b = line_break(items.clone(), start, fit0, last, &params, self)?;
        let text = &self.out.facts.text;
        let base = if first { text.par_starts.last().map(|s| s.0) } else { text.lines.last().map(|l| l.0) };
        let ends = base.and_then(|base| line_ends(&items, &chars, &b, base, at));
        self.note_line_ends(ends);
        Some((b.lines, b.fit))
    }

    /// The lines a paragraph on no enclosing horizontal list broke into go into
    /// [`crate::analysis::facts::Text::lines`], `None` as one not known.
    pub(crate) fn note_line_ends(&mut self, ends: Option<Vec<(u32, LineEnd)>>) {
        if self.in_inner_list() {
            return;
        }
        let at = self.out.facts.text.at.len() as u32;
        let lines = &mut self.out.facts.text.lines;
        match ends {
            Some(ends) => lines.extend(ends),
            None if lines.last() != Some(&(at, LineEnd::Unknown)) => lines.push((at, LineEnd::Unknown)),
            None => {}
        }
    }
}

/// What `line_break` reads of the engine beyond the list: codes, fonts and the
/// hyphenation tables in force. The machine answers from its state, a plugin
/// from a finished run's.
pub(crate) trait Tables {
    /// `\lccode`, `None` unknown.
    fn lower(&mut self, c: u32) -> Option<i64>;
    /// `\hyphenchar` of a font, `None` unknown.
    fn hyphen_char(&mut self, font: u32) -> Option<i64>;
    /// A font's metrics and size.
    fn metrics(&mut self, font: u32) -> Option<(Rc<crate::engine::tfm::Metrics>, Scaled)>;
    /// The current language's patterns and exceptions.
    fn patterns(&mut self) -> Option<crate::engine::hyph::HyphTables>;
    /// `\lefthyphenmin` and `\righthyphenmin` (§ 1091).
    fn mins(&mut self) -> (usize, usize);
}

impl Tables for Machine<'_> {
    fn lower(&mut self, c: u32) -> Option<i64> {
        self.char_code_known(CodeTable::Lower, char::from_u32(c)?)
    }
    fn hyphen_char(&mut self, font: u32) -> Option<i64> {
        self.font_field(font, "hyphenchar").as_int()
    }
    fn metrics(&mut self, font: u32) -> Option<(Rc<crate::engine::tfm::Metrics>, Scaled)> {
        Some((self.font_metrics(font)?, self.font_size(font)?))
    }
    fn patterns(&mut self) -> Option<crate::engine::hyph::HyphTables> {
        self.hyph_tables()
    }
    fn mins(&mut self) -> (usize, usize) {
        self.hyphen_mins()
    }
}

/// The parameters `line_break` reads (tex.web §§ 236-247), all known, with
/// `\looseness` 0 and `\lastlinefit` 0 or less: the breaker follows no other.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Params {
    /// The width of every line (`\hsize`, or a uniform `\parshape`).
    pub width: Scaled,
    /// `\linepenalty`, `\adjdemerits`, `\doublehyphendemerits`,
    /// `\finalhyphendemerits`, `\hyphenpenalty`, `\exhyphenpenalty`.
    pub dem: [i64; 6],
    pub tolerance: i64,
    pub pretolerance: i64,
    pub uchyph: i64,
    pub emergency: Scaled,
    pub left: Glue,
    pub right: Glue,
    pub fill: Glue,
}

impl Params {
    /// The parameters `cx` gives through `int` and `glue`; `None` where one is not known.
    pub(crate) fn read<C>(
        width: Scaled,
        emergency: Option<Scaled>,
        cx: &mut C,
        int: impl Fn(&mut C, &str) -> Option<i64>,
        glue: impl Fn(&mut C, &str) -> Option<Glue>,
    ) -> Option<Params> {
        #[rustfmt::skip]
        let [loose, line, adj, double, fin, hyph, ex, tol, fit, pretol, uc] = [
            "looseness", "linepenalty", "adjdemerits", "doublehyphendemerits", "finalhyphendemerits",
            "hyphenpenalty", "exhyphenpenalty", "tolerance", "lastlinefit", "pretolerance", "uchyph",
        ]
        .map(|name| int(cx, name));
        if loose? != 0 || fit? > 0 {
            return None;
        }
        Some(Params {
            width,
            dem: [line?, adj?, double?, fin?, hyph?, ex?],
            tolerance: tol?,
            pretolerance: pretol?,
            uchyph: uc?,
            emergency: emergency?,
            left: glue(cx, "leftskip")?,
            right: glue(cx, "rightskip")?,
            fill: glue(cx, "parfillskip")?,
        })
    }
}

/// What [`line_break`] made: the lines, the last one's fitness class, the
/// list it broke (hyphenated in the second pass), and where it put each
/// discretionary hyphen: its index, the index of the word's first letter in
/// the list it was given and the letters of the word before it.
pub(crate) struct Breaking {
    pub lines: Vec<Broken>,
    pub fit: u8,
    pub items: Vec<Kp>,
    pub points: Vec<(usize, usize, usize)>,
}

/// tex.web §§ 813-890 `line_break` of `items` from `start` (past a forced
/// break, the line before of fitness `fit0`), at `\par` when `last`.
pub(crate) fn line_break(
    mut items: Vec<Kp>,
    start: usize,
    fit0: u8,
    last: bool,
    p: &Params,
    tables: &mut impl Tables,
) -> Option<Breaking> {
    if last {
        // § 816: a final glue becomes `\penalty10000`, then `\parfillskip`.
        if matches!(items.last(), Some(Kp::Glue { .. })) {
            items.pop();
        }
        items.push(Kp::Penalty(10000));
        items.push(Kp::glue(p.fill));
    }
    // § 825: infinite shrink is an error TeX makes finite.
    let infinite = |k: &Kp| matches!(*k, Kp::Glue { sho, sh, .. } if sho > 0 && sh != 0);
    if items.iter().chain(&[Kp::glue(p.left), Kp::glue(p.right)]).any(infinite) {
        return None;
    }
    let mut bg = [0i64; 6];
    for g in [p.left, p.right] {
        add(&mut bg, Kp::glue(g));
    }
    let pass = Pass { width: p.width, bg, dem: p.dem, start, fit0, end_hyphenated: last };
    let done = |items, points, (lines, fit)| Breaking { lines, fit, items, points };
    // § 863: a first pass without hyphenation, a second with, a third
    // with `\emergencystretch`.
    if p.pretolerance >= 0
        && let Some(r) = pass.run(&items, p.pretolerance, false)
    {
        return Some(done(items, Vec::new(), r));
    }
    let (hyphenated, points) = hyphenate(tables, &items, start, p.uchyph)?;
    if let Some(r) = pass.run(&hyphenated, p.tolerance, p.emergency <= 0) {
        return Some(done(hyphenated, points, r));
    }
    let mut third = pass;
    third.bg[1] += p.emergency;
    let r = (p.emergency > 0).then(|| third.run(&hyphenated, p.tolerance, true)).flatten()?;
    Some(done(hyphenated, points, r))
}

/// Where a discretionary went: its item, its word's first letter, the letters before it.
type Placed = (usize, usize, usize);
/// A discretionary's pre-break width, replaced items and post-break width.
type Passed = (Dims, u8, Option<Dims>);

/// §§ 894-918: the second pass's list, with a discretionary hyphen at
/// each hyphenation point of each word after glue. `None` where a word
/// is not followed exactly (ligatures not followed or with the hyphen,
/// boundary characters, unknown codes or patterns).
fn hyphenate(t: &mut impl Tables, items: &[Kp], start: usize, uc: i64) -> Option<(Vec<Kp>, Vec<Placed>)> {
    let (left, right) = t.mins();
    let tables = t.patterns();
    let mut out: Vec<Kp> = items[..start].to_vec();
    // Discretionaries after an item, each with its word's first letter and the letters before it.
    let mut points: HashMap<usize, (Dims, (usize, usize))> = HashMap::new();
    // Discretionaries before an item: pre-break, replaced items, post-break.
    let mut passed: HashMap<usize, (Passed, (usize, usize))> = HashMap::new();
    let mut placed = Vec::new();
    let letters_at = |k: Option<&Kp>| match k {
        Some(&Kp::Box { chr: Some((f, c)), lig, .. }) => Some((f, letters(c, lig))),
        _ => None,
    };
    for i in start..items.len() {
        if !matches!(items[i], Kp::Glue { .. }) {
            continue;
        }
        // § 896: the first letter after the glue.
        let mut s = i + 1;
        let (hf, c) = loop {
            match items.get(s) {
                k @ Some(Kp::Box { chr: Some(_), .. }) => {
                    let (f, l) = letters_at(k).expect("a character");
                    let c = l?[0];
                    if t.lower(c)? == 0 {
                        s += 1;
                        continue;
                    }
                    break (f, c);
                }
                Some(Kp::Kern { explicit: false, .. } | Kp::Whatsit) => s += 1,
                _ => break (0, u32::MAX),
            }
        };
        if c == u32::MAX || t.lower(c)? != i64::from(c) && uc <= 0 {
            continue;
        }
        let hyf_char = match t.hyphen_char(hf)? {
            h @ 0..=255 => h as u32,
            _ => continue,
        };
        // §§ 897-898: the letters, a ligature's all or none, font kerns between them.
        let mut word = Vec::new();
        let mut at = Vec::new();
        let mut e = s;
        'word: loop {
            match items.get(e) {
                k @ Some(Kp::Box { chr: Some((f, _)), .. }) if *f == hf => {
                    let l = letters_at(k).expect("a character").1?;
                    let mut lcs = Vec::new();
                    for &c in &l {
                        match t.lower(c)? {
                            0 => break 'word,
                            lc => lcs.push(lc as u32),
                        }
                    }
                    if word.len() + lcs.len() > 63 {
                        break;
                    }
                    at.extend(std::iter::repeat_n(e, lcs.len()));
                    word.extend(lcs);
                }
                Some(Kp::Kern { explicit: false, .. }) => {}
                _ => break,
            }
            e += 1;
        }
        if word.len() < left + right {
            continue;
        }
        // § 899: what follows must permit hyphenation.
        let ok = loop {
            match items.get(e) {
                Some(Kp::Box { chr: Some(_), .. } | Kp::Kern { explicit: false, .. }) => e += 1,
                None | Some(Kp::Whatsit | Kp::Glue { .. } | Kp::Penalty(_) | Kp::Kern { .. }) => break true,
                _ => break false,
            }
        };
        if !ok {
            continue;
        }
        let hyf = crate::engine::hyph::hyf(tables.as_ref()?, &word);
        let (m, z) = t.metrics(hf)?;
        if m.boundary_char().is_some() {
            return None;
        }
        let b = m.char_box(hyf_char, z)?;
        for j in left..=word.len() - right {
            if hyf[j].is_multiple_of(2) {
                continue;
            }
            let (k, next) = (at[j - 1], at[j]);
            let Some(&Kp::Box { w, h, d, chr: Some((_, prev)), .. }) = items.get(k) else {
                return None;
            };
            // § 916: a left boundary program puts its own material
            // at the start of the post-break text.
            let first = letters_at(items.get(next)).and_then(|(_, l)| l)?[0];
            if m.left_boundary(first, z).is_some() {
                return None;
            }
            let disc = if k == next {
                // §§ 913-916, inside ligature `k`: its letters up to the
                // point and the hyphen before the break, the rest after
                // it, with its kern to the letter that follows.
                let l = letters_at(items.get(k)).and_then(|(_, l)| l)?;
                let cut = j - at.iter().position(|&x| x == k)?;
                let mut pre_letters = l[..cut].to_vec();
                pre_letters.push(hyf_char);
                let pre = reconstitute(&m, z, &pre_letters)?;
                let (kern, follow) = match items.get(k + 1) {
                    Some(&Kp::Kern { explicit: false, .. }) => (1, items.get(k + 2)),
                    n => (0, n),
                };
                let (mut post, last) = reconstitute(&m, z, &l[cut..])?;
                match letters_at(follow) {
                    Some((f, l)) if f == hf => match m.lig_kern(last, l?[0], z) {
                        None => {}
                        Some(LigKern::Kern(x)) => post.0 += x,
                        Some(LigKern::Lig { .. }) => return None,
                    },
                    Some(_) => return None,
                    None => {}
                }
                (pre.0, 1 + kern, Some(post))
            } else {
                // § 909: a kern with the hyphen, or one with the next
                // letter, passes the hyphen at this letter: the
                // discretionary goes before it and replaces it and its
                // kern (§§ 914-918), its pre-break text is the letter, the
                // kern with the hyphen and the hyphen (§ 915).
                let with_hyphen = match m.lig_kern(prev, hyf_char, z) {
                    None => None,
                    Some(LigKern::Kern(x)) => Some(x),
                    Some(LigKern::Lig { .. }) => return None,
                };
                let between = match items.get(k + 1) {
                    _ if k + 1 == next => None,
                    Some(&Kp::Kern { w, explicit: false }) if k + 2 == next => Some(w),
                    _ => return None,
                };
                if with_hyphen.is_none() && between.is_none() {
                    points.insert(k, ((b.width, b.height, b.depth), (s, j)));
                    continue;
                }
                let pre = (w + with_hyphen.unwrap_or(0) + b.width, h.max(b.height), d.max(b.depth));
                (pre, 1 + u8::from(between.is_some()), None)
            };
            // Two discretionaries at one item are not followed.
            if passed.insert(k, (disc, (s, j))).is_some() {
                return None;
            }
        }
    }
    for (i, k) in items.iter().enumerate().skip(start) {
        if let Some(&((pre, r, post), (s, j))) = passed.get(&i) {
            placed.push((out.len(), s, j));
            out.push(Kp::Disc(Some(pre), r, post));
        }
        out.push(*k);
        if let Some(&(pre, (s, j))) = points.get(&i) {
            placed.push((out.len(), s, j));
            out.push(Kp::Disc(Some(pre), 0, None));
        }
    }
    Some((out, placed))
}

/// §§ 906-911 `reconstitute`: `letters` set with their ligatures and kerns
/// (only `=:` ligatures followed), and the last character.
fn reconstitute(m: &crate::engine::tfm::Metrics, z: Scaled, letters: &[u32]) -> Option<(Dims, u32)> {
    let (items, last) = set_letters(m, z, 0, letters)?;
    let dims = items.iter().fold((0, 0, 0), |(w, h, d), k| match *k {
        Kp::Box { w: bw, h: bh, d: bd, .. } => (w + bw, h.max(bh), d.max(bd)),
        k => (w + k.width(), h, d),
    });
    Some((dims, last))
}

/// `letters` of `font` as the main loop sets them (§§ 1034-1040): a box per
/// character or `=:` ligature, its letters in `lig`, font kerns between; the
/// last character. `None` for another ligature or a missing character.
pub(crate) fn set_letters(
    m: &crate::engine::tfm::Metrics,
    z: Scaled,
    font: u32,
    letters: &[u32],
) -> Option<(Vec<Kp>, u32)> {
    let mut items = Vec::new();
    let mut put = |c: u32, lig: u32, kern: Option<Scaled>| {
        let b = m.char_box(c, z)?;
        items.push(Kp::Box { w: b.width, h: b.height, d: b.depth, chr: Some((font, c)), lig });
        if let Some(w) = kern {
            items.push(Kp::Kern { w, explicit: false });
        }
        Some(())
    };
    let (&first, rest) = letters.split_first()?;
    let (mut cur, mut lig) = (first, LIG_NONE);
    for &next in rest {
        match m.lig_kern(cur, next, z) {
            None => put(cur, lig, None)?,
            Some(LigKern::Kern(x)) => put(cur, lig, Some(x))?,
            Some(LigKern::Lig { op: 0, char }) => {
                let had = if lig == LIG_NONE { lig_letter(LIG_NONE, cur) } else { lig };
                (cur, lig) = (char, lig_letter(had, next));
                continue;
            }
            Some(LigKern::Lig { .. }) => return None,
        }
        (cur, lig) = (next, LIG_NONE);
    }
    put(cur, lig, None)?;
    Some((items, cur))
}

fn add(sums: &mut [i64; 6], k: Kp) {
    sums[0] += k.width();
    if let Kp::Glue { st, sto, sh, .. } = k {
        sums[1 + usize::from(sto.min(3))] += st;
        sums[5] += sh;
    }
}

/// § 869: past a discretionary with a pre-break text, its replacement too.
fn after_break(items: &[Kp], e: usize) -> usize {
    match items.get(e) {
        Some(Kp::Disc(Some(_), r, _)) => e + 1 + usize::from(*r),
        _ => e,
    }
}

/// § 879: past a break, glue, penalties and explicit kerns go.
fn skip_discardable(items: &[Kp], mut s: usize) -> usize {
    while matches!(items.get(s), Some(Kp::Glue { .. } | Kp::Penalty(_) | Kp::Kern { explicit: true, .. })) {
        s += 1;
    }
    s
}

/// § 668 `vpack`: width, natural height and depth of a vertical list,
/// its depth at most `max`.
pub(crate) fn vpack(items: &[Kp], max: Scaled) -> [Scaled; 3] {
    let [mut w, mut x, mut d] = [0; 3];
    for k in items {
        match *k {
            Kp::Box { w: bw, h, d: bd, .. } => (w, x, d) = (w.max(bw), x + d + h, bd),
            Kp::Glue { w: g, .. } | Kp::Kern { w: g, .. } => (x, d) = (x + d + g, 0),
            _ => {}
        }
    }
    if d > max { [w, x + d - max, max] } else { [w, x, d] }
}

/// §§ 970-974 `vert_break`: the index of the best break of a vertical list
/// for height `h` and depth `dmax` (its length: the end).
fn vert_break(items: &[Kp], h: Scaled, dmax: Scaled) -> Option<usize> {
    let (mut cur, mut prev_dp, mut st, mut sh, mut best, mut least) = (0, 0, [0; 4], 0, 0, AWFUL);
    for i in 0..=items.len() {
        let pi = match items.get(i) {
            None => Some(EJECT),
            Some(Kp::Glue { .. }) => (i > 0 && items[i - 1].non_discardable()).then_some(0),
            Some(Kp::Kern { .. }) => matches!(items.get(i + 1), Some(Kp::Glue { .. })).then_some(0),
            Some(Kp::Penalty(p)) => Some(*p),
            _ => None,
        };
        if let Some(pi) = pi.filter(|&p| p < 10000) {
            // § 975.
            let b = match cur < h {
                true if st[1..] != [0; 3] => 0,
                true => badness(h - cur, st[0]),
                false if cur - h > sh => AWFUL,
                false => badness(cur - h, sh),
            };
            let b = match b < AWFUL {
                true if pi <= EJECT => pi,
                true if b < INF_BAD => b + pi,
                true => DEPLORABLE,
                false => b,
            };
            if b <= least {
                (best, least) = (i, b);
            }
            if b == AWFUL || pi <= EJECT {
                break;
            }
        }
        match items.get(i) {
            Some(&Kp::Box { h, d, .. }) => (cur, prev_dp) = (cur + prev_dp + h, d),
            // § 976: infinite shrink is an error.
            Some(&Kp::Glue { sho, sh: k, .. }) if sho > 0 && k != 0 => return None,
            Some(&Kp::Glue { w, st: s, sto, sh: k, .. }) => {
                st[usize::from(sto.min(3))] += s;
                sh += k;
                (cur, prev_dp) = (cur + prev_dp + w, 0);
            }
            Some(&Kp::Kern { w, .. }) => (cur, prev_dp) = (cur + prev_dp + w, 0),
            _ => {}
        }
        if prev_dp > dmax {
            (cur, prev_dp) = (cur + prev_dp - dmax, dmax);
        }
    }
    Some(best)
}

/// A recorded vertical list kept in the environment ([`Value::List`]).
pub(crate) type VList = (Rc<[Kp]>, Rc<[Rc<[crate::engine::tex::Token]>]>);

pub(crate) fn vlist(value: Value) -> Option<VList> {
    match value {
        Value::List(items, texts) => Some((items, texts)),
        _ => None,
    }
}

/// `a` then `b` as one list, the marks of `b` pointing past the texts of `a`.
pub(crate) fn vlist_cat((items, texts): VList, b: Option<VList>) -> VList {
    let Some((more, more_texts)) = b else { return (items, texts) };
    let moved = more.iter().map(|k| match *k {
        Kp::Mark(c, t) => Kp::Mark(c, t + texts.len() as u32),
        k => k,
    });
    (items.iter().copied().chain(moved).collect(), texts.iter().chain(more_texts.iter()).cloned().collect())
}

pub(crate) fn list_value(list: Option<VList>) -> Value {
    list.map_or(Value::Unknown, |(items, texts)| Value::List(items, texts))
}

/// §§ 977-979 `vsplit` to `h`: the part split off, and the rest as
/// `prune_page_top` (§ 968) leaves it below `\splittopskip` `top`, with
/// the items it discards (etex.ch `\splitdiscards`).
pub(crate) fn vsplit_list(items: &[Kp], h: Scaled, dmax: Scaled, mut top: Glue) -> Option<[Vec<Kp>; 3]> {
    let at = vert_break(items, h, dmax)?;
    let (mut kept, mut disc) = (Vec::new(), Vec::new());
    for (i, &k) in items[at..].iter().enumerate() {
        match k {
            Kp::Glue { .. } | Kp::Kern { .. } | Kp::Penalty(_) => disc.push(k),
            Kp::Box { h, .. } => {
                top.width = (top.width - h).max(0);
                kept.push(Kp::glue(top));
                kept.extend_from_slice(&items[at + i..]);
                break;
            }
            _ => kept.push(k),
        }
    }
    Some([items[..at].to_vec(), kept, disc])
}

/// § 108.
pub(crate) fn badness(t: i64, s: i64) -> i64 {
    if t == 0 {
        return 0;
    }
    if s <= 0 {
        return INF_BAD;
    }
    let r = if t <= 7230584 {
        t * 297 / s
    } else if s >= 1663497 {
        t / (s / 297)
    } else {
        t
    };
    if r > 1290 { INF_BAD } else { (r * r * r + 0o400000) / 0o1000000 }
}

/// One line `line_break` made: its height, depth, by how much its natural
/// width less its shrink passes the line width (§ 664), the item it ends at
/// (the list's length at the end) and its glue set (§§ 658, 664): the ratio,
/// negative when it shrinks, and the order it stretches.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Broken {
    pub(crate) height: Scaled,
    pub(crate) depth: Scaled,
    pub(crate) excess: Scaled,
    pub(crate) at: usize,
    pub(crate) set: (f64, u8),
}

#[derive(Clone, Copy)]
struct Pass {
    width: Scaled,
    bg: [i64; 6],
    /// `\linepenalty`, `\adjdemerits`, double and final hyphen demerits,
    /// `\hyphenpenalty`, `\exhyphenpenalty`.
    dem: [i64; 6],
    start: usize,
    fit0: u8,
    end_hyphenated: bool,
}

struct Active {
    start: usize,
    /// The post-break text the line starts with (§ 840).
    post: Option<Dims>,
    fit: u8,
    hyphenated: bool,
    total: i64,
    passive: Option<usize>,
}

impl Pass {
    /// §§ 829-860, 863-875 with `\looseness`=0 and one line width
    /// (`easy_line`=0, so all active nodes are one class).
    fn run(&self, items: &[Kp], threshold: i64, last_pass: bool) -> Option<(Vec<Broken>, u8)> {
        let n = items.len();
        let mut pre = vec![[0i64; 6]; n + 1];
        for (i, k) in items.iter().enumerate() {
            pre[i + 1] = pre[i];
            add(&mut pre[i + 1], *k);
        }
        // Break positions: (item, prev passive, pre-break box).
        let mut passive: Vec<(usize, Option<usize>, Option<Dims>)> = Vec::new();
        let mut active =
            vec![Active { start: self.start, post: None, fit: self.fit0, hyphenated: false, total: 0, passive: None }];
        let [line_pen, adj, double, fin, hyph, ex] = self.dem;
        for e in self.start..=n {
            let (pi, hyphenated, brk) = match items.get(e) {
                None => (EJECT, self.end_hyphenated, None),
                Some(Kp::Glue { .. }) if e > 0 => match items[e - 1] {
                    k if k.non_discardable() || matches!(k, Kp::Kern { explicit: false, .. }) => (0, false, None),
                    _ => continue,
                },
                Some(Kp::Kern { explicit: true, .. }) if matches!(items.get(e + 1), Some(Kp::Glue { .. })) => {
                    (0, false, None)
                }
                Some(Kp::Penalty(p)) if *p < 10000 => ((*p).max(EJECT), false, None),
                Some(Kp::Disc(b, ..)) => (if b.is_some() { hyph } else { ex }, true, *b),
                _ => continue,
            };
            let mut minimal = [AWFUL; 4];
            let mut best: [Option<usize>; 4] = [None; 4];
            let mut minimum = AWFUL;
            let mut kept = Vec::new();
            let count = active.len();
            for (ri, r) in std::mem::take(&mut active).into_iter().enumerate() {
                let mut s = pre[e];
                for (x, y) in s.iter_mut().zip(pre[r.start]) {
                    *x -= y;
                }
                for (x, y) in s.iter_mut().zip(self.bg) {
                    *x += y;
                }
                s[0] += brk.map_or(0, |b| b.0) + r.post.map_or(0, |p| p.0);
                let shortfall = self.width - s[0];
                let (b, fit) = if shortfall > 0 {
                    if s[2] != 0 || s[3] != 0 || s[4] != 0 {
                        (0, DECENT)
                    } else if shortfall > 7230584 && s[1] < 1663497 {
                        (INF_BAD, 0)
                    } else {
                        let b = badness(shortfall, s[1]);
                        (
                            b,
                            if b > 99 {
                                0
                            } else if b > 12 {
                                1
                            } else {
                                DECENT
                            },
                        )
                    }
                } else {
                    let b = if -shortfall > s[5] { INF_BAD + 1 } else { badness(-shortfall, s[5]) };
                    (b, if b > 12 { 3 } else { DECENT })
                };
                let mut artificial = false;
                let stays = if b > INF_BAD || pi == EJECT {
                    // § 854.
                    if last_pass && minimum == AWFUL && ri + 1 == count && kept.is_empty() {
                        artificial = true;
                    } else if b > threshold {
                        continue;
                    }
                    false
                } else if b > threshold {
                    kept.push(r);
                    continue;
                } else {
                    true
                };
                // § 859.
                let mut d = 0;
                if !artificial {
                    d = line_pen + b;
                    d = if d.abs() >= 10000 { 100000000 } else { d * d };
                    if pi > 0 {
                        d += pi * pi;
                    } else if pi < 0 && pi > EJECT {
                        d -= pi * pi;
                    }
                    if hyphenated && r.hyphenated {
                        d += if e < n { double } else { fin };
                    }
                    if (i16::from(fit) - i16::from(r.fit)).abs() > 1 {
                        d += adj;
                    }
                }
                d += r.total;
                if d <= minimal[usize::from(fit)] {
                    minimal[usize::from(fit)] = d;
                    passive.push((e, r.passive, brk));
                    best[usize::from(fit)] = Some(passive.len() - 1);
                    minimum = minimum.min(d);
                }
                if stays {
                    kept.push(r);
                }
            }
            active = kept;
            // § 836.
            if minimum < AWFUL {
                let limit = if adj.abs() >= AWFUL - minimum { AWFUL - 1 } else { minimum + adj.abs() };
                let next = after_break(items, e);
                // § 840: nothing is discarded after a post-break text.
                let post = match items.get(e) {
                    Some(&Kp::Disc(Some(_), _, p)) => p,
                    _ => None,
                };
                for fit in 0..4u8 {
                    if minimal[usize::from(fit)] <= limit {
                        active.push(Active {
                            start: if post.is_some() { next } else { skip_discardable(items, next) },
                            post,
                            fit,
                            hyphenated,
                            total: minimal[usize::from(fit)],
                            passive: best[usize::from(fit)],
                        });
                    }
                }
            }
            if active.is_empty() {
                return None;
            }
            if e == n {
                // § 874: the fewest demerits, first on ties, among nodes made at the end.
                let ends = || active.iter().filter(|r| r.passive.is_some_and(|q| passive[q].0 == n));
                // A forced break mid-paragraph keeps a node per fitness class
                // (§ 836): with `\adjdemerits` the lines after it choose
                // among them, which this segment cannot know.
                if !self.end_hyphenated && adj != 0 && ends().any(|r| ends().any(|o| o.fit != r.fit)) {
                    return None;
                }
                let mut bestr: Option<&Active> = None;
                for r in ends() {
                    if bestr.is_none_or(|b| r.total < b.total) {
                        bestr = Some(r);
                    }
                }
                let r = bestr?;
                let mut ends = Vec::new();
                let mut q = r.passive;
                while let Some(i) = q {
                    ends.push((passive[i].0, passive[i].2));
                    q = passive[i].1;
                }
                ends.reverse();
                let (mut from, mut post) = (self.start, None);
                let lines = ends
                    .into_iter()
                    .map(|(to, brk)| {
                        let (mut h, mut d) = (0, 0);
                        for (_, kh, kd) in [brk, post].into_iter().flatten() {
                            (h, d) = (h.max(kh), d.max(kd));
                        }
                        for k in &items[from.min(to)..to] {
                            if let Kp::Box { h: kh, d: kd, .. } = k {
                                (h, d) = (h.max(*kh), d.max(*kd));
                            }
                        }
                        // § 889 `hpack`: its natural width past its finite shrink (§ 664).
                        let mut s = pre[to];
                        for ((x, y), b) in s.iter_mut().zip(pre[from.min(to)]).zip(self.bg) {
                            *x += b - y;
                        }
                        let ends = [brk, post].into_iter().flatten().map(|b| b.0).sum::<Scaled>();
                        let (natural, width) = (s[0] + ends, self.width);
                        let excess = natural - width - s[5];
                        let set = match (1..=4).rev().find(|&o| s[o] != 0) {
                            Some(o) if natural < width => ((width - natural) as f64 / s[o] as f64, o as u8 - 1),
                            _ if natural > width && s[5] != 0 => {
                                (-((natural - width) as f64 / s[5] as f64).min(1.0), 0)
                            }
                            _ => (0.0, 0),
                        };
                        post = match items.get(to) {
                            Some(&Kp::Disc(Some(_), _, p)) => p,
                            _ => None,
                        };
                        from = after_break(items, to);
                        if post.is_none() {
                            from = skip_discardable(items, from);
                        }
                        Broken { height: h, depth: d, excess, at: to, set }
                    })
                    .collect();
                return Some((lines, r.fit));
            }
        }
        None
    }
}

impl Machine<'_> {
    /// `item` goes onto the current paragraph's recorded list; `None`
    /// stops following it.
    pub(crate) fn record_item(&mut self, item: Option<Kp>) {
        let Some(mut n) = self.natural else { return };
        match item {
            Some(k) => self.kp_push(&mut n.para.items, k),
            None => n.para.items = None,
        }
        self.natural = Some(n);
    }

    /// Arena list `id` as a list of its own, with its marks' texts.
    pub(crate) fn vlist_of(&self, id: u32) -> VList {
        let mut texts = Vec::new();
        let items = self.kp_list(id).into_iter().map(|k| match k {
            Kp::Mark(c, t) => {
                texts.push(self.kp.texts[t as usize].clone());
                Kp::Mark(c, texts.len() as u32 - 1)
            }
            k => k,
        });
        (items.collect(), texts.into())
    }

    /// The list engine slot `key` keeps: empty before any.
    pub(crate) fn list_slot(&mut self, key: &str) -> Option<VList> {
        let sym = self.engine_sym(key);
        match self.env.get(sym) {
            None => Some((Rc::from([]), Rc::from([]))),
            Some(_) => vlist(self.env.value(sym)),
        }
    }

    pub(crate) fn set_list_slot(&mut self, key: &str, list: Option<VList>) {
        self.set_engine(key, list_value(list), true);
    }
}

/// Where each line of `b` ends in the text: `items` is the list given to the breaker, `chars`
/// the characters set when each item was pushed, `base` those before the segment, `now` at its
/// end. Glue, boxes of no character, explicit kerns, penalties, whatsits and marks are pushed as
/// they come, so they fix the count; a character box's count is its letters after the last such
/// item (the main loop pushes it only when the next character comes, § 1034). `None` where the
/// counts do not rise along the lines (a list copied from a box, `\unhbox`).
pub(crate) fn line_ends(items: &[Kp], chars: &[u32], b: &Breaking, base: u32, now: u32) -> Option<Vec<(u32, LineEnd)>> {
    use LineEnd as L;
    let fixed = |k: &Kp| {
        matches!(
            k,
            Kp::Glue { .. }
                | Kp::Box { chr: None, .. }
                | Kp::Kern { explicit: true, .. }
                | Kp::Penalty(_)
                | Kp::Whatsit
                | Kp::Mark(..)
        )
    };
    // the count after each item of a list whose fixed items, in order, have counts `anchors`
    let after = |list: &[Kp], anchors: &mut dyn Iterator<Item = u32>| -> Option<Vec<u32>> {
        let mut pos = base;
        let mut out = vec![base];
        for k in list {
            match *k {
                // `\parfillskip` and its penalty (§ 816) come after the given list
                _ if fixed(k) => pos = anchors.next().unwrap_or(now),
                Kp::Box { chr: Some((_, c)), lig, .. } => pos += letters(c, lig)?.len() as u32,
                _ => {}
            }
            out.push(pos);
        }
        Some(out)
    };
    let anchor_chars = || items.iter().zip(chars).filter(|(k, _)| fixed(k)).map(|(_, &c)| c);
    let given = after(items, &mut anchor_chars())?;
    let broken = after(&b.items, &mut anchor_chars())?;
    let mut out = Vec::with_capacity(b.lines.len());
    for line in &b.lines {
        let end = match b.items.get(line.at) {
            None => (now, L::Line),
            Some(Kp::Disc(..)) => match b.points.iter().find(|p| p.0 == line.at) {
                Some(&(_, s, j)) => (given[s] + j as u32, L::Hyphen),
                // after an explicit hyphen, a character of the text
                None => (broken[line.at], L::Line),
            },
            Some(_) => (broken[line.at + 1], L::Line),
        };
        out.push(end);
    }
    let rising = out.iter().try_fold(base, |was, &(e, _)| (e > was && e <= now).then_some(e)).is_some();
    (rising && out.last().is_some_and(|l| l.0 == now)).then_some(out)
}
