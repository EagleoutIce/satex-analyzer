//! Code clones: blocks of the document's own source that repeat with only
//! their values changed. The files are read as TeX's mouth reads them, each
//! run of letters and other characters becomes one value, and windows of the
//! normalized tokens are hashed. Pairs that share a window are extended,
//! chained across small gaps, cut to balanced blocks and grouped. Aligning a
//! group's places gives the holes a macro takes as parameters.

use std::rc::Rc;

use regex::Regex;

use crate::engine::tex::{Catcode, CatcodeTable, FileId, Span, Tok};
use crate::fixed::{HashMap, HashSet};
use crate::linter::fix::{Applicability, Edit, Fix, Pos, Scan, Text};
use crate::linter::verbatim::{self, Bodies};
use crate::linter::{Category, Report, Rule, Severity};
use crate::project::config::{CloneScope, Clones};

pub static RULES: &[Rule] = &[Rule {
    code: "clones",
    category: Category::Style,
    severity: Severity::Info,
    summary: "a block of the source repeats with only its values changed",
    explanation: "\
Two figures that differ only in the width, the file, the caption and the
label are one macro used twice. Copies drift apart when one is edited and
the other is not.

The rule reads the document's own files token by token, as TeX reads them,
with comments and spaces dropped. A run of letters and other characters is
a value. Blocks that match up to their values are clones, and so are blocks
that match after a few inserted or deleted tokens (`lints.clones.similarity`).
The finding stands at the first block and names the other places. Its
message lists each hole, the places where the blocks differ, with the values
there.

A clone shares at least `min_tokens` tokens, and its values take at most
`max_value_share` of it, so paragraphs and lists of items are no clones.
Library code is never read. Two of the document's macros with the same body
are reported too.

Fix by defining one macro with a parameter per hole and using it at every
place. The fix writes it when there are at most `max_parameters` holes (TeX
allows 9). It is unsafe: the macro's name is a guess.",
    references: &[],
    fixes: &[Applicability::Unsafe],
    engines: &[],
    run: clones,
}];

/// What a token is, as far as clones go.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Cs,
    Value,
    Open,
    Close,
    Mark,
    /// `#`: a block holding one is part of a definition and gets no macro.
    Param,
    /// Where a block may not run on: a file's end, a verbatim body, an ignored environment.
    Cut,
}

/// One normalized token: a control sequence (`\begin{x}` is one), a value,
/// a brace or a mark. Windows compare `norm`, holes compare `exact`.
#[derive(Clone, Copy, Debug)]
struct Unit {
    kind: Kind,
    norm: u32,
    exact: u32,
    toks: u32,
    file: FileId,
    start: Pos,
    end: Pos,
    body: bool,
    /// What it does to the nesting: a brace or an environment opens or closes.
    delta: i8,
}

/// The `norm` of every value.
const VALUE: u32 = 0;

#[derive(Default)]
struct Names {
    ids: HashMap<String, u32>,
    list: Vec<String>,
}

impl Names {
    fn id(&mut self, text: &str) -> u32 {
        if let Some(&id) = self.ids.get(text) {
            return id;
        }
        self.list.push(text.to_string());
        let id = self.list.len() as u32;
        self.ids.insert(text.to_string(), id);
        id
    }

    fn text(&self, id: u32) -> &str {
        id.checked_sub(1).and_then(|i| self.list.get(i as usize)).map_or("", String::as_str)
    }
}

/// Reads files into one stream of [`Unit`]s, with a cut between files.
struct Lexer<'a> {
    cfg: &'a Clones,
    ignore: Vec<Regex>,
    names: Names,
    out: Vec<Unit>,
    value: Option<(Unit, String)>,
    /// An ignored environment being skipped: its name, its nesting and where its units start.
    skip: Option<(u32, u32, usize)>,
    cuts: u32,
}

impl<'a> Lexer<'a> {
    fn new(cfg: &'a Clones) -> Lexer<'a> {
        let ignore = cfg.ignore.iter().filter_map(|p| Regex::new(&format!("^(?:{p})$")).ok()).collect();
        Lexer { cfg, ignore, names: Names::default(), out: Vec::new(), value: None, skip: None, cuts: 0 }
    }

    /// One file, from its first line, with `body` telling where the document body is.
    fn file(&mut self, file: FileId, text: &Text, cats: CatcodeTable, bodies: &Bodies, body: impl Fn(Pos) -> bool) {
        let Some(mut scan) = Scan::at(text, Pos::new(1, 1), cats) else { return };
        let mut space = false;
        while let Some(lexeme) = scan.advance() {
            let (start, end) = (lexeme.start, lexeme.end);
            if bodies.holds(file, start.line, start.col) {
                self.flush();
                self.cut();
                continue;
            }
            let at = body(start);
            let unit =
                |kind, toks| Unit { kind, norm: VALUE, exact: VALUE, toks, file, start, end, body: at, delta: 0 };
            match lexeme.tok {
                Tok::Chr(_, Catcode::Space) => space = self.value.is_some(),
                Tok::Chr(c, Catcode::Letter | Catcode::Other) if !"[]=,".contains(c) => {
                    let (value, text) = self.value.get_or_insert_with(|| (unit(Kind::Value, 0), String::new()));
                    if std::mem::take(&mut space) {
                        text.push(' ');
                    }
                    text.push(c);
                    value.toks += 1;
                    value.end = end;
                }
                tok => {
                    self.flush();
                    space = false;
                    let (kind, text) = match tok {
                        Tok::Cs(_) => {
                            let name = scan.name(&lexeme).unwrap_or_default().to_string();
                            match name.as_str() {
                                "makeatletter" => scan.set_cat('@', Catcode::Letter),
                                "makeatother" => scan.set_cat('@', Catcode::Other),
                                _ => {}
                            }
                            (Kind::Cs, format!("\\{name}"))
                        }
                        Tok::Chr(c, Catcode::Begin) => (Kind::Open, c.to_string()),
                        Tok::Chr(c, Catcode::End) => (Kind::Close, c.to_string()),
                        Tok::Chr(c, Catcode::Param) => (Kind::Param, c.to_string()),
                        Tok::Chr(c, _) => (Kind::Mark, c.to_string()),
                        Tok::Param(n) => (Kind::Param, format!("#{n}")),
                    };
                    let id = self.names.id(&text);
                    let delta = match kind {
                        Kind::Open => 1,
                        Kind::Close => -1,
                        _ => 0,
                    };
                    self.push(Unit { norm: id, exact: id, delta, ..unit(kind, 1) });
                }
            }
        }
        self.flush();
        if let Some((_, _, mark)) = self.skip.take() {
            self.out.truncate(mark);
        }
        self.cut();
    }

    fn flush(&mut self) {
        if let Some((mut value, text)) = self.value.take() {
            value.exact = self.names.id(&text);
            self.push(value);
        }
    }

    fn cut(&mut self) {
        if self.out.last().is_some_and(|u| u.kind == Kind::Cut) {
            return;
        }
        self.cuts += 1;
        let at = self.out.last().map_or(Pos::new(1, 1), |u| u.end);
        let id = u32::MAX - self.cuts;
        let (file, body) = self.out.last().map_or((0, false), |u| (u.file, u.body));
        self.out.push(Unit { kind: Kind::Cut, norm: id, exact: id, toks: 0, file, start: at, end: at, body, delta: 0 });
    }

    fn push(&mut self, unit: Unit) {
        let wanted = match self.cfg.scope {
            CloneScope::Both => true,
            CloneScope::Body => unit.body,
            CloneScope::Preamble => !unit.body,
        };
        if !wanted {
            self.cut();
            return;
        }
        self.out.push(unit);
        // `\begin{name}` and `\end{name}` are one unit, whose name is no value.
        let n = self.out.len();
        if unit.kind != Kind::Close || n < 4 {
            return;
        }
        let [cs, open, name] = [self.out[n - 4], self.out[n - 3], self.out[n - 2]];
        let command = self.names.text(cs.exact).to_string();
        if cs.kind != Kind::Cs || open.kind != Kind::Open || name.kind != Kind::Value {
            return;
        }
        if command != "\\begin" && command != "\\end" {
            return;
        }
        let begin = command == "\\begin";
        let env = self.names.text(name.exact).to_string();
        let id = self.names.id(&format!("{command}{{{env}}}"));
        let toks = cs.toks + open.toks + name.toks + unit.toks;
        self.out.truncate(n - 4);
        let delta = if begin { 1 } else { -1 };
        self.out.push(Unit { kind: Kind::Cs, norm: id, exact: id, toks, end: unit.end, delta, ..cs });
        let env = name.exact;
        match self.skip {
            Some((name, depth, mark)) if name == env => {
                let depth = if begin { depth + 1 } else { depth - 1 };
                self.skip = Some((name, depth, mark));
                if depth == 0 {
                    self.skip = None;
                    self.out.truncate(mark);
                    self.cut();
                }
            }
            None if begin && self.ignore.iter().any(|r| r.is_match(self.names.text(env))) => {
                self.skip = Some((env, 1, self.out.len() - 1));
            }
            _ => {}
        }
    }
}

/// A place: the units from the first up to, not including, the second.
type Place = (u32, u32);

/// Two places that match: segments `(a, b, len)` where `b`'s units match `a`'s.
#[derive(Clone, Debug)]
struct Pair {
    segs: Vec<(u32, u32, u32)>,
}

impl Pair {
    fn a(&self) -> (u32, u32) {
        let last = self.segs[self.segs.len() - 1];
        (self.segs[0].0, last.0 + last.2)
    }

    fn b(&self) -> (u32, u32) {
        let last = self.segs[self.segs.len() - 1];
        (self.segs[0].1, last.1 + last.2)
    }

    /// Where the unit `a` stands in `b`'s place, when it is matched.
    fn map(&self, a: u32) -> Option<u32> {
        let i = self.segs.partition_point(|s| s.0 + s.2 <= a);
        self.segs.get(i).filter(|s| s.0 <= a).map(|s| s.1 + a - s.0)
    }

    fn matched(&self) -> u32 {
        self.segs.iter().map(|s| s.2).sum()
    }
}

/// The clone search over one stream of units.
struct Search<'a> {
    u: &'a [Unit],
    names: &'a Names,
    cfg: &'a Clones,
    /// Tokens before each unit, for a block's size.
    toks: Vec<u32>,
}

impl Search<'_> {
    fn tokens(&self, from: u32, to: u32) -> u32 {
        self.toks[to as usize] - self.toks[from as usize]
    }

    fn text(&self, i: u32) -> &str {
        self.names.text(self.u[i as usize].exact)
    }

    /// Maximal runs of equal normalized units, found from shared windows.
    fn matches(&self) -> Vec<(u32, u32, u32)> {
        let (u, k) = (self.u, self.cfg.seed.max(2));
        let n = u.len();
        if n < 2 * k {
            return Vec::new();
        }
        let mut windows: Vec<(u64, u32)> = Vec::new();
        let mut run = 0;
        for (i, unit) in u.iter().enumerate() {
            run = if unit.kind == Kind::Cut { 0 } else { run + 1 };
            if run >= k {
                let start = i + 1 - k;
                let hash = u[start..=i]
                    .iter()
                    .fold(0xcbf2_9ce4_8422_2325u64, |h, x| (h ^ u64::from(x.norm)).wrapping_mul(0x100_0000_01b3));
                windows.push((hash, start as u32));
            }
        }
        windows.sort_unstable();
        let mut next = vec![u32::MAX; n];
        for w in windows.windows(2) {
            if w[0].0 == w[1].0 {
                next[w[0].1 as usize] = w[1].1;
            }
        }
        drop(windows);
        let mut covered: HashMap<u32, u32> = HashMap::default();
        let mut out = Vec::new();
        for p in 0..n as u32 {
            let mut q = next[p as usize];
            for _ in 0..self.cfg.partners {
                if q == u32::MAX {
                    break;
                }
                let d = q - p;
                let floor = covered.get(&d).copied().unwrap_or(0);
                if d as usize >= k && floor <= p {
                    let mut l = 0;
                    while p - l > floor && u[(p - l - 1) as usize].norm == u[(q - l - 1) as usize].norm {
                        l += 1;
                    }
                    let mut r = 0;
                    while ((q + r) as usize) < n
                        && p + r < q - l
                        && u[(p + r) as usize].norm == u[(q + r) as usize].norm
                    {
                        r += 1;
                    }
                    covered.insert(d, p + r);
                    if (l + r) as usize >= k {
                        out.push((p - l, q - l, l + r));
                    }
                }
                q = next[q as usize];
            }
        }
        out.sort_unstable();
        out
    }

    /// Matches chained across small gaps while the pair stays `similarity` alike.
    fn chains(&self, matches: &[(u32, u32, u32)]) -> Vec<Pair> {
        let s = self.cfg.similarity.clamp(0.01, 1.0);
        let mut used = vec![false; matches.len()];
        let mut out = Vec::new();
        for i in 0..matches.len() {
            if used[i] {
                continue;
            }
            let mut segs = vec![matches[i]];
            let mut matched = matches[i].2;
            loop {
                let (a, b, len) = segs[segs.len() - 1];
                let (a0, b0) = (segs[0].0, segs[0].1);
                let gap = ((f64::from(matched) * (1.0 - s) / s).ceil() as u32).max(self.cfg.seed as u32);
                let from = matches.partition_point(|m| m.0 < a + len);
                let best = (from..matches.len())
                    .take_while(|&j| matches[j].0 <= a + len + gap)
                    .filter(|&j| !used[j])
                    .filter(|&j| {
                        let m = matches[j];
                        m.1 >= b + len && m.1 <= b + len + gap && m.0 + m.2 <= b0
                    })
                    .filter(|&j| {
                        let m = matches[j];
                        let span = (m.0 + m.2 - a0).max(m.1 + m.2 - b0);
                        f64::from(matched + m.2) >= s * f64::from(span)
                    })
                    .min_by_key(|&j| (matches[j].0 - a - len) + (matches[j].1 - b - len));
                let Some(j) = best else { break };
                used[j] = true;
                matched += matches[j].2;
                segs.push(matches[j]);
            }
            out.push(Pair { segs });
        }
        out
    }

    fn start_ok(&self, x: u32) -> bool {
        let unit = self.u[x as usize];
        match unit.kind {
            Kind::Close | Kind::Cut => false,
            Kind::Mark => !matches!(self.text(x), "]" | "=" | ","),
            // A group right after a command or an option is its argument.
            Kind::Open => {
                x == 0 || {
                    let before = self.u[x as usize - 1];
                    !(before.kind == Kind::Cs || (before.kind == Kind::Mark && self.text(x - 1) == "]"))
                }
            }
            _ => true,
        }
    }

    /// Whether a block may end before unit `x`: no argument of its last command follows.
    fn end_ok(&self, x: u32) -> bool {
        let unit = self.u[x as usize];
        !(unit.kind == Kind::Open || (unit.kind == Kind::Mark && self.text(x) == "["))
    }

    fn balanced(&self, from: u32, to: u32) -> bool {
        let mut depth = 0;
        for unit in &self.u[from as usize..to as usize] {
            depth += i32::from(unit.delta);
            if depth < 0 || unit.kind == Kind::Cut {
                return false;
            }
        }
        depth == 0
    }

    /// The pair cut to its largest block that is balanced on both sides and
    /// starts and ends where both places agree.
    fn trim(&self, pair: &Pair) -> Option<Pair> {
        let (a0, a1) = pair.a();
        let mut candidates = Vec::new();
        let mut first: HashMap<i32, u32> = HashMap::default();
        let mut depth = 0;
        for x in a0..=a1 {
            if x > a0 {
                let delta = i32::from(self.u[x as usize - 1].delta);
                if delta < 0 {
                    first.remove(&depth);
                }
                depth += delta;
            }
            let end_mapped = x > a0 && pair.map(x - 1).is_some();
            if end_mapped
                && let Some(&from) = first.get(&depth)
                && self.end_ok(x)
                && pair.map(x - 1).is_some_and(|y| self.end_ok(y + 1))
            {
                candidates.push((self.tokens(from, x), from, x));
            }
            if x < a1 && pair.map(x).is_some() && self.start_ok(x) {
                first.entry(depth).or_insert(x);
            }
        }
        candidates.sort_unstable_by(|a, b| b.cmp(a));
        candidates.dedup_by_key(|c| c.1);
        for &(_, mut from, mut to) in candidates.iter().take(8) {
            let differs = |x: u32| pair.map(x).is_none_or(|y| self.u[x as usize].exact != self.u[y as usize].exact);
            while from < to && differs(from) {
                from += 1;
            }
            while to > from && differs(to - 1) {
                to -= 1;
            }
            if to <= from || !self.start_ok(from) || !self.end_ok(to) {
                continue;
            }
            let (Some(b0), Some(b1)) = (pair.map(from), pair.map(to - 1)) else { continue };
            if !self.end_ok(b1 + 1) || !self.balanced(b0, b1 + 1) {
                continue;
            }
            let segs = pair
                .segs
                .iter()
                .filter_map(|&(a, b, len)| {
                    let (lo, hi) = (a.max(from), (a + len).min(to));
                    (lo < hi).then(|| (lo, b + lo - a, hi - lo))
                })
                .collect();
            return Some(Pair { segs });
        }
        None
    }

    /// Whether a trimmed pair is a clone worth a finding.
    fn worth(&self, pair: &Pair) -> bool {
        let ((a0, a1), (b0, b1)) = (pair.a(), pair.b());
        let shared: u32 = pair
            .segs
            .iter()
            .flat_map(|&(a, b, len)| (0..len).map(move |i| (a + i, b + i)))
            .filter(|&(x, y)| self.u[x as usize].exact == self.u[y as usize].exact)
            .map(|(x, _)| self.u[x as usize].toks)
            .sum();
        let total = self.tokens(a0, a1).max(self.tokens(b0, b1)).max(1);
        let units = (a1 - a0).max(b1 - b0);
        let lines = |from: u32, to: u32| self.u[to as usize - 1].end.line - self.u[from as usize].start.line + 1;
        shared as usize >= self.cfg.min_tokens
            && f64::from(total - shared) <= self.cfg.max_value_share * f64::from(total)
            && f64::from(pair.matched()) >= self.cfg.similarity * f64::from(units)
            && lines(a0, a1).min(lines(b0, b1)) as usize >= self.cfg.min_lines
    }

    /// The clone groups: places joined by their pairs, each with its pair to the first place.
    fn groups(&self) -> Vec<Group> {
        let pairs: Vec<Pair> =
            self.chains(&self.matches()).iter().filter_map(|p| self.trim(p)).filter(|p| self.worth(p)).collect();
        let mut index: HashMap<(u32, u32), usize> = HashMap::default();
        let mut parent: Vec<usize> = Vec::new();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        let mut by_pair: HashMap<(Place, Place), &Pair> = HashMap::default();
        for pair in &pairs {
            let [a, b] = [pair.a(), pair.b()].map(|o| {
                *index.entry(o).or_insert_with(|| {
                    parent.push(parent.len());
                    parent.len() - 1
                })
            });
            let (ra, rb) = (root(&mut parent, a), root(&mut parent, b));
            parent[ra.max(rb)] = ra.min(rb);
            by_pair.entry((pair.a(), pair.b())).or_insert(pair);
        }
        let mut members: HashMap<usize, Vec<(u32, u32)>> = HashMap::default();
        for (&o, &i) in &index {
            let r = root(&mut parent, i);
            members.entry(r).or_default().push(o);
        }
        let mut groups: Vec<Group> = Vec::new();
        for (_, mut places) in members {
            places.sort_unstable();
            let first = places[0];
            let mut kept = vec![(first, Pair { segs: vec![(first.0, first.0, first.1 - first.0)] })];
            for &o in &places[1..] {
                let Some(pair) = by_pair.get(&(first, o)) else { continue };
                if kept.iter().all(|(k, _)| k.1 <= o.0 || o.1 <= k.0) {
                    kept.push((o, (*pair).clone()));
                }
            }
            if kept.len() < self.cfg.min_occurrences.max(2) {
                continue;
            }
            // A hole that opens or closes a group is no value: the places differ in structure.
            let group = self.holes(kept);
            if group.holes.iter().flatten().all(|&(from, to)| self.balanced(from, to)) {
                groups.push(group);
            }
        }
        groups.sort_by_key(|g| (std::cmp::Reverse(g.shared * g.places.len() as u32), g.places[0].0));
        let mut taken: Vec<(u32, u32)> = Vec::new();
        groups.retain(|g| {
            let free = g.places.iter().all(|&o| taken.iter().all(|t| t.1 <= o.0 || o.1 <= t.0));
            if free {
                taken.extend(g.places.iter().copied());
            }
            free
        });
        groups.sort_by_key(|g| g.places[0].0);
        groups
    }

    /// The group's holes: the runs of the first place where some other differs.
    fn holes(&self, places: Vec<((u32, u32), Pair)>) -> Group {
        let (t0, t1) = places[0].0;
        let anchor = |t: u32| {
            places.iter().all(|(_, p)| p.map(t).is_some_and(|m| self.u[m as usize].exact == self.u[t as usize].exact))
        };
        let mut holes: Vec<Vec<(u32, u32)>> = Vec::new();
        let mut prev: Vec<u32> = places.iter().map(|(o, _)| o.0).collect();
        let mut shared = 0;
        let close = |at: Vec<u32>, prev: &mut Vec<u32>, holes: &mut Vec<Vec<(u32, u32)>>| {
            if prev.iter().zip(&at).any(|(p, a)| p < a) {
                holes.push(prev.iter().zip(&at).map(|(&p, &a)| (p, a)).collect());
            }
            *prev = at;
        };
        for t in t0..t1 {
            if anchor(t) {
                let at: Vec<u32> = places.iter().map(|(_, p)| p.map(t).unwrap_or(t)).collect();
                close(at.clone(), &mut prev, &mut holes);
                prev = at.iter().map(|a| a + 1).collect();
                shared += self.u[t as usize].toks;
            }
        }
        close(places.iter().map(|(o, _)| o.1).collect(), &mut prev, &mut holes);
        Group { places: places.into_iter().map(|(o, _)| o).collect(), holes, shared }
    }
}

/// A clone group: its places, first the template, and per hole each place's unit range.
struct Group {
    places: Vec<(u32, u32)>,
    holes: Vec<Vec<(u32, u32)>>,
    shared: u32,
}

fn clones(report: &mut Report) {
    let analysis = report.analysis();
    let cfg = &analysis.settings.lints.clones;
    if !cfg.enabled {
        return;
    }
    let sources = report.edits().sources;
    let main = std::path::Path::new(analysis.file_name(analysis.main_file));
    let frozen = crate::linter::Frozen::new(&analysis.settings.project, &crate::project::paths::dir_of(main));
    let bodies = verbatim::map(analysis, sources);
    let start = report.index().document_start();
    let mut lexer = Lexer::new(cfg);
    let mut texts: HashMap<FileId, Rc<Text>> = HashMap::default();
    for file in verbatim::files(analysis, sources) {
        let path = analysis.file_name(file.id);
        let file_path = std::path::Path::new(path);
        if crate::linter::written_by_run(path)
            || !crate::engine::loader::tex_source(file_path)
            || frozen.holds(file_path)
        {
            continue;
        }
        let kind = analysis.files.get(file.id as usize).map(|f| f.kind);
        let package =
            matches!(kind, Some(crate::engine::builtins::LoadKind::Package | crate::engine::builtins::LoadKind::Class));
        let from = start.filter(|s| s.file == file.id).map(|s| Pos::new(s.line, s.col));
        let cats = report.edits().cats(file.id);
        lexer.file(file.id, &file.text, cats, &bodies, |p| !package && from.is_none_or(|f| p >= f));
        texts.insert(file.id, file.text);
    }
    let Lexer { names, out: units, .. } = lexer;
    let search = Search::new(&units, &names, cfg);
    let mut taken: HashSet<String> = HashSet::default();
    for group in search.groups() {
        finding(report, &search, &group, &texts, start, &mut taken);
    }
    same_bodies(report);
}

impl<'a> Search<'a> {
    fn new(u: &'a [Unit], names: &'a Names, cfg: &'a Clones) -> Search<'a> {
        let mut toks = Vec::with_capacity(u.len() + 1);
        toks.push(0);
        for unit in u {
            toks.push(toks[toks.len() - 1] + unit.toks);
        }
        Search { u, names, cfg, toks }
    }
}

/// The clone groups of `source` read as one LaTeX file's body: each group's
/// places and holes, for tests and measurements without a run.
pub fn groups_in(source: &str, cfg: &Clones) -> Vec<(usize, usize)> {
    let text = Text::new(source.to_string());
    let mut lexer = Lexer::new(cfg);
    lexer.file(0, &text, CatcodeTable::latex(), &Bodies::default(), |_| true);
    let search = Search::new(&lexer.out, &lexer.names, cfg);
    search.groups().iter().map(|g| (g.places.len(), g.holes.len())).collect()
}

fn span(unit: &Unit) -> Span {
    Span { file: unit.file, line: unit.start.line, col: unit.start.col }
}

/// What a hole is the value of: the key before its `=`, else the nearest command before it.
fn label(search: &Search, at: u32, from: u32) -> String {
    let u = search.u;
    if at >= from + 2 && search.text(at - 1) == "=" && u[at as usize - 2].kind == Kind::Value {
        return format!("`{}`", search.text(at - 2));
    }
    // Back over whole groups and options, and out of the one the hole is in.
    let mut depth = 0;
    for i in (from..at).rev() {
        let unit = u[i as usize];
        match (unit.kind, search.text(i)) {
            (Kind::Close, _) | (Kind::Mark, "]") => depth += 1,
            (Kind::Open, _) | (Kind::Mark, "[") => depth = (depth - 1).max(0),
            (Kind::Cs, name) if depth == 0 => return format!("`{name}`"),
            _ => {}
        }
    }
    "a value".to_string()
}

/// A hole's source in one place: from the end of the unit before to the start of the unit after.
fn region(search: &Search, place: (u32, u32), hole: (u32, u32)) -> (FileId, Pos, Pos) {
    let u = search.u;
    let start = if hole.0 > place.0 { u[hole.0 as usize - 1].end } else { u[hole.0 as usize].start };
    let end = if hole.1 < place.1 { u[hole.1 as usize].start } else { u[hole.1 as usize - 1].end };
    (u[place.0 as usize].file, start, end.max(start))
}

fn finding(
    report: &mut Report,
    search: &Search,
    group: &Group,
    texts: &HashMap<FileId, Rc<Text>>,
    start: Option<Span>,
    taken: &mut HashSet<String>,
) {
    let u = search.u;
    let places = &group.places;
    let at = span(&u[places[0].0 as usize]);
    if !report.wanted(at) {
        return;
    }
    let slice = |(file, from, to): (FileId, Pos, Pos)| -> String {
        texts.get(&file).and_then(|t| t.slice(from, to)).unwrap_or_default().to_string()
    };
    let values: Vec<Vec<String>> = group
        .holes
        .iter()
        .map(|hole| {
            hole.iter().zip(places.iter()).map(|(&h, &p)| slice(region(search, p, h)).trim().to_string()).collect()
        })
        .collect();
    let shown = |v: &str| match v.chars().count() {
        0 => "nothing".to_string(),
        n if n > 24 => format!("{}…", v.chars().take(24).collect::<String>().replace('\n', " ")),
        _ => v.replace('\n', " "),
    };
    let described: Vec<String> = group
        .holes
        .iter()
        .zip(&values)
        .map(|(hole, vals)| {
            let vals: Vec<String> = vals.iter().map(|v| shown(v)).collect();
            format!("{} ({})", label(search, hole[0].0, places[0].0), vals.join(" / "))
        })
        .collect();
    let others: Vec<String> = places[1..].iter().map(|p| report.place(span(&u[p.0 as usize]))).collect();
    let message = match described.len() {
        0 => format!("this block repeats unchanged at {}", others.join(", ")),
        n => format!(
            "this block repeats at {} and differs only in {n} {}: {}",
            others.join(", "),
            if n == 1 { "place" } else { "places" },
            described.join(", ")
        ),
    };
    let name = search.text(places[0].0).to_string();
    let related: Vec<Span> = places[1..].iter().map(|p| span(&u[p.0 as usize])).collect();
    let fix = macro_fix(report, search, group, &values, texts, start, taken);
    report.add_fix_related(at, &name, message, fix, &related);
}

/// The fix: one `\newcommand` with a parameter per hole, used at every place.
fn macro_fix(
    report: &Report,
    search: &Search,
    group: &Group,
    values: &[Vec<String>],
    texts: &HashMap<FileId, Rc<Text>>,
    start: Option<Span>,
    taken: &mut HashSet<String>,
) -> Fix {
    let u = search.u;
    let places = &group.places;
    let n = group.holes.len();
    let most = search.cfg.max_parameters.min(9);
    if n > most {
        return Fix::new(
            format!("define one macro for the {} places: it would take {n} parameters, more than {most}", places.len()),
            Applicability::Unsafe,
            None,
        );
    }
    // The macro's name, from the block's first environment or command.
    let analysis = report.analysis();
    let base: String = (places[0].0..places[0].1)
        .map(|i| search.text(i))
        .find(|t| t.starts_with('\\'))
        .map(|t| t.trim_start_matches("\\begin{").chars().filter(char::is_ascii_alphabetic).collect())
        .filter(|b: &String| !b.is_empty())
        .unwrap_or_else(|| "repeated".to_string());
    let free = |name: &String| {
        !taken.contains(name) && analysis.interner.lookup(name).is_none_or(|s| !analysis.env.is_defined(s))
    };
    let name = std::iter::once(format!("{base}block"))
        .chain(('b'..='z').map(|c| format!("{base}block{c}")))
        .find(free)
        .unwrap_or_else(|| format!("{base}blockzz"));
    taken.insert(name.clone());
    let advice = format!("define `\\{name}` with {n} parameters and use it at all {} places", places.len());
    let none = || Fix::new(advice.clone(), Applicability::Unsafe, None);
    let slice = |(file, from, to): (FileId, Pos, Pos)| -> Option<String> {
        texts.get(&file).and_then(|t| t.slice(from, to)).map(str::to_string)
    };
    // A `#` belongs to a definition, a comment would swallow the brace after a value.
    let commented = |text: &str| text.lines().any(|l| crate::engine::tex::comment_start(l).is_some());
    let param = (places[0].0..places[0].1).any(|i| u[i as usize].kind == Kind::Param);
    let holes_ok = group.holes.iter().all(|hole| {
        hole.iter().zip(places.iter()).all(|(&h, &p)| {
            !(h.0..h.1).any(|i| u[i as usize].kind == Kind::Param)
                && slice(region(search, p, h)).is_some_and(|t| !commented(&t))
        })
    });
    if param || !holes_ok {
        return none();
    }
    // The body: the first place with each hole a parameter.
    let t = places[0];
    let file = u[t.0 as usize].file;
    let mut body = String::new();
    let mut from = u[t.0 as usize].start;
    for (k, hole) in group.holes.iter().enumerate() {
        let (_, start, end) = region(search, t, hole[0]);
        let Some(before) = slice((file, from, start)) else { return none() };
        let Some(inside) = slice((file, start, end)) else { return none() };
        let lead = &inside[..inside.len() - inside.trim_start().len()];
        let trail = if inside.trim().is_empty() { "" } else { &inside[inside.trim_end().len()..] };
        body.push_str(&before);
        body.push_str(&format!("{lead}#{}{trail}", k + 1));
        from = end;
    }
    let Some(rest) = slice((file, from, u[t.1 as usize - 1].end)) else { return none() };
    body.push_str(&rest);
    let arity = if n == 0 { String::new() } else { format!("[{n}]") };
    let definition = format!("\\newcommand{{\\{name}}}{arity}{{{body}}}\n");
    // Before `\begin{document}` when every place is in the body, else before the first place.
    let edits_of = report.edits();
    let at_start = start.filter(|_| places.iter().all(|p| u[p.0 as usize].body));
    let (def_file, def_at) = match at_start {
        Some(s) => (s.file, Pos::new(s.line, 1)),
        None => (file, Pos::new(u[t.0 as usize].start.line, 1)),
    };
    let Some((def_path, _)) = edits_of.text(def_file) else { return none() };
    let mut edits = Vec::new();
    let mut defined = false;
    for (i, &p) in places.iter().enumerate() {
        let unit = u[p.0 as usize];
        let Some((path, _)) = edits_of.text(unit.file) else { return none() };
        let args: String = values.iter().map(|v| format!("{{{}}}", v[i])).collect();
        let mut replacement = format!("\\{name}{}", if n == 0 { "{}" } else { &args });
        if path == def_path && unit.start == def_at {
            replacement.insert_str(0, &definition);
            defined = true;
        }
        edits.push(Edit { path, start: unit.start, end: u[p.1 as usize - 1].end, replacement });
    }
    if !defined {
        edits.push(Edit { path: def_path, start: def_at, end: def_at, replacement: definition });
    }
    Fix::new(advice, Applicability::Unsafe, Some(edits))
}

/// Two of the document's macros with the same body: the later one is defined through the earlier.
fn same_bodies(report: &mut Report) {
    let analysis = report.analysis();
    let index = report.index();
    let mut seen: HashMap<String, (crate::engine::tex::Sym, Span)> = HashMap::default();
    let mut names: HashSet<crate::engine::tex::Sym> = HashSet::default();
    let mut found = Vec::new();
    for def in &analysis.facts.defs {
        if def.tag != "macro" || !index.own_definition(def) || index.implements_sibling(def) || !names.insert(def.name)
        {
            continue;
        }
        let Some(mac) = def.mac.as_ref().filter(|m| !m.replacement_text.is_empty()) else { continue };
        // Written with `\def` and its kin, or with `\newcommand`'s, whose body a fix can rewrite.
        let by = analysis.interner.name(def.by);
        let command = match analysis.env.meaning(def.by).prim() {
            Some(crate::engine::builtins::Primitive::Def { .. }) => None,
            None if matches!(by, "newcommand" | "renewcommand" | "providecommand") => {
                report.edits().call_before(def.span, by)
            }
            _ => continue,
        };
        let toks: Vec<Tok> = mac.replacement_text.iter().map(|t| t.tok).collect();
        let key = format!("{:?}|{:?}|{}", mac.parameter_text, toks, mac.long);
        match seen.get(&key) {
            Some(&(first, at)) => found.push((def.span, def.name, first, at, mac.arity(), command)),
            None => drop(seen.insert(key, (def.name, def.span))),
        }
    }
    for (span, name, first, at, arity, command) in found {
        let (name, first) = (report.cs(name), report.cs(first));
        let call: String = std::iter::once(first.clone()).chain((1..=arity).map(|i| format!("{{#{i}}}"))).collect();
        let message = format!("`{name}` has the same body as `{first}`, defined at {}", report.place(at));
        let edits = command.and_then(|at| body_edit(&report.edits(), at, &call));
        let fix = Fix::new(format!("define `{name}` as `{call}`"), Applicability::Unsafe, edits);
        report.add_fix(span, &name, message, fix);
    }
}

/// The edit that makes the body of the `\newcommand` at `span` read `call`.
fn body_edit(edits: &crate::linter::fix::Edits, span: Span, call: &str) -> Option<Vec<Edit>> {
    let (path, text) = edits.text(span.file)?;
    let mut scan = Scan::at(&text, Pos::new(span.line, span.col), edits.cats(span.file))?;
    scan.advance()?;
    scan.star();
    scan.argument()?;
    scan.optional();
    if scan.optional().is_some() {
        return None;
    }
    let body = scan.group()?;
    Some(vec![Edit { path, start: body.inner_start, end: body.inner_end, replacement: call.to_string() }])
}
