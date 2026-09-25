//! What a command takes, found by running it: a sandbox copy of the machine
//! reads the command followed by probe characters, and whatever the main
//! loop does not get to typeset itself the command consumed.  Comparing runs
//! that offer `*`, `[P]` or neither in front of a row of `X`s tells a star,
//! an optional and a mandatory argument apart without knowing how the
//! command reads them (`\@ifstar`, `\@testopt`, ltcmd, a parameter text, a
//! primitive — all look the same from here).

use crate::config::{Config, Limits};
use crate::machine::{Analysis, Machine};
use std::rc::Rc;

use crate::tex::{ArgSpec, ArgType, FileId, Sym, Token};

/// Tokens one probe run may read: a sectioning command takes a few
/// thousand, or a million where hyperref's `\\GetTitleString` loops over
/// its title; a runaway loop is cut off here.
const PROBE_STEPS: u64 = 2_000_000;
/// And the time it may take, whatever it reads.
const PROBE_SECONDS: u64 = 5;
/// Mandatory arguments offered: TeX's parameter texts stop at nine (The
/// TeXbook, chapter 20), so reading all of these means reading on.
const TAIL: usize = 10;
/// One mandatory argument as a call writes it: a group reads the same to
/// an undelimited parameter, a verbatim reader and a box.
const ARG: &str = "{X}";
/// Arguments looked for, star and optional ones included.
const MAX_ITEMS: usize = 12;
/// Macro arguments one run remembers, for finding a default.
const MAX_BOUND: usize = 4096;

/// A probed signature: `complete` is false where the probe could not tell
/// what follows the items it found.
#[derive(Clone, Debug)]
pub struct Probed {
    pub spec: ArgSpec,
    pub complete: bool,
    /// The `\errmessage` the command raises when called with no optional
    /// arguments, as `\usepackage` does once the document has begun.
    pub error: Option<String>,
    /// The other forms a call may take where the item that picks one
    /// leads elsewhere: a star's own arguments (`\@ifstar\a\b`), the
    /// text read when an optional argument is left out (`\b` delimited).
    pub forms: Vec<ArgSpec>,
}

/// One macro argument a probe run bound: which probe characters it holds,
/// and its text when it holds none.
#[derive(Clone, Debug)]
pub struct Bound {
    pub mac: Sym,
    pub call: usize,
    pub param: usize,
    pub positions: Vec<usize>,
    /// `None` where the text cannot be typed.
    pub text: Option<String>,
}

/// What a sandbox run watches: the probe file, the first of its characters
/// the main loop took itself, and the macro arguments bound on the way.
/// What a parameter text wanted where the probe offered something else
/// (tex.web §§ 392, 398).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Want {
    /// Required text: `\def\a x#1{…}` called without the `x`.
    Literal,
    /// A delimiter the argument starting here runs to, which never came.
    Until,
    /// The `{` a `#{` parameter text ends with.
    Group,
}

/// The first place a probe run failed a parameter text: the offset of the
/// probe character, and the wanted text as it is written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Demand {
    pub at: usize,
    pub text: String,
    pub want: Want,
}

pub struct Watch {
    pub(crate) file: FileId,
    pub demand: Option<Demand>,
    /// Names `\let` or `\futurelet` to a probe character, by its offset.
    peeked: std::collections::HashMap<Sym, usize>,
    /// Characters a peeked probe character was compared with by `\ifx`:
    /// what the command looks for there (`\@ifnextchar`, ltcmd's `t`, `d`,
    /// `e`), by offset.
    pub looked_for: Vec<(usize, char)>,
    /// What TeX's scanners read a probe character as, by offset: a keyword
    /// (`to`, `plus`, `=`) or a quantity (`number`, `dimen`, `glue`).
    pub scanned: Vec<(usize, String)>,
    /// Probe characters some macro took as part of a delimited argument.
    pub delimited: Vec<usize>,
    /// How many errors the run raised.
    pub errors: usize,
    pub reached: Option<usize>,
    /// Reached inside a list the command opened (a box), not a group of
    /// the main loop's own.
    pub boxed: bool,
    pub exhausted: bool,
    pub error: Option<String>,
    raw: Vec<(Sym, usize, usize, Vec<Token>)>,
    calls: usize,
    pub bound: Vec<Bound>,
}

impl Watch {
    pub(crate) fn new(file: FileId) -> Self {
        Watch { file, demand: None, peeked: Default::default(), looked_for: Vec::new(), scanned: Vec::new(), delimited: Vec::new(), errors: 0, reached: None, boxed: false, exhausted: false, error: None, raw: Vec::new(), calls: 0, bound: Vec::new() }
    }

    pub(crate) fn peek(&mut self, name: Sym, source: Token) {
        match source.span.file == self.file {
            true => self.peeked.insert(name, source.span.col.saturating_sub(1) as usize),
            false => self.peeked.remove(&name),
        };
    }

    pub(crate) fn compared(&mut self, token: Token, other: &crate::tex::Meaning) {
        use crate::tex::{Catcode, Meaning, Tok};
        let at = match token.tok {
            Tok::Cs(sym) => self.peeked.get(&sym).copied(),
            Tok::Chr(..) if token.span.file == self.file => Some(token.span.col.saturating_sub(1) as usize),
            _ => None,
        };
        if let (Some(at), Meaning::Char(c, cat)) = (at, other)
            && !matches!(cat, Catcode::Begin | Catcode::End | Catcode::Space)
            && !self.looked_for.contains(&(at, *c))
        {
            self.looked_for.push((at, *c));
        }
    }

    pub(crate) fn bound(&mut self, mac: Sym, arguments: &[Vec<Token>], delimited: &[bool]) {
        self.calls += 1;
        for (param, argument) in arguments.iter().enumerate() {
            if delimited.get(param).copied().unwrap_or(false) {
                self.delimited.extend(
                    argument.iter().filter(|t| t.span.file == self.file).map(|t| t.span.col.saturating_sub(1) as usize),
                );
            }
            if self.raw.len() < MAX_BOUND {
                self.raw.push((mac, self.calls, param, argument.clone()));
            }
        }
    }

    /// `text` is `None` for tokens no call can write (a marker such as
    /// ltcmd's `-NoValue-`, whose first `-` is a letter).
    pub(crate) fn render(&mut self, text: impl Fn(&[Token]) -> Option<String>) {
        let file = self.file;
        self.bound = std::mem::take(&mut self.raw)
            .into_iter()
            .map(|(mac, call, param, toks)| {
                let positions: Vec<usize> = toks
                    .iter()
                    .filter(|t| t.span.file == file)
                    .map(|t| t.span.col.saturating_sub(1) as usize)
                    .collect();
                let text = if positions.is_empty() { text(&toks) } else { Some(String::new()) };
                Bound { mac, call, param, positions, text }
            })
            .collect();
    }
}

/// What `sym` takes where the run ended, or at `\begin{document}` with
/// `preamble`; computed once per name and view.
pub fn probe(analysis: &Analysis, sym: Sym, preamble: bool) -> Option<Probed> {
    if let Some(found) = analysis.probes.lock().ok().and_then(|p| p.get(&(sym, preamble)).cloned()) {
        return found;
    }
    let cfg = sandbox_config(&analysis.settings);
    let found = Prober { cfg: &cfg, analysis, sym, preamble, context: None, seen: Default::default() }.derive();
    if let Ok(mut probes) = analysis.probes.lock() {
        probes.insert((sym, preamble), found.clone());
    }
    found
}

/// Environments worth calling `sym` inside, generically: names the loaded
/// kernel, class or packages made an environment (both `\⟨name⟩` and
/// `\end⟨name⟩` defined) whose begin-code's own reach (the static call
/// graph, `record_calls`) overlaps what `sym`'s reach already touches —
/// never a table of environment names. Bounded in count for cost.
const MAX_CONTEXTS: usize = 16;
/// How far the static call graph is walked from a name, and how many
/// names that walk may collect, on both sides of the comparison.
const CLOSURE_DEPTH: usize = 8;
const CLOSURE_CAP: usize = 500;

/// The names reachable from `start` in `analysis.calls`, breadth first,
/// bounded by `CLOSURE_DEPTH`/`CLOSURE_CAP`.
pub(crate) fn closure(analysis: &Analysis, start: Sym) -> std::collections::HashSet<Sym> {
    closure_bounded(analysis, start, CLOSURE_DEPTH, CLOSURE_CAP)
}

/// [`closure`], with its own depth and count bound: a shallow walk finds
/// only a name's direct machinery (`itemize` calling `\list`), a deep one
/// its every last resort (`\@ifdefinable`, which nearly everything that
/// defines a command reaches eventually) — the two answer different
/// questions, so callers that want "the mechanism a definition site is
/// specific to" ask shallow.
pub(crate) fn closure_bounded(analysis: &Analysis, start: Sym, depth: usize, cap: usize) -> std::collections::HashSet<Sym> {
    let mut seen = std::collections::HashSet::new();
    seen.insert(start);
    let mut frontier = vec![start];
    for _ in 0..depth {
        if seen.len() >= cap || frontier.is_empty() {
            break;
        }
        let mut next = Vec::new();
        for sym in frontier {
            let Some(i) = analysis.calls.get(sym) else { continue };
            for &t in &analysis.calls.edges[i] {
                let callee = analysis.calls.nodes[t];
                if seen.insert(callee) {
                    next.push(callee);
                    if seen.len() >= cap {
                        break;
                    }
                }
            }
        }
        frontier = next;
    }
    seen
}

/// Bound on how many environments [`all_environments`] enumerates, for cost.
const MAX_ENVIRONMENTS: usize = 4096;

/// Every name the loaded kernel, class or packages made an environment:
/// both `\⟨name⟩` and `\end⟨name⟩` defined. What `explain` derives "which
/// environments reach this" from, generically, instead of a table of
/// environment names.
pub(crate) fn all_environments(analysis: &Analysis) -> Vec<Sym> {
    let mut found = Vec::new();
    for i in 0..analysis.interner.len() as u32 {
        if found.len() >= MAX_ENVIRONMENTS {
            break;
        }
        let env = crate::tex::Sym(i);
        if !matches!(analysis.env.meaning(env), crate::tex::Meaning::Macro(_)) {
            continue;
        }
        let name = analysis.interner.name(env);
        if name.is_empty() || name.starts_with("end") {
            continue;
        }
        let Some(end) = analysis.interner.lookup(&format!("end{name}")) else { continue };
        if analysis.env.is_defined(end) {
            found.push(env);
        }
    }
    found
}

fn candidate_environments(analysis: &Analysis, sym: Sym) -> Vec<Sym> {
    let reach = closure(analysis, sym);
    all_environments(analysis)
        .into_iter()
        .filter(|&env| env != sym && closure(analysis, env).intersection(&reach).next().is_some())
        .take(MAX_CONTEXTS)
        .collect()
}

/// What a bare call to `sym` does — nothing follows it, the way `\item`
/// alone in a document does — outside any environment when `context` is
/// `None`, or after really entering it there (`\begin{name}`, run to
/// completion, real kernel code): the `\errmessage` it reaches, if any.
/// This asks the same question a real use in the run answers (`raised`),
/// not what the command's signature is — a probe run that decides `\item`
/// takes an optional argument no longer counts as "the plain call took
/// nothing", the signature probe's own condition for reporting an error.
fn probe_bare_error(analysis: &Analysis, sym: Sym, context: Option<Sym>) -> Option<String> {
    let cfg = sandbox_config(&analysis.settings);
    let mut machine = Machine::sandbox(&cfg, analysis, false)?;
    if let Some(env) = context {
        enter_environment(&mut machine, env);
    }
    // A real `\item` alone in a document is never truly at EOF right after
    // it — something else always follows, even if only `\end{…}` — and a
    // lookahead such as `\@ifnextchar[` needs a real token to compare, not
    // an empty file, to take the branch a real call would. `.` is inert to
    // read as text and, in `catcode 12`, never a delimiter of its own.
    let watch = machine.run_probe(sym, ".");
    if watch.exhausted {
        return None;
    }
    watch.error
}

/// What calling `sym` alone raises outside any environment (`None`), and
/// inside each environment [`candidate_environments`] found worth trying —
/// the run's own kernel code, not a guess. Computed once per name and
/// cached.
pub fn probe_contexts(analysis: &Analysis, sym: Sym) -> Vec<(Option<Sym>, Option<String>)> {
    if let Some(found) = analysis.context_probes.lock().ok().and_then(|p| p.get(&sym).cloned()) {
        return found;
    }
    let mut found = vec![(None, probe_bare_error(analysis, sym, None))];
    found.extend(candidate_environments(analysis, sym).into_iter().map(|env| (Some(env), probe_bare_error(analysis, sym, Some(env)))));
    if let Ok(mut cache) = analysis.context_probes.lock() {
        cache.insert(sym, found.clone());
    }
    found
}

/// Really `\begin{env}` in `machine`, for its side effects only: what a
/// probe run afterwards sees. Reuses `run_probe`'s own plumbing — nothing
/// follows `{name}` in the file it reads, so the main loop just runs the
/// environment's begin-code to completion and then finds it at EOF.
fn enter_environment(machine: &mut Machine, env: Sym) {
    let Some(begin) = machine.out.interner.lookup("begin") else { return };
    let name = machine.out.interner.name(env).to_string();
    machine.run_probe(begin, &format!("{{{name}}}"));
}

fn sandbox_config(base: &Config) -> Config {
    Config {
        load_packages: false,
        load_classes: false,
        load_inputs: false,
        load_format: false,
        use_kpsewhich: false,
        cache: false,
        record_arguments: false,
        verbose: 0,
        trace: false,
        timings: false,
        limits: Limits { steps: PROBE_STEPS, seconds: PROBE_SECONDS, ..base.limits.clone() },
        ..base.clone()
    }
}

struct Prober<'c, 'a> {
    cfg: &'c Config,
    analysis: &'a Analysis,
    sym: Sym,
    preamble: bool,
    /// An environment to really `\begin` in the sandbox, real kernel code
    /// and all, before probing `sym`: what it takes and raises inside
    /// that environment instead of outside every one.
    context: Option<Sym>,
    /// Runs already made, by probe text.
    seen: std::cell::RefCell<std::collections::HashMap<String, Option<Rc<Watch>>>>,
}

impl Prober<'_, '_> {
    /// How many characters of `prefix` + `extra` + the `X`s the command
    /// consumed beyond `prefix`, and what the run bound; `None` when the
    /// run could not say.
    fn run(&self, prefix: &str, extra: &str) -> Option<(usize, Rc<Watch>)> {
        let text = format!("{prefix}{extra}{}", ARG.repeat(TAIL));
        let known = self.seen.borrow().get(&text).cloned();
        let watch = match known {
            Some(watch) => watch?,
            None => {
                let watch = Machine::sandbox(self.cfg, self.analysis, self.preamble)
                    .map(|mut machine| {
                        if let Some(env) = self.context {
                            enter_environment(&mut machine, env);
                        }
                        machine.run_probe(self.sym, &text)
                    })
                    .filter(|watch| !watch.exhausted)
                    .map(Rc::new);
                self.seen.borrow_mut().insert(text.clone(), watch.clone());
                watch?
            }
        };
        let consumed = watch.reached.unwrap_or(text.len());
        // An argument a parameter text wanted delimited, and did not get,
        // was not read (tex.web § 398).
        let consumed = watch.demand.as_ref().map_or(consumed, |d| consumed.min(d.at));
        // Stopped inside the prefix: the command handed what the probe
        // offered to the main loop (a box's contents), and read no further.
        Some((consumed.saturating_sub(prefix.len()), watch))
    }

    fn derive(&self) -> Option<Probed> {
        let (items, complete, error, forms) = self.derive_from(String::new(), Vec::new(), None, true);
        if items.is_empty() && !complete {
            return None;
        }
        // An error is the command's own answer only when it takes nothing:
        // one raised on the way through its arguments is about them.
        let error = error.filter(|_| items.is_empty());
        let raw = raw_of(&items);
        Some(Probed { spec: ArgSpec { items, raw }, complete, error, forms })
    }

    /// Items from `prefix` on, with `items` found before it; `bare` is where
    /// an optional argument is left out. With `branch`, forms an item leads
    /// to are derived as well.
    fn derive_from(
        &self,
        mut prefix: String,
        mut items: Vec<ArgType>,
        bare: Option<usize>,
        branch: bool,
    ) -> (Vec<ArgType>, bool, Option<String>, Vec<ArgSpec>) {
        let mut forms = Vec::new();
        let mut form = |prefix: String, items: Vec<ArgType>, bare: Option<usize>| {
            if branch {
                let (items, complete, _, _) = self.derive_from(prefix, items, bare, false);
                if complete {
                    let raw = raw_of(&items);
                    forms.push(ArgSpec { items, raw });
                }
            }
        };
        let mut complete = true;
        // A star is not given on the way on: the starred form of a command
        // may take other arguments than the one described (`\section*`).
        let mut star_seen = false;
        // A `#{` delimiter wants a group next, which is the probe's to offer.
        let mut group_next = false;
        let mut error = None;
        while items.len() < MAX_ITEMS {
            if std::mem::take(&mut group_next) {
                let Some((clipped, w)) = self.run(&prefix, ARG) else {
                    complete = false;
                    break;
                };
                // The `{` that ended the text before it is the one offered,
                // so the group it opens may still be read whole (xparse
                // `lm`) or by what it is handed to (`\def\a#1#{\hbox#1}`); only
                // that `{` read is it put back (`\def\a#1#{`).
                let n = match (&w.demand, w.reached) {
                    (Some(d), Some(reached))
                        if d.at == prefix.len()
                            && d.want == Want::Until
                            && d.text == "{"
                            && (reached >= prefix.len() + ARG.len() || (w.boxed && reached > prefix.len())) =>
                    {
                        reached - prefix.len()
                    }
                    _ => clipped,
                };
                // Put back and left to the caller, or read as an argument.
                if n == 0 {
                    break;
                }
                items.push(ArgType::Mandatory);
                prefix.push_str(ARG);
                star_seen = false;
                continue;
            }
            let Some((base, plain)) = self.run(&prefix, "") else {
                complete = false;
                break;
            };
            if prefix.is_empty() {
                error = plain.error.as_deref().and_then(|e| e.lines().next()).map(|e| e.trim().to_string());
            }
            // An empty text up to a `{` found earlier is an item already
            // taken; it wants nothing more.
            let demand = plain.demand.clone().filter(|d| !(d.at < prefix.len() && d.want == Want::Until && d.text == "{"));
            // What was offered so far is not what the command wants.
            if demand.as_ref().is_some_and(|d| d.at < prefix.len()) {
                complete = false;
                break;
            }
            // An item that is there only when asked for adds exactly itself
            // to what is consumed, and leaves the rest read as before: what
            // a parameter text wanted further on is wanted as far further on.
            // Or it leads elsewhere, where nothing is wanted before it ends
            // (`\@ifnextchar[\a\b`, `\b` delimited) — unless it is the
            // very text the plain call was refused for (`\def\a[#1]`).
            let at = prefix.len();
            let refused = |extra: &str| {
                plain.demand.as_ref().is_some_and(|d| {
                    d.at == at && d.want == Want::Literal && d.text.chars().next() == extra.chars().next()
                })
            };
            let adds = |extra: &str| {
                self.run(&prefix, extra)
                    .filter(|(n, w)| {
                        // Taken as text of a delimited argument, it is no item
                        // of its own (`\def\a#1#{…}` reads `*` as well).
                        !w.delimited.contains(&at)
                            && *n == base + extra.len()
                            && (same_demand(&plain, w, extra.len())
                                || (!refused(extra) && w.demand.as_ref().is_none_or(|d| d.at >= at + extra.len())))
                    })
                    .map(|(_, w)| w)
            };
            // A starred form may read other arguments than the plain one
            // (`\@ifstar\a\b`, `\a` delimited): the star is taken, and
            // what follows it is wanted delimited, where nothing was.
            let star_same = !star_seen && adds("*").is_some();
            // Or it reads other arguments than the plain form (`\@ifstar\a\b`,
            // `\a` taking two): the star was looked for, and taken.
            let star_own = !star_seen
                && !star_same
                && (plain.demand.is_none() || plain.looked_for.contains(&(at, '*')))
                && self.run(&prefix, "*").is_some_and(|(n, w)| {
                    n >= 1
                        && !w.delimited.contains(&at)
                        && (plain.looked_for.contains(&(at, '*'))
                            || w.demand.as_ref().is_some_and(|d| d.at == at + 1 && d.want == Want::Until))
                });
            let star = star_same || star_own;
            let optional = if bare == Some(at) { None } else { adds("[P]") };
            let optional_first = star && optional.is_some() && adds("[P]*").is_some() && adds("*[P]").is_none();
            if star && !optional_first {
                items.push(ArgType::Star);
                star_seen = true;
                if star_own {
                    form(format!("{prefix}*"), items.clone(), None);
                }
                continue;
            }
            // Characters the command looked for here, in the order it did:
            // an optional argument, a flag taken alone, an embellishment
            // followed by an argument, or the opening of a delimited
            // argument whose closing it then wants.
            let mut order: Vec<char> =
                plain.looked_for.iter().filter(|(p, c)| *p == at && *c != '*').map(|(_, c)| *c).collect();
            if !order.contains(&'[') {
                order.push('[');
            }
            let mut found = None;
            for c in order {
                if c == '[' {
                    if let Some(given) = &optional {
                        // Left out, the call reads on elsewhere (`\@ifnextchar[\a\b`,
                        // `\b` delimited): that is a form of its own.
                        if !same_demand(&plain, given, 3) {
                            form(prefix.clone(), items.clone(), Some(at));
                        }
                        found = Some((ArgType::Optional(default(given, &plain, at + 1)), "[P]".to_string()));
                        break;
                    }
                    continue;
                }
                let open = c.to_string();
                if adds(&open).is_some() {
                    found = Some((ArgType::TokenFlag(c), open));
                    break;
                }
                let embellished = format!("{c}{ARG}");
                if adds(&embellished).is_some() {
                    found = Some((ArgType::Embellishment(open), embellished));
                    break;
                }
                let close = self.run(&prefix, &open).and_then(|(_, w)| {
                    w.demand.clone().filter(|d| d.at == at + 1 && d.want == Want::Until).map(|d| d.text)
                });
                if let Some(close) = close {
                    let text = format!("{c}P{close}");
                    if let Some(given) = adds(&text) {
                        let required = plain.errors > given.errors;
                        let default = default(&given, &plain, at + 1);
                        if let [close] = close.chars().collect::<Vec<_>>()[..] {
                            found = Some((ArgType::Delimited { open: c, close, required, default }, text));
                            break;
                        }
                    }
                }
            }
            // A keyword a scanner tried here, with the quantity after it.
            let quantity = |w: &Watch, p: usize| {
                w.scanned.iter().find(|(q, k)| *q == p && QUANTITIES.contains(&k.as_str())).map(|(_, k)| k.clone())
            };
            // Those tried after a quantity began here are its own (units,
            // `true`); those before it are the command's (`=`, `to`).
            let keywords: Vec<String> = plain
                .scanned
                .iter()
                .filter(|(p, _)| *p == at)
                .take_while(|(_, k)| !QUANTITIES.contains(&k.as_str()))
                .filter(|(_, k)| {
                    !items.iter().any(|i| {
                        matches!(i, ArgType::Keyword { word, .. } if word.split('|').any(|w| w.split(' ').next() == Some(k.as_str())))
                    })
                })
                .map(|(_, k)| k.clone())
                .collect();
            for word in keywords {
                if found.is_some() {
                    break;
                }
                // A quantity read whether or not the keyword is there is not
                // the keyword's own (`\count0=1`, `\count0 1`).
                let valued = |word: &str| {
                    let spaced = format!("{word} ");
                    let value = match quantity(&plain, at) {
                        Some(_) => None,
                        None => self.run(&prefix, &spaced).and_then(|(_, w)| quantity(&w, at + spaced.len())),
                    };
                    let text = match &value {
                        Some(kind) => format!("{spaced}{} ", sample(kind)),
                        None => spaced,
                    };
                    (value, text)
                };
                let (value, text) = valued(&word);
                if adds(&text).is_some() {
                    // Keywords taken here are alternatives, each with its own
                    // quantity (`\font… at ⟨dimen⟩|scaled ⟨number⟩`).
                    let others: Vec<(String, Option<String>)> = plain
                        .scanned
                        .iter()
                        .filter(|(p, k)| *p == at && *k != word && !QUANTITIES.contains(&k.as_str()))
                        // Still looked for after it, it may follow instead
                        // (`plus … minus …`).
                        .filter(|(_, k)| {
                            self.run(&prefix, &text)
                                .is_none_or(|(_, w)| !w.scanned.contains(&(at + text.len(), k.clone())))
                        })
                        .filter_map(|(_, k)| {
                            let (v, t) = valued(k);
                            adds(&t).is_some().then(|| (k.clone(), v))
                        })
                        .collect();
                    let item = if others.iter().all(|(_, v)| *v == value) {
                        let word = std::iter::once(word.clone()).chain(others.into_iter().map(|(k, _)| k)).collect::<Vec<_>>().join("|");
                        ArgType::Keyword { word, value }
                    } else {
                        let shown = |k: &str, v: &Option<String>| match v {
                            Some(v) => format!("{k} ⟨{v}⟩"),
                            None => k.to_string(),
                        };
                        let word = std::iter::once(shown(&word, &value))
                            .chain(others.iter().map(|(k, v)| shown(k, v)))
                            .collect::<Vec<_>>()
                            .join("|");
                        ArgType::Keyword { word, value: None }
                    };
                    found = Some((item, text));
                }
            }
            if found.is_none()
                && demand.is_none()
                && let Some(kind) = quantity(&plain, at)
            {
                let text = format!("{} ", sample(&kind));
                found = Some((ArgType::Quantity(kind), text));
            }
            if let Some((item, text)) = found {
                items.push(item);
                prefix.push_str(&text);
                continue;
            }
            match demand {
                // Required text right here: TeX's parameter matching wanted
                // it where the probe offered an `X`.
                Some(d) if d.at == prefix.len() => {
                    if d.text.is_empty() && d.want != Want::Group {
                        complete = false;
                        break;
                    }
                    match d.want {
                        Want::Literal => {
                            prefix.push_str(&d.text);
                            items.push(ArgType::Literal(d.text));
                        }
                        // Text up to a `#{` is empty here: what the macro
                        // hands it to (`\hbox`, `\def`) wants no `X`.
                        Want::Until => {
                            if d.text == "{" {
                                group_next = true;
                            } else {
                                prefix.push('X');
                                prefix.push_str(&d.text);
                            }
                            items.push(ArgType::Until(d.text));
                        }
                        Want::Group => group_next = true,
                    }
                    star_seen = false;
                    continue;
                }
                // Wanted further on: what comes before it is read first.
                Some(_) => {}
                None if base == 0 => break,
                None if base >= TAIL * ARG.len() => {
                    items.push(ArgType::Mandatory);
                    complete = false;
                    break;
                }
                None => {}
            }
            items.push(ArgType::Mandatory);
            prefix.push_str(ARG);
            star_seen = false;
        }
        (items, complete, error, forms)
    }
}

/// What `Machine::probe_scans` calls the quantities it reads.
const QUANTITIES: [&str; 6] = ["number", "dimen", "glue", "mudimen", "muglue", "cs"];

/// A value of each quantity, as a call writes it.
fn sample(kind: &str) -> &'static str {
    match kind {
        "number" => "1",
        "mudimen" | "muglue" => "1mu",
        "cs" => "\\P",
        _ => "1pt",
    }
}

/// Whether `variant` wants what `plain` wants, `by` characters on.
fn same_demand(plain: &Watch, variant: &Watch, by: usize) -> bool {
    match (&plain.demand, &variant.demand) {
        (None, None) => true,
        (Some(a), Some(b)) => b.at == a.at + by && b.text == a.text && b.want == a.want,
        _ => false,
    }
}

/// Items as an xparse argument specification, with `u{…}` for text up to
/// a delimiter and required text as itself.
pub fn raw_of(items: &[ArgType]) -> String {
    let mut raw = String::new();
    for item in items {
        match item {
            ArgType::Mandatory => raw.push('m'),
            ArgType::Star => raw.push('s'),
            ArgType::Optional(None) => raw.push('o'),
            ArgType::Optional(Some(d)) => raw.push_str(&format!("O{{{d}}}")),
            ArgType::Until(d) if d == "{" => raw.push('l'),
            ArgType::Until(d) => raw.push_str(&format!("u{{{d}}}")),
            ArgType::Literal(text) => raw.push_str(text),
            ArgType::Keyword { word, value: Some(value) } => raw.push_str(&format!("[{word} ⟨{value}⟩]")),
            ArgType::Keyword { word, value: None } => raw.push_str(&format!("[{word}]")),
            ArgType::Quantity(value) => raw.push_str(&format!("⟨{value}⟩")),
            ArgType::TokenFlag(c) => raw.push_str(&format!("t{c}")),
            ArgType::Delimited { open, close, required, default } => {
                let letter = match (required, default) {
                    (true, None) => 'r',
                    (true, Some(_)) => 'R',
                    (false, None) => 'd',
                    (false, Some(_)) => 'D',
                };
                raw.push_str(&format!("{letter}{open}{close}"));
                if let Some(d) = default {
                    raw.push_str(&format!("{{{d}}}"));
                }
            }
            ArgType::Embellishment(e) => raw.push_str(&format!("e{{{e}}}")),
        }
    }
    raw
}

/// The default an optional argument has, when it shows: the argument that
/// holds the given `[P]` (of the last call that is handed what follows it
/// as well, when there is one: the call the arguments are delivered to)
/// holds text of its own when `[P]` is left out.  The macro must be called
/// as often in both runs, or they did not go the same way through it.
fn default(given: &Watch, plain: &Watch, at: usize) -> Option<String> {
    let calls = |w: &Watch, b: &Bound| w.bound.iter().filter(|c| c.mac == b.mac && c.param == b.param).count();
    let holds = |b: &Bound| b.positions == [at];
    let delivered = given.bound.iter().rposition(|b| {
        holds(b) && given.bound.iter().any(|c| c.call == b.call && c.positions.iter().any(|p| *p > at + 1))
    });
    let found = delivered.or_else(|| given.bound.iter().position(holds))?;
    let slot = &given.bound[found];
    if calls(given, slot) != calls(plain, slot) {
        return None;
    }
    let same = |b: &&Bound| b.mac == slot.mac && b.param == slot.param;
    let nth = given.bound[..found].iter().filter(same).count();
    let left_out = plain.bound.iter().filter(same).nth(nth)?;
    // No text to show: an empty default, or the marker ltcmd hands a
    // left-out `o` (`\NoValue` since the 2026-06-01 kernel).
    let text = left_out.text.as_deref()?;
    let marker = matches!(text.trim(), r"\NoValue" | r"\c_novalue_tl");
    (left_out.positions.is_empty() && !text.trim().is_empty() && !marker)
        .then(|| text.trim_end().to_string())
}
