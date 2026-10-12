//! TeX's line breaker (tex.web §§ 813-890, Knuth-Plass) for a paragraph
//! whose items, metrics and parameters are all known. Anything else keeps
//! the line-count interval of [`crate::mode`].

use crate::builtins::CodeTable;
use crate::machine::Machine;
use crate::mode::Natural;
use crate::value::{Glue, Scaled, Value};
use std::collections::HashMap;

/// One item of a paragraph's horizontal list, as `line_break` reads it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub(crate) enum Kp {
    /// A box, rule or character (`chr`: font and code; `lig`: may be a
    /// ligature).
    Box {
        w: Scaled,
        h: Scaled,
        d: Scaled,
        chr: Option<(u32, u32)>,
        lig: bool,
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
    /// A discretionary with an empty replacement and post-break list, and
    /// this pre-break box (none after a `\hyphenchar`, § 1039).
    Disc(Option<Dims>),
}

impl Kp {
    pub(crate) fn glue(g: Glue) -> Kp {
        Kp::Glue { w: g.width, st: g.stretch.amount, sto: g.stretch.order, sh: g.shrink.amount, sho: g.shrink.order }
    }
    fn width(self) -> Scaled {
        match self {
            Kp::Box { w, .. } | Kp::Glue { w, .. } | Kp::Kern { w, .. } => w,
            _ => 0,
        }
    }
    /// § 148 `precedes_break` and characters: glue after it is a breakpoint.
    fn non_discardable(self) -> bool {
        matches!(self, Kp::Box { .. } | Kp::Whatsit | Kp::Disc(_))
    }
}

/// The recorded lists, hash-consed: a list is the id of its last node.
#[derive(Default)]
pub(crate) struct Arena {
    nodes: Vec<(Kp, u32, u32)>,
    index: HashMap<(Kp, u32), u32>,
}

/// Longest paragraph segment and largest arena followed.
/// A box's width, height and depth.
type Dims = (Scaled, Scaled, Scaled);

const MAX_ITEMS: u32 = 4000;
const MAX_NODES: usize = 1 << 21;
const INF_BAD: i64 = 10000;
const EJECT: i64 = -10000;
const AWFUL: i64 = 0o7777777777;
const DECENT: u8 = 2;
/// A line's fitness class is not known.
pub(crate) const UNFIT: u8 = 4;

impl Machine<'_> {
    /// `item` goes onto the paragraph's recorded list `items`.
    pub(crate) fn kp_push(&mut self, items: &mut Option<u32>, item: Kp) {
        let Some(prev) = *items else { return };
        let a = &mut self.kp;
        let len = if prev == 0 { 0 } else { a.nodes[prev as usize - 1].2 };
        if len >= MAX_ITEMS || a.nodes.len() >= MAX_NODES {
            *items = None;
            return;
        }
        let next = a.nodes.len() as u32 + 1;
        let nodes = &mut a.nodes;
        *items = Some(*a.index.entry((item, prev)).or_insert_with(|| {
            nodes.push((item, prev, len + 1));
            next
        }));
    }

    /// `\unskip` and relatives take the recorded tail away where it is
    /// the item `last` the list's tail says.
    pub(crate) fn kp_pop(&mut self, items: &mut Option<u32>, removed: crate::mode::Removed, last: crate::mode::Last) {
        use crate::mode::{Last, Removed};
        let Some(id) = items.filter(|&i| i > 0) else { return };
        let (k, prev, _) = self.kp.nodes[id as usize - 1];
        let hit = matches!(
            (removed, last),
            (Removed::Glue, Last::Glue(..))
                | (Removed::Kern, Last::Kern(_))
                | (Removed::Penalty, Last::Penalty(_))
                | (_, Last::Unknown)
        );
        if hit {
            *items = match (last, k) {
                (Last::Glue(w, _), Kp::Glue { w: v, .. })
                | (Last::Kern(w), Kp::Kern { w: v, .. })
                | (Last::Penalty(w), Kp::Penalty(v))
                    if w == v =>
                {
                    Some(prev)
                }
                _ => None,
            };
        }
    }

    fn kp_list(&self, mut id: u32) -> Vec<Kp> {
        let mut out = Vec::new();
        while id > 0 {
            let (k, prev, _) = self.kp.nodes[id as usize - 1];
            out.push(k);
            id = prev;
        }
        out.reverse();
        out
    }

    /// The lines (height, depth) the segment of `n` ending here breaks
    /// into, and the last line's fitness class, where all is known.
    /// `last`: at `\par` (§ 816), else at a forced break.
    pub(crate) fn break_lines(&mut self, n: &Natural, last: bool) -> Option<(Vec<(Scaled, Scaled)>, u8)> {
        let p = n.para;
        if p.lost || p.rough && p.lines.1 > 0 {
            return None;
        }
        let mut items = self.kp_list(p.items?);
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
        let width = self.line_width().known()?;
        let int = |m: &mut Self, name| m.int_parameter(name);
        #[rustfmt::skip]
        let [loose, line, adj, double, fin, hyph, ex, tol, fit, pretol, uc] = [
            "looseness", "linepenalty", "adjdemerits", "doublehyphendemerits", "finalhyphendemerits",
            "hyphenpenalty", "exhyphenpenalty", "tolerance", "lastlinefit", "pretolerance", "uchyph",
        ]
        .map(|name| int(self, name));
        let (loose, fit, adj) = (loose?, fit?, adj?);
        if loose != 0 || fit > 0 || p.fit == UNFIT && adj != 0 {
            return None;
        }
        let emergency = self.dimen_parameter("emergencystretch").known()?;
        let (left, right) = (self.glue_parameter("leftskip")?, self.glue_parameter("rightskip")?);
        if last {
            // § 816: a final glue becomes `\penalty10000`, then `\parfillskip`.
            if matches!(items.last(), Some(Kp::Glue { .. })) {
                items.pop();
            }
            items.push(Kp::Penalty(10000));
            items.push(Kp::glue(self.glue_parameter("parfillskip")?));
        }
        // § 825: infinite shrink is an error TeX makes finite.
        let infinite = |k: &Kp| matches!(*k, Kp::Glue { sho, sh, .. } if sho > 0 && sh != 0);
        if items.iter().chain(&[Kp::glue(left), Kp::glue(right)]).any(infinite) {
            return None;
        }
        let mut bg = [0i64; 6];
        for g in [left, right] {
            add(&mut bg, Kp::glue(g));
        }
        let pass = Pass {
            width,
            bg,
            dem: [line?, adj, double?, fin?, hyph?, ex?],
            start,
            fit0: if p.fit == UNFIT { DECENT } else { p.fit },
            end_hyphenated: last,
        };
        // § 863: a first pass without hyphenation, a second with, a third
        // with `\emergencystretch`.
        let pretol = pretol?;
        if pretol >= 0
            && let Some(r) = pass.run(&items, pretol, false)
        {
            return Some(r);
        }
        let hyphenated = self.hyphenate(&items, start, uc?)?;
        let tol = tol?;
        if let Some(r) = pass.run(&hyphenated, tol, emergency <= 0) {
            return Some(r);
        }
        let mut third = pass;
        third.bg[1] += emergency;
        (emergency > 0).then(|| third.run(&hyphenated, tol, true)).flatten()
    }

    /// §§ 894-903: the second pass's list, with a discretionary hyphen at
    /// each hyphenation point of each word after glue. `None` where a word
    /// is not followed exactly (ligatures, kerns at a point, boundary
    /// characters, unknown codes or patterns).
    fn hyphenate(&mut self, items: &[Kp], start: usize, uc: i64) -> Option<Vec<Kp>> {
        let (left, right) = self.hyphen_mins();
        let tables = self.hyph_tables();
        let mut out: Vec<Kp> = items[..start].to_vec();
        let mut points: HashMap<usize, Dims> = HashMap::new();
        for i in start..items.len() {
            if !matches!(items[i], Kp::Glue { .. }) {
                continue;
            }
            // § 896: the first letter after the glue.
            let mut s = i + 1;
            let (hf, c) = loop {
                match items.get(s) {
                    Some(Kp::Box { chr: Some((f, c)), lig, .. }) => {
                        let lc = self.lc(*c)?;
                        if lc == 0 && !lig {
                            s += 1;
                            continue;
                        }
                        if *lig {
                            return None;
                        }
                        break (*f, *c);
                    }
                    Some(Kp::Kern { explicit: false, .. } | Kp::Whatsit) => s += 1,
                    _ => break (0, u32::MAX),
                }
            };
            if c == u32::MAX || self.lc(c)? != i64::from(c) && uc <= 0 {
                continue;
            }
            let hyf_char = match self.font_field(hf, "hyphenchar") {
                Value::Int(h) if (0..=255).contains(&h) => h as u32,
                Value::Int(_) => continue,
                _ => return None,
            };
            // § 897: the letters, font kerns between them.
            let mut word = Vec::new();
            let mut at = Vec::new();
            let mut e = s;
            loop {
                match items.get(e) {
                    Some(Kp::Box { chr: Some((f, c)), lig, .. }) if *f == hf => {
                        let lc = self.lc(*c)?;
                        if lc == 0 || word.len() == 63 {
                            break;
                        }
                        if *lig {
                            return None;
                        }
                        word.push(lc as u32);
                        at.push(e);
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
            let hyf = self.hyf(tables.as_ref()?, &word);
            let m = self.font_metrics(hf)?;
            let z = self.font_size(hf)?;
            if m.boundary_char().is_some() {
                return None;
            }
            let b = m.char_box(hyf_char, z)?;
            for j in left..=word.len() - right {
                if hyf[j] % 2 == 1 {
                    let (k, Some(&Kp::Box { chr: Some((_, prev)), .. })) = (at[j - 1], items.get(at[j - 1])) else {
                        return None;
                    };
                    // A kern at the point or with the hyphen is not followed.
                    if k + 1 != at[j] || m.lig_kern(prev, hyf_char, z).is_some() {
                        return None;
                    }
                    points.insert(k, (b.width, b.height, b.depth));
                }
            }
        }
        for (i, k) in items.iter().enumerate().skip(start) {
            out.push(*k);
            if let Some(&pre) = points.get(&i) {
                out.push(Kp::Disc(Some(pre)));
            }
        }
        Some(out)
    }

    fn lc(&mut self, c: u32) -> Option<i64> {
        self.char_code_known(CodeTable::Lower, char::from_u32(c)?)
    }
}

fn add(sums: &mut [i64; 6], k: Kp) {
    sums[0] += k.width();
    if let Kp::Glue { st, sto, sh, .. } = k {
        sums[1 + usize::from(sto.min(3))] += st;
        sums[5] += sh;
    }
}

/// § 879: past a break, glue, penalties and explicit kerns go.
fn skip_discardable(items: &[Kp], mut s: usize) -> usize {
    while matches!(items.get(s), Some(Kp::Glue { .. } | Kp::Penalty(_) | Kp::Kern { explicit: true, .. })) {
        s += 1;
    }
    s
}

/// § 108.
fn badness(t: i64, s: i64) -> i64 {
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
    fit: u8,
    hyphenated: bool,
    total: i64,
    passive: Option<usize>,
}

impl Pass {
    /// §§ 829-860, 863-875 with `\looseness`=0 and one line width
    /// (`easy_line`=0, so all active nodes are one class).
    fn run(&self, items: &[Kp], threshold: i64, last_pass: bool) -> Option<(Vec<(Scaled, Scaled)>, u8)> {
        let n = items.len();
        let mut pre = vec![[0i64; 6]; n + 1];
        for (i, k) in items.iter().enumerate() {
            pre[i + 1] = pre[i];
            add(&mut pre[i + 1], *k);
        }
        // Break positions: (item, prev passive, pre-break box).
        let mut passive: Vec<(usize, Option<usize>, Option<Dims>)> = Vec::new();
        let mut active = vec![Active { start: self.start, fit: self.fit0, hyphenated: false, total: 0, passive: None }];
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
                Some(Kp::Disc(b)) => (if b.is_some() { hyph } else { ex }, true, *b),
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
                s[0] += brk.map_or(0, |b| b.0);
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
                let next = if brk.is_some() { e + 1 } else { e };
                for fit in 0..4u8 {
                    if minimal[usize::from(fit)] <= limit {
                        active.push(Active {
                            start: skip_discardable(items, next),
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
                let mut bestr: Option<&Active> = None;
                for r in active.iter().filter(|r| r.passive.is_some_and(|q| passive[q].0 == n)) {
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
                let mut from = self.start;
                let lines = ends
                    .into_iter()
                    .map(|(to, brk)| {
                        let (mut h, mut d) = brk.map_or((0, 0), |b| (b.1.max(0), b.2.max(0)));
                        for k in &items[from.min(to)..to] {
                            if let Kp::Box { h: kh, d: kd, .. } = k {
                                (h, d) = (h.max(*kh), d.max(*kd));
                            }
                        }
                        from = skip_discardable(items, if brk.is_some() { to + 1 } else { to });
                        (h, d)
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
}
