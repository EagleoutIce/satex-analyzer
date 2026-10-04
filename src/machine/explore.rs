//! Undecided choices (SEMANTICS.md § 14).
//!
//! Every choice the run cannot decide goes through [`Machine::branch`]. A
//! main-loop command runs once per sequence of arms its choices allow,
//! rerun from where it began, so a choice inside a scan is as exact as one
//! the main loop reads. Each run is a path. Paths wait on a worklist and
//! join where they stand at the same place; none is dropped. A command
//! that branches is a loop head, whose joined state grows to a fixpoint,
//! widened after `widen_after` passes.

use super::*;
use crate::env::{Fork, PathEnv};
use crate::value::Num;

/// Runs of one command before further choices are left to the caller,
/// which widens them, as it does past `limits.choices`.
const RUNS: usize = 64;
/// Paths one place inside an expansion keeps apart when they differ in what
/// a join loses (a [`Tag`]).  Running on usually settles such a difference
/// (`\leavevmode` on one path, the next character on the other).
const PARTITIONS: usize = 2;

/// A choice the running command met: how many arms it has and which one
/// this run takes.
#[derive(Clone, Copy)]
pub(super) struct Choice {
    span: Span,
    by: Sym,
    arms: usize,
    arm: usize,
}

/// A path parked between two commands.
pub(super) struct Config {
    state: PathState,
    env: PathEnv,
    key: Key,
    tag: Tag,
    /// It reached `\end` or the end of its input.
    ended: bool,
}

/// Where a path stands, for joining: its input, bottom first and without
/// exhausted levels, its conditionals and its groups.
#[derive(PartialEq)]
struct Key {
    progress: u64,
    places: Vec<Place>,
    conds: Vec<(CondLimit, Span)>,
    groups: Vec<GroupKind>,
    scans: Vec<(usize, bool)>,
}

/// What a join of paths at one place loses: which mode, switches and
/// waiting tokens go together (trace partitioning).
#[derive(PartialEq)]
struct Tag {
    mode: crate::mode::Modes,
    catcodes: CatcodeTable,
    end_line: EndLineChar,
    after_assignment: Option<Tok>,
    streams: Streams,
    after: Vec<Vec<Tok>>,
}

impl Key {
    fn new(state: &PathState, groups: &[(GroupKind, Vec<Tok>)]) -> Key {
        let places = state.input.iter().filter_map(place).collect();
        Key {
            progress: state.source_progress,
            places,
            conds: state.conds.iter().map(|c| (c.limit, c.at)).collect(),
            groups: groups.iter().map(|g| g.0).collect(),
            scans: state.scans.iter().map(Accum::shape).collect(),
        }
    }
}

impl Tag {
    fn new(state: &PathState, groups: Vec<(GroupKind, Vec<Tok>)>) -> Tag {
        Tag {
            mode: state.mode,
            catcodes: state.catcodes.clone(),
            end_line: state.end_line,
            after_assignment: state.after_assignment.map(|t| t.tok),
            streams: state.streams.clone(),
            after: groups.into_iter().map(|g| g.1).collect(),
        }
    }
}

impl Config {
    fn new(state: PathState, env: PathEnv, ended: bool) -> Config {
        let groups = env.groups();
        let (key, tag) = (Key::new(&state, &groups), Tag::new(&state, groups));
        Config { state, env, key, tag, ended }
    }

    fn clone_config(&self) -> Config {
        Config::new(self.state.clone(), self.env.clone(), self.ended)
    }
}

/// What a join holds more of than the state it grew: names, and parts of
/// the state besides the bindings.
#[derive(Default)]
struct Growth {
    slots: Vec<Sym>,
    parts: Vec<&'static str>,
}

impl Growth {
    fn any(&self) -> bool {
        !self.slots.is_empty() || !self.parts.is_empty()
    }
}

/// A command that branched: the join of every state a path began it in.
struct Head {
    span: Span,
    progress: u64,
    config: Config,
    passes: u32,
}

/// The paths of one [`Machine::drive`].
#[derive(Default)]
struct Paths {
    /// Where they parted: their bindings are kept relative to it.
    fork: Option<Fork>,
    parked: Vec<Config>,
    /// Paths that left what the drive runs, waiting for the others.
    done: Vec<Config>,
    heads: Vec<Head>,
    /// Recursions widened past `recursion_depth`, each the join of every
    /// path cut there: one that adds nothing is covered and stops.
    cut: Vec<(Span, Config)>,
    /// How many paths waited after the last [`Machine::crowd`]: it is only
    /// tried again once more wait.
    crowded: usize,
    /// The drive of the whole run: join points and dropped paths apply.
    top: bool,
    /// Paths at a join point ([`Machine::join_arrived`]), waiting for the rest.
    arrived: Vec<Config>,
    /// Paths dropped for a certain error (`analysis.assume_compiles`), with
    /// where: kept until the others are done. If one of those ended in an
    /// error too, no path compiles and these run on ([`Machine::settle`]).
    shelved: Vec<Shelved>,
    /// A path went on with a certain error: nothing else compiled.
    failed: bool,
}

/// A dropped path: its error, where, and the diagnostics' counts the
/// command made (rolled back with it, given back when it runs on).
struct Shelved {
    config: Config,
    error: String,
    at: Span,
    counts: Vec<(usize, u32)>,
}

/// What one command left.
enum Next {
    /// One path, the running one.
    One,
    /// One path, which ended.
    Ended,
    /// Several, all parked.
    Many(Vec<Config>),
    /// None: every arm was absent, or the state is one a head holds already.
    None,
}

impl Machine<'_> {
    /// The arm a choice the run cannot decide takes: every one of `arms` is
    /// taken, each on a path of its own, by running the command again.
    /// `None` when the command may not make more choices: the caller leaves
    /// this one undecided another way.
    pub(crate) fn branch(&mut self, by: Sym, arms: usize, span: Span) -> Option<usize> {
        if arms < 2 {
            return (arms == 1).then_some(0);
        }
        self.undecided_at = self.out.steps;
        let i = self.choices.len();
        let scripted = i < self.script.len();
        // Out of memory, no new path starts: the run goes on undecided.
        if !self.replayable
            || i >= usize::from(self.cfg.limits.choices)
            || (!scripted && (self.runs >= RUNS || crate::budget::pressed()))
        {
            return None;
        }
        let arm = self.script.get(i).copied().unwrap_or(0);
        self.out.heat(|h| &mut h.forks, span);
        self.choices.push(Choice { span, by, arms, arm });
        Some(arm)
    }

    /// The arm [`Machine::branch`] gave does not exist, nor any after it:
    /// the path is no run of the document.
    pub(crate) fn no_arm(&mut self) {
        self.infeasible = true;
        self.halted = true;
    }

    /// Whether the run may be on one of several paths: what it does now
    /// need not happen in every run.
    pub(crate) fn on_paths(&self) -> bool {
        self.parted || self.env.forking() || !self.choices.is_empty()
    }

    /// Run commands on every path until `done` holds for all of them or
    /// they end.  Paths that reach `done` are joined and go on as one.
    pub(super) fn drive(&mut self, unit: Unit, done: impl Fn(&Self) -> bool) {
        let mut outer =
            (std::mem::take(&mut self.script), std::mem::take(&mut self.choices), self.runs, self.replayable);
        let outer_unit = (std::mem::replace(&mut self.unit, unit), self.lone);
        // Inside a command, whose run again undoes what this one counts.
        let nested = self.replayable;
        self.replayable = true;
        let mut paths = Paths { top: !nested && matches!(unit, Unit::Command), ..Paths::default() };
        let (mut commands, mut ended) = (0u64, false);
        while !self.out.exhausted {
            if !nested {
                self.undo.clear();
            }
            let finished = ended || self.halted || done(self);
            if finished
                && !paths.shelved.is_empty()
                && paths.parked.is_empty()
                && paths.arrived.is_empty()
                && paths.done.is_empty()
            {
                self.settle(&mut paths, ended);
            }
            self.lone = paths.fork.is_none();
            if self.lone && paths.top {
                self.approximated = false;
            }
            let next = if finished { None } else { Some(self.explore(&mut paths, true)) };
            self.waiting = paths.parked.len() + paths.done.len();
            self.report_progress();
            self.report_paths(&paths, commands);
            commands += 1;
            let live = match next {
                Some(Next::One) if self.at_join && paths.top && paths.fork.is_some() => {
                    self.at_join = false;
                    let env = self.env.path(paths.fork.as_ref().expect("checked"));
                    paths.arrived.push(Config::new(self.save_path(), env, false));
                    false
                }
                Some(Next::One) => {
                    self.at_join = false;
                    true
                }
                Some(Next::Ended) => {
                    ended = true;
                    true
                }
                Some(Next::Many(outcomes)) => {
                    let fork = paths.fork.clone().expect("paths parted");
                    for mut outcome in outcomes {
                        match std::mem::take(&mut outcome.state.at_join) && paths.top {
                            true => paths.arrived.push(outcome),
                            false => self.park(&mut paths, &fork, outcome),
                        }
                    }
                    false
                }
                Some(Next::None) => false,
                None if paths.fork.is_none() => break,
                None => {
                    self.parted |= ended || self.halted;
                    if !ended && !self.halted {
                        let fork = paths.fork.as_ref().expect("checked above");
                        let env = self.env.path(fork);
                        paths.done.push(Config::new(self.save_path(), env, ended));
                    }
                    false
                }
            };
            let Some(fork) = paths.fork.take() else { continue };
            if live && !ended {
                self.close_extra_groups(&mut paths, &fork);
                self.meet(&mut paths, &fork);
            }
            if paths.parked.len() > self.path_bound().max(paths.crowded) {
                self.crowd(&mut paths, &fork);
            }
            if let Some(i) = self.behind(&paths, live) {
                if live {
                    let env = self.env.path(&fork);
                    paths.parked.push(Config::new(self.save_path(), env, ended));
                }
                let path = paths.parked.swap_remove(i);
                ended = path.ended;
                self.resume_config(&fork, path);
            } else if !live && !paths.arrived.is_empty() {
                self.join_arrived(&mut paths, &fork);
            } else if !live {
                let configs = std::mem::take(&mut paths.done);
                if configs.is_empty() {
                    self.halted = true;
                    self.env.unfork(fork);
                    break;
                }
                let joined = self.join_done(&fork, &configs);
                // Paths that end apart (other conditionals open, text that
                // cannot be one) are the enclosing command's choice.
                let joined = match joined {
                    Some(joined) => joined,
                    None => {
                        let span = self.position_span();
                        self.swap_choices(&mut outer);
                        let arm = self.branch(self.unknown, configs.len(), span);
                        self.swap_choices(&mut outer);
                        if arm.is_none() {
                            self.diagnose(
                                Severity::Unsupported,
                                "unjoined-lookahead",
                                span,
                                "paths a look ahead parted into end apart past the choices one command may make; the first is followed".into(),
                            );
                        }
                        // A replayed arm past the configs this run ended with: no run.
                        let Some(config) = configs.into_iter().nth(arm.unwrap_or(0)) else {
                            self.no_arm();
                            self.env.unfork(fork);
                            break;
                        };
                        config
                    }
                };
                ended = false;
                self.resume_config(&fork, joined);
            }
            if paths.parked.is_empty() && paths.done.is_empty() && paths.arrived.is_empty() && paths.shelved.is_empty()
            {
                self.env.unfork(fork);
                paths.heads.clear();
            } else if paths.parked.is_empty() && paths.done.is_empty() && paths.arrived.is_empty() {
                // Only paths dropped for an error wait. They and the
                // recursion summaries move up to where the running path
                // stands, so its later paths part from there, not from the
                // error's command: a join compares what changed since its
                // fork, else everything the document did since.
                let shelved = paths.shelved.iter_mut().map(|s| &mut s.config.env);
                let mut envs: Vec<&mut PathEnv> = shelved.chain(paths.cut.iter_mut().map(|c| &mut c.1.env)).collect();
                if self.env.rebase(&fork, &mut envs) {
                    self.env.unfork(fork);
                    let mut fresh = self.env.fork();
                    self.env.part(&mut fresh);
                    paths.heads.clear();
                    paths.fork = Some(fresh);
                } else {
                    paths.fork = Some(fork);
                }
            } else {
                paths.fork = Some(fork);
            }
        }
        if let Some(fork) = paths.fork.take() {
            self.env.unfork(fork);
        }
        self.keep_errors = false;
        (self.script, self.choices, self.runs, self.replayable) = outer;
        (self.unit, self.lone) = outer_unit;
    }

    /// Every other path is done and the running one is finished. If a path
    /// went on with a certain error, nothing compiles: the dropped paths
    /// run on, errors kept. Else they were no run of the document.
    fn settle(&mut self, paths: &mut Paths, ended: bool) {
        let shelved = std::mem::take(&mut paths.shelved);
        if !paths.failed {
            for s in shelved {
                self.report_dropped(&s.error, s.at);
            }
            return;
        }
        self.keep_errors = true;
        let fork = paths.fork.clone().expect("shelved paths keep the fork");
        if ended && !self.halted {
            let env = self.env.path(&fork);
            paths.parked.push(Config::new(self.save_path(), env, true));
        }
        for s in shelved {
            for (i, delta) in s.counts {
                let d = &mut self.out.facts.diagnostics[i];
                d.count = d.count.saturating_add(delta);
            }
            paths.parked.push(s.config);
        }
    }

    /// By how much the running command raised the counts of diagnostics
    /// seen before, from `undo[mark..]`.
    fn made_counts(&self, mark: usize) -> Vec<(usize, u32)> {
        let mut counts: Vec<(usize, u32)> = Vec::new();
        for u in &self.undo[mark..] {
            if let Undo::Count(i, old) = u
                && !counts.iter().any(|c| c.0 == *i)
            {
                counts.push((*i, self.out.facts.diagnostics[*i].count.saturating_sub(*old)));
            }
        }
        counts
    }

    /// Trade the running drive's choices for the enclosing command's.
    fn swap_choices(&mut self, outer: &mut (Vec<usize>, Vec<Choice>, usize, bool)) {
        std::mem::swap(&mut self.script, &mut outer.0);
        std::mem::swap(&mut self.choices, &mut outer.1);
        std::mem::swap(&mut self.runs, &mut outer.2);
        std::mem::swap(&mut self.replayable, &mut outer.3);
    }

    /// Paths that left what the drive runs, joined. Their input joins as
    /// one text ([`Machine::widen_input`]): a path that stopped elsewhere
    /// still reads what it has left. A look ahead ([`Unit::Head`]) also
    /// needs the same conditionals, groups and scans. `None` when they
    /// cannot be one.
    fn join_done(&mut self, fork: &Fork, configs: &[Config]) -> Option<Config> {
        let first = configs.first()?;
        let span = self.position_span();
        let mut joined = first.clone_config();
        for b in &configs[1..] {
            let (x, y) = (&joined.key, &b.key);
            if matches!(self.unit, Unit::Head | Unit::Number)
                && (x.conds != y.conds || x.groups != y.groups || x.scans != y.scans)
            {
                return None;
            }
            let input = match x.places == y.places {
                true => joined.state.input.clone(),
                false => self.widen_input(&joined.state.input, &b.state.input, span, false)?,
            };
            let (j, _) = self.join_configs(fork, &joined, b, false);
            joined = Config::new(PathState { input, ..j.state }, j.env, j.ended);
        }
        Some(joined)
    }

    /// Run one command on the running path, once for every sequence of
    /// arms its choices allow.  Without `head` it is not checked against
    /// the loop heads: it runs from one.
    fn explore(&mut self, paths: &mut Paths, head: bool) -> Next {
        let start = self.save_path();
        let mut fork = self.env.fork();
        let own = paths.fork.is_none();
        let counters = (self.undo.len(), self.env.assignments, self.gaps_raised, self.gap_depth, self.tex_errors);
        self.script.clear();
        self.runs = 0;
        let mut outcomes = Vec::new();
        let mut first = None;
        let mut dropped: Vec<(Config, String, Vec<(usize, u32)>)> = Vec::new();
        loop {
            self.choices.clear();
            self.infeasible = false;
            self.at_join = false;
            self.dropping = None;
            let going = match self.unit {
                Unit::Command => self.command(),
                unit => self.scan_step(unit),
            };
            self.runs += 1;
            first = first.or(self.choices.first().copied());
            let next = next_script(&self.choices, self.infeasible);
            let error = self.dropping.take().filter(|_| paths.top && !self.infeasible);
            if outcomes.is_empty() && dropped.is_empty() && next.is_none() {
                self.choices.clear();
                // Other paths wait: this one raised a certain error.
                let others = !paths.parked.is_empty() || !paths.arrived.is_empty();
                paths.failed |= error.is_some() && !others;
                if let Some(error) = error.filter(|_| others) {
                    let base = paths.fork.as_ref().expect("others wait");
                    let (state, env, at) = (self.save_path(), self.env.path(base), self.position_span());
                    let counts = self.made_counts(counters.0);
                    paths.shelved.push(Shelved { config: Config::new(state, env, !going), error, at, counts });
                    self.restore_path(start);
                    self.env.rewind(&fork);
                    self.env.unfork(fork);
                    self.undo_counters(counters);
                    return Next::None;
                }
                self.env.unfork(fork);
                return match (self.infeasible, going) {
                    (true, _) => Next::None,
                    (false, true) => Next::One,
                    (false, false) => Next::Ended,
                };
            }
            if paths.fork.is_none() {
                self.env.part(&mut fork);
                paths.fork = Some(fork.clone());
            }
            if !self.infeasible {
                let base = paths.fork.as_ref().expect("set above");
                let (state, env) = (self.save_path(), self.env.path(base));
                let config = Config::new(state, env, !going);
                match error {
                    Some(error) => dropped.push((config, error, self.made_counts(counters.0))),
                    None => outcomes.push(config),
                }
            }
            self.restore_path(start.clone());
            self.env.rewind(&fork);
            self.undo_counters(counters);
            match next {
                Some(script) => self.script = script,
                None => break,
            }
        }
        self.choices.clear();
        self.script.clear();
        if !own {
            self.env.unfork(fork);
        }
        // The last path is kept, error and all: the document has one.
        if outcomes.is_empty() && paths.parked.is_empty() && paths.arrived.is_empty() {
            paths.failed |= !dropped.is_empty();
            outcomes.extend(dropped.drain(..).map(|d| d.0));
        }
        for (config, error, counts) in dropped {
            let at = self.position_span();
            paths.shelved.push(Shelved { config, error, at, counts });
        }
        if outcomes.is_empty() {
            return Next::None;
        }
        match first {
            Some(choice) if head => self.loop_head(paths, choice, start, outcomes),
            _ => Next::Many(outcomes),
        }
    }

    /// A command that branched, begun in `start`, is a loop head. Met again
    /// in the same place with nothing read since, its state joins the
    /// head's, and paths go on from the join only when it grew. Met deeper
    /// in the input, it is a recursion, cut after as many levels as
    /// widening waits for.
    fn loop_head(&mut self, paths: &mut Paths, choice: Choice, start: PathState, outcomes: Vec<Config>) -> Next {
        let Choice { span, by, .. } = choice;
        self.out.heat(|h| &mut h.heads, span);
        let fork = paths.fork.clone().expect("paths parted");
        let progress = start.source_progress;
        let here = Config::new(start, self.env.path(&fork), false);
        let same = |h: &Head| h.span == span && h.progress == progress;
        let widen_after = u32::from(self.limits.widen_after);
        let exact = paths.heads.iter().position(|h| same(h) && h.config.key == here.key);
        // A head at the same place whose input differs only in the digits
        // of its token lists (a loop whose counter is in its argument,
        // expl3's `\__flag_height_loop:wN`): the same head, the counter
        // read as unknown digits ([`widen_input`]). Without this every
        // pass is a new head and none ever meets. Texts that differ in
        // more (pgfmath's parser, its expression shrinking) are new heads,
        // bounded by `recursion_depth` as before.
        let syms = self.typed_syms();
        let typed = crate::env::Typed {
            interner: &self.out.interner,
            syms: &syms,
            digits: self.unknown_digits,
            more: self.unknown_more,
        };
        let alike = exact
            .is_none()
            .then(|| paths.heads.iter().position(|h| same(h) && alike_places(&h.config.key, &here.key, &typed)))
            .flatten();
        // Heads at one place whose token lists differ in more than digits
        // (an argument growing every pass): past `widen_after` of them, the
        // newest is the same head, its input widened.
        let kin = |h: &Head| {
            let (a, b) = (&h.config.key, &here.key);
            let text = |p: &Place| matches!(p, Place::Tokens(..) | Place::Returned(..));
            same(h)
                && (&a.conds, &a.groups, &a.scans) == (&b.conds, &b.groups, &b.scans)
                && a.places.len() == b.places.len()
                && a.places.iter().zip(&b.places).all(|(x, y)| x == y || (text(x) && text(y)))
        };
        let alike = alike.or_else(|| {
            let kins = exact.is_none().then(|| paths.heads.iter().filter(|h| kin(h)).count()).unwrap_or(0);
            (kins > usize::from(self.limits.widen_after)).then(|| paths.heads.iter().rposition(kin)).flatten()
        });
        // Whether the head's input grew by the widening: what it reads
        // next is then not covered by what it ran with.
        let mut input_grew = false;
        let (h, here) = match (exact, alike) {
            (Some(h), _) => (h, here),
            (None, Some(h)) => {
                let widen = paths.heads[h].passes >= widen_after;
                match self.widen_input(&paths.heads[h].config.state.input, &here.state.input, span, widen) {
                    Some(input) => {
                        let head = &mut paths.heads[h];
                        let widened = Config::new(
                            PathState { input: input.clone(), ..head.config.state.clone() },
                            head.config.env.clone(),
                            false,
                        );
                        input_grew = widened.key.places != head.config.key.places;
                        head.config = widened;
                        (h, Config::new(PathState { input, ..here.state }, here.env, false))
                    }
                    None => (usize::MAX, here),
                }
            }
            (None, None) => (usize::MAX, here),
        };
        if h == usize::MAX {
            // Met again inside its own input, it is a recursion: every
            // frame below the one it ran from is still there.
            let deeper = |h: &Head| {
                let below = &h.config.key.places[..h.config.key.places.len().saturating_sub(1)];
                same(h) && h.config.key.places.len() < here.key.places.len() && here.key.places.starts_with(below)
            };
            if let Some(level) = paths
                .heads
                .iter()
                .filter(|h| deeper(h))
                .nth(usize::from(self.limits.recursion_depth))
                .and_then(|_| paths.heads.iter().rposition(deeper))
            {
                // Widened: the deepest level's state joins this one, and
                // the call it made, with every level below it, expands to
                // unknown text.
                self.diagnose(
                    Severity::Imprecision,
                    "recursion-widened",
                    span,
                    format!(
                        "\\{} decides a recursion and could not be decided; its deeper levels are widened",
                        self.name(by)
                    ),
                );
                if self.cfg.verbose >= 4 {
                    let places: Vec<String> = here.key.places.iter().map(|p| self.describe_place(p)).collect();
                    crate::progress::note!("satex: recursion widened at {span:?}, input: {places:#?}");
                }
                let depth = paths.heads[level].config.key.places.len();
                let (joined, _) = self.join_configs(&fork, &paths.heads[level].config, &here, true);
                self.resume_config(&fork, Config::new(here.state, joined.env, false));
                while self.input.iter().filter_map(place).count() > depth {
                    self.pop_frame();
                }
                let unknown = self.unknown_token(span);
                self.summarized.insert(span);
                self.push_tokens(Rc::from(vec![unknown]), None);
                // Every path cut here joins one summary, widened, so the
                // cuts reach a fixpoint instead of each running on.
                let cut = Config::new(self.save_path(), self.env.path(&fork), false);
                let typed = crate::env::Typed {
                    interner: &self.out.interner,
                    syms: &syms,
                    digits: self.unknown_digits,
                    more: self.unknown_more,
                };
                let met = paths
                    .cut
                    .iter()
                    .position(|(at, c)| *at == span && (c.key == cut.key || alike_places(&c.key, &cut.key, &typed)));
                let Some(i) = met else {
                    paths.cut.push((span, cut));
                    return self.explore(paths, false);
                };
                let (mut cut, mut grew) = (cut, false);
                if paths.cut[i].1.key != cut.key {
                    let Some(input) = self.widen_input(&paths.cut[i].1.state.input, &cut.state.input, span, true)
                    else {
                        paths.cut.push((span, cut));
                        return self.explore(paths, false);
                    };
                    let old = &paths.cut[i].1;
                    let summary =
                        Config::new(PathState { input: input.clone(), ..old.state.clone() }, old.env.clone(), false);
                    grew = summary.key.places != old.key.places;
                    paths.cut[i].1 = summary;
                    cut = Config::new(PathState { input, ..cut.state }, cut.env, false);
                }
                let (joined, growth) = self.join_configs(&fork, &paths.cut[i].1, &cut, true);
                let growth =
                    Growth { parts: [growth.parts, if grew { vec!["input"] } else { vec![] }].concat(), ..growth };
                if !growth.any() {
                    return Next::None;
                }
                paths.cut[i].1 = Config::new(joined.state.clone(), joined.env.clone(), false);
                self.resume_config(&fork, joined);
                return self.explore(paths, false);
            }
            // A head every path read on past is left behind: nothing meets
            // it again. Without reading source (a hook's code) progress
            // alone never retires one.
            let typed = crate::env::Typed {
                interner: &self.out.interner,
                syms: &syms,
                digits: self.unknown_digits,
                more: self.unknown_more,
            };
            let live = |h: &Head| {
                let on = |key: &Key| !left(&h.config.key, key, &typed);
                on(&here.key) || paths.parked.iter().chain(&paths.arrived).any(|p| on(&p.key))
            };
            paths.heads.retain(|h| h.progress >= progress && live(h));
            paths.heads.push(Head { span, progress, config: here, passes: 0 });
            self.out.loop_heads = self.out.loop_heads.max(paths.heads.len());
            return Next::Many(outcomes);
        }
        let widen = paths.heads[h].passes >= widen_after;
        let (joined, mut growth) = self.join_configs(&fork, &paths.heads[h].config, &here, widen);
        if input_grew {
            growth.parts.push("input");
        }
        if !growth.any() {
            return Next::None;
        }
        if self.cfg.verbose >= 4 && widen {
            let names: Vec<&str> = growth.slots.iter().take(8).map(|&sym| self.name(sym)).collect();
            crate::progress::note!(
                "satex: the loop at {span:?} still grows when widened: {names:?} {:?}",
                growth.parts
            );
        }
        let head = &mut paths.heads[h];
        head.passes += 1;
        head.config = Config::new(joined.state.clone(), joined.env.clone(), false);
        let widened = if widen { ", widened" } else { "" };
        self.diagnose(
            Severity::Imprecision,
            "undecided-loop",
            span,
            format!(
                "\\{} decides a loop and could not be decided; every number of passes is joined{widened}",
                self.name(by)
            ),
        );
        self.resume_config(&fork, joined);
        self.explore(paths, false)
    }

    /// `analysis.assume_compiles`: a path that raised a certain error was dropped.
    fn report_dropped(&mut self, error: &str, span: Span) {
        self.diagnose(
            Severity::Assumption,
            "assumed-compiles",
            span,
            format!("a path raising {error} was dropped: the document is assumed to compile"),
        );
    }

    /// Every path is at a join point (`analysis.join_paths_at`): those that
    /// stand in the same conditionals, groups and scans, and whose input can
    /// be one, are joined. The first runs on, the rest wait as before.
    fn join_arrived(&mut self, paths: &mut Paths, fork: &Fork) {
        let arrived = std::mem::take(&mut paths.arrived);
        let count = arrived.len();
        let span = self.position_span();
        let mut groups: Vec<Config> = Vec::new();
        let (mut slots, mut parts): (Vec<Sym>, Vec<&'static str>) = (Vec::new(), Vec::new());
        for b in arrived {
            let mut rest = Some(b);
            for a in &mut groups {
                let b = rest.take().expect("still there");
                let same = a.key.conds == b.key.conds && a.key.groups == b.key.groups && a.key.scans == b.key.scans;
                let input = same.then(|| self.widen_input(&a.state.input, &b.state.input, span, false)).flatten();
                let Some(input) = input else {
                    rest = Some(b);
                    continue;
                };
                if a.key.places != b.key.places {
                    parts.push("input");
                }
                for (part, apart) in [("mode", a.tag.mode != b.tag.mode), ("aftergroup", a.tag.after != b.tag.after)] {
                    if apart {
                        parts.push(part);
                    }
                }
                let (joined, growth) = self.join_configs(fork, a, &b, false);
                slots.extend(growth.slots);
                parts.extend(growth.parts);
                *a = Config::new(PathState { input, ..joined.state }, joined.env, joined.ended);
                break;
            }
            groups.extend(rest);
        }
        slots.sort_by_key(|s| s.0);
        slots.dedup();
        parts.sort_unstable();
        parts.dedup();
        let mut groups = groups.into_iter();
        let first = groups.next().expect("a path arrived");
        let left = groups.len();
        paths.parked.extend(groups);
        let mut names: Vec<String> = slots.iter().take(8).map(|&s| format!("\\{}", self.name(s))).collect();
        if slots.len() > 8 {
            names.push(format!("{} more", slots.len() - 8));
        }
        names.extend(parts.iter().map(|p| p.to_string()));
        let apart = if names.is_empty() { "nothing".to_string() } else { names.join(", ") };
        let kept = if left == 0 { String::new() } else { format!("; {} stay apart", left + 1) };
        if count > 1 {
            self.diagnose(
                Severity::Imprecision,
                "paths-joined-at",
                span,
                format!("{count} paths arrived at a join point and were joined{kept}; they differed in {apart}"),
            );
        }
        self.resume_config(fork, first);
    }

    /// The running path joins a parked one at its place ([`partner`]).
    fn meet(&mut self, paths: &mut Paths, fork: &Fork) {
        while paths.parked.iter().any(|p| p.key.progress == self.source_progress) {
            let groups = self.env.groups();
            let state = self.save_path();
            let key = Key::new(&state, &groups);
            let tag = Tag::new(&state, groups);
            let Some(i) = partner(&paths.parked, &key, &tag) else { return };
            let live = Config { key, tag, env: self.env.path(fork), state, ended: false };
            let other = paths.parked.swap_remove(i);
            let (joined, _) = self.join_configs(fork, &live, &other, false);
            self.resume_config(fork, joined);
        }
    }

    /// Assumed, as of a package that is done: it closed the groups it opened.
    /// Between two lines of a file, a path that stands where another does,
    /// in the same conditionals but in more groups, has its extra groups
    /// closed, so the two can join.
    fn close_extra_groups(&mut self, paths: &mut Paths, fork: &Fork) {
        let places: Vec<Place> = self.input.iter().filter_map(place).collect();
        if !places.iter().all(|p| matches!(p, Place::File(..) | Place::Pseudo(..))) {
            return;
        }
        let here: Vec<GroupKind> = self.env.groups().into_iter().map(|g| g.0).collect();
        let conds: Vec<(CondLimit, Span)> = self.conds.iter().map(|c| (c.limit, c.at)).collect();
        let Some(i) = paths.parked.iter().position(|p| {
            let k = &p.key;
            let n = k.groups.len().min(here.len());
            k.progress == self.source_progress
                && k.places == places
                && k.conds == conds
                && k.groups.len() != here.len()
                && k.groups[..n] == here[..n]
        }) else {
            return;
        };
        let keep = paths.parked[i].key.groups.len().min(here.len());
        if paths.parked[i].key.groups.len() > here.len() {
            // The waiting path is the deeper one: it runs on from here.
            let other = paths.parked.swap_remove(i);
            let env = self.env.path(fork);
            paths.parked.push(Config::new(self.save_path(), env, false));
            self.resume_config(fork, other);
        }
        let span = self.position_span();
        while self.env.depth() > keep {
            let mut catcodes = std::mem::take(&mut self.catcodes);
            self.env.pop_group(&mut catcodes);
            self.catcodes = catcodes;
        }
        self.diagnose(
            Severity::Assumption,
            "unbalanced-path",
            span,
            "a path stood here in more groups than another; the groups are assumed closed, as a package closes what it opens".into(),
        );
    }

    /// Too many paths wait: pairs alike but for the text left in their input
    /// are joined, their input widened ([`widen_input`]), until the count
    /// is back in bounds or no pair can be joined.
    /// Paths a drive keeps apart before it joins those that stand alike:
    /// `limits.paths` between commands, `limits.choices` inside a scan,
    /// where the arms of a choice are meant to join as soon as they can.
    fn path_bound(&self) -> usize {
        match self.unit {
            Unit::Command => usize::from(self.limits.paths),
            _ => usize::from(self.limits.choices),
        }
    }

    fn crowd(&mut self, paths: &mut Paths, fork: &Fork) {
        let limit = self.path_bound();
        let span = self.position_span();
        let (mut i, before) = (0, paths.parked.len());
        while paths.parked.len() > limit && i < paths.parked.len() {
            let joined = (i + 1..paths.parked.len()).find_map(|j| {
                let (a, b) = (&paths.parked[i], &paths.parked[j]);
                if !alike(a, b) {
                    return None;
                }
                let input = self.widen_input(&a.state.input, &b.state.input, span, false)?;
                Some((j, input))
            });
            let Some((j, input)) = joined else {
                i += 1;
                continue;
            };
            let b = paths.parked.swap_remove(j);
            let a = paths.parked.swap_remove(i);
            let widen = |c: Config| Config::new(PathState { input: input.clone(), ..c.state }, c.env, c.ended);
            let (joined, _) = self.join_configs(fork, &widen(a), &widen(b), false);
            paths.parked.push(joined);
        }
        paths.crowded = paths.parked.len();
        if paths.crowded == before {
            return;
        }
        self.diagnose(
            Severity::Imprecision,
            "paths-widened",
            span,
            format!("more than {limit} paths waited; paths that differ only in the text they have left were joined"),
        );
    }

    /// The input of two paths that stand alike, joined. Level by level when
    /// they are as deep: equal levels are kept, and two token lists become
    /// their common end after one token standing for either beginning
    /// ([`Machine::one_of`]). Otherwise the shared bottom levels are kept,
    /// and what each reads above them becomes one such text. `None` when a
    /// file or a boundary differs.
    fn widen_input(&mut self, a: &[Frame], b: &[Frame], span: Span, widen: bool) -> Option<Vec<Frame>> {
        let (a, b) = (live_frames(a), live_frames(b));
        let tokens = |text: Vec<Token>| Frame::Tokens {
            held: text.len(),
            toks: Rc::from(text),
            pos: 0,
            expanding: None,
            upper: false,
        };
        if a.len() == b.len() {
            let mut input = Vec::with_capacity(a.len());
            for (x, y) in a.iter().zip(&b) {
                let frame = match (pending(&[x]), pending(&[y])) {
                    _ if place(x) == place(y) => (*x).clone(),
                    (Some(p), Some(q)) => tokens(self.widen_text(&p, &q, span, widen)?),
                    _ => return None,
                };
                input.push(frame);
            }
            return Some(input);
        }
        let shared = a.iter().zip(&b).take_while(|(x, y)| place(x) == place(y)).count();
        let (p, q) = (pending(&a[shared..])?, pending(&b[shared..])?);
        let mut input: Vec<Frame> = a[..shared].iter().map(|f| (*f).clone()).collect();
        input.push(tokens(self.widen_text(&p, &q, span, widen)?));
        Some(input)
    }

    /// Two texts left to read, joined: their common end, after one token
    /// for either of what comes before it. `None` when a beginning is not
    /// brace balanced: one token cannot stand for a `}` closing a group
    /// opened before it.
    fn widen_text(&mut self, p: &[Token], q: &[Token], span: Span, widen: bool) -> Option<Vec<Token>> {
        let mut common = p.iter().rev().zip(q.iter().rev()).take_while(|(x, y)| x.tok == y.tok).count();
        if common == p.len() && common == q.len() {
            return Some(p.to_vec());
        }
        // A shorter common end, so what differs before it is balanced: an
        // argument that grows inside its braces is then one text.
        while common > 0 && !(balanced(&p[..p.len() - common]) && balanced(&q[..q.len() - common])) {
            common -= 1;
        }
        let (head, tail) = (&p[..p.len() - common], &p[p.len() - common..]);
        let other = &q[..q.len() - common];
        if !balanced(head) || !balanced(other) {
            return None;
        }
        // Texts that differ only in their digits (a loop's counter) are
        // the text with unknown digits, which the next pass meets again as
        // it is. Others are one of either, and unknown text once widened.
        let syms = self.typed_syms();
        let typed = crate::env::Typed {
            interner: &self.out.interner,
            syms: &syms,
            digits: self.unknown_digits,
            more: self.unknown_more,
        };
        let (p, q) = (crate::env::unknown_runs(head, &typed), crate::env::unknown_runs(other, &typed));
        if same_text(&p, &q) {
            let mut text = p.to_vec();
            text.extend_from_slice(tail);
            return Some(text);
        }
        // What both begin with is kept too, when what differs after it is
        // balanced: a recursion whose passes differ in a letter at the
        // end of their input (`\__int_to_Base:nnnN`) then joins at that
        // letter, and its calls stay text a scan reads.
        let shared = head.iter().zip(other).take_while(|(x, y)| x.tok == y.tok).count();
        let shared = if balanced(&head[shared..]) && balanced(&other[shared..]) { shared } else { 0 };
        let one = match widen {
            true => self.typed_or_unknown(&[&head[shared..], &other[shared..]], span),
            false => self.one_of(vec![Rc::from(&head[shared..]), Rc::from(&other[shared..])], span),
        };
        let mut text = head[..shared].to_vec();
        text.push(one);
        text.extend_from_slice(tail);
        Some(text)
    }

    /// Park `config`, joined with a parked path at its place ([`partner`]).
    fn park(&mut self, paths: &mut Paths, fork: &Fork, mut config: Config) {
        while !config.ended
            && let Some(i) = partner(&paths.parked, &config.key, &config.tag)
        {
            let other = paths.parked.swap_remove(i);
            config = self.join_configs(fork, &other, &config, false).0;
        }
        paths.parked.push(config);
    }

    /// The parked path to run next, if the running one is ahead of it: the
    /// one furthest back, in the source and then in its input, so that
    /// paths ahead wait where the others will reach them and join.
    fn behind(&self, paths: &Paths, live: bool) -> Option<usize> {
        let i = (0..paths.parked.len()).min_by(|&i, &j| {
            let (a, b) = (&paths.parked[i].key, &paths.parked[j].key);
            a.progress.cmp(&b.progress).then_with(|| place_order(&a.places, &b.places))
        })?;
        let p = &paths.parked[i].key;
        if !live || p.progress < self.source_progress {
            return Some(i);
        }
        let here: Vec<Place> = self.input.iter().filter_map(place).collect();
        (p.progress == self.source_progress && place_order(&p.places, &here).is_lt()).then_some(i)
    }

    /// Put the counters that decide when the run gives up back as the
    /// running command found them.
    fn undo_counters(&mut self, (mark, assignments, gaps, gap_depth, errors): (usize, u64, u64, Option<usize>, u32)) {
        while self.undo.len() > mark {
            match self.undo.pop().expect("length checked") {
                Undo::Site(key, site) => {
                    self.expansion_sites.insert(key, site);
                }
                Undo::Count(i, count) => self.out.facts.diagnostics[i].count = count,
            }
        }
        (self.env.assignments, self.gaps_raised, self.gap_depth, self.tex_errors, self.repeat_cut) =
            (assignments, gaps, gap_depth, errors, None);
    }

    /// A token that stands for one of `texts`: it holds each as one of its
    /// meanings, a macro without parameters whose text it is. A path that
    /// executes it splits per text; to anything else it is unknown text.
    /// A text that is itself such a token counts as its texts. The name
    /// says which texts, so every path and pass that makes the same set
    /// makes the same token with the same meaning. Past `meaning_set`
    /// texts it is unknown.
    pub(crate) fn one_of(&mut self, texts: Vec<Rc<[Token]>>, span: Span) -> Token {
        let prefix = format!("{UNKNOWN_NAME}{ONE_OF}");
        let mut members: Vec<Rc<[Token]>> = Vec::new();
        for text in texts {
            // Unknown text holds every text already.
            if matches!(&*text, [t] if t.tok == Tok::Cs(self.unknown)) {
                return self.unknown_token(span);
            }
            let set = match &*text {
                [t] => t
                    .cs()
                    .filter(|&sym| self.name(sym).starts_with(&prefix))
                    .and_then(|sym| self.env.slot(sym)?.may.clone()),
                _ => None,
            };
            let texts = match set {
                Some(may) => may.iter().filter_map(|m| m.as_macro().map(|m| m.replacement_text.clone())).collect(),
                None => vec![text],
            };
            for text in texts {
                if !members.iter().any(|t| same_text(t, &text)) {
                    members.push(text);
                }
            }
        }
        // Texts that are each a number are the digits of any number in
        // their hull, typed with it, so a bound survives the join.
        if members.len() > 1
            && let Some(hull) = members.iter().map(|t| self.number_text(t)).try_fold(None, |acc: Option<Num>, n| {
                Some(Some(match acc {
                    Some(acc) => acc.join(&n?),
                    None => n?,
                }))
            })
            && let Some(hull) = hull
            && let [token] = self.digits_in(hull, span)[..]
        {
            return token;
        }
        if members.len() > usize::from(self.limits.meaning_set) {
            let texts: Vec<&[Token]> = members.iter().map(|t| &**t).collect();
            return self.typed_or_unknown(&texts, span);
        }
        let mut named: Vec<(u128, Rc<[Token]>)> = members.into_iter().map(|t| (text_hash(&t), t)).collect();
        named.sort_by_key(|n| n.0);
        let sym = self.intern(&format!("{prefix}{:x?}", named.iter().map(|n| n.0).collect::<Vec<_>>()));
        let may: Vec<Meaning> = named
            .into_iter()
            .map(|(_, text)| {
                Meaning::Macro(Rc::new(MacroDef {
                    parameter_text: Default::default(),
                    arg_spec: None,
                    replacement_text: text,
                    long: false,
                    outer: false,
                    protected: false,
                }))
            })
            .collect();
        let mut binding = Binding::builtin(Meaning::Unknown);
        binding.may = Some(may.into());
        self.env.constant(sym, binding);
        Token::new(Tok::Cs(sym), span)
    }

    /// Widening of a text that grows at its end (`\\aftergroup` per pass, an
    /// append): the common part, then any number of repetitions of what one
    /// side adds ([`Machine::repeat_token`]). A repetition already there
    /// absorbs one more. Any other difference is unknown text.
    pub(crate) fn repeat_join(&mut self, x: &[Token], y: &[Token], span: Span) -> Vec<Token> {
        let (short, long) = if x.len() <= y.len() { (x, y) } else { (y, x) };
        if !same_text(short, &long[..short.len()]) || !balanced(&long[short.len()..]) {
            return vec![self.unknown_token(span)];
        }
        let added = &long[short.len()..];
        let rep = self.repeat_token(added, span);
        let mut text = short.to_vec();
        // `D* D` is `D*`, and so is `D* D*`.
        if text.last().is_none_or(|t| t.tok != rep.tok) {
            text.push(rep);
        }
        text
    }

    /// A token for any number of repetitions of `text` (`text*`): one of the
    /// empty text and `text` followed by itself. Run, it parts like any
    /// [`Machine::one_of`] token, and the loop it makes meets by fixpoint.
    pub(crate) fn repeat_token(&mut self, text: &[Token], span: Span) -> Token {
        if let [t] = text
            && self.name(t.cs().unwrap_or(self.unknown)).starts_with(&format!("{UNKNOWN_NAME}{ONE_OF}*"))
        {
            return *t;
        }
        let sym = self.intern(&format!("{UNKNOWN_NAME}{ONE_OF}*{:x}", text_hash(text)));
        let token = Token::new(Tok::Cs(sym), span);
        if self.env.slot(sym).is_none() {
            let def = |replacement_text: Rc<[Token]>| {
                Meaning::Macro(Rc::new(MacroDef {
                    parameter_text: Default::default(),
                    arg_spec: None,
                    replacement_text,
                    long: false,
                    outer: false,
                    protected: false,
                }))
            };
            let mut binding = Binding::builtin(Meaning::Unknown);
            binding.may = Some(vec![def(Rc::from(Vec::new())), def(Rc::from([text, &[token]].concat()))].into());
            self.env.constant(sym, binding);
        }
        token
    }

    /// The number a text is: digits, or one token of digits typed with
    /// an interval. `None` for any other text.
    fn number_text(&self, text: &[Token]) -> Option<Num> {
        if let [t] = text
            && let Some(num) = t.cs().and_then(|sym| self.typed_digits_of(sym))
        {
            return Some(num);
        }
        if text.is_empty() || text.len() > 10 {
            return None;
        }
        let digits: Option<String> = text
            .iter()
            .map(|t| match t.tok {
                Tok::Chr(c @ '0'..='9', Catcode::Other) => Some(c),
                _ => None,
            })
            .collect();
        digits?.parse::<i64>().ok().map(Num::exact)
    }

    /// The texts a token from [`Machine::one_of`] stands for. `None` for
    /// any other token.
    pub(crate) fn one_of_members(&self, t: Token) -> Option<Vec<Rc<[Token]>>> {
        let sym = t.cs().filter(|&sym| {
            self.name(sym).starts_with(UNKNOWN_NAME) && self.name(sym)[UNKNOWN_NAME.len()..].starts_with(ONE_OF)
        })?;
        let may = self.env.slot(sym)?.may.as_ref()?;
        Some(may.iter().filter_map(|m| m.as_macro().map(|m| m.replacement_text.clone())).collect())
    }

    /// `-vvvv`: where the waiting paths stand, the most crowded places
    /// first, for seeing why they do not meet.
    fn report_paths(&self, paths: &Paths, commands: u64) {
        /// Commands between two reports.
        const EVERY: u64 = 20_000;
        /// Places a report names.
        const SHOWN: usize = 6;
        if self.cfg.verbose < 4 || paths.parked.is_empty() || !commands.is_multiple_of(EVERY) {
            return;
        }
        let mut places: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, p) in paths.parked.iter().enumerate() {
            let top: Vec<String> = p.key.places.iter().rev().take(2).map(|place| self.describe_place(place)).collect();
            let at = format!(
                "progress {} depth {} groups {} conds {} mode {:?}: {}",
                p.key.progress,
                p.key.places.len(),
                p.key.groups.len(),
                p.key.conds.len(),
                p.tag.mode,
                top.join(" <- ")
            );
            places.entry(at).or_default().push(i);
        }
        // The openers of groups some paths are in and the shallowest is not.
        let floor = paths.parked.iter().map(|p| p.key.groups.len()).min().unwrap_or(0);
        let mut openers: HashMap<String, usize> = HashMap::new();
        for p in &paths.parked {
            for at in p.env.openers().into_iter().skip(floor) {
                *openers.entry(format!("{}:{}", self.out.short_name(at.file), at.line)).or_default() += 1;
            }
        }
        let mut openers: Vec<(String, usize)> = openers.into_iter().collect();
        openers.sort_by_key(|o| std::cmp::Reverse(o.1));
        if !openers.is_empty() {
            crate::progress::note!(
                "satex: groups open on some paths only, by opener: {:?}",
                &openers[..openers.len().min(SHOWN)]
            );
        }
        let mut places: Vec<(String, Vec<usize>)> = places.into_iter().collect();
        places.sort_by_key(|p| std::cmp::Reverse(p.1.len()));
        crate::progress::note!("satex: {} paths waiting, at {} places:", paths.parked.len(), places.len());
        for (at, which) in places.into_iter().take(SHOWN) {
            // What keeps the first two apart.
            let apart = match which.as_slice() {
                [x, y, ..] => {
                    let (a, b) = (&paths.parked[*x], &paths.parked[*y]);
                    let parts = [
                        ("places", a.key.places == b.key.places),
                        ("conds", a.key.conds == b.key.conds),
                        ("groups", a.key.groups == b.key.groups),
                        ("tag", a.tag == b.tag),
                    ];
                    let differ: Vec<&str> = parts.iter().filter(|p| !p.1).map(|p| p.0).collect();
                    let conds = |c: &Config| -> Vec<String> {
                        c.key
                            .conds
                            .iter()
                            .map(|(limit, at)| format!("{limit:?}@{}:{}", self.out.short_name(at.file), at.line))
                            .collect()
                    };
                    let detail = if differ.contains(&"conds") {
                        format!(" {:?} vs {:?}", conds(a), conds(b))
                    } else {
                        String::new()
                    };
                    format!(" (apart by {differ:?}{detail})")
                }
                _ => String::new(),
            };
            crate::progress::note!("  {}x {at}{apart}", which.len());
        }
    }

    fn describe_place(&self, place: &Place) -> String {
        /// Tokens of a list a report shows.
        const TOKENS: usize = 8;
        match place {
            Place::File(file, offset, _) => format!("{}@{offset}", self.out.short_name(*file)),
            Place::Tokens(toks, pos) => {
                format!("[{}]", crate::tex::detokenize(&toks[*pos..toks.len().min(pos + TOKENS)], &self.out.interner))
            }
            Place::Returned(toks) => {
                format!("back[{}]", crate::tex::detokenize(&toks[..toks.len().min(TOKENS)], &self.out.interner))
            }
            Place::Pseudo(id, at) => format!("scantokens {id}@{at}"),
            Place::Boundary => "boundary".into(),
        }
    }

    /// Continue as `config`.
    fn resume_config(&mut self, fork: &Fork, config: Config) {
        self.restore_path(config.state);
        self.env.resume(fork, &config.env);
    }

    /// The least upper bound of two paths at one place, and what it holds
    /// more of than `a`.  The bindings are left as `fork` saw them.
    fn join_configs(&mut self, fork: &Fork, a: &Config, b: &Config, widen: bool) -> (Config, Growth) {
        let span = self.position_span();
        self.out.heat(|h| &mut h.joins, span);
        let unknown = self.unknown_token(span);
        let after: Vec<Option<Vec<Token>>> = a
            .env
            .after()
            .into_iter()
            .zip(b.env.after())
            .map(|(x, y)| {
                (!same_text(x, y)).then(|| match widen {
                    true => self.repeat_join(x, y, span),
                    false => vec![self.one_of(vec![Rc::from(x), Rc::from(y)], span)],
                })
            })
            .collect();
        let syms = self.typed_syms();
        let typed = crate::env::Typed {
            interner: &self.out.interner,
            syms: &syms,
            digits: self.unknown_digits,
            more: self.unknown_more,
        };
        self.out.joined_slots += (a.env.len() + b.env.len()) as u64;
        let (env, slots, waiting) = self.env.join_paths(fork, &a.env, &b.env, &typed, widen, &after);
        // Widened, a name the loop makes from a number (`\flag test 12`)
        // is one of a family that may be defined ([`Wild`]): its members
        // stay as defined, and one more is no growth. Without this a loop
        // that defines one more such name per pass never meets.
        let slots = match widen {
            false => slots,
            true => slots
                .into_iter()
                .filter(|&sym| {
                    let name = self.name(sym).to_string();
                    match digit_run(&name).filter(|_| !self.is_unknown_name(sym)) {
                        Some((prefix, suffix)) => {
                            let real = !matches!(self.env.meaning(sym), Meaning::Primitive(Primitive::Relax));
                            self.wild.add(prefix, suffix, real);
                            false
                        }
                        None => true,
                    }
                })
                .collect(),
        };
        let mut growth = Growth { slots, parts: Vec::new() };
        if waiting {
            growth.parts.push("aftergroup");
        }
        let mut state = a.state.clone();
        let (x, y) = (&mut state, &b.state);
        let mode = (x.mode.union(y.mode), x.list.union(y.list));
        if mode != (x.mode, x.list) {
            growth.parts.push("mode");
        }
        (x.mode, x.list) = mode;
        // What the paths depend on is kept whole: the longer list, with the
        // conditionals that pushed it, so their `\fi`s pop it in step.
        if y.cds.len() > x.cds.len() {
            (x.cds, x.conds) = (y.cds.clone(), y.conds.clone());
        }
        x.material |= y.material;
        x.approximated |= y.approximated;
        x.page_touched |= y.page_touched;
        x.natural = crate::mode::join_natural(x.natural, y.natural);
        x.wild.union(&y.wild);
        // What the scans in flight made: each is the common beginning and
        // end of both, and one token for either of what lies between.
        for (p, q) in x.scans.iter_mut().zip(&y.scans) {
            if let Some(joined) = self.join_accum(p, q, widen, span) {
                *p = joined;
                growth.parts.push("scan");
            }
        }
        // A category the paths parted about is not known (tex.web § 232),
        // nor is a line end (§ 240), a waiting token (§ 1269) or an
        // `\openin` stream (§ 480).
        let catcodes = x.catcodes.clone();
        x.catcodes.join_widen(&y.catcodes, widen);
        if x.catcodes != catcodes {
            growth.parts.push("catcodes");
        }
        let end_line = x.end_line.join(y.end_line);
        if end_line != x.end_line {
            growth.parts.push("endlinechar");
        }
        x.end_line = end_line;
        if !same_streams(&x.streams, &y.streams) {
            growth.parts.push("streams");
        }
        x.streams = join_streams(&x.streams, &y.streams);
        // A waiting `\afterassignment` token (tex.web § 1269) joins like
        // the `\aftergroup` ones: one token for either, or none.
        if x.after_assignment.map(|t| t.tok) != y.after_assignment.map(|t| t.tok) {
            let text = |t: Option<Token>| Rc::from(t.into_iter().collect::<Vec<_>>());
            let joined = match widen {
                true => unknown,
                false => self.one_of(vec![text(x.after_assignment), text(y.after_assignment)], span),
            };
            if x.after_assignment.map(|t| t.tok) != Some(joined.tok) {
                growth.parts.push("afterassignment");
            }
            x.after_assignment = Some(joined);
        }
        x.halted = false;
        (Config::new(state, env, a.ended && b.ended), growth)
    }
}

impl Machine<'_> {
    /// The join of what two paths made in one scan: `None` when they made
    /// the same text. Widened, or when what lies between is not brace
    /// balanced (one token cannot stand for a `}`), it is unknown text; an
    /// unknown token already there absorbs what follows it, so the join
    /// settles.
    fn join_accum(&mut self, a: &Accum, b: &Accum, widen: bool, span: Span) -> Option<Accum> {
        if a.name.is_some() || b.name.is_some() {
            return join_names(a, b, usize::from(self.limits.meaning_set));
        }
        if a.number.is_some() || b.number.is_some() {
            let number = join_numbers(a.number, b.number);
            return (number != a.number).then(|| Accum { number, ..a.clone() });
        }
        let (p, q) = (a.to_vec(), b.to_vec());
        if same_text(&p, &q) {
            return None;
        }
        // Two numbers of typed digits stand alike: their hull stands there.
        let typed =
            |m: &Self, t: &Token| t.cs().filter(|&s| m.typed_slice(s).is_none()).and_then(|s| m.typed_digits_of(s));
        let alike = |m: &Self, x: &Token, y: &Token| x.tok == y.tok || (typed(m, x).is_some() && typed(m, y).is_some());
        let head = p.iter().zip(&q).take_while(|(x, y)| alike(self, x, y)).count();
        let tail = p[head..].iter().rev().zip(q[head..].iter().rev()).take_while(|(x, y)| alike(self, x, y)).count();
        let (orig, mut p, q) = (p.clone(), p, q);
        let aligned: Vec<usize> = (0..head).chain(p.len() - tail..p.len()).collect();
        for i in aligned {
            let j = if i < head { i } else { q.len() - (p.len() - i) };
            // Widened, a hull that grew jumps to the end of its range.
            if p[i].tok != q[j].tok
                && let (Some(x), Some(y)) = (typed(self, &p[i]), typed(self, &q[j]))
                && let hull = if widen { x.widen(&x.join(&y)) } else { x.join(&y) }
                && hull != x
                && let [t] = self.digits_in(hull, p[i].span)[..]
            {
                p[i] = t;
            }
        }
        // Widened, unknown and typed text next to the change is part of it:
        // it absorbs the growth, so repeated growth settles in one token.
        let (mut head, mut tail) = (head, tail);
        if widen {
            let open = |m: &Self, t: &Token| {
                matches!(t.tok, Tok::Cs(s) if m.is_unknown_marker(s)) || m.typed_shape(*t).is_some()
            };
            while head > 0 && open(self, &p[head - 1]) {
                head -= 1;
            }
            while tail > 0 && open(self, &p[p.len() - tail]) && open(self, &q[q.len() - tail]) {
                tail -= 1;
            }
        }
        let (mp, mq) = (&p[head..p.len() - tail], &q[head..q.len() - tail]);
        if mp.is_empty() && mq.is_empty() {
            return (!same_text(&p, &orig)).then(|| a.with(p));
        }
        let mut text = p[..head].to_vec();
        let unknown = Tok::Cs(self.unknown);
        let last_unknown = text.last().is_some_and(|t| t.tok == unknown);
        if last_unknown {
        } else if !balanced(mp) || !balanced(mq) || (widen && mp.iter().chain(mq).any(|t| t.tok == unknown)) {
            text.push(self.unknown_token(span));
        } else if widen {
            let token = self.typed_or_unknown(&[mp, mq], span);
            text.push(token);
        } else {
            let token = self.one_of(vec![Rc::from(mp), Rc::from(mq)], span);
            text.push(token);
        }
        text.extend_from_slice(&p[p.len() - tail..]);
        (!same_text(&text, &orig)).then(|| a.with(text))
    }
}

/// The join of two [`Unit::Number`] results: one value, else the hull of
/// both, else unknown.
fn join_numbers(
    a: Option<(Option<i64>, Option<Num>)>,
    b: Option<(Option<i64>, Option<Num>)>,
) -> Option<(Option<i64>, Option<Num>)> {
    let hull = |n: Option<(Option<i64>, Option<Num>)>| n.and_then(|(v, abs)| v.map(Num::exact).or(abs));
    match (a, b) {
        (Some(x), Some(y)) if x == y => Some(x),
        _ => Some((None, hull(a).zip(hull(b)).map(|(x, y)| x.join(&y)))),
    }
}

/// The join of two names being built: `None` when they are the same.
/// Otherwise every name either built, kept apart up to `meaning_set` of
/// them (the name is one of them), or past that one name with an unknown
/// part ([`join_name_parts`]).
fn join_names(a: &Accum, b: &Accum, limit: usize) -> Option<Accum> {
    let mut n = a.name.as_deref().cloned().unwrap_or_default();
    let other = b.name.as_deref().cloned().unwrap_or_default();
    let mut all = n.names.clone();
    let mut grew = false;
    for name in other.names {
        if !all.contains(&name) {
            all.push(name);
            grew = true;
        }
    }
    if !grew && n.files == other.files && n.closed == other.closed {
        return None;
    }
    for file in other.files {
        if !n.files.contains(&file) {
            n.files.push(file);
        }
    }
    n.at = n.at.or(other.at);
    n.closed = n.closed && other.closed;
    n.stopped = n.stopped.or(other.stopped);
    n.names = all;
    if n.names.len() > limit {
        n.collapse();
    }
    Some(Accum { name: Some(Box::new(n)), ..a.with(Vec::new()) })
}

/// Two names, each `(prefix, name)` (the prefix once an unknown part was
/// met), as one: the text both begin with, an unknown part, and the text
/// both end with after it.
pub(crate) fn join_name_parts(x: (&Option<String>, &String), y: (&Option<String>, &String)) -> (String, String) {
    let (px, sx) = (x.0.as_deref().unwrap_or(x.1), x.1.as_str());
    let (py, sy) = (y.0.as_deref().unwrap_or(y.1), y.1.as_str());
    let prefix: String = px.chars().zip(py.chars()).take_while(|(c, d)| c == d).map(|(c, _)| c).collect();
    // A whole name's end is compared past the shared beginning, so the two
    // do not overlap.
    let rest = |whole: bool, s: &str| if whole { s[prefix.len().min(s.len())..].to_string() } else { s.to_string() };
    let (sx, sy) = (rest(x.0.is_none(), sx), rest(y.0.is_none(), sy));
    let mut suffix: Vec<char> =
        sx.chars().rev().zip(sy.chars().rev()).take_while(|(c, d)| c == d).map(|(c, _)| c).collect();
    suffix.reverse();
    (prefix, suffix.into_iter().collect())
}

/// Whether two parked paths stand alike but for the text they have left to
/// read before they are back in their common input: the same place in the
/// source, the same conditionals and groups.
fn alike(a: &Config, b: &Config) -> bool {
    let (x, y) = (&a.key, &b.key);
    !a.ended && !b.ended && x.progress == y.progress && x.conds == y.conds && x.groups == y.groups && x.scans == y.scans
}

/// The text input levels hold, in the order it is read: `None` when one of
/// them is a file, whose text is not a token list.
fn pending(frames: &[&Frame]) -> Option<Vec<Token>> {
    let mut text = Vec::new();
    for frame in frames.iter().rev() {
        match frame {
            Frame::Tokens { toks, pos, .. } => text.extend_from_slice(&toks[*pos..]),
            Frame::Returned(tokens) => text.extend(tokens.iter().rev().copied()),
            _ => return None,
        }
    }
    Some(text)
}

/// Which of two inputs (bottom first) is further back: at the first level
/// they part, the lower position in one file or list, or the longer text
/// left in two lists. With one a prefix of the other, the deeper one, which
/// has more to read first.
fn place_order(a: &[Place], b: &[Place]) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let left = |p: &Place| match p {
        Place::Tokens(t, i) => t.len() - i,
        Place::Returned(t) => t.len(),
        _ => 0,
    };
    for (x, y) in a.iter().zip(b) {
        let order = match (x, y) {
            _ if x == y => continue,
            (Place::File(f, i, _), Place::File(g, j, _)) if f == g => i.cmp(j),
            (Place::Pseudo(p, i), Place::Pseudo(q, j)) if p == q => i.cmp(j),
            (Place::Tokens(..) | Place::Returned(_), Place::Tokens(..) | Place::Returned(_)) => left(y).cmp(&left(x)),
            _ => Ordering::Equal,
        };
        return order;
    }
    b.len().cmp(&a.len())
}

/// The input levels that still have something to read.
fn live_frames(input: &[Frame]) -> Vec<&Frame> {
    input.iter().filter(|frame| place(frame).is_some()).collect()
}

/// Where one input level stands, `None` once it is read.
fn place(frame: &Frame) -> Option<Place> {
    match frame {
        Frame::File { mouth, .. } => Some(Place::File(mouth.file, mouth.offset(), mouth.ending())),
        Frame::Tokens { toks, pos, .. } => (*pos < toks.len()).then(|| Place::Tokens(toks.clone(), *pos)),
        Frame::Pseudo { mouth, pos, .. } => Some(Place::Pseudo(mouth.source_id(), mouth.offset() + pos)),
        Frame::Returned(tokens) => (!tokens.is_empty()).then(|| Place::Returned(tokens.clone())),
        Frame::Boundary => Some(Place::Boundary),
    }
}

/// A name for a text: 128 bits of it, which two texts that differ do not
/// share in any run satex will see.
fn text_hash(text: &[Token]) -> u128 {
    use std::hash::{Hash, Hasher};
    let half = |salt: u8| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        salt.hash(&mut hasher);
        for token in text {
            token.tok.hash(&mut hasher);
        }
        hasher.finish()
    };
    (u128::from(half(0)) << 64) | u128::from(half(1))
}

/// A name ending in a run of digits (a loop's counter, `l_test_flag12`),
/// split before them: the text before, and the empty text after.
fn digit_run(name: &str) -> Option<(String, String)> {
    // satex's own names (`\count10`'s slot, markers) are no generated family.
    if name.starts_with('\u{2}') {
        return None;
    }
    let prefix = name.trim_end_matches(|c: char| c.is_ascii_digit());
    (!prefix.is_empty() && prefix.len() < name.len()).then(|| (prefix.to_string(), String::new()))
}

/// Whether two keys stand at one place but for the digits in the text of
/// their token lists: as many levels, the same conditionals and groups, and
/// every level equal or both a token list of the same text with its runs
/// of digits collapsed.
fn alike_places(a: &Key, b: &Key, typed: &crate::env::Typed) -> bool {
    a.progress == b.progress
        && a.conds == b.conds
        && a.groups == b.groups
        && a.scans == b.scans
        && a.places.len() == b.places.len()
        && a.places.iter().zip(&b.places).all(|(x, y)| same_place(x, y, typed))
}

/// Whether `x` and `y` read the same text but for unknown digits.
fn same_place(x: &Place, y: &Place, typed: &crate::env::Typed) -> bool {
    let text = |p: &Place| match p {
        Place::Tokens(toks, pos) => Some(crate::env::unknown_runs(&toks[*pos..], typed)),
        Place::Returned(toks) => Some(crate::env::unknown_runs(toks, typed)),
        _ => None,
    };
    x == y || matches!((text(x), text(y)), (Some(p), Some(q)) if same_text(&p, &q))
}

/// Whether `key` moved on past a frame below the head at `head`: at the
/// first level they differ it reads the same file or token list further
/// on. Only reading on is final. A frame popped and pushed anew (a tail
/// call, `\unspace`'s loop) may bring the head back, so it stays.
fn left(head: &Key, key: &Key, typed: &crate::env::Typed) -> bool {
    let below = &head.places[..head.places.len().saturating_sub(1)];
    let Some((x, y)) = below.iter().zip(&key.places).find(|(x, y)| !same_place(x, y, typed)) else {
        return false;
    };
    match (x, y) {
        (Place::File(f, i, _), Place::File(g, j, _)) => f == g && j > i,
        (Place::Tokens(a, i), Place::Tokens(b, j)) => Rc::ptr_eq(a, b) && j > i,
        (Place::Pseudo(a, i), Place::Pseudo(b, j)) => a == b && j > i,
        _ => false,
    }
}

/// Whether every `{` in `text` is closed in it and every `}` opened.
pub(crate) fn balanced(text: &[Token]) -> bool {
    let mut depth = 0i64;
    for t in text {
        match t.tok {
            Tok::Chr(_, Catcode::Begin) => depth += 1,
            Tok::Chr(_, Catcode::End) => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return false;
        }
    }
    depth == 0
}

/// Whether two token lists are the same text.
fn same_text(a: &[Token], b: &[Token]) -> bool {
    a.iter().map(|t| t.tok).eq(b.iter().map(|t| t.tok))
}

/// The prefix of the names [`Machine::one_of`] makes.
const ONE_OF: &str = "one of:";

/// The parked path a path at `key` with `tag` joins: one with the same tag,
/// or, past [`PARTITIONS`] tags at the place or between two lines of a file
/// (the statement is over), the first there.
fn partner(parked: &[Config], key: &Key, tag: &Tag) -> Option<usize> {
    let here: Vec<usize> = (0..parked.len()).filter(|&i| !parked[i].ended && parked[i].key == *key).collect();
    let merge = here.len() >= PARTITIONS || key.places.iter().all(|p| matches!(p, Place::File(..) | Place::Pseudo(..)));
    here.iter().copied().find(|&i| parked[i].tag == *tag).or_else(|| here.first().copied().filter(|_| merge))
}

/// The arms the next run of a command takes: the last choice with an arm
/// left moves on to it, and the choices after it start over. With `absent`,
/// the last choice's arm did not exist, and none after it does.
fn next_script(choices: &[Choice], absent: bool) -> Option<Vec<usize>> {
    let mut arms: Vec<(usize, usize)> = choices.iter().map(|c| (c.arm, c.arms)).collect();
    if absent {
        arms.pop();
    }
    while let Some((arm, count)) = arms.pop() {
        if arm + 1 < count {
            let mut script: Vec<usize> = arms.iter().map(|a| a.0).collect();
            script.push(arm + 1);
            return Some(script);
        }
    }
    None
}
